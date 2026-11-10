//! End-to-end UI agent demo.
//!
//! A trusted backend accepts a UI-change request, generates a new Axiom
//! frontend from declared templates, compiles it for `wasm_browser`, publishes
//! the artifact in memory, and serves it to the trusted browser host. The
//! control page reloads the host, so the UI changes on demand.
//!
//! The "agent" here is deterministic: it maps the request to one of a few
//! declared templates and never interpolates arbitrary source. This is the
//! orchestration boundary that a real model/provider adapter plugs into later.
//!
//! Run:
//!   cargo run -p axiom-server --example ui_agent_demo
//!   open http://127.0.0.1:8099/

use std::net::TcpListener;
use std::path::Path;
use std::sync::{Arc, RwLock};

use axiom_ir::{lower, ModuleInput, Target};
use axiom_runtime::{ApiGuard, ApiPolicy, Principal, Value};
use axiom_server::{Request, Response, Server, ServerError};

#[derive(Clone)]
struct Built {
    theme: String,
    release_id: String,
    content_hash: String,
    manifest: String,
    wasm: Vec<u8>,
}

fn source_for(theme: &str) -> Option<String> {
    let (heading, button) = match theme {
        "light" => ("Axiom TeamBoard", "Increment"),
        "dark" => ("Axiom TeamBoard · Dark", "Boost"),
        "minimal" => ("Axiom TeamBoard", "Add one"),
        _ => return None,
    };
    Some(format!(
        r#"module examples.ui_agent_demo;

export type DemoState = {{ count: int; }};

export enum AppError {{ BadEvent }}

export fn init(input: int) -> DemoState {{
  return {{ count: input }};
}}

export fn render(state: DemoState) -> UiTree {{
  return ui.column([
    ui.heading("{heading}"),
    ui.text("Theme: {theme}"),
    ui.metric("Completed tasks", state.count),
    ui.button("{button}", 1)
  ]);
}}

export fn handle_event(state: DemoState, event: int) -> Result<DemoState, AppError> {{
  return Ok({{ count: state.count + event }});
}}
"#
    ))
}

fn build_frontend(theme: &str) -> Result<Built, String> {
    let source = source_for(theme).ok_or_else(|| format!("unknown theme `{theme}`"))?;
    let parsed = axiom_syntax::parse(&source);
    if parsed
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.is_error())
    {
        return Err("generated frontend has parse errors".to_string());
    }
    let input = ModuleInput {
        name: "examples.ui_agent_demo".to_string(),
        display: "agent-frontend.ax".to_string(),
        file: &parsed.file,
    };
    let program = lower(&[input], Target::WasmBrowser);
    let artifact = axiom_wasm::build(&program).map_err(|error| error.to_string())?;
    let release_id = format!("rel-{}", &artifact.content_hash[..12]);
    Ok(Built {
        theme: theme.to_string(),
        release_id,
        content_hash: artifact.content_hash.clone(),
        manifest: artifact.manifest_json(),
        wasm: artifact.module,
    })
}

fn request_field<'a>(body: &'a Value, name: &str) -> Option<&'a str> {
    match body {
        Value::Record(fields) => {
            fields
                .iter()
                .find(|(field, _)| field == name)
                .and_then(|(_, value)| match value {
                    Value::Str(text) => Some(text.as_str()),
                    _ => None,
                })
        }
        _ => None,
    }
}

fn theme_for(request: &str) -> Option<&'static str> {
    let request = request.to_ascii_lowercase();
    if request.contains("dark") {
        Some("dark")
    } else if request.contains("minimal") {
        Some("minimal")
    } else if request.contains("light") || request.contains("default") {
        Some("light")
    } else {
        None
    }
}

fn read_asset(root: &Path, relative: &str) -> Result<Vec<u8>, String> {
    std::fs::read(root.join(relative)).map_err(|error| format!("cannot read {relative}: {error}"))
}

