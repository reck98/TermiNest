//! Comprehensive integration test suite for the TermiNest CLI (`terminest`).

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;
    use tempfile::tempdir;

    fn get_binary_path() -> std::path::PathBuf {
        let mut path = std::env::current_exe().expect("current exe");
        path.pop(); // pop test exe name
        if path.ends_with("deps") {
            path.pop();
        }
        path.push("terminest.exe");
        if !path.exists() {
            // Unix fallback if running on non-windows
            let mut unix_path = path.clone();
            unix_path.set_extension("");
            if unix_path.exists() {
                return unix_path;
            }
        }
        path
    }

    fn run_terminest(args: &[&str], data_dir: &std::path::Path) -> (i32, String, String) {
        let bin = get_binary_path();
        let mut cmd = Command::new(&bin);
        cmd.arg("--data-dir").arg(data_dir);
        for arg in args {
            cmd.arg(arg);
        }

        let output = cmd.output().unwrap_or_else(|e| {
            panic!("Failed to execute {:?}: {}", bin, e);
        });

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let code = output.status.code().unwrap_or(-1);

        (code, stdout, stderr)
    }

    #[test]
    fn test_cli_help_and_version() {
        let temp = tempdir().expect("tempdir");
        let (code, stdout, _) = run_terminest(&["--help"], temp.path());
        assert_eq!(code, 0);
        assert!(stdout.contains("TermiNest"));
        assert!(stdout.contains("init"));
        assert!(stdout.contains("doctor"));
        assert!(stdout.contains("machine"));

        let (v_code, v_stdout, _) = run_terminest(&["--version"], temp.path());
        assert_eq!(v_code, 0);
        assert!(v_stdout.contains("terminest"));
    }

    #[test]
    fn test_cli_init() {
        let temp = tempdir().expect("tempdir");
        let (code, stdout, _) = run_terminest(&["init"], temp.path());
        assert_eq!(code, 0);
        assert!(stdout.contains("TermiNest initialized successfully"));

        // Check directories created
        assert!(temp.path().join("machines").exists());
        assert!(temp.path().join("templates").exists());
        assert!(temp.path().join("run").exists());
        assert!(temp.path().join("config.json").exists());

        // Test init with --json
        let (j_code, j_stdout, _) = run_terminest(&["--json", "init"], temp.path());
        assert_eq!(j_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&j_stdout).expect("valid json");
        assert_eq!(parsed["status"], "success");
    }

    #[test]
    fn test_cli_doctor() {
        let temp = tempdir().expect("tempdir");
        let (code, stdout, _) = run_terminest(&["doctor"], temp.path());
        assert_eq!(code, 0);
        assert!(stdout.contains("TermiNest Environment Pre-Flight Verification"));
        assert!(stdout.contains("M1 Status: READY"));

        let (j_code, j_stdout, _) = run_terminest(&["--json", "doctor"], temp.path());
        assert_eq!(j_code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&j_stdout).expect("valid json");
        assert_eq!(parsed["ready_for_m1"], true);
    }

    #[test]
    fn test_cli_complete_machine_lifecycle() {
        let temp = tempdir().expect("tempdir");

        // 1. Initialize
        let (code_init, _, _) = run_terminest(&["init"], temp.path());
        assert_eq!(code_init, 0);

        // 2. Create machine via flags
        let (code_create, stdout_create, _) = run_terminest(
            &[
                "machine",
                "create",
                "--name",
                "cli-dev-box",
                "--cpus",
                "4",
                "--ram",
                "8192",
                "--disk",
                "64",
            ],
            temp.path(),
        );
        assert_eq!(code_create, 0);
        assert!(stdout_create.contains("cli-dev-box"));
        assert!(stdout_create.contains("successfully created"));

        // 3. List machines
        let (code_ls, stdout_ls, _) = run_terminest(&["machine", "list"], temp.path());
        assert_eq!(code_ls, 0);
        assert!(stdout_ls.contains("cli-dev-box"));
        assert!(stdout_ls.contains("created"));

        // 4. Inspect machine
        let (code_inspect, stdout_inspect, _) =
            run_terminest(&["machine", "inspect", "cli-dev-box"], temp.path());
        assert_eq!(code_inspect, 0);
        assert!(stdout_inspect.contains("vCPUs:       4"));
        assert!(stdout_inspect.contains("RAM:         8192 MiB"));

        // 5. Start machine
        let (code_start, stdout_start, _) =
            run_terminest(&["machine", "start", "cli-dev-box"], temp.path());
        assert_eq!(code_start, 0);
        assert!(stdout_start.contains("successfully started"));
        assert!(stdout_start.contains("running"));

        // 6. Query status
        let (code_status, stdout_status, _) =
            run_terminest(&["machine", "status", "cli-dev-box"], temp.path());
        assert_eq!(code_status, 0);
        assert!(stdout_status.contains("State:       running"));

        // 7. Pause machine
        let (code_pause, stdout_pause, _) =
            run_terminest(&["machine", "pause", "cli-dev-box"], temp.path());
        assert_eq!(code_pause, 0);
        assert!(stdout_pause.contains("paused"));

        // 8. Resume machine
        let (code_resume, stdout_resume, _) =
            run_terminest(&["machine", "resume", "cli-dev-box"], temp.path());
        assert_eq!(code_resume, 0);
        assert!(stdout_resume.contains("resumed"));

        // 9. Attempt to delete running machine without force (must fail with exit code 1)
        let (code_del_fail, _, stderr_del_fail) =
            run_terminest(&["machine", "delete", "cli-dev-box"], temp.path());
        assert_eq!(code_del_fail, 1);
        assert!(stderr_del_fail.contains("Cannot delete machine while it is running"));

        // 10. Stop machine cleanly
        let (code_stop, stdout_stop, _) =
            run_terminest(&["machine", "stop", "cli-dev-box"], temp.path());
        assert_eq!(code_stop, 0);
        assert!(stdout_stop.contains("stopped"));

        // 11. Delete stopped machine
        let (code_del, stdout_del, _) =
            run_terminest(&["machine", "delete", "cli-dev-box"], temp.path());
        assert_eq!(code_del, 0);
        assert!(stdout_del.contains("successfully deleted"));

        // Verify list is now empty
        let (code_empty, stdout_empty, _) = run_terminest(&["machine", "list"], temp.path());
        assert_eq!(code_empty, 0);
        assert!(stdout_empty.contains("No virtual machines registered"));
    }

    #[test]
    fn test_cli_create_from_file_and_json_mode() {
        let temp = tempdir().expect("tempdir");

        let json_spec = r#"{
            "version": "1.0.0",
            "id": "77777777-7777-7777-7777-777777777777",
            "name": "from-spec-file",
            "os": {
                "family": "linux",
                "distro": "ubuntu",
                "version": "24.04",
                "arch": "x86_64"
            },
            "hardware": {
                "cpus": 2,
                "memory_mb": 4096,
                "nested_virtualization": false,
                "display": {
                    "width": 1920,
                    "height": 1080,
                    "dpi": 96
                }
            },
            "storage": {
                "format": "vhdx",
                "disk_path": "disk.vhdx",
                "capacity_gb": 32,
                "ephemeral": false
            },
            "networking": {
                "tier": "nat",
                "port_forwarding": []
            },
            "hypervisor": {
                "backend": "mock",
                "vsock_port": 10001,
                "vsock_cid": 3
            },
            "created_at": "2026-11-05T00:00:00Z",
            "updated_at": "2026-11-05T00:00:00Z"
        }"#;

        let file_path = temp.path().join("spec.json");
        fs::write(&file_path, json_spec).expect("write spec");

        // Create with --file and --json
        let (code, stdout, _) = run_terminest(
            &["--json", "create", "--file", file_path.to_str().unwrap()],
            temp.path(),
        );
        assert_eq!(code, 0);
        let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");
        assert_eq!(parsed["config"]["name"], "from-spec-file");
        assert_eq!(parsed["state"], "created");

        // Query status in --json
        let (code_st, stdout_st, _) =
            run_terminest(&["--json", "status", "from-spec-file"], temp.path());
        assert_eq!(code_st, 0);
        let status_json: serde_json::Value = serde_json::from_str(&stdout_st).expect("valid json");
        assert_eq!(status_json["name"], "from-spec-file");
        assert_eq!(status_json["state"], "created");
    }

    #[test]
    fn test_cli_persistence_across_separate_invocations() {
        let temp = tempdir().expect("tempdir");

        // Invocation 1: create a machine
        let (c1, _, _) = run_terminest(
            &[
                "create",
                "--name",
                "retained-vm",
                "--cpus",
                "2",
                "--ram",
                "2048",
            ],
            temp.path(),
        );
        assert_eq!(c1, 0);

        // Invocation 2: completely fresh process invocation retrieves machine
        let (c2, stdout2, _) = run_terminest(&["inspect", "retained-vm"], temp.path());
        assert_eq!(c2, 0);
        assert!(stdout2.contains("retained-vm"));
        assert!(stdout2.contains("RAM:         2048 MiB"));

        // Invocation 3: verify list displays it
        let (c3, stdout3, _) = run_terminest(&["ls"], temp.path());
        assert_eq!(c3, 0);
        assert!(stdout3.contains("retained-vm"));
    }

    #[test]
    fn test_cli_error_exit_codes() {
        let temp = tempdir().expect("tempdir");

        // Query non-existent machine -> exit code 1
        let (code_nf, _, stderr_nf) = run_terminest(&["status", "does-not-exist"], temp.path());
        assert_eq!(code_nf, 1);
        assert!(stderr_nf.contains("Machine not found"));

        // Create with invalid cpu (0) -> exit code 1
        let (code_val, _, stderr_val) =
            run_terminest(&["create", "--name", "bad-cpu", "--cpus", "0"], temp.path());
        assert_eq!(code_val, 1);
        assert!(stderr_val.contains("Invalid CPU allocation"));

        // Create with invalid path traversal in name -> exit code 1
        let (code_trav, _, stderr_trav) =
            run_terminest(&["create", "--name", "../escape"], temp.path());
        assert_eq!(code_trav, 1);
        assert!(stderr_trav.contains("Path traversal or illegal path detected"));
    }
}
