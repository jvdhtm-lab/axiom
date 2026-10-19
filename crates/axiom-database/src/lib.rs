//! Database subsystem: parameterized SQL, least-privilege roles, write
//! approvals, auditing, and reviewed migrations.
//!
//! Axiom has no `query`/`migration` declarations. Axiom code calls `db.*`
//! library APIs; this crate supplies the trusted runtime behind them. SQL text
//! is validated as a single statement with named placeholders, then executed
//! through a role connection. SQLite is the offline engine; PostgreSQL target
//! policy is enforced even where no driver is linked.

pub mod approval;
pub mod audit;
pub mod migrations;
pub mod postgres;
pub mod roles;
pub mod sql;
pub mod sqlite;
pub mod value;

pub use approval::{ApprovalLedger, WriteApproval, WriteBinding, WriteRequest};
pub use audit::{Actor, AuditLog, AuditOutcome, AuditRecord};
pub use migrations::{
    apply_plan, plan_migrations, DestructiveChange, MigrationApproval, MigrationFile,
    MigrationLock, MigrationPlan, MigrationWorkspace,
};
pub use postgres::{PostgresPolicy, PostgresTarget};
pub use roles::{Database, DatabaseRole, DatabaseSpec, RoleAudit, WriteOutcome};
pub use sql::{SqlClass, SqlStatement};
pub use value::{Params, Row, Value};

/// A database error. Messages are deliberately generic so driver text, SQL,
/// and bound values never reach agent-visible diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbError {
    /// A configuration reference is missing or invalid.
    Config(String),
    /// SQL is empty or does not start with a recognized statement class.
    InvalidSql,
    /// More than one statement was supplied.
    MultipleStatements,
    /// The statement class does not match the API (e.g. a write through `query`).
    WrongClass {
        /// The class the API requires.
        expected: &'static str,
        /// The class that was supplied.
        actual: &'static str,
    },
    /// A placeholder or parameter set does not match.
    PlaceholderMismatch(String),
    /// A table is outside the configured table scope.
    UnknownTable(String),
    /// A statement class that application code may never use.
    Forbidden(&'static str),
    /// The role is not allowed to perform the operation.
    RoleDenied {
        /// The role used.
        role: &'static str,
        /// The operation class.
        operation: &'static str,
    },
    /// A single-row query returned no rows.
    NoRows,
    /// A single-row query returned more than one row.
    TooManyRows,
    /// The query returned more rows than the configured limit.
    RowLimitExceeded {
        /// The configured maximum.
        max_rows: u64,
    },
    /// A write would affect more rows than allowed.
    AffectedRowLimitExceeded {
        /// The configured maximum.
        max_rows: u64,
    },
    /// The statement exceeded its time budget.
    Timeout,
    /// The database is locked by another writer or migration.
    Busy,
    /// A required write approval is missing.
    ApprovalRequired,
    /// An approval does not match the request.
    ApprovalMismatch(&'static str),
    /// An approval has expired.
    ApprovalExpired,
    /// An approval was already consumed.
    ApprovalReplayed,
    /// An idempotency key was reused with a different request.
    IdempotencyConflict,
    /// A migration plan diverged from the recorded history.
    MigrationDiverged(String),
    /// A destructive migration needs an explicit flag and approval.
    DestructiveRefused,
    /// A migration workspace or file is invalid.
    MigrationWorkspace(String),
    /// Another migration is already running.
    MigrationLocked,
    /// The engine rejected the operation. Only the numeric code is retained.
    Engine {
        /// SQLite extended result code (0 when unavailable).
        code: i32,
    },
    /// The required engine or driver is unavailable.
    Unavailable(&'static str),
}

impl DbError {
    /// A stable, redacted message for diagnostics and traces.
    pub fn message(&self) -> String {
        match self {
            DbError::Config(detail) => format!("invalid database configuration: {detail}"),
            DbError::InvalidSql => "SQL must be one recognized statement".to_string(),
            DbError::MultipleStatements => "multiple SQL statements are not allowed".to_string(),
            DbError::WrongClass { expected, actual } => {
                format!("expected a {expected} statement, found {actual}")
            }
            DbError::PlaceholderMismatch(detail) => format!("SQL parameter mismatch: {detail}"),
            DbError::UnknownTable(table) => format!("table `{table}` is outside the allowed scope"),
            DbError::Forbidden(what) => format!("`{what}` is not allowed in application SQL"),
            DbError::RoleDenied { role, operation } => {
                format!("role `{role}` may not perform `{operation}`")
            }
            DbError::NoRows => "query returned no rows".to_string(),
            DbError::TooManyRows => "query returned more than one row".to_string(),
            DbError::RowLimitExceeded { max_rows } => {
                format!("query exceeded the row limit of {max_rows}")
            }
            DbError::AffectedRowLimitExceeded { max_rows } => {
                format!("write affected more than {max_rows} rows")
            }
            DbError::Timeout => "database operation timed out".to_string(),
            DbError::Busy => "database is busy".to_string(),
            DbError::ApprovalRequired => "a write approval is required".to_string(),
            DbError::ApprovalMismatch(field) => {
                format!("write approval does not match the request ({field})")
            }
            DbError::ApprovalExpired => "write approval has expired".to_string(),
            DbError::ApprovalReplayed => "write approval was already used".to_string(),
            DbError::IdempotencyConflict => {
                "idempotency key was reused with different input".to_string()
            }
            DbError::MigrationDiverged(version) => {
                format!("migration {version} does not match the applied checksum")
            }
            DbError::DestructiveRefused => {
                "destructive migration requires an explicit flag and approval".to_string()
            }
            DbError::MigrationWorkspace(detail) => {
                format!("invalid migration workspace: {detail}")
            }
            DbError::MigrationLocked => "another migration is running".to_string(),
            DbError::Engine { code } => format!("database engine error (code {code})"),
            DbError::Unavailable(what) => format!("{what} is unavailable on this platform"),
        }
    }
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for DbError {}

/// Lowercase hexadecimal encoding shared by fingerprints and checksums.
pub(crate) fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from_digit((byte >> 4) as u32, 16).unwrap_or('0'));
        out.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap_or('0'));
    }
    out
}

/// A monotonic wall-clock helper used by approvals and migration plans.
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
