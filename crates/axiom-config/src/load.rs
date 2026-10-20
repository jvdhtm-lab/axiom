//! Load and validate an `axiom.yaml` project configuration.
//!
//! The loader is deliberately explicit: it parses YAML into a value tree,
//! expands `${...}` placeholders itself, checks the schema strictly (unknown
//! keys are errors, not silently dropped), and cross-checks references between
//! integrations, model profiles, and runtime defaults.

use std::collections::BTreeMap;
use std::fmt;

use axiom_syntax::{Diagnostic, Span};
use yaml_serde::value::Mapping;
use yaml_serde::Value;

use crate::expand::expand;
use crate::model::{
    ApiPolicyConfig, BrowserConfig, BrowserStateConfig, BuildCacheConfig, Capability,
    CommandProfile, CompilationConfig, DataCacheConfig, DatabaseConfig, EntryPointConfig,
    FilesystemConfig, FrontendConfig, FrontendStateConfig, IntegrationConfig, ModelConfig,
    ProjectConfig, RegistryConfig, RuntimeConfig, Secret, StateMigrationConfig, WasmConfig,
};

/// The outcome of loading a configuration file.
#[derive(Debug, Clone)]
pub struct ConfigReport {
    pub config: Option<ProjectConfig>,
    pub diagnostics: Vec<Diagnostic>,
}

impl ConfigReport {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }

    pub fn error_count(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.is_error()).count()
    }

    pub fn warning_count(&self) -> usize {
        self.diagnostics.iter().filter(|d| !d.is_error()).count()
    }
}

/// Load configuration using the process environment for placeholders.
pub fn parse_config(name: &str, text: &str) -> ConfigReport {
    parse_config_with(name, text, &|key| std::env::var(key).ok())
}

/// Load configuration using `env` for placeholder expansion.
pub fn parse_config_with(
    name: &str,
    text: &str,
    env: &dyn Fn(&str) -> Option<String>,
) -> ConfigReport {
    let _ = name;
    let value: Value = match yaml_serde::from_str(text) {
        Ok(value) => value,
        Err(err) => {
            let span = yaml_error_span(text, &err);
            return ConfigReport {
                config: None,
                diagnostics: vec![Diagnostic::error(span, format!("invalid YAML: {err}"))],
            };
        }
    };

    let mut builder = Builder::new(env, text);
    let config = build_root(&value, &mut builder).unwrap_or_else(default_config);
    ConfigReport {
        config: Some(config),
        diagnostics: builder.diagnostics,
    }
}

fn default_config() -> ProjectConfig {
    ProjectConfig {
        version: ProjectConfig::SUPPORTED_VERSION,
        language: None,
        sources: Vec::new(),
        integrations: BTreeMap::new(),
        models: BTreeMap::new(),
        runtime: RuntimeConfig::default(),
        api_policies: BTreeMap::new(),
        compilation: CompilationConfig::default(),
        browser: BrowserConfig::default(),
        frontend: FrontendConfig::default(),
        frontend_state: None,
        filesystem: FilesystemConfig::default(),
        databases: BTreeMap::new(),
        commands: BTreeMap::new(),
    }
}

/// Accumulates diagnostics and shares the environment source and raw text.
struct Builder<'a> {
    env: &'a dyn Fn(&str) -> Option<String>,
    text: &'a str,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Builder<'a> {
    fn new(env: &'a dyn Fn(&str) -> Option<String>, text: &'a str) -> Self {
        Builder {
            env,
            text,
            diagnostics: Vec::new(),
        }
    }

    /// Approximate the span of a labelled key. YAML values carry no spans, so we
    /// look for the last path segment (`integrations.claude` -> `claude:`). When
    /// it cannot be found we fall back to the start of the file; this is
    /// best-effort and documented in `docs/configuration.md`.
    fn span_for(&self, label: &str) -> Span {
        let segment = label.rsplit('.').next().unwrap_or(label);
        let needle = format!("{segment}:");
        match self.text.find(&needle) {
            Some(index) => Span::new(index as u32, (index + needle.len()) as u32),
            None => Span::new(0, 0),
        }
    }

    fn error(&mut self, label: &str, message: impl fmt::Display) {
        let span = self.span_for(label);
        self.diagnostics
            .push(Diagnostic::error(span, format!("{label}: {message}")));
    }

    fn warn(&mut self, label: &str, message: impl fmt::Display) {
        let span = self.span_for(label);
        self.diagnostics
            .push(Diagnostic::warning(span, format!("{label}: {message}")));
    }

    fn expand(&mut self, label: &str, field: &str, raw: &str) -> Option<String> {
        match expand(raw, self.env) {
            Ok(value) => Some(value),
            Err(error) => {
                self.error(label, format!("`{field}`: {error}"));
                None
            }
        }
    }
}

