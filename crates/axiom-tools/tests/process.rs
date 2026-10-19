//! Process command runner tests. These execute real, benign child processes.

use std::time::{Duration, Instant};

use axiom_tools::sandbox::{CommandPlan, CommandRunner, ProcessCommandRunner};
use axiom_tools::ToolError;

fn plan(executable: &str, argv: &[&str]) -> CommandPlan {
    CommandPlan {
        profile: "test".to_string(),
        executable: executable.to_string(),
        argv: argv.iter().map(|arg| arg.to_string()).collect(),
        working_directory: None,
        timeout_seconds: Some(30),
        network: false,
    }
}

fn runner() -> ProcessCommandRunner {
    ProcessCommandRunner::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn runs_an_argv_command_without_a_shell() {
    let outcome = runner().run(&plan("rustc", &["--version"])).unwrap();
    assert!(outcome.is_success(), "{:?}", outcome);
    assert!(outcome.stdout.contains("rustc"), "{:?}", outcome);
    assert!(!outcome.truncated);
}

#[test]
fn reports_nonzero_exit_status() {
    let outcome = runner()
        .run(&plan("rustc", &["--this-flag-does-not-exist"]))
        .unwrap();
    assert!(!outcome.is_success());
    assert!(!outcome.stderr.is_empty());
}

#[test]
fn refuses_network_enabled_profiles() {
    let mut plan = plan("rustc", &["--version"]);
    plan.network = true;
    let error = runner().run(&plan).unwrap_err();
    assert!(matches!(error, ToolError::ExecutionUnavailable(_)));
}

#[test]
fn refuses_an_escaping_working_directory() {
    let mut plan = plan("rustc", &["--version"]);
    plan.working_directory = Some("../outside".to_string());
    let error = runner().run(&plan).unwrap_err();
    assert!(matches!(error, ToolError::UnsafeInput(_)));
}

#[test]
fn truncates_oversized_output() {
    let outcome = runner()
        .with_output_limit(4)
        .run(&plan("rustc", &["--version"]))
        .unwrap();
    assert!(outcome.truncated);
    assert!(outcome.stdout.len() <= 4);
}

#[test]
fn kills_a_command_that_exceeds_its_timeout() {
    #[cfg(windows)]
    let plan = plan("ping", &["-n", "6", "127.0.0.1"]);
    #[cfg(unix)]
    let plan = plan("sleep", &["5"]);
    let mut plan = plan;
    plan.timeout_seconds = Some(1);

    let started = Instant::now();
    let error = runner().run(&plan).unwrap_err();
    assert!(matches!(error, ToolError::Timeout { .. }));
    assert!(started.elapsed() < Duration::from_secs(4));
}
