//! Reviewed, versioned migrations.
//!
//! Migrations never run as ad-hoc SQL from agents. A plan is built from a
//! versioned directory, checked against the recorded history, validated against
//! a scratch database, and applied under an exclusive workspace lock. Files that
//! drop tables/columns or write without a `WHERE` clause are destructive and
//! require both an explicit flag and an approval bound to the plan checksum.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use axiom_hash::digest;

use crate::audit::{record, Actor, AuditLog, AuditOutcome};
use crate::hex;
use crate::roles::DatabaseSpec;
use crate::sqlite::Connection;
use crate::unix_now;
use crate::DbError;

/// A versioned migration file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationFile {
    pub version: u32,
    pub name: String,
    pub sql: String,
    pub checksum: String,
}

/// The current plan for a database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationPlan {
    pub files: Vec<MigrationFile>,
    pub pending: Vec<MigrationFile>,
    pub applied_version: u32,
}

impl MigrationPlan {
    /// A checksum over the full ordered plan.
    pub fn checksum(&self) -> String {
        let mut text = String::new();
        for file in &self.files {
            text.push_str(&format!(
                "{}:{}:{}\n",
                file.version, file.name, file.checksum
            ));
        }
        hex(&digest(text.as_bytes()))
    }
}

/// An approval for a destructive plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationApproval {
    pub plan_checksum: String,
    pub database: String,
    pub expires_at: u64,
    pub approved_by: String,
}

impl MigrationApproval {
    /// Issue an approval for a plan and database.
    pub fn issue(
        plan: &MigrationPlan,
        database: impl Into<String>,
        approved_by: impl Into<String>,
        expires_at: u64,
    ) -> Self {
        MigrationApproval {
            plan_checksum: plan.checksum(),
            database: database.into(),
            expires_at,
            approved_by: approved_by.into(),
        }
    }

    fn verify(&self, plan: &MigrationPlan, database: &str, now: u64) -> Result<(), DbError> {
        if self.plan_checksum != plan.checksum() || self.database != database {
            return Err(DbError::ApprovalMismatch("migration plan"));
        }
        if now >= self.expires_at {
            return Err(DbError::ApprovalExpired);
        }
        Ok(())
    }
}

/// A destructive change detected in a migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestructiveChange {
    pub version: u32,
    /// Redacted categories, e.g. `"drop table"`.
    pub kinds: Vec<String>,
}

/// An exclusive migration lock for a workspace.
#[derive(Debug)]
pub struct MigrationLock {
    path: PathBuf,
}

impl MigrationLock {
    /// Acquire `<workspace>/.axiom/migrations.lock`.
    pub fn acquire(workspace: impl AsRef<Path>) -> Result<Self, DbError> {
        let root = workspace.as_ref();
        let dir = root.join(".axiom");
        std::fs::create_dir_all(&dir)
            .map_err(|_| DbError::MigrationWorkspace("cannot create .axiom".into()))?;
        let path = dir.join("migrations.lock");
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => Ok(MigrationLock { path }),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(DbError::MigrationLocked)
            }
            Err(_) => Err(DbError::MigrationWorkspace("cannot create lock".into())),
        }
    }
}

impl Drop for MigrationLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// A dedicated, marked migration workspace.
#[derive(Debug, Clone)]
pub struct MigrationWorkspace {
    root: PathBuf,
}

impl MigrationWorkspace {
    /// Open a workspace that was explicitly marked for migrations.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, DbError> {
        let root = root
            .as_ref()
            .canonicalize()
            .map_err(|_| DbError::MigrationWorkspace("workspace does not exist".into()))?;
        if !root.join(".axiom").join("migration-workspace").is_file() {
            return Err(DbError::MigrationWorkspace(
                "missing .axiom/migration-workspace marker".into(),
            ));
        }
        Ok(MigrationWorkspace { root })
    }

    /// The workspace root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn contains(&self, path: &Path) -> bool {
        path.canonicalize()
            .map(|path| path.starts_with(&self.root))
            .unwrap_or(false)
    }
}

/// Options for applying a plan.
pub struct ApplyOptions<'a> {
    pub allow_destructive: bool,
    pub approval: Option<&'a MigrationApproval>,
    pub actor: Actor,
}

/// What an apply did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyReport {
    pub applied: Vec<u32>,
    pub scanned_scratch: bool,
}

/// Load migration files from a directory.
pub fn load_migrations(dir: &Path) -> Result<Vec<MigrationFile>, DbError> {
    let entries = std::fs::read_dir(dir)
        .map_err(|_| DbError::MigrationWorkspace("migrations directory is unreadable".into()))?;
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("sql") {
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| DbError::MigrationWorkspace("invalid migration file name".into()))?;
        let (version, name) = parse_name(file_name)?;
        let sql = std::fs::read_to_string(&path)
            .map_err(|_| DbError::MigrationWorkspace("migration file is unreadable".into()))?;
        files.push(MigrationFile {
            version,
            name,
            checksum: hex(&digest(sql.as_bytes())),
            sql,
        });
    }
    files.sort_by_key(|file| file.version);
    for pair in files.windows(2) {
        if pair[0].version == pair[1].version {
            return Err(DbError::MigrationWorkspace(format!(
                "duplicate migration version {}",
                pair[0].version
            )));
        }
    }
    Ok(files)
}

