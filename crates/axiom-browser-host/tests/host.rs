//! Browser-host ABI and verification tests.

use std::path::Path;
use std::process::Command;

use axiom_browser_host::{check_module_header, validate_import, verify, HostError};
use axiom_ir::{lower, ModuleInput, Program, Target};
use axiom_wasm::{build, WasmError};

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
fn builds_and_verifies_a_portable_artifact() {
    let artifact = build(&example_program()).expect("build");
    verify(&artifact).expect("verify");
    assert!(artifact.exports.contains(&"memory".to_string()));
    assert!(artifact.entry_points.iter().any(|e| e == "main"));
}

#[test]
fn detects_tampering() {
    let mut artifact = build(&example_program()).expect("build");
    let last = artifact.module.len() - 1;
    artifact.module[last] ^= 0xff;
    assert!(matches!(
        verify(&artifact),
        Err(HostError::HashMismatch { .. })
    ));
}

#[test]
fn rejects_non_browser_effects() {
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

#[test]
fn import_and_header_validation() {
    assert!(validate_import("axiom.api_call").is_ok());
    assert!(matches!(
        validate_import("env.hacks"),
        Err(HostError::UnsupportedImport(_))
    ));
    assert!(matches!(
        check_module_header(b"nope"),
        Err(HostError::BadModule(_))
    ));
}

/// Node-based integration smoke test. Opt-in because it requires `node`; run
/// with `cargo test -p axiom-browser-host -- --ignored`.
#[test]
#[ignore = "requires node; run with --ignored"]
fn node_host_smoke() {
    let artifact = build(&example_program()).expect("build");
    let dir = std::env::temp_dir().join(format!("axiom-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let wasm_path = dir.join("app.wasm");
    let manifest_path = dir.join("app.manifest.json");
    std::fs::write(&wasm_path, &artifact.module).expect("write wasm");
    std::fs::write(&manifest_path, artifact.manifest_json()).expect("write manifest");

    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../browser/host-smoke.mjs");
    let output = Command::new("node")
        .arg(script)
        .arg(&wasm_path)
        .arg(&manifest_path)
        .output()
        .expect("run node");
    assert!(
        output.status.success(),
        "node smoke failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout).trim());
}
