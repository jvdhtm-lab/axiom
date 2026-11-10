//! A small HTTP/1.1 server core with API authorization.
//!
//! This is intentionally not a general web framework. It provides:
//!
//! - explicit routes (`method`, `path`, exported API name);
//! - an injected authenticator that maps a request to a [`Principal`];
//! - [`ApiGuard`] authorization before the handler runs;
//! - JSON request/response bodies through [`crate::json`];
//! - bounded request sizes and one response per connection.
//!
//! TLS, CORS, chunked transfer encoding, and HTTP keep-alive are not implemented.
//! Put this behind a TLS-terminating proxy for production.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use axiom_runtime::{AccessError, ApiGuard, Principal, Value};

use crate::json::{from_json, to_json, JsonError};

/// The maximum request size, including headers and body.
pub const MAX_REQUEST_BYTES: usize = 1_048_576;

/// A parsed HTTP request.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

impl Request {
    /// A case-insensitive header lookup.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// Parse the body as JSON. An empty body is `Unit`.
    pub fn json(&self) -> Result<Value, JsonError> {
        if self.body.is_empty() {
            return Ok(Value::Unit);
        }
        let text = std::str::from_utf8(&self.body).map_err(|_| JsonError {
            message: "request body is not UTF-8".to_string(),
        })?;
        from_json(text)
    }
}

/// A response ready to be written.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

impl Response {
    /// A JSON response.
    pub fn json(status: u16, value: &Value) -> Self {
        match to_json(value) {
            Ok(text) => Response::text(status, "application/json; charset=utf-8", text),
            Err(error) => Response::error(500, "encoding_error", error.to_string()),
        }
    }

    /// A JSON error body with stable `error` and human-readable `message` fields.
    pub fn error(status: u16, code: &'static str, message: impl Into<String>) -> Self {
        let value = Value::Record(vec![
            ("error".to_string(), Value::Str(code.to_string())),
            ("message".to_string(), Value::Str(message.into())),
        ]);
        let text = to_json(&value).unwrap_or_else(|_| "{\"error\":\"encoding_error\"}".to_string());
        Response::text(status, "application/json; charset=utf-8", text)
    }

    /// A raw response for static files or non-JSON routes.
    pub fn raw(status: u16, content_type: &str, body: Vec<u8>) -> Self {
        let mut headers = BTreeMap::new();
        headers.insert("content-type".to_string(), content_type.to_string());
        Response {
            status,
            headers,
            body,
        }
    }

    fn text(status: u16, content_type: &str, text: String) -> Self {
        Response::raw(status, content_type, text.into_bytes())
    }

    fn write_to(&self, stream: &mut TcpStream) -> io::Result<()> {
        let mut head = format!("HTTP/1.1 {} {}\r\n", self.status, status_text(self.status));
        for (name, value) in &self.headers {
            head.push_str(name);
            head.push_str(": ");
            head.push_str(value);
            head.push_str("\r\n");
        }
        head.push_str(&format!("content-length: {}\r\n", self.body.len()));
        head.push_str("connection: close\r\n\r\n");
        stream.write_all(head.as_bytes())?;
        stream.write_all(&self.body)?;
        stream.flush()
    }
}

fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "Unknown",
    }
}

/// An error returned by an API handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerError {
    pub status: u16,
    pub code: &'static str,
    pub message: String,
}

