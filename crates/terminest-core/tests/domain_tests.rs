//! Tests for domain modeling and lifecycle state transitions.

#[cfg(test)]
mod tests {
    use terminest_core::domain::{
        Architecture, ConfigValidationError, DiskFormat, DisplaySpec, HardwareSpec, HypervisorSpec,
        HypervisorType, LifecycleError, MachineConfig, MachineId, MachineName, MachineState,
        NetworkSpec, NetworkTier, OsFamily, OsSpec, StorageSpec,
    };

    #[test]
    fn test_machine_id_generation_and_validation() {
        let id = MachineId::new();
        let s = id.to_string();
        let parsed: MachineId = s.parse().expect("valid machine id string");
        assert_eq!(id, parsed);

        assert!(MachineId::parse("not-a-uuid").is_err());
        assert!(MachineId::parse("12345678-1234-1234-1234").is_err());
    }

    #[test]
    fn test_machine_name_validation() {
        let valid = MachineName::new("ubuntu-dev-01").expect("valid name");
        assert_eq!(valid.as_str(), "ubuntu-dev-01");

        assert!(MachineName::new("").is_err(), "empty name rejected");
        assert!(
            MachineName::new("has space").is_err(),
            "space in name rejected"
        );
        assert!(
            MachineName::new("../traversal").is_err(),
            "traversal rejected"
        );
        assert!(
            MachineName::new("a".repeat(65)).is_err(),
            "name > 64 chars rejected"
        );
    }

    fn sample_config() -> MachineConfig {
        MachineConfig {
            version: "1.0.0".to_string(),
            id: MachineId::new(),
            name: MachineName::new("test-vm").unwrap(),
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
                display: DisplaySpec {
                    width: 1920,
                    height: 1080,
                    dpi: 96,
                },
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

    #[test]
    fn test_valid_config_passes_validation() {
        let config = sample_config();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_cpu_rejected() {
        let mut config = sample_config();
        config.hardware.cpus = 0;
        assert!(matches!(
            config.validate(),
            Err(ConfigValidationError::InvalidCpu(_))
        ));

        config.hardware.cpus = 128;
        assert!(matches!(
            config.validate(),
            Err(ConfigValidationError::InvalidCpu(_))
        ));
    }

    #[test]
    fn test_invalid_memory_rejected() {
        let mut config = sample_config();
        config.hardware.memory_mb = 256;
        assert!(matches!(
            config.validate(),
            Err(ConfigValidationError::InvalidMemory(_))
        ));

        config.hardware.memory_mb = 1_000_000;
        assert!(matches!(
            config.validate(),
            Err(ConfigValidationError::InvalidMemory(_))
        ));
    }

    #[test]
    fn test_storage_path_traversal_rejected() {
        let mut config = sample_config();
        config.storage.disk_path = "../escaped.vhdx".into();
        assert!(matches!(
            config.validate(),
            Err(ConfigValidationError::PathTraversal(_))
        ));
    }

    #[test]
    fn test_lifecycle_allowed_transitions() {
        assert!(MachineState::Defined
            .transition_to(MachineState::Created)
            .is_ok());
        assert!(MachineState::Created
            .transition_to(MachineState::Starting)
            .is_ok());
        assert!(MachineState::Starting
            .transition_to(MachineState::Running)
            .is_ok());
        assert!(MachineState::Running
            .transition_to(MachineState::Stopping)
            .is_ok());
        assert!(MachineState::Stopping
            .transition_to(MachineState::Stopped)
            .is_ok());
        assert!(MachineState::Stopped
            .transition_to(MachineState::Starting)
            .is_ok());

        assert!(MachineState::Running
            .transition_to(MachineState::Pausing)
            .is_ok());
        assert!(MachineState::Pausing
            .transition_to(MachineState::Paused)
            .is_ok());
        assert!(MachineState::Paused
            .transition_to(MachineState::Resuming)
            .is_ok());
        assert!(MachineState::Resuming
            .transition_to(MachineState::Running)
            .is_ok());

        assert!(MachineState::Stopped
            .transition_to(MachineState::Deleting)
            .is_ok());
        assert!(MachineState::Defined
            .transition_to(MachineState::Deleting)
            .is_ok());
        assert!(MachineState::Created
            .transition_to(MachineState::Deleting)
            .is_ok());
    }

    #[test]
    fn test_lifecycle_forbidden_transitions() {
        // Cannot start an already running machine
        assert!(matches!(
            MachineState::Running.transition_to(MachineState::Starting),
            Err(LifecycleError::InvalidTransition { .. })
        ));

        // Cannot pause a stopped machine
        assert!(matches!(
            MachineState::Stopped.transition_to(MachineState::Pausing),
            Err(LifecycleError::InvalidTransition { .. })
        ));

        // Cannot resume a stopped or running machine
        assert!(matches!(
            MachineState::Stopped.transition_to(MachineState::Resuming),
            Err(LifecycleError::InvalidTransition { .. })
        ));
        assert!(matches!(
            MachineState::Running.transition_to(MachineState::Resuming),
            Err(LifecycleError::InvalidTransition { .. })
        ));

        // Cannot delete a running machine without stopping
        assert!(matches!(
            MachineState::Running.transition_to(MachineState::Deleting),
            Err(LifecycleError::CannotDeleteWhileRunning)
        ));
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_machine_id_roundtrip(u1 in any::<u128>()) {
            let uuid = uuid::Uuid::from_u128(u1);
            let id = MachineId::from_uuid(uuid);
            let s = id.to_string();
            let parsed = MachineId::parse(&s).expect("valid parse");
            prop_assert_eq!(id, parsed);
        }

        #[test]
        fn prop_machine_name_path_traversal_always_rejected(
            prefix in "[a-zA-Z0-9_-]{0,10}",
            traversal in "(/|\\\\|\\.\\.)",
            suffix in "[a-zA-Z0-9_-]{0,10}"
        ) {
            let malicious = format!("{prefix}{traversal}{suffix}");
            prop_assert!(MachineName::new(malicious).is_err());
        }
    }
}
