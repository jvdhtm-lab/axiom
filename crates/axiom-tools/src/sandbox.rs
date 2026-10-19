//! Command sandbox: allowlisted profiles with structured arguments.
//!
//! Axiom never runs a shell string assembled from model output. A command is
//! described by a registered `commands.<name>` profile plus a list of literal
//! arguments; planning turns that into an explicit `(executable, argv)` pair
//! with a validated working directory. Metacharacters carry no special meaning
//! because no shell is involved.
//!
//! [`ProcessCommandRunner`] executes a plan without a shell: inherited
//! environment is cleared (only a safe allowlist is passed), stdin is null,
//! output is capped, and a wall-clock timeout kills the child. Network-enabled
//! profiles are refused until a real OS/container network sandbox is available;
//! process isolation alone is not claimed to be a network boundary.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

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
    /// Whether either stream exceeded the runner's output limit.
    pub truncated: bool,
}

impl CommandOutcome {
    pub fn success(stdout: impl Into<String>) -> CommandOutcome {
        CommandOutcome {
            exit_code: 0,
            stdout: stdout.into(),
            stderr: String::new(),
            truncated: false,
        }
    }

    pub fn failure(exit_code: i32, stderr: impl Into<String>) -> CommandOutcome {
        CommandOutcome {
            exit_code,
            stdout: String::new(),
            stderr: stderr.into(),
            truncated: false,
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

/// Environment variables a child may inherit. Everything else is cleared.
const ENV_ALLOWLIST: [&str; 9] = [
    "PATH",
    "PATHEXT",
    "SystemRoot",
    "SYSTEMROOT",
    "TEMP",
    "TMP",
    "HOME",
    "USERPROFILE",
    "LANG",
];

/// Executes [`CommandPlan`]s as child processes without a shell.
///
/// This is **process isolation**, not a complete OS sandbox: it prevents shell
/// injection, ambient environment leakage, hanging commands, and unbounded
/// output, but it does not prevent a child from opening sockets. Profiles that
/// declare `network: true` are therefore refused.
pub struct ProcessCommandRunner {
    project_root: PathBuf,
    max_output_bytes: usize,
    poll_interval: Duration,
}

impl ProcessCommandRunner {
    pub fn new(project_root: impl Into<PathBuf>) -> ProcessCommandRunner {
        ProcessCommandRunner {
            project_root: project_root.into(),
            max_output_bytes: 1 << 20,
            poll_interval: Duration::from_millis(10),
        }
    }

    /// Cap the retained bytes for each of stdout and stderr (default 1 MiB).
    pub fn with_output_limit(mut self, bytes: usize) -> ProcessCommandRunner {
        self.max_output_bytes = bytes;
        self
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }
}

impl CommandRunner for ProcessCommandRunner {
    fn run(&mut self, plan: &CommandPlan) -> Result<CommandOutcome, ToolError> {
        if plan.network {
            return Err(ToolError::ExecutionUnavailable(
                "network-enabled command profiles require an OS/container sandbox".to_string(),
            ));
        }
        if plan.executable.contains('\0') || plan.argv.iter().any(|arg| arg.contains('\0')) {
            return Err(ToolError::UnsafeInput(
                "command contains a NUL byte".to_string(),
            ));
        }
        if let Some(dir) = &plan.working_directory {
            validate_working_directory(dir)?;
        }

        let working_directory = match &plan.working_directory {
            Some(dir) => self.project_root.join(dir),
            None => self.project_root.clone(),
        };
        if !working_directory.is_dir() {
            return Err(ToolError::ExecutionUnavailable(format!(
                "working directory `{}` does not exist",
                working_directory.display()
            )));
        }

        let mut command = Command::new(&plan.executable);
        command
            .args(&plan.argv)
            .current_dir(&working_directory)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear();
        for key in ENV_ALLOWLIST {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }

        let mut child = command.spawn().map_err(|err| {
            ToolError::ExecutionUnavailable(format!("failed to spawn `{}`: {err}", plan.executable))
        })?;
        let stdout = child
            .stdout
            .take()
            .map(|pipe| drain(pipe, self.max_output_bytes));
        let stderr = child
            .stderr
            .take()
            .map(|pipe| drain(pipe, self.max_output_bytes));

        let deadline = plan
            .timeout_seconds
            .map(|seconds| Instant::now() + Duration::from_secs(seconds));
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => {
                    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                        let _ = child.kill();
                        let _ = child.wait();
                        break None;
                    }
                    thread::sleep(self.poll_interval);
                }
                Err(err) => {
                    let _ = child.kill();
                    return Err(ToolError::ExecutionUnavailable(format!(
                        "failed to wait for `{}`: {err}",
                        plan.executable
                    )));
                }
            }
        };

        let (stdout, stdout_truncated) = join_reader(stdout);
        let (stderr, stderr_truncated) = join_reader(stderr);

        let Some(status) = status else {
            return Err(ToolError::Timeout {
                profile: plan.profile.clone(),
                seconds: plan.timeout_seconds.unwrap_or(0),
            });
        };

        Ok(CommandOutcome {
            exit_code: status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&stdout).to_string(),
            stderr: String::from_utf8_lossy(&stderr).to_string(),
            truncated: stdout_truncated || stderr_truncated,
        })
    }
}

fn drain(mut reader: impl Read + Send + 'static, limit: usize) -> JoinHandle<(Vec<u8>, bool)> {
    thread::spawn(move || {
        let mut kept = Vec::new();
        let mut truncated = false;
        let mut buffer = [0u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let room = limit.saturating_sub(kept.len());
                    if read > room {
                        truncated = true;
                    }
                    if room > 0 {
                        kept.extend_from_slice(&buffer[..read.min(room)]);
                    }
                }
                Err(_) => break,
            }
        }
        (kept, truncated)
    })
}

fn join_reader(handle: Option<JoinHandle<(Vec<u8>, bool)>>) -> (Vec<u8>, bool) {
    match handle {
        Some(handle) => handle.join().unwrap_or((Vec::new(), false)),
        None => (Vec::new(), false),
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