impl ServerError {
    pub fn new(status: u16, code: &'static str, message: impl Into<String>) -> Self {
        ServerError {
            status,
            code,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(400, "bad_request", message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(500, "internal_error", message)
    }
}

/// A route handler. The runtime value is the decoded JSON body.
pub type Handler =
    Arc<dyn Fn(&Request, Option<&Principal>, Value) -> Result<Value, ServerError> + Send + Sync>;

/// A registered API route.
#[derive(Clone)]
pub struct Route {
    pub method: &'static str,
    pub path: &'static str,
    /// The exported API name used for policy lookup. This is not the URL path.
    pub api: String,
    handler: Handler,
}

/// A raw route handler. These routes are public by design (static files, host
/// assets, control pages) and do not pass through `ApiGuard`.
pub type RawHandler = Arc<dyn Fn(&Request) -> Response + Send + Sync>;

/// A registered raw route.
#[derive(Clone)]
pub struct RawRoute {
    pub method: &'static str,
    pub path: &'static str,
    handler: RawHandler,
}

/// The authentication boundary. An identity provider is injected; the server
/// never trusts a principal from the request body.
pub type Authenticator = Arc<dyn Fn(&Request) -> Option<Principal> + Send + Sync>;

/// A small route table plus authorization and JSON handling.
pub struct Server {
    guard: ApiGuard,
    routes: Vec<Route>,
    raw_routes: Vec<RawRoute>,
    authenticate: Authenticator,
}

impl Server {
    pub fn new(guard: ApiGuard, authenticate: Authenticator) -> Self {
        Server {
            guard,
            routes: Vec::new(),
            raw_routes: Vec::new(),
            authenticate,
        }
    }

    pub fn route(
        mut self,
        method: &'static str,
        path: &'static str,
        api: impl Into<String>,
        handler: impl Fn(&Request, Option<&Principal>, Value) -> Result<Value, ServerError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.routes.push(Route {
            method,
            path,
            api: api.into(),
            handler: Arc::new(handler),
        });
        self
    }

    /// Register a public raw route (static files, host assets, control pages).
    pub fn route_raw(
        mut self,
        method: &'static str,
        path: &'static str,
        handler: impl Fn(&Request) -> Response + Send + Sync + 'static,
    ) -> Self {
        self.raw_routes.push(RawRoute {
            method,
            path,
            handler: Arc::new(handler),
        });
        self
    }

    /// Handle one parsed request without a socket.
    pub fn handle(&self, request: &Request) -> Response {
        let path = request.path.split('?').next().unwrap_or(&request.path);
        if let Some(route) = self.routes.iter().find(|route| route.path == path) {
            if route.method != request.method {
                return Response::error(405, "method_not_allowed", "method not allowed");
            }
            let principal = (self.authenticate)(request);
            if let Err(error) = self.guard.authorize(&route.api, principal.as_ref()) {
                return access_response(error);
            }
            let body = match request.json() {
                Ok(value) => value,
                Err(error) => return Response::error(400, "invalid_json", error.message),
            };
            return match (route.handler)(request, principal.as_ref(), body) {
                Ok(value) => Response::json(200, &value),
                Err(error) => Response::error(error.status, error.code, error.message),
            };
        }

        if let Some(route) = self.raw_routes.iter().find(|route| route.path == path) {
            if route.method != request.method {
                return Response::error(405, "method_not_allowed", "method not allowed");
            }
            return (route.handler)(request);
        }

        Response::error(404, "not_found", "route not found")
    }

    /// Accept requests forever. Connection errors do not stop the server.
    pub fn serve(&self, listener: TcpListener) -> io::Result<()> {
        for stream in listener.incoming() {
            let stream = stream?;
            let _ = self.serve_stream(stream);
        }
        Ok(())
    }

    /// Accept and handle exactly one connection. Useful for tests and one-shot
    /// processes.
    pub fn serve_once(&self, listener: TcpListener) -> io::Result<()> {
        let (stream, _) = listener.accept()?;
        self.serve_stream(stream)
    }

    /// Read one request from a connection, write one response, and close.
    pub fn serve_stream(&self, mut stream: TcpStream) -> io::Result<()> {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(15)));
        let request = match read_request(&mut stream) {
            Ok(Some(request)) => request,
            Ok(None) => return Ok(()),
            Err(error) => {
                let response = if error.kind() == io::ErrorKind::InvalidData {
                    Response::error(400, "bad_request", "malformed HTTP request")
                } else {
                    Response::error(400, "bad_request", "could not read request")
                };
                let _ = response.write_to(&mut stream);
                return Ok(());
            }
        };
        let response = self.handle(&request);
        response.write_to(&mut stream)
    }
}

fn access_response(error: AccessError) -> Response {
    let message = error.to_string();
    match error {
        AccessError::Unauthenticated(_) => Response::error(401, "unauthenticated", message),
        AccessError::MissingPolicy(_) => Response::error(403, "missing_policy", message),
        AccessError::Forbidden { .. } => Response::error(403, "forbidden", message),
        AccessError::TenantMismatch { .. } => Response::error(403, "tenant_mismatch", message),
    }
}

fn read_request(stream: &mut TcpStream) -> io::Result<Option<Request>> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    let header_end = loop {
        if let Some(position) = find_header_end(&buffer) {
            break position;
        }
        if buffer.len() >= MAX_REQUEST_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request too large",
            ));
        }
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            if buffer.is_empty() {
                return Ok(None);
            }
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "incomplete request",
            ));
        }
        buffer.extend_from_slice(&chunk[..read]);
    };

    let header_text = std::str::from_utf8(&buffer[..header_end])
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "headers are not UTF-8"))?;
    let mut lines = header_text.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing request line"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing method"))?
        .to_ascii_uppercase();
    let path = parts
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing path"))?
        .to_string();
    let version = parts
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing HTTP version"))?;
    if !version.starts_with("HTTP/") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsupported HTTP version",
        ));
    }

    let mut headers = BTreeMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "malformed header"))?;
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    if headers
        .get("transfer-encoding")
        .is_some_and(|value| !value.eq_ignore_ascii_case("identity"))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "transfer encoding is not supported",
        ));
    }
    let content_length = match headers.get("content-length") {
        Some(value) => value
            .parse::<usize>()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid content-length"))?,
        None => 0,
    };
    if content_length > MAX_REQUEST_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "body too large"));
    }

    let body_start = header_end + 4;
    while buffer.len() < body_start + content_length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "incomplete body",
            ));
        }
        if buffer.len() + read > MAX_REQUEST_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request too large",
            ));
        }
        buffer.extend_from_slice(&chunk[..read]);
    }

    Ok(Some(Request {
        method,
        path,
        headers,
        body: buffer[body_start..body_start + content_length].to_vec(),
    }))
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}
