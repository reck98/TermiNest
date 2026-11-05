use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

use crate::backend::{
    BackendMachineStatus, HypervisorBackend, HypervisorCapabilities, MockFailure,
};
use crate::domain::{HypervisorType, MachineConfig, MachineId, MachineState};
use crate::error::HypervisorError;
use crate::persistence::FileLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MockMachineEntry {
    config: MachineConfig,
    state: MachineState,
    pid: Option<u32>,
    started_at_secs: Option<u64>,
}

fn current_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Mock hypervisor backend for deterministic unit and CLI integration testing.
/// Can operate purely in-memory or persist simulated hypervisor state to a file across CLI invocations.
#[derive(Debug, Clone)]
pub struct MockHypervisorBackend {
    capabilities: HypervisorCapabilities,
    machines: Arc<RwLock<HashMap<MachineId, MockMachineEntry>>>,
    failure: Arc<StdRwLock<Option<MockFailure>>>,
    pid_counter: Arc<AtomicU32>,
    state_file: Option<PathBuf>,
}

impl Default for MockHypervisorBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MockHypervisorBackend {
    /// Creates a new MockHypervisorBackend with standard default capabilities.
    pub fn new() -> Self {
        Self::with_capabilities(HypervisorCapabilities::default())
    }

    /// Creates a MockHypervisorBackend with custom capability flags.
    pub fn with_capabilities(capabilities: HypervisorCapabilities) -> Self {
        Self::with_state_file(capabilities, None)
    }

