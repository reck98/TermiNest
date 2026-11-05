use clap::Parser;
use std::process::ExitCode;
use std::sync::Arc;

mod cli;
mod config;
mod doctor;
mod formatter;
mod handlers;

use cli::{Cli, Commands, MachineCommands};
use config::TermiNestPaths;
use formatter::print_error;
use terminest_core::backend::MockHypervisorBackend;
use terminest_core::domain::HypervisorType;
use terminest_core::error::CoreError;
use terminest_core::orchestrator::{BackendRegistry, Orchestrator};
use terminest_core::persistence::FilesystemMachineRepository;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    if cli.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("terminest=debug,terminest_core=debug")
            .init();
    }

    let paths = TermiNestPaths::discover(cli.data_dir);

    let res = match cli.command {
        Commands::Init => handlers::handle_init(&paths, cli.json),
        Commands::Doctor => handlers::handle_doctor(&paths, cli.json),
        other => {
            // Ensure directories are initialized before machine operations
            if let Err(e) = paths.initialize() {
                print_error(
                    &format!("Failed to initialize data directory: {e}"),
                    cli.json,
                );
                return ExitCode::from(2);
            }

            let repo = match FilesystemMachineRepository::new(&paths.base_dir) {
                Ok(r) => Arc::new(r),
                Err(e) => {
                    print_error(&format!("Failed to open repository: {e}"), cli.json);
                    return ExitCode::from(2);
                }
            };

            let mut registry = BackendRegistry::new();
            let mock_state_file = paths.run_dir.join("mock_backend.json");
            let mock_backend = Arc::new(MockHypervisorBackend::with_state_file(
                terminest_core::backend::HypervisorCapabilities::default(),
                Some(mock_state_file),
            ));
            registry.register(HypervisorType::Mock, mock_backend);

            let orchestrator = Orchestrator::new(repo, Arc::new(registry));

            match other {
                Commands::Create(args) => {
                    handlers::handle_create(&orchestrator, args, cli.json).await
                }
                Commands::Ls(_) | Commands::List(_) => {
                    handlers::handle_list(&orchestrator, cli.json).await
                }
                Commands::Inspect(args) => {
                    handlers::handle_inspect(&orchestrator, args, cli.json).await
                }
                Commands::Start(args) => {
                    handlers::handle_start(&orchestrator, args, cli.json).await
                }
                Commands::Stop(args) => handlers::handle_stop(&orchestrator, args, cli.json).await,
                Commands::Pause(args) => {
                    handlers::handle_pause(&orchestrator, args, cli.json).await
                }
                Commands::Resume(args) => {
                    handlers::handle_resume(&orchestrator, args, cli.json).await
                }
                Commands::Status(args) => {
                    handlers::handle_status(&orchestrator, args, cli.json).await
                }
                Commands::Delete(args) | Commands::Destroy(args) => {
                    handlers::handle_delete(&orchestrator, args, cli.json).await
                }
                Commands::Machine { command } => match command {
                    MachineCommands::Create(args) => {
                        handlers::handle_create(&orchestrator, args, cli.json).await
                    }
                    MachineCommands::List(_) => {
                        handlers::handle_list(&orchestrator, cli.json).await
                    }
                    MachineCommands::Inspect(args) => {
                        handlers::handle_inspect(&orchestrator, args, cli.json).await
                    }
                    MachineCommands::Start(args) => {
                        handlers::handle_start(&orchestrator, args, cli.json).await
                    }
                    MachineCommands::Stop(args) => {
                        handlers::handle_stop(&orchestrator, args, cli.json).await
                    }
                    MachineCommands::Pause(args) => {
                        handlers::handle_pause(&orchestrator, args, cli.json).await
                    }
                    MachineCommands::Resume(args) => {
                        handlers::handle_resume(&orchestrator, args, cli.json).await
                    }
                    MachineCommands::Status(args) => {
                        handlers::handle_status(&orchestrator, args, cli.json).await
                    }
                    MachineCommands::Delete(args) => {
                        handlers::handle_delete(&orchestrator, args, cli.json).await
                    }
                },
                _ => unreachable!(),
            }
        }
    };

    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            print_error(&err.to_string(), cli.json);
            match err {
                CoreError::Validation(_)
                | CoreError::Lifecycle(_)
                | CoreError::MachineNotFound(_)
                | CoreError::MachineAlreadyExists(_)
                | CoreError::ConcurrencyConflict(_, _)
                | CoreError::Serialization(_) => ExitCode::from(1),

                CoreError::Backend(_) | CoreError::Persistence(_) | CoreError::Io(_) => {
                    ExitCode::from(2)
                }
            }
        }
    }
}
