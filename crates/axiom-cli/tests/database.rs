//! Config → runtime database spec bridging.

use std::path::Path;

use axiom_cli::database;
use axiom_config::{DatabaseConfig, Secret};

fn sqlite_config(path: Option<&str>, migrations: Option<&str>) -> DatabaseConfig {
    DatabaseConfig {
        driver: "sqlite".to_string(),
        path: path.map(str::to_string),
        url: None,
        data_class: Some("internal".to_string()),
        allowed_tables: vec!["users".to_string(), "orders".to_string()],
        max_rows: Some(42),
        timeout_seconds: Some(3),
        migrations: migrations.map(str::to_string),
        destructive_migrations: None,
        allow_private_network: None,
        require_write_approval: None,
    }
}

fn remote_config(driver: &str, url: &str) -> DatabaseConfig {
    DatabaseConfig {
        driver: driver.to_string(),
        path: None,
        url: Some(Secret::new(url)),
        data_class: None,
        allowed_tables: Vec::new(),
        max_rows: None,
        timeout_seconds: None,
        migrations: None,
        destructive_migrations: None,
        allow_private_network: None,
        require_write_approval: None,
    }
}

#[test]
fn builds_a_bounded_sqlite_spec_under_the_project_root() {
    let config = sqlite_config(Some("data/app.sqlite"), Some("migrations"));
    let spec = database::spec_from_config(Path::new("E:/proj"), "app", &config).unwrap();
    assert_eq!(spec.name, "app");
    assert!(spec.path.ends_with("data/app.sqlite"));
    assert_eq!(spec.allowed_tables, vec!["users", "orders"]);
    assert_eq!(spec.max_rows, 42);
    assert_eq!(spec.timeout.as_secs(), 3);
    assert!(spec.require_write_approval);
    assert!(spec
        .migrations_dir
        .as_deref()
        .unwrap()
        .ends_with("migrations"));
}

#[test]
fn unsafe_paths_and_unregistered_drivers_are_refused() {
    let absolute = sqlite_config(Some("C:/etc/app.sqlite"), None);
    assert!(database::spec_from_config(Path::new("proj"), "app", &absolute).is_err());
    let absolute_unix = sqlite_config(Some("/etc/app.sqlite"), None);
    assert!(database::spec_from_config(Path::new("proj"), "app", &absolute_unix).is_err());
    let escape = sqlite_config(Some("../outside.sqlite"), None);
    assert!(database::spec_from_config(Path::new("proj"), "app", &escape).is_err());
    let missing = sqlite_config(None, None);
    assert!(database::spec_from_config(Path::new("proj"), "app", &missing).is_err());
    let bad_migrations = sqlite_config(Some("data/app.sqlite"), Some("/etc/migrations"));
    assert!(database::spec_from_config(Path::new("proj"), "app", &bad_migrations).is_err());
    let windows_migrations = sqlite_config(Some("data/app.sqlite"), Some("C:/migrations"));
    assert!(database::spec_from_config(Path::new("proj"), "app", &windows_migrations).is_err());

    let remote = remote_config("postgres", "postgres://u:p@db.example.com:5432/app");
    assert!(
        database::spec_from_config(Path::new("proj"), "reporting", &remote)
            .unwrap_err()
            .contains("not registered")
    );
}

#[test]
fn descriptions_and_secrets_are_redacted() {
    let remote = remote_config(
        "postgres",
        "postgres://alice:supersecret@db.example.com:5432/app",
    );
    let summary = database::describe("reporting", &remote);
    assert!(!summary.contains("supersecret"), "{summary}");
    assert!(!summary.contains("db.example.com"), "{summary}");
    assert!(summary.contains("postgres (url redacted)"), "{summary}");

    let sqlite = database::describe("app", &sqlite_config(Some("data/app.sqlite"), None));
    assert!(sqlite.contains("sqlite data/app.sqlite"), "{sqlite}");

    assert_eq!(database::redact(&Secret::new("x")), "(redacted)");
    assert_eq!(database::redact(&Secret::new("")), "(empty)");
}