fn build_root(value: &Value, builder: &mut Builder<'_>) -> Option<ProjectConfig> {
    let mapping = match value {
        Value::Null => return Some(default_config()),
        Value::Mapping(mapping) => mapping,
        other => {
            builder.error(
                "axiom.yaml",
                format!(
                    "expected a mapping at the top level, found {}",
                    type_name(other)
                ),
            );
            return None;
        }
    };

    check_allowed(
        mapping,
        &[
            "version",
            "language",
            "sources",
            "integrations",
            "models",
            "runtime",
            "api_policies",
            "compilation",
            "browser",
            "frontend",
            "frontend_state",
            "filesystem",
            "databases",
            "commands",
        ],
        "axiom.yaml",
        builder,
    );

    let version = read_version(mapping, builder);
    let language = read_language(mapping, builder);
    let sources = build_string_list(mapping, "axiom.yaml", "sources", builder);
    let integrations = build_integrations(mapping, builder);
    let models = build_models(mapping, builder);
    let runtime = build_runtime(mapping, builder);
    let api_policies = build_api_policies(mapping, builder);
    let compilation = build_compilation(mapping, builder);
    let browser = build_browser(mapping, builder);
    let frontend = build_frontend(mapping, builder);
    let frontend_state = build_frontend_state(mapping, builder);
    let filesystem = build_filesystem(mapping, builder);
    let databases = build_databases(mapping, builder);
    let commands = build_commands(mapping, builder);

    for (name, model) in &models {
        if !integrations.contains_key(&model.integration) {
            builder.error(
                &format!("models.{name}"),
                format!(
                    "unknown integration `{}`; declare it under `integrations:`",
                    model.integration
                ),
            );
        }
    }
    if let Some(default_model) = &runtime.default_model {
        if !models.contains_key(default_model) {
            builder.error(
                "runtime",
                format!("`default_model` refers to unknown model profile `{default_model}`"),
            );
        }
    }

    Some(ProjectConfig {
        version,
        language,
        sources,
        integrations,
        models,
        runtime,
        api_policies,
        compilation,
        browser,
        frontend,
        frontend_state,
        filesystem,
        databases,
        commands,
    })
}

fn read_version(mapping: &Mapping, builder: &mut Builder<'_>) -> u32 {
    let value = match mapping_field(mapping, "version") {
        None => return ProjectConfig::SUPPORTED_VERSION,
        Some(value) => value,
    };
    let version = match value {
        Value::Number(number) => match number.as_u64() {
            Some(number) => number as u32,
            None => {
                builder.error("axiom.yaml", "`version` must be a non-negative integer");
                return ProjectConfig::SUPPORTED_VERSION;
            }
        },
        other => {
            builder.error(
                "axiom.yaml",
                format!("`version` must be an integer, found {}", type_name(other)),
            );
            return ProjectConfig::SUPPORTED_VERSION;
        }
    };
    if version != ProjectConfig::SUPPORTED_VERSION {
        builder.error(
            "axiom.yaml",
            format!(
                "unsupported configuration version `{version}`; this build supports version {}",
                ProjectConfig::SUPPORTED_VERSION
            ),
        );
    }
    version
}

fn read_language(mapping: &Mapping, builder: &mut Builder<'_>) -> Option<String> {
    let language = optional_string(mapping, "axiom.yaml", "language", builder)?;
    if language != ProjectConfig::SUPPORTED_LANGUAGE {
        builder.error(
            "axiom.yaml",
            format!(
                "unsupported language version `{language}`; this build supports `{}`",
                ProjectConfig::SUPPORTED_LANGUAGE
            ),
        );
    }
    Some(language)
}

fn build_integrations(
    mapping: &Mapping,
    builder: &mut Builder<'_>,
) -> BTreeMap<String, IntegrationConfig> {
    let mut integrations = BTreeMap::new();
    let Some(value) = mapping_field(mapping, "integrations") else {
        return integrations;
    };
    let Some(entries) = expect_mapping(value, "integrations", builder) else {
        return integrations;
    };

    for (key, entry) in entries {
        let Some(id) = key.as_str() else {
            builder.error("integrations", "integration names must be strings");
            continue;
        };
        let label = format!("integrations.{id}");
        if !is_ident(id) {
            builder.error(
                &label,
                "invalid integration name; use letters, digits, and underscores",
            );
        }
        if let Some(integration) = build_integration(entry, &label, builder) {
            integrations.insert(id.to_string(), integration);
        }
    }
    integrations
}

fn build_integration(
    value: &Value,
    label: &str,
    builder: &mut Builder<'_>,
) -> Option<IntegrationConfig> {
    let mapping = expect_mapping(value, label, builder)?;
    check_allowed(
        mapping,
        &[
            "adapter",
            "dialect",
            "base_url",
            "api_key",
            "api_key_optional",
            "timeout_seconds",
            "headers",
            "allowed_data_classes",
        ],
        label,
        builder,
    );

    let adapter = required_string(mapping, label, "adapter", builder)?;
    let dialect = optional_string(mapping, label, "dialect", builder);
    let base_url = required_string(mapping, label, "base_url", builder)?;
    let api_key_optional = optional_bool(mapping, label, "api_key_optional", builder, false);
    let timeout_seconds = optional_u64(mapping, label, "timeout_seconds", builder);
    let headers = build_headers(mapping, label, builder);
    let allowed_data_classes = build_allowed_data_classes(mapping, label, builder);
    let api_key = build_api_key(mapping, label, api_key_optional, builder);

    Some(IntegrationConfig {
        adapter,
        dialect,
        base_url,
        api_key,
        api_key_optional,
        timeout_seconds,
        headers,
        allowed_data_classes,
    })
}

fn build_allowed_data_classes(
    mapping: &Mapping,
    label: &str,
    builder: &mut Builder<'_>,
) -> Vec<String> {
    let mut classes = build_string_list(mapping, label, "allowed_data_classes", builder);
    for class in &classes {
        match class.as_str() {
            "public" | "internal" | "pii" => {}
            "secret" => builder.error(
                label,
                "`allowed_data_classes` cannot contain `secret`; secret data never leaves the process",
            ),
            other => builder.error(
                label,
                format!("unknown data class `{other}` (known: public, internal, pii)"),
            ),
        }
    }
    if classes.is_empty() {
        classes.push("public".to_string());
    }
    classes
}

