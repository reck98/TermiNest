use std::path::PathBuf;

use super::error::QemuError;

/// Minimal typed process launch configuration for QEMU x86_64 virtualization.
///
/// Implements strict argument validation to guarantee injection immunity and
/// headless execution safety. Arbitrary CLI arguments (`extra_args`) are
/// explicitly excluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QemuProcessConfig {
    pub executable: PathBuf,
    pub working_dir: PathBuf,
    pub machine_type: QemuMachineType,
    pub accelerator: QemuAccelerator,
    pub cpu: Option<QemuCpuConfig>,
    pub memory_mb: Option<u64>,
    pub smp_vcpus: Option<u32>,
    pub display: QemuDisplay,
}

/// Supported QEMU x86_64 machine types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QemuMachineType {
    /// Modern PCI Express and ICH9 chipset machine type (`q35`).
    Q35,
    /// Standard legacy PC (i440FX + PIIX, 1996) machine type (`pc`).
    Pc,
}

impl QemuMachineType {
    /// Returns the discrete argument string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Q35 => "q35",
            Self::Pc => "pc",
        }
    }
}

/// Supported QEMU acceleration engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QemuAccelerator {
    /// Windows Hypervisor Platform (required for Windows host production).
    Whpx,
    /// Kernel-based Virtual Machine (Linux host provider).
    Kvm,
    /// Tiny Code Generator software emulation (restricted to test harnesses).
    Tcg,
}

impl QemuAccelerator {
    /// Returns the discrete argument string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Whpx => "whpx",
            Self::Kvm => "kvm",
            Self::Tcg => "tcg",
        }
    }
}

/// Typed CPU configuration with model and optional feature flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QemuCpuConfig {
    pub model: String,
    pub features: Vec<String>,
}

impl QemuCpuConfig {
    /// Creates a new CPU configuration with a base model and no features.
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            features: Vec::new(),
        }
    }

    /// Creates a CPU configuration with a base model and explicit feature flags.
    pub fn with_features(model: impl Into<String>, features: Vec<String>) -> Self {
        Self {
            model: model.into(),
            features,
        }
    }

    /// Validates the CPU model and feature strings against QEMU option grammar rules.
    pub fn validate(&self) -> Result<(), QemuError> {
        if self.model.is_empty()
            || !self
                .model
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
        {
            return Err(QemuError::ConfigValidation(format!(
                "Invalid CPU model '{}': must match ^[a-zA-Z0-9_.-]+$",
                self.model
            )));
        }

        for feat in &self.features {
            if feat.len() < 2
                || (!feat.starts_with('+') && !feat.starts_with('-'))
                || !feat[1..]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
            {
                return Err(QemuError::ConfigValidation(format!(
                    "Invalid CPU feature '{}': must match ^[+-][a-zA-Z0-9_.-]+$",
                    feat
                )));
            }
        }

        Ok(())
    }

    /// Formats the CPU argument for QEMU: `<model>[,+feat1,-feat2,...]`.
    pub(crate) fn to_qemu_arg(&self) -> String {
        if self.features.is_empty() {
            self.model.clone()
        } else {
            format!("{},{}", self.model, self.features.join(","))
        }
    }
}

/// Headless display suppression modes for background agent execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QemuDisplay {
    /// Complete display suppression (`-display none`), preventing graphical windows.
    None,
}

impl QemuDisplay {
    /// Returns the discrete argument string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
        }
    }
}

impl QemuProcessConfig {
    /// Creates a new minimal launch configuration.
    pub fn new(
        executable: impl Into<PathBuf>,
        working_dir: impl Into<PathBuf>,
        machine_type: QemuMachineType,
        accelerator: QemuAccelerator,
    ) -> Self {
        Self {
            executable: executable.into(),
            working_dir: working_dir.into(),
            machine_type,
            accelerator,
            cpu: None,
            memory_mb: None,
            smp_vcpus: None,
            display: QemuDisplay::None,
        }
    }

    /// Sets the CPU configuration.
    pub fn with_cpu(mut self, cpu: QemuCpuConfig) -> Self {
        self.cpu = Some(cpu);
        self
    }

    /// Sets the allocated memory in megabytes.
    pub fn with_memory_mb(mut self, memory_mb: u64) -> Self {
        self.memory_mb = Some(memory_mb);
        self
    }

