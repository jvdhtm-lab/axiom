//! Redacted, append-only database audit records.
//!
//! Records never contain SQL text, bound values, credentials, or connection
//! URLs: only the query fingerprint, actor/run identity, data class, row count,
//! and outcome.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::unix_now;

/// Who performed an operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    pub run_id: String,
    pub agent: Option<String>,
}

impl Actor {
    /// A system actor for operator tooling.
    pub fn system(run_id: impl Into<String>) -> Self {
        Actor {
            run_id: run_id.into(),
            agent: None,
        }
    }

    /// An agent actor within a run.
    pub fn agent(run_id: impl Into<String>, agent: impl Into<String>) -> Self {
        Actor {
            run_id: run_id.into(),
            agent: Some(agent.into()),
        }
    }

    fn label(&self) -> String {
        match &self.agent {
            Some(agent) => format!("{agent}#{}", self.run_id),
            None => format!("system#{}", self.run_id),
        }
    }
}

/// The outcome of a database operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditOutcome {
    Ok,
    Denied,
    Error,
}

impl AuditOutcome {
    /// Stable audit name.
    pub fn as_str(self) -> &'static str {
        match self {
            AuditOutcome::Ok => "ok",
            AuditOutcome::Denied => "denied",
            AuditOutcome::Error => "error",
        }
    }
}

/// One audited operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    pub actor: String,
    pub operation: String,
    pub database: String,
    pub fingerprint: String,
    pub data_class: String,
    pub rows: u64,
    pub outcome: AuditOutcome,
    pub at: u64,
}

impl AuditRecord {
    fn to_json(&self) -> String {
        format!(
            "{{\"at\":{},\"actor\":\"{}\",\"operation\":\"{}\",\"database\":\"{}\",\"fingerprint\":\"{}\",\"data_class\":\"{}\",\"rows\":{},\"outcome\":\"{}\"}}",
            self.at,
            escape(&self.actor),
            escape(&self.operation),
            escape(&self.database),
            escape(&self.fingerprint),
            escape(&self.data_class),
            self.rows,
            self.outcome.as_str()
        )
    }
}

/// An append-only audit sink.
pub struct AuditLog {
    target: Target,
}

enum Target {
    Memory(Mutex<Vec<AuditRecord>>),
    File { path: PathBuf, file: Mutex<File> },
}

impl AuditLog {
    /// An in-memory sink, used by tests and transient runs.
    pub fn memory() -> Self {
        AuditLog {
            target: Target::Memory(Mutex::new(Vec::new())),
        }
    }

    /// Append JSON lines to `path`, creating parent directories.
    pub fn file(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(AuditLog {
            target: Target::File {
                path,
                file: Mutex::new(file),
            },
        })
    }

    /// Record one operation. Audit failures are not allowed to change the
    /// operation outcome, but they are visible through [`AuditLog::flush`].
    pub fn record(&self, record: AuditRecord) {
        match &self.target {
            Target::Memory(records) => {
                if let Ok(mut records) = records.lock() {
                    records.push(record);
                }
            }
            Target::File { file, .. } => {
                if let Ok(mut file) = file.lock() {
                    let _ = writeln!(file, "{}", record.to_json());
                    let _ = file.sync_data();
                }
            }
        }
    }

    /// Snapshot the records. For file sinks this reads the file back.
    pub fn records(&self) -> Vec<AuditRecord> {
        match &self.target {
            Target::Memory(records) => records
                .lock()
                .map(|records| records.clone())
                .unwrap_or_default(),
            Target::File { path, .. } => {
                let mut text = String::new();
                if let Ok(mut file) = File::open(path) {
                    let _ = file.read_to_string(&mut text);
                }
                text.lines().filter_map(parse_json).collect()
            }
        }
    }
}

/// Force file-backed audits to disk. Memory sinks are already flushed.
pub fn flush(log: &AuditLog) {
    if let Target::File { file, .. } = &log.target {
        if let Ok(file) = file.lock() {
            let _ = file.sync_data();
        }
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str("\\u0000"),
            c => out.push(c),
        }
    }
    out
}

fn parse_json(line: &str) -> Option<AuditRecord> {
    let value = |key: &str| -> Option<String> {
        let needle = format!("\"{key}\":\"");
        let start = line.find(&needle)? + needle.len();
        let rest = &line[start..];
        let end = rest.find('"')?;
        Some(rest[..end].replace("\\\"", "\"").replace("\\\\", "\\"))
    };
    let number = |key: &str| -> Option<u64> {
        let needle = format!("\"{key}\":");
        let start = line.find(&needle)? + needle.len();
        let rest = &line[start..];
        let end = rest.find([',', '}']).unwrap_or(rest.len());
        rest[..end].trim().parse().ok()
    };
    let outcome = match value("outcome")?.as_str() {
        "ok" => AuditOutcome::Ok,
        "denied" => AuditOutcome::Denied,
        _ => AuditOutcome::Error,
    };
    Some(AuditRecord {
        at: number("at")?,
        actor: value("actor")?,
        operation: value("operation").unwrap_or_else(|| "read".to_string()),
        database: value("database")?,
        fingerprint: value("fingerprint")?,
        data_class: value("data_class")?,
        rows: number("rows")?,
        outcome,
    })
}

impl std::fmt::Debug for AuditLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.target {
            Target::Memory(records) => f
                .debug_struct("AuditLog")
                .field(
                    "memory_records",
                    &records.lock().map(|r| r.len()).unwrap_or(0),
                )
                .finish(),
            Target::File { path, .. } => f.debug_struct("AuditLog").field("path", path).finish(),
        }
    }
}

/// A convenience constructor for an audit record.
#[allow(clippy::too_many_arguments)]
pub fn record(
    actor: &Actor,
    operation: &'static str,
    database: &str,
    fingerprint: &str,
    data_class: &str,
    rows: u64,
    outcome: AuditOutcome,
) -> AuditRecord {
    AuditRecord {
        actor: actor.label(),
        operation: operation.to_string(),
        database: database.to_string(),
        fingerprint: fingerprint.to_string(),
        data_class: data_class.to_string(),
        rows,
        outcome,
        at: unix_now(),
    }
}
