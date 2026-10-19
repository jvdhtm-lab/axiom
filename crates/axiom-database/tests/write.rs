mod common;

use axiom_database::sql::validate;
use axiom_database::sqlite::Connection;
use axiom_database::value::{params_hash, Params, Value};
use axiom_database::{
    Actor, ApprovalLedger, AuditLog, Database, DatabaseSpec, DbError, SqlClass, WriteApproval,
    WriteBinding, WriteRequest,
};

use common::TempDir;

fn params(name: &str) -> Params {
    let mut params = Params::new();
    params.insert("name".to_string(), Value::Text(name.to_string()));
    params
}

fn open(temp: &TempDir) -> (DatabaseSpec, Database) {
    let spec = DatabaseSpec::new("app", temp.db_path("app.sqlite"))
        .with_tables(&["users"])
        .with_max_rows(10);
    let database = Database::open(spec.clone()).expect("open");
    let setup = Connection::open(&temp.db_path("app.sqlite"), false).expect("setup");
    setup
        .script("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE)")
        .expect("ddl");
    (spec, database)
}

fn request(name: &str, idem: &str) -> WriteRequest {
    WriteRequest::new(
        "app",
        "INSERT INTO users (name) VALUES (:name)",
        params(name),
        1,
        idem,
        "action-hash",
    )
}

fn approval(spec: &DatabaseSpec, request: &WriteRequest, id: &str, expires: u64) -> WriteApproval {
    let statement = validate(
        &request.sql,
        SqlClass::Write,
        &request.params,
        &spec.allowed_tables,
    )
    .expect("valid write");
    let binding = WriteBinding::new(
        &spec.name,
        &statement.fingerprint,
        params_hash(&request.params),
        request.max_affected_rows,
        &request.action_hash,
    );
    WriteApproval::issue(binding, id, "reviewer", expires)
}

#[test]
fn writes_require_an_approval_and_execute_once() {
    let temp = TempDir::new("write-ok");
    let (spec, database) = open(&temp);
    let audit = AuditLog::memory();
    let actor = Actor::system("run");
    let mut ledger = ApprovalLedger::memory();
    let request = request("alice", "idem-1");

    assert_eq!(
        database
            .execute(&actor, &request, None, &mut ledger, &audit)
            .unwrap_err(),
        DbError::ApprovalRequired
    );

    let approval = approval(
        &spec,
        &request,
        "approval-1",
        axiom_database::unix_now() + 60,
    );
    let outcome = database
        .execute(&actor, &request, Some(&approval), &mut ledger, &audit)
        .expect("write");
    assert_eq!(outcome.affected_rows, 1);
    assert!(!outcome.replayed);

    let replayed = database
        .execute(&actor, &request, None, &mut ledger, &audit)
        .expect("idempotent replay");
    assert!(replayed.replayed);
    assert_eq!(replayed.affected_rows, 1);

    let setup = Connection::open(&temp.db_path("app.sqlite"), true).expect("read");
    let rows = setup
        .query(
            "SELECT name FROM users",
            &Params::new(),
            10,
            std::time::Duration::from_secs(1),
        )
        .expect("count");
    assert_eq!(rows.len(), 1);
}

#[test]
fn every_approval_field_is_bound() {
    let temp = TempDir::new("write-binding");
    let (spec, database) = open(&temp);
    let audit = AuditLog::memory();
    let actor = Actor::system("run");
    let mut ledger = ApprovalLedger::memory();
    let base = request("alice", "idem-bind");

    let wrong_params = approval(
        &spec,
        &request("bob", "idem-other"),
        "approval-params",
        axiom_database::unix_now() + 60,
    );
    assert_eq!(
        database
            .execute(&actor, &base, Some(&wrong_params), &mut ledger, &audit)
            .unwrap_err(),
        DbError::ApprovalMismatch("parameters")
    );

    let mut wrong_limit = approval(
        &spec,
        &base,
        "approval-limit",
        axiom_database::unix_now() + 60,
    );
    wrong_limit.binding.max_affected_rows = 99;
    assert_eq!(
        database
            .execute(&actor, &base, Some(&wrong_limit), &mut ledger, &audit)
            .unwrap_err(),
        DbError::ApprovalMismatch("affected-row limit")
    );

    let mut wrong_action = approval(
        &spec,
        &base,
        "approval-action",
        axiom_database::unix_now() + 60,
    );
    wrong_action.binding.action_hash = "other".to_string();
    assert_eq!(
        database
            .execute(&actor, &base, Some(&wrong_action), &mut ledger, &audit)
            .unwrap_err(),
        DbError::ApprovalMismatch("action")
    );
}

