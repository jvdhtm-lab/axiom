//! Tool registry: builtins and configured command profiles.

use axiom_config::parse_config_with;
use axiom_tools::{ToolError, ToolKind, ToolRegistry};

const CONFIG: &str = "\
version: 1
commands:
  cargo_check:
    executable: cargo
    args: [\"check\", \"--offline\"]
    network: false
";

fn config() -> axiom_config::ProjectConfig {
    let env = |_: &str| None;
    let report = parse_config_with("inline.yaml", CONFIG, &env);
    assert!(!report.has_errors(), "{:?}", report.diagnostics);
    report.config.expect("a resolved config")
}

#[test]
fn builtins_are_registered_with_effects() {
    let registry = ToolRegistry::with_builtins();
    assert_eq!(
        registry.classify("fs.write").unwrap().kind,
        ToolKind::FilesystemWrite
    );
    assert_eq!(
        registry.classify("fs.write").unwrap().kind.effect(),
        "fs.write"
    );
    assert_eq!(
        registry.classify("fs.read").unwrap().kind.effect(),
        "fs.read"
    );
    assert_eq!(
        registry.classify("compiler.check").unwrap().kind.effect(),
        "command.run"
    );
    assert!(registry.contains("command.run"));
}

#[test]
fn configured_commands_become_tools() {
    let registry = ToolRegistry::from_config(&config());
    let entry = registry.classify("command.cargo_check").unwrap();
    assert_eq!(entry.kind, ToolKind::Command);
    assert_eq!(entry.command.as_deref(), Some("cargo_check"));

    let profile = registry.command("cargo_check").unwrap();
    assert_eq!(profile.executable, "cargo");
    assert_eq!(profile.args, vec!["check", "--offline"]);
    assert_eq!(profile.network, Some(false));
}

#[test]
fn unknown_tool_is_refused() {
    let registry = ToolRegistry::with_builtins();
    let error = registry.classify("telepathy").unwrap_err();
    assert_eq!(error, ToolError::UnknownTool("telepathy".to_string()));
}

#[test]
fn unknown_command_profile_is_refused() {
    let registry = ToolRegistry::from_config(&config());
    let error = registry.command("nope").unwrap_err();
    assert_eq!(error, ToolError::UnknownCommand("nope".to_string()));
}

#[test]
fn names_are_sorted() {
    let registry = ToolRegistry::from_config(&config());
    let mut names = registry.names();
    let sorted = names.clone();
    names.sort();
    assert_eq!(names, sorted);
    assert!(names.contains(&"fs.read"));
    assert!(names.contains(&"command.cargo_check"));
}
