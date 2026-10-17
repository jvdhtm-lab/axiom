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
    pub base_url: String,
    pub api_key: Option<Secret>,
    pub api_key_optional: bool,
    pub timeout_seconds: Option<u64>,
    pub headers: BTreeMap<String, String>,
}

/// A model profile: a logical alias bound to an integration and concrete model id.
#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub integration: String,
    pub model: String,
    pub capabilities: Vec<Capability>,
}

/// Project-level runtime defaults and limits.
#[derive(Debug, Clone, Default)]
pub struct RuntimeConfig {
    pub default_model: Option<String>,
    pub max_parallel_agents: Option<u32>,
    pub max_retries: Option<u32>,
    pub request_timeout_seconds: Option<u64>,
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
    pub max_files_per_task: Option<u64>,
    pub protect_vcs_metadata: Option<bool>,
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

/// A validated `axiom.yaml` project configuration.
#[derive(Debug, Clone)]
pub struct ProjectConfig {
    pub version: u32,
    pub integrations: BTreeMap<String, IntegrationConfig>,
    pub models: BTreeMap<String, ModelConfig>,
    pub runtime: RuntimeConfig,
    pub filesystem: FilesystemConfig,
    pub commands: BTreeMap<String, CommandProfile>,
}

impl ProjectConfig {
    /// The configuration version this build understands.
    pub const SUPPORTED_VERSION: u32 = 1;
}
