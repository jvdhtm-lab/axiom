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
    Capability, CommandProfile, FilesystemConfig, IntegrationConfig, ModelConfig, ProjectConfig,
    RuntimeConfig, Secret,
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
        integrations: BTreeMap::new(),
        models: BTreeMap::new(),
        runtime: RuntimeConfig::default(),
        filesystem: FilesystemConfig::default(),
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
            "integrations",
            "models",
            "runtime",
            "filesystem",
            "commands",
        ],
        "axiom.yaml",
        builder,
    );

    let version = read_version(mapping, builder);
    let integrations = build_integrations(mapping, builder);
    let models = build_models(mapping, builder);
    let runtime = build_runtime(mapping, builder);
    let filesystem = build_filesystem(mapping, builder);
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
        integrations,
        models,
        runtime,
        filesystem,
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
            "base_url",
            "api_key",
            "api_key_optional",
            "timeout_seconds",
            "headers",
        ],
        label,
        builder,
    );

    let adapter = required_string(mapping, label, "adapter", builder)?;
    let base_url = required_string(mapping, label, "base_url", builder)?;
    let api_key_optional = optional_bool(mapping, label, "api_key_optional", builder, false);
    let timeout_seconds = optional_u64(mapping, label, "timeout_seconds", builder);
    let headers = build_headers(mapping, label, builder);
    let api_key = build_api_key(mapping, label, api_key_optional, builder);

    Some(IntegrationConfig {
        adapter,
        base_url,
        api_key,
        api_key_optional,
        timeout_seconds,
        headers,
    })
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
    filesystem.max_files_per_task =
        optional_u64(entries, "filesystem", "max_files_per_task", builder);
    filesystem.protect_vcs_metadata =
        optional_bool_value(entries, "filesystem", "protect_vcs_metadata", builder);
    filesystem
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
