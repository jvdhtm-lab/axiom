//! Negative parser fixtures: constructs that Axiom 1.0 rejects.
//!
//! Each case asserts that parsing produces an error diagnostic carrying a
//! specific stable code.

use axiom_syntax::parse;

fn codes(source: &str) -> Vec<&'static str> {
    parse(source)
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .filter_map(|d| d.code)
        .collect()
}

fn assert_rejected(source: &str, expected: &str) {
    let found = codes(source);
    assert!(
        found.contains(&expected),
        "expected {expected}, got {found:?} for source:\n{source}"
    );
}

#[test]
fn missing_module_is_reported() {
    assert_rejected("export type X = int;\n", "AX0002");
}

#[test]
fn forbidden_declaration_keywords_are_rejected() {
    for keyword in [
        "interface",
        "const",
        "team",
        "goal",
        "task",
        "contract",
        "policy",
        "workflow",
        "operation",
        "query",
        "state",
        "store",
        "transition",
        "migration",
        "tool",
    ] {
        let source = format!("module a;\n{keyword} Foo {{ }}\n");
        assert_rejected(&source, "AX0010");
    }
}

#[test]
fn forbidden_statement_keywords_are_rejected() {
    for keyword in [
        "while",
        "throw",
        "try",
        "yield",
        "require",
        "retry",
        "timeout",
        "on_error",
        "checkpoint",
        "emit",
        "delegate",
        "break",
        "continue",
    ] {
        let source = format!("module a;\nexport fn f() {{ {keyword} 1; }}\n");
        assert_rejected(&source, "AX0011");
    }
}

#[test]
fn let_mut_is_rejected() {
    assert_rejected("module a;\nexport fn f() { let mut x = 1; }\n", "AX0011");
}

#[test]
fn assignment_is_rejected() {
    assert_rejected("module a;\nexport fn f() { x = 1; }\n", "AX0012");
    assert_rejected("module a;\nexport fn f() { x += 1; }\n", "AX0012");
}

#[test]
fn lambda_is_rejected() {
    // `(x) => x` is not a valid expression in 1.0.
    let result = parse("module a;\nexport fn f() { let g = (x) => x; }\n");
    assert!(result.diagnostics.iter().any(|d| d.is_error()));
}

#[test]
fn unterminated_string_is_reported() {
    let result = parse("module a;\nexport fn f() { let s = \"abc; }\n");
    assert!(result.diagnostics.iter().any(|d| d.is_error()));
}

#[test]
fn malformed_enum_and_block_recover() {
    // Should not panic and should still report an error.
    let result = parse("module a;\nenum E { A; B\nexport fn f() { return 1; }\n");
    assert!(result.diagnostics.iter().any(|d| d.is_error()));
    // Recovery still finds the following function.
    assert!(result
        .file
        .declarations
        .iter()
        .any(|d| matches!(d.kind, axiom_syntax::ast::DeclKind::Function(_))));
}
