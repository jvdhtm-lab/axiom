//! Axiom trusted browser host: the versioned ABI and artifact verification
//! (Phase 4, partial).
//!
//! WASM runs in an isolated worker with no ambient filesystem, network,
//! database, or DOM access. The host exposes a narrow, versioned ABI and fails
//! closed on unknown ABI versions or imports. This crate holds the validation
//! the host (and the Node/browser harnesses) must run before trusting an
//! artifact; the DOM renderer itself lives in the JS shell under `browser/`.

use axiom_wasm::{sha256, Artifact};

/// The host ABI version this crate implements.
pub const ABI_VERSION: u32 = 1;

/// The only import names the host will ever provide, at ABI v1.
pub const SUPPORTED_IMPORTS: &[&str] = &[
    "axiom.ui_render",
    "axiom.ui_event",
    "axiom.api_call",
    "axiom.storage_get",
    "axiom.storage_set",
    "axiom.navigate",
];

/// A host-level validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    AbiMismatch { expected: u32, found: u32 },
    IrMismatch { expected: u32, found: u32 },
    HashMismatch { expected: String, found: String },
    UnsupportedImport(String),
    BadModule(String),
    EntryPointMissing(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostError::AbiMismatch { expected, found } => {
                write!(
                    f,
                    "the host ABI is v{expected}, but the artifact needs v{found}"
                )
            }
            HostError::IrMismatch { expected, found } => {
                write!(
                    f,
                    "the host understands IR v{expected}, but the artifact is v{found}"
                )
            }
            HostError::HashMismatch { expected, found } => {
                write!(
                    f,
                    "artifact content hash mismatch: manifest {expected}, module {found}"
                )
            }
            HostError::UnsupportedImport(name) => {
                write!(f, "unsupported host import `{name}`")
            }
            HostError::BadModule(message) => write!(f, "invalid WASM module: {message}"),
            HostError::EntryPointMissing(name) => {
                write!(f, "artifact is missing entry point `{name}`")
            }
        }
    }
}

impl std::error::Error for HostError {}

/// Reject any import the host does not provide.
pub fn validate_import(name: &str) -> Result<(), HostError> {
    if SUPPORTED_IMPORTS.contains(&name) {
        Ok(())
    } else {
        Err(HostError::UnsupportedImport(name.to_string()))
    }
}

/// Validate a WASM module header (`\0asm` + version 1).
pub fn check_module_header(bytes: &[u8]) -> Result<(), HostError> {
    if bytes.len() < 8 {
        return Err(HostError::BadModule(
            "shorter than the 8-byte header".to_string(),
        ));
    }
    if &bytes[0..4] != b"\0asm" {
        return Err(HostError::BadModule("bad magic number".to_string()));
    }
    if bytes[4..8] != [0x01, 0x00, 0x00, 0x00] {
        return Err(HostError::BadModule(
            "unsupported WASM binary version".to_string(),
        ));
    }
    Ok(())
}

/// Fully verify an artifact the way a host must before instantiating it:
/// header, ABI/IR versions, content hash, and exported entry points.
pub fn verify(artifact: &Artifact) -> Result<(), HostError> {
    check_module_header(&artifact.module)?;

    if artifact.abi_version != ABI_VERSION {
        return Err(HostError::AbiMismatch {
            expected: ABI_VERSION,
            found: artifact.abi_version,
        });
    }
    if artifact.ir_version != axiom_wasm::IR_VERSION {
        return Err(HostError::IrMismatch {
            expected: axiom_wasm::IR_VERSION,
            found: artifact.ir_version,
        });
    }

    let actual = sha256::hex_digest(&artifact.module);
    if actual != artifact.content_hash {
        return Err(HostError::HashMismatch {
            expected: artifact.content_hash.clone(),
            found: actual,
        });
    }

    // Entry points are metadata in this partial backend; only the structural
    // exports must exist. Function-level codegen (Phase 4 remainder) will add
    // callable per-function exports and check each here.
    if !artifact.exports.iter().any(|e| e == "memory") {
        return Err(HostError::EntryPointMissing("memory".to_string()));
    }
    Ok(())
}
