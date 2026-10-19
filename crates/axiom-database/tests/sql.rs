use std::collections::BTreeMap;

use axiom_database::sql::{analyze, validate};
use axiom_database::value::Value;
use axiom_database::{DbError, SqlClass};

fn params(pairs: &[(&str, &str)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), Value::Text((*value).to_string())))
        .collect()
}

#[test]
fn injection_text_stays_a_parameter() {
    let statement = validate(
        "SELECT id FROM users WHERE name = :name",
        SqlClass::Read,
        &params(&[("name", "x'; DROP TABLE users; --")]),
        &["users".to_string()],
    )
    .expect("valid read");
    assert_eq!(statement.class, SqlClass::Read);
    assert_eq!(
        statement.tables,
        ["users".to_string()].into_iter().collect()
    );
    assert_eq!(statement.placeholders.len(), 1);
}

#[test]
fn multiple_statements_are_refused() {
    for sql in [
        "SELECT 1; DROP TABLE users",
        "SELECT 1; SELECT 2",
        "DELETE FROM users; DELETE FROM tasks",
    ] {
        assert_eq!(analyze(sql).unwrap_err(), DbError::MultipleStatements);
    }
}

#[test]
fn trailing_semicolon_and_semicolons_in_literals_are_allowed() {
    let statement = analyze("SELECT 'a;b' AS value;").expect("one statement");
    assert_eq!(statement.class, SqlClass::Read);
    assert!(analyze("SELECT 1;;").is_ok());
}

#[test]
fn anonymous_and_mismatched_placeholders_are_refused() {
    assert!(matches!(
        analyze("SELECT * FROM users WHERE id = ?"),
        Err(DbError::PlaceholderMismatch(_))
    ));
    assert!(matches!(
        validate(
            "SELECT * FROM users WHERE id = :id",
            SqlClass::Read,
            &params(&[("other", "1")]),
            &[]
        ),
        Err(DbError::PlaceholderMismatch(_))
    ));
    assert!(matches!(
        validate(
            "SELECT * FROM users WHERE id = :id",
            SqlClass::Read,
            &params(&[("id", "1"), ("extra", "2")]),
            &[]
        ),
        Err(DbError::PlaceholderMismatch(_))
    ));
}

#[test]
fn placeholders_in_comments_and_literals_are_ignored() {
    let statement = analyze("SELECT ':not_a_param', id -- :also_not\nFROM users WHERE id = :id")
        .expect("valid");
    assert_eq!(
        statement.placeholders,
        ["id".to_string()].into_iter().collect()
    );
}

#[test]
fn table_scope_is_enforced() {
    assert_eq!(
        validate(
            "SELECT * FROM secrets",
            SqlClass::Read,
            &BTreeMap::new(),
            &["users".to_string()]
        )
        .unwrap_err(),
        DbError::UnknownTable("secrets".to_string())
    );
    assert!(validate(
        "SELECT u.id FROM users u JOIN tasks t ON t.id = u.id",
        SqlClass::Read,
        &BTreeMap::new(),
        &["users".to_string(), "tasks".to_string()]
    )
    .is_ok());
}

#[test]
fn statement_classes_are_enforced() {
    assert_eq!(
        validate("DELETE FROM users", SqlClass::Read, &BTreeMap::new(), &[]).unwrap_err(),
        DbError::WrongClass {
            expected: "read",
            actual: "write"
        }
    );
    assert!(matches!(
        validate(
            "CREATE TABLE evil (id INTEGER)",
            SqlClass::Write,
            &BTreeMap::new(),
            &[]
        ),
        Err(DbError::WrongClass { .. })
    ));
}

#[test]
fn dangerous_statement_forms_are_forbidden() {
    assert!(matches!(
        analyze("ATTACH DATABASE 'x' AS y"),
        Err(DbError::Forbidden("attach"))
    ));
    assert_eq!(
        analyze("PRAGMA writable_schema = ON")
            .expect("schema statement")
            .class,
        SqlClass::Schema
    );
    assert!(matches!(
        analyze("BEGIN"),
        Err(DbError::Forbidden("transaction control"))
    ));
    assert!(matches!(
        analyze("SELECT load_extension('evil')"),
        Err(DbError::Forbidden("load_extension"))
    ));
}

#[test]
fn fingerprints_ignore_formatting_but_not_text() {
    let a = analyze("select  id from users where name = 'A'").expect("a");
    let b = analyze("SELECT id\nFROM users\nWHERE name = 'A';").expect("b");
    let c = analyze("SELECT id FROM users WHERE name = 'B'").expect("c");
    assert_eq!(a.fingerprint, b.fingerprint);
    assert_ne!(a.fingerprint, c.fingerprint);
    assert!(!a.normalized.contains("--"));
}

#[test]
fn cte_names_are_not_treated_as_tables() {
    let statement =
        analyze("WITH recent AS (SELECT id FROM users) SELECT id FROM recent").expect("valid");
    assert_eq!(
        statement.tables,
        ["users".to_string()].into_iter().collect()
    );
}

#[test]
fn with_write_is_classified_as_write() {
    let statement = analyze(
        "WITH target AS (SELECT id FROM users) DELETE FROM tasks WHERE id IN (SELECT id FROM target)",
    )
    .expect("valid");
    assert_eq!(statement.class, SqlClass::Write);
}
