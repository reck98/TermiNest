use chrono::Utc;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::time::Duration;

use terminest_core::domain::{
    Architecture, DiskFormat, DisplaySpec, HardwareSpec, HypervisorSpec, HypervisorType,
    MachineConfig, MachineId, MachineName, NetworkSpec, NetworkTier, OsFamily, OsSpec, StorageSpec,
};
use terminest_core::error::CoreError;
use terminest_core::orchestrator::Orchestrator;

use crate::cli::{
    CreateArgs, DeleteArgs, InspectArgs, PauseArgs, ResumeArgs, StartArgs, StatusArgs, StopArgs,
};
use crate::config::TermiNestPaths;
use crate::doctor::run_doctor;
use crate::formatter::{
    print_doctor_report, print_json, print_machine_record, print_machine_status,
    print_machine_summary_table,
};

pub fn handle_init(paths: &TermiNestPaths, json: bool) -> Result<(), CoreError> {
    paths.initialize().map_err(CoreError::Io)?;

    if json {
        let res = serde_json::json!({
            "status": "success",
            "message": "TermiNest initialized successfully",
            "data_directory": paths.base_dir.display().to_string(),
            "config_file": paths.config_file.display().to_string()
        });
        print_json(&res);
    } else {
        println!("TermiNest initialized successfully.");
        println!("  Data directory:   {}", paths.base_dir.display());
        println!("  Configuration:    {}", paths.config_file.display());
        println!("  Machines root:    {}", paths.machines_dir.display());
    }

    Ok(())
}

pub fn handle_doctor(paths: &TermiNestPaths, json: bool) -> Result<(), CoreError> {
    let report = run_doctor(paths);

    if json {
        print_json(&report);
    } else {
        print_doctor_report(&report);
    }

    Ok(())
}

pub async fn handle_create(
    orchestrator: &Orchestrator,
    args: CreateArgs,
    json: bool,
) -> Result<(), CoreError> {
    let config = if let Some(path) = args.file {
        let content = fs::read_to_string(&path).map_err(|e| {
            CoreError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "Failed to read configuration file '{}': {e}",
                    path.display()
                ),
            ))
        })?;
        let parsed: MachineConfig =
            serde_json::from_str(&content).map_err(CoreError::Serialization)?;
        parsed
    } else {
        let name_str = args.name.ok_or_else(|| {
            CoreError::MachineNotFound(
                "Either --file <PATH> or --name <NAME> must be specified".into(),
            )
        })?;
        let name = MachineName::new(name_str)?;

        let (os_family, os_distro, os_version) = if args.os.starts_with("win") {
            (OsFamily::Windows, "windows".to_string(), "11".to_string())
        } else if args.os.starts_with("debian") {
            (OsFamily::Linux, "debian".to_string(), "12".to_string())
        } else {
            (OsFamily::Linux, "ubuntu".to_string(), "24.04".to_string())
        };

        let tier = match args.network.to_lowercase().as_str() {
            "isolated" => NetworkTier::Isolated,
            "bridged" => NetworkTier::Bridged,
            _ => NetworkTier::Nat,
        };

        let backend = match args.backend.to_lowercase().as_str() {
            "hyperv" => HypervisorType::HyperV,
            "qemu-whp" => HypervisorType::QemuWhp,
            "qemu-kvm" => HypervisorType::QemuKvm,
            _ => HypervisorType::Mock,
        };

        let format = if os_family == OsFamily::Windows {
            DiskFormat::Vhdx
        } else {
            DiskFormat::Qcow2
        };

        MachineConfig {
            version: "1.0.0".to_string(),
            id: MachineId::new(),
            name,
            os: OsSpec {
                family: os_family,
                distro: os_distro,
                version: os_version,
                arch: Architecture::X86_64,
                guest_agent_version: Some("1.0.0".to_string()),
            },
            hardware: HardwareSpec {
                cpus: args.cpus,
                memory_mb: args.ram,
                nested_virtualization: false,
                display: DisplaySpec::default(),
            },
            storage: StorageSpec {
                format,
                disk_path: PathBuf::from("disk.vhdx"),
                base_image: None,
                capacity_gb: args.disk,
                ephemeral: false,
            },
            networking: NetworkSpec {
                tier,
                mac_address: None,
                port_forwarding: vec![],
            },
            hypervisor: HypervisorSpec {
                backend,
                vsock_port: 10001,
                vsock_cid: 3,
            },
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    };

    let record = orchestrator.create_machine(config).await?;

    if json {
        print_json(&record);
    } else {
        println!(
            "Machine '{}' ({}) successfully created.",
            record.config.name.as_str(),
            record.config.id
        );
        println!("  State:       {}", record.state);
        println!("  Backend:     {}", record.config.hypervisor.backend);
        println!(
            "  Resources:   {} vCPUs, {} MiB RAM, {} GiB disk",
            record.config.hardware.cpus,
            record.config.hardware.memory_mb,
            record.config.storage.capacity_gb
        );
    }

    Ok(())
}

