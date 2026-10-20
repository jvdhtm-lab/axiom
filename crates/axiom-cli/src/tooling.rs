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