fn build_api_key(
    mapping: &Mapping,
    label: &str,
    api_key_optional: bool,
    builder: &mut Builder<'_>,
) -> Option<Secret> {
    let Some(value) = mapping_field(mapping, "api_key") else {
        if !api_key_optional {
            builder.warn(
                label,
                "`api_key` is not set; add one or mark the integration `api_key_optional: true` if the endpoint needs no credentials",
            );
        }
        return None;
    };
    let raw = scalar_string(value, label, "api_key", builder)?;
    match expand(&raw, builder.env) {
        Ok(expanded) if !expanded.is_empty() => Some(Secret::new(expanded)),
        Ok(_) if api_key_optional => {
            builder.warn(
                label,
                "`api_key` expanded to an empty value; continuing because `api_key_optional: true`",
            );
            None
        }
        Ok(_) => {
            builder.error(label, "`api_key` expanded to an empty value");
            None
        }
        Err(error) if api_key_optional => {
            builder.warn(
                label,
                format!("`api_key` is not configured ({error}); continuing because `api_key_optional: true`"),
            );
            None
        }
        Err(error) => {
            builder.error(label, format!("`api_key`: {error}"));
            None
        }
    }
}

fn build_headers(
    mapping: &Mapping,
    label: &str,
    builder: &mut Builder<'_>,
) -> BTreeMap<String, String> {
    let mut headers = BTreeMap::new();
    let Some(value) = mapping_field(mapping, "headers") else {
        return headers;
    };
    let header_label = format!("{label}.headers");
    let Some(entries) = expect_mapping(value, &header_label, builder) else {
        return headers;
    };
    for (key, entry) in entries {
        let Some(name) = key.as_str() else {
            builder.error(&header_label, "header names must be strings");
            continue;
        };
        let Some(raw) = entry.as_str() else {
            builder.error(&header_label, format!("`{name}` must be a string"));
            continue;
        };
        if let Some(expanded) = builder.expand(&header_label, name, raw) {
            headers.insert(name.to_string(), expanded);
        }
    }
    headers
}

fn build_models(mapping: &Mapping, builder: &mut Builder<'_>) -> BTreeMap<String, ModelConfig> {
    let mut models = BTreeMap::new();
    let Some(value) = mapping_field(mapping, "models") else {
        return models;
    };
    let Some(entries) = expect_mapping(value, "models", builder) else {
        return models;
    };

    for (key, entry) in entries {
        let Some(id) = key.as_str() else {
            builder.error("models", "model profile names must be strings");
            continue;
        };
        let label = format!("models.{id}");
        if !is_ident(id) {
            builder.error(
                &label,
                "invalid model profile name; use letters, digits, and underscores",
            );
        }
        if let Some(model) = build_model(entry, &label, builder) {
            models.insert(id.to_string(), model);
        }
    }
    models
}

fn build_model(value: &Value, label: &str, builder: &mut Builder<'_>) -> Option<ModelConfig> {
    let mapping = expect_mapping(value, label, builder)?;
    check_allowed(
        mapping,
        &["integration", "model", "capabilities"],
        label,
        builder,
    );

    let integration = required_string(mapping, label, "integration", builder)?;
    let model = required_string(mapping, label, "model", builder)?;
    let capabilities = build_capabilities(mapping, label, builder);

    Some(ModelConfig {
        integration,
        model,
        capabilities,
    })
}

fn build_capabilities(
    mapping: &Mapping,
    label: &str,
    builder: &mut Builder<'_>,
) -> Vec<Capability> {
    let mut capabilities = Vec::new();
    let Some(value) = mapping_field(mapping, "capabilities") else {
        return capabilities;
    };
    match value {
        Value::Null => {}
        Value::Sequence(items) => {
            for item in items {
                let Some(name) = item.as_str() else {
                    builder.error(label, "`capabilities` entries must be strings");
                    continue;
                };
                match Capability::parse(name) {
                    Some(capability) => capabilities.push(capability),
                    None => {
                        let known = Capability::ALL
                            .iter()
                            .map(|c| c.as_str())
                            .collect::<Vec<_>>()
                            .join(", ");
                        builder.error(
                            label,
                            format!("unknown capability `{name}` (known: {known})"),
                        );
                    }
                }
            }
        }
        other => builder.error(
            label,
            format!(
                "`capabilities` must be a sequence, found {}",
                type_name(other)
            ),
        ),
    }
    capabilities
}

fn build_runtime(mapping: &Mapping, builder: &mut Builder<'_>) -> RuntimeConfig {
    let mut runtime = RuntimeConfig::default();
    let Some(value) = mapping_field(mapping, "runtime") else {
        return runtime;
    };
    let Some(entries) = expect_mapping(value, "runtime", builder) else {
        return runtime;
    };
    check_allowed(
        entries,
        &[
            "default_model",
            "max_parallel_agents",
            "max_retries",
            "request_timeout_seconds",
            "max_model_calls",
            "max_wall_clock_seconds",
            "max_agent_attempts",
            "max_run_cost",
        ],
        "runtime",
        builder,
    );

    runtime.default_model = optional_string(entries, "runtime", "default_model", builder);
    runtime.max_parallel_agents =
        optional_bounded_u32(entries, "runtime", "max_parallel_agents", 1, builder);
    runtime.max_retries = optional_u32(entries, "runtime", "max_retries", builder);
    runtime.request_timeout_seconds =
        optional_u64(entries, "runtime", "request_timeout_seconds", builder);
    runtime.max_model_calls = optional_u64(entries, "runtime", "max_model_calls", builder);
    runtime.max_wall_clock_seconds =
        optional_u64(entries, "runtime", "max_wall_clock_seconds", builder);
    runtime.max_agent_attempts =
        optional_bounded_u32(entries, "runtime", "max_agent_attempts", 1, builder);
    runtime.max_run_cost = optional_f64(entries, "runtime", "max_run_cost", builder);
    runtime
}

