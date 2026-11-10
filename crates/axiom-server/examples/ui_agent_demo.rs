//! End-to-end UI agent demo.
//!
//! A trusted backend accepts a UI-change request, asks an agent for a new Axiom
//! frontend, validates/compiles it for `wasm_browser`, publishes the artifact in
//! memory, and serves it to the trusted browser host. The control page reloads
//! the host, so the UI changes on demand.
//!
//! Two agent modes:
//!
//! - `template` (default): deterministic, offline, safe template selection.
//! - `opencode`: calls the OpenCode CLI/API with a free model and a restricted,
//!   tool-less agent, then validates and compiles the generated Axiom source.
//!
//! Run:
//!   cargo run -p axiom-server --example ui_agent_demo
//!   AXIOM_UI_AGENT=opencode OPENCODE_BIN=...
//!     cargo run -p axiom-server --example ui_agent_demo
//!
//! The OpenCode mode needs `OPENCODE_AUTH`/auth configured and a free model
//! (`OPENCODE_MODEL`, default `opencode-go/beam-free`). See
//! `docs/ui-agent-demo.md`.

use std::io::Read;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use axiom_ir::{lower, ModuleInput, Target};
use axiom_runtime::{ApiGuard, ApiPolicy, Principal, Value};
use axiom_server::{Request, Response, Server, ServerError};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, PartialEq, Eq)]
enum AgentMode {
    Template,
    OpenCode,
}

impl AgentMode {
    fn from_env() -> Self {
        match std::env::var("AXIOM_UI_AGENT").as_deref() {
            Ok("opencode") => AgentMode::OpenCode,
            _ => AgentMode::Template,
        }
    }

    fn label(self) -> &'static str {
        match self {
            AgentMode::Template => "template",
            AgentMode::OpenCode => "opencode",
        }
    }
}

#[derive(Clone)]
struct Built {
    source: String,
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
    build_source(&source, theme)
}

