//! Git worktree isolation tests. These require `git` on PATH (CI has it).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use axiom_filesystem::{FsError, GitWorktree};

fn temp_root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("axiom-fs-{label}-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("git should be installed");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repo(repo: &Path) {
    git(repo, &["init", "-q"]);
    std::fs::write(repo.join("a.txt"), "hello\n").unwrap();
    git(repo, &["add", "."]);
    git(
        repo,
        &[
            "-c",
            "user.email=axiom@example.test",
            "-c",
            "user.name=Axiom Test",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "init",
        ],
    );
}

#[test]
fn creates_modifies_and_removes_a_worktree() {
    let repo = temp_root("worktree");
    init_repo(&repo);

    let mut worktree = GitWorktree::create(&repo, "axiom/test-isolation").unwrap();
    assert!(worktree.path().exists());
    assert_ne!(worktree.path(), repo.canonicalize().unwrap());
    assert_eq!(worktree.branch(), "axiom/test-isolation");

    std::fs::write(worktree.path().join("a.txt"), "changed\n").unwrap();
    std::fs::write(worktree.path().join("new.txt"), "new\n").unwrap();

    let changed = worktree.changed_files().unwrap();
    assert!(changed.contains(&"a.txt".to_string()));
    assert!(changed.contains(&"new.txt".to_string()));

    let diff = worktree.diff().unwrap();
    assert!(diff.contains("-hello"));
    assert!(diff.contains("+changed"));

    let path = worktree.path().to_path_buf();
    worktree.remove().unwrap();
    assert!(!path.exists());

    std::fs::remove_dir_all(&repo).ok();
}

#[test]
fn refuses_a_directory_that_is_not_a_repository() {
    let dir = temp_root("not-repo");
    let error = GitWorktree::create(&dir, "axiom/test").unwrap_err();
    assert!(matches!(error, FsError::Worktree(_)));
    std::fs::remove_dir_all(&dir).ok();
}
