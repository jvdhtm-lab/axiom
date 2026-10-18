//! Command sandbox: allowlisted profiles with structured arguments.
//!
//! Axiom never runs a shell string assembled from model output. A command is
//! described by a registered `commands.<name>` profile plus a list of literal
//! arguments; planning turns that into an explicit `(executable, argv)` pair
//! with a validated working directory. Metacharacters carry no special meaning
//! because no shell is involved.
//!
//! Actual OS/container execution is Phase 6 hardening; this module provides the
//! planning/validation boundary and a deterministic [`MockCommandRunner`] used
//! in tests. A runner that refuses to execute makes the remaining gap explicit
//! rather than silently running unsandboxed.

use std::collections::BTreeMap;

use crate::{ToolError, ToolRegistry};

/// A request to run a registered command profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRequest {
    pub profile: String,
    pub args: Vec<String>,
    pub working_directory: Option<String>,
}

impl CommandRequest {
    pub fn new(profile: impl Into<String>) -> CommandRequest {
        CommandRequest {
            profile: profile.into(),
            args: Vec::new(),
            working_directory: None,
        }
    }

    pub fn arg(mut self, value: impl Into<String>) -> CommandRequest {
        self.args.push(value.into());
        self
    }

    pub fn in_directory(mut self, dir: impl Into<String>) -> CommandRequest {
        self.working_directory = Some(dir.into());
        self
    }
}

/// A validated, shell-free command invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPlan {
    pub profile: String,
    pub executable: String,
    /// Literal arguments; never interpreted by a shell.
    pub argv: Vec<String>,
    pub working_directory: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub network: bool,
}

/// The result of running a planned command.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommandOutcome {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutcome {
    pub fn success(stdout: impl Into<String>) -> CommandOutcome {
        CommandOutcome {
            exit_code: 0,
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    pub fn failure(exit_code: i32, stderr: impl Into<String>) -> CommandOutcome {
        CommandOutcome {
            exit_code,
            stdout: String::new(),
            stderr: stderr.into(),
        }
    }

    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }
}

/// Anything that can execute a [`CommandPlan`].
pub trait CommandRunner {
    fn run(&mut self, plan: &CommandPlan) -> Result<CommandOutcome, ToolError>;
}

/// A deterministic runner that records plans and returns scripted outcomes.
#[derive(Debug, Default)]
pub struct MockCommandRunner {
    responses: BTreeMap<String, CommandOutcome>,
    calls: Vec<CommandPlan>,
}

impl MockCommandRunner {
    pub fn new() -> MockCommandRunner {
        MockCommandRunner::default()
    }

    /// Script the outcome for an executable (default: exit 0, empty output).
    pub fn with_response(mut self, executable: impl Into<String>, outcome: CommandOutcome) -> Self {
        self.responses.insert(executable.into(), outcome);
        self
    }

    pub fn calls(&self) -> &[CommandPlan] {
        &self.calls
    }
}

impl CommandRunner for MockCommandRunner {
    fn run(&mut self, plan: &CommandPlan) -> Result<CommandOutcome, ToolError> {
        self.calls.push(plan.clone());
        Ok(self
            .responses
            .get(&plan.executable)
            .cloned()
            .unwrap_or_default())
    }
}

impl ToolRegistry {
    /// Turn a request into a validated, shell-free command plan.
    pub fn plan(&self, request: &CommandRequest) -> Result<CommandPlan, ToolError> {
        let profile = self.command(&request.profile)?;

        if profile.executable.contains('\0') {
            return Err(ToolError::UnsafeInput(
                "executable contains a NUL byte".to_string(),
            ));
        }
        for arg in &request.args {
            if arg.contains('\0') {
                return Err(ToolError::UnsafeInput(
                    "argument contains a NUL byte".to_string(),
                ));
            }
        }

        let working_directory = request
            .working_directory
            .clone()
            .or_else(|| profile.working_directory.clone());
        if let Some(dir) = &working_directory {
            validate_working_directory(dir)?;
        }

        let mut argv: Vec<String> = profile.args.clone();
        argv.extend(request.args.iter().cloned());

        Ok(CommandPlan {
            profile: request.profile.clone(),
            executable: profile.executable.clone(),
            argv,
            working_directory,
            timeout_seconds: profile.timeout_seconds,
            network: profile.network.unwrap_or(false),
        })
    }
}

/// Working directories must be project-relative and must not escape upward.
fn validate_working_directory(dir: &str) -> Result<(), ToolError> {
    let unified = dir.replace('\\', "/");
    let absolute = unified.starts_with('/')
        || (unified.len() >= 2
            && unified.as_bytes()[1] == b':'
            && unified.as_bytes()[0].is_ascii_alphabetic());
    if absolute {
        return Err(ToolError::UnsafeInput(format!(
            "working directory `{dir}` is absolute"
        )));
    }
    if unified.split('/').any(|segment| segment == "..") {
        return Err(ToolError::UnsafeInput(format!(
            "working directory `{dir}` escapes the project root"
        )));
    }
    Ok(())
}
