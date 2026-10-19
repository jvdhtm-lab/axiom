//! Write approvals bound to the exact request.
//!
//! An approval binds the database, role, SQL fingerprint, parameter hash,
//! affected-row limit, and action hash, and expires. Approvals are single-use:
//! a replayed id is refused. The ledger can be file-backed for crash recovery.

use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use axiom_hash::digest;

use crate::hex;
use crate::value::Params;
use crate::DbError;

/// The immutable parts of a write that an approval authorizes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteBinding {
    pub database: String,
    pub role: String,
    pub sql_fingerprint: String,
    pub params_hash: String,
    pub max_affected_rows: u64,
    pub action_hash: String,
}

impl WriteBinding {
    /// Build a binding for a write request.
    pub fn new(
        database: impl Into<String>,
        sql_fingerprint: impl Into<String>,
        params_hash: impl Into<String>,
        max_affected_rows: u64,
        action_hash: impl Into<String>,
    ) -> Self {
        WriteBinding {
            database: database.into(),
            role: "write".to_string(),
            sql_fingerprint: sql_fingerprint.into(),
            params_hash: params_hash.into(),
            max_affected_rows,
            action_hash: action_hash.into(),
        }
    }

    /// A stable hash of the binding, used as the runtime approval key.
    pub fn hash(&self) -> String {
        let mut canonical = Vec::new();
        for part in [
            self.database.as_str(),
            self.role.as_str(),
            self.sql_fingerprint.as_str(),
            self.params_hash.as_str(),
            self.action_hash.as_str(),
        ] {
            canonical.extend_from_slice(&(part.len() as u64).to_le_bytes());
            canonical.extend_from_slice(part.as_bytes());
        }
        canonical.extend_from_slice(&self.max_affected_rows.to_le_bytes());
        hex(&digest(&canonical))
    }
}

/// An approval issued by a human or a separate authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteApproval {
    pub id: String,
    pub binding: WriteBinding,
    pub expires_at: u64,
    pub approved_by: String,
}

impl WriteApproval {
    /// Issue an approval for a binding.
    pub fn issue(
        binding: WriteBinding,
        id: impl Into<String>,
        approved_by: impl Into<String>,
        expires_at: u64,
    ) -> Self {
        WriteApproval {
            id: id.into(),
            binding,
            expires_at,
            approved_by: approved_by.into(),
        }
    }

    /// Check that the approval matches the request and has not expired.
    pub fn verify(&self, binding: &WriteBinding, now: u64) -> Result<(), DbError> {
        if self.binding.database != binding.database {
            return Err(DbError::ApprovalMismatch("database"));
        }
        if self.binding.role != binding.role {
            return Err(DbError::ApprovalMismatch("role"));
        }
        if self.binding.sql_fingerprint != binding.sql_fingerprint {
            return Err(DbError::ApprovalMismatch("sql"));
        }
        if self.binding.params_hash != binding.params_hash {
            return Err(DbError::ApprovalMismatch("parameters"));
        }
        if self.binding.max_affected_rows != binding.max_affected_rows {
            return Err(DbError::ApprovalMismatch("affected-row limit"));
        }
        if self.binding.action_hash != binding.action_hash {
            return Err(DbError::ApprovalMismatch("action"));
        }
        if now >= self.expires_at {
            return Err(DbError::ApprovalExpired);
        }
        Ok(())
    }
}

/// A write request as presented to the database.
#[derive(Debug, Clone, PartialEq)]
pub struct WriteRequest {
    pub database: String,
    pub sql: String,
    pub params: Params,
    pub max_affected_rows: u64,
    pub idempotency_key: String,
    pub action_hash: String,
}

impl WriteRequest {
    /// Build a write request.
    pub fn new(
        database: impl Into<String>,
        sql: impl Into<String>,
        params: Params,
        max_affected_rows: u64,
        idempotency_key: impl Into<String>,
        action_hash: impl Into<String>,
    ) -> Self {
        WriteRequest {
            database: database.into(),
            sql: sql.into(),
            params,
            max_affected_rows,
            idempotency_key: idempotency_key.into(),
            action_hash: action_hash.into(),
        }
    }
}

/// Consumed approval ids.
#[derive(Debug, Default)]
pub struct ApprovalLedger {
    consumed: BTreeSet<String>,
    path: Option<PathBuf>,
}

impl ApprovalLedger {
    /// A process-local ledger.
    pub fn memory() -> Self {
        ApprovalLedger::default()
    }

    /// A file-backed ledger. Existing ids are loaded on open.
    pub fn file(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let consumed = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();
        Ok(ApprovalLedger {
            consumed,
            path: Some(path),
        })
    }

    /// Consume an approval id. Replays are refused.
    pub fn consume(&mut self, id: &str) -> Result<(), DbError> {
        if self.consumed.contains(id) {
            return Err(DbError::ApprovalReplayed);
        }
        if let Some(path) = &self.path {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|_| DbError::Config("approval ledger is not writable".to_string()))?;
            writeln!(file, "{id}")
                .map_err(|_| DbError::Config("approval ledger is not writable".to_string()))?;
            file.sync_data()
                .map_err(|_| DbError::Config("approval ledger is not writable".to_string()))?;
        }
        self.consumed.insert(id.to_string());
        Ok(())
    }

    /// Whether an id was consumed.
    pub fn is_consumed(&self, id: &str) -> bool {
        self.consumed.contains(id)
    }
}
