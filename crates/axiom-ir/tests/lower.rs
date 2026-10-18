//! IR lowering tests: determinism, provenance, and recorded effects.

use std::path::Path;

use axiom_ir::{lower, program_to_json, ModuleInput, Target};
use axiom_syntax::parse;

fn json_for(src: &str) -> String {
    let file = parse(src).file;
    let inputs = [ModuleInput {
        name: "t".to_string(),
        display: "t.ax".to_string(),
        file: &file,
    }];
    program_to_json(&lower(&inputs, Target::Server))
}

fn example() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/src/deterministic.ax");
    std::fs::read_to_string(path).expect("read deterministic example")
}

#[test]
fn lowering_is_deterministic() {
    let src = example();
    assert_eq!(json_for(&src), json_for(&src));
}

#[test]
fn ir_records_effects_and_provenance() {
    let src = "module a;\nexport fn f() -> int effects [db.read] { return 1; }\n";
    let json = json_for(src);
    assert!(json.contains("\"effects\""), "{json}");
    assert!(json.contains("\"db.read\""), "{json}");
    assert!(json.contains("\"span\""), "{json}");
    assert!(json.contains("\"required_capabilities\""), "{json}");
}

#[test]
fn ir_resolves_enum_constructors() {
    let json = json_for(&example());
    assert!(json.contains("Sign.Positive"), "{json}");
    assert!(json.contains("\"variant\""), "{json}");
}

#[test]
fn ir_marks_api_functions() {
    let src = "module a;\nexport api fn f() -> int { return 1; }\n";
    let json = json_for(src);
    assert!(json.contains("\"api\": true"), "{json}");
}
