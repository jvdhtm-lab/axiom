//! Configuration loading, expansion, and validation tests. They run fully
//! offline against a stubbed environment.

use std::fs;
use std::path::{Path, PathBuf};

use axiom_config::{parse_config_with, Capability, ConfigReport};
use axiom_syntax::Source;

/// A deterministic environment stub so tests never depend on the real process
/// environment.
fn stub_env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |name: &str| {
        pairs
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| (*value).to_string())
    }
}

fn default_env() -> impl Fn(&str) -> Option<String> {
    stub_env(&[
        ("ANTHROPIC_API_KEY", "sk-ant-test-secret"),
        ("DEEPSEEK_API_KEY", "sk-ds-test-secret"),
        ("KIMI_API_KEY", "sk-kimi-test-secret"),
    ])
}

fn parse(name: &str, text: &str) -> ConfigReport {
    parse_config_with(name, text, &default_env())
}

fn fixtures(sub: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(sub)
}

fn messages(report: &ConfigReport) -> String {
    report
        .diagnostics
        .iter()
        .map(|d| d.message.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn valid_fixtures_load_without_errors() {
    let mut count = 0;
    for entry in fs::read_dir(fixtures("valid")).expect("valid fixtures dir") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        let report = parse(&path.display().to_string(), &text);
        assert_eq!(
            report.error_count(),
            0,
            "{} produced errors:\n{}",
            path.display(),
            messages(&report)
        );
        count += 1;
    }
    assert!(count >= 2);
}

#[test]
fn bundled_example_configuration_loads() {
    let text = include_str!("../../../examples/axiom.yaml");
    let report = parse("examples/axiom.yaml", text);
    assert_eq!(report.error_count(), 0, "{}", messages(&report));

    let config = report.config.expect("a resolved config");
    assert_eq!(config.integrations.len(), 4);
    assert_eq!(config.models.len(), 4);
    assert_eq!(config.runtime.default_model.as_deref(), Some("reasoning"));
    assert_eq!(config.runtime.max_parallel_agents, Some(4));
    assert_eq!(config.runtime.request_timeout_seconds, Some(120));
    assert_eq!(config.runtime.max_model_calls, Some(100));
    assert_eq!(config.runtime.max_wall_clock_seconds, Some(1800));
    assert_eq!(config.runtime.max_agent_attempts, Some(4));
    assert_eq!(config.runtime.max_run_cost, None);

    let reasoning = &config.models["reasoning"];
    assert_eq!(reasoning.integration, "claude");
    assert_eq!(
        reasoning.capabilities,
        vec![
            Capability::Chat,
            Capability::StructuredOutput,
            Capability::ToolCalling
        ]
    );

    let claude = &config.integrations["claude"];
    assert_eq!(claude.adapter, "anthropic_messages");
    assert_eq!(claude.base_url, "https://api.anthropic.com");
    assert_eq!(claude.timeout_seconds, Some(120));
    assert!(claude.api_key.is_some());

    let filesystem = &config.filesystem;
    assert_eq!(filesystem.project_root.as_deref(), Some("."));
    assert_eq!(
        filesystem.default_agent_isolation.as_deref(),
        Some("per_task")
    );
    assert_eq!(filesystem.max_read_bytes, Some(2_000_000));
    assert_eq!(filesystem.max_file_bytes, Some(2_000_000));
    assert_eq!(filesystem.max_files_per_task, Some(500));
    assert_eq!(filesystem.protect_vcs_metadata, Some(true));

    let cargo_check = &config.commands["cargo_check"];
    assert_eq!(cargo_check.executable, "cargo");
    assert_eq!(
        cargo_check.args,
        vec!["check", "--offline", "--workspace", "--all-targets"]
    );
    assert_eq!(cargo_check.network, Some(false));
    assert_eq!(cargo_check.timeout_seconds, Some(300));
}

#[test]
fn local_integration_without_a_key_warns_but_loads() {
    let text = include_str!("../../../examples/axiom.yaml");
    let report = parse("examples/axiom.yaml", text);
    assert!(report.warning_count() >= 1);
    assert!(messages(&report).contains("api_key"));
}

#[test]
fn unknown_integration_is_reported() {
    let text = fs::read_to_string(fixtures("invalid/unknown_integration.yaml")).unwrap();
    let report = parse("unknown_integration.yaml", &text);
    assert!(report.has_errors());
    assert!(messages(&report).contains("unknown integration"));
}

#[test]
fn unknown_model_default_is_reported() {
    let text = fs::read_to_string(fixtures("invalid/unknown_model.yaml")).unwrap();
    let report = parse("unknown_model.yaml", &text);
    assert!(messages(&report).contains("unknown model profile"));
}

#[test]
fn unknown_key_is_reported() {
    let text = fs::read_to_string(fixtures("invalid/unknown_key.yaml")).unwrap();
    let report = parse("unknown_key.yaml", &text);
    assert!(messages(&report).contains("unknown key `foo`"));
}

#[test]
fn missing_adapter_is_reported() {
    let text = fs::read_to_string(fixtures("invalid/missing_adapter.yaml")).unwrap();
    let report = parse("missing_adapter.yaml", &text);
    assert!(messages(&report).contains("missing required field `adapter`"));
}

#[test]
fn unknown_capability_is_reported() {
    let text = fs::read_to_string(fixtures("invalid/bad_capability.yaml")).unwrap();
    let report = parse("bad_capability.yaml", &text);
    assert!(messages(&report).contains("unknown capability `telepathy`"));
}

#[test]
fn unsupported_version_is_reported() {
    let text = fs::read_to_string(fixtures("invalid/unsupported_version.yaml")).unwrap();
    let report = parse("unsupported_version.yaml", &text);
    assert!(messages(&report).contains("unsupported configuration version"));
}

#[test]
fn malformed_placeholder_is_reported() {
    let text = fs::read_to_string(fixtures("invalid/bad_placeholder.yaml")).unwrap();
    let report = parse("bad_placeholder.yaml", &text);
    assert!(messages(&report).contains("malformed placeholder"));
}

#[test]
fn missing_required_environment_variable_is_reported() {
    let text = fs::read_to_string(fixtures("invalid/missing_required_env.yaml")).unwrap();
    let report = parse("missing_required_env.yaml", &text);
    assert!(messages(&report).contains("AXIOM_TEST_MISSING_KEY"));
}

#[test]
fn duplicate_keys_are_rejected() {
    let text = fs::read_to_string(fixtures("invalid/duplicate_key.yaml")).unwrap();
    let report = parse("duplicate_key.yaml", &text);
    assert!(report.has_errors());
}

#[test]
fn invalid_yaml_reports_and_renders_a_location() {
    let text = fs::read_to_string(fixtures("invalid/bad_yaml.yaml")).unwrap();
    let report = parse("bad_yaml.yaml", &text);
    assert!(report.has_errors());
    assert!(report.config.is_none());

    let source = Source::new("bad_yaml.yaml", text);
    let rendered = report.diagnostics[0].render(&source);
    assert!(rendered.contains("invalid YAML"));
    assert!(rendered.contains("bad_yaml.yaml:"));
}

#[test]
fn every_invalid_fixture_reports_and_renders() {
    let mut checked = 0;
    for entry in fs::read_dir(fixtures("invalid")).expect("invalid fixtures dir") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        let report = parse(&path.display().to_string(), &text);
        assert!(
            report.has_errors(),
            "{} should have reported an error",
            path.display()
        );

        let source = Source::new(path.display().to_string(), text);
        for diagnostic in &report.diagnostics {
            let _ = diagnostic.render(&source);
        }
        checked += 1;
    }
    assert!(checked >= 8);
}

