//! Formatter tests: deterministic output that re-parses cleanly.

use axiom_format::format;
use axiom_syntax::parse;

fn format_source(source: &str) -> String {
    let result = parse(source);
    assert!(
        !result.diagnostics.iter().any(|d| d.is_error()),
        "input did not parse: {:#?}",
        result.diagnostics
    );
    format(&result.file)
}

#[test]
fn formatting_is_idempotent() {
    let source = std::fs::read_to_string(format!(
        "{}/../../examples/src/frontend.ax",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let once = format_source(&source);
    let twice = format(&parse(&once).file);
    assert_eq!(once, twice);
}

#[test]
fn formatted_output_reparses_without_errors() {
    for name in ["shared.ax", "server.ax", "frontend.ax", "agents.ax"] {
        let path = format!("{}/../../examples/src/{name}", env!("CARGO_MANIFEST_DIR"));
        let source = std::fs::read_to_string(&path).unwrap();
        let formatted = format_source(&source);
        let reparsed = parse(&formatted);
        assert!(
            !reparsed.diagnostics.iter().any(|d| d.is_error()),
            "{name} reparse failed:\n{formatted}\n{:#?}",
            reparsed.diagnostics
        );
    }
}

#[test]
fn normalizes_whitespace() {
    let formatted = format_source("module   a;\nexport   type   X=int;\n");
    assert_eq!(formatted, "module a;\n\nexport type X = int;\n");
}
