//! WASM backend tests: SHA-256 correctness, module validity, and determinism.

use std::path::Path;

use axiom_ir::{lower, ModuleInput, Program, Target};
use axiom_wasm::{build, sha256, WasmError};

fn example_program() -> Program {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/src/deterministic.ax");
    let src = std::fs::read_to_string(path).expect("read deterministic example");
    let file = axiom_syntax::parse(&src).file;
    let inputs = [ModuleInput {
        name: "examples.deterministic".to_string(),
        display: "deterministic.ax".to_string(),
        file: &file,
    }];
    lower(&inputs, Target::WasmBrowser)
}

#[test]
fn sha256_matches_known_vectors() {
    assert_eq!(
        sha256::hex_digest(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256::hex_digest(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn artifact_is_a_valid_deterministic_module() {
    let program = example_program();
    let first = build(&program).expect("build");
    let second = build(&program).expect("build");

    assert_eq!(first.module, second.module, "module bytes must be stable");
    assert_eq!(first.content_hash, second.content_hash);
    assert_eq!(first.content_hash.len(), 64);
    assert_eq!(&first.module[0..4], b"\0asm");
    assert_eq!(&first.module[4..8], &[0x01, 0x00, 0x00, 0x00]);
    assert_eq!(first.byte_len, first.module.len());
    assert!(first.exports.contains(&"memory".to_string()));
}

#[test]
fn non_browser_effects_are_rejected() {
    let src = "module a;\nexport type Row = { id: int; };\n\
               export async fn f() -> int effects [db.read] {\n\
                 await db.query_one<Row>(database: \"d\", sql: \"SELECT 1\");\n\
                 return 1;\n\
               }\n";
    let file = axiom_syntax::parse(src).file;
    let inputs = [ModuleInput {
        name: "a".to_string(),
        display: "a.ax".to_string(),
        file: &file,
    }];
    let program = lower(&inputs, Target::WasmBrowser);
    assert!(matches!(
        build(&program),
        Err(WasmError::UnsupportedEffect { .. })
    ));
}
