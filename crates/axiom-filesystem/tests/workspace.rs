//! Workspace path/security/isolation/quota and patch-conflict behavior.

use std::collections::BTreeMap;

use axiom_filesystem::{
    EffectivePolicy, FileChangeKind, FsError, IsolationMode, Limits, Snapshot, Workspace,
    WorkspacePolicy,
};

fn base() -> Snapshot {
    let mut files = BTreeMap::new();
    files.insert("src/frontend/app.ax".to_string(), b"render(state)".to_vec());
    files.insert("src/shared/util.ax".to_string(), b"fn util() {}".to_vec());
    files.insert("src/server/db.ax".to_string(), b"fn db() {}".to_vec());
    files.insert("axiom.yaml".to_string(), b"version: 1".to_vec());
    Snapshot::from_files(files)
}

fn policy(limits: Limits, isolation: IsolationMode) -> WorkspacePolicy {
    WorkspacePolicy::build(
        ".",
        &["src/**"],
        &["src/frontend/**"],
        &["src/server/**"],
        isolation,
        limits,
    )
    .unwrap()
}

fn workspace(limits: Limits) -> Workspace {
    Workspace::open(
        base(),
        EffectivePolicy::single(policy(limits, IsolationMode::CopyOnWrite)),
        IsolationMode::CopyOnWrite,
    )
}

#[test]
fn path_and_permission_boundaries() {
    let ws = workspace(Limits::default());
    assert!(ws.read_text("src/frontend/app.ax").is_ok());
    assert!(ws.read_text("src/shared/util.ax").is_ok());

    assert!(matches!(
        ws.read_text("src/server/db.ax"),
        Err(FsError::Denied { .. })
    ));
    assert!(matches!(
        ws.read_text("axiom.yaml"),
        Err(FsError::Denied { .. })
    ));
    assert!(matches!(
        ws.read_text("docs/readme.md"),
        Err(FsError::Denied { .. })
    ));
    assert!(matches!(
        ws.read_text("../escape"),
        Err(FsError::InvalidPath(_))
    ));
}

#[test]
fn writes_are_confined_to_the_write_set() {
    let mut ws = workspace(Limits::default());
    assert!(ws.write_text("src/frontend/app.ax", "new").is_ok());
    assert!(matches!(
        ws.write_text("src/shared/util.ax", "x"),
        Err(FsError::Denied { .. })
    ));
    assert!(matches!(
        ws.write_text("axiom.yaml", "x"),
        Err(FsError::Denied { .. })
    ));
}

#[test]
fn read_only_workspace_refuses_writes() {
    let mut ws = Workspace::open(
        base(),
        EffectivePolicy::single(policy(Limits::default(), IsolationMode::ReadOnly)),
        IsolationMode::ReadOnly,
    );
    assert!(matches!(
        ws.write_text("src/frontend/app.ax", "x"),
        Err(FsError::ReadOnly(_))
    ));
}

#[test]
fn copy_on_write_keeps_base_pristine() {
    let mut ws = workspace(Limits::default());
    let before = ws.base_revision().to_string();
    ws.write_text("src/frontend/app.ax", "changed").unwrap();

    // The private view sees the change...
    assert_eq!(ws.read_text("src/frontend/app.ax").unwrap(), "changed");
    // ...but the base snapshot is unchanged.
    assert_eq!(ws.base_revision(), before);
    assert_eq!(ws.base_revision(), base().revision());

    let patch = ws.diff();
    assert_eq!(patch.changes.len(), 1);
    assert_eq!(patch.changes[0].kind, FileChangeKind::Modified);
    assert_eq!(patch.changes[0].path, "src/frontend/app.ax");

    ws.discard();
    assert_eq!(
        ws.read_text("src/frontend/app.ax").unwrap(),
        "render(state)"
    );
    assert!(ws.diff().is_empty());
}

#[test]
fn snapshot_and_restore_round_trip() {
    let mut ws = workspace(Limits::default());
    ws.write_text("src/frontend/app.ax", "one").unwrap();
    let snap = ws.snapshot();

    ws.write_text("src/frontend/app.ax", "two").unwrap();
    assert_eq!(ws.read_text("src/frontend/app.ax").unwrap(), "two");

    ws.restore(snap);
    assert_eq!(ws.read_text("src/frontend/app.ax").unwrap(), "one");
}

#[test]
fn file_size_and_changed_file_quotas() {
    let mut small = workspace(Limits {
        max_file_bytes: Some(4),
        ..Limits::default()
    });
    assert!(matches!(
        small.write_text("src/frontend/app.ax", "too long"),
        Err(FsError::Limit { .. })
    ));

    let mut limited = workspace(Limits {
        max_changed_files: Some(1),
        ..Limits::default()
    });
    limited.write_text("src/frontend/app.ax", "a").unwrap();
    limited.write_text("src/frontend/new.ax", "b").unwrap();
    assert!(matches!(limited.submit(), Err(FsError::Limit { .. })));
}

#[test]
fn patch_conflicts_are_detected_against_a_moved_base() {
    let mut ws = workspace(Limits::default());
    ws.write_text("src/frontend/app.ax", "rebuilt").unwrap();
    let patch = ws.submit().unwrap();
    assert!(patch.check_conflicts(&base()).is_empty());

    let mut moved = BTreeMap::new();
    for (path, bytes) in base().iter() {
        moved.insert(path.clone(), bytes.clone());
    }
    moved.insert("src/frontend/app.ax".to_string(), b"someone else".to_vec());
    let moved = Snapshot::from_files(moved);
    let conflicts = patch.check_conflicts(&moved);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].path, "src/frontend/app.ax");
}

#[test]
fn list_and_search_respect_read_permissions() {
    let ws = workspace(Limits::default());
    let listed = ws.list("src").unwrap();
    assert_eq!(
        listed,
        vec![
            "src/frontend/app.ax".to_string(),
            "src/shared/util.ax".to_string()
        ]
    );

    let hits = ws.search("util").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "src/shared/util.ax");
    assert_eq!(hits[0].line, 1);
}
