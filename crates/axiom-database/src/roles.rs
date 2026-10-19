//! Least-privilege database roles and typed calls.
//!
//! SQLite expresses the read boundary with a read-only connection; the other
//! role boundaries (write/schema/provision) are enforced by this crate's API
//! and by SQL statement classification. A read-only connection is the actual
//! security boundary for reads.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::approval::{ApprovalLedger, WriteApproval, WriteBinding, WriteRequest};
use crate::audit::{record, Actor, AuditLog, AuditOutcome};
use crate::sql::{self, SqlClass};
use crate::sqlite::Connection;
use crate::value::{params_hash, Params, Row, Value};
use crate::DbError;

/// A configured database.
#[derive(Debug, Clone)]
pub struct DatabaseSpec {
    pub name: String,
    pub path: PathBuf,
    pub data_class: String,
    pub allowed_tables: Vec<String>,
    pub max_rows: u64,
    pub timeout: Duration,
    pub require_write_approval: bool,
    pub migrations_dir: Option<PathBuf>,
}

impl DatabaseSpec {
    /// A SQLite database with conservative defaults.
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        DatabaseSpec {
            name: name.into(),
            path: path.into(),
            data_class: "internal".to_string(),
            allowed_tables: Vec::new(),
            max_rows: 1_000,
            timeout: Duration::from_secs(5),
            require_write_approval: true,
            migrations_dir: None,
        }
    }

    /// Set the allowed table scope.
    pub fn with_tables(mut self, tables: &[&str]) -> Self {
        self.allowed_tables = tables.iter().map(|table| table.to_string()).collect();
        self
    }

    /// Set the row limit.
    pub fn with_max_rows(mut self, max_rows: u64) -> Self {
        self.max_rows = max_rows;
        self
    }

    /// Set the statement timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Allow writes without an approval (tests and trusted tooling only).
    pub fn without_write_approval(mut self) -> Self {
        self.require_write_approval = false;
        self
    }

    /// Set the reviewed migration directory.
    pub fn with_migrations(mut self, dir: impl Into<PathBuf>) -> Self {
        self.migrations_dir = Some(dir.into());
        self
    }
}

/// The database roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseRole {
    Read,
    Write,
    Schema,
    Provision,
}

impl DatabaseRole {
    /// Stable role name.
    pub fn as_str(self) -> &'static str {
        match self {
            DatabaseRole::Read => "read",
            DatabaseRole::Write => "write",
            DatabaseRole::Schema => "schema",
            DatabaseRole::Provision => "provision",
        }
    }
}

/// The result of a write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteOutcome {
    pub affected_rows: u64,
    /// True when an idempotency key returned a previously recorded outcome.
    pub replayed: bool,
}

/// The result of probing configured role privileges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleAudit {
    pub role: DatabaseRole,
    pub can_read: bool,
    pub can_write: bool,
    pub can_schema: bool,
}

/// A database handle with separate read, write, and schema connections.
pub struct Database {
    spec: DatabaseSpec,
    read: Connection,
    write: Connection,
    schema: Connection,
}