    /// Creates a MockHypervisorBackend with optional disk-backed persistence across process restarts.
    pub fn with_state_file(
        capabilities: HypervisorCapabilities,
        state_file: Option<PathBuf>,
    ) -> Self {
        let initial_machines = if let Some(ref path) = state_file {
            if path.exists() {
                fs::read(path)
                    .ok()
                    .and_then(|bytes| {
                        serde_json::from_slice::<HashMap<MachineId, MockMachineEntry>>(&bytes).ok()
                    })
                    .unwrap_or_default()
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };

        Self {
            capabilities,
            machines: Arc::new(RwLock::new(initial_machines)),
            failure: Arc::new(StdRwLock::new(None)),
            pid_counter: Arc::new(AtomicU32::new(50000)),
            state_file,
        }
    }

    fn lock_path(&self) -> Option<PathBuf> {
        self.state_file.as_ref().map(|p| p.with_extension("lock"))
    }

    async fn acquire_backend_lock(&self) -> Result<Option<FileLock>, HypervisorError> {
        if let Some(lock_path) = self.lock_path() {
            let lock = FileLock::acquire_exclusive_async(&lock_path, Duration::from_secs(5))
                .await
                .map_err(|e| {
                    HypervisorError::ExecutionFailed(format!(
                        "Timed out acquiring mock backend lock: {e}"
                    ))
                })?;
            Ok(Some(lock))
        } else {
            Ok(None)
        }
    }

    fn reload_from_disk(&self) -> Result<HashMap<MachineId, MockMachineEntry>, HypervisorError> {
        if let Some(ref path) = self.state_file {
            if path.exists() {
                let bytes = fs::read(path).map_err(|e| {
                    HypervisorError::ExecutionFailed(format!(
                        "Failed to read mock backend state file '{}': {e}",
                        path.display()
                    ))
                })?;
                let map: HashMap<MachineId, MockMachineEntry> = serde_json::from_slice(&bytes)
                    .map_err(|e| {
                        HypervisorError::ExecutionFailed(format!(
                            "Corrupt mock backend state file at '{}': {e}",
                            path.display()
                        ))
                    })?;
                return Ok(map);
            }
        }
        Ok(HashMap::new())
    }

    fn persist_state_atomic(
        &self,
        machines: &HashMap<MachineId, MockMachineEntry>,
    ) -> Result<(), HypervisorError> {
        if let Some(ref path) = self.state_file {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let tmp_path = path.with_extension(format!("tmp.{}", uuid::Uuid::new_v4()));
            let json = serde_json::to_vec_pretty(machines).map_err(|e| {
                HypervisorError::ExecutionFailed(format!(
                    "Failed to serialize mock backend state: {e}"
                ))
            })?;

            let mut file = fs::File::create(&tmp_path).map_err(|e| {
                HypervisorError::ExecutionFailed(format!(
                    "Failed to create temporary mock state file: {e}"
                ))
            })?;

            use std::io::Write;
            file.write_all(&json).map_err(|e| {
                let _ = fs::remove_file(&tmp_path);
                HypervisorError::ExecutionFailed(format!(
                    "Failed to write temporary mock state file: {e}"
                ))
            })?;

            file.sync_all().map_err(|e| {
                let _ = fs::remove_file(&tmp_path);
                HypervisorError::ExecutionFailed(format!(
                    "Failed to sync temporary mock state file: {e}"
                ))
            })?;

            drop(file);

            fs::rename(&tmp_path, path).map_err(|e| {
                let _ = fs::remove_file(&tmp_path);
                HypervisorError::ExecutionFailed(format!(
                    "Failed to atomically replace mock state file: {e}"
                ))
            })?;
        }
        Ok(())
    }

    /// Injects a deterministic failure for subsequent operations.
    pub fn inject_failure(&self, failure: MockFailure) {
        let mut lock = self.failure.write().expect("failure lock poisoned");
        *lock = Some(failure);
    }

    /// Clears any previously injected failures.
    pub fn clear_failures(&self) {
        let mut lock = self.failure.write().expect("failure lock poisoned");
        *lock = None;
    }

    fn check_failure(&self, op: &str) -> Result<(), HypervisorError> {
        let lock = self.failure.read().expect("failure lock poisoned");
        if let Some(ref failure) = *lock {
            match failure {
                MockFailure::Unavailable(msg) => {
                    return Err(HypervisorError::Unavailable(msg.clone()));
                }
                MockFailure::Timeout(dur) => {
                    return Err(HypervisorError::Timeout(*dur));
                }
                MockFailure::FailCreate(msg) if op == "create" => {
                    return Err(HypervisorError::ExecutionFailed(msg.clone()));
                }
                MockFailure::FailStart(msg) if op == "start" => {
                    return Err(HypervisorError::ExecutionFailed(msg.clone()));
                }
                MockFailure::FailStop(msg) if op == "stop" => {
                    return Err(HypervisorError::ExecutionFailed(msg.clone()));
                }
                MockFailure::FailPause(msg) if op == "pause" => {
                    return Err(HypervisorError::ExecutionFailed(msg.clone()));
                }
                MockFailure::FailResume(msg) if op == "resume" => {
                    return Err(HypervisorError::ExecutionFailed(msg.clone()));
                }
                MockFailure::FailDelete(msg) if op == "delete" => {
                    return Err(HypervisorError::ExecutionFailed(msg.clone()));
                }
                MockFailure::FailStatus(msg) if op == "status" => {
                    return Err(HypervisorError::ExecutionFailed(msg.clone()));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[async_trait]
impl HypervisorBackend for MockHypervisorBackend {
    fn backend_type(&self) -> HypervisorType {
        HypervisorType::Mock
    }

    fn capabilities(&self) -> HypervisorCapabilities {
        self.capabilities
    }

    async fn probe(&self) -> Result<HypervisorCapabilities, HypervisorError> {
        self.check_failure("probe")?;
        Ok(self.capabilities)
    }

    async fn create_machine(&self, config: &MachineConfig) -> Result<(), HypervisorError> {
        self.check_failure("create")?;

        let _lock_guard = self.acquire_backend_lock().await?;
        let mut lock = self.machines.write().await;
        if self.state_file.is_some() {
            *lock = self.reload_from_disk()?;
        }

        if lock.contains_key(&config.id) {
            return Err(HypervisorError::MachineAlreadyExists(config.id));
        }

        lock.insert(
            config.id,
            MockMachineEntry {
                config: config.clone(),
                state: MachineState::Created,
                pid: None,
                started_at_secs: None,
            },
        );

        self.persist_state_atomic(&lock)?;
        Ok(())
    }

    async fn start_machine(&self, id: &MachineId) -> Result<BackendMachineStatus, HypervisorError> {
        self.check_failure("start")?;

        let _lock_guard = self.acquire_backend_lock().await?;
        let mut lock = self.machines.write().await;
        if self.state_file.is_some() {
            *lock = self.reload_from_disk()?;
        }

        let entry = lock
            .get_mut(id)
            .ok_or(HypervisorError::MachineNotFound(*id))?;

        if entry.state == MachineState::Running {
            return Err(HypervisorError::ExecutionFailed(
                "Machine is already running".into(),
            ));
        }

        let pid = self.pid_counter.fetch_add(1, Ordering::SeqCst);
        let memory_mb = entry.config.hardware.memory_mb;
        entry.state = MachineState::Running;
        entry.pid = Some(pid);
        entry.started_at_secs = Some(current_epoch_secs());

        self.persist_state_atomic(&lock)?;

        Ok(BackendMachineStatus {
            id: *id,
            state: MachineState::Running,
            pid: Some(pid),
            uptime_secs: 0,
            memory_rss_bytes: memory_mb * 1024 * 1024,
        })
    }

    async fn stop_machine(
        &self,
        id: &MachineId,
        _timeout: Duration,
        force: bool,
    ) -> Result<(), HypervisorError> {
        self.check_failure("stop")?;

        let _lock_guard = self.acquire_backend_lock().await?;
        let mut lock = self.machines.write().await;
        if self.state_file.is_some() {
            *lock = self.reload_from_disk()?;
        }

        let entry = lock
            .get_mut(id)
            .ok_or(HypervisorError::MachineNotFound(*id))?;

        if !force && entry.state != MachineState::Running && entry.state != MachineState::Paused {
            return Err(HypervisorError::ExecutionFailed(format!(
                "Cannot stop machine in state '{}'",
                entry.state
            )));
        }

        entry.state = MachineState::Stopped;
        entry.pid = None;
        entry.started_at_secs = None;

        self.persist_state_atomic(&lock)?;
        Ok(())
    }

    async fn pause_machine(&self, id: &MachineId) -> Result<(), HypervisorError> {
        self.check_failure("pause")?;

        if !self.capabilities.pause_resume {
            return Err(HypervisorError::UnsupportedOperation("pause", "mock"));
        }

        let _lock_guard = self.acquire_backend_lock().await?;
        let mut lock = self.machines.write().await;
        if self.state_file.is_some() {
            *lock = self.reload_from_disk()?;
        }

        let entry = lock
            .get_mut(id)
            .ok_or(HypervisorError::MachineNotFound(*id))?;

        if entry.state != MachineState::Running {
            return Err(HypervisorError::ExecutionFailed(format!(
                "Cannot pause machine in state '{}'",
                entry.state
            )));
        }

        entry.state = MachineState::Paused;
        self.persist_state_atomic(&lock)?;
        Ok(())
    }

    async fn resume_machine(&self, id: &MachineId) -> Result<(), HypervisorError> {
        self.check_failure("resume")?;

        if !self.capabilities.pause_resume {
            return Err(HypervisorError::UnsupportedOperation("resume", "mock"));
        }

        let _lock_guard = self.acquire_backend_lock().await?;
        let mut lock = self.machines.write().await;
        if self.state_file.is_some() {
            *lock = self.reload_from_disk()?;
        }

        let entry = lock
            .get_mut(id)
            .ok_or(HypervisorError::MachineNotFound(*id))?;

        if entry.state != MachineState::Paused {
            return Err(HypervisorError::ExecutionFailed(format!(
                "Cannot resume machine in state '{}'",
                entry.state
            )));
        }

        entry.state = MachineState::Running;
        self.persist_state_atomic(&lock)?;
        Ok(())
    }

    async fn delete_machine(&self, id: &MachineId) -> Result<(), HypervisorError> {
        self.check_failure("delete")?;

        let _lock_guard = self.acquire_backend_lock().await?;
        let mut lock = self.machines.write().await;
        if self.state_file.is_some() {
            *lock = self.reload_from_disk()?;
        }

        let entry = lock.get(id).ok_or(HypervisorError::MachineNotFound(*id))?;

        if entry.state == MachineState::Running {
            return Err(HypervisorError::ExecutionFailed(
                "Cannot delete running machine from hypervisor backend".into(),
            ));
        }

        lock.remove(id);
        self.persist_state_atomic(&lock)?;
        Ok(())
    }

    async fn get_machine_status(
        &self,
        id: &MachineId,
    ) -> Result<BackendMachineStatus, HypervisorError> {
        self.check_failure("status")?;

        let _lock_guard = self.acquire_backend_lock().await?;
        let mut lock = self.machines.write().await;
        if self.state_file.is_some() {
            *lock = self.reload_from_disk()?;
        }

        let entry = lock.get(id).ok_or(HypervisorError::MachineNotFound(*id))?;

        let uptime_secs = entry
            .started_at_secs
            .map(|start| current_epoch_secs().saturating_sub(start))
            .unwrap_or(0);

        Ok(BackendMachineStatus {
            id: *id,
            state: entry.state,
            pid: entry.pid,
            uptime_secs,
            memory_rss_bytes: if entry.state == MachineState::Running {
                entry.config.hardware.memory_mb * 1024 * 1024
            } else {
                0
            },
        })
    }
}
