//! Command sandbox planning: allowlisting, structured args, and safe dirs.

use axiom_config::parse_config_with;
use axiom_tools::{
    CommandOutcome, CommandRequest, CommandRunner, MockCommandRunner, ToolError, ToolRegistry,
};

const CONFIG: &str = "\
version: 1
commands:
  cargo_check:
    executable: cargo
    args: [\"check\", \"--offline\"]
    working_directory: \".\"
    timeout_seconds: 300
    network: false
";

fn registry() -> ToolRegistry {
    let env = |_: &str| None;
    let report = parse_config_with("inline.yaml", CONFIG, &env);
    assert!(!report.has_errors(), "{:?}", report.diagnostics);
    ToolRegistry::from_config(&report.config.unwrap())
}

#[test]
fn plans_allowlisted_profile_with_structured_args() {
    let plan = registry()
        .plan(&CommandRequest::new("cargo_check").arg("--lib"))
        .unwrap();
    assert_eq!(plan.executable, "cargo");
    assert_eq!(plan.argv, vec!["check", "--offline", "--lib"]);
    assert_eq!(plan.timeout_seconds, Some(300));
    assert!(!plan.network);
}

#[test]
fn metacharacters_stay_literal_argv_elements() {
    let plan = registry()
        .plan(&CommandRequest::new("cargo_check").arg("; rm -rf /"))
        .unwrap();
    // No shell: the whole string is one argument, not a command chain.
    assert_eq!(plan.argv.last().unwrap(), "; rm -rf /");
    assert_eq!(plan.argv.len(), 3);
}

#[test]
fn unknown_profile_is_refused() {
    let error = registry().plan(&CommandRequest::new("nope")).unwrap_err();
    assert_eq!(error, ToolError::UnknownCommand("nope".to_string()));
}

#[test]
fn escaping_working_directory_is_refused() {
    let error = registry()
        .plan(&CommandRequest::new("cargo_check").in_directory("../.."))
        .unwrap_err();
    assert!(matches!(error, ToolError::UnsafeInput(_)));

    let absolute = registry()
        .plan(&CommandRequest::new("cargo_check").in_directory("/etc"))
        .unwrap_err();
    assert!(matches!(absolute, ToolError::UnsafeInput(_)));
}

#[test]
fn mock_runner_records_plans_and_returns_scripted_outcomes() {
    let mut runner = MockCommandRunner::new().with_response("cargo", CommandOutcome::success("ok"));

    let plan = registry()
        .plan(&CommandRequest::new("cargo_check"))
        .unwrap();
    let outcome = runner.run(&plan).unwrap();
    assert!(outcome.is_success());
    assert_eq!(outcome.stdout, "ok");
    assert_eq!(runner.calls(), &[plan]);
}