fn build_filesystem(mapping: &Mapping, builder: &mut Builder<'_>) -> FilesystemConfig {
    let mut filesystem = FilesystemConfig::default();
    let Some(value) = mapping_field(mapping, "filesystem") else {
        return filesystem;
    };
    let Some(entries) = expect_mapping(value, "filesystem", builder) else {
        return filesystem;
    };
    check_allowed(
        entries,
        &[
            "project_root",
            "default_agent_isolation",
            "max_read_bytes",
            "max_write_bytes",
            "max_file_bytes",
            "max_files_per_task",
            "protect_vcs_metadata",
        ],
        "filesystem",
        builder,
    );

    filesystem.project_root = optional_string(entries, "filesystem", "project_root", builder);
    filesystem.default_agent_isolation =
        optional_string(entries, "filesystem", "default_agent_isolation", builder);
    filesystem.max_read_bytes = optional_u64(entries, "filesystem", "max_read_bytes", builder);
    filesystem.max_write_bytes = optional_u64(entries, "filesystem", "max_write_bytes", builder);
    filesystem.max_file_bytes = optional_u64(entries, "filesystem", "max_file_bytes", builder);
    filesystem.max_files_per_task =
        optional_u64(entries, "filesystem", "max_files_per_task", builder);
    filesystem.protect_vcs_metadata =
        optional_bool_value(entries, "filesystem", "protect_vcs_metadata", builder);
    filesystem
}

fn build_api_policies(
    mapping: &Mapping,
    builder: &mut Builder<'_>,
) -> BTreeMap<String, ApiPolicyConfig> {
    let mut policies = BTreeMap::new();
    let Some(value) = mapping_field(mapping, "api_policies") else {
        return policies;
    };
    let Some(entries) = expect_mapping(value, "api_policies", builder) else {
        return policies;
    };

    for (key, entry) in entries {
        let Some(id) = key.as_str() else {
            builder.error("api_policies", "API policy names must be strings");
            continue;
        };
        let label = format!("api_policies.{id}");
        if let Some(policy) = build_api_policy(entry, &label, builder) {
            policies.insert(id.to_string(), policy);
        }
    }
    policies
}

fn build_api_policy(
    value: &Value,
    label: &str,
    builder: &mut Builder<'_>,
) -> Option<ApiPolicyConfig> {
    let mapping = expect_mapping(value, label, builder)?;
    check_allowed(
        mapping,
        &[
            "version",
            "authentication",
            "requires",
            "data_class",
            "cache",
        ],
        label,
        builder,
    );

    let policy = ApiPolicyConfig {
        version: optional_string(mapping, label, "version", builder),
        authentication: optional_string(mapping, label, "authentication", builder),
        requires: build_string_list(mapping, label, "requires", builder),
        data_class: optional_string(mapping, label, "data_class", builder),
        cache: optional_string(mapping, label, "cache", builder),
    };
    if let Some(data_class) = &policy.data_class {
        if !matches!(data_class.as_str(), "public" | "internal" | "pii") {
            builder.error(
                label,
                format!("unknown data class `{data_class}` (known: public, internal, pii)"),
            );
        }
    }
    Some(policy)
}

fn build_compilation(mapping: &Mapping, builder: &mut Builder<'_>) -> CompilationConfig {
    let mut compilation = CompilationConfig::default();
    let Some(value) = mapping_field(mapping, "compilation") else {
        return compilation;
    };
    let Some(entries) = expect_mapping(value, "compilation", builder) else {
        return compilation;
    };
    check_allowed(
        entries,
        &["targets", "entrypoints", "wasm"],
        "compilation",
        builder,
    );

    compilation.targets = build_string_list(entries, "compilation", "targets", builder);
    for target in &compilation.targets {
        if !ProjectConfig::TARGETS.contains(&target.as_str()) {
            builder.error(
                "compilation",
                format!(
                    "unknown target `{target}` (known: {})",
                    ProjectConfig::TARGETS.join(", ")
                ),
            );
        }
    }
    compilation.entrypoints = build_entrypoints(entries, builder);
    compilation.wasm = build_wasm(entries, builder);
    compilation
}

fn build_entrypoints(
    entries: &Mapping,
    builder: &mut Builder<'_>,
) -> BTreeMap<String, EntryPointConfig> {
    let mut entrypoints = BTreeMap::new();
    let Some(value) = mapping_field(entries, "entrypoints") else {
        return entrypoints;
    };
    let Some(map) = expect_mapping(value, "compilation.entrypoints", builder) else {
        return entrypoints;
    };

    for (key, entry) in map {
        let Some(target) = key.as_str() else {
            builder.error("compilation.entrypoints", "target names must be strings");
            continue;
        };
        let label = format!("compilation.entrypoints.{target}");
        if !ProjectConfig::TARGETS.contains(&target) {
            builder.error(
                &label,
                format!(
                    "unknown target `{target}` (known: {})",
                    ProjectConfig::TARGETS.join(", ")
                ),
            );
            continue;
        }
        let Some(mapping) = expect_mapping(entry, &label, builder) else {
            continue;
        };
        let allowed: &[&str] = if target == "server" {
            &["module"]
        } else {
            &["module", "init", "render", "handle_event"]
        };
        check_allowed(mapping, allowed, &label, builder);

        entrypoints.insert(
            target.to_string(),
            EntryPointConfig {
                target: target.to_string(),
                module: required_string(mapping, &label, "module", builder),
                init: optional_string(mapping, &label, "init", builder),
                render: optional_string(mapping, &label, "render", builder),
                handle_event: optional_string(mapping, &label, "handle_event", builder),
            },
        );
    }
    entrypoints
}

