//! Exclusive integration gate (agents.md §6.1 `exclusive_integration`).
//!
//! Integration is a privileged operation: only a workspace whose isolation mode
//! is `exclusive_integration` may produce an integrated tree, it must hold the
//! exclusive lock for the repository, and the patch must apply cleanly to the
//! current base revision. Protected files still require separate authorization.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::FsError;

/// An exclusively held integration lock for a repository root.
///
/// The lock is a `create_new` file under `<root>/.axiom/integration.lock`, so
/// two processes cannot integrate at the same time. Dropping the guard removes
/// the file; a stale lock must be removed deliberately by an operator.
#[derive(Debug)]
pub struct IntegrationLock {
    root: PathBuf,
    path: PathBuf,
    active: bool,
}

impl IntegrationLock {
    pub fn acquire(root: impl AsRef<Path>) -> Result<IntegrationLock, FsError> {
        let root = root
            .as_ref()
            .canonicalize()
            .map_err(|err| FsError::Io(format!("`{}`: {err}", root.as_ref().display())))?;
        let directory = root.join(".axiom");
        std::fs::create_dir_all(&directory)
            .map_err(|err| FsError::Io(format!("`{}`: {err}", directory.display())))?;
        let path = directory.join("integration.lock");

        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(FsError::IntegrationLocked(path.display().to_string()))
            }
            Err(err) => {
                return Err(FsError::Io(format!("`{}`: {err}", path.display())));
            }
        };
        let _ = writeln!(file, "pid={}", std::process::id());
        let _ = writeln!(file, "root={}", root.display());

        Ok(IntegrationLock {
            root,
            path,
            active: true,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for IntegrationLock {
    fn drop(&mut self) {
        if self.active {
            self.active = false;
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
