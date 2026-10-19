//! Parsing `workspace` declarations into enforcing runtime policies.

use axiom_cli::workspace;
use axiom_filesystem::IsolationMode;
use axiom_syntax::ast::{DeclKind, WorkspaceDecl};
use axiom_syntax::Diagnostic;

fn workspace_decl(source: &str) -> WorkspaceDecl {
    let parsed = axiom_syntax::parse(source);
    assert!(
        parsed.diagnostics.is_empty(),
        "{}",
        parsed
            .diagnostics
            .iter()
            .map(|d| format!("{}: {}", d.code.unwrap_or(""), d.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
    parsed
        .file
        .declarations
        .iter()
        .find_map(|decl| match &decl.kind {
            DeclKind::Workspace(workspace) => Some(workspace.clone()),
            _ => None,
        })
        .expect("a workspace declaration")
}

fn messages(issues: Vec<(axiom_syntax::Span, String)>) -> String {
    issues
        .into_iter()
        .map(|(_, message)| message)
        .collect::<Vec<_>>()
        .join("\n")
}

const VALID: &str = r#"
module t;

export workspace frontend_edit {
  root: ".";
  read: ["src/frontend/**", "src/shared/**"];
  write: ["src/frontend/**"];
  deny: ["src/server/**"];
  isolation: copy_on_write;
  limits: { max_files: 200; max_file_bytes: 1000000; max_changed_files: 30; }
}
"#;

#[test]
fn builds_the_runtime_policy() {
    let policy = workspace::policy(&workspace_decl(VALID)).unwrap();
    assert_eq!(policy.root, "");
    assert_eq!(policy.isolation, IsolationMode::CopyOnWrite);
    assert_eq!(policy.read.len(), 2);
    assert_eq!(policy.write.len(), 1);
    assert_eq!(policy.deny.len(), 1);
    assert_eq!(policy.limits.max_files, Some(200));
    assert_eq!(policy.limits.max_file_bytes, Some(1_000_000));
    assert_eq!(policy.limits.max_changed_files, Some(30));

    // The generated policy is the one the runtime enforces.
    assert!(policy.check_read("src/frontend/app.ax").is_ok());
    assert!(policy.check_read("src/server/app.ax").is_err());
    assert!(policy.check_write("src/shared/util.ax").is_err());
}

#[test]
fn unknown_isolation_fails_closed() {
    let source = VALID.replace("copy_on_write", "teleport");
    let error = workspace::policy(&workspace_decl(&source)).unwrap_err();
    assert!(messages(error).contains("unknown isolation mode `teleport`"));
}

#[test]
fn missing_root_is_reported() {
    let source = VALID.replace("root: \".\";", "");
    let error = workspace::policy(&workspace_decl(&source)).unwrap_err();
    assert!(messages(error).contains("missing required `root`"));
}

#[test]
fn path_escapes_are_rejected() {
    let source = VALID.replace("\"src/shared/**\"", "\"../outside/**\"");
    let error = workspace::policy(&workspace_decl(&source)).unwrap_err();
    assert!(messages(error).contains("invalid workspace path"));
}

#[test]
fn non_string_patterns_are_reported() {
    let source = VALID.replace("\"src/shared/**\"", "42");
    let error = workspace::policy(&workspace_decl(&source)).unwrap_err();
    assert!(messages(error).contains("string literals"));
}

#[test]
fn unknown_limit_is_reported() {
    let source = VALID.replace("max_files: 200;", "max_threads: 2;");
    let error = workspace::policy(&workspace_decl(&source)).unwrap_err();
    assert!(messages(error).contains("unknown workspace limit `max_threads`"));
}

#[test]
fn diagnostic_has_a_usable_code_path() {
    // Keep the `Diagnostic` import honest: the CLI renders issues with spans.
    let source = VALID.replace("copy_on_write", "teleport");
    let issues = workspace::policy(&workspace_decl(&source)).unwrap_err();
    let (span, message) = &issues[0];
    let diagnostic = Diagnostic::error(*span, message.clone());
    assert!(!diagnostic
        .render(&axiom_syntax::Source::new("t.ax", source))
        .is_empty());
}