fn build_wasm(entries: &Mapping, builder: &mut Builder<'_>) -> WasmConfig {
    let mut wasm = WasmConfig::default();
    let Some(value) = mapping_field(entries, "wasm") else {
        return wasm;
    };
    let Some(mapping) = expect_mapping(value, "compilation.wasm", builder) else {
        return wasm;
    };
    check_allowed(
        mapping,
        &[
            "target",
            "host_api_version",
            "max_module_bytes",
            "reproducible",
        ],
        "compilation.wasm",
        builder,
    );

    wasm.target = optional_string(mapping, "compilation.wasm", "target", builder);
    wasm.host_api_version = optional_u32(mapping, "compilation.wasm", "host_api_version", builder);
    if let Some(version) = wasm.host_api_version {
        if version != axiom_browser_host_version() {
            builder.error(
                "compilation.wasm",
                format!(
                    "unsupported `host_api_version` {version}; this build supports {}",
                    axiom_browser_host_version()
                ),
            );
        }
    }
    wasm.max_module_bytes = optional_u64(mapping, "compilation.wasm", "max_module_bytes", builder);
    wasm.reproducible = optional_bool_value(mapping, "compilation.wasm", "reproducible", builder);
    wasm
}

/// The host ABI version this build implements; kept in one place so config and
/// the browser host stay in sync without a dependency edge.
fn axiom_browser_host_version() -> u32 {
    1
}

fn build_browser(mapping: &Mapping, builder: &mut Builder<'_>) -> BrowserConfig {
    let mut browser = BrowserConfig::default();
    let Some(value) = mapping_field(mapping, "browser") else {
        return browser;
    };
    let Some(entries) = expect_mapping(value, "browser", builder) else {
        return browser;
    };
    check_allowed(
        entries,
        &[
            "isolation",
            "max_memory_pages",
            "max_compute_ms_per_event",
            "max_ui_nodes",
            "max_event_bytes",
            "max_storage_bytes",
            "max_snapshot_bytes",
            "preserve_previous_release_on_failure",
            "state",
            "data_cache",
        ],
        "browser",
        builder,
    );

    browser.isolation = optional_string(entries, "browser", "isolation", builder);
    browser.max_memory_pages = optional_u64(entries, "browser", "max_memory_pages", builder);
    browser.max_compute_ms_per_event =
        optional_u64(entries, "browser", "max_compute_ms_per_event", builder);
    browser.max_ui_nodes = optional_u64(entries, "browser", "max_ui_nodes", builder);
    browser.max_event_bytes = optional_u64(entries, "browser", "max_event_bytes", builder);
    browser.max_storage_bytes = optional_u64(entries, "browser", "max_storage_bytes", builder);
    browser.max_snapshot_bytes = optional_u64(entries, "browser", "max_snapshot_bytes", builder);
    browser.preserve_previous_release_on_failure = optional_bool_value(
        entries,
        "browser",
        "preserve_previous_release_on_failure",
        builder,
    );
    browser.state = build_browser_state(entries, builder);
    browser.data_cache = build_data_cache(entries, builder);
    browser
}

fn build_browser_state(entries: &Mapping, builder: &mut Builder<'_>) -> BrowserStateConfig {
    let mut state = BrowserStateConfig::default();
    let Some(value) = mapping_field(entries, "state") else {
        return state;
    };
    let Some(mapping) = expect_mapping(value, "browser.state", builder) else {
        return state;
    };
    check_allowed(
        mapping,
        &["default_scope", "default_persistence"],
        "browser.state",
        builder,
    );
    state.default_scope = optional_string(mapping, "browser.state", "default_scope", builder);
    state.default_persistence =
        optional_string(mapping, "browser.state", "default_persistence", builder);
    state
}

fn build_data_cache(entries: &Mapping, builder: &mut Builder<'_>) -> DataCacheConfig {
    let mut cache = DataCacheConfig::default();
    let Some(value) = mapping_field(entries, "data_cache") else {
        return cache;
    };
    let Some(mapping) = expect_mapping(value, "browser.data_cache", builder) else {
        return cache;
    };
    check_allowed(
        mapping,
        &[
            "enabled",
            "max_entries",
            "max_bytes",
            "default_ttl_seconds",
            "serve_stale_on_error",
        ],
        "browser.data_cache",
        builder,
    );
    cache.enabled = optional_bool_value(mapping, "browser.data_cache", "enabled", builder);
    cache.max_entries = optional_u64(mapping, "browser.data_cache", "max_entries", builder);
    cache.max_bytes = optional_u64(mapping, "browser.data_cache", "max_bytes", builder);
    cache.default_ttl_seconds = optional_u64(
        mapping,
        "browser.data_cache",
        "default_ttl_seconds",
        builder,
    );
    cache.serve_stale_on_error = optional_bool_value(
        mapping,
        "browser.data_cache",
        "serve_stale_on_error",
        builder,
    );
    cache
}

fn build_frontend(mapping: &Mapping, builder: &mut Builder<'_>) -> FrontendConfig {
    let mut frontend = FrontendConfig::default();
    let Some(value) = mapping_field(mapping, "frontend") else {
        return frontend;
    };
    let Some(entries) = expect_mapping(value, "frontend", builder) else {
        return frontend;
    };
    check_allowed(entries, &["registry", "build_cache"], "frontend", builder);

    frontend.registry = build_registry(entries, builder);
    frontend.build_cache = build_build_cache(entries, builder);
    frontend
}