pub async fn handle_list(orchestrator: &Orchestrator, json: bool) -> Result<(), CoreError> {
    let list = orchestrator.list_machines().await?;
    if json {
        print_json(&list);
    } else {
        print_machine_summary_table(&list);
    }
    Ok(())
}

pub async fn handle_inspect(
    orchestrator: &Orchestrator,
    args: InspectArgs,
    json: bool,
) -> Result<(), CoreError> {
    let record = orchestrator.inspect_machine(&args.machine).await?;
    if json {
        print_json(&record);
    } else {
        print_machine_record(&record);
    }
    Ok(())
}

pub async fn handle_start(
    orchestrator: &Orchestrator,
    args: StartArgs,
    json: bool,
) -> Result<(), CoreError> {
    let status = orchestrator.start_machine(&args.machine).await?;
    if json {
        print_json(&status);
    } else {
        println!("Machine '{}' successfully started.", status.name.as_str());
        print_machine_status(&status);
    }
    Ok(())
}

pub async fn handle_stop(
    orchestrator: &Orchestrator,
    args: StopArgs,
    json: bool,
) -> Result<(), CoreError> {
    let status = orchestrator
        .stop_machine(&args.machine, Duration::from_secs(args.timeout), args.force)
        .await?;
    if json {
        print_json(&status);
    } else {
        println!("Machine '{}' stopped.", status.name.as_str());
        print_machine_status(&status);
    }
    Ok(())
}

pub async fn handle_pause(
    orchestrator: &Orchestrator,
    args: PauseArgs,
    json: bool,
) -> Result<(), CoreError> {
    let status = orchestrator.pause_machine(&args.machine).await?;
    if json {
        print_json(&status);
    } else {
        println!("Machine '{}' paused.", status.name.as_str());
        print_machine_status(&status);
    }
    Ok(())
}

pub async fn handle_resume(
    orchestrator: &Orchestrator,
    args: ResumeArgs,
    json: bool,
) -> Result<(), CoreError> {
    let status = orchestrator.resume_machine(&args.machine).await?;
    if json {
        print_json(&status);
    } else {
        println!("Machine '{}' resumed.", status.name.as_str());
        print_machine_status(&status);
    }
    Ok(())
}

pub async fn handle_status(
    orchestrator: &Orchestrator,
    args: StatusArgs,
    json: bool,
) -> Result<(), CoreError> {
    let status = orchestrator.get_machine_status(&args.machine).await?;
    if json {
        print_json(&status);
    } else {
        print_machine_status(&status);
    }
    Ok(())
}

pub async fn handle_delete(
    orchestrator: &Orchestrator,
    args: DeleteArgs,
    json: bool,
) -> Result<(), CoreError> {
    if !args.confirm {
        if io::stdin().is_terminal() {
            // Interactive terminal prompt
            eprint!(
                "Are you sure you want to delete machine '{}'? [y/N]: ",
                args.machine
            );
            io::stderr().flush().map_err(CoreError::Io)?;

            let mut input = String::new();
            let bytes = io::stdin().read_line(&mut input).map_err(CoreError::Io)?;
            if bytes == 0 {
                return Err(CoreError::OperationAborted(
                    "Deletion aborted: EOF received on confirmation prompt".into(),
                ));
            }
            let trimmed = input.trim();
            if !trimmed.eq_ignore_ascii_case("y") && !trimmed.eq_ignore_ascii_case("yes") {
                return Err(CoreError::OperationAborted(format!(
                    "Deletion of machine '{}' aborted by user",
                    args.machine
                )));
            }
        } else {
            // Non-interactive execution requires explicit --confirm flag
            return Err(CoreError::OperationAborted(
                "Deletion in non-interactive mode requires the '--confirm' flag".into(),
            ));
        }
    }

    orchestrator
        .delete_machine(&args.machine, args.force)
        .await?;
    if json {
        let res = serde_json::json!({
            "status": "success",
            "message": format!("Machine '{}' successfully deleted", args.machine)
        });
        print_json(&res);
    } else {
        println!("Machine '{}' successfully deleted.", args.machine);
    }
    Ok(())
}
