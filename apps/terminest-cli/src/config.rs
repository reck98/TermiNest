use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Global configuration file structure (`config.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub default_hypervisor: String,
    pub telemetry_enabled: bool,
    pub log_level: String,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            default_hypervisor: "mock".to_string(),
            telemetry_enabled: false,
            log_level: "info".to_string(),
        }
    }
}

/// Directory paths resolution for TermiNest following cross-platform OS standards.
#[derive(Debug, Clone)]
pub struct TermiNestPaths {
    pub base_dir: PathBuf,
    pub machines_dir: PathBuf,
    pub templates_dir: PathBuf,
    pub run_dir: PathBuf,
    pub config_file: PathBuf,
}

impl TermiNestPaths {
    /// Discovers and constructs TermiNest directory paths.
    /// Priority:
    /// 1. Explicit override path (`override_path`)
    /// 2. `TERMINEST_DATA_DIR` environment variable
    /// 3. Standard platform directory (`%LOCALAPPDATA%\TermiNest` on Windows; `~/.local/share/terminest` on Linux/macOS).
    pub fn discover(override_path: Option<PathBuf>) -> Self {
        let base_dir = if let Some(p) = override_path {
            p
        } else if let Ok(env_path) = std::env::var("TERMINEST_DATA_DIR") {
            PathBuf::from(env_path)
        } else {
            Self::platform_default_dir()
        };

        let machines_dir = base_dir.join("machines");
        let templates_dir = base_dir.join("templates");
        let run_dir = base_dir.join("run");
        let config_file = base_dir.join("config.json");

        Self {
            base_dir,
            machines_dir,
            templates_dir,
            run_dir,
            config_file,
        }
    }

    #[cfg(windows)]
    fn platform_default_dir() -> PathBuf {
        std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::var("USERPROFILE")
                    .map(|u| PathBuf::from(u).join("AppData").join("Local"))
                    .unwrap_or_else(|_| PathBuf::from("."))
            })
            .join("TermiNest")
    }

    #[cfg(not(windows))]
    fn platform_default_dir() -> PathBuf {
        std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::var("HOME")
                    .map(|h| PathBuf::from(h).join(".local").join("share"))
                    .unwrap_or_else(|_| PathBuf::from("."))
            })
            .join("terminest")
    }

    /// Initializes all required directories on disk and generates `config.json` if absent.
    pub fn initialize(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.base_dir)?;
        fs::create_dir_all(&self.machines_dir)?;
        fs::create_dir_all(&self.templates_dir)?;
        fs::create_dir_all(&self.run_dir)?;

        if !self.config_file.exists() {
            let default_cfg = GlobalConfig::default();
            let json = serde_json::to_string_pretty(&default_cfg).unwrap_or_default();
            fs::write(&self.config_file, json)?;
        }

        Ok(())
    }
}
