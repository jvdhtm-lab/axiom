//! Database host integration: typed `db.*` calls from Axiom code.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use axiom_database::sql::{self, SqlClass};
use axiom_database::sqlite::Connection;
use axiom_database::value::{params_hash, Params, Value as DbValue};
use axiom_database::{Actor, AuditLog, DatabaseSpec, SqlStatement, WriteApproval, WriteBinding};
use axiom_ir::{lower, ModuleInput, Program, Target};
use axiom_runtime::{run_with_host, DatabaseHost, Host, Value};
use axiom_syntax::parse;

fn temp_dir(label: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("axiom-runtime-db-{label}-{unique}"));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("temp dir");
    path
}

fn spec(dir: &Path, require_approval: bool) -> DatabaseSpec {
    let mut spec = DatabaseSpec::new("app", dir.join("app.sqlite"))
        .with_tables(&["users"])
        .with_max_rows(10);
    if !require_approval {
        spec = spec.without_write_approval();
    }
    spec
}

fn seed(spec: &DatabaseSpec) {
    let connection = Connection::open(&spec.path, false).expect("open");
    connection
        .script(
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
             INSERT INTO users (id, name) VALUES (1, 'alice'), (2, 'bob')",
        )
        .expect("seed");
}

fn load(src: &str) -> Program {
    let file = parse(src).file;
    let inputs = [ModuleInput {
        name: "t".to_string(),
        display: "t.ax".to_string(),
        file: &file,
    }];
    lower(&inputs, Target::Server)
}

fn call_args(pairs: &[(&str, &str)]) -> Vec<(Option<String>, Value)> {
    pairs
        .iter()
        .map(|(name, value)| (Some(name.to_string()), Value::Str(value.to_string())))
        .collect()
}

const SOURCE: &str = r#"
module t;

type User { id: Int, name: String }

fn find() -> List<User> {
  return db.query(database: "app", sql: "SELECT id, name FROM users WHERE id <= :limit", params: { limit: 2 });
}

fn find_one() -> User {
  return db.query_one(database: "app", sql: "SELECT id, name FROM users WHERE id = :id", params: { id: 1 });
}
"#;

#[test]
fn queries_flow_through_the_host() {
    let dir = temp_dir("query");
    let spec = spec(&dir, false);
    seed(&spec);
    let host = DatabaseHost::open(spec, Actor::system("run"), AuditLog::memory()).expect("host");

    let program = load(SOURCE);
    let rows = run_with_host(&program, "find", vec![], &host).expect("find");
    match rows {
        Value::List(rows) => {
            assert_eq!(rows.len(), 2);
            match &rows[0] {
                Value::Record(fields) => {
                    assert!(fields.iter().any(|(name, value)| {
                        name == "name" && value == &Value::Str("alice".to_string())
                    }));
                }
                other => panic!("expected a record, got {other:?}"),
            }
        }
        other => panic!("expected a list, got {other:?}"),
    }

    let one = run_with_host(&program, "find_one", vec![], &host).expect("find_one");
    match one {
        Value::Record(fields) => {
            assert!(fields
                .iter()
                .any(|(name, value)| name == "id" && value == &Value::Int(1)));
        }
        other => panic!("expected a record, got {other:?}"),
    }
}

#[test]
fn host_audits_reads_without_values() {
    let dir = temp_dir("audit");
    let spec = spec(&dir, false);
    seed(&spec);
    let audit = AuditLog::memory();
    let host = DatabaseHost::open(spec, Actor::system("run"), audit).expect("host");
    let program = load(SOURCE);
    run_with_host(&program, "find", vec![], &host).expect("find");
    let records = host.audit().records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].outcome, axiom_database::AuditOutcome::Ok);
}

#[test]
fn writes_require_an_approval_and_then_succeed() {
    let dir = temp_dir("write");
    let spec = spec(&dir, true);
    seed(&spec);
    let host =
        DatabaseHost::open(spec.clone(), Actor::system("run"), AuditLog::memory()).expect("host");

    let mut params = Params::new();
    params.insert("name".to_string(), DbValue::Text("carol".to_string()));
    let args = vec![
        (Some("database".to_string()), Value::Str("app".to_string())),
        (
            Some("sql".to_string()),
            Value::Str("INSERT INTO users (name) VALUES (:name)".to_string()),
        ),
        (
            Some("params".to_string()),
            Value::Record(vec![("name".to_string(), Value::Str("carol".to_string()))]),
        ),
        (Some("max_affected_rows".to_string()), Value::Int(1)),
        (
            Some("idempotency_key".to_string()),
            Value::Str("key-1".to_string()),
        ),
    ];

    let denied = host.call("db", "execute", &args).unwrap_err();
    assert!(denied.contains("approval"), "{denied}");

    let statement: SqlStatement = sql::validate(
        "INSERT INTO users (name) VALUES (:name)",
        SqlClass::Write,
        &params,
        &spec.allowed_tables,
    )
    .unwrap();
    let binding = WriteBinding::new(
        "app",
        &statement.fingerprint,
        params_hash(&params),
        1,
        &statement.fingerprint,
    );
    host.approve(WriteApproval::issue(
        binding,
        "approval-1",
        "reviewer",
        axiom_database::unix_now() + 60,
    ));

    let outcome = host.call("db", "execute", &args).expect("execute");
    match outcome {
        Value::Record(fields) => {
            assert!(fields
                .iter()
                .any(|(name, value)| name == "affected_rows" && value == &Value::Int(1)));
        }
        other => panic!("expected an outcome record, got {other:?}"),
    }

    let replay = host.call("db", "execute", &args).expect("replay");
    match replay {
        Value::Record(fields) => {
            assert!(fields
                .iter()
                .any(|(name, value)| name == "replayed" && value == &Value::Bool(true)));
        }
        other => panic!("expected an outcome record, got {other:?}"),
    }
}

#[test]
fn operator_only_methods_are_refused() {
    let dir = temp_dir("refuse");
    let spec = spec(&dir, false);
    seed(&spec);
    let host = DatabaseHost::open(spec, Actor::system("run"), AuditLog::memory()).expect("host");
    for method in [
        "schema.inspect",
        "migrations.plan",
        "migrations.apply",
        "scratch.create",
    ] {
        let error = host.call("db", method, &[]).unwrap_err();
        assert!(error.contains("operator API"), "{method}: {error}");
    }
    let args = call_args(&[("database", "app"), ("sql", "SELECT 1")]);
    assert!(host.call("db", "query", &args).is_ok());
    assert!(host
        .call(
            "db",
            "query",
            &call_args(&[("database", "other"), ("sql", "SELECT 1")])
        )
        .is_err());
}