    /// Sets the number of virtual CPUs.
    pub fn with_smp_vcpus(mut self, smp_vcpus: u32) -> Self {
        self.smp_vcpus = Some(smp_vcpus);
        self
    }

    /// Validates the configuration for process launch.
    ///
    /// Unconditionally enforces that production configurations use a valid hardware
    /// accelerator (`WHPX` on Windows, `KVM` on Linux). Software emulation via `TCG`
    /// is strictly prohibited on production paths on all platforms and requires explicit
    /// test seam activation.
    pub fn validate(&self) -> Result<(), QemuError> {
        self.validate_internal(false)
    }

    /// Private internal validation engine.
    /// Visible strictly within `crate::backend::qemu::config`.
    fn validate_internal(&self, allow_tcg: bool) -> Result<(), QemuError> {
        if self.executable.as_os_str().is_empty() {
            return Err(QemuError::ConfigValidation(
                "Executable path cannot be empty".to_string(),
            ));
        }

        // TCG software emulation is prohibited on all production paths; permitted only in test harnesses via private seam.
        if self.accelerator == QemuAccelerator::Tcg && !allow_tcg {
            #[cfg(windows)]
            return Err(QemuError::ConfigValidation(
                "TCG software emulation is prohibited on Windows production paths; WHPX is required".into(),
            ));
            #[cfg(not(windows))]
            return Err(QemuError::ConfigValidation(
                "TCG software emulation is prohibited on non-Windows production paths; KVM is required".into(),
            ));
        }

        #[cfg(windows)]
        {
            if self.accelerator == QemuAccelerator::Kvm {
                return Err(QemuError::ConfigValidation(
                    "KVM acceleration is not available on Windows; WHPX is required".into(),
                ));
            }
        }

        #[cfg(not(windows))]
        {
            if self.accelerator == QemuAccelerator::Whpx {
                return Err(QemuError::ConfigValidation(
                    "WHPX acceleration is only supported on Windows".into(),
                ));
            }
        }

        if let Some(ref cpu) = self.cpu {
            cpu.validate()?;
        }

        if let Some(smp) = self.smp_vcpus {
            if smp == 0 || smp > 256 {
                return Err(QemuError::ConfigValidation(format!(
                    "Invalid SMP vCPU count {}: must be between 1 and 256",
                    smp
                )));
            }
        }

        if let Some(mem) = self.memory_mb {
            if !(128..=1_048_576).contains(&mem) {
                return Err(QemuError::ConfigValidation(format!(
                    "Invalid memory size {} MB: must be between 128 MB and 1048576 MB",
                    mem
                )));
            }
        }

        Ok(())
    }

    /// Constructs the discrete argument list for QEMU process invocation.
    ///
    /// This method unconditionally validates the configuration before constructing
    /// arguments. If validation fails (e.g. invalid CPU model, unauthorized TCG on Windows,
    /// or out-of-range memory/SMP), returns `Err(QemuError::ConfigValidation)`.
    pub fn to_args(&self) -> Result<Vec<String>, QemuError> {
        self.validate()?;
        Ok(self.to_args_internal())
    }