fn build_registry(entries: &Mapping, builder: &mut Builder<'_>) -> RegistryConfig {
    let mut registry = RegistryConfig::default();
    let Some(value) = mapping_field(entries, "registry") else {
        return registry;
    };
    let Some(mapping) = expect_mapping(value, "frontend.registry", builder) else {
        return registry;
    };
    check_allowed(
        mapping,
        &[
            "content_addressed",
            "retain_accepted_versions",
            "require_pinned_references",
        ],
        "frontend.registry",
        builder,
    );
    registry.content_addressed =
        optional_bool_value(mapping, "frontend.registry", "content_addressed", builder);
    registry.retain_accepted_versions = optional_u64(
        mapping,
        "frontend.registry",
        "retain_accepted_versions",
        builder,
    );
    registry.require_pinned_references = optional_bool_value(
        mapping,
        "frontend.registry",
        "require_pinned_references",
        builder,
    );
    registry
}

fn build_build_cache(entries: &Mapping, builder: &mut Builder<'_>) -> BuildCacheConfig {
    let mut cache = BuildCacheConfig::default();
    let Some(value) = mapping_field(entries, "build_cache") else {
        return cache;
    };
    let Some(mapping) = expect_mapping(value, "frontend.build_cache", builder) else {
        return cache;
    };
    check_allowed(
        mapping,
        &["enabled", "max_bytes"],
        "frontend.build_cache",
        builder,
    );
    cache.enabled = optional_bool_value(mapping, "frontend.build_cache", "enabled", builder);
    cache.max_bytes = optional_u64(mapping, "frontend.build_cache", "max_bytes", builder);
    cache
}

/// Parse the optional top-level `frontend_state` block.
///
/// Validation is structural: ids/type names, a positive schema version,
/// the closed scope/persistence vocabulary, and a contiguous, unique
/// migration chain ending at `schema_version`.
fn build_frontend_state(
    mapping: &Mapping,
    builder: &mut Builder<'_>,
) -> Option<FrontendStateConfig> {
    let value = mapping_field(mapping, "frontend_state")?;
    let label = "frontend_state";
    let entries = expect_mapping(value, label, builder)?;
    check_allowed(
        entries,
        &[
            "id",
            "type",
            "schema_version",
            "scope",
            "persistence",
            "migrations",
        ],
        label,
        builder,
    );

    let Some(id) = required_string(entries, label, "id", builder) else {
        return Some(FrontendStateConfig::default());
    };
    if !is_dotted_name(&id) {
        builder.error(
            label,
            "`id` must be a dotted name using letters, digits, and underscores",
        );
    }
    let record_type = required_string(entries, label, "type", builder).unwrap_or_default();
    if !is_dotted_name(&record_type) {
        builder.error(
            label,
            "`type` must be a dotted record type name using letters, digits, and underscores",
        );
    }
    let schema_version = match optional_u32(entries, label, "schema_version", builder) {
        Some(version) if version >= 1 => version,
        Some(_) => {
            builder.error(label, "`schema_version` must be at least 1");
            1
        }
        None => {
            builder.error(label, "missing required field `schema_version`");
            1
        }
    };

    let scope = required_string(entries, label, "scope", builder).unwrap_or_default();
    if !matches!(scope.as_str(), "component" | "route" | "application") {
        builder.error(
            label,
            "`scope` must be one of `component`, `route`, or `application`",
        );
    }
    let persistence = required_string(entries, label, "persistence", builder).unwrap_or_default();
    if !matches!(persistence.as_str(), "none" | "session" | "persistent") {
        builder.error(
            label,
            "`persistence` must be one of `none`, `session`, or `persistent`",
        );
    }

    let mut migrations = Vec::new();
    if let Some(value) = mapping_field(entries, "migrations") {
        match value {
            Value::Null => {}
            Value::Sequence(items) => {
                let mut previous: Option<u32> = None;
                for (index, item) in items.iter().enumerate() {
                    let item_label = format!("{label}.migrations[{index}]");
                    let Some(entry) = expect_mapping(item, &item_label, builder) else {
                        continue;
                    };
                    check_allowed(entry, &["from", "to", "function"], &item_label, builder);
                    let from = optional_u32(entry, &item_label, "from", builder).unwrap_or(0);
                    let to = optional_u32(entry, &item_label, "to", builder).unwrap_or(0);
                    if to != from + 1 {
                        builder.error(
                            &item_label,
                            "a migration must advance exactly one version (`to` = `from` + 1)",
                        );
                    }
                    if let Some(expected) = previous {
                        if from != expected {
                            builder.error(
                                &item_label,
                                format!(
                                    "migrations must be contiguous; expected `from: {expected}`"
                                ),
                            );
                        }
                    } else if from != 1 {
                        builder.error(&item_label, "the first migration must start at version 1");
                    }
                    previous = Some(to);
                    let function = required_string(entry, &item_label, "function", builder)
                        .unwrap_or_default();
                    if !is_dotted_name(&function) {
                        builder.error(&item_label, "`function` must be a pure function name");
                    }
                    migrations.push(StateMigrationConfig { from, to, function });
                }
                if let Some(last) = previous {
                    if last != schema_version {
                        builder.error(
                            label,
                            format!(
                                "migrations must reach `schema_version` {schema_version}, but end at {last}"
                            ),
                        );
                    }
                }
            }
            other => builder.error(
                label,
                format!(
                    "`migrations` must be a sequence, found {}",
                    type_name(other)
                ),
            ),
        }
    }

    Some(FrontendStateConfig {
        id,
        record_type,
        schema_version,
        scope,
        persistence,
        migrations,
    })
}

fn build_databases(
    mapping: &Mapping,
    builder: &mut Builder<'_>,
) -> BTreeMap<String, DatabaseConfig> {
    let mut databases = BTreeMap::new();
    let Some(value) = mapping_field(mapping, "databases") else {
        return databases;
    };
    let Some(entries) = expect_mapping(value, "databases", builder) else {
        return databases;
    };

    for (key, entry) in entries {
        let Some(id) = key.as_str() else {
            builder.error("databases", "database names must be strings");
            continue;
        };
        let label = format!("databases.{id}");
        if !is_ident(id) {
            builder.error(
                &label,
                "invalid database name; use letters, digits, and underscores",
            );
        }
        if let Some(database) = build_database(entry, &label, builder) {
            databases.insert(id.to_string(), database);
        }
    }
    databases
}

