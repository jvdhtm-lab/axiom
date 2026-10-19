//! Copy-on-write workspace overlays and snapshots.
//!
//! A [`Workspace`] reads through to an immutable base [`Snapshot`] and stages
//! writes privately. Nothing the workspace stages is visible outside it until a
//! patch is produced and integrated, which is exactly the `copy_on_write`
//! isolation contract.

use std::collections::BTreeMap;

use axiom_hash::hex_digest;

use crate::patch::{FileChange, FileChangeKind, Patch};
use crate::policy::{EffectivePolicy, IsolationMode, Limits};
use crate::FsError;

/// An immutable, content-addressed revision of a file tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    revision: String,
    files: BTreeMap<String, Vec<u8>>,
}

impl Snapshot {
    pub fn from_files(files: BTreeMap<String, Vec<u8>>) -> Snapshot {
        let revision = compute_revision(&files);
        Snapshot { revision, files }
    }

    pub fn empty() -> Snapshot {
        Snapshot::from_files(BTreeMap::new())
    }

    pub fn revision(&self) -> &str {
        &self.revision
    }

    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }

    pub fn hash_of(&self, path: &str) -> Option<String> {
        self.files.get(path).map(|bytes| hex_digest(bytes))
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn paths(&self) -> impl Iterator<Item = &String> {
        self.files.keys()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Vec<u8>)> {
        self.files.iter()
    }
}

fn compute_revision(files: &BTreeMap<String, Vec<u8>>) -> String {
    let mut manifest = String::new();
    for (path, bytes) in files {
        manifest.push_str(path);
        manifest.push('\0');
        manifest.push_str(&hex_digest(bytes));
        manifest.push('\n');
    }
    hex_digest(manifest.as_bytes())
}

/// A search hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub path: String,
    /// 1-based line number.
    pub line: u64,
    pub text: String,
}

/// A private copy-on-write workspace over a base snapshot.
#[derive(Debug, Clone)]
pub struct Workspace {
    base: Snapshot,
    policy: EffectivePolicy,
    isolation: IsolationMode,
    /// Staged changes: `Some(bytes)` writes, `None` deletes.
    staged: BTreeMap<String, Option<Vec<u8>>>,
}

impl Workspace {
    pub fn open(base: Snapshot, policy: EffectivePolicy, isolation: IsolationMode) -> Workspace {
        Workspace {
            base,
            policy,
            isolation,
            staged: BTreeMap::new(),
        }
    }

    pub fn isolation(&self) -> IsolationMode {
        self.isolation
    }

    pub fn base_revision(&self) -> &str {
        self.base.revision()
    }

    pub fn limits(&self) -> Limits {
        self.policy.limits()
    }

    /// The full current view (base overlaid with staged changes).
    pub fn materialized(&self) -> BTreeMap<String, Vec<u8>> {
        let mut files: BTreeMap<String, Vec<u8>> = self.base.files.clone();
        for (path, entry) in &self.staged {
            match entry {
                Some(bytes) => {
                    files.insert(path.clone(), bytes.clone());
                }
                None => {
                    files.remove(path);
                }
            }
        }
        files
    }

    /// Read a file as text, honoring read permissions and the overlay.
    pub fn read_text(&self, path: &str) -> Result<String, FsError> {
        let normalized = self.policy.check_read(path)?;
        let bytes = match self.staged.get(&normalized) {
            Some(Some(bytes)) => bytes.as_slice(),
            Some(None) => return Err(FsError::NotFound(normalized)),
            None => self
                .base
                .get(&normalized)
                .ok_or_else(|| FsError::NotFound(normalized.clone()))?,
        };
        String::from_utf8(bytes.to_vec()).map_err(|_| FsError::NotText(normalized))
    }

