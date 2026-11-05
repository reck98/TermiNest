use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use crate::domain::{
    AuditAction, AuditRecord, MachineConfig, MachineId, MachineName, MachineState,
};
use crate::error::{CoreError, HypervisorError};
use crate::orchestrator::{BackendRegistry, MachineStatusView, MachineSummaryView};
use crate::persistence::{MachineRecord, MachineRepository};

/// Central orchestration engine coordinating machine definitions, lifecycle transitions,
/// persistence, backend operations, durable audit logging, and state reconciliation.
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

        // Level 1: Acquire repository inventory lock
        let _repo_lock = self
            .repository
            .acquire_repo_lock(Duration::from_secs(5))
            .await?;

        // Level 2: Acquire per-machine lock for the new machine ID
        let _machine_file_lock = self
            .repository
            .acquire_machine_lock(&config.id, Duration::from_secs(5))
            .await?;
        let in_proc_lock = self.acquire_machine_lock(&config.id).await;
        let _in_proc_guard = in_proc_lock.lock().await;

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

        // Pre-mutation Intent: Record requested audit event before backend execution (fail closed)
        let intent_event = AuditRecord::requested(
            config.id,
            config.name.clone(),
            AuditAction::Create,
            Some(MachineState::Defined),
            Some(MachineState::Created),
            "orchestrator",
        );
        self.repository
            .append_audit_event(&config.id, &intent_event)
            .await
            .map_err(|e| CoreError::AuditFailure {
                machine: config.name.to_string(),
                error: format!("Failed to record audit intent: {e}"),
            })?;

        // Instantiate virtual machine on backend
        if let Err(err) = backend.create_machine(&config).await {
            // Record failed audit event before compensation rollback
            let fail_event = AuditRecord::failed(
                config.id,
                config.name.clone(),
                AuditAction::Create,
                Some(MachineState::Defined),
                Some(MachineState::Failed),
                "orchestrator",
                err.to_string(),
            );
            if let Err(e) = self
                .repository
                .append_audit_event(&config.id, &fail_event)
                .await
            {
                tracing::error!(
                    "Failed to record audit failure event for machine {}: {}",
                    config.id,
                    e
                );
            }

            // Compensation: rollback persisted definition if backend creation failed
            let _ = self.repository.delete(&config.id).await;
            return Err(CoreError::Backend(err));
        }

        // Transition persisted state to Created
        let updated = self
            .repository
            .update_state(&config.id, MachineState::Created)
            .await?;

        // Record successful creation audit event
        let success_event = AuditRecord::completed(
            config.id,
            config.name.clone(),
            AuditAction::Create,
            Some(MachineState::Defined),
            Some(MachineState::Created),
            "orchestrator",
        );
        if let Err(e) = self
            .repository
            .append_audit_event(&config.id, &success_event)
            .await
        {
            tracing::error!(
                "Failed to record audit completion event for machine {}: {}",
                config.id,
                e
            );
            return Err(CoreError::AuditPostMutationFailure {
                operation: "create".into(),
                machine: config.name.to_string(),
                error: e.to_string(),
            });
        }

        Ok(updated)
    }

    /// Starts a virtual machine, updating persisted state and returning runtime status.
    pub async fn start_machine(&self, selector: &str) -> Result<MachineStatusView, CoreError> {
        let record = self.resolve_machine(selector).await?;
        let id = record.config.id;

        // Level 2: Acquire per-machine lock
        let _machine_file_lock = self
            .repository
            .acquire_machine_lock(&id, Duration::from_secs(5))
            .await?;
        let lock = self.acquire_machine_lock(&id).await;
        let _guard = lock.lock().await;

        // Reload latest state from repository under lock to guarantee fresh data; fail if deleted
        let record = self
            .repository
            .get_by_id(&id)
            .await?
            .ok_or_else(|| CoreError::MachineNotFound(selector.to_string()))?;

        // Verify lifecycle rule
        record.state.transition_to(MachineState::Starting)?;

        let backend = self
            .backend_registry
            .get(record.config.hypervisor.backend)?;

        // Pre-mutation Intent: Record requested audit event before backend execution (fail closed)
        let intent_event = AuditRecord::requested(
            id,
            record.config.name.clone(),
            AuditAction::Start,
            Some(record.state),
            Some(MachineState::Starting),
            "orchestrator",
        );
        self.repository
            .append_audit_event(&id, &intent_event)
            .await
            .map_err(|e| CoreError::AuditFailure {
                machine: record.config.name.to_string(),
                error: format!("Failed to record audit intent: {e}"),
            })?;

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

                let success_event = AuditRecord::completed(
                    id,
                    updated.config.name.clone(),
                    AuditAction::Start,
                    Some(MachineState::Starting),
                    Some(MachineState::Running),
                    "orchestrator",
                );
                if let Err(e) = self
                    .repository
                    .append_audit_event(&id, &success_event)
                    .await
                {
                    tracing::error!(
                        "Failed to record audit completion event for machine {}: {}",
                        id,
                        e
                    );
                    return Err(CoreError::AuditPostMutationFailure {
                        operation: "start".into(),
                        machine: updated.config.name.to_string(),
                        error: e.to_string(),
                    });
                }

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

                let fail_event = AuditRecord::failed(
                    id,
                    record.config.name.clone(),
                    AuditAction::Start,
                    Some(MachineState::Starting),
                    Some(MachineState::Failed),
                    "orchestrator",
                    err.to_string(),
                );
                if let Err(e) = self.repository.append_audit_event(&id, &fail_event).await {
                    tracing::error!(
                        "Failed to record audit failure event for machine {}: {}",
                        id,
                        e
                    );
                }

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

        // Level 2: Acquire per-machine lock
        let _machine_file_lock = self
            .repository
            .acquire_machine_lock(&id, Duration::from_secs(5))
            .await?;
        let lock = self.acquire_machine_lock(&id).await;
        let _guard = lock.lock().await;

        let record = self
            .repository
            .get_by_id(&id)
            .await?
            .ok_or_else(|| CoreError::MachineNotFound(selector.to_string()))?;

        if !force {
            record.state.transition_to(MachineState::Stopping)?;
        }

        let backend = self
            .backend_registry
            .get(record.config.hypervisor.backend)?;

        // Pre-mutation Intent: Record requested audit event before backend execution (fail closed)
        let intent_event = AuditRecord::requested(
            id,
            record.config.name.clone(),
            AuditAction::Stop,
            Some(record.state),
            Some(MachineState::Stopping),
            "orchestrator",
        );
        self.repository
            .append_audit_event(&id, &intent_event)
            .await
            .map_err(|e| CoreError::AuditFailure {
                machine: record.config.name.to_string(),
                error: format!("Failed to record audit intent: {e}"),
            })?;

        self.repository
            .update_state(&id, MachineState::Stopping)
            .await?;

        match backend.stop_machine(&id, timeout, force).await {
            Ok(()) => {
                let updated = self
                    .repository
                    .update_state(&id, MachineState::Stopped)
                    .await?;

                let success_event = AuditRecord::completed(
                    id,
                    updated.config.name.clone(),
                    AuditAction::Stop,
                    Some(MachineState::Stopping),
                    Some(MachineState::Stopped),
                    "orchestrator",
                );
                if let Err(e) = self
                    .repository
                    .append_audit_event(&id, &success_event)
                    .await
                {
                    tracing::error!(
                        "Failed to record audit completion event for machine {}: {}",
                        id,
                        e
                    );
                    return Err(CoreError::AuditPostMutationFailure {
                        operation: "stop".into(),
                        machine: updated.config.name.to_string(),
                        error: e.to_string(),
                    });
                }

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
            Err(err) => {
                let _ = self
                    .repository
                    .update_state(&id, MachineState::Failed)
                    .await;

                let fail_event = AuditRecord::failed(
                    id,
                    record.config.name.clone(),
                    AuditAction::Stop,
                    Some(MachineState::Stopping),
                    Some(MachineState::Failed),
                    "orchestrator",
                    err.to_string(),
                );
                if let Err(e) = self.repository.append_audit_event(&id, &fail_event).await {
                    tracing::error!(
                        "Failed to record audit failure event for machine {}: {}",
                        id,
                        e
                    );
                }

                Err(CoreError::Backend(err))
            }
        }
    }

    /// Freezes virtual CPU execution of a running machine.
    pub async fn pause_machine(&self, selector: &str) -> Result<MachineStatusView, CoreError> {
        let record = self.resolve_machine(selector).await?;
        let id = record.config.id;

        // Level 2: Acquire per-machine lock
        let _machine_file_lock = self
            .repository
            .acquire_machine_lock(&id, Duration::from_secs(5))
            .await?;
        let lock = self.acquire_machine_lock(&id).await;
        let _guard = lock.lock().await;

        let record = self
            .repository
            .get_by_id(&id)
            .await?
            .ok_or_else(|| CoreError::MachineNotFound(selector.to_string()))?;

        record.state.transition_to(MachineState::Pausing)?;

        let backend = self
            .backend_registry
            .get(record.config.hypervisor.backend)?;

        // Pre-mutation Intent: Record requested audit event before backend execution (fail closed)
        let intent_event = AuditRecord::requested(
            id,
            record.config.name.clone(),
            AuditAction::Pause,
            Some(record.state),
            Some(MachineState::Paused),
            "orchestrator",
        );
        self.repository
            .append_audit_event(&id, &intent_event)
            .await
            .map_err(|e| CoreError::AuditFailure {
                machine: record.config.name.to_string(),
                error: format!("Failed to record audit intent: {e}"),
            })?;

        match backend.pause_machine(&id).await {
            Ok(()) => {
                let updated = self
                    .repository
                    .update_state(&id, MachineState::Paused)
                    .await?;

                let success_event = AuditRecord::completed(
                    id,
                    updated.config.name.clone(),
                    AuditAction::Pause,
                    Some(MachineState::Running),
                    Some(MachineState::Paused),
                    "orchestrator",
                );
                if let Err(e) = self
                    .repository
                    .append_audit_event(&id, &success_event)
                    .await
                {
                    tracing::error!(
                        "Failed to record audit completion event for machine {}: {}",
                        id,
                        e
                    );
                    return Err(CoreError::AuditPostMutationFailure {
                        operation: "pause".into(),
                        machine: updated.config.name.to_string(),
                        error: e.to_string(),
                    });
                }

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
            Err(err) => {
                let fail_event = AuditRecord::failed(
                    id,
                    record.config.name.clone(),
                    AuditAction::Pause,
                    Some(record.state),
                    Some(record.state),
                    "orchestrator",
                    err.to_string(),
                );
                if let Err(e) = self.repository.append_audit_event(&id, &fail_event).await {
                    tracing::error!(
                        "Failed to record audit failure event for machine {}: {}",
                        id,
                        e
                    );
                }

                Err(CoreError::Backend(err))
            }
        }
    }

    /// Unfreezes virtual CPU execution of a paused machine.
    pub async fn resume_machine(&self, selector: &str) -> Result<MachineStatusView, CoreError> {
        let record = self.resolve_machine(selector).await?;
        let id = record.config.id;

        // Level 2: Acquire per-machine lock
        let _machine_file_lock = self
            .repository
            .acquire_machine_lock(&id, Duration::from_secs(5))
            .await?;
        let lock = self.acquire_machine_lock(&id).await;
        let _guard = lock.lock().await;

        let record = self
            .repository
            .get_by_id(&id)
            .await?
            .ok_or_else(|| CoreError::MachineNotFound(selector.to_string()))?;

        record.state.transition_to(MachineState::Resuming)?;

        let backend = self
            .backend_registry
            .get(record.config.hypervisor.backend)?;

        // Pre-mutation Intent: Record requested audit event before backend execution (fail closed)
        let intent_event = AuditRecord::requested(
            id,
            record.config.name.clone(),
            AuditAction::Resume,
            Some(record.state),
            Some(MachineState::Running),
            "orchestrator",
        );
        self.repository
            .append_audit_event(&id, &intent_event)
            .await
            .map_err(|e| CoreError::AuditFailure {
                machine: record.config.name.to_string(),
                error: format!("Failed to record audit intent: {e}"),
            })?;

        match backend.resume_machine(&id).await {
            Ok(()) => {
                let updated = self
                    .repository
                    .update_state(&id, MachineState::Running)
                    .await?;

                let success_event = AuditRecord::completed(
                    id,
                    updated.config.name.clone(),
                    AuditAction::Resume,
                    Some(MachineState::Paused),
                    Some(MachineState::Running),
                    "orchestrator",
                );
                if let Err(e) = self
                    .repository
                    .append_audit_event(&id, &success_event)
                    .await
                {
                    tracing::error!(
                        "Failed to record audit completion event for machine {}: {}",
                        id,
                        e
                    );
                    return Err(CoreError::AuditPostMutationFailure {
                        operation: "resume".into(),
                        machine: updated.config.name.to_string(),
                        error: e.to_string(),
                    });
                }

                let status = backend.get_machine_status(&id).await.unwrap_or(
                    crate::backend::BackendMachineStatus {
                        id,
                        state: MachineState::Running,
                        pid: None,
                        uptime_secs: 0,
                        memory_rss_bytes: 0,
                    },
                );

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
                let fail_event = AuditRecord::failed(
                    id,
                    record.config.name.clone(),
                    AuditAction::Resume,
                    Some(record.state),
                    Some(record.state),
                    "orchestrator",
                    err.to_string(),
                );
                if let Err(e) = self.repository.append_audit_event(&id, &fail_event).await {
                    tracing::error!(
                        "Failed to record audit failure event for machine {}: {}",
                        id,
                        e
                    );
                }

                Err(CoreError::Backend(err))
            }
        }
    }

    /// Safely deletes a virtual machine, enforcing lifecycle invariants, persistent audit durability,
    /// external per-machine locking, and idempotent backend recovery.
    pub async fn delete_machine(&self, selector: &str, force: bool) -> Result<(), CoreError> {
        // Level 1: Acquire repository inventory lock
        let _repo_lock = self
            .repository
            .acquire_repo_lock(Duration::from_secs(5))
            .await?;

        let record = self.resolve_machine(selector).await?;
        let id = record.config.id;

        // Level 2: Acquire per-machine lock on external lock path
        let _machine_file_lock = self
            .repository
            .acquire_machine_lock(&id, Duration::from_secs(5))
            .await?;
        let lock = self.acquire_machine_lock(&id).await;
        let _guard = lock.lock().await;

        let record = self
            .repository
            .get_by_id(&id)
            .await?
            .ok_or_else(|| CoreError::MachineNotFound(selector.to_string()))?;

        // Strict lifecycle invariant enforcement
        if !force {
            record.state.transition_to(MachineState::Deleting)?;
        } else if record.state.is_running()
            || record.state == MachineState::Paused
            || record.state == MachineState::Pausing
            || record.state == MachineState::Stopping
        {
            // Active or transitional machine requires forced stop on backend
            if let Ok(backend) = self.backend_registry.get(record.config.hypervisor.backend) {
                backend
                    .stop_machine(&id, Duration::from_secs(3), true)
                    .await?;
            }
        }

        // Pre-mutation Intent: Record requested delete event (fail closed)
        let intent_event = AuditRecord::requested(
            id,
            record.config.name.clone(),
            AuditAction::Delete,
            Some(record.state),
            Some(MachineState::Deleting),
            "orchestrator",
        );
        self.repository
            .append_audit_event(&id, &intent_event)
            .await
            .map_err(|e| CoreError::AuditFailure {
                machine: record.config.name.to_string(),
                error: format!("Failed to record audit intent: {e}"),
            })?;

        let _ = self
            .repository
            .update_state(&id, MachineState::Deleting)
            .await;

        // Delete from backend with idempotent handling (treating MachineNotFound as already removed)
        if let Ok(backend) = self.backend_registry.get(record.config.hypervisor.backend) {
            match backend.delete_machine(&id).await {
                Ok(()) => {}
                Err(HypervisorError::MachineNotFound(_)) => {
                    tracing::info!(
                        "Machine '{}' was already removed from hypervisor backend; proceeding with repository deletion",
                        id
                    );
                }
                Err(e) => return Err(CoreError::Backend(e)),
            }
        }

        // Record deletion completion audit event (persisted in dedicated audit directory)
        let delete_event = AuditRecord::completed(
            id,
            record.config.name.clone(),
            AuditAction::Delete,
            Some(record.state),
            Some(MachineState::Deleting),
            "orchestrator",
        );
        if let Err(e) = self.repository.append_audit_event(&id, &delete_event).await {
            tracing::error!(
                "Failed to record audit completion event for machine {}: {}",
                id,
                e
            );
        }

        // Delete machine directory from repository.
        // NOTE: The external per-machine lock (_machine_file_lock) remains held across this operation
        // and is NOT dropped early, preventing any concurrent race interleaving.
        self.repository.delete(&id).await?;

        Ok(())
    }

    /// Queries the live runtime status of a virtual machine, reconciling persistent state with the hypervisor backend
    /// under exclusive per-machine locking whenever state mutation is required.
    pub async fn get_machine_status(&self, selector: &str) -> Result<MachineStatusView, CoreError> {
        let record = self.resolve_machine(selector).await?;
        let id = record.config.id;

        let backend = self.backend_registry.get(record.config.hypervisor.backend);

        let (mut reconciled_state, pid, uptime_secs, memory_rss_bytes) = match backend {
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
                    (state, None, 0, 0)
                }
                Err(HypervisorError::Unavailable(_)) => (MachineState::Unknown, None, 0, 0),
                Err(_) => (record.state, None, 0, 0),
            },
            Err(_) => (MachineState::Unknown, None, 0, 0),
        };

        // If a state discrepancy is detected, acquire per-machine lock and reconcile persistent state
        if reconciled_state != record.state {
            let _machine_file_lock = self
                .repository
                .acquire_machine_lock(&id, Duration::from_secs(5))
                .await?;
            let lock = self.acquire_machine_lock(&id).await;
            let _guard = lock.lock().await;

            let current = self
                .repository
                .get_by_id(&id)
                .await?
                .ok_or_else(|| CoreError::MachineNotFound(selector.to_string()))?;

            // Avoid overwriting active transitional states (Starting, Pausing, Resuming, Stopping, Deleting)
            let is_transitional = matches!(
                current.state,
                MachineState::Starting
                    | MachineState::Pausing
                    | MachineState::Resuming
                    | MachineState::Stopping
                    | MachineState::Deleting
            );

            if !is_transitional && reconciled_state != current.state {
                let _ = self.repository.update_state(&id, reconciled_state).await;
                let rec_event = AuditRecord::completed(
                    id,
                    current.config.name.clone(),
                    AuditAction::Reconcile,
                    Some(current.state),
                    Some(reconciled_state),
                    "orchestrator",
                );
                if let Err(e) = self.repository.append_audit_event(&id, &rec_event).await {
                    tracing::error!(
                        "Failed to record audit reconciliation event for machine {}: {}",
                        id,
                        e
                    );
                }
            } else if is_transitional {
                // If transitional, preserve recorded transitional state
                reconciled_state = current.state;
            }
        }

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
