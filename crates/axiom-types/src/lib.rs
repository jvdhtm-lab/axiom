//! Axiom semantic analysis.
//!
//! Phase 1: the module graph, per-module symbol tables, and declaration-level
//! checks (parse diagnostics, duplicates, known effect names, agent/workspace
//! member shape). Phase 2 adds name resolution, type inference, effect
//! propagation, and target compatibility. See `docs/implementation-ledger.md`.

pub mod check;
pub mod graph;
pub mod symbols;

pub use check::{check_project, CheckOptions, ModuleDiagnostic, ISOLATION_MODES, KNOWN_EFFECTS};
pub use graph::{load, GraphDiagnostic, Module, ModuleGraph, ModuleId, Project};
pub use symbols::{Symbol, SymbolKind, SymbolTable};

use axiom_config::ProjectConfig;

/// Build the cross-validation options from a loaded `axiom.yaml`.
pub fn check_options_from_config(config: &ProjectConfig) -> CheckOptions {
    CheckOptions {
        model_aliases: Some(config.models.keys().cloned().collect()),
        command_names: Some(config.commands.keys().cloned().collect()),
    }
}

/// Whether any diagnostic in `diagnostics` is an error.
pub fn has_errors(diagnostics: &[ModuleDiagnostic]) -> bool {
    diagnostics.iter().any(|d| d.diagnostic.is_error())
}

/// The crate version, surfaced by the CLI.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
