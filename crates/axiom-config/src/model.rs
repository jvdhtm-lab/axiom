//! The resolved configuration model.
//!
//! These are the shapes the rest of the toolchain consumes after `axiom.yaml`
//! has been loaded, placeholders expanded, and the schema validated. The model
//! keeps the integration concepts separate: an **integration** names a protocol
//! `adapter` and an endpoint, while a **model** profile names an integration plus
//! a concrete model id and the capabilities it expects.

use std::collections::BTreeMap;

/// A secret value.
///
/// Its contents are only reachable through [`Secret::expose`]; `Debug` and
/// `Display` render a redacted placeholder so secrets cannot leak into logs,
/// diagnostics, IR dumps, or traces by accident.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Secret(value.into())
    }

    /// Reveal the underlying value. Callers must not log the result.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

impl std::fmt::Display for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

/// A capability a model profile expects from its endpoint.
///
/// Declared capabilities are expectations, not proof: adapters negotiate them
/// (Phase 4) and unsupported operations must fail explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    Chat,
    StructuredOutput,
    ToolCalling,
    Streaming,
    Vision,
    Embeddings,
    Reasoning,
}

impl Capability {
    pub const ALL: [Capability; 7] = [
        Capability::Chat,
        Capability::StructuredOutput,
        Capability::ToolCalling,
        Capability::Streaming,
        Capability::Vision,
        Capability::Embeddings,
        Capability::Reasoning,
    ];

    pub fn parse(text: &str) -> Option<Capability> {
        Some(match text {
            "chat" => Capability::Chat,
            "structured_output" => Capability::StructuredOutput,
            "tool_calling" => Capability::ToolCalling,
            "streaming" => Capability::Streaming,
            "vision" => Capability::Vision,
            "embeddings" => Capability::Embeddings,
            "reasoning" => Capability::Reasoning,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Capability::Chat => "chat",
            Capability::StructuredOutput => "structured_output",
            Capability::ToolCalling => "tool_calling",
            Capability::Streaming => "streaming",
            Capability::Vision => "vision",
            Capability::Embeddings => "embeddings",
            Capability::Reasoning => "reasoning",
        }
    }
}

/// An integration connection: a named endpoint speaking a protocol `adapter`.
#[derive(Debug, Clone)]
pub struct IntegrationConfig {
    /// The protocol adapter, e.g. `anthropic_messages` or `openai_compatible`.
    pub adapter: String,
    /// An optional documented protocol dialect (`deepseek`, `moonshot`, ...).
    pub dialect: Option<String>,
    pub base_url: String,
    pub api_key: Option<Secret>,
    pub api_key_optional: bool,
    pub timeout_seconds: Option<u64>,
    pub headers: BTreeMap<String, String>,
    /// Data classes this integration is cleared to receive (default `[public]`).
    pub allowed_data_classes: Vec<String>,
}

/// A model profile: a logical alias bound to an integration and concrete model id.
#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub integration: String,
    pub model: String,
    pub capabilities: Vec<Capability>,
}

/// Project-level runtime defaults and hard limits (agents.md §4).
#[derive(Debug, Clone, Default)]
pub struct RuntimeConfig {
    pub default_model: Option<String>,
    pub max_parallel_agents: Option<u32>,
    pub max_retries: Option<u32>,
    pub request_timeout_seconds: Option<u64>,
    pub max_model_calls: Option<u64>,
    pub max_wall_clock_seconds: Option<u64>,
    pub max_agent_attempts: Option<u32>,
    /// `max_run_cost: null` means "no cost ceiling".
    pub max_run_cost: Option<f64>,
}

/// Environment-independent filesystem operational limits.
///
/// These values are parsed and validated in Phase 1/4 but enforced by the
/// filesystem subsystem in Phase 5.
#[derive(Debug, Clone, Default)]
pub struct FilesystemConfig {
    pub project_root: Option<String>,
    pub default_agent_isolation: Option<String>,
    pub max_read_bytes: Option<u64>,
    pub max_write_bytes: Option<u64>,
    pub max_file_bytes: Option<u64>,
    pub max_files_per_task: Option<u64>,
    pub protect_vcs_metadata: Option<bool>,
}

/// A configured database (agents.md §10).
///
/// SQLite databases are files under the project root; PostgreSQL targets are
/// remote URLs. The connection URL is a [`Secret`] so credentials cannot leak
/// into diagnostics, IR dumps, or traces.
#[derive(Debug, Clone, Default)]
pub struct DatabaseConfig {
    /// `sqlite` or `postgres`.
    pub engine: String,
    /// SQLite file path, relative to the project root.
    pub path: Option<String>,
    /// PostgreSQL connection URL (redacted in debug output).
    pub url: Option<Secret>,
    /// The data class this database holds (default `internal`).
    pub data_class: Option<String>,
    /// Optional allowlist of tables application SQL may reference.
    pub allowed_tables: Vec<String>,
    /// Row cap for reads.
    pub max_rows: Option<u64>,
    /// Statement timeout in seconds.
    pub timeout_seconds: Option<u64>,
    /// Reviewed migration directory.
    pub migrations: Option<String>,
    /// Whether destructive migrations are configured as allowed at all.
    pub destructive_migrations: Option<bool>,
    /// Permit private/link-local PostgreSQL targets (default false).
    pub allow_private_network: Option<bool>,
    /// Require a write approval (default true).
    pub require_write_approval: Option<bool>,
}

