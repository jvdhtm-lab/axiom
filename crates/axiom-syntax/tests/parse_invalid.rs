//! Invalid input must produce diagnostics (never a panic), and diagnostics must
//! render with source context.

use std::fs;
use std::path::{Path, PathBuf};

use axiom_syntax::{parse, Source};

fn fixtures(sub: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(sub)
}

#[test]
fn every_invalid_fixture_reports_an_error() {
    let dir = fixtures("invalid");
    let mut checked = 0;
    for entry in fs::read_dir(&dir).expect("invalid fixtures directory exists") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("ax") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        let result = parse(&text);
        assert!(
            result.has_errors(),
            "expected at least one error in {}",
            path.display()
        );

        // Rendering must not panic and must include a location header.
        let source = Source::new(path.display().to_string(), text);
        for diagnostic in &result.diagnostics {
            let rendered = diagnostic.render(&source);
            assert!(rendered.contains("-->"), "rendered without a location");
        }
        checked += 1;
    }
    assert!(checked >= 5, "expected several invalid fixtures");
}

#[test]
fn missing_binding_value_is_reported() {
    let source = "module x;\nconst n = ;\n";
    let result = parse(source);
    assert!(result.has_errors());
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.message.contains("expected an expression")));
}

#[test]
fn unknown_declaration_is_reported() {
    let source = "module x;\nwidget thing;\n";
    let result = parse(source);
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.message.contains("unknown declaration keyword")));
}

#[test]
fn diagnostics_carry_line_and_column() {
    let source = "module x;\ninterface I {\n  name string;\n}\n";
    let result = parse(source);
    let diagnostic = result.diagnostics.iter().find(|d| d.is_error()).unwrap();
    let source = Source::new("x.ax", source);
    let rendered = diagnostic.render(&source);
    assert!(rendered.contains("x.ax:3:"), "rendered was: {rendered}");
    assert!(rendered.contains('^'), "rendered was: {rendered}");
}

#[test]
fn error_recovery_reports_multiple_problems() {
    let source = "module x;\nconst a = ;\nconst b = ;\n";
    let result = parse(source);
    let error_count = result.diagnostics.iter().filter(|d| d.is_error()).count();
    assert!(
        error_count >= 2,
        "expected recovery to report multiple errors, got {error_count}"
    );
}

#[test]
fn parser_never_panics_on_token_soup() {
    let alphabet = [
        "module",
        "import",
        "from",
        "export",
        "type",
        "interface",
        "enum",
        "const",
        "let",
        "fn",
        "pure",
        "agent",
        "implements",
        "team",
        "tool",
        "contract",
        "goal",
        "policy",
        "workspace",
        "workflow",
        "if",
        "else",
        "match",
        "for",
        "while",
        "in",
        "return",
        "require",
        "escalate",
        "parallel",
        "retry",
        "emit",
        "delegate",
        "await",
        "effects",
        "{",
        "}",
        "(",
        ")",
        "[",
        "]",
        "<",
        ">",
        ":",
        ",",
        ".",
        ";",
        "|",
        "&&",
        "||",
        "=>",
        "->",
        "=",
        "==",
        "!",
        "?",
        "\"x\"",
        "120s",
        "1",
        "0.2",
        "true",
        "null",
        "\n",
        "//c\n",
        "/*x*/",
    ];

    let mut state: u64 = 0x1234_5678_9abc_def0;
    for _ in 0..1000 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let count = (state % 40) as usize;
        let mut source = String::new();
        for _ in 0..count {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let index = ((state >> 33) as usize) % alphabet.len();
            source.push_str(alphabet[index]);
            source.push(' ');
        }
        // The contract: parsing any input returns; it must not panic.
        let result = parse(&source);
        let _ = result.file.items.len();
    }
}
