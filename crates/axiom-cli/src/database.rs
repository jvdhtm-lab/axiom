//! Bridge from the `axiom.yaml` `databases` section to the enforcing runtime.
//!
//! Configuration is descriptive; `axiom-database` is the boundary that opens
//! least-privilege connections, bounds statements, and audits access. This
//! module builds the runtime [`DatabaseSpec`] (SQLite) or [`PostgresPolicy`]
//! (PostgreSQL target policy) from a parsed configuration, and never logs a
//! connection URL.

use std::path::{Path, PathBuf};
use std::time::Duration;

use axiom_config::{DatabaseConfig, Secret};
use axiom_database::migrations::{plan_migrations, MigrationPlan};
use axiom_database::postgres::{PostgresPolicy, PostgresTarget};
use axiom_database::sqlite::Connection;
use axiom_database::{DatabaseSpec, DbError};

/// Build the SQLite runtime spec from a configured database.
///
/// `base` is the directory the configured relative `path` resolves against
/// (the project root). The configuration layer already rejects absolute paths
/// and `..` escapes, but the runtime re-validates rather than trusting it.
pub fn spec_from_config(
    base: &Path,
    name: &str,
    config: &DatabaseConfig,
) -> Result<DatabaseSpec, String> {
    if config.engine != "sqlite" {
        return Err(format!(
            "database `{name}` uses engine `{}`; only `sqlite` opens a local spec",
            config.engine
        ));
    }
    let Some(relative) = config.path.as_deref() else {
        return Err(format!("database `{name}` is missing `path`"));
    };
    let relative_path = Path::new(relative);
    if relative_path.has_root()
        || relative_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!(
            "database `{name}` path `{relative}` is not a safe relative path"
        ));
    }

    let mut spec = DatabaseSpec::new(name, base.join(relative_path));
    spec.data_class = config
        .data_class
        .clone()
        .unwrap_or_else(|| "internal".to_string());
    spec.allowed_tables = config.allowed_tables.clone();
    spec.max_rows = config.max_rows.unwrap_or(100);
    spec.timeout = Duration::from_secs(config.timeout_seconds.unwrap_or(5));
    spec.require_write_approval = config.require_write_approval.unwrap_or(true);
    if let Some(migrations) = config.migrations.as_deref() {
        let migration_path = Path::new(migrations);
        if migration_path.has_root()
            || migration_path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(format!(
                "database `{name}` migrations directory `{migrations}` is not a safe relative path"
            ));
        }
        spec.migrations_dir = Some(base.join(migration_path));
    }
    Ok(spec)
}

/// Build the PostgreSQL target policy. This validates the release target only;
/// Phase 7 ships no PostgreSQL wire adapter.
pub fn postgres_policy(name: &str, config: &DatabaseConfig) -> Result<PostgresTarget, String> {
    if config.engine != "postgres" {
        return Err(format!(
            "database `{name}` does not use the `postgres` engine"
        ));
    }
    let Some(url) = config.url.as_ref() else {
        return Err(format!("database `{name}` is missing `url`"));
    };
    let policy = PostgresPolicy {
        allow_private_network: config.allow_private_network.unwrap_or(false),
    };
    policy
        .parse(url.expose())
        .map_err(|error| format!("database `{name}`: {}", error.message()))
}

/// Plan the reviewed migrations a configured SQLite database would apply.
pub fn plan(base: &Path, name: &str, config: &DatabaseConfig) -> Result<MigrationPlan, DbError> {
    let spec = spec_from_config(base, name, config).map_err(DbError::Config)?;
    let Some(dir) = spec.migrations_dir.clone() else {
        return Err(DbError::Config(format!(
            "database `{name}` has no `migrations` directory configured"
        )));
    };
    if let Some(parent) = spec.path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|_| DbError::Config("database directory is not writable".into()))?;
        }
    }
    let connection = Connection::open(&spec.path, false)?;
    plan_migrations(&dir, &connection)
}

/// A redacted one-line summary of a database target.
pub fn describe(name: &str, config: &DatabaseConfig) -> String {
    match config.engine.as_str() {
        "postgres" => {
            let target = config.url.as_ref().and_then(|url| {
                let policy = PostgresPolicy {
                    allow_private_network: config.allow_private_network.unwrap_or(false),
                };
                policy.parse(url.expose()).ok()
            });
            let target = target
                .map(|target| target.sanitized())
                .unwrap_or_else(|| "(invalid url)".to_string());
            format!("{name}: postgres {target} (release CI only, no local driver)")
        }
        engine => {
            let path = config.path.as_deref().unwrap_or("(missing)");
            format!(
                "{name}: {engine} {path} (tables {}, max_rows {}, timeout {}s)",
                config.allowed_tables.len(),
                config.max_rows.unwrap_or(100),
                config.timeout_seconds.unwrap_or(5)
            )
        }
    }
}

/// A redacted summary of a secret URL, for diagnostics.
pub fn redact(url: &Secret) -> String {
    if url.is_empty() {
        "(empty)".to_string()
    } else {
        "(redacted)".to_string()
    }
}

/// Resolve the project root a configuration's relative paths are based on.
pub fn project_root(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}
