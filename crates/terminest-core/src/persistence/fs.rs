use async_trait::async_trait;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::domain::{AuditRecord, MachineId, MachineName, MachineState};
use crate::error::PersistenceError;
use crate::persistence::{AuditLogger, FileLock, MachineRecord, MachineRepository};

/// Local filesystem-backed machine repository implementing atomic JSON persistence,
/// durable post-deletion audit storage, and external lock management.
#[derive(Debug, Clone)]
pub struct FilesystemMachineRepository {
    base_dir: PathBuf,
    machines_dir: PathBuf,
    audit_dir: PathBuf,
    locks_dir: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl FilesystemMachineRepository {
    /// Initializes a repository rooted at `base_dir`.
    /// Creates the `machines`, `audit`, and `locks` subdirectories if they do not yet exist,
    /// and migrates any legacy machine-nested audit files.
    pub fn new(base_dir: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        let base_dir = base_dir.as_ref().to_path_buf();
        let machines_dir = base_dir.join("machines");
        let audit_dir = base_dir.join("audit");
        let locks_dir = base_dir.join("locks");
        let machine_locks_dir = locks_dir.join("machines");

        fs::create_dir_all(&machines_dir)
            .map_err(|_| PersistenceError::DirectoryUnreachable(machines_dir.clone()))?;
        fs::create_dir_all(&audit_dir)
            .map_err(|_| PersistenceError::DirectoryUnreachable(audit_dir.clone()))?;
        fs::create_dir_all(&machine_locks_dir)
            .map_err(|_| PersistenceError::DirectoryUnreachable(machine_locks_dir.clone()))?;

        let repo = Self {
            base_dir,
            machines_dir,
            audit_dir,
            locks_dir,
            lock: Arc::new(Mutex::new(())),
        };

        repo.migrate_legacy_audit_logs()?;

        Ok(repo)
    }

    /// Returns the base directory of the repository.
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Returns the path to the machine directory for a specific MachineId.
    pub fn machine_dir(&self, id: &MachineId) -> PathBuf {
        self.machines_dir.join(id.to_string())
    }

    /// Returns the path to the dedicated audit directory.
    pub fn audit_dir(&self) -> &Path {
        &self.audit_dir
    }

    /// Returns the persistent audit log file path for a machine ID.
    pub fn audit_file_path(&self, id: &MachineId) -> PathBuf {
        self.audit_dir.join(format!("{}.jsonl", id))
    }

    /// Returns the path to the dedicated locks directory.
    pub fn locks_dir(&self) -> &Path {
        &self.locks_dir
    }

    /// Returns the path to the repository-wide inventory lock file.
    pub fn repo_lock_path(&self) -> PathBuf {
        self.locks_dir.join("repo.lock")
    }

    /// Returns the path to a machine's external lock file.
    pub fn machine_lock_path(&self, id: &MachineId) -> PathBuf {
        self.locks_dir.join("machines").join(format!("{}.lock", id))
    }

    fn machine_file(&self, id: &MachineId) -> PathBuf {
        self.machine_dir(id).join("machine.json")
    }

    /// Discovers and migrates legacy `<machine_dir>/audit.jsonl` files to `<audit_dir>/<id>.jsonl`.
    /// Operates idempotently without data loss.
    fn migrate_legacy_audit_logs(&self) -> Result<(), PersistenceError> {
        if !self.machines_dir.exists() {
            return Ok(());
        }

        let entries = match fs::read_dir(&self.machines_dir) {
            Ok(entries) => entries,
            Err(_) => return Ok(()),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                if let Ok(id) = MachineId::parse(file_name) {
                    let legacy_audit = path.join("audit.jsonl");
                    if legacy_audit.exists() {
                        let target_audit = self.audit_file_path(&id);
                        if !target_audit.exists() {
                            // Copy legacy audit log to target persistent audit directory
                            let _ = fs::copy(&legacy_audit, &target_audit);
                        } else {
                            // Merge missing records if any
                            let legacy_records =
                                AuditLogger::read_from_file(&legacy_audit).unwrap_or_default();
                            let existing_records =
                                AuditLogger::read_from_file(&target_audit).unwrap_or_default();
                            let existing_ids: std::collections::HashSet<uuid::Uuid> =
                                existing_records.into_iter().map(|r| r.event_id).collect();

                            for rec in legacy_records {
                                if !existing_ids.contains(&rec.event_id) {
                                    let _ = AuditLogger::append_to_file(&target_audit, &rec);
                                }
                            }
                        }

                        // Remove legacy file once verified
                        if target_audit.exists() {
                            let _ = fs::remove_file(&legacy_audit);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn write_atomic(&self, target_path: &Path, content: &[u8]) -> Result<(), PersistenceError> {
        let parent = target_path.parent().ok_or_else(|| {
            PersistenceError::AtomicWriteFailed(target_path.to_path_buf(), "No parent dir".into())
        })?;

        fs::create_dir_all(parent).map_err(PersistenceError::Io)?;

        let tmp_file_name = format!("machine.json.tmp.{}", Uuid::new_v4());
        let tmp_path = parent.join(tmp_file_name);

        let mut file = File::create(&tmp_path).map_err(|e| {
            PersistenceError::AtomicWriteFailed(
                target_path.to_path_buf(),
                format!("Failed to create temporary file: {e}"),
            )
        })?;

        file.write_all(content).map_err(|e| {
            let _ = fs::remove_file(&tmp_path);
            PersistenceError::AtomicWriteFailed(
                target_path.to_path_buf(),
                format!("Failed to write temporary file: {e}"),
            )
        })?;

        file.sync_all().map_err(|e| {
            let _ = fs::remove_file(&tmp_path);
            PersistenceError::AtomicWriteFailed(
                target_path.to_path_buf(),
                format!("Failed to sync temporary file to disk: {e}"),
            )
        })?;

        drop(file);

        fs::rename(&tmp_path, target_path).map_err(|e| {
            let _ = fs::remove_file(&tmp_path);
            PersistenceError::AtomicWriteFailed(
                target_path.to_path_buf(),
                format!("Failed to atomically rename file into place: {e}"),
            )
        })?;

        Ok(())
    }

    fn read_record_file(path: &Path) -> Result<MachineRecord, PersistenceError> {
        let bytes = fs::read(path).map_err(PersistenceError::Io)?;
        let record: MachineRecord = serde_json::from_slice(&bytes)
            .map_err(|e| PersistenceError::CorruptMetadata(path.to_path_buf(), e.to_string()))?;
        Ok(record)
    }
}

#[async_trait]
impl MachineRepository for FilesystemMachineRepository {
    async fn acquire_repo_lock(&self, timeout: Duration) -> Result<FileLock, PersistenceError> {
        FileLock::acquire_exclusive_async(self.repo_lock_path(), timeout).await
    }

    async fn acquire_machine_lock(
        &self,
        id: &MachineId,
        timeout: Duration,
    ) -> Result<FileLock, PersistenceError> {
        FileLock::acquire_exclusive_async(self.machine_lock_path(id), timeout).await
    }

    async fn append_audit_event(
        &self,
        id: &MachineId,
        record: &AuditRecord,
    ) -> Result<(), PersistenceError> {
        let audit_path = self.audit_file_path(id);
        AuditLogger::append_to_file(&audit_path, record)
    }

    async fn read_audit_events(
        &self,
        id: &MachineId,
    ) -> Result<Vec<AuditRecord>, PersistenceError> {
        let audit_path = self.audit_file_path(id);
        AuditLogger::read_from_file(&audit_path)
    }

    async fn save(&self, record: &MachineRecord) -> Result<(), PersistenceError> {
        let _guard = self.lock.lock().await;

        // Check for name collision with any existing machine with a different ID
        let existing = self.list().await?;
        for other in existing {
            if other.config.id != record.config.id && other.config.name == record.config.name {
                return Err(PersistenceError::NameCollision(record.config.name.clone()));
            }
        }

        let dir = self.machine_dir(&record.config.id);
        fs::create_dir_all(&dir).map_err(PersistenceError::Io)?;

        let target_file = self.machine_file(&record.config.id);
        let serialized = serde_json::to_vec_pretty(record)
            .map_err(|e| PersistenceError::CorruptMetadata(target_file.clone(), e.to_string()))?;

        self.write_atomic(&target_file, &serialized)?;
        Ok(())
    }

    async fn get_by_id(&self, id: &MachineId) -> Result<Option<MachineRecord>, PersistenceError> {
        let _guard = self.lock.lock().await;
        let file = self.machine_file(id);
        if !file.exists() {
            return Ok(None);
        }

        let record = Self::read_record_file(&file)?;
        Ok(Some(record))
    }

    async fn get_by_name(
        &self,
        name: &MachineName,
    ) -> Result<Option<MachineRecord>, PersistenceError> {
        let all = self.list().await?;
        for rec in all {
            if rec.config.name == *name {
                return Ok(Some(rec));
            }
        }
        Ok(None)
    }

    async fn list(&self) -> Result<Vec<MachineRecord>, PersistenceError> {
        let mut results = Vec::new();
        if !self.machines_dir.exists() {
            return Ok(results);
        }

        let entries = match fs::read_dir(&self.machines_dir) {
            Ok(entries) => entries,
            Err(e) => return Err(PersistenceError::Io(e)),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let machine_file = path.join("machine.json");
                if machine_file.exists() {
                    match Self::read_record_file(&machine_file) {
                        Ok(rec) => results.push(rec),
                        Err(e) => {
                            tracing::warn!(
                                "Skipping corrupt or unreadable machine metadata at '{}': {}",
                                machine_file.display(),
                                e
                            );
                        }
                    }
                }
            }
        }

        // Sort consistently by name
        results.sort_by(|a, b| a.config.name.as_str().cmp(b.config.name.as_str()));
        Ok(results)
    }

    async fn delete(&self, id: &MachineId) -> Result<(), PersistenceError> {
        let _guard = self.lock.lock().await;
        let dir = self.machine_dir(id);
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(PersistenceError::Io)?;
        }
        // Note: Audit history at self.audit_file_path(id) and external locks at self.machine_lock_path(id)
        // are intentionally preserved across machine directory deletion.
        Ok(())
    }

    async fn update_state(
        &self,
        id: &MachineId,
        state: MachineState,
    ) -> Result<MachineRecord, PersistenceError> {
        let _guard = self.lock.lock().await;
        let file = self.machine_file(id);
        if !file.exists() {
            return Err(PersistenceError::NotFound(*id));
        }

        let mut record = Self::read_record_file(&file)?;
        record.state = state;
        record.config.updated_at = chrono::Utc::now();

        let serialized = serde_json::to_vec_pretty(&record)
            .map_err(|e| PersistenceError::CorruptMetadata(file.clone(), e.to_string()))?;

        self.write_atomic(&file, &serialized)?;
        Ok(record)
    }
}
