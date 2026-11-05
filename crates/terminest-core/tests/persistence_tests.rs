//! Tests for MachineRepository and FilesystemMachineRepository persistence.

#[cfg(test)]
mod tests {
    use std::fs;
    use tempfile::tempdir;
    use terminest_core::domain::{
        Architecture, DiskFormat, DisplaySpec, HardwareSpec, HypervisorSpec, HypervisorType,
        MachineConfig, MachineId, MachineName, MachineState, NetworkSpec, NetworkTier, OsFamily,
        OsSpec, StorageSpec,
    };
    use terminest_core::error::PersistenceError;
    use terminest_core::persistence::{FilesystemMachineRepository, MachineRecord, MachineRepository};

    fn sample_record(name: &str) -> MachineRecord {
        let config = MachineConfig {
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
        };

        MachineRecord {
            config,
            state: MachineState::Created,
        }
    }

    #[tokio::test]
    async fn test_persistence_save_load_and_list() {
        let temp = tempdir().expect("tempdir");
        let repo = FilesystemMachineRepository::new(temp.path()).expect("init repo");

        let rec1 = sample_record("vm-alpha");
        let rec2 = sample_record("vm-beta");
        let id1 = rec1.config.id;
        let id2 = rec2.config.id;

        // Save records
        repo.save(&rec1).await.expect("save rec1");
        repo.save(&rec2).await.expect("save rec2");

        // Retrieve by ID
        let loaded1 = repo.get_by_id(&id1).await.expect("load ok").expect("found");
        assert_eq!(loaded1.config.id, id1);
        assert_eq!(loaded1.config.name.as_str(), "vm-alpha");
        assert_eq!(loaded1.state, MachineState::Created);

        // Retrieve by Name
        let name_alpha = MachineName::new("vm-alpha").unwrap();
        let loaded_name = repo
            .get_by_name(&name_alpha)
            .await
            .expect("load ok")
            .expect("found");
        assert_eq!(loaded_name.config.id, id1);

        // List records
        let list = repo.list().await.expect("list ok");
        assert_eq!(list.len(), 2);

        // Update state
        let updated = repo
            .update_state(&id1, MachineState::Running)
            .await
            .expect("update state ok");
        assert_eq!(updated.state, MachineState::Running);

        // Verify state is persisted
        let reloaded1 = repo.get_by_id(&id1).await.expect("load ok").expect("found");
        assert_eq!(reloaded1.state, MachineState::Running);

        // Delete rec2
        repo.delete(&id2).await.expect("delete ok");
        assert!(repo.get_by_id(&id2).await.expect("load ok").is_none());

        let list_after = repo.list().await.expect("list ok");
        assert_eq!(list_after.len(), 1);
        assert_eq!(list_after[0].config.id, id1);
    }

    #[tokio::test]
    async fn test_persistence_duplicate_name_collision() {
        let temp = tempdir().expect("tempdir");
        let repo = FilesystemMachineRepository::new(temp.path()).expect("init repo");

        let rec1 = sample_record("collision-test");
        let mut rec2 = sample_record("collision-test");
        rec2.config.id = MachineId::new(); // different ID, same name

        repo.save(&rec1).await.expect("save rec1 ok");
        let err = repo.save(&rec2).await.unwrap_err();
        assert!(matches!(err, PersistenceError::NameCollision(_)));
    }

    #[tokio::test]
    async fn test_persistence_corrupt_metadata_handling() {
        let temp = tempdir().expect("tempdir");
        let repo = FilesystemMachineRepository::new(temp.path()).expect("init repo");

        let rec = sample_record("good-vm");
        repo.save(&rec).await.expect("save ok");

        // Manually write a corrupt machine directory
        let corrupt_dir = temp.path().join("machines").join("bad-uuid-dir");
        fs::create_dir_all(&corrupt_dir).expect("create bad dir");
        fs::write(corrupt_dir.join("machine.json"), b"{ invalid json ...").expect("write bad file");

        // list should still succeed and return the valid record, safely skipping or isolating corrupt entry
        let list = repo.list().await.expect("list ok despite corruption");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].config.name.as_str(), "good-vm");
    }

    #[tokio::test]
    async fn test_persistence_survives_restart() {
        let temp = tempdir().expect("tempdir");
        let id;
        {
            let repo = FilesystemMachineRepository::new(temp.path()).expect("init repo");
            let rec = sample_record("retained-vm");
            id = rec.config.id;
            repo.save(&rec).await.expect("save ok");
        }

        // Simulate application restart by creating a new repository instance pointing to the same directory
        let new_repo = FilesystemMachineRepository::new(temp.path()).expect("init repo 2");
        let loaded = new_repo.get_by_id(&id).await.expect("load ok").expect("found");
        assert_eq!(loaded.config.name.as_str(), "retained-vm");
        assert_eq!(loaded.state, MachineState::Created);
    }
}
