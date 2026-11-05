use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use crate::domain::{LifecycleError, MachineConfig, MachineId, MachineName, MachineState};
use crate::error::{CoreError, HypervisorError};
use crate::orchestrator::{BackendRegistry, MachineStatusView, MachineSummaryView};
use crate::persistence::{MachineRecord, MachineRepository};

/// Central orchestration engine coordinating machine definitions, lifecycle transitions,
/// persistence, backend operations, and state reconciliation.
pub struct Orchestrator {
    repository: Arc<dyn MachineRepository>,
    backend_registry: Arc<BackendRegistry>,
    locks: Arc<Mutex<HashMap<MachineId, Arc<Mutex<()>>>>>,
}

impl Orchestrator {
    /// Constructs a new Orchestrator with the given repository and hypervisor backend registry.
    pub fn new(
        repository: Arc<dyn MachineRepository>,
        backend_registry: Arc<BackendRegistry>,
    ) -> Self {
        Self {
            repository,
            backend_registry,
            locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Acquires an in-process mutual exclusion guard for a specific machine ID.
    async fn acquire_machine_lock(&self, id: &MachineId) -> Arc<Mutex<()>> {
        let mut map = self.locks.lock().await;
        map.entry(*id)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Resolves a machine selector (either a UUID or human-readable machine name) to a MachineRecord.
    pub async fn resolve_machine(&self, selector: &str) -> Result<MachineRecord, CoreError> {
        if let Ok(id) = MachineId::parse(selector) {
            if let Some(rec) = self.repository.get_by_id(&id).await? {
                return Ok(rec);
            }
        }

        if let Ok(name) = MachineName::new(selector) {
            if let Some(rec) = self.repository.get_by_name(&name).await? {
                return Ok(rec);
            }
        }

        Err(CoreError::MachineNotFound(selector.to_string()))
    }

    /// Defines, persists, and instantiates a new virtual machine on its configured hypervisor backend.
    pub async fn create_machine(&self, config: MachineConfig) -> Result<MachineRecord, CoreError> {
        config.validate()?;

        let lock = self.acquire_machine_lock(&config.id).await;
        let _guard = lock.lock().await;

        if let Some(existing) = self.repository.get_by_id(&config.id).await? {
            return Err(CoreError::MachineAlreadyExists(
                existing.config.name.to_string(),
            ));
        }
        if let Some(existing) = self.repository.get_by_name(&config.name).await? {
            return Err(CoreError::MachineAlreadyExists(
                existing.config.name.to_string(),
            ));
        }

        let backend = self.backend_registry.get(config.hypervisor.backend)?;

        // Save initial record in Defined state
        let record = MachineRecord::with_state(config.clone(), MachineState::Defined);
        self.repository.save(&record).await?;

        // Instantiate virtual machine on backend
        if let Err(err) = backend.create_machine(&config).await {
            // Compensation: rollback persisted definition if backend creation failed
            let _ = self.repository.delete(&config.id).await;
            return Err(CoreError::Backend(err));
        }

        // Transition persisted state to Created
        let updated = self
            .repository
            .update_state(&config.id, MachineState::Created)
            .await?;

        Ok(updated)
    }

    /// Starts a virtual machine, updating persisted state and returning runtime status.
    pub async fn start_machine(&self, selector: &str) -> Result<MachineStatusView, CoreError> {
        let record = self.resolve_machine(selector).await?;
        let id = record.config.id;

        let lock = self.acquire_machine_lock(&id).await;
        let _guard = lock.lock().await;

        // Verify lifecycle rule
        record.state.transition_to(MachineState::Starting)?;

        let backend = self
            .backend_registry
            .get(record.config.hypervisor.backend)?;

        // Record transitioning to Starting
        self.repository
            .update_state(&id, MachineState::Starting)
            .await?;

        // Invoke backend
        let start_res = backend.start_machine(&id).await;
        match start_res {
            Ok(status) => {
                let updated = self
                    .repository
                    .update_state(&id, MachineState::Running)
                    .await?;

                Ok(MachineStatusView {
                    id,
                    name: updated.config.name,
                    state: MachineState::Running,
                    backend: updated.config.hypervisor.backend,
                    pid: status.pid,
                    uptime_secs: status.uptime_secs,
                    memory_rss_bytes: status.memory_rss_bytes,
                    created_at: updated.config.created_at,
                    updated_at: updated.config.updated_at,
                })
            }
            Err(err) => {
                let _ = self
                    .repository
                    .update_state(&id, MachineState::Failed)
                    .await;
                Err(CoreError::Backend(err))
            }
        }
    }

    /// Shuts down a virtual machine cleanly or forcefully.
    pub async fn stop_machine(
        &self,
        selector: &str,
        timeout: Duration,
        force: bool,
    ) -> Result<MachineStatusView, CoreError> {
        let record = self.resolve_machine(selector).await?;
        let id = record.config.id;

        let lock = self.acquire_machine_lock(&id).await;
        let _guard = lock.lock().await;

        if !force {
            record.state.transition_to(MachineState::Stopping)?;
        }

        let backend = self
            .backend_registry
            .get(record.config.hypervisor.backend)?;

        self.repository
            .update_state(&id, MachineState::Stopping)
            .await?;

        backend.stop_machine(&id, timeout, force).await?;

        let updated = self
            .repository
            .update_state(&id, MachineState::Stopped)
            .await?;

        Ok(MachineStatusView {
            id,
            name: updated.config.name,
            state: MachineState::Stopped,
            backend: updated.config.hypervisor.backend,
            pid: None,
            uptime_secs: 0,
            memory_rss_bytes: 0,
            created_at: updated.config.created_at,
            updated_at: updated.config.updated_at,
        })
    }

    /// Freezes virtual CPU execution of a running machine.
    pub async fn pause_machine(&self, selector: &str) -> Result<MachineStatusView, CoreError> {
        let record = self.resolve_machine(selector).await?;
        let id = record.config.id;

        let lock = self.acquire_machine_lock(&id).await;
        let _guard = lock.lock().await;

        record.state.transition_to(MachineState::Pausing)?;

        let backend = self
            .backend_registry
            .get(record.config.hypervisor.backend)?;

        backend.pause_machine(&id).await?;

        let updated = self
            .repository
            .update_state(&id, MachineState::Paused)
            .await?;

        Ok(MachineStatusView {
            id,
            name: updated.config.name,
            state: MachineState::Paused,
            backend: updated.config.hypervisor.backend,
            pid: None,
            uptime_secs: 0,
            memory_rss_bytes: 0,
            created_at: updated.config.created_at,
            updated_at: updated.config.updated_at,
        })
    }

    /// Unfreezes virtual CPU execution of a paused machine.
    pub async fn resume_machine(&self, selector: &str) -> Result<MachineStatusView, CoreError> {
        let record = self.resolve_machine(selector).await?;
        let id = record.config.id;

        let lock = self.acquire_machine_lock(&id).await;
        let _guard = lock.lock().await;

        record.state.transition_to(MachineState::Resuming)?;

        let backend = self
            .backend_registry
            .get(record.config.hypervisor.backend)?;

        backend.resume_machine(&id).await?;

        let updated = self
            .repository
            .update_state(&id, MachineState::Running)
            .await?;

        let status =
            backend
                .get_machine_status(&id)
                .await
                .unwrap_or(crate::backend::BackendMachineStatus {
                    id,
                    state: MachineState::Running,
                    pid: None,
                    uptime_secs: 0,
                    memory_rss_bytes: 0,
                });

        Ok(MachineStatusView {
            id,
            name: updated.config.name,
            state: MachineState::Running,
            backend: updated.config.hypervisor.backend,
            pid: status.pid,
            uptime_secs: status.uptime_secs,
            memory_rss_bytes: status.memory_rss_bytes,
            created_at: updated.config.created_at,
            updated_at: updated.config.updated_at,
        })
    }

    /// Safely deletes a virtual machine.
    pub async fn delete_machine(&self, selector: &str, force: bool) -> Result<(), CoreError> {
        let record = self.resolve_machine(selector).await?;
        let id = record.config.id;

        let lock = self.acquire_machine_lock(&id).await;
        let _guard = lock.lock().await;

        if record.state.is_running() {
            if !force {
                return Err(CoreError::Lifecycle(
                    LifecycleError::CannotDeleteWhileRunning,
                ));
            }
            // Stop machine with force before deleting
            if let Ok(backend) = self.backend_registry.get(record.config.hypervisor.backend) {
                let _ = backend
                    .stop_machine(&id, Duration::from_secs(1), true)
                    .await;
            }
        }

        let _ = self
            .repository
            .update_state(&id, MachineState::Deleting)
            .await;

        // Delete from backend
        if let Ok(backend) = self.backend_registry.get(record.config.hypervisor.backend) {
            let _ = backend.delete_machine(&id).await;
        }

        // Delete from repository
        self.repository.delete(&id).await?;
        Ok(())
    }

    /// Queries the live runtime status of a virtual machine, reconciling persistent state with the hypervisor backend.
    pub async fn get_machine_status(&self, selector: &str) -> Result<MachineStatusView, CoreError> {
        let record = self.resolve_machine(selector).await?;
        let id = record.config.id;

        let backend = self.backend_registry.get(record.config.hypervisor.backend);

        let (reconciled_state, pid, uptime_secs, memory_rss_bytes) = match backend {
            Ok(b) => match b.get_machine_status(&id).await {
                Ok(status) => {
                    let state = if record.state == MachineState::Failed
                        && status.state != MachineState::Running
                        && status.state != MachineState::Paused
                    {
                        MachineState::Failed
                    } else {
                        status.state
                    };

                    if state != record.state {
                        let _ = self.repository.update_state(&id, state).await;
                    }
                    (
                        state,
                        status.pid,
                        status.uptime_secs,
                        status.memory_rss_bytes,
                    )
                }
                Err(HypervisorError::MachineNotFound(_)) => {
                    let state = if record.state.is_running() {
                        MachineState::Unknown
                    } else {
                        record.state
                    };
                    if state != record.state {
                        let _ = self.repository.update_state(&id, state).await;
                    }
                    (state, None, 0, 0)
                }
                Err(HypervisorError::Unavailable(_)) => (MachineState::Unknown, None, 0, 0),
                Err(_) => (record.state, None, 0, 0),
            },
            Err(_) => (MachineState::Unknown, None, 0, 0),
        };

        Ok(MachineStatusView {
            id,
            name: record.config.name,
            state: reconciled_state,
            backend: record.config.hypervisor.backend,
            pid,
            uptime_secs,
            memory_rss_bytes,
            created_at: record.config.created_at,
            updated_at: record.config.updated_at,
        })
    }

    /// Inspects the complete configuration and metadata of a machine.
    pub async fn inspect_machine(&self, selector: &str) -> Result<MachineRecord, CoreError> {
        self.resolve_machine(selector).await
    }

    /// Lists all machines registered in the repository.
    pub async fn list_machines(&self) -> Result<Vec<MachineSummaryView>, CoreError> {
        let records = self.repository.list().await?;
        let mut summaries = Vec::with_capacity(records.len());

        for rec in records {
            // Check status for live reconciliation
            let status_res = self.get_machine_status(&rec.config.id.to_string()).await;
            let state = status_res.map(|s| s.state).unwrap_or(rec.state);

            summaries.push(MachineSummaryView {
                id: rec.config.id,
                name: rec.config.name,
                state,
                backend: rec.config.hypervisor.backend,
                cpus: rec.config.hardware.cpus,
                memory_mb: rec.config.hardware.memory_mb,
                storage_gb: rec.config.storage.capacity_gb,
            });
        }

        Ok(summaries)
    }
}