    /// List the readable files under `dir`, sorted.
    pub fn list(&self, dir: &str) -> Result<Vec<String>, FsError> {
        let prefix = normalize_dir(dir)?;
        let mut out = Vec::new();
        for path in self.materialized().keys() {
            if path_matches_dir(path, &prefix) && self.policy.check_read(path).is_ok() {
                out.push(path.clone());
            }
        }
        Ok(out)
    }

    /// Substring search across readable files, in path/line order.
    pub fn search(&self, query: &str) -> Result<Vec<Match>, FsError> {
        let files = self.materialized();
        let mut hits = Vec::new();
        for path in files.keys() {
            if self.policy.check_read(path).is_err() {
                continue;
            }
            let bytes: Option<&[u8]> = match self.staged.get(path) {
                Some(Some(bytes)) => Some(bytes.as_slice()),
                Some(None) => None,
                None => self.base.get(path),
            };
            let Some(bytes) = bytes else {
                continue;
            };
            let Ok(text) = std::str::from_utf8(bytes) else {
                continue;
            };
            for (index, line) in text.lines().enumerate() {
                if line.contains(query) {
                    hits.push(Match {
                        path: path.clone(),
                        line: index as u64 + 1,
                        text: line.to_string(),
                    });
                }
            }
        }
        Ok(hits)
    }

    /// Stage a write, enforcing size and file-count quotas.
    pub fn write_text(&mut self, path: &str, contents: &str) -> Result<(), FsError> {
        let normalized = self.policy.check_write(path)?;
        let limits = self.limits();
        if let Some(max) = limits.max_file_bytes {
            if contents.len() as u64 > max {
                return Err(FsError::Limit {
                    what: "file bytes",
                    limit: max,
                });
            }
        }

        let previous = self
            .staged
            .insert(normalized.clone(), Some(contents.as_bytes().to_vec()));
        if let Some(max) = limits.max_files {
            if self.materialized().len() as u64 > max {
                match previous {
                    Some(entry) => {
                        self.staged.insert(normalized, entry);
                    }
                    None => {
                        self.staged.remove(&normalized);
                    }
                }
                return Err(FsError::Limit {
                    what: "files in workspace",
                    limit: max,
                });
            }
        }
        Ok(())
    }

    /// Stage a delete.
    pub fn delete(&mut self, path: &str) -> Result<(), FsError> {
        let normalized = self.policy.check_write(path)?;
        let exists = self.materialized().contains_key(&normalized);
        if !exists {
            return Err(FsError::NotFound(normalized));
        }
        self.staged.insert(normalized, None);
        Ok(())
    }

    /// Build a patch from the staged view against the base.
    pub fn diff(&self) -> Patch {
        let current = self.materialized();
        let mut changes: Vec<FileChange> = Vec::new();

        for (path, bytes) in &current {
            match self.base.get(path) {
                None => changes.push(FileChange {
                    path: path.clone(),
                    kind: FileChangeKind::Added,
                    base_hash: None,
                    new_hash: Some(hex_digest(bytes)),
                }),
                Some(base_bytes) if base_bytes != bytes.as_slice() => {
                    changes.push(FileChange {
                        path: path.clone(),
                        kind: FileChangeKind::Modified,
                        base_hash: self.base.hash_of(path),
                        new_hash: Some(hex_digest(bytes)),
                    });
                }
                Some(_) => {}
            }
        }
        for (path, bytes) in self.base.iter() {
            if !current.contains_key(path) {
                changes.push(FileChange {
                    path: path.clone(),
                    kind: FileChangeKind::Deleted,
                    base_hash: Some(hex_digest(bytes)),
                    new_hash: None,
                });
            }
        }

        Patch::new(self.base.revision().to_string(), changes)
    }

    /// Produce the patch to submit, enforcing the changed-file quota.
    pub fn submit(&self) -> Result<Patch, FsError> {
        let patch = self.diff();
        if let Some(max) = self.limits().max_changed_files {
            if patch.changes.len() as u64 > max {
                return Err(FsError::Limit {
                    what: "changed files",
                    limit: max,
                });
            }
        }
        Ok(patch)
    }