/// An allowlisted command profile: an executable plus fixed arguments.
///
/// Commands are never assembled from model output; the runtime runs only named
/// profiles (Phase 5).
#[derive(Debug, Clone)]
pub struct CommandProfile {
    pub executable: String,
    pub args: Vec<String>,
    pub working_directory: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub network: Option<bool>,
}

/// A published API's policy (agents.md §4). The server enforces these on every
/// call; they are configuration, not an authorization grant from the client.
#[derive(Debug, Clone, Default)]
pub struct ApiPolicyConfig {
    pub version: Option<String>,
    pub authentication: Option<String>,
    pub requires: Vec<String>,
    pub data_class: Option<String>,
    pub cache: Option<String>,
}

/// Where a build target's entry points live.
#[derive(Debug, Clone, Default)]
pub struct EntryPointConfig {
    pub target: String,
    pub module: Option<String>,
    pub init: Option<String>,
    pub render: Option<String>,
    pub handle_event: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct WasmConfig {
    pub target: Option<String>,
    pub host_api_version: Option<u32>,
    pub max_module_bytes: Option<u64>,
    pub reproducible: Option<bool>,
}

/// Which targets to build and their entry points.
#[derive(Debug, Clone, Default)]
pub struct CompilationConfig {
    pub targets: Vec<String>,
    pub entrypoints: BTreeMap<String, EntryPointConfig>,
    pub wasm: WasmConfig,
}

#[derive(Debug, Clone, Default)]
pub struct BrowserStateConfig {
    pub default_scope: Option<String>,
    pub default_persistence: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DataCacheConfig {
    pub enabled: Option<bool>,
    pub max_entries: Option<u64>,
    pub max_bytes: Option<u64>,
    pub default_ttl_seconds: Option<u64>,
    pub serve_stale_on_error: Option<bool>,
}

/// Browser-host quotas and state/cache defaults (agents.md §4).
#[derive(Debug, Clone, Default)]
pub struct BrowserConfig {
    pub isolation: Option<String>,
    pub max_memory_pages: Option<u64>,
    pub max_compute_ms_per_event: Option<u64>,
    pub max_ui_nodes: Option<u64>,
    pub max_event_bytes: Option<u64>,
    pub max_storage_bytes: Option<u64>,
    pub max_snapshot_bytes: Option<u64>,
    pub preserve_previous_release_on_failure: Option<bool>,
    pub state: BrowserStateConfig,
    pub data_cache: DataCacheConfig,
}

#[derive(Debug, Clone, Default)]
pub struct RegistryConfig {
    pub content_addressed: Option<bool>,
    pub retain_accepted_versions: Option<u64>,
    pub require_pinned_references: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct BuildCacheConfig {
    pub enabled: Option<bool>,
    pub max_bytes: Option<u64>,
}

/// Frontend artifact registry and build-cache policy (agents.md §4, §8.5).
#[derive(Debug, Clone, Default)]
pub struct FrontendConfig {
    pub registry: RegistryConfig,
    pub build_cache: BuildCacheConfig,
}

/// A validated `axiom.yaml` project configuration.
#[derive(Debug, Clone, Default)]
pub struct ProjectConfig {
    pub version: u32,
    /// The Axiom language version the project targets (e.g. `"1.0"`).
    pub language: Option<String>,
    /// Module search set (`sources`), e.g. `["src/**/*.ax"]`.
    pub sources: Vec<String>,
    pub integrations: BTreeMap<String, IntegrationConfig>,
    pub models: BTreeMap<String, ModelConfig>,
    pub runtime: RuntimeConfig,
    pub api_policies: BTreeMap<String, ApiPolicyConfig>,
    pub compilation: CompilationConfig,
    pub browser: BrowserConfig,
    pub frontend: FrontendConfig,
    pub filesystem: FilesystemConfig,
    pub databases: BTreeMap<String, DatabaseConfig>,
    pub commands: BTreeMap<String, CommandProfile>,
}

impl ProjectConfig {
    /// The configuration version this build understands.
    pub const SUPPORTED_VERSION: u32 = 1;

    /// The Axiom language version this build understands.
    pub const SUPPORTED_LANGUAGE: &'static str = "1.0";

    /// The valid build target names.
    pub const TARGETS: [&'static str; 2] = ["server", "wasm_browser"];
}
