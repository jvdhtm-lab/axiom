//! Reference-app boundary check: policy enforcement before a mock-backed API
//! handler runs. The handler source is the real `examples/src/server.ax`.

use std::cell::Cell;
use std::path::Path;

use axiom_config::parse_config;
use axiom_ir::{lower, ModuleInput, Program, Target};
use axiom_runtime::{run_with_host, AccessError, ApiGuard, Host, Principal, Value};
use axiom_syntax::parse;

const POLICY: &str = r#"
version: 1
api_policies:
  get_dashboard:
    version: "1.0.0"
    authentication: required
    requires: ["dashboard:read"]
    data_class: internal
    cache: none
"#;

fn guard() -> ApiGuard {
    let report = parse_config("axiom.yaml", POLICY);
    assert_eq!(report.error_count(), 0, "{:?}", report.diagnostics);
    ApiGuard::from_config(&report.config.expect("config"))
}

fn load_reference_app() -> Program {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/src");
    let shared = std::fs::read_to_string(root.join("shared.ax")).expect("shared.ax");
    let server = std::fs::read_to_string(root.join("server.ax")).expect("server.ax");
    let shared_file = parse(&shared).file;
    let server_file = parse(&server).file;
    assert!(parse(&shared).diagnostics.is_empty());
    assert!(parse(&server).diagnostics.is_empty());
    let inputs = [
        ModuleInput {
            name: "examples.shared".to_string(),
            display: "shared.ax".to_string(),
            file: &shared_file,
        },
        ModuleInput {
            name: "examples.server".to_string(),
            display: "server.ax".to_string(),
            file: &server_file,
        },
    ];
    lower(&inputs, Target::Server)
}

fn named<'a>(args: &'a [(Option<String>, Value)], name: &str) -> Option<&'a Value> {
    args.iter()
        .find(|(key, _)| key.as_deref() == Some(name))
        .map(|(_, value)| value)
}

/// Stands in for the database host and records whether the handler reached it.
struct MockDatabase {
    calls: Cell<usize>,
}

impl Host for MockDatabase {
    fn call(
        &self,
        namespace: &str,
        method: &str,
        args: &[(Option<String>, Value)],
    ) -> Result<Value, String> {
        if namespace != "db" || method != "query_one" {
            return Err(format!("unexpected call `{namespace}.{method}`"));
        }
        self.calls.set(self.calls.get() + 1);
        assert_eq!(
            named(args, "database"),
            Some(&Value::Str("teamboard".to_string()))
        );
        let sql = match named(args, "sql") {
            Some(Value::Str(sql)) => sql,
            _ => return Err("missing sql".to_string()),
        };
        assert!(sql.contains("v_team_performance"), "{sql}");
        let team_id = match named(args, "params") {
            Some(Value::Record(fields)) => fields
                .iter()
                .find(|(name, _)| name == "team_id")
                .and_then(|(_, value)| match value {
                    Value::Str(team_id) => Some(team_id.clone()),
                    _ => None,
                })
                .ok_or("missing team_id param")?,
            _ => return Err("missing params".to_string()),
        };
        let completed = if team_id == "team-1" { 3 } else { 0 };
        Ok(Value::Variant(
            "Ok".to_string(),
            vec![Value::Record(vec![
                ("team_id".to_string(), Value::Str(team_id)),
                ("completed".to_string(), Value::Int(completed)),
            ])],
        ))
    }
}

fn input(team_id: &str) -> Vec<Value> {
    vec![Value::Record(vec![(
        "team_id".to_string(),
        Value::Str(team_id.to_string()),
    )])]
}

#[test]
fn policy_is_loaded_from_config() {
    let guard = guard();
    let policy = guard.policy("get_dashboard").expect("policy");
    assert!(policy.authentication_required);
    assert_eq!(policy.required_capabilities, ["dashboard:read"]);
    assert_eq!(policy.data_class.as_deref(), Some("internal"));
    assert_eq!(policy.version.as_deref(), Some("1.0.0"));
}

#[test]
fn missing_policy_fails_closed() {
    let guard = guard();
    assert_eq!(
        guard.authorize("undeclared", None),
        Err(AccessError::MissingPolicy("undeclared".to_string()))
    );
}

#[test]
fn authentication_and_capability_are_required() {
    let guard = guard();
    assert_eq!(
        guard.authorize("get_dashboard", None),
        Err(AccessError::Unauthenticated("get_dashboard".to_string()))
    );
    let caller = Principal::new("u1", "team-1");
    assert_eq!(
        guard.authorize("get_dashboard", Some(&caller)),
        Err(AccessError::Forbidden {
            api: "get_dashboard".to_string(),
            capability: "dashboard:read".to_string(),
        })
    );
    let caller = caller.with_capability("dashboard:read");
    assert!(guard.authorize("get_dashboard", Some(&caller)).is_ok());
}

#[test]
fn tenant_is_checked_against_the_caller() {
    let guard = guard();
    let caller = Principal::new("u1", "team-1").with_capability("dashboard:read");
    assert!(guard
        .authorize_tenant("get_dashboard", Some(&caller), "team-1")
        .is_ok());
    assert_eq!(
        guard.authorize_tenant("get_dashboard", Some(&caller), "team-2"),
        Err(AccessError::TenantMismatch {
            expected: "team-2".to_string(),
            actual: "team-1".to_string(),
        })
    );
}

#[test]
fn refused_calls_never_reach_the_database() {
    let guard = guard();
    let program = load_reference_app();
    let host = MockDatabase {
        calls: Cell::new(0),
    };
    let caller = Principal::new("u1", "team-1");
    let refusal = guard.authorize("get_dashboard", Some(&caller)).unwrap_err();
    assert!(matches!(refusal, AccessError::Forbidden { .. }));
    assert_eq!(host.calls.get(), 0);
    let _ = &program;
}

#[test]
fn authorized_call_runs_the_reference_handler() {
    let guard = guard();
    let program = load_reference_app();
    let host = MockDatabase {
        calls: Cell::new(0),
    };
    let caller = Principal::new("u1", "team-1").with_capability("dashboard:read");
    let policy = guard
        .authorize_tenant("get_dashboard", Some(&caller), "team-1")
        .expect("authorized");
    assert_eq!(policy.data_class.as_deref(), Some("internal"));

    let result = run_with_host(&program, "get_dashboard", input("team-1"), &host).expect("run");
    match result {
        Value::Variant(name, args) => {
            assert_eq!(name, "Ok");
            match &args[0] {
                Value::Record(fields) => {
                    assert!(fields
                        .iter()
                        .any(|(name, value)| { name == "completed" && value == &Value::Int(3) }));
                }
                other => panic!("unexpected payload: {other:?}"),
            }
        }
        other => panic!("unexpected result: {other:?}"),
    }
    assert_eq!(host.calls.get(), 1);
}
