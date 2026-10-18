//! Immutable patches and integration conflict checks (agents.md §6.1, §6.2).
//!
//! A patch is a *proposal*: it records the base revision it was computed against
//! and the content hashes on both sides of each change. Integrating it can be
//! refused if the shared tree moved underneath it.

use crate::overlay::Snapshot;

/// What a patch does to one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChangeKind {
    Added,
    Modified,
    Deleted,
}

impl FileChangeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FileChangeKind::Added => "added",
            FileChangeKind::Modified => "modified",
            FileChangeKind::Deleted => "deleted",
        }
    }
}

/// A single file change in a patch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub kind: FileChangeKind,
    /// Content hash at the patch base (empty for `Added`).
    pub base_hash: Option<String>,
    /// Content hash after the patch (empty for `Deleted`).
    pub new_hash: Option<String>,
}

/// A proposed set of changes against a specific base revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    pub base_revision: String,
    pub changes: Vec<FileChange>,
}

impl Patch {
    /// A patch with changes sorted by path (deterministic).
    pub fn new(base_revision: String, mut changes: Vec<FileChange>) -> Patch {
        changes.sort_by(|a, b| a.path.cmp(&b.path));
        Patch {
            base_revision,
            changes,
        }
    }

    /// Whether the patch changes anything.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Check this patch against the current base snapshot.
    ///
    /// A change conflicts if the tree no longer matches what the patch was
    /// computed against: `Added` over an existing file, `Modified`/`Deleted`
    /// over a missing file, or a content hash that has since changed.
    pub fn check_conflicts(&self, current: &Snapshot) -> Vec<Conflict> {
        let mut conflicts = Vec::new();
        for change in &self.changes {
            let now = current.hash_of(&change.path);
            match change.kind {
                FileChangeKind::Added => {
                    if now.is_some() {
                        conflicts.push(Conflict {
                            path: change.path.clone(),
                            reason: "file already exists in the target".to_string(),
                        });
                    }
                }
                FileChangeKind::Modified | FileChangeKind::Deleted => match now {
                    None => conflicts.push(Conflict {
                        path: change.path.clone(),
                        reason: "file is missing from the target".to_string(),
                    }),
                    Some(hash) if Some(&hash) != change.base_hash.as_ref() => {
                        conflicts.push(Conflict {
                            path: change.path.clone(),
                            reason: "file changed since the patch base".to_string(),
                        });
                    }
                    Some(_) => {}
                },
            }
        }
        conflicts
    }
}

/// A refused integration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub path: String,
    pub reason: String,
}

impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "conflict at `{}`: {}", self.path, self.reason)
    }
}
