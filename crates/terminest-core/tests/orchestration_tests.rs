//! Integration tests for the Orchestrator service.

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::tempdir;
    use terminest_core::backend::{MockFailure, MockHypervisorBackend};
    use terminest_core::domain::{
        Architecture, DiskFormat, DisplaySpec, HardwareSpec, HypervisorSpec, HypervisorType,
        MachineConfig, MachineId, MachineName, MachineState, NetworkSpec, NetworkTier, OsFamily,
        OsSpec, StorageSpec,
    };
    use terminest_core::error::{CoreError, LifecycleError};
    use terminest_core::orchestrator::{BackendRegistry, Orchestrator};
    use terminest_core::persistence::{FilesystemMachineRepository, MachineRepository};

    fn sample_config(name: &str) -> MachineConfig {
        MachineConfig {
            version: "1.0.0".to_string(),
            id: MachineId::new(),
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

    fn setup_orchestrator() -> (Orchestrator, Arc<MockHypervisorBackend>, tempfile::TempDir) {
        let temp = tempdir().expect("tempdir");
        let repo = Arc::new(FilesystemMachineRepository::new(temp.path()).expect("repo"));
        let mock_backend = Arc::new(MockHypervisorBackend::new());

        let mut registry = BackendRegistry::new();
        registry.register(HypervisorType::Mock, mock_backend.clone());

        let orchestrator = Orchestrator::new(repo, Arc::new(registry));
        (orchestrator, mock_backend, temp)
    }

    #[tokio::test]
    async fn test_orchestrator_complete_lifecycle() {
        let (orch, _, _temp) = setup_orchestrator();
        let config = sample_config("orch-test");
        let name_str = config.name.to_string();

        // 1. Create machine
        let created = orch.create_machine(config).await.expect("create ok");
        assert_eq!(created.state, MachineState::Created);

        // 2. Inspect machine
        let inspected = orch.inspect_machine(&name_str).await.expect("inspect ok");
        assert_eq!(inspected.config.name.as_str(), "orch-test");

        // 3. List machines
        let list = orch.list_machines().await.expect("list ok");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name.as_str(), "orch-test");
        assert_eq!(list[0].state, MachineState::Created);

        // 4. Start machine
        let start_status = orch.start_machine(&name_str).await.expect("start ok");
        assert_eq!(start_status.state, MachineState::Running);
        assert!(start_status.pid.is_some());

        // 5. Query status
        let status = orch.get_machine_status(&name_str).await.expect("status ok");
        assert_eq!(status.state, MachineState::Running);

        // 6. Pause machine
        let paused = orch.pause_machine(&name_str).await.expect("pause ok");
        assert_eq!(paused.state, MachineState::Paused);

        // 7. Resume machine
        let resumed = orch.resume_machine(&name_str).await.expect("resume ok");
        assert_eq!(resumed.state, MachineState::Running);

        // 8. Stop machine
        let stopped = orch
            .stop_machine(&name_str, Duration::from_secs(5), false)
            .await
            .expect("stop ok");
        assert_eq!(stopped.state, MachineState::Stopped);

        // 9. Delete machine
        orch.delete_machine(&name_str, false)
            .await
            .expect("delete ok");
        let list_after = orch.list_machines().await.expect("list ok");
        assert!(list_after.is_empty());
    }

    #[tokio::test]
    async fn test_orchestrator_rejects_deleting_running_without_force() {
        let (orch, _, _temp) = setup_orchestrator();
        let config = sample_config("delete-guard-test");
        let name_str = config.name.to_string();

        orch.create_machine(config).await.expect("created");
        orch.start_machine(&name_str).await.expect("started");

        // Attempting to delete running machine without force must fail
        let err = orch.delete_machine(&name_str, false).await.unwrap_err();
        assert!(matches!(
            err,
            CoreError::Lifecycle(LifecycleError::CannotDeleteWhileRunning)
        ));

        // With force = true, delete stops and removes the machine
        orch.delete_machine(&name_str, true)
            .await
            .expect("force delete succeeds");
        assert!(orch.list_machines().await.expect("list ok").is_empty());
    }

    #[tokio::test]
    async fn test_orchestrator_backend_failure_compensation() {
        let (orch, backend, _temp) = setup_orchestrator();
        let config = sample_config("failure-comp-test");
        let name_str = config.name.to_string();

        // Inject backend creation failure
        backend.inject_failure(MockFailure::FailCreate("hypervisor out of memory".into()));
        let err = orch.create_machine(config.clone()).await.unwrap_err();
        assert!(matches!(err, CoreError::Backend(_)));

        // Ensure persistence was not left with orphaned state
        assert!(orch.list_machines().await.expect("list").is_empty());

        // Now clear failure and create cleanly
        backend.clear_failures();
        orch.create_machine(config).await.expect("created");

        // Inject start failure
        backend.inject_failure(MockFailure::FailStart("vCPU allocation error".into()));
        let err = orch.start_machine(&name_str).await.unwrap_err();
        assert!(matches!(err, CoreError::Backend(_)));

        // Status should reflect Failed state rather than remaining stuck in Starting
        let status = orch.get_machine_status(&name_str).await.expect("status");
        assert_eq!(status.state, MachineState::Failed);
    }

    #[tokio::test]
    async fn test_orchestrator_reconciles_stale_running_state_on_restart() {
        let temp = tempdir().expect("tempdir");
        let repo = Arc::new(FilesystemMachineRepository::new(temp.path()).expect("repo"));
        let mock_backend = Arc::new(MockHypervisorBackend::new());

        let mut registry = BackendRegistry::new();
        registry.register(HypervisorType::Mock, mock_backend.clone());

        let orch1 = Orchestrator::new(repo.clone(), Arc::new(registry));
        let config = sample_config("reconcile-test");
        let id = config.id;
        let name_str = config.name.to_string();

        orch1.create_machine(config).await.expect("created");
        orch1.start_machine(&name_str).await.expect("started");

        // Now simulate TermiNest process termination and restart with a fresh Mock backend
        // (where the mock backend has no knowledge of the previously running machine)
        let fresh_mock_backend = Arc::new(MockHypervisorBackend::new());
        let mut fresh_registry = BackendRegistry::new();
        fresh_registry.register(HypervisorType::Mock, fresh_mock_backend.clone());

        let orch2 = Orchestrator::new(repo.clone(), Arc::new(fresh_registry));

        // Persisted state was "running", but query status reconciles with fresh backend
        let status = orch2
            .get_machine_status(&name_str)
            .await
            .expect("status ok");
        // Reconciled state should not be falsely reported as Running!
        assert_ne!(status.state, MachineState::Running);
        assert!(matches!(
            status.state,
            MachineState::Unknown | MachineState::Stopped
        ));

        // Reconciled state is saved to repository
        let record = repo.get_by_id(&id).await.expect("load").expect("record");
        assert_ne!(record.state, MachineState::Running);
    }

    #[tokio::test]
    async fn test_orchestrator_concurrent_lifecycle_protection() {
        let (orch, _, _temp) = setup_orchestrator();
        let config = sample_config("concurrent-test");
        let name_str = config.name.to_string();

        orch.create_machine(config).await.expect("created");
        orch.start_machine(&name_str).await.expect("started");

        // Spawn two concurrent stop calls
        let orch_arc = Arc::new(orch);
        let name_clone1 = name_str.clone();
        let name_clone2 = name_str.clone();
        let orch1 = orch_arc.clone();
        let orch2 = orch_arc.clone();

        let handle1 = tokio::spawn(async move {
            orch1
                .stop_machine(&name_clone1, Duration::from_secs(2), false)
                .await
        });
        let handle2 = tokio::spawn(async move {
            orch2
                .stop_machine(&name_clone2, Duration::from_secs(2), false)
                .await
        });

        let (res1, res2) = tokio::join!(handle1, handle2);
        let r1 = res1.unwrap();
        let r2 = res2.unwrap();

        // Exactly one should succeed, and the other should either succeed or fail with invalid transition,
        // but no panic or data race occurs!
        assert!(r1.is_ok() || r2.is_ok());

        let final_status = orch_arc
            .get_machine_status(&name_str)
            .await
            .expect("status");
        assert_eq!(final_status.state, MachineState::Stopped);
    }

    #[tokio::test]
    async fn test_orchestrator_rejects_deleting_transitional_states_without_force() {
        let (orch, _, _temp) = setup_orchestrator();

        // 1. Test Stopped -> Deleting (allowed without force)
        let cfg1 = sample_config("stop-del-test");
        let name1 = cfg1.name.to_string();
        orch.create_machine(cfg1).await.expect("created");
        orch.start_machine(&name1).await.expect("started");
        orch.stop_machine(&name1, Duration::from_secs(1), false)
            .await
            .expect("stopped");
        orch.delete_machine(&name1, false)
            .await
            .expect("delete stopped machine succeeds");

        // 2. Test Paused -> Deleting (rejected without force)
        let cfg2 = sample_config("pause-del-test");
        let name2 = cfg2.name.to_string();
        orch.create_machine(cfg2).await.expect("created");
        orch.start_machine(&name2).await.expect("started");
        orch.pause_machine(&name2).await.expect("paused");

        let err = orch.delete_machine(&name2, false).await.unwrap_err();
        assert!(matches!(
            err,
            CoreError::Lifecycle(LifecycleError::InvalidTransition { .. })
        ));

        // Paused with force = true must succeed
        orch.delete_machine(&name2, true)
            .await
            .expect("force delete paused machine succeeds");
    }

    #[tokio::test]
    async fn test_orchestrator_deletion_all_eligibility_states() {
        let (orch, _, _temp) = setup_orchestrator();

        // Created state -> deletable without force
        let cfg = sample_config("created-del-test");
        let name = cfg.name.to_string();
        orch.create_machine(cfg).await.expect("created");
        orch.delete_machine(&name, false)
            .await
            .expect("delete created succeeds");

        // Running state -> rejected without force
        let cfg = sample_config("running-del-test");
        let name = cfg.name.to_string();
        orch.create_machine(cfg).await.expect("created");
        orch.start_machine(&name).await.expect("started");
        let err = orch.delete_machine(&name, false).await.unwrap_err();
        assert!(matches!(
            err,
            CoreError::Lifecycle(LifecycleError::CannotDeleteWhileRunning)
        ));
        orch.delete_machine(&name, true)
            .await
            .expect("force delete succeeds");
    }

    #[tokio::test]
    async fn test_orchestrator_audit_trail_recorded_across_lifecycle() {
        let temp = tempdir().expect("tempdir");
        let repo = Arc::new(FilesystemMachineRepository::new(temp.path()).expect("repo"));
        let mock_backend = Arc::new(MockHypervisorBackend::new());

        let mut registry = BackendRegistry::new();
        registry.register(HypervisorType::Mock, mock_backend.clone());

        let orch = Orchestrator::new(repo.clone(), Arc::new(registry));
        let config = sample_config("audit-trail-test");
        let id = config.id;
        let name_str = config.name.to_string();

        // 1. Create machine -> audit event 'create'
        orch.create_machine(config).await.expect("created");

        // 2. Start machine -> audit event 'start'
        orch.start_machine(&name_str).await.expect("started");

        // 3. Pause machine -> audit event 'pause'
        orch.pause_machine(&name_str).await.expect("paused");

        // 4. Resume machine -> audit event 'resume'
        orch.resume_machine(&name_str).await.expect("resumed");

        // 5. Stop machine -> audit event 'stop'
        orch.stop_machine(&name_str, Duration::from_secs(1), false)
            .await
            .expect("stopped");

        // Read audit records from disk before deletion
        let records = repo.read_audit_events(&id).await.expect("read audit");
        assert_eq!(records.len(), 5);
        assert_eq!(
            records[0].action,
            terminest_core::domain::AuditAction::Create
        );
        assert_eq!(
            records[1].action,
            terminest_core::domain::AuditAction::Start
        );
        assert_eq!(
            records[2].action,
            terminest_core::domain::AuditAction::Pause
        );
        assert_eq!(
            records[3].action,
            terminest_core::domain::AuditAction::Resume
        );
        assert_eq!(records[4].action, terminest_core::domain::AuditAction::Stop);

        for r in &records {
            assert_eq!(r.schema_version, "1.0.0");
            assert_eq!(r.result, terminest_core::domain::AuditResult::Success);
            assert!(r.error_message.is_none());
        }

        // 6. Delete machine -> audit event 'delete' is appended before folder deletion
        orch.delete_machine(&name_str, false)
            .await
            .expect("deleted");
    }
}
