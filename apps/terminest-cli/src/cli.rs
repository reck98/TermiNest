use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "terminest",
    version,
    about = "TermiNest Cross-Platform Virtual Computer Infrastructure for AI Agents",
    long_about = "TermiNest exposes isolated virtual machines to CLI-based AI agents through standardized computer-use operations without manipulating the host desktop."
)]
pub struct Cli {
    #[arg(long, global = true, help = "Override base data directory")]
    pub data_dir: Option<PathBuf>,

    #[arg(long, global = true, help = "Output all responses in JSON format")]
    pub json: bool,

    #[arg(short, long, global = true, help = "Enable verbose debug logging")]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(about = "Initialize local TermiNest environment and directory structures")]
    Init,

    #[command(about = "Verify host virtualization capabilities and environment health")]
    Doctor,

    #[command(about = "Manage virtual machines (create, start, stop, pause, inspect, delete)")]
    Machine {
        #[command(subcommand)]
        command: MachineCommands,
    },

    // Direct top-level aliases conforming to Report 08 and operational ergonomics
    #[command(about = "Provision a new virtual machine")]
    Create(CreateArgs),

    #[command(about = "List registered virtual machines")]
    Ls(ListArgs),

    #[command(about = "List registered virtual machines")]
    List(ListArgs),

    #[command(about = "Inspect complete machine definition")]
    Inspect(InspectArgs),

    #[command(about = "Boot a configured virtual machine")]
    Start(StartArgs),

    #[command(about = "Stop a running virtual machine")]
    Stop(StopArgs),

    #[command(about = "Pause vCPU execution of a running virtual machine")]
    Pause(PauseArgs),

    #[command(about = "Resume vCPU execution of a paused virtual machine")]
    Resume(ResumeArgs),

    #[command(about = "Query live reconciled status of a virtual machine")]
    Status(StatusArgs),

    #[command(about = "Delete a virtual machine registration and its metadata")]
    Delete(DeleteArgs),

    #[command(about = "Delete a virtual machine registration and its metadata")]
    Destroy(DeleteArgs),
}

#[derive(Subcommand, Debug)]
pub enum MachineCommands {
    #[command(about = "Create a new virtual machine from a configuration file or parameters")]
    Create(CreateArgs),

    #[command(about = "List all configured virtual machines")]
    List(ListArgs),

    #[command(about = "Inspect complete machine definition")]
    Inspect(InspectArgs),

    #[command(about = "Boot a configured virtual machine")]
    Start(StartArgs),

    #[command(about = "Stop a running virtual machine")]
    Stop(StopArgs),

    #[command(about = "Pause vCPU execution of a running virtual machine")]
    Pause(PauseArgs),

    #[command(about = "Resume vCPU execution of a paused virtual machine")]
    Resume(ResumeArgs),

    #[command(about = "Query live reconciled status of a virtual machine")]
    Status(StatusArgs),

    #[command(about = "Delete a virtual machine registration and its metadata")]
    Delete(DeleteArgs),
}

#[derive(Args, Debug, Clone)]
pub struct CreateArgs {
    #[arg(long, short, help = "Path to machine.json specification file")]
    pub file: Option<PathBuf>,

    #[arg(
        long,
        short,
        help = "Machine name (alphanumeric, hyphens, underscores, max 64 chars)"
    )]
    pub name: Option<String>,

    #[arg(long, default_value = "ubuntu-24.04", help = "Guest OS template")]
    pub os: String,

    #[arg(long, default_value_t = 2, help = "Virtual CPU count (1..=64)")]
    pub cpus: u32,

    #[arg(
        long,
        default_value_t = 4096,
        help = "RAM allocation in MiB (512..=131072)"
    )]
    pub ram: u64,

    #[arg(
        long,
        default_value_t = 32,
        help = "Virtual disk capacity in GiB (1..=2048)"
    )]
    pub disk: u64,

    #[arg(
        long,
        default_value = "nat",
        help = "Network isolation profile (isolated | nat | bridged)"
    )]
    pub network: String,

    #[arg(
        long,
        default_value = "mock",
        help = "Hypervisor backend provider (mock | hyperv | qemu-whp | qemu-kvm)"
    )]
    pub backend: String,
}

#[derive(Args, Debug, Clone)]
pub struct ListArgs {}

#[derive(Args, Debug, Clone)]
pub struct InspectArgs {
    #[arg(help = "Machine identifier (UUID) or machine name")]
    pub machine: String,
}

#[derive(Args, Debug, Clone)]
pub struct StartArgs {
    #[arg(help = "Machine identifier (UUID) or machine name")]
    pub machine: String,
}

#[derive(Args, Debug, Clone)]
pub struct StopArgs {
    #[arg(help = "Machine identifier (UUID) or machine name")]
    pub machine: String,

    #[arg(
        long,
        short,
        default_value_t = 15,
        help = "Shutdown timeout in seconds"
    )]
    pub timeout: u64,

    #[arg(long, short, help = "Force immediate termination (virtual power cut)")]
    pub force: bool,
}

#[derive(Args, Debug, Clone)]
pub struct PauseArgs {
    #[arg(help = "Machine identifier (UUID) or machine name")]
    pub machine: String,
}

#[derive(Args, Debug, Clone)]
pub struct ResumeArgs {
    #[arg(help = "Machine identifier (UUID) or machine name")]
    pub machine: String,
}

#[derive(Args, Debug, Clone)]
pub struct StatusArgs {
    #[arg(help = "Machine identifier (UUID) or machine name")]
    pub machine: String,
}

#[derive(Args, Debug, Clone)]
pub struct DeleteArgs {
    #[arg(help = "Machine identifier (UUID) or machine name")]
    pub machine: String,

    #[arg(long, short, help = "Force deletion even if running")]
    pub force: bool,

    #[arg(long, help = "Confirm deletion without interactive prompt")]
    pub confirm: bool,
}
