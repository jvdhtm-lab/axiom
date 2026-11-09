//! Reference-app HTTP wiring: the real `examples/src/server.ax` handler runs
//! through `axiom-server`, behind `ApiGuard`, with a mock database host.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axiom_config::parse_config;
use axiom_ir::{lower, ModuleInput, Program, Target};
use axiom_runtime::{run_with_host, ApiGuard, Host, Principal, Value};
use axiom_server::{Request, Server, ServerError};
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

struct MockDatabase {
    calls: Arc<AtomicUsize>,
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
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(
            named(args, "database"),
            Some(&Value::Str("teamboard".to_string()))
        );
        let team_id = match named(args, "params") {
            Some(Value::Record(fields)) => fields
                .iter()
                .find(|(name, _)| name == "team_id")
                .and_then(|(_, value)| match value {
                    Value::Str(team_id) => Some(team_id.clone()),
                    _ => None,
                })
                .ok_or("missing team_id")?,
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

fn team_id(body: &Value) -> Result<String, ServerError> {
    match body {
        Value::Record(fields) => fields
            .iter()
            .find(|(name, _)| name == "team_id")
            .and_then(|(_, value)| match value {
                Value::Str(team_id) => Some(team_id.clone()),
                _ => None,
            })
            .ok_or_else(|| ServerError::bad_request("`team_id` must be a string")),
        _ => Err(ServerError::bad_request("body must be a record")),
    }
}

fn server(calls: Arc<AtomicUsize>) -> Server {
    let program = load_reference_app();
    let host = Arc::new(MockDatabase {
        calls: calls.clone(),
    });
    Server::new(
        guard(),
        Arc::new(
            |request: &Request| match request.header("x-axiom-principal") {
                Some("reader") => {
                    Some(Principal::new("u1", "team-1").with_capability("dashboard:read"))
                }
                Some("viewer") => Some(Principal::new("u2", "team-1")),
                _ => None,
            },
        ),
    )
    .route(
        "POST",
        "/api/get_dashboard",
        "get_dashboard",
        move |_request, _principal, body| {
            let team_id = team_id(&body)?;
            let input = vec![Value::Record(vec![(
                "team_id".to_string(),
                Value::Str(team_id),
            )])];
            run_with_host(&program, "get_dashboard", input, host.as_ref())
                .map_err(|error| ServerError::internal(error.message))
        },
    )
}

fn request(principal: Option<&str>, team_id: &str) -> Request {
    let mut headers = BTreeMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());
    if let Some(principal) = principal {
        headers.insert("x-axiom-principal".to_string(), principal.to_string());
    }
    Request {
        method: "POST".to_string(),
        path: "/api/get_dashboard".to_string(),
        headers,
        body: format!("{{\"team_id\":\"{team_id}\"}}").into_bytes(),
    }
}

#[test]
fn authorized_reference_handler_runs_over_http() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = server(calls.clone());
    let response = server.handle(&request(Some("reader"), "team-1"));
    assert_eq!(response.status, 200, "{response:?}");
    let body = String::from_utf8(response.body).expect("utf8");
    assert!(body.contains("\"completed\":3"), "{body}");
    assert!(body.contains("\"team_id\":\"team-1\""), "{body}");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn refused_reference_calls_never_reach_the_handler() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = server(calls.clone());
    assert_eq!(server.handle(&request(None, "team-1")).status, 401);
    assert_eq!(
        server.handle(&request(Some("viewer"), "team-1")).status,
        403
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn missing_team_id_is_a_bad_request() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = server(calls);
    let mut request = request(Some("reader"), "ignored");
    request.body = b"{}".to_vec();
    let response = server.handle(&request);
    assert_eq!(response.status, 400, "{response:?}");
}
