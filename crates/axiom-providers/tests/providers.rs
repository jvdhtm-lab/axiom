//! Provider alias resolution and the offline mock provider.

use axiom_config::parse_config_with;
use axiom_config::ProjectConfig;
use axiom_providers::{resolve_spec, ModelRequest, ProviderError, ProviderRegistry};

const CONFIG: &str = "\
version: 1
integrations:
  claude:
    adapter: anthropic_messages
    base_url: https://api.anthropic.com
    api_key: ${TEST_ANTHROPIC_KEY}
  deepseek:
    adapter: openai_compatible
    base_url: https://api.deepseek.com
    api_key: ${TEST_DEEPSEEK_KEY}
models:
  reasoning:
    integration: claude
    model: claude-3-5
  coding:
    integration: deepseek
    model: deepseek-coder
";

fn env(name: &str) -> Option<String> {
    match name {
        "TEST_ANTHROPIC_KEY" | "TEST_DEEPSEEK_KEY" => Some("sk-test-secret".to_string()),
        _ => None,
    }
}

fn config() -> ProjectConfig {
    let report = parse_config_with("inline.yaml", CONFIG, &env);
    assert!(!report.has_errors(), "{:?}", report.diagnostics);
    report.config.expect("a resolved config")
}

#[test]
fn resolves_alias_to_vendor_neutral_spec() {
    let spec = resolve_spec(&config(), "reasoning").unwrap();
    assert_eq!(spec.alias, "reasoning");
    assert_eq!(spec.adapter, "anthropic_messages");
    assert_eq!(spec.model, "claude-3-5");
    assert_eq!(spec.base_url, "https://api.anthropic.com");
    assert!(spec.has_secret);
}

#[test]
fn switching_the_alias_changes_the_provider_without_source_edits() {
    let config = config();
    let reasoning = resolve_spec(&config, "reasoning").unwrap();
    let coding = resolve_spec(&config, "coding").unwrap();
    assert_ne!(reasoning.adapter, coding.adapter);
    assert_ne!(reasoning.model, coding.model);
}

#[test]
fn unknown_alias_is_refused() {
    let error = resolve_spec(&config(), "nope").unwrap_err();
    assert_eq!(error, ProviderError::UnknownModel("nope".to_string()));
}

#[test]
fn specs_never_expose_the_secret() {
    let spec = resolve_spec(&config(), "reasoning").unwrap();
    let rendered = format!("{spec:?}");
    assert!(!rendered.contains("sk-test-secret"));
    assert!(spec.has_secret);
}

#[test]
fn unsupported_adapter_is_refused() {
    let registry = ProviderRegistry::new();
    let error = registry.build(&config(), "coding").map(|_| ()).unwrap_err();
    assert_eq!(
        error,
        ProviderError::UnsupportedAdapter("openai_compatible".to_string())
    );
}

#[test]
fn mock_registry_builds_and_labels_every_adapter() {
    let registry = ProviderRegistry::with_mock();
    assert!(registry.is_supported("anthropic_messages"));
    assert!(registry.is_supported("openai_compatible"));
    assert_eq!(
        registry.adapters(),
        vec!["anthropic_messages", "openai_compatible"]
    );

    let provider = registry.build(&config(), "reasoning").unwrap();
    assert_eq!(provider.spec().model, "claude-3-5");
}

#[test]
fn mock_provider_is_deterministic_and_records_calls() {
    let registry = ProviderRegistry::with_mock();
    let mut provider = registry.build(&config(), "coding").unwrap();

    let mut request = ModelRequest::new("coding", "summarize the diff");
    request.max_tokens = Some(64);
    let response = provider.complete(&request).unwrap();
    assert_eq!(response.text, "mock:deepseek-coder");
    assert_eq!(response.model, "deepseek-coder");
    assert_eq!(response.usage.input_tokens, 3);

    let response = provider.complete(&request).unwrap();
    assert_eq!(response.text, "mock:deepseek-coder");
    assert_eq!(provider.calls().len(), 2);
}
