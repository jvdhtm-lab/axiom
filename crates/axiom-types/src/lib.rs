//! Axiom semantic analysis (Phase 2).
//!
//! This crate builds the module graph for a set of `.ax` files and runs the
//! semantic checks that do not require a full type-inference engine: duplicate
//! declarations, malformed aggregates, generic arity, effect validity, `pure`
//! discipline, and cross-validation of literal model aliases and command names
//! against `axiom.yaml`.
//!
//! Type inference, expression typing, and the effect-propagation analysis are
//! tracked in `docs/implementation-ledger.md`.

pub mod check;
pub mod graph;
pub mod symbols;

pub use check::{check_project, has_errors, CheckOptions, KNOWN_EFFECTS};
pub use graph::{load, GraphDiagnostic, Module, ModuleGraph, ModuleId, Project};
pub use symbols::{Symbol, SymbolKind, SymbolTable};

use axiom_config::ProjectConfig;

/// Build the cross-validation options from a loaded `axiom.yaml`.
///
/// Model aliases and command names referenced literally in `.ax` source are
/// checked against the configured profiles.
pub fn check_options_from_config(config: &ProjectConfig) -> CheckOptions {
    CheckOptions {
        model_aliases: Some(config.models.keys().cloned().collect()),
        command_names: Some(config.commands.keys().cloned().collect()),
    }
}

/// The crate version, surfaced by the CLI.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