fn check_source(source: &str) -> Result<(), String> {
    let parsed = axiom_syntax::parse(source);
    let parse_errors: Vec<_> = parsed
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.is_error())
        .collect();
    if !parse_errors.is_empty() {
        let details = parse_errors
            .iter()
            .map(|diagnostic| format!("{} {}", diagnostic.code.unwrap_or("-"), diagnostic.message))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(details);
    }

    let counter = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path =
        std::env::temp_dir().join(format!("axiom-agent-{}-{counter}.ax", std::process::id()));
    std::fs::write(&path, source).map_err(|error| format!("cannot stage source: {error}"))?;
    let project = axiom_types::load(std::slice::from_ref(&path));
    let options = axiom_types::CheckOptions {
        target: Some(axiom_types::Target::WasmBrowser),
        ..axiom_types::CheckOptions::none()
    };
    let diagnostics = axiom_types::check_project(&project, &options);
    let _ = std::fs::remove_file(&path);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.diagnostic.is_error())
        .map(|diagnostic| {
            format!(
                "{} {}",
                diagnostic.diagnostic.code.unwrap_or("-"),
                diagnostic.diagnostic.message
            )
        })
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn build_source(source: &str, label: &str) -> Result<Built, String> {
    check_source(source)?;
    let parsed = axiom_syntax::parse(source);
    let input = ModuleInput {
        name: "examples.ui_agent_demo".to_string(),
        display: "agent-frontend.ax".to_string(),
        file: &parsed.file,
    };
    let program = lower(&[input], Target::WasmBrowser);
    let artifact = axiom_wasm::build(&program).map_err(|error| error.to_string())?;
    let release_id = format!("rel-{}", &artifact.content_hash[..12]);
    Ok(Built {
        source: source.to_string(),
        theme: label.to_string(),
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

fn opencode_bin() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("OPENCODE_BIN") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    let local = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.tools/opencode/opencode.exe");
    if local.exists() {
        return Some(local);
    }
    let local_unix = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.tools/opencode/opencode");
    if local_unix.exists() {
        return Some(local_unix);
    }
    None
}

fn build_prompt(request: &str, current: &str) -> String {
    format!(
        r#"You are updating one Axiom frontend module in a demo backend.

Current module:
```ax
{current}
```

User request:
{request}

Return only one fenced `ax` block containing the complete updated module.
Keep `module examples.ui_agent_demo;` and the `init`, `render`, and
`handle_event` entry points with compatible signatures. Keep a button whose
event is an integer (1) so state can change. Use only ui.column, ui.row,
ui.heading, ui.text, ui.metric, and ui.button. Do not use tools, imports,
effects, JSON, HTML, JavaScript, or markdown inside the module.
"#
    )
}

fn repair_prompt(request: &str, previous: &str, error: &str) -> String {
    format!(
        r#"The previous Axiom module failed validation:

{error}

Previous module:
```ax
{previous}
```

Fix it for this request:
{request}

Return only one fenced `ax` block with the complete corrected module. Keep
`module examples.ui_agent_demo;`, `init`, `render`, `handle_event`, and a
button event of `1`. Use only ui.column, ui.row, ui.heading, ui.text, ui.metric,
and ui.button. Do not use tools, imports, effects, JSON, HTML, or JavaScript.
"#
    )
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for character in chars.by_ref() {
                if character.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(character);
    }
    out
}

fn extract_axiom(text: &str) -> Result<String, String> {
    let text = strip_ansi(text);
    let mut blocks: Vec<(bool, String)> = Vec::new();
    let mut current: Option<(bool, Vec<String>)> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some((tagged, lines)) = current.as_mut() {
            if trimmed.starts_with("```") {
                blocks.push((*tagged, lines.join("\n")));
                current = None;
            } else {
                lines.push(line.to_string());
            }
        } else if trimmed.starts_with("```") {
            let tagged = matches!(
                trimmed
                    .trim_start_matches('`')
                    .trim()
                    .to_ascii_lowercase()
                    .as_str(),
                "ax" | "axiom"
            );
            current = Some((tagged, Vec::new()));
        }
    }
    if let Some((tagged, lines)) = current {
        blocks.push((tagged, lines.join("\n")));
    }
    if let Some((_, block)) = blocks.iter().find(|(tagged, _)| *tagged) {
        let block = block.trim();
        if !block.is_empty() {
            return Ok(block.to_string());
        }
    }
    if let Some((_, block)) = blocks.first() {
        let block = block.trim();
        if !block.is_empty() {
            return Ok(block.to_string());
        }
    }
    if let Some(start) = text.find("module ") {
        if let Some(end) = text.rfind('}') {
            return Ok(text[start..=end].trim().to_string());
        }
    }
    Err("the agent did not return an Axiom code block".to_string())
}

fn kill_process_tree(child: &mut std::process::Child) {
    if cfg!(windows) {
        let _ = Command::new("taskkill")
            .args(["/pid", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    } else {
        let _ = child.kill();
    }
}

fn run_opencode(prompt: &str) -> Result<String, String> {
    let bin = opencode_bin().ok_or_else(|| {
        "OpenCode CLI not found; run scripts/install-opencode.ps1 or set OPENCODE_BIN".to_string()
    })?;
    let model =
        std::env::var("OPENCODE_MODEL").unwrap_or_else(|_| "opencode-go/beam-free".to_string());
    let agent = std::env::var("OPENCODE_AGENT").unwrap_or_else(|_| "axiom-ui".to_string());
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut child = Command::new(&bin)
        .current_dir(&repo_root)
        .args([
            "run",
            "--standalone",
            "--model",
            model.as_str(),
            "--agent",
            agent.as_str(),
            "--title",
            "Axiom UI update",
            prompt,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot start OpenCode: {error}"))?;

    let stdout = child.stdout.take().expect("stdout");
    let stderr = child.stderr.take().expect("stderr");
    let stdout_reader = thread::spawn(move || {
        let mut buffer = Vec::new();
        let mut stdout = stdout;
        let _ = stdout.read_to_end(&mut buffer);
        buffer
    });
    let stderr_reader = thread::spawn(move || {
        let mut buffer = Vec::new();
        let mut stderr = stderr;
        let _ = stderr.read_to_end(&mut buffer);
        buffer
    });

    let deadline = Instant::now() + Duration::from_secs(240);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                kill_process_tree(&mut child);
                let _ = child.wait();
                return Err("OpenCode timed out".to_string());
            }
            Ok(None) => thread::sleep(Duration::from_millis(200)),
            Err(error) => return Err(format!("cannot wait for OpenCode: {error}")),
        }
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| "OpenCode stdout reader failed".to_string())?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| "OpenCode stderr reader failed".to_string())?;
    let text = String::from_utf8_lossy(&stdout).to_string();
    if !status.success() {
        let error = strip_ansi(&String::from_utf8_lossy(&stderr));
        return Err(format!("OpenCode failed: {}", error.trim()));
    }
    Ok(text)
}

fn generate_and_build(mode: AgentMode, request: &str, current: &str) -> Result<Built, String> {
    match mode {
        AgentMode::Template => {
            let theme = theme_for(request).ok_or_else(|| {
                "choose `light`, `dark`, or `minimal`, or enable OpenCode".to_string()
            })?;
            build_frontend(theme)
        }
        AgentMode::OpenCode => {
            let mut prompt = build_prompt(request, current);
            let mut last_error = String::new();
            for _ in 0..2 {
                let output = run_opencode(&prompt)?;
                let source = extract_axiom(&output)?;
                match build_source(&source, "agent") {
                    Ok(built) => return Ok(built),
                    Err(error) => {
                        last_error = error;
                        prompt = repair_prompt(request, &source, &last_error);
                    }
                }
            }
            Err(format!("agent candidates failed validation: {last_error}"))
        }
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

    let mode = AgentMode::from_env();
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
        "initial release {} · theme {} · mode {} · {} bytes",
        initial.release_id,
        initial.theme,
        mode.label(),
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
                let current_source = update_state.read().expect("state lock").source.clone();
                let built =
                    generate_and_build(mode, request, &current_source).map_err(|error| {
                        ServerError::internal(format!("agent update failed: {error}"))
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
                    ("agent".to_string(), Value::Str(mode.label().to_string())),
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
