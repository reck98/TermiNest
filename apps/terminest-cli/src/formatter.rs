use crate::doctor::DoctorReport;
use serde::Serialize;
use terminest_core::orchestrator::{MachineStatusView, MachineSummaryView};
use terminest_core::persistence::MachineRecord;

pub fn print_json<T: Serialize>(value: &T) {
    if let Ok(json) = serde_json::to_string_pretty(value) {
        println!("{json}");
    }
}

pub fn print_error(msg: &str, json: bool) {
    if json {
        let err_obj = serde_json::json!({
            "status": "error",
            "message": msg
        });
        eprintln!("{err_obj}");
    } else {
        eprintln!("Error: {msg}");
    }
}

pub fn print_machine_summary_table(summaries: &[MachineSummaryView]) {
    if summaries.is_empty() {
        println!("No virtual machines registered. Use 'terminest machine create' to define one.");
        return;
    }

    println!(
        "{:<36}  {:<16}  {:<10}  {:<8}  {:<4}  {:<8}  {:<8}",
        "ID", "NAME", "STATE", "BACKEND", "CPUS", "RAM (MB)", "DISK (GB)"
    );
    println!("{}", "-".repeat(102));

    for s in summaries {
        println!(
            "{:<36}  {:<16}  {:<10}  {:<8}  {:<4}  {:<8}  {:<8}",
            s.id,
            s.name.as_str(),
            s.state,
            s.backend,
            s.cpus,
            s.memory_mb,
            s.storage_gb
        );
    }
}

pub fn print_machine_status(status: &MachineStatusView) {
    println!("Machine Status: {}", status.name.as_str());
    println!("  ID:          {}", status.id);
    println!("  State:       {}", status.state);
    println!("  Backend:     {}", status.backend);
    if let Some(pid) = status.pid {
        println!("  PID:         {pid}");
    } else {
        println!("  PID:         None");
    }
    println!("  Uptime:      {}s", status.uptime_secs);
    println!(
        "  Memory RSS:  {} MiB",
        status.memory_rss_bytes / (1024 * 1024)
    );
    println!("  Created:     {}", status.created_at.to_rfc3339());
    println!("  Updated:     {}", status.updated_at.to_rfc3339());
}

pub fn print_machine_record(record: &MachineRecord) {
    println!("Machine Definition: {}", record.config.name.as_str());
    println!("  ID:          {}", record.config.id);
    println!("  Version:     {}", record.config.version);
    println!("  State:       {}", record.state);
    println!(
        "  Guest OS:    {} {} ({})",
        record.config.os.family, record.config.os.distro, record.config.os.arch
    );
    println!("  vCPUs:       {}", record.config.hardware.cpus);
    println!("  RAM:         {} MiB", record.config.hardware.memory_mb);
    println!(
        "  Display:     {}x{} ({} dpi)",
        record.config.hardware.display.width,
        record.config.hardware.display.height,
        record.config.hardware.display.dpi
    );
    println!(
        "  Disk:        {} GiB ({})",
        record.config.storage.capacity_gb, record.config.storage.format
    );
    println!("  Network:     {}", record.config.networking.tier);
    println!("  Backend:     {}", record.config.hypervisor.backend);
}

pub fn print_doctor_report(report: &DoctorReport) {
    println!("TermiNest Environment Pre-Flight Verification");
    println!("=============================================");
    println!(
        "Host OS:           {} ({})",
        report.host_os, report.host_arch
    );
    println!("Data Directory:    {}", report.data_directory);
    println!(
        "Filesystem Ready:  {}",
        if report.filesystem_ready { "YES" } else { "NO" }
    );
    println!("\nHypervisor Backends:");
    for b in &report.hypervisor_backends {
        println!("  - {:<12} [{:<16}] : {}", b.name, b.status, b.details);
    }
    println!(
        "\nM1 Status: {}",
        if report.ready_for_m1 {
            "READY"
        } else {
            "NOT READY"
        }
    );
}