impl Database {
    /// Open the database, creating the file (and the runtime metadata table)
    /// when absent.
    pub fn open(spec: DatabaseSpec) -> Result<Self, DbError> {
        if let Some(parent) = spec.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|_| DbError::Config("database directory is not writable".into()))?;
            }
        }
        let write = Connection::open(&spec.path, false)?;
        write.script(
            "CREATE TABLE IF NOT EXISTS _axiom_idempotency (\
             key TEXT PRIMARY KEY, fingerprint TEXT NOT NULL, \
             affected INTEGER NOT NULL, created_at INTEGER NOT NULL)",
        )?;
        let read = Connection::open(&spec.path, true)?;
        let schema = Connection::open(&spec.path, false)?;
        Ok(Database {
            spec,
            read,
            write,
            schema,
        })
    }

    /// The configured spec.
    pub fn spec(&self) -> &DatabaseSpec {
        &self.spec
    }

    /// Run a bounded read query.
    pub fn query(
        &self,
        actor: &Actor,
        sql_text: &str,
        params: &Params,
        audit: &AuditLog,
    ) -> Result<Vec<Row>, DbError> {
        let statement =
            match sql::validate(sql_text, SqlClass::Read, params, &self.spec.allowed_tables) {
                Ok(statement) => statement,
                Err(error) => {
                    audit.record(record(
                        actor,
                        "read",
                        &self.spec.name,
                        "",
                        &self.spec.data_class,
                        0,
                        outcome_for(&error),
                    ));
                    return Err(error);
                }
            };
        match self
            .read
            .query(sql_text, params, self.spec.max_rows, self.spec.timeout)
        {
            Ok(rows) => {
                audit.record(record(
                    actor,
                    "read",
                    &self.spec.name,
                    &statement.fingerprint,
                    &self.spec.data_class,
                    rows.len() as u64,
                    AuditOutcome::Ok,
                ));
                Ok(rows)
            }
            Err(error) => {
                audit.record(record(
                    actor,
                    "read",
                    &self.spec.name,
                    &statement.fingerprint,
                    &self.spec.data_class,
                    0,
                    outcome_for(&error),
                ));
                Err(error)
            }
        }
    }

    /// Run a read query that must return exactly one row.
    pub fn query_one(
        &self,
        actor: &Actor,
        sql_text: &str,
        params: &Params,
        audit: &AuditLog,
    ) -> Result<Row, DbError> {
        let rows = self.query(actor, sql_text, params, audit)?;
        match rows.len() {
            0 => Err(DbError::NoRows),
            1 => Ok(rows.into_iter().next().expect("one row")),
            _ => Err(DbError::TooManyRows),
        }
    }

    /// Execute a write under approval and idempotency rules.
    pub fn execute(
        &self,
        actor: &Actor,
        request: &WriteRequest,
        approval: Option<&WriteApproval>,
        ledger: &mut ApprovalLedger,
        audit: &AuditLog,
    ) -> Result<WriteOutcome, DbError> {
        if request.database != self.spec.name {
            return Err(DbError::Config(format!(
                "request targets `{}`, handle is `{}`",
                request.database, self.spec.name
            )));
        }
        let statement = match sql::validate(
            &request.sql,
            SqlClass::Write,
            &request.params,
            &self.spec.allowed_tables,
        ) {
            Ok(statement) => statement,
            Err(error) => {
                audit.record(record(
                    actor,
                    "write",
                    &self.spec.name,
                    "",
                    &self.spec.data_class,
                    0,
                    outcome_for(&error),
                ));
                return Err(error);
            }
        };
        let params_hash = params_hash(&request.params);
        let binding = WriteBinding::new(
            &self.spec.name,
            &statement.fingerprint,
            &params_hash,
            request.max_affected_rows,
            &request.action_hash,
        );

        if let Some(replay) = self.lookup_idempotency(&request.idempotency_key, &binding)? {
            audit.record(record(
                actor,
                "write",
                &self.spec.name,
                &statement.fingerprint,
                &self.spec.data_class,
                replay,
                AuditOutcome::Ok,
            ));
            return Ok(WriteOutcome {
                affected_rows: replay,
                replayed: true,
            });
        }

        if self.spec.require_write_approval || approval.is_some() {
            let Some(approval) = approval else {
                audit.record(record(
                    actor,
                    "write",
                    &self.spec.name,
                    &statement.fingerprint,
                    &self.spec.data_class,
                    0,
                    AuditOutcome::Denied,
                ));
                return Err(DbError::ApprovalRequired);
            };
            if let Err(error) = approval.verify(&binding, crate::unix_now()) {
                audit.record(record(
                    actor,
                    "write",
                    &self.spec.name,
                    &statement.fingerprint,
                    &self.spec.data_class,
                    0,
                    outcome_for(&error),
                ));
                return Err(error);
            }
            ledger.consume(&approval.id)?;
        }

        self.write.begin_immediate()?;
        let result = (|| {
            let affected = self
                .write
                .execute(&request.sql, &request.params, self.spec.timeout)?;
            if affected as u64 > request.max_affected_rows {
                return Err(DbError::AffectedRowLimitExceeded {
                    max_rows: request.max_affected_rows,
                });
            }
            let mut params = BTreeMap::new();
            params.insert(
                "key".to_string(),
                Value::Text(request.idempotency_key.clone()),
            );
            params.insert("fingerprint".to_string(), Value::Text(binding.hash()));
            params.insert("affected".to_string(), Value::Integer(affected as i64));
            params.insert(
                "created_at".to_string(),
                Value::Integer(crate::unix_now() as i64),
            );
            self.write.execute(
                "INSERT INTO _axiom_idempotency (key, fingerprint, affected, created_at) \
                 VALUES (:key, :fingerprint, :affected, :created_at)",
                &params,
                self.spec.timeout,
            )?;
            Ok(affected)
        })();

        match result {
            Ok(affected) => {
                self.write.commit()?;
                audit.record(record(
                    actor,
                    "write",
                    &self.spec.name,
                    &statement.fingerprint,
                    &self.spec.data_class,
                    affected as u64,
                    AuditOutcome::Ok,
                ));
                Ok(WriteOutcome {
                    affected_rows: affected as u64,
                    replayed: false,
                })
            }
            Err(error) => {
                let _ = self.write.rollback();
                audit.record(record(
                    actor,
                    "write",
                    &self.spec.name,
                    &statement.fingerprint,
                    &self.spec.data_class,
                    0,
                    outcome_for(&error),
                ));
                Err(error)
            }
        }
    }

    /// Probe the read/write/schema boundaries.
    pub fn audit_roles(&self) -> Result<Vec<RoleAudit>, DbError> {
        let read = self
            .read
            .query("SELECT 1", &Params::new(), 1, self.spec.timeout)
            .is_ok();
        let write = probe_schema(&self.write).unwrap_or(false);
        let schema = probe_schema(&self.schema).unwrap_or(false);
        Ok(vec![
            RoleAudit {
                role: DatabaseRole::Read,
                can_read: read,
                can_write: false,
                can_schema: false,
            },
            RoleAudit {
                role: DatabaseRole::Write,
                can_read: true,
                can_write: write,
                can_schema: false,
            },
            RoleAudit {
                role: DatabaseRole::Schema,
                can_read: true,
                can_write: write,
                can_schema: schema,
            },
            RoleAudit {
                role: DatabaseRole::Provision,
                can_read: false,
                can_write: false,
                can_schema: false,
            },
        ])
    }

    fn lookup_idempotency(
        &self,
        key: &str,
        binding: &WriteBinding,
    ) -> Result<Option<u64>, DbError> {
        let mut params = Params::new();
        params.insert("key".to_string(), Value::Text(key.to_string()));
        let rows = self.write.query(
            "SELECT fingerprint, affected FROM _axiom_idempotency WHERE key = :key LIMIT 1",
            &params,
            1,
            self.spec.timeout,
        )?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        let fingerprint = match row.get("fingerprint") {
            Some(Value::Text(text)) => text,
            _ => return Err(DbError::Engine { code: 0 }),
        };
        if fingerprint != &binding.hash() {
            return Err(DbError::IdempotencyConflict);
        }
        match row.get("affected") {
            Some(Value::Integer(affected)) => Ok(Some((*affected).max(0) as u64)),
            _ => Err(DbError::Engine { code: 0 }),
        }
    }
}

fn probe_schema(connection: &Connection) -> Result<bool, DbError> {
    connection.begin_immediate()?;
    let result = connection.script(
        "CREATE TABLE IF NOT EXISTS _axiom_role_probe (id INTEGER); DROP TABLE _axiom_role_probe",
    );
    let _ = connection.rollback();
    Ok(result.is_ok())
}

fn outcome_for(error: &DbError) -> AuditOutcome {
    match error {
        DbError::RoleDenied { .. }
        | DbError::UnknownTable(_)
        | DbError::Forbidden(_)
        | DbError::ApprovalRequired
        | DbError::ApprovalMismatch(_)
        | DbError::ApprovalExpired
        | DbError::ApprovalReplayed
        | DbError::DestructiveRefused
        | DbError::MigrationLocked
        | DbError::WrongClass { .. }
        | DbError::MultipleStatements
        | DbError::InvalidSql
        | DbError::PlaceholderMismatch(_)
        | DbError::IdempotencyConflict => AuditOutcome::Denied,
        _ => AuditOutcome::Error,
    }
}
