//! Model provider registry and alias resolution (agents.md §4, §6).
//!
//! Configuration names a model *alias* (`models.coding`) that points at an
//! integration (`integrations.deepseek`). Switching providers or models is a
//! configuration change: the compiled source never names a vendor. This crate
//! resolves an alias into a [`ProviderSpec`] and builds a [`ModelProvider`] from
//! a registered adapter factory.
//!
//! Phase 5 ships only the offline [`MockProvider`]; networked adapters arrive
//! behind the same trait later. Secrets never appear in a [`ProviderSpec`]: the
//! spec records only whether a key resolved, so specs are safe to put in traces.

use std::collections::BTreeMap;
use std::fmt;

use axiom_config::ProjectConfig;

/// Token accounting for a single completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// A single model request, addressed by alias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRequest {
    pub alias: String,
    pub prompt: String,
    pub max_tokens: Option<u32>,
}

impl ModelRequest {
    pub fn new(alias: impl Into<String>, prompt: impl Into<String>) -> Self {
        ModelRequest {
            alias: alias.into(),
            prompt: prompt.into(),
            max_tokens: None,
        }
    }
}

/// A model completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelResponse {
    pub text: String,
    pub model: String,
    pub usage: Usage,
}

/// The vendor-neutral description of a resolved model alias.
///
/// It intentionally carries no secret material — only [`ProviderSpec::has_secret`]
/// — so it can be logged or embedded in traces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSpec {
    pub alias: String,
    pub adapter: String,
    pub model: String,
    pub base_url: String,
    pub has_secret: bool,
    pub timeout_seconds: Option<u64>,
}

/// Anything that can answer a [`ModelRequest`].
pub trait ModelProvider {
    fn spec(&self) -> &ProviderSpec;
    fn complete(&mut self, request: &ModelRequest) -> Result<ModelResponse, ProviderError>;

    /// Requests observed so far, for tests and dry-run traces. Providers that
    /// keep no history return an empty slice.
    fn calls(&self) -> &[ModelRequest] {
        &[]
    }
}

/// A construction site for a provider of one adapter kind.
pub trait ProviderFactory: Send + Sync {
    /// The `adapter:` value this factory handles (e.g. `openai_compatible`).
    fn adapter(&self) -> &str;
    fn build(&self, spec: &ProviderSpec) -> Box<dyn ModelProvider>;
}

/// Reasons alias resolution or provider construction can fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// No `models.<alias>` profile exists.
    UnknownModel(String),
    /// The model profile points at an integration that is not declared.
    UnknownIntegration { alias: String, integration: String },
    /// No factory is registered for the integration's adapter.
    UnsupportedAdapter(String),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderError::UnknownModel(alias) => {
                write!(
                    f,
                    "unknown model alias `{alias}`; declare it under `models:`"
                )
            }
            ProviderError::UnknownIntegration { alias, integration } => write!(
                f,
                "model alias `{alias}` refers to unknown integration `{integration}`"
            ),
            ProviderError::UnsupportedAdapter(adapter) => {
                write!(f, "no provider factory registered for adapter `{adapter}`")
            }
        }
    }
}

impl std::error::Error for ProviderError {}

/// Resolve a model alias against a project configuration, without building a
/// provider and without exposing any secret value.
pub fn resolve_spec(config: &ProjectConfig, alias: &str) -> Result<ProviderSpec, ProviderError> {
    let model = config
        .models
        .get(alias)
        .ok_or_else(|| ProviderError::UnknownModel(alias.to_string()))?;
    let integration = config.integrations.get(&model.integration).ok_or_else(|| {
        ProviderError::UnknownIntegration {
            alias: alias.to_string(),
            integration: model.integration.clone(),
        }
    })?;

    Ok(ProviderSpec {
        alias: alias.to_string(),
        adapter: integration.adapter.clone(),
        model: model.model.clone(),
        base_url: integration.base_url.clone(),
        has_secret: integration.api_key.is_some(),
        timeout_seconds: integration.timeout_seconds,
    })
}