fn build_database(value: &Value, label: &str, builder: &mut Builder<'_>) -> Option<DatabaseConfig> {
    let mapping = expect_mapping(value, label, builder)?;
    check_allowed(
        mapping,
        &[
            "engine",
            "path",
            "url",
            "data_class",
            "allowed_tables",
            "max_rows",
            "timeout_seconds",
            "migrations",
            "destructive_migrations",
            "allow_private_network",
            "require_write_approval",
        ],
        label,
        builder,
    );

    let engine = required_string(mapping, label, "engine", builder)?;
    if !matches!(engine.as_str(), "sqlite" | "postgres") {
        builder.error(
            label,
            format!("unknown engine `{engine}`; use sqlite or postgres"),
        );
        return None;
    }

    let path = optional_string(mapping, label, "path", builder);
    let url = optional_string(mapping, label, "url", builder).map(Secret::new);

    if engine == "sqlite" {
        match &path {
            Some(path) if is_safe_relative_path(path) => {}
            Some(_) => {
                builder.error(
                    label,
                    "`path` must be a relative path inside the project root (no `..`)",
                );
            }
            None => builder.error(label, "sqlite databases require `path`"),
        }
        if url.is_some() {
            builder.error(label, "sqlite databases must not set `url`");
        }
    } else {
        match &url {
            Some(url)
                if url.expose().starts_with("postgres://")
                    || url.expose().starts_with("postgresql://") => {}
            Some(_) => builder.error(label, "`url` must use the postgres:// scheme"),
            None => builder.error(label, "postgres databases require `url`"),
        }
        if path.is_some() {
            builder.error(label, "postgres databases must not set `path`");
        }
    }

    let allowed_tables = build_string_list(mapping, label, "allowed_tables", builder);
    for table in &allowed_tables {
        if table.trim().is_empty() {
            builder.error(label, "`allowed_tables` entries must not be empty");
        }
    }

    Some(DatabaseConfig {
        engine,
        path,
        url,
        data_class: optional_string(mapping, label, "data_class", builder),
        allowed_tables,
        max_rows: optional_u64(mapping, label, "max_rows", builder),
        timeout_seconds: optional_u64(mapping, label, "timeout_seconds", builder),
        migrations: optional_string(mapping, label, "migrations", builder),
        destructive_migrations: optional_bool_value(
            mapping,
            label,
            "destructive_migrations",
            builder,
        ),
        allow_private_network: optional_bool_value(
            mapping,
            label,
            "allow_private_network",
            builder,
        ),
        require_write_approval: optional_bool_value(
            mapping,
            label,
            "require_write_approval",
            builder,
        ),
    })
}

fn is_safe_relative_path(path: &str) -> bool {
    !path.trim().is_empty()
        && !path.starts_with(['/', '\\'])
        && !path.contains("..")
        && !path.contains(':')
}

fn build_commands(
    mapping: &Mapping,
    builder: &mut Builder<'_>,
) -> BTreeMap<String, CommandProfile> {
    let mut commands = BTreeMap::new();
    let Some(value) = mapping_field(mapping, "commands") else {
        return commands;
    };
    let Some(entries) = expect_mapping(value, "commands", builder) else {
        return commands;
    };

    for (key, entry) in entries {
        let Some(id) = key.as_str() else {
            builder.error("commands", "command profile names must be strings");
            continue;
        };
        let label = format!("commands.{id}");
        if !is_ident(id) {
            builder.error(
                &label,
                "invalid command profile name; use letters, digits, and underscores",
            );
        }
        if let Some(profile) = build_command(entry, &label, builder) {
            commands.insert(id.to_string(), profile);
        }
    }
    commands
}

fn build_command(value: &Value, label: &str, builder: &mut Builder<'_>) -> Option<CommandProfile> {
    let mapping = expect_mapping(value, label, builder)?;
    check_allowed(
        mapping,
        &[
            "executable",
            "args",
            "working_directory",
            "timeout_seconds",
            "network",
        ],
        label,
        builder,
    );

    let executable = required_string(mapping, label, "executable", builder)?;
    let args = build_string_list(mapping, label, "args", builder);
    let working_directory = optional_string(mapping, label, "working_directory", builder);
    let timeout_seconds = optional_u64(mapping, label, "timeout_seconds", builder);
    let network = optional_bool_value(mapping, label, "network", builder);

    Some(CommandProfile {
        executable,
        args,
        working_directory,
        timeout_seconds,
        network,
    })
}

fn build_string_list(
    mapping: &Mapping,
    label: &str,
    field: &str,
    builder: &mut Builder<'_>,
) -> Vec<String> {
    let mut values = Vec::new();
    let Some(value) = mapping_field(mapping, field) else {
        return values;
    };
    match value {
        Value::Null => {}
        Value::Sequence(items) => {
            for item in items {
                let Some(raw) = item.as_str() else {
                    builder.error(label, format!("`{field}` entries must be strings"));
                    continue;
                };
                if let Some(expanded) = builder.expand(label, field, raw) {
                    values.push(expanded);
                }
            }
        }
        other => builder.error(
            label,
            format!("`{field}` must be a sequence, found {}", type_name(other)),
        ),
    }
    values
}

// --- value helpers -------------------------------------------------------

fn mapping_field<'a>(mapping: &'a Mapping, name: &str) -> Option<&'a Value> {
    mapping
        .iter()
        .find(|(key, _)| key.as_str() == Some(name))
        .map(|(_, value)| value)
}

