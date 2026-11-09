use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use axiom_cli::tooling::{format_source, scaffold, scaffold_app};
use axiom_config::parse_config;

fn temp_root(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("axiom-tooling-{label}-{nanos}"))
}

#[test]
fn formatting_is_deterministic_and_idempotent() {
    let messy = "module   t;\nexport   fn main( )->string{return   \"hi\" ;}\n";
    let report = format_source(messy).expect("parses");
    assert!(report.changed);
    assert_eq!(
        report.formatted,
        "module t;\n\nexport fn main() -> string {\n  return \"hi\";\n}\n"
    );
    let again = format_source(&report.formatted).expect("reparses");
    assert!(!again.changed);
    assert_eq!(again.formatted, report.formatted);
}

#[test]
fn invalid_source_reports_diagnostics_instead_of_formatting() {
    let error = format_source("not a module").unwrap_err();
    assert!(!error.is_empty());
    assert!(error.iter().all(|diagnostic| diagnostic.is_error()));
}

#[test]
fn scaffold_writes_a_loadable_project_and_refuses_overwrite() {
    let root = temp_root("scaffold");
    let paths = scaffold(&root).expect("scaffold");
    assert_eq!(paths.len(), 2);
    let config = std::fs::read_to_string(root.join("axiom.yaml")).expect("config");
    let report = parse_config("axiom.yaml", &config);
    assert!(!report.has_errors(), "{:?}", report.diagnostics);
    let source = std::fs::read_to_string(root.join("src/main.ax")).expect("source");
    let parsed = axiom_syntax::parse(&source);
    assert!(
        !parsed.diagnostics.iter().any(|d| d.is_error()),
        "{:?}",
        parsed.diagnostics
    );
    assert_eq!(
        parsed.file.module.as_ref().expect("module").dotted(),
        "app.main"
    );

    let error = scaffold(&root).unwrap_err();
    assert!(error.contains("already exists"), "{error}");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn createapp_writes_a_server_frontend_and_config() {
    let parent = temp_root("createapp");
    let paths = scaffold_app(&parent, "my_app").expect("createapp");
    assert_eq!(paths.len(), 3);
    let root = parent.join("my_app");

    let config = std::fs::read_to_string(root.join("config.yml")).expect("config");
    let report = parse_config("config.yml", &config);
    assert!(!report.has_errors(), "{:?}", report.diagnostics);
    assert!(config.contains("module: my_app.server"));
    assert!(config.contains("module: my_app.frontend"));
    assert!(config.contains("# Optional model connections"));
    assert!(config.contains("# models:"));
    assert!(config.contains("#   local_coding:"));
    assert!(config.contains("#     model: \"${AXIOM_CODING_MODEL:-example-coder}\""));

    for (file, module) in [
        ("server/main.ax", "my_app.server"),
        ("frontend/main.ax", "my_app.frontend"),
    ] {
        let source = std::fs::read_to_string(root.join(file)).expect("source");
        let parsed = axiom_syntax::parse(&source);
        assert!(
            !parsed.diagnostics.iter().any(|d| d.is_error()),
            "{:?}",
            parsed.diagnostics
        );
        assert_eq!(
            parsed.file.module.as_ref().expect("module").dotted(),
            module
        );
    }

    let error = scaffold_app(&parent, "my_app").unwrap_err();
    assert!(error.contains("already exists"), "{error}");
    std::fs::remove_dir_all(&parent).ok();
}

#[test]
fn createapp_rejects_unsafe_or_invalid_names() {
    let parent = temp_root("createapp-names");
    for name in ["", "../escape", "my-app", "1app", "my app"] {
        let error = scaffold_app(&parent, name).unwrap_err();
        assert!(!error.is_empty(), "{name}");
    }
    assert!(!parent.join("escape").exists());
    std::fs::remove_dir_all(&parent).ok();
}
