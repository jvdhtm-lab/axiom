use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;

use axiom_runtime::{ApiGuard, ApiPolicy, Principal, Value};
use axiom_server::{Request, Server};

fn guard() -> ApiGuard {
    let mut guard = ApiGuard::new();
    guard.register(ApiPolicy {
        name: "get_dashboard".to_string(),
        authentication_required: true,
        required_capabilities: vec!["dashboard:read".to_string()],
        data_class: Some("internal".to_string()),
        version: Some("1.0.0".to_string()),
    });
    guard
}

fn server() -> Server {
    Server::new(
        guard(),
        Arc::new(
            |request: &Request| match request.header("x-axiom-principal") {
                Some("reader") => {
                    Some(Principal::new("alice", "acme").with_capability("dashboard:read"))
                }
                Some("viewer") => Some(Principal::new("bob", "acme")),
                _ => None,
            },
        ),
    )
    .route(
        "POST",
        "/api/get_dashboard",
        "get_dashboard",
        |_request, _principal, body| Ok(body),
    )
    .route(
        "POST",
        "/api/no_policy",
        "missing",
        |_request, _principal, _body| Ok(Value::Unit),
    )
}

fn request(principal: Option<&str>, body: &str) -> Request {
    let mut headers = BTreeMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());
    if let Some(principal) = principal {
        headers.insert("x-axiom-principal".to_string(), principal.to_string());
    }
    Request {
        method: "POST".to_string(),
        path: "/api/get_dashboard".to_string(),
        headers,
        body: body.as_bytes().to_vec(),
    }
}

#[test]
fn authentication_and_capabilities_are_checked_before_the_handler() {
    let server = server();
    let anonymous = server.handle(&request(None, r#"{"team_id":"t1"}"#));
    assert_eq!(anonymous.status, 401, "{anonymous:?}");

    let viewer = server.handle(&request(Some("viewer"), r#"{"team_id":"t1"}"#));
    assert_eq!(viewer.status, 403, "{viewer:?}");

    let reader = server.handle(&request(Some("reader"), r#"{"team_id":"t1"}"#));
    assert_eq!(reader.status, 200, "{reader:?}");
    let body = String::from_utf8(reader.body).expect("utf8");
    assert!(body.contains("\"team_id\":\"t1\""), "{body}");
}

#[test]
fn missing_policy_fails_closed() {
    let mut request = request(Some("reader"), "{}");
    request.path = "/api/no_policy".to_string();
    let response = server().handle(&request);
    assert_eq!(response.status, 403, "{response:?}");
    let body = String::from_utf8(response.body).expect("utf8");
    assert!(body.contains("missing_policy"), "{body}");
}

#[test]
fn routes_and_json_are_validated() {
    let server = server();
    let mut unknown = request(Some("reader"), "{}");
    unknown.path = "/api/unknown".to_string();
    assert_eq!(server.handle(&unknown).status, 404);

    let mut wrong_method = request(Some("reader"), "{}");
    wrong_method.method = "GET".to_string();
    assert_eq!(server.handle(&wrong_method).status, 405);

    let invalid = server.handle(&request(Some("reader"), "{not json"));
    assert_eq!(invalid.status, 400, "{invalid:?}");
}

#[test]
fn a_real_tcp_request_is_served() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("address");
    let server = Arc::new(server());
    let serving = server.clone();
    let handle = thread::spawn(move || serving.serve_once(listener));

    let mut stream = TcpStream::connect(address).expect("connect");
    let body = r#"{"team_id":"t1"}"#;
    write!(
        stream,
        "POST /api/get_dashboard HTTP/1.1\r\nHost: localhost\r\nx-axiom-principal: reader\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
        body.len(),
        body
    )
    .expect("write request");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read response");

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.contains("\"team_id\":\"t1\""), "{response}");
    handle.join().expect("join").expect("serve");
}

#[test]
fn raw_routes_serve_public_assets() {
    let server = server()
        .route_raw("GET", "/app/", |_request| {
            axiom_server::Response::raw(200, "text/html", b"<html>".to_vec())
        })
        .route_raw("GET", "/app/host.js", |_request| {
            axiom_server::Response::raw(200, "text/javascript", b"host".to_vec())
        });
    let mut request = Request {
        method: "GET".to_string(),
        path: "/app/host.js".to_string(),
        headers: BTreeMap::new(),
        body: Vec::new(),
    };
    let response = server.handle(&request);
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"host");
    request.path = "/app/".to_string();
    assert_eq!(server.handle(&request).body, b"<html>");
}
