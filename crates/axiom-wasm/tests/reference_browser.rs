//! The reference frontend builds for the browser with typed API stubs, and the
//! server handler body does not enter the WASM artifact.

use std::path::Path;

use axiom_ir::{lower, ModuleInput, Program, Target};
use axiom_wasm::build;

fn load_reference_frontend() -> Program {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/src");
    let shared = std::fs::read_to_string(root.join("shared.ax")).expect("shared.ax");
    let server = std::fs::read_to_string(root.join("server.ax")).expect("server.ax");
    let frontend = std::fs::read_to_string(root.join("frontend.ax")).expect("frontend.ax");
    let shared_file = axiom_syntax::parse(&shared).file;
    let server_file = axiom_syntax::parse(&server).file;
    let frontend_file = axiom_syntax::parse(&frontend).file;
    let inputs = [
        ModuleInput {
            name: "examples.shared".to_string(),
            display: "shared.ax".to_string(),
            file: &shared_file,
        },
        ModuleInput {
            name: "examples.server".to_string(),
            display: "server.ax".to_string(),
            file: &server_file,
        },
        ModuleInput {
            name: "examples.frontend".to_string(),
            display: "frontend.ax".to_string(),
            file: &frontend_file,
        },
    ];
    lower(&inputs, Target::WasmBrowser)
}

#[test]
fn reference_frontend_builds_with_api_stubs_and_no_server_body() {
    let artifact = build(&load_reference_frontend()).expect("reference frontend builds");
    for entry in ["init", "render", "handle_event"] {
        assert!(
            artifact.entry_points.iter().any(|name| name == entry),
            "missing entry point {entry}"
        );
    }
    assert!(!artifact
        .entry_points
        .iter()
        .any(|name| name == "get_dashboard"));

    let embedded = String::from_utf8_lossy(&artifact.module);
    assert!(
        !embedded.contains("db.read"),
        "server effect leaked into WASM"
    );
    assert!(
        !embedded.contains("v_team_performance"),
        "server SQL leaked into WASM"
    );
    assert!(
        !embedded.contains("DbError"),
        "server database dependencies leaked into WASM"
    );
}