/// A deterministic, offline provider used by tests and dry runs.
///
/// Scripted replies match when the prompt contains the script key; unmatched
/// requests fall back to `mock:<model>`. Every request is recorded so tests can
/// assert what was sent.
pub struct MockProvider {
    spec: ProviderSpec,
    scripted: BTreeMap<String, String>,
    calls: Vec<ModelRequest>,
}

impl MockProvider {
    pub fn new(spec: ProviderSpec) -> Self {
        MockProvider {
            spec,
            scripted: BTreeMap::new(),
            calls: Vec::new(),
        }
    }

    /// Register a reply for any prompt containing `prompt_contains`.
    pub fn script(mut self, prompt_contains: impl Into<String>, reply: impl Into<String>) -> Self {
        self.scripted.insert(prompt_contains.into(), reply.into());
        self
    }

    /// Requests seen so far, in order.
    pub fn calls(&self) -> &[ModelRequest] {
        &self.calls
    }
}

impl ModelProvider for MockProvider {
    fn spec(&self) -> &ProviderSpec {
        &self.spec
    }

    fn calls(&self) -> &[ModelRequest] {
        &self.calls
    }

    fn complete(&mut self, request: &ModelRequest) -> Result<ModelResponse, ProviderError> {
        self.calls.push(request.clone());
        let text = self
            .scripted
            .iter()
            .find(|(needle, _)| request.prompt.contains(needle.as_str()))
            .map(|(_, reply)| reply.clone())
            .unwrap_or_else(|| format!("mock:{}", self.spec.model));

        Ok(ModelResponse {
            usage: Usage {
                input_tokens: request.prompt.split_whitespace().count() as u64,
                output_tokens: text.split_whitespace().count() as u64,
            },
            text,
            model: self.spec.model.clone(),
        })
    }
}

/// A factory that builds [`MockProvider`]s for a single adapter kind.
pub struct MockFactory {
    adapter: String,
}

impl MockFactory {
    pub fn new(adapter: impl Into<String>) -> Self {
        MockFactory {
            adapter: adapter.into(),
        }
    }
}

impl ProviderFactory for MockFactory {
    fn adapter(&self) -> &str {
        &self.adapter
    }

    fn build(&self, spec: &ProviderSpec) -> Box<dyn ModelProvider> {
        Box::new(MockProvider::new(spec.clone()))
    }
}

/// The set of adapter factories available to the runtime.
#[derive(Default)]
pub struct ProviderRegistry {
    factories: BTreeMap<String, Box<dyn ProviderFactory>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        ProviderRegistry::default()
    }

    /// A registry with mocks for the adapters the reference config uses.
    pub fn with_mock() -> Self {
        let mut registry = ProviderRegistry::new();
        registry.register(Box::new(MockFactory::new("anthropic_messages")));
        registry.register(Box::new(MockFactory::new("openai_compatible")));
        registry
    }

    pub fn register(&mut self, factory: Box<dyn ProviderFactory>) {
        self.factories
            .insert(factory.adapter().to_string(), factory);
    }

    pub fn is_supported(&self, adapter: &str) -> bool {
        self.factories.contains_key(adapter)
    }

    /// The registered adapter names, in sorted order.
    pub fn adapters(&self) -> Vec<&str> {
        self.factories.keys().map(String::as_str).collect()
    }

    /// Resolve `alias` and build a provider for its integration's adapter.
    pub fn build(
        &self,
        config: &ProjectConfig,
        alias: &str,
    ) -> Result<Box<dyn ModelProvider>, ProviderError> {
        let spec = resolve_spec(config, alias)?;
        match self.factories.get(&spec.adapter) {
            Some(factory) => Ok(factory.build(&spec)),
            None => Err(ProviderError::UnsupportedAdapter(spec.adapter)),
        }
    }
}
