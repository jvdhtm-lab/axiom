#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use axiom_database::{Actor, DatabaseSpec};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(label: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("axiom-db-{label}-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        TempDir { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn db_path(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

pub fn actor(run: &str) -> Actor {
    Actor::system(run)
}

pub fn spec(temp: &TempDir, name: &str) -> DatabaseSpec {
    DatabaseSpec::new(name, temp.db_path(&format!("{name}.sqlite")))
        .with_tables(&["users", "tasks", "audit"])
        .with_max_rows(10)
        .with_timeout(std::time::Duration::from_secs(2))
}
