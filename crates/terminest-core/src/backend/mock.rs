use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::{Duration, Instant};
use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::backend::{BackendMachineStatus, HypervisorBackend, HypervisorCapabilities, MockFailure};
use crate::domain::{HypervisorType, MachineConfig, MachineId, MachineState};
use crate::error::HypervisorError;

#[derive(Debug, Clone)]
struct MockMachineEntry {
    config: MachineConfig,
    state: MachineState,
    pid: Option<u32>,
    started_at: Option<Instant>,
}

/// In-memory mock hypervisor backend for deterministic unit and orchestration testing.
#[derive(Debug, Clone)]
pub struct MockHypervisorBackend {
    capabilities: HypervisorCapabilities,
    machines: Arc<RwLock<HashMap<MachineId, MockMachineEntry>>>,
    failure: Arc<StdRwLock<Option<MockFailure>>>,
    pid_counter: Arc<AtomicU32>,
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
        Self {
            capabilities,
            machines: Arc::new(RwLock::new(HashMap::new())),
            failure: Arc::new(StdRwLock::new(None)),
            pid_counter: Arc::new(AtomicU32::new(50000)),
        }
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

        let mut lock = self.machines.write().await;
        if lock.contains_key(&config.id) {
            return Err(HypervisorError::MachineAlreadyExists(config.id));
        }

        lock.insert(
            config.id,
            MockMachineEntry {
                config: config.clone(),
                state: MachineState::Created,
                pid: None,
                started_at: None,
            },
        );

        Ok(())
    }

    async fn start_machine(&self, id: &MachineId) -> Result<BackendMachineStatus, HypervisorError> {
        self.check_failure("start")?;

        let mut lock = self.machines.write().await;
        let entry = lock
            .get_mut(id)
            .ok_or(HypervisorError::MachineNotFound(*id))?;

        if entry.state == MachineState::Running {
            return Err(HypervisorError::ExecutionFailed(
                "Machine is already running".into(),
            ));
        }

        let pid = self.pid_counter.fetch_add(1, Ordering::SeqCst);
        entry.state = MachineState::Running;
        entry.pid = Some(pid);
        entry.started_at = Some(Instant::now());

        Ok(BackendMachineStatus {
            id: *id,
            state: MachineState::Running,
            pid: Some(pid),
            uptime_secs: 0,
            memory_rss_bytes: entry.config.hardware.memory_mb * 1024 * 1024,
        })
    }

    async fn stop_machine(
        &self,
        id: &MachineId,
        _timeout: Duration,
        force: bool,
    ) -> Result<(), HypervisorError> {
        self.check_failure("stop")?;

        let mut lock = self.machines.write().await;
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
        entry.started_at = None;

        Ok(())
    }

    async fn pause_machine(&self, id: &MachineId) -> Result<(), HypervisorError> {
        self.check_failure("pause")?;

        if !self.capabilities.pause_resume {
            return Err(HypervisorError::UnsupportedOperation("pause", "mock"));
        }

        let mut lock = self.machines.write().await;
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
        Ok(())
    }

    async fn resume_machine(&self, id: &MachineId) -> Result<(), HypervisorError> {
        self.check_failure("resume")?;

        if !self.capabilities.pause_resume {
            return Err(HypervisorError::UnsupportedOperation("resume", "mock"));
        }

        let mut lock = self.machines.write().await;
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
        Ok(())
    }

    async fn delete_machine(&self, id: &MachineId) -> Result<(), HypervisorError> {
        self.check_failure("delete")?;

        let mut lock = self.machines.write().await;
        let entry = lock.get(id).ok_or(HypervisorError::MachineNotFound(*id))?;

        if entry.state == MachineState::Running {
            return Err(HypervisorError::ExecutionFailed(
                "Cannot delete running machine from hypervisor backend".into(),
            ));
        }

        lock.remove(id);
        Ok(())
    }

    async fn get_machine_status(
        &self,
        id: &MachineId,
    ) -> Result<BackendMachineStatus, HypervisorError> {
        self.check_failure("status")?;

        let lock = self.machines.read().await;
        let entry = lock.get(id).ok_or(HypervisorError::MachineNotFound(*id))?;

        let uptime_secs = entry
            .started_at
            .map(|t| t.elapsed().as_secs())
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