fn parse_name(file_name: &str) -> Result<(u32, String), DbError> {
    let stem = file_name
        .strip_suffix(".sql")
        .ok_or_else(|| DbError::MigrationWorkspace("invalid migration file name".into()))?;
    let (version, name) = stem
        .split_once('_')
        .ok_or_else(|| DbError::MigrationWorkspace("migration must be NNNN_name.sql".into()))?;
    let version: u32 = version
        .parse()
        .map_err(|_| DbError::MigrationWorkspace("invalid migration version".into()))?;
    if version == 0 {
        return Err(DbError::MigrationWorkspace(
            "migration versions start at 1".into(),
        ));
    }
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(DbError::MigrationWorkspace(
            "migration name must use letters, digits, and underscores".into(),
        ));
    }
    Ok((version, name.to_string()))
}

/// The applied migrations recorded in a database.
pub fn applied_migrations(connection: &Connection) -> Result<Vec<(u32, String)>, DbError> {
    ensure_history(connection)?;
    let rows = connection.query(
        "SELECT version, checksum FROM _axiom_schema_migrations ORDER BY version",
        &Default::default(),
        10_000,
        Duration::from_secs(5),
    )?;
    let mut out = Vec::new();
    for row in rows {
        let version = match row.get("version") {
            Some(crate::value::Value::Integer(version)) => (*version).max(0) as u32,
            _ => return Err(DbError::MigrationDiverged("history".into())),
        };
        let checksum = match row.get("checksum") {
            Some(crate::value::Value::Text(checksum)) => checksum.clone(),
            _ => return Err(DbError::MigrationDiverged("history".into())),
        };
        out.push((version, checksum));
    }
    Ok(out)
}

/// Build a plan from files and recorded history.
pub fn build_plan(
    files: Vec<MigrationFile>,
    applied: &[(u32, String)],
) -> Result<MigrationPlan, DbError> {
    for (version, checksum) in applied {
        let Some(file) = files.iter().find(|file| file.version == *version) else {
            return Err(DbError::MigrationDiverged(format!(
                "{version} (missing file)"
            )));
        };
        if &file.checksum != checksum {
            return Err(DbError::MigrationDiverged(version.to_string()));
        }
    }
    let applied_version = applied
        .iter()
        .map(|(version, _)| *version)
        .max()
        .unwrap_or(0);
    if let Some(file) = files.iter().find(|file| {
        file.version <= applied_version
            && !applied.iter().any(|(version, _)| version == &file.version)
    }) {
        return Err(DbError::MigrationDiverged(format!(
            "{} (out of order)",
            file.version
        )));
    }
    let pending = files
        .iter()
        .filter(|file| !applied.iter().any(|(version, _)| version == &file.version))
        .cloned()
        .collect();
    Ok(MigrationPlan {
        files,
        pending,
        applied_version,
    })
}

/// Load files and build a plan for a live database.
pub fn plan_migrations(dir: &Path, connection: &Connection) -> Result<MigrationPlan, DbError> {
    let files = load_migrations(dir)?;
    let applied = applied_migrations(connection)?;
    build_plan(files, &applied)
}

/// Detect destructive statements in pending migrations.
pub fn destructive_changes(plan: &MigrationPlan) -> Vec<DestructiveChange> {
    let mut out = Vec::new();
    for file in &plan.pending {
        let kinds = destructive_kinds(&file.sql);
        if !kinds.is_empty() {
            out.push(DestructiveChange {
                version: file.version,
                kinds,
            });
        }
    }
    out
}