#[test]
fn placeholder_defaults_are_used_when_variables_are_absent() {
    let text = "version: 1\nintegrations:\n  local:\n    adapter: openai_compatible\n    base_url: \"${AXIOM_LOCAL_BASE_URL:-http://localhost:11434/v1}\"\n    api_key_optional: true\n";
    let report = parse("inline.yaml", text);
    assert_eq!(report.error_count(), 0, "{}", messages(&report));
    let config = report.config.unwrap();
    assert_eq!(
        config.integrations["local"].base_url,
        "http://localhost:11434/v1"
    );
}

#[test]
fn secrets_are_redacted_in_debug_output() {
    let text = include_str!("../../../examples/axiom.yaml");
    let report = parse("examples/axiom.yaml", text);
    let config = report.config.unwrap();
    let claude = &config.integrations["claude"];

    let secret = claude.api_key.as_ref().expect("a resolved secret");
    assert_eq!(secret.expose(), "sk-ant-test-secret");
    assert_eq!(format!("{secret}"), "<redacted>");

    let debug = format!("{claude:?}");
    assert!(!debug.contains("sk-ant-test-secret"));
    assert!(debug.contains("<redacted>"));
}

#[test]
fn missing_api_key_without_optional_is_a_warning_not_an_error() {
    let text = "version: 1\nintegrations:\n  cloud:\n    adapter: openai_compatible\n    base_url: \"https://example.invalid\"\n";
    let report = parse("inline.yaml", text);
    assert_eq!(report.error_count(), 0, "{}", messages(&report));
    assert!(report.warning_count() >= 1);
    assert!(messages(&report).contains("api_key"));
}

#[test]
fn language_and_sources_are_read() {
    let text = include_str!("../../../examples/axiom.yaml");
    let report = parse("examples/axiom.yaml", text);
    assert_eq!(report.error_count(), 0, "{}", messages(&report));
    let config = report.config.unwrap();
    assert_eq!(config.language.as_deref(), Some("1.0"));
    assert_eq!(config.sources, vec!["src/**/*.ax".to_string()]);
}