fn expect_mapping<'a>(
    value: &'a Value,
    label: &str,
    builder: &mut Builder<'_>,
) -> Option<&'a Mapping> {
    match value {
        Value::Mapping(mapping) => Some(mapping),
        other => {
            builder.error(
                label,
                format!("expected a mapping, found {}", type_name(other)),
            );
            None
        }
    }
}

fn scalar_string(
    value: &Value,
    label: &str,
    field: &str,
    builder: &mut Builder<'_>,
) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        other => {
            builder.error(
                label,
                format!("`{field}` must be a string, found {}", type_name(other)),
            );
            None
        }
    }
}

fn required_string(
    mapping: &Mapping,
    label: &str,
    field: &str,
    builder: &mut Builder<'_>,
) -> Option<String> {
    let Some(value) = mapping_field(mapping, field) else {
        builder.error(label, format!("missing required field `{field}`"));
        return None;
    };
    let raw = scalar_string(value, label, field, builder)?;
    let expanded = builder.expand(label, field, &raw)?;
    if expanded.trim().is_empty() {
        builder.error(label, format!("`{field}` must not be empty"));
        None
    } else {
        Some(expanded)
    }
}

fn optional_string(
    mapping: &Mapping,
    label: &str,
    field: &str,
    builder: &mut Builder<'_>,
) -> Option<String> {
    let value = mapping_field(mapping, field)?;
    let raw = scalar_string(value, label, field, builder)?;
    builder.expand(label, field, &raw)
}

fn optional_bool(
    mapping: &Mapping,
    label: &str,
    field: &str,
    builder: &mut Builder<'_>,
    default: bool,
) -> bool {
    optional_bool_value(mapping, label, field, builder).unwrap_or(default)
}

fn optional_bool_value(
    mapping: &Mapping,
    label: &str,
    field: &str,
    builder: &mut Builder<'_>,
) -> Option<bool> {
    match mapping_field(mapping, field) {
        None => None,
        Some(Value::Bool(value)) => Some(*value),
        Some(other) => {
            builder.error(
                label,
                format!("`{field}` must be a boolean, found {}", type_name(other)),
            );
            None
        }
    }
}

fn optional_u64(
    mapping: &Mapping,
    label: &str,
    field: &str,
    builder: &mut Builder<'_>,
) -> Option<u64> {
    match mapping_field(mapping, field) {
        None => None,
        Some(Value::Number(number)) => match number.as_u64() {
            Some(value) => Some(value),
            None => {
                builder.error(label, format!("`{field}` must be a non-negative integer"));
                None
            }
        },
        Some(other) => {
            builder.error(
                label,
                format!("`{field}` must be an integer, found {}", type_name(other)),
            );
            None
        }
    }
}

/// Reads an optional non-negative number (`null` disables the ceiling).
fn optional_f64(
    mapping: &Mapping,
    label: &str,
    field: &str,
    builder: &mut Builder<'_>,
) -> Option<f64> {
    match mapping_field(mapping, field) {
        None | Some(Value::Null) => None,
        Some(Value::Number(number)) => match number.as_f64() {
            Some(value) if value >= 0.0 => Some(value),
            Some(_) => {
                builder.error(label, format!("`{field}` must be non-negative"));
                None
            }
            None => {
                builder.error(label, format!("`{field}` must be a number"));
                None
            }
        },
        Some(other) => {
            builder.error(
                label,
                format!("`{field}` must be a number, found {}", type_name(other)),
            );
            None
        }
    }
}

fn optional_u32(
    mapping: &Mapping,
    label: &str,
    field: &str,
    builder: &mut Builder<'_>,
) -> Option<u32> {
    optional_u64(mapping, label, field, builder).map(|value| value.min(u32::MAX as u64) as u32)
}

fn optional_bounded_u32(
    mapping: &Mapping,
    label: &str,
    field: &str,
    minimum: u32,
    builder: &mut Builder<'_>,
) -> Option<u32> {
    let value = optional_u32(mapping, label, field, builder)?;
    if value < minimum {
        builder.error(label, format!("`{field}` must be at least {minimum}"));
        return None;
    }
    Some(value)
}

fn check_allowed(mapping: &Mapping, allowed: &[&str], label: &str, builder: &mut Builder<'_>) {
    for key in mapping.keys() {
        match key.as_str() {
            Some(name) if allowed.contains(&name) => {}
            Some(name) => builder.error(
                label,
                format!("unknown key `{name}` (allowed: {})", allowed.join(", ")),
            ),
            None => builder.error(label, "mapping keys must be strings"),
        }
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Sequence(_) => "a sequence",
        Value::Mapping(_) => "a mapping",
        _ => "an unsupported value",
    }
}

fn is_ident(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_dotted_name(text: &str) -> bool {
    !text.is_empty() && text.split('.').all(is_ident)
}

fn yaml_error_span(text: &str, error: &yaml_serde::Error) -> Span {
    match error.location() {
        Some(location) => Span::empty(offset_of(text, location.line(), location.column())),
        None => Span::empty(0),
    }
}

/// Byte offset of a 1-based `(line, column)` position, clamped to the text.
fn offset_of(text: &str, line: usize, column: usize) -> u32 {
    let mut line_start = 0usize;
    let mut current = 1usize;
    for (idx, ch) in text.char_indices() {
        if current == line {
            line_start = idx;
            break;
        }
        if ch == '\n' {
            current += 1;
            line_start = idx + 1;
        }
    }

    let mut offset = line_start;
    for (count, ch) in text[line_start..].chars().enumerate() {
        if count + 1 >= column {
            break;
        }
        offset += ch.len_utf8();
    }
    offset.min(text.len()) as u32
}
