//! Axiom project configuration (`axiom.yaml`).
//!
//! Phase 1 scope: load a configuration file, expand `${...}` placeholders
//! explicitly, validate the schema strictly, and cross-check integration/model
//! references. No integration connection is made here (Phase 4).

pub mod expand;
pub mod load;
pub mod model;

pub use expand::{expand, ExpandError};
pub use load::{parse_config, parse_config_with, ConfigReport};
pub use model::{
    ApiPolicyConfig, BrowserConfig, BrowserStateConfig, BuildCacheConfig, Capability,
    CommandProfile, CompilationConfig, DataCacheConfig, DatabaseConfig, EntryPointConfig,
    FilesystemConfig, FrontendConfig, FrontendStateConfig, IntegrationConfig, ModelConfig,
    ProjectConfig, RegistryConfig, RuntimeConfig, Secret, StateMigrationConfig, WasmConfig,
};

/// The crate version, surfaced by the CLI.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
