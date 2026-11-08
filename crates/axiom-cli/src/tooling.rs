//! Developer-tooling helpers: deterministic formatting and project scaffolding.

use std::path::{Path, PathBuf};

use axiom_syntax::{parse, Diagnostic};

/// The result of formatting one source string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatReport {
    pub formatted: String,
    pub changed: bool,
}

/// Format a source string, refusing to proceed when it does not parse.
pub fn format_source(source: &str) -> Result<FormatReport, Vec<Diagnostic>> {
    let parsed = parse(source);
    let errors: Vec<Diagnostic> = parsed
        .diagnostics
        .into_iter()
        .filter(Diagnostic::is_error)
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    let formatted = axiom_format::format(&parsed.file);
    Ok(FormatReport {
        changed: formatted != source,
        formatted,
    })
}

/// Create a minimal project skeleton. Refuses to overwrite any file.
pub fn scaffold(root: &Path) -> Result<Vec<PathBuf>, String> {
    let config = root.join("axiom.yaml");
    let source = root.join("src").join("main.ax");
    if config.exists() {
        return Err(format!("`{}` already exists", config.display()));
    }
    if source.exists() {
        return Err(format!("`{}` already exists", source.display()));
    }
    std::fs::create_dir_all(root.join("src"))
        .map_err(|error| format!("cannot create `{}`: {error}", root.join("src").display()))?;
    std::fs::write(&config, CONFIG)
        .map_err(|error| format!("cannot write `{}`: {error}", config.display()))?;
    std::fs::write(&source, SOURCE)
        .map_err(|error| format!("cannot write `{}`: {error}", source.display()))?;
    Ok(vec![config, source])
}

const CONFIG: &str = "\
# Axiom project configuration (Axiom 1.0 schema).
version: 1
language: \"1.0\"

sources:
  - \"src/**/*.ax\"

compilation:
  entrypoints:
    server:
      module: app.main
";

const SOURCE: &str = "\
module app.main;

export fn main() -> string {
  return \"hello from Axiom\";
}
";

/// Create a two-target application scaffold:
/// `<name>/config.yml`, `<name>/server/main.ax`, `<name>/frontend/main.ax`.
/// The name must be a valid Axiom module identifier.
pub fn scaffold_app(parent: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    validate_app_name(name)?;
    let root = parent.join(name);
    if root.exists() {
        return Err(format!("`{}` already exists", root.display()));
    }
    let server_dir = root.join("server");
    let frontend_dir = root.join("frontend");
    let config = root.join("config.yml");
    let server_source = server_dir.join("main.ax");
    let frontend_source = frontend_dir.join("main.ax");

    std::fs::create_dir_all(&server_dir)
        .map_err(|error| format!("cannot create `{}`: {error}", server_dir.display()))?;
    std::fs::create_dir_all(&frontend_dir)
        .map_err(|error| format!("cannot create `{}`: {error}", frontend_dir.display()))?;
    write(&config, &app_config(name))?;
    write(&server_source, &app_server(name))?;
    write(&frontend_source, &app_frontend(name))?;
    Ok(vec![config, server_source, frontend_source])
}

fn write(path: &Path, contents: &str) -> Result<(), String> {
    std::fs::write(path, contents)
        .map_err(|error| format!("cannot write `{}`: {error}", path.display()))
}

fn validate_app_name(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() => {}
        _ => return Err(format!("app name `{name}` must start with an ASCII letter")),
    }
    if let Some(bad) = chars.find(|c| !c.is_ascii_alphanumeric() && *c != '_') {
        return Err(format!(
            "app name `{name}` contains `{bad}`; use ASCII letters, digits, or `_`"
        ));
    }
    Ok(())
}

fn app_config(name: &str) -> String {
    format!(
        "\
# Axiom project configuration (Axiom 1.0 schema).
version: 1
language: \"1.0\"

sources:
  - \"server/**/*.ax\"
  - \"frontend/**/*.ax\"

compilation:
  targets: [server, wasm_browser]
  entrypoints:
    server:
      module: {name}.server
    wasm_browser:
      module: {name}.frontend
"
    )
}

fn app_server(name: &str) -> String {
    format!(
        "\
module {name}.server;

export api async fn health() -> string {{
  return \"ok\";
}}
"
    )
}

fn app_frontend(name: &str) -> String {
    format!(
        "\
module {name}.frontend;

export fn main() -> string {{
  return \"hello from {name}\";
}}
"
    )
}