    /// Apply an approved patch to the shared base, returning the integrated
    /// tree. The caller must persist the returned [`Snapshot`].
    ///
    /// Only `exclusive_integration` workspaces may integrate, the lock must be
    /// held (proven by its type), the patch must target the current base, and
    /// every touched path must still be writable and unprotected.
    pub fn integrate(
        &self,
        _lock: &crate::integration::IntegrationLock,
        patch: &Patch,
        files: &BTreeMap<String, Vec<u8>>,
    ) -> Result<Snapshot, FsError> {
        if self.isolation != IsolationMode::ExclusiveIntegration {
            return Err(FsError::IntegrationRefused(format!(
                "`{}` workspaces cannot integrate",
                self.isolation.as_str()
            )));
        }
        if patch.base_revision != self.base.revision() {
            return Err(FsError::IntegrationRefused(format!(
                "patch base `{}` does not match the current base `{}`",
                patch.base_revision,
                self.base.revision()
            )));
        }
        let conflicts = patch.check_conflicts(&self.base);
        if !conflicts.is_empty() {
            return Err(FsError::IntegrationRefused(
                conflicts
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; "),
            ));
        }

        let limits = self.limits();
        if let Some(max) = limits.max_changed_files {
            if patch.changes.len() as u64 > max {
                return Err(FsError::Limit {
                    what: "changed files",
                    limit: max,
                });
            }
        }

        let mut tree: BTreeMap<String, Vec<u8>> = self
            .base
            .iter()
            .map(|(path, bytes)| (path.clone(), bytes.clone()))
            .collect();

        for change in &patch.changes {
            let path = self.policy.check_write(&change.path)?;
            match change.kind {
                FileChangeKind::Added | FileChangeKind::Modified => {
                    let bytes = files.get(&change.path).ok_or_else(|| {
                        FsError::IntegrationRefused(format!(
                            "missing content for `{}`",
                            change.path
                        ))
                    })?;
                    if let Some(max) = limits.max_file_bytes {
                        if bytes.len() as u64 > max {
                            return Err(FsError::Limit {
                                what: "file bytes",
                                limit: max,
                            });
                        }
                    }
                    let hash = hex_digest(bytes);
                    if change.new_hash.as_deref() != Some(hash.as_str()) {
                        return Err(FsError::IntegrationRefused(format!(
                            "content for `{}` does not match the patch",
                            change.path
                        )));
                    }
                    tree.insert(path, bytes.clone());
                }
                FileChangeKind::Deleted => {
                    if change.new_hash.is_some() {
                        return Err(FsError::IntegrationRefused(format!(
                            "deleted file `{}` has new content",
                            change.path
                        )));
                    }
                    tree.remove(&path);
                }
            }
        }

        Ok(Snapshot::from_files(tree))
    }

    /// Snapshot the current view.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot::from_files(self.materialized())
    }

    /// Replace the staged view so it matches `snapshot`.
    pub fn restore(&mut self, snapshot: Snapshot) {
        let target = snapshot.files;
        let mut staged = BTreeMap::new();
        for (path, bytes) in &target {
            if self.base.get(path) != Some(bytes.as_slice()) {
                staged.insert(path.clone(), Some(bytes.clone()));
            }
        }
        for path in self.base.paths() {
            if !target.contains_key(path) {
                staged.insert(path.clone(), None);
            }
        }
        self.staged = staged;
    }

    /// Drop all staged changes.
    pub fn discard(&mut self) {
        self.staged.clear();
    }
}

fn normalize_dir(dir: &str) -> Result<String, FsError> {
    if dir.is_empty() || dir == "." {
        return Ok(String::new());
    }
    crate::path::normalize(dir)
}

fn path_matches_dir(path: &str, prefix: &str) -> bool {
    prefix.is_empty() || path == prefix || path.starts_with(&format!("{prefix}/"))
}