#[test]
fn unsupported_language_version_is_reported() {
    let text = "version: 1\nlanguage: \"2.0\"\n";
    let report = parse("inline.yaml", text);
    assert!(messages(&report).contains("unsupported language version `2.0`"));
}

#[test]
fn allowed_data_classes_default_and_validation() {
    let text = "version: 1\nintegrations:\n  cloud:\n    adapter: openai_compatible\n    base_url: \"https://example.invalid\"\n    api_key_optional: true\n";
    let report = parse("inline.yaml", text);
    assert_eq!(report.error_count(), 0, "{}", messages(&report));
    assert_eq!(
        report.config.unwrap().integrations["cloud"].allowed_data_classes,
        vec!["public".to_string()]
    );

    let bad = "version: 1\nintegrations:\n  cloud:\n    adapter: openai_compatible\n    base_url: \"https://example.invalid\"\n    api_key_optional: true\n    allowed_data_classes: [public, secret]\n";
    let report = parse("inline.yaml", bad);
    assert!(messages(&report).contains("cannot contain `secret`"));
}

#[test]
fn api_policies_compilation_browser_and_frontend_are_read() {
    let text = include_str!("../../../examples/axiom.yaml");
    let report = parse("examples/axiom.yaml", text);
    assert_eq!(report.error_count(), 0, "{}", messages(&report));
    let config = report.config.unwrap();

    let policy = &config.api_policies["get_dashboard"];
    assert_eq!(policy.version.as_deref(), Some("1.0.0"));
    assert_eq!(policy.authentication.as_deref(), Some("required"));
    assert_eq!(policy.requires, vec!["dashboard:read".to_string()]);
    assert_eq!(policy.data_class.as_deref(), Some("internal"));
    assert_eq!(policy.cache.as_deref(), Some("none"));

    assert_eq!(
        config.compilation.targets,
        vec!["server".to_string(), "wasm_browser".to_string()]
    );
    let server = &config.compilation.entrypoints["server"];
    assert_eq!(server.module.as_deref(), Some("src.server"));
    assert_eq!(server.init, None);
    let wasm = &config.compilation.entrypoints["wasm_browser"];
    assert_eq!(wasm.module.as_deref(), Some("src.frontend"));
    assert_eq!(wasm.init.as_deref(), Some("init"));
    assert_eq!(wasm.render.as_deref(), Some("render"));
    assert_eq!(wasm.handle_event.as_deref(), Some("handle_event"));

    let wasm_cfg = &config.compilation.wasm;
    assert_eq!(wasm_cfg.target.as_deref(), Some("wasm32-unknown-unknown"));
    assert_eq!(wasm_cfg.host_api_version, Some(1));
    assert_eq!(wasm_cfg.max_module_bytes, Some(8_388_608));
    assert_eq!(wasm_cfg.reproducible, Some(true));

    let browser = &config.browser;
    assert_eq!(browser.isolation.as_deref(), Some("dedicated_worker"));
    assert_eq!(browser.max_memory_pages, Some(256));
    assert_eq!(browser.max_compute_ms_per_event, Some(50));
    assert_eq!(browser.max_event_bytes, Some(65_536));
    assert_eq!(browser.preserve_previous_release_on_failure, Some(true));
    assert_eq!(browser.state.default_scope.as_deref(), Some("per_origin"));
    assert_eq!(browser.state.default_persistence.as_deref(), Some("memory"));
    assert_eq!(browser.data_cache.enabled, Some(true));
    assert_eq!(browser.data_cache.max_entries, Some(128));

    let registry = &config.frontend.registry;
    assert_eq!(registry.content_addressed, Some(true));
    assert_eq!(registry.retain_accepted_versions, Some(5));
    assert_eq!(registry.require_pinned_references, Some(true));
    assert_eq!(config.frontend.build_cache.enabled, Some(true));
    assert_eq!(config.frontend.build_cache.max_bytes, Some(268_435_456));
}

#[test]
fn unknown_compilation_target_is_reported() {
    let text = "version: 1\ncompilation:\n  targets: [server, native]\n";
    let report = parse("inline.yaml", text);
    assert!(messages(&report).contains("unknown target `native`"));
}

#[test]
fn unsupported_host_api_version_is_reported() {
    let text = "version: 1\ncompilation:\n  wasm:\n    host_api_version: 9\n";
    let report = parse("inline.yaml", text);
    assert!(messages(&report).contains("unsupported `host_api_version` 9"));
}

#[test]
fn entrypoint_missing_module_is_reported() {
    let text = "version: 1\ncompilation:\n  entrypoints:\n    server: {}\n";
    let report = parse("inline.yaml", text);
    assert!(messages(&report).contains("missing required field `module`"));
}

#[test]
fn unknown_browser_key_is_reported() {
    let text = "version: 1\nbrowser:\n  max_cpu: 4\n";
    let report = parse("inline.yaml", text);
    assert!(messages(&report).contains("unknown key `max_cpu`"));
}