fn destructive_kinds(sql: &str) -> Vec<String> {
    let mut out = Vec::new();
    for statement in sql.split(';') {
        let text = statement.trim();
        if text.is_empty() {
            continue;
        }
        let upper = text.to_ascii_uppercase();
        let compact = upper.split_whitespace().collect::<Vec<_>>().join(" ");
        if compact.starts_with("DROP TABLE") || compact.starts_with("DROP VIEW") {
            out.push("drop table".to_string());
        } else if compact.starts_with("ALTER TABLE") && compact.contains(" DROP ") {
            out.push("drop column".to_string());
        } else if compact.starts_with("TRUNCATE") {
            out.push("truncate".to_string());
        } else if compact.starts_with("DELETE FROM") && !compact.contains(" WHERE ") {
            out.push("delete without where".to_string());
        } else if compact.starts_with("UPDATE ") && !compact.contains(" WHERE ") {
            out.push("update without where".to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Validate and apply a plan.
pub fn apply_plan(
    spec: &DatabaseSpec,
    plan: &MigrationPlan,
    workspace: &MigrationWorkspace,
    options: &ApplyOptions<'_>,
    audit: &AuditLog,
) -> Result<ApplyReport, DbError> {
    let mut migrations_dir = spec.migrations_dir.clone().ok_or_else(|| {
        DbError::MigrationWorkspace("database has no migrations directory".into())
    })?;
    if !migrations_dir.is_absolute() {
        migrations_dir = workspace.root().join(migrations_dir);
    }
    if !workspace.contains(&migrations_dir) {
        return Err(DbError::MigrationWorkspace(
            "migrations directory is outside the workspace".into(),
        ));
    }
    let _lock = MigrationLock::acquire(workspace.root())?;

    let target = Connection::open(&spec.path, false)?;
    ensure_history(&target)?;
    let files = load_migrations(&migrations_dir)?;
    let applied = applied_migrations(&target)?;
    let current = build_plan(files, &applied)?;
    if current.checksum() != plan.checksum() {
        return Err(DbError::MigrationDiverged("plan changed".into()));
    }

    let destructive = destructive_changes(&current);
    if !destructive.is_empty() {
        if !options.allow_destructive {
            return Err(DbError::DestructiveRefused);
        }
        let Some(approval) = options.approval else {
            return Err(DbError::DestructiveRefused);
        };
        approval.verify(&current, &spec.name, unix_now())?;
    }

    apply_scratch(&current, spec.timeout)?;

    for file in &current.pending {
        target.begin_immediate()?;
        let result = (|| {
            target.script(&file.sql)?;
            let mut params = crate::value::Params::new();
            params.insert(
                "version".to_string(),
                crate::value::Value::Integer(file.version as i64),
            );
            params.insert(
                "name".to_string(),
                crate::value::Value::Text(file.name.clone()),
            );
            params.insert(
                "checksum".to_string(),
                crate::value::Value::Text(file.checksum.clone()),
            );
            params.insert(
                "applied_at".to_string(),
                crate::value::Value::Integer(unix_now() as i64),
            );
            params.insert(
                "applied_by".to_string(),
                crate::value::Value::Text(options.actor.run_id.clone()),
            );
            target.execute(
                "INSERT INTO _axiom_schema_migrations \
                 (version, name, checksum, applied_at, applied_by) \
                 VALUES (:version, :name, :checksum, :applied_at, :applied_by)",
                &params,
                spec.timeout,
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                target.commit()?;
                audit.record(record(
                    &options.actor,
                    "migrate",
                    &spec.name,
                    &file.checksum,
                    &spec.data_class,
                    file.version as u64,
                    AuditOutcome::Ok,
                ));
            }
            Err(error) => {
                let _ = target.rollback();
                audit.record(record(
                    &options.actor,
                    "migrate",
                    &spec.name,
                    &file.checksum,
                    &spec.data_class,
                    0,
                    AuditOutcome::Error,
                ));
                return Err(error);
            }
        }
    }

    Ok(ApplyReport {
        applied: current.pending.iter().map(|file| file.version).collect(),
        scanned_scratch: true,
    })
}

fn apply_scratch(plan: &MigrationPlan, timeout: Duration) -> Result<(), DbError> {
    let path = std::env::temp_dir().join(format!(
        "axiom-migration-scratch-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_file(&path);
    let connection = Connection::open(&path, false)?;
    ensure_history(&connection)?;
    let result = (|| {
        for file in &plan.files {
            connection.begin_immediate()?;
            if let Err(error) = connection.script(&file.sql) {
                let _ = connection.rollback();
                return Err(error);
            }
            let params = {
                let mut params = crate::value::Params::new();
                params.insert(
                    "version".to_string(),
                    crate::value::Value::Integer(file.version as i64),
                );
                params.insert(
                    "name".to_string(),
                    crate::value::Value::Text(file.name.clone()),
                );
                params.insert(
                    "checksum".to_string(),
                    crate::value::Value::Text(file.checksum.clone()),
                );
                params.insert(
                    "applied_at".to_string(),
                    crate::value::Value::Integer(unix_now() as i64),
                );
                params.insert(
                    "applied_by".to_string(),
                    crate::value::Value::Text("scratch".to_string()),
                );
                params
            };
            if let Err(error) = connection.execute(
                "INSERT INTO _axiom_schema_migrations \
                 (version, name, checksum, applied_at, applied_by) \
                 VALUES (:version, :name, :checksum, :applied_at, :applied_by)",
                &params,
                timeout,
            ) {
                let _ = connection.rollback();
                return Err(error);
            }
            connection.commit()?;
        }
        Ok(())
    })();
    drop(connection);
    let _ = std::fs::remove_file(&path);
    result
}

fn ensure_history(connection: &Connection) -> Result<(), DbError> {
    connection.script(
        "CREATE TABLE IF NOT EXISTS _axiom_schema_migrations (\
         version INTEGER PRIMARY KEY, name TEXT NOT NULL, checksum TEXT NOT NULL, \
         applied_at INTEGER NOT NULL, applied_by TEXT NOT NULL)",
    )
}

/// Create a marker directory for a migration workspace. Used by trusted setup.
pub fn mark_workspace(root: &Path) -> std::io::Result<()> {
    let dir = root.join(".axiom");
    std::fs::create_dir_all(&dir)?;
    File::create(dir.join("migration-workspace"))?.write_all(b"axiom\n")
}