    /// Internal argument construction without public validation.
    pub(crate) fn to_args_internal(&self) -> Vec<String> {
        let mut args = vec![
            "-machine".to_string(),
            self.machine_type.as_str().to_string(),
            "-accel".to_string(),
            self.accelerator.as_str().to_string(),
        ];

        if let Some(ref cpu) = self.cpu {
            args.push("-cpu".to_string());
            args.push(cpu.to_qemu_arg());
        }

        if let Some(smp) = self.smp_vcpus {
            args.push("-smp".to_string());
            args.push(smp.to_string());
        }

        if let Some(mem) = self.memory_mb {
            args.push("-m".to_string());
            args.push(format!("{}M", mem));
        }

        args.push("-display".to_string());
        args.push(self.display.as_str().to_string());

        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_minimal_config() -> QemuProcessConfig {
        #[cfg(windows)]
        let accel = QemuAccelerator::Whpx;
        #[cfg(not(windows))]
        let accel = QemuAccelerator::Kvm;

        QemuProcessConfig::new("qemu-system-x86_64", ".", QemuMachineType::Q35, accel)
    }

    #[test]
    fn test_config_command_building_minimal() {
        let config = valid_minimal_config();
        let args = config.to_args().unwrap();

        assert_eq!(
            args,
            vec![
                "-machine",
                "q35",
                "-accel",
                config.accelerator.as_str(),
                "-display",
                "none"
            ]
        );
    }

    #[test]
    fn test_config_machine_types_x86_64() {
        assert_eq!(QemuMachineType::Q35.as_str(), "q35");
        assert_eq!(QemuMachineType::Pc.as_str(), "pc");

        let mut config = valid_minimal_config();
        config.machine_type = QemuMachineType::Pc;
        let args = config.to_args().unwrap();
        assert_eq!(args[0], "-machine");
        assert_eq!(args[1], "pc");
    }

    #[test]
    fn test_config_display_suppression() {
        let config = valid_minimal_config();
        assert_eq!(config.display.as_str(), "none");
        let args = config.to_args().unwrap();
        assert_eq!(args[args.len() - 2], "-display");
        assert_eq!(args[args.len() - 1], "none");
    }

    #[test]
    #[cfg(windows)]
    fn test_config_windows_tcg_rejected() {
        let mut config = valid_minimal_config();
        config.accelerator = QemuAccelerator::Tcg;

        let result = config.validate();
        assert!(
            matches!(result, Err(QemuError::ConfigValidation(msg)) if msg.contains("TCG software emulation is prohibited on Windows production paths; WHPX is required"))
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn test_config_non_windows_tcg_rejected() {
        let mut config = valid_minimal_config();
        config.accelerator = QemuAccelerator::Tcg;

        let result = config.validate();
        assert!(
            matches!(result, Err(QemuError::ConfigValidation(msg)) if msg.contains("TCG software emulation is prohibited on non-Windows production paths; KVM is required"))
        );
    }

    #[test]
    #[cfg(windows)]
    fn test_config_windows_kvm_rejected() {
        let mut config = valid_minimal_config();
        config.accelerator = QemuAccelerator::Kvm;

        let result = config.validate();
        assert!(
            matches!(result, Err(QemuError::ConfigValidation(msg)) if msg.contains("KVM acceleration is not available on Windows"))
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn test_config_non_windows_whpx_rejected() {
        let mut config = valid_minimal_config();
        config.accelerator = QemuAccelerator::Whpx;

        let result = config.validate();
        assert!(
            matches!(result, Err(QemuError::ConfigValidation(msg)) if msg.contains("WHPX acceleration is only supported on Windows"))
        );
    }

    #[test]
    fn test_config_valid_platform_accelerator_accepted() {
        let config = valid_minimal_config();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_unit_tcg_validation() {
        let mut config = valid_minimal_config();
        config.accelerator = QemuAccelerator::Tcg;

        // Calling private validate_internal with allow_tcg = true passes on all platforms
        let result = config.validate_internal(true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_injection_cpu_model_rejected() {
        let mut config = valid_minimal_config();
        config.cpu = Some(QemuCpuConfig::new("Skylake,kernel-irqchip=off"));

        let result = config.validate();
        assert!(
            matches!(result, Err(QemuError::ConfigValidation(msg)) if msg.contains("Invalid CPU model"))
        );

        // Additional injection vectors
        let dangerous_models = [
            "Skylake;reboot",
            "Skylake=on",
            "Skylake cpu",
            "Skylake\0evil",
            "",
        ];
        for model in dangerous_models {
            config.cpu = Some(QemuCpuConfig::new(model));
            assert!(
                config.validate().is_err(),
                "Expected rejection for model: {:?}",
                model
            );
        }
    }

    #[test]
    fn test_config_injection_cpu_feature_valid() {
        let mut config = valid_minimal_config();
        config.cpu = Some(QemuCpuConfig::with_features(
            "Skylake-Client",
            vec!["+aes".to_string(), "-hle".to_string()],
        ));

        assert!(config.validate().is_ok());

        let args = config.to_args().unwrap();
        assert!(args.contains(&"-cpu".to_string()));
        let cpu_idx = args.iter().position(|a| a == "-cpu").unwrap();
        assert_eq!(args[cpu_idx + 1], "Skylake-Client,+aes,-hle");
    }

    #[test]
    fn test_config_injection_cpu_feature_invalid() {
        let mut config = valid_minimal_config();

        let bad_features = [
            "+aes=on",
            "+aes,hack",
            "aes",       // missing +/-
            "+",         // missing feature name
            "-",         // missing feature name
            "+aes;drop", // semicolon
            "+aes foo",  // space
        ];

        for bad in bad_features {
            config.cpu = Some(QemuCpuConfig::with_features(
                "Skylake-Client",
                vec![bad.to_string()],
            ));
            let result = config.validate();
            assert!(
                matches!(result, Err(QemuError::ConfigValidation(msg)) if msg.contains("Invalid CPU feature")),
                "Expected rejection for feature: {:?}",
                bad
            );
        }
    }

    #[test]
    fn test_config_smp_validation() {
        let mut config = valid_minimal_config();

        // Valid bounds: 1..=256
        config.smp_vcpus = Some(1);
        assert!(config.validate().is_ok());
        config.smp_vcpus = Some(256);
        assert!(config.validate().is_ok());

        // Invalid: 0
        config.smp_vcpus = Some(0);
        assert!(
            matches!(config.validate(), Err(QemuError::ConfigValidation(msg)) if msg.contains("Invalid SMP vCPU count"))
        );

        // Invalid: 257
        config.smp_vcpus = Some(257);
        assert!(
            matches!(config.validate(), Err(QemuError::ConfigValidation(msg)) if msg.contains("Invalid SMP vCPU count"))
        );
    }

    #[test]
    fn test_config_memory_validation() {
        let mut config = valid_minimal_config();

        // Valid bounds: 128..=1048576
        config.memory_mb = Some(128);
        assert!(config.validate().is_ok());
        config.memory_mb = Some(1_048_576);
        assert!(config.validate().is_ok());

        // Invalid: 127
        config.memory_mb = Some(127);
        assert!(
            matches!(config.validate(), Err(QemuError::ConfigValidation(msg)) if msg.contains("Invalid memory size"))
        );

        // Invalid: 1048577
        config.memory_mb = Some(1_048_577);
        assert!(
            matches!(config.validate(), Err(QemuError::ConfigValidation(msg)) if msg.contains("Invalid memory size"))
        );
    }

    #[test]
    fn test_config_empty_executable_rejected() {
        let mut config = valid_minimal_config();
        config.executable = PathBuf::new();
        assert!(
            matches!(config.validate(), Err(QemuError::ConfigValidation(msg)) if msg.contains("Executable path cannot be empty"))
        );
    }

    #[test]
    fn test_config_full_argument_emission() {
        let config = valid_minimal_config()
            .with_cpu(QemuCpuConfig::with_features(
                "Skylake-Client",
                vec!["-hle".to_string(), "-rtm".to_string()],
            ))
            .with_memory_mb(4096)
            .with_smp_vcpus(4);

        assert!(config.validate().is_ok());

        let args = config.to_args().unwrap();
        assert_eq!(
            args,
            vec![
                "-machine",
                "q35",
                "-accel",
                config.accelerator.as_str(),
                "-cpu",
                "Skylake-Client,-hle,-rtm",
                "-smp",
                "4",
                "-m",
                "4096M",
                "-display",
                "none",
            ]
        );
    }

    #[test]
    fn test_config_to_args_rejects_invalid_config() {
        let mut config = valid_minimal_config();
        config.executable = PathBuf::new();
        assert!(config.to_args().is_err());

        let mut config = valid_minimal_config();
        config.cpu = Some(QemuCpuConfig::new("Skylake;injected"));
        assert!(config.to_args().is_err());

        let mut config = valid_minimal_config();
        config.smp_vcpus = Some(0);
        assert!(config.to_args().is_err());

        let mut config = valid_minimal_config();
        config.memory_mb = Some(50);
        assert!(config.to_args().is_err());

        let mut config = valid_minimal_config();
        config.accelerator = QemuAccelerator::Tcg;
        assert!(config.to_args().is_err());
    }

    #[test]
    fn test_cpu_config_validation() {
        let valid_cpu =
            QemuCpuConfig::with_features("host", vec!["+invtsc".to_string(), "-vmx".to_string()]);
        assert!(valid_cpu.validate().is_ok());

        let empty_model = QemuCpuConfig::new("");
        assert!(empty_model.validate().is_err());

        let invalid_model = QemuCpuConfig::new("Skylake,hack");
        assert!(invalid_model.validate().is_err());

        let invalid_feature =
            QemuCpuConfig::with_features("host", vec!["invalid_feat".to_string()]);
        assert!(invalid_feature.validate().is_err());
    }
}