fn main() {
    let mut bind = "127.0.0.1:8099".to_string();
    let arguments: Vec<String> = std::env::args().collect();
    let mut index = 1;
    while index < arguments.len() {
        if arguments[index] == "--bind" {
            index += 1;
            if let Some(value) = arguments.get(index) {
                bind = value.clone();
            }
        }
        index += 1;
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let index_html = read_asset(&root, "browser/index.html").unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });
    let host_js = read_asset(&root, "browser/host.js").unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });
    let control_html = read_asset(&root, "browser/control.html").unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });

    let initial = build_frontend("light").unwrap_or_else(|error| {
        eprintln!("initial frontend failed: {error}");
        std::process::exit(1);
    });
    println!(
        "initial release {} · theme {} · {} bytes",
        initial.release_id,
        initial.theme,
        initial.wasm.len()
    );
    let current = Arc::new(RwLock::new(initial));

    let mut guard = ApiGuard::new();
    guard.register(ApiPolicy {
        name: "update_frontend".to_string(),
        authentication_required: true,
        required_capabilities: vec!["ui:write".to_string()],
        data_class: Some("internal".to_string()),
        version: Some("1.0.0".to_string()),
    });

    let authenticate = Arc::new(
        |request: &Request| match request.header("x-axiom-principal") {
            Some("demo-admin") => {
                Some(Principal::new("demo-admin", "demo").with_capability("ui:write"))
            }
            Some("demo-viewer") => Some(Principal::new("demo-viewer", "demo")),
            _ => None,
        },
    );

    let update_state = current.clone();
    let server = Server::new(guard, authenticate)
        .route(
            "POST",
            "/api/update_frontend",
            "update_frontend",
            move |_request, _principal, body| {
                let request = request_field(&body, "request").ok_or_else(|| {
                    ServerError::bad_request("body must be `{ request: string }`")
                })?;
                let theme = theme_for(request).ok_or_else(|| {
                    ServerError::bad_request("choose `light`, `dark`, or `minimal`")
                })?;
                let built = build_frontend(theme).map_err(|error| {
                    ServerError::internal(format!("generated frontend failed: {error}"))
                })?;
                let response = Value::Record(vec![
                    (
                        "release_id".to_string(),
                        Value::Str(built.release_id.clone()),
                    ),
                    (
                        "content_hash".to_string(),
                        Value::Str(built.content_hash.clone()),
                    ),
                    ("theme".to_string(), Value::Str(built.theme.clone())),
                ]);
                *update_state.write().expect("state lock") = built;
                Ok(response)
            },
        )
        .route_raw("GET", "/", move |_request| {
            Response::raw(200, "text/html; charset=utf-8", control_html.clone())
        })
        .route_raw("GET", "/app/", move |_request| {
            Response::raw(200, "text/html; charset=utf-8", index_html.clone())
        })
        .route_raw("GET", "/app/host.js", {
            let script = host_js.clone();
            move |_request| Response::raw(200, "text/javascript; charset=utf-8", script.clone())
        })
        .route_raw("GET", "/host.js", move |_request| {
            Response::raw(200, "text/javascript; charset=utf-8", host_js.clone())
        })
        .route_raw("GET", "/api/frontend/manifest", {
            let state = current.clone();
            move |_request| {
                let manifest = state
                    .read()
                    .expect("state lock")
                    .manifest
                    .clone()
                    .into_bytes();
                Response::raw(200, "application/json; charset=utf-8", manifest)
            }
        })
        .route_raw("GET", "/api/frontend/wasm", {
            let state = current.clone();
            move |_request| {
                let wasm = state.read().expect("state lock").wasm.clone();
                Response::raw(200, "application/wasm", wasm)
            }
        });

    let listener = TcpListener::bind(&bind).unwrap_or_else(|error| {
        eprintln!("cannot bind {bind}: {error}");
        std::process::exit(1);
    });
    println!("Axiom UI agent demo: http://{bind}/");
    if let Err(error) = server.serve(listener) {
        eprintln!("server error: {error}");
        std::process::exit(1);
    }
}
