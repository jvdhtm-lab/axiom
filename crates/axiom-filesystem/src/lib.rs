//! Secure filesystem workspaces (agents.md §6, Phase 6 partial).
//!
//! The compiler checks workspace declarations, but the **runtime** is the
//! security boundary. This crate implements that boundary in memory so it is
//! deterministic and offline-testable:
//!
//! - [`path`] normalizes project-relative paths and refuses root escapes;
//! - [`policy`] evaluates workspace permissions (read/write/deny/protected) and
//!   narrows derived workspaces;
//! - [`overlay`] provides copy-on-write isolation with snapshot/restore/discard;
//! - [`patch`] produces immutable patches and detects integration conflicts.
//!
//! Real OS sandboxing (process isolation, Git worktrees, containerization) is
//! tracked as remaining Phase 6 work in `docs/implementation-ledger.md`.

pub mod overlay;
pub mod patch;
pub mod path;
pub mod policy;

pub use overlay::{Snapshot, Workspace};
pub use patch::{Conflict, FileChange, FileChangeKind, Patch};
pub use path::{normalize, Pattern};
pub use policy::{Access, DenyReason, EffectivePolicy, IsolationMode, Limits, WorkspacePolicy};

use std::fmt;

/// A refused filesystem operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsError {
    /// The path is absolute, empty, or escapes the workspace root.
    InvalidPath(String),
    /// The path is well-formed but the policy forbids this access.
    Denied { path: String, reason: DenyReason },
    /// The path does not exist in the base or staged view.
    NotFound(String),
    /// A write or patch exceeded a configured size/count limit.
    Limit { what: &'static str, limit: u64 },
    /// An isolation mode name is not one of the known modes.
    UnknownIsolation(String),
    /// A write was attempted in a `read_only` workspace.
    ReadOnly(String),
    /// A file was read as text but is not valid UTF-8.
    NotText(String),
    /// A derived workspace would widen its base's authority.
    Widening(String),
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FsError::InvalidPath(path) => {
                write!(f, "invalid workspace path `{path}`")
            }
            FsError::Denied { path, reason } => {
                write!(f, "access to `{path}` denied: {reason}")
            }
            FsError::NotFound(path) => write!(f, "`{path}` not found"),
            FsError::Limit { what, limit } => {
                write!(f, "limit exceeded: {what} (limit {limit})")
            }
            FsError::UnknownIsolation(name) => {
                write!(f, "unknown isolation mode `{name}`")
            }
            FsError::ReadOnly(path) => {
                write!(f, "`{path}` is read-only in this workspace")
            }
            FsError::NotText(path) => {
                write!(f, "`{path}` is not valid UTF-8 text")
            }
            FsError::Widening(detail) => {
                write!(f, "workspace would widen authority: {detail}")
            }
        }
    }
}

impl std::error::Error for FsError {}
