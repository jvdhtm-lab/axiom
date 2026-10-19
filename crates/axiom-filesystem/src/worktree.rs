//! Isolated Git worktrees/branches (agents.md §6.1 `git_worktree`).
//!
//! A worktree shares the repository object store but has a private working
//! directory and branch, so writes never touch the shared checkout. All Git
//! invocations are argv-only (no shell) and refuse unsafe branch names.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::FsError;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// A live Git worktree; removing it also removes its branch checkout.
#[derive(Debug)]
pub struct GitWorktree {
    repo: PathBuf,
    path: PathBuf,
    branch: String,
    active: bool,
}

impl GitWorktree {
    /// Create a worktree at a fresh temporary directory on a new branch.
    ///
    /// `repo` must be the repository root (not a subdirectory). The branch name
    /// is validated and may not already exist.
    pub fn create(repo: impl AsRef<Path>, branch: &str) -> Result<GitWorktree, FsError> {
        validate_branch(branch)?;
        let repo = repo.as_ref();
        let root = canonicalize(repo)?;
        let top = run_git(&root, &["rev-parse", "--show-toplevel"])?;
        let top = canonicalize(Path::new(top.trim()))?;
        if top != root {
            return Err(FsError::Worktree(format!(
                "`{}` is not the repository root (`{}`)",
                repo.display(),
                top.display()
            )));
        }

        let path = unique_worktree_path();
        let path_arg = path.to_string_lossy().to_string();
        run_git(&root, &["worktree", "add", "-b", branch, &path_arg, "HEAD"])?;

        Ok(GitWorktree {
            repo: root,
            path,
            branch: branch.to_string(),
            active: true,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn branch(&self) -> &str {
        &self.branch
    }

    pub fn repo(&self) -> &Path {
        &self.repo
    }

    /// Paths reported as changed/untracked by `git status`, in Git's order.
    pub fn changed_files(&self) -> Result<Vec<String>, FsError> {
        let output = run_git(&self.path, &["status", "--porcelain=v1"])?;
        let mut files = Vec::new();
        for line in output.lines() {
            if line.len() < 4 {
                continue;
            }
            let mut path = line[3..].to_string();
            if let Some((_, renamed)) = path.rsplit_once(" -> ") {
                path = renamed.to_string();
            }
            files.push(path.trim_matches('"').to_string());
        }
        Ok(files)
    }

    /// Tracked changes relative to the worktree's `HEAD` (untracked files are
    /// reported by [`GitWorktree::changed_files`]).
    pub fn diff(&self) -> Result<String, FsError> {
        run_git(&self.path, &["diff", "HEAD", "--no-color"])
    }

    /// Remove the worktree checkout. Safe to call more than once.
    pub fn remove(&mut self) -> Result<(), FsError> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        let path_arg = self.path.to_string_lossy().to_string();
        let result = run_git(&self.repo, &["worktree", "remove", "--force", &path_arg]);
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&self.path);
        }
        result.map(|_| ())
    }
}

impl Drop for GitWorktree {
    fn drop(&mut self) {
        let _ = self.remove();
    }
}

fn validate_branch(branch: &str) -> Result<(), FsError> {
    let invalid = branch.is_empty()
        || branch.len() > 200
        || branch.starts_with('-')
        || branch.starts_with('/')
        || branch.ends_with('/')
        || branch.contains('\0')
        || branch.contains("..")
        || branch.contains("//")
        || branch.contains('\\')
        || branch.contains(' ')
        || !branch
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'));
    if invalid {
        return Err(FsError::Worktree(format!(
            "unsafe or invalid branch name `{branch}`"
        )));
    }
    Ok(())
}

fn canonicalize(path: &Path) -> Result<PathBuf, FsError> {
    path.canonicalize()
        .map_err(|err| FsError::Worktree(format!("`{}`: {err}", path.display())))
}

fn run_git(dir: &Path, args: &[&str]) -> Result<String, FsError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|err| FsError::Worktree(format!("cannot run git: {err}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(FsError::Worktree(format!(
            "`git {}` failed: {}",
            args.join(" "),
            stderr.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn unique_worktree_path() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "axiom-worktree-{}-{sequence}-{nonce}",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_branch_names_are_refused() {
        for name in ["", "-evil", "../escape", "a..b", "a b", "a\\b", "/root"] {
            assert!(validate_branch(name).is_err(), "`{name}` should be refused");
        }
        assert!(validate_branch("axiom/frontend-edit").is_ok());
    }
}
