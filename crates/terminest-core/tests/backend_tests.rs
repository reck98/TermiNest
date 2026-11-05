//! Tests for HypervisorBackend abstraction and MockHypervisorBackend implementation.

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use terminest_core::backend::{
        HypervisorBackend, HypervisorCapabilities, MockFailure, MockHypervisorBackend,
    };
    use terminest_core::domain::{
        Architecture, DiskFormat, DisplaySpec, HardwareSpec, HypervisorSpec, HypervisorType,
        MachineConfig, MachineId, MachineName, MachineState, NetworkSpec, NetworkTier, OsFamily,
        OsSpec, StorageSpec,
    };
    use terminest_core::error::HypervisorError;

    fn sample_config(id: MachineId, name: &str) -> MachineConfig {
        MachineConfig {
            version: "1.0.0".to_string(),
            id,
            name: MachineName::new(name).unwrap(),
            os: OsSpec {
                family: OsFamily::Linux,
                distro: "ubuntu".to_string(),
                version: "24.04".to_string(),
                arch: Architecture::X86_64,
                guest_agent_version: Some("1.0.0".to_string()),
            },
            hardware: HardwareSpec {
                cpus: 2,
                memory_mb: 4096,
                nested_virtualization: false,
                display: DisplaySpec::default(),
            },
            storage: StorageSpec {
                format: DiskFormat::Vhdx,
                disk_path: "disk.vhdx".into(),
                base_image: None,
                capacity_gb: 32,
                ephemeral: false,
            },
            networking: NetworkSpec {
                tier: NetworkTier::Nat,
                mac_address: None,
                port_forwarding: vec![],
            },
            hypervisor: HypervisorSpec {
                backend: HypervisorType::Mock,
                vsock_port: 10001,
                vsock_cid: 3,
            },
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_mock_backend_complete_lifecycle() {
        let backend = MockHypervisorBackend::new();
        let id = MachineId::new();
        let config = sample_config(id, "test-vm");

        // 1. Initial status query fails (not found)
        assert!(matches!(
            backend.get_machine_status(&id).await,
            Err(HypervisorError::MachineNotFound(_))
        ));

        // 2. Create machine
        backend
            .create_machine(&config)
            .await
            .expect("create succeeds");
        let status = backend.get_machine_status(&id).await.expect("status ok");
        assert_eq!(status.state, MachineState::Created);
        assert_eq!(status.pid, None);

        // 3. Duplicate creation fails
        assert!(matches!(
            backend.create_machine(&config).await,
            Err(HypervisorError::MachineAlreadyExists(_))
        ));

        // 4. Start machine
        let start_status = backend.start_machine(&id).await.expect("start succeeds");
        assert_eq!(start_status.state, MachineState::Running);
        assert!(start_status.pid.is_some());

        // 5. Query status while running
        let running_status = backend.get_machine_status(&id).await.expect("status ok");
        assert_eq!(running_status.state, MachineState::Running);

        // 6. Pause machine
        backend.pause_machine(&id).await.expect("pause succeeds");
        let paused_status = backend.get_machine_status(&id).await.expect("status ok");
        assert_eq!(paused_status.state, MachineState::Paused);

        // 7. Resume machine
        backend.resume_machine(&id).await.expect("resume succeeds");
        let resumed_status = backend.get_machine_status(&id).await.expect("status ok");
        assert_eq!(resumed_status.state, MachineState::Running);

        // 8. Stop machine
        backend
            .stop_machine(&id, Duration::from_secs(5), false)
            .await
            .expect("stop succeeds");
        let stopped_status = backend.get_machine_status(&id).await.expect("status ok");
        assert_eq!(stopped_status.state, MachineState::Stopped);
        assert_eq!(stopped_status.pid, None);

        // 9. Delete machine
        backend.delete_machine(&id).await.expect("delete succeeds");
        assert!(matches!(
            backend.get_machine_status(&id).await,
            Err(HypervisorError::MachineNotFound(_))
        ));
    }

    #[tokio::test]
    async fn test_mock_backend_unsupported_pause_capability() {
        let caps = HypervisorCapabilities {
            hardware_acceleration: false,
            live_snapshots: false,
            disk_differencing: true,
            vsock_transport: true,
            pause_resume: false, // explicitly disabled
        };
        let backend = MockHypervisorBackend::with_capabilities(caps);
        let id = MachineId::new();
        let config = sample_config(id, "no-pause-vm");

        backend.create_machine(&config).await.expect("created");
        backend.start_machine(&id).await.expect("started");

        // Attempt pause -> returns UnsupportedOperation
        let err = backend.pause_machine(&id).await.unwrap_err();
        assert!(matches!(
            err,
            HypervisorError::UnsupportedOperation("pause", "mock")
        ));
    }

    #[tokio::test]
    async fn test_mock_backend_deterministic_failure_injection() {
        let backend = MockHypervisorBackend::new();
        let id = MachineId::new();
        let config = sample_config(id, "failure-vm");

        // Inject create failure
        backend.inject_failure(MockFailure::FailCreate(
            "disk image allocation failed".into(),
        ));
        let err = backend.create_machine(&config).await.unwrap_err();
        assert_eq!(
            err,
            HypervisorError::ExecutionFailed("disk image allocation failed".into())
        );

        // Clear failures and create
        backend.clear_failures();
        backend.create_machine(&config).await.expect("created");

        // Inject start failure
        backend.inject_failure(MockFailure::FailStart("vCPU scheduling error".into()));
        let err = backend.start_machine(&id).await.unwrap_err();
        assert_eq!(
            err,
            HypervisorError::ExecutionFailed("vCPU scheduling error".into())
        );

        // Clear failure and start
        backend.clear_failures();
        backend.start_machine(&id).await.expect("started");

        // Inject timeout
        backend.inject_failure(MockFailure::Timeout(Duration::from_millis(500)));
        let err = backend
            .stop_machine(&id, Duration::from_secs(1), false)
            .await
            .unwrap_err();
        assert_eq!(err, HypervisorError::Timeout(Duration::from_millis(500)));

        // Clear and stop cleanly
        backend.clear_failures();
        backend
            .stop_machine(&id, Duration::from_secs(1), false)
            .await
            .expect("stopped");
    }

    #[tokio::test]
    async fn test_mock_backend_concurrent_state_file_synchronization() {
        use tempfile::tempdir;
        let temp = tempdir().expect("tempdir");
        let state_file = temp.path().join("mock_backend.json");

        // Two independent backend instances sharing the same state file (simulating separate processes)
        let backend1 = MockHypervisorBackend::with_state_file(
            HypervisorCapabilities::default(),
            Some(state_file.clone()),
        );
        let backend2 = MockHypervisorBackend::with_state_file(
            HypervisorCapabilities::default(),
            Some(state_file.clone()),
        );

        let id1 = MachineId::new();
        let id2 = MachineId::new();
        let config1 = sample_config(id1, "vm-1");
        let config2 = sample_config(id2, "vm-2");

        // Process 1 creates vm-1
        backend1
            .create_machine(&config1)
            .await
            .expect("vm-1 created");

        // Process 2 creates vm-2
        backend2
            .create_machine(&config2)
            .await
            .expect("vm-2 created");

        // Process 1 starts vm-1
        backend1.start_machine(&id1).await.expect("vm-1 started");

        // Process 2 starts vm-2
        backend2.start_machine(&id2).await.expect("vm-2 started");

        // A third fresh process reads the state file from disk
        let backend3 = MockHypervisorBackend::with_state_file(
            HypervisorCapabilities::default(),
            Some(state_file),
        );

        // BOTH machines must exist and be Running!
        let status1 = backend3
            .get_machine_status(&id1)
            .await
            .expect("vm-1 status");
        let status2 = backend3
            .get_machine_status(&id2)
            .await
            .expect("vm-2 status");

        assert_eq!(status1.state, MachineState::Running);
        assert_eq!(status2.state, MachineState::Running);
        assert!(status1.pid.is_some());
        assert!(status2.pid.is_some());
    }
}
