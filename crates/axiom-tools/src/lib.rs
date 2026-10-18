//! Tool registry (agents.md §3, §4).
//!
//! An `agent` declares the tools it may call (`tools: [fs.read, fs.search, ...]`).
//! Those names must resolve to something the runtime is allowed to do. Some
//! tools are builtins provided by the host (`fs.read`, `fs.write`,
//! `compiler.check`); command tools run a `commands.<name>` profile from
//! `axiom.yaml`. Unknown tools are refused rather than ignored.

use std::collections::BTreeMap;
use std::fmt;

use axiom_config::{CommandProfile, ProjectConfig};

/// What a tool is allowed to touch, and therefore which effect it requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    /// Reads files inside the agent's workspace (`fs.read`).
    FilesystemRead,
    /// Writes files inside the agent's workspace (`fs.write`).
    FilesystemWrite,
    /// Runs a configured command profile (`command.run`).
    Command,
    /// Runs a named compiler check (`compiler.check`).
    Check,
}

impl ToolKind {
    /// The effect an agent must declare in its `effects` clause to use the tool.
    pub fn effect(self) -> &'static str {
        match self {
            ToolKind::FilesystemRead => "fs.read",
            ToolKind::FilesystemWrite => "fs.write",
            ToolKind::Command | ToolKind::Check => "command.run",
        }
    }
}

/// A resolved tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolEntry {
    pub name: String,
    pub kind: ToolKind,
    /// A `commands.<name>` profile the tool runs, when applicable.
    pub command: Option<String>,
}

/// Reasons a tool or command reference cannot be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    /// The agent asked for a tool that is not registered.
    UnknownTool(String),
    /// A command tool referenced a `commands.<name>` profile that does not exist.
    UnknownCommand(String),
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolError::UnknownTool(name) => {
                write!(f, "unknown tool `{name}`; register it before use")
            }
            ToolError::UnknownCommand(name) => write!(
                f,
                "unknown command profile `{name}`; declare it under `commands:`"
            ),
        }
    }
}

impl std::error::Error for ToolError {}

/// The tools available to agents in one project.
#[derive(Debug, Clone, Default)]
pub struct ToolRegistry {
    entries: BTreeMap<String, ToolEntry>,
    commands: BTreeMap<String, CommandProfile>,
}

impl ToolRegistry {
    /// The host-provided tools, present in every project.
    pub fn with_builtins() -> Self {
        let mut registry = ToolRegistry::default();
        for (name, kind) in [
            ("fs.read", ToolKind::FilesystemRead),
            ("fs.search", ToolKind::FilesystemRead),
            ("fs.write", ToolKind::FilesystemWrite),
            ("compiler.check", ToolKind::Check),
            ("command.run", ToolKind::Command),
        ] {
            registry.insert(ToolEntry {
                name: name.to_string(),
                kind,
                command: None,
            });
        }
        registry
    }

    /// Builtins plus one tool per configured command profile, named
    /// `command.<profile>`, so agents can call a specific command directly.
    pub fn from_config(config: &ProjectConfig) -> Self {
        let mut registry = ToolRegistry::with_builtins();
        for (name, profile) in &config.commands {
            registry.commands.insert(name.clone(), profile.clone());
            registry.insert(ToolEntry {
                name: format!("command.{name}"),
                kind: ToolKind::Command,
                command: Some(name.clone()),
            });
        }
        registry
    }

    fn insert(&mut self, entry: ToolEntry) {
        self.entries.insert(entry.name.clone(), entry);
    }

    /// Classify a tool name, or refuse it.
    pub fn classify(&self, name: &str) -> Result<&ToolEntry, ToolError> {
        self.entries
            .get(name)
            .ok_or_else(|| ToolError::UnknownTool(name.to_string()))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Look up a configured command profile used by a command tool.
    pub fn command(&self, profile: &str) -> Result<&CommandProfile, ToolError> {
        self.commands
            .get(profile)
            .ok_or_else(|| ToolError::UnknownCommand(profile.to_string()))
    }

    /// Registered tool names, in sorted order.
    pub fn names(&self) -> Vec<&str> {
        self.entries.keys().map(String::as_str).collect()
    }
}
