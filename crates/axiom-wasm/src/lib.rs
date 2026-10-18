//! Axiom browser WASM backend and artifact manifest (Phase 4, partial).
//!
//! This crate compiles the checked IR toward the `wasm_browser` target. The
//! Phase 4 gate requires real browser smoke tests; those cannot run in this
//! environment (no `wasm32` runtime/browser harness), so this backend currently
//! emits a **valid, content-addressed WASM module that embeds the checked IR in
//! linear memory** together with a deterministic manifest. Function-level WASM
//! code generation and the browser render/event loop are tracked as remaining
//! Phase 4 work in `docs/implementation-ledger.md`.
//!
//! Portability is enforced here, fail-closed: any effect not allowed on the
//! browser target aborts the build rather than being stubbed.

use axiom_ir::{Program, Target};

pub mod sha256;

/// The host ABI version this backend targets.
pub const ABI_VERSION: u32 = 1;

/// The IR format version this backend embeds.
pub const IR_VERSION: u32 = axiom_ir::IR_VERSION;

/// A build failure on the `wasm_browser` target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasmError {
    /// A function uses an effect that the browser target forbids.
    UnsupportedEffect { function: String, effect: String },
    /// The module graph had no portable entry points.
    NoEntryPoints,
}

impl std::fmt::Display for WasmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WasmError::UnsupportedEffect { function, effect } => {
                write!(
                    f,
                    "function `{function}` uses `{effect}`, which is not available on wasm_browser"
                )
            }
            WasmError::NoEntryPoints => write!(f, "no portable entry points to lower"),
        }
    }
}

impl std::error::Error for WasmError {}

/// A compiled browser artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub abi_version: u32,
    pub ir_version: u32,
    pub content_hash: String,
    pub byte_len: usize,
    /// Exports actually present in the module.
    pub exports: Vec<String>,
    /// Exported, portable functions that the host may call (metadata).
    pub entry_points: Vec<String>,
    pub module: Vec<u8>,
}

impl Artifact {
    /// A deterministic manifest describing the artifact.
    pub fn manifest_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\n");
        out.push_str(&format!("  \"abi_version\": {},\n", self.abi_version));
        out.push_str(&format!("  \"ir_version\": {},\n", self.ir_version));
        out.push_str("  \"target\": \"wasm_browser\",\n");
        out.push_str("  \"codegen\": \"ir-embedding\",\n");
        out.push_str(&format!("  \"content_hash\": \"{}\",\n", self.content_hash));
        out.push_str(&format!("  \"byte_len\": {},\n", self.byte_len));
        out.push_str(&format!("  \"exports\": {},\n", json_array(&self.exports)));
        out.push_str(&format!(
            "  \"entry_points\": {}\n",
            json_array(&self.entry_points)
        ));
        out.push_str("}\n");
        out
    }
}

fn json_array(items: &[String]) -> String {
    let inner = items
        .iter()
        .map(|s| format!("\"{s}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{inner}]")
}

/// Compile a program to a browser artifact.
pub fn build(program: &Program) -> Result<Artifact, WasmError> {
    for module in &program.modules {
        for f in &module.functions {
            for effect in &f.effects {
                if !Target::WasmBrowser.allows_effect(effect) {
                    return Err(WasmError::UnsupportedEffect {
                        function: f.name.clone(),
                        effect: effect.clone(),
                    });
                }
            }
        }
    }

    let entry_points: Vec<String> = program
        .modules
        .iter()
        .flat_map(|m| m.functions.iter())
        .filter(|f| f.exported)
        .map(|f| f.name.clone())
        .collect();
    if entry_points.is_empty() {
        return Err(WasmError::NoEntryPoints);
    }

    let ir_bytes = axiom_ir::program_to_json(program).into_bytes();
    let module = encode_module(&ir_bytes);
    let content_hash = sha256::hex_digest(&module);

    Ok(Artifact {
        abi_version: ABI_VERSION,
        ir_version: axiom_ir::IR_VERSION,
        content_hash,
        byte_len: module.len(),
        exports: vec!["memory".to_string(), "ir_len".to_string()],
        entry_points,
        module,
    })
}

/// Encode a minimal, valid WASM module that exposes the IR bytes in memory.
///
/// Layout: magic/version, memory (min pages sized to hold the data), one
/// immutable `i32` global (`ir_len`), exports (`memory`, `ir_len`), and one
/// active data segment at offset 0.
fn encode_module(ir_bytes: &[u8]) -> Vec<u8> {
    let mut module = Vec::new();
    module.extend_from_slice(b"\0asm");
    module.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);

    let pages = ir_bytes.len().div_ceil(65536).max(1) as u32;

    section(&mut module, 5, &memory_section(pages));
    section(&mut module, 6, &global_section(ir_bytes.len() as i32));
    section(&mut module, 7, &export_section());
    section(&mut module, 11, &data_section(ir_bytes));
    module
}

fn memory_section(pages: u32) -> Vec<u8> {
    let mut body = vec![0x01, 0x00];
    unsigned_leb(&mut body, pages);
    body
}

fn global_section(len: i32) -> Vec<u8> {
    let mut body = vec![0x01, 0x7f, 0x00, 0x41];
    signed_leb(&mut body, len as i64);
    body.push(0x0b);
    body
}

fn export_section() -> Vec<u8> {
    let mut body = vec![0x02];
    export_entry(&mut body, "memory", 0x02, 0);
    export_entry(&mut body, "ir_len", 0x03, 0);
    body
}

fn export_entry(body: &mut Vec<u8>, name: &str, kind: u8, index: u32) {
    unsigned_leb(body, name.len() as u32);
    body.extend_from_slice(name.as_bytes());
    body.push(kind);
    unsigned_leb(body, index);
}

fn data_section(ir_bytes: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    unsigned_leb(&mut body, 1);
    unsigned_leb(&mut body, 0);
    body.push(0x41);
    signed_leb(&mut body, 0);
    body.push(0x0b);
    unsigned_leb(&mut body, ir_bytes.len() as u32);
    body.extend_from_slice(ir_bytes);
    body
}

fn section(module: &mut Vec<u8>, id: u8, body: &[u8]) {
    module.push(id);
    unsigned_leb(module, body.len() as u32);
    module.extend_from_slice(body);
}

fn unsigned_leb(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn signed_leb(out: &mut Vec<u8>, mut value: i64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        let done = (value == 0 && byte & 0x40 == 0) || (value == -1 && byte & 0x40 != 0);
        if done {
            out.push(byte);
            break;
        }
        out.push(byte | 0x80);
    }
}
