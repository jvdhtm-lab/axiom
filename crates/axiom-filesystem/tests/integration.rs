//! Exclusive integration gate tests.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use axiom_filesystem::{
    EffectivePolicy, FileChange, FileChangeKind, FsError, IntegrationLock, IsolationMode, Limits,
    Patch, Snapshot, Workspace, WorkspacePolicy,
};

fn temp_root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "axiom-integrate-{label}-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn policy() -> WorkspacePolicy {
    WorkspacePolicy::build(
        ".",
        &["**"],
        &["src/**"],
        &[],
        IsolationMode::ExclusiveIntegration,
        Limits {
            max_changed_files: Some(5),
            ..Limits::default()
        },
    )
    .unwrap()
}

fn base() -> Snapshot {
    let mut files = BTreeMap::new();
    files.insert("src/a.txt".to_string(), b"one".to_vec());
    files.insert("src/b.txt".to_string(), b"two".to_vec());
    Snapshot::from_files(files)
}

fn workspace() -> Workspace {
    Workspace::open(
        base(),
        EffectivePolicy::single(policy()),
        IsolationMode::ExclusiveIntegration,
    )
}

#[test]
fn integrates_under_an_exclusive_lock() {
    let root = temp_root("apply");
    let mut workspace = workspace();
    workspace.write_text("src/a.txt", "changed").unwrap();
    let patch = workspace.submit().unwrap();

    let lock = IntegrationLock::acquire(&root).unwrap();
    let integrated = workspace
        .integrate(&lock, &patch, &workspace.materialized())
        .unwrap();
    assert_eq!(integrated.get("src/a.txt"), Some(b"changed".as_slice()));
    assert_eq!(integrated.get("src/b.txt"), Some(b"two".as_slice()));

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_lock_is_exclusive_and_released_on_drop() {
    let root = temp_root("lock");
    let lock = IntegrationLock::acquire(&root).unwrap();
    assert_eq!(lock.root(), root.canonicalize().unwrap());
    assert!(matches!(
        IntegrationLock::acquire(&root),
        Err(FsError::IntegrationLocked(_))
    ));
    drop(lock);
    assert!(IntegrationLock::acquire(&root).is_ok());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn non_exclusive_workspaces_cannot_integrate() {
    let root = temp_root("wrong-mode");
    let policy = WorkspacePolicy::build(
        ".",
        &["**"],
        &["src/**"],
        &[],
        IsolationMode::CopyOnWrite,
        Limits::default(),
    )
    .unwrap();
    let mut workspace = Workspace::open(
        base(),
        EffectivePolicy::single(policy),
        IsolationMode::CopyOnWrite,
    );
    workspace.write_text("src/a.txt", "changed").unwrap();
    let patch = workspace.submit().unwrap();

    let lock = IntegrationLock::acquire(&root).unwrap();
    let error = workspace
        .integrate(&lock, &patch, &workspace.materialized())
        .unwrap_err();
    assert!(matches!(error, FsError::IntegrationRefused(_)));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn stale_or_conflicting_patches_are_refused() {
    let root = temp_root("conflict");
    let mut workspace = workspace();
    workspace.write_text("src/a.txt", "changed").unwrap();
    let patch = workspace.submit().unwrap();
    let lock = IntegrationLock::acquire(&root).unwrap();

    let stale = Patch::new("deadbeef".to_string(), patch.changes.clone());
    assert!(matches!(
        workspace.integrate(&lock, &stale, &workspace.materialized()),
        Err(FsError::IntegrationRefused(_))
    ));

    let conflicting = Patch::new(
        workspace.base_revision().to_string(),
        vec![FileChange {
            path: "src/a.txt".to_string(),
            kind: FileChangeKind::Modified,
            base_hash: Some("not-the-base-hash".to_string()),
            new_hash: Some("also-wrong".to_string()),
        }],
    );
    assert!(matches!(
        workspace.integrate(&lock, &conflicting, &workspace.materialized()),
        Err(FsError::IntegrationRefused(_))
    ));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn protected_files_are_refused_at_integration() {
    let root = temp_root("protected");
    let mut files = BTreeMap::new();
    files.insert("axiom.yaml".to_string(), b"version: 1\n".to_vec());
    files.insert("src/a.txt".to_string(), b"one".to_vec());
    let base = Snapshot::from_files(files);
    let workspace = Workspace::open(
        base,
        EffectivePolicy::single(policy()),
        IsolationMode::ExclusiveIntegration,
    );

    let hash = axiom_hash::hex_digest(b"version: 2\n");
    let patch = Patch::new(
        workspace.base_revision().to_string(),
        vec![FileChange {
            path: "axiom.yaml".to_string(),
            kind: FileChangeKind::Modified,
            base_hash: Some(axiom_hash::hex_digest(b"version: 1\n")),
            new_hash: Some(hash),
        }],
    );
    let mut content = BTreeMap::new();
    content.insert("axiom.yaml".to_string(), b"version: 2\n".to_vec());
    let lock = IntegrationLock::acquire(&root).unwrap();

    let error = workspace.integrate(&lock, &patch, &content).unwrap_err();
    assert!(matches!(error, FsError::Denied { .. }));
    std::fs::remove_dir_all(&root).ok();
}