#[test]
fn expired_and_replayed_approvals_are_refused() {
    let temp = TempDir::new("write-expiry");
    let (spec, database) = open(&temp);
    let audit = AuditLog::memory();
    let actor = Actor::system("run");
    let mut ledger = ApprovalLedger::memory();

    let expired = approval(&spec, &request("alice", "idem-expired"), "id-expired", 0);
    assert_eq!(
        database
            .execute(
                &actor,
                &request("alice", "idem-expired"),
                Some(&expired),
                &mut ledger,
                &audit
            )
            .unwrap_err(),
        DbError::ApprovalExpired
    );

    let first = request("bob", "idem-first");
    let first_approval = approval(&spec, &first, "shared-id", axiom_database::unix_now() + 60);
    database
        .execute(&actor, &first, Some(&first_approval), &mut ledger, &audit)
        .expect("first write");

    let second = request("carol", "idem-second");
    let second_approval = approval(&spec, &second, "shared-id", axiom_database::unix_now() + 60);
    assert_eq!(
        database
            .execute(&actor, &second, Some(&second_approval), &mut ledger, &audit)
            .unwrap_err(),
        DbError::ApprovalReplayed
    );
}

#[test]
fn affected_row_limits_roll_back() {
    let temp = TempDir::new("write-limit");
    let (spec, database) = open(&temp);
    let audit = AuditLog::memory();
    let actor = Actor::system("run");
    let mut ledger = ApprovalLedger::memory();

    let mut insert = Params::new();
    insert.insert("a".to_string(), Value::Text("a".to_string()));
    insert.insert("b".to_string(), Value::Text("b".to_string()));
    insert.insert("c".to_string(), Value::Text("c".to_string()));
    let request = WriteRequest::new(
        "app",
        "INSERT INTO users (name) VALUES (:a), (:b), (:c)",
        insert,
        1,
        "idem-limit",
        "action-limit",
    );
    let approval = approval(
        &spec,
        &request,
        "approval-limit",
        axiom_database::unix_now() + 60,
    );
    assert_eq!(
        database
            .execute(&actor, &request, Some(&approval), &mut ledger, &audit)
            .unwrap_err(),
        DbError::AffectedRowLimitExceeded { max_rows: 1 }
    );

    let read = Connection::open(&temp.db_path("app.sqlite"), true).expect("read");
    let rows = read
        .query(
            "SELECT COUNT(*) AS count FROM users",
            &Params::new(),
            1,
            std::time::Duration::from_secs(1),
        )
        .expect("count");
    assert_eq!(rows[0].get("count"), Some(&Value::Integer(0)));
}

#[test]
fn idempotency_keys_cannot_be_reused_with_different_input() {
    let temp = TempDir::new("write-idem");
    let (spec, database) = open(&temp);
    let audit = AuditLog::memory();
    let actor = Actor::system("run");
    let mut ledger = ApprovalLedger::memory();

    let first = request("alice", "same-key");
    let first_approval = approval(&spec, &first, "approval-a", axiom_database::unix_now() + 60);
    database
        .execute(&actor, &first, Some(&first_approval), &mut ledger, &audit)
        .expect("first");

    let second = request("bob", "same-key");
    let second_approval = approval(
        &spec,
        &second,
        "approval-b",
        axiom_database::unix_now() + 60,
    );
    assert_eq!(
        database
            .execute(&actor, &second, Some(&second_approval), &mut ledger, &audit)
            .unwrap_err(),
        DbError::IdempotencyConflict
    );
}

#[test]
fn write_audit_redacts_values() {
    let temp = TempDir::new("write-audit");
    let (spec, database) = open(&temp);
    let audit = AuditLog::memory();
    let actor = Actor::system("run");
    let mut ledger = ApprovalLedger::memory();
    let request = request("top-secret-value", "idem-audit");
    let approval = approval(
        &spec,
        &request,
        "approval-audit",
        axiom_database::unix_now() + 60,
    );
    database
        .execute(&actor, &request, Some(&approval), &mut ledger, &audit)
        .expect("write");
    let records = audit.records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].outcome, axiom_database::AuditOutcome::Ok);
    assert!(!format!("{records:?}").contains("top-secret-value"));
}
