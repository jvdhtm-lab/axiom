mod common;

use std::collections::BTreeMap;
use std::time::Duration;

use axiom_database::sqlite::Connection;
use axiom_database::value::{Params, Value};
use axiom_database::{Actor, AuditLog, Database, DatabaseSpec, DbError};

use common::TempDir;

fn params(pairs: &[(&str, Value)]) -> Params {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect()
}

#[test]
fn reads_map_storage_classes_and_parameters() {
    let temp = TempDir::new("sqlite-read");
    let path = temp.db_path("app.sqlite");
    let connection = Connection::open(&path, false).expect("open");
    connection
        .script(
            "CREATE TABLE sample (id INTEGER, name TEXT, score REAL, data BLOB, note TEXT);
             INSERT INTO sample VALUES (1, 'alice', 1.5, x'0102', NULL)",
        )
        .expect("setup");

    let rows = connection
        .query(
            "SELECT id, name, score, data, note FROM sample WHERE name = :name",
            &params(&[("name", Value::Text("alice".to_string()))]),
            10,
            Duration::from_secs(2),
        )
        .expect("query");
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.get("id"), Some(&Value::Integer(1)));
    assert_eq!(row.get("name"), Some(&Value::Text("alice".to_string())));
    assert_eq!(row.get("score"), Some(&Value::Real(1.5)));
    assert_eq!(row.get("data"), Some(&Value::Blob(vec![1, 2])));
    assert_eq!(row.get("note"), Some(&Value::Null));
}

#[test]
fn read_only_connections_cannot_write() {
    let temp = TempDir::new("sqlite-ro");
    let path = temp.db_path("app.sqlite");
    let writer = Connection::open(&path, false).expect("writer");
    writer
        .script("CREATE TABLE users (id INTEGER)")
        .expect("ddl");
    drop(writer);

    let reader = Connection::open(&path, true).expect("reader");
    assert!(reader
        .query("SELECT 1", &Params::new(), 1, Duration::from_secs(1))
        .is_ok());
    assert!(matches!(
        reader.script("CREATE TABLE evil (id INTEGER)"),
        Err(DbError::RoleDenied { .. })
    ));
    assert!(matches!(
        reader.execute(
            "INSERT INTO users (id) VALUES (1)",
            &Params::new(),
            Duration::from_secs(1)
        ),
        Err(DbError::RoleDenied { .. })
    ));
}

#[test]
fn row_limits_and_query_one_cardinality_are_enforced() {
    let temp = TempDir::new("sqlite-limits");
    let spec = DatabaseSpec::new("app", temp.db_path("app.sqlite"))
        .with_max_rows(2)
        .without_write_approval();
    let database = Database::open(spec).expect("open");
    let setup = Connection::open(&temp.db_path("app.sqlite"), false).expect("setup");
    setup
        .script("CREATE TABLE users (id INTEGER); INSERT INTO users VALUES (1),(2),(3)")
        .expect("rows");
    let audit = AuditLog::memory();
    let actor = Actor::system("run");

    assert_eq!(
        database
            .query(&actor, "SELECT id FROM users", &Params::new(), &audit)
            .unwrap_err(),
        DbError::RowLimitExceeded { max_rows: 2 }
    );
    assert_eq!(
        database
            .query_one(
                &actor,
                "SELECT id FROM users WHERE id > 10",
                &Params::new(),
                &audit
            )
            .unwrap_err(),
        DbError::NoRows
    );
    assert_eq!(
        database
            .query_one(
                &actor,
                "SELECT id FROM users WHERE id <= 2",
                &Params::new(),
                &audit
            )
            .unwrap_err(),
        DbError::TooManyRows
    );
}

#[test]
fn timeouts_interrupt_runaway_queries() {
    let temp = TempDir::new("sqlite-timeout");
    let path = temp.db_path("app.sqlite");
    let connection = Connection::open(&path, false).expect("open");
    let result = connection.query(
        "WITH RECURSIVE cnt(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM cnt WHERE x < 100000000) \
         SELECT COUNT(*) FROM cnt",
        &Params::new(),
        1,
        Duration::from_millis(1),
    );
    assert_eq!(result.unwrap_err(), DbError::Timeout);
}

#[test]
fn role_audit_reports_least_privilege() {
    let temp = TempDir::new("sqlite-roles");
    let database = Database::open(
        DatabaseSpec::new("app", temp.db_path("app.sqlite")).without_write_approval(),
    )
    .expect("open");
    let audit = database.audit_roles().expect("audit");
    let read = audit
        .iter()
        .find(|role| role.role.as_str() == "read")
        .unwrap();
    assert!(read.can_read);
    assert!(!read.can_write);
    let write = audit
        .iter()
        .find(|role| role.role.as_str() == "write")
        .unwrap();
    assert!(write.can_write);
    assert!(!write.can_schema);
    let schema = audit
        .iter()
        .find(|role| role.role.as_str() == "schema")
        .unwrap();
    assert!(schema.can_schema);
}

#[test]
fn read_audit_omits_parameter_values() {
    let temp = TempDir::new("sqlite-audit");
    let database = Database::open(
        DatabaseSpec::new("app", temp.db_path("app.sqlite")).without_write_approval(),
    )
    .expect("open");
    let audit = AuditLog::memory();
    let mut secret = BTreeMap::new();
    secret.insert("name".to_string(), Value::Text("hunter2".to_string()));
    let setup = Connection::open(&temp.db_path("app.sqlite"), false).expect("setup");
    setup.script("CREATE TABLE users (name TEXT)").expect("ddl");
    // The parameter does not exist in this query, so the call is refused, but
    // the audit still records the fingerprint (not the value).
    let _ = database.query(
        &Actor::system("run"),
        "SELECT name FROM users",
        &secret,
        &audit,
    );
    let records = audit.records();
    assert_eq!(records.len(), 1);
    let rendered = format!("{records:?}");
    assert!(!rendered.contains("hunter2"));
    assert_eq!(records[0].outcome, axiom_database::AuditOutcome::Denied);
}
