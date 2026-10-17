//! Axiom project configuration (`axiom.yaml`).
//!
//! Phase 1 scope: load a configuration file, expand `${...}` placeholders
//! explicitly, validate the schema strictly, and cross-check provider/model
//! references. No provider connection is made here (Phase 4).

pub mod expand;
pub mod load;
pub mod model;

pub use expand::{expand, ExpandError};
pub use load::{parse_config, parse_config_with, ConfigReport};
pub use model::{Capability, ModelConfig, ProjectConfig, ProviderConfig, RuntimeConfig, Secret};

/// The crate version, surfaced by the CLI.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
