use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::domain::AuditRecord;
use crate::error::PersistenceError;

/// Persistence adapter for append-only JSON-Lines audit logs (`audit.jsonl`).
pub struct AuditLogger;

impl AuditLogger {
    /// Appends an audit event to the machine's `audit.jsonl` file.
    /// Issues an immediate `sync_all` to guarantee durability on disk.
    pub fn append(machine_dir: &Path, record: &AuditRecord) -> Result<(), PersistenceError> {
        fs::create_dir_all(machine_dir).map_err(PersistenceError::Io)?;
        let audit_path = machine_dir.join("audit.jsonl");

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&audit_path)
            .map_err(PersistenceError::Io)?;

        let mut json = serde_json::to_string(record)
            .map_err(|e| PersistenceError::CorruptMetadata(audit_path.clone(), e.to_string()))?;
        json.push('\n');

        file.write_all(json.as_bytes())
            .map_err(PersistenceError::Io)?;
        file.sync_all().map_err(PersistenceError::Io)?;

        Ok(())
    }

    /// Reads and deserializes all audit events from a machine's `audit.jsonl` file.
    pub fn read_all(machine_dir: &Path) -> Result<Vec<AuditRecord>, PersistenceError> {
        let audit_path = machine_dir.join("audit.jsonl");
        if !audit_path.exists() {
            return Ok(Vec::new());
        }

        let file = fs::File::open(&audit_path).map_err(PersistenceError::Io)?;
        let reader = BufReader::new(file);
        let mut records = Vec::new();

        for (idx, line_res) in reader.lines().enumerate() {
            let line = line_res.map_err(PersistenceError::Io)?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            match serde_json::from_str::<AuditRecord>(trimmed) {
                Ok(rec) => records.push(rec),
                Err(e) => {
                    return Err(PersistenceError::CorruptMetadata(
                        audit_path.clone(),
                        format!("Line {}: {e}", idx + 1),
                    ));
                }
            }
        }

        Ok(records)
    }

    /// Returns the path to the audit log file for a machine directory.
    pub fn audit_path(machine_dir: &Path) -> PathBuf {
        machine_dir.join("audit.jsonl")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AuditAction, AuditResult, MachineId, MachineName, MachineState};
    use tempfile::tempdir;

    #[test]
    fn test_audit_logger_append_and_read() {
        let temp = tempdir().expect("tempdir");
        let machine_dir = temp.path().join("test-machine");

        let id = MachineId::new();
        let name = MachineName::new("audit-test").unwrap();

        let event1 = AuditRecord::completed(
            id,
            name.clone(),
            AuditAction::Create,
            None,
            Some(MachineState::Created),
            "test",
        );
        AuditLogger::append(&machine_dir, &event1).expect("append 1");

        let event2 = AuditRecord::completed(
            id,
            name.clone(),
            AuditAction::Start,
            Some(MachineState::Created),
            Some(MachineState::Running),
            "test",
        );
        AuditLogger::append(&machine_dir, &event2).expect("append 2");

        let event3 = AuditRecord::failed(
            id,
            name,
            AuditAction::Stop,
            Some(MachineState::Running),
            Some(MachineState::Failed),
            "test",
            "ACPI shutdown timeout",
        );
        AuditLogger::append(&machine_dir, &event3).expect("append 3");

        let records = AuditLogger::read_all(&machine_dir).expect("read records");
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].action, AuditAction::Create);
        assert_eq!(records[0].result, AuditResult::Success);
        assert_eq!(records[1].action, AuditAction::Start);
        assert_eq!(records[2].action, AuditAction::Stop);
        assert_eq!(records[2].result, AuditResult::Failure);
        assert_eq!(
            records[2].error_message.as_deref(),
            Some("ACPI shutdown timeout")
        );
    }
}
