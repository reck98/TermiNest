use crate::config::TermiNestPaths;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub host_os: String,
    pub host_arch: String,
    pub virtualization_available: bool,
    pub hypervisor_backends: Vec<BackendCheckResult>,
    pub filesystem_ready: bool,
    pub data_directory: String,
    pub ready_for_m1: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendCheckResult {
    pub name: String,
    pub status: String,
    pub details: String,
}

pub fn run_doctor(paths: &TermiNestPaths) -> DoctorReport {
    let host_os = std::env::consts::OS.to_string();
    let host_arch = std::env::consts::ARCH.to_string();

    // Check filesystem write access
    let fs_ready = match fs::create_dir_all(&paths.base_dir) {
        Ok(_) => {
            let probe_file = paths.base_dir.join(".probe_write");
            let write_ok = fs::write(&probe_file, b"ok").is_ok();
            let _ = fs::remove_file(&probe_file);
            write_ok
        }
        Err(_) => false,
    };

    let mut backends = Vec::new();

    // Mock backend check
    backends.push(BackendCheckResult {
        name: "mock".to_string(),
        status: "ready".to_string(),
        details: "In-memory deterministic test hypervisor operational".to_string(),
    });

    // Check platform hypervisors
    if host_os == "windows" {
        backends.push(BackendCheckResult {
            name: "hyperv".to_string(),
            status: "deferred_to_m2".to_string(),
            details: "Native Windows Hyper-V WMI backend scheduled for Milestone 2".to_string(),
        });
        backends.push(BackendCheckResult {
            name: "qemu-whp".to_string(),
            status: "deferred_to_m2".to_string(),
            details: "Windows Hypervisor Platform fallback scheduled for Milestone 2".to_string(),
        });
    } else if host_os == "linux" {
        backends.push(BackendCheckResult {
            name: "qemu-kvm".to_string(),
            status: "deferred_to_m5".to_string(),
            details: "Linux KVM QMP backend scheduled for Milestone 5".to_string(),
        });
    }

    DoctorReport {
        host_os,
        host_arch,
        virtualization_available: true,
        hypervisor_backends: backends,
        filesystem_ready: fs_ready,
        data_directory: paths.base_dir.display().to_string(),
        ready_for_m1: fs_ready,
    }
}
