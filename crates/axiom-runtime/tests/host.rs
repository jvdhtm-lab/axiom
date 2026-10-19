//! Runtime host integration: secure workspaces and artifacts from Axiom code.

use std::collections::BTreeMap;

use axiom_artifacts::ArtifactStore;
use axiom_filesystem::{
    EffectivePolicy, IsolationMode, Limits, Snapshot, Workspace, WorkspacePolicy,
};
use axiom_ir::{lower, ModuleInput, Program, Target};
use axiom_runtime::{run, run_with_host, Host, Value, WorkspaceHost};
use axiom_syntax::parse;

fn load(src: &str) -> Program {
    let file = parse(src).file;
    let inputs = [ModuleInput {
        name: "t".to_string(),
        display: "t.ax".to_string(),
        file: &file,
    }];
    lower(&inputs, Target::Server)
}

fn host(files: &[(&str, &str)], read: &[&str], write: &[&str]) -> WorkspaceHost {
    let mut tree = BTreeMap::new();
    for (path, contents) in files {
        tree.insert(path.to_string(), contents.as_bytes().to_vec());
    }
    let policy = WorkspacePolicy::build(
        ".",
        read,
        write,
        &[],
        IsolationMode::CopyOnWrite,
        Limits::default(),
    )
    .unwrap();
    let workspace = Workspace::open(
        Snapshot::from_files(tree),
        EffectivePolicy::single(policy),
        IsolationMode::CopyOnWrite,
    );
    WorkspaceHost::new(workspace, ArtifactStore::new())
}

const SOURCE: &str = r#"
module t;

fn read() -> String {
  return fs.read_text("src/a.ax");
}

fn write() {
  fs.write_text("src/b.ax", "hi");
}
"#;

#[test]
fn reads_through_the_workspace_policy() {
    let program = load(SOURCE);
    let host = host(&[("src/a.ax", "hello")], &["src/**"], &["src/**"]);
    assert_eq!(
        run_with_host(&program, "read", vec![], &host).unwrap(),
        Value::Str("hello".to_string())
    );
    // Without a host the evaluator still fails closed.
    assert!(run(&program, "read", vec![]).is_err());
}

#[test]
fn writes_stay_in_the_private_overlay() {
    let program = load(SOURCE);
    let host = host(&[], &["src/**"], &["src/**"]);
    run_with_host(&program, "write", vec![], &host).unwrap();
    assert_eq!(host.workspace().read_text("src/b.ax").unwrap(), "hi");
}

#[test]
fn policy_denials_fail_the_call() {
    let program = load(
        r#"
module t;

fn read() -> String {
  return fs.read_text("secret.txt");
}
"#,
    );
    let host = host(&[("secret.txt", "x")], &["src/**"], &["src/**"]);
    let error = run_with_host(&program, "read", vec![], &host).unwrap_err();
    assert!(error.message.contains("denied"), "{}", error.message);
}

#[test]
fn artifacts_are_content_addressed_and_verified() {
    let host = host(&[], &["**"], &["**"]);
    let id = host
        .call(
            "artifacts",
            "publish",
            &[(None, Value::Bytes(b"hello".to_vec()))],
        )
        .unwrap();
    let Value::Str(id) = id else {
        panic!("publish should return an id");
    };

    assert_eq!(
        host.call("artifacts", "fetch", &[(None, Value::Str(id.clone()))])
            .unwrap(),
        Value::Bytes(b"hello".to_vec())
    );
    assert_eq!(
        host.call("artifacts", "verify", &[(None, Value::Str(id.clone()))])
            .unwrap(),
        Value::Bool(true)
    );
    assert!(matches!(
        host.call("artifacts", "inspect", &[(None, Value::Str(id))])
            .unwrap(),
        Value::Record(_)
    ));
    assert!(host
        .call(
            "artifacts",
            "fetch",
            &[(None, Value::Str("missing".into()))]
        )
        .is_err());
}

#[test]
fn snapshots_restore_the_staged_view() {
    let program = load(SOURCE);
    let host = host(&[], &["src/**"], &["src/**"]);
    run_with_host(&program, "write", vec![], &host).unwrap();
    let snapshot = host.call("workspaces", "snapshot", &[]).unwrap();

    host.workspace_mut()
        .write_text("src/b.ax", "changed")
        .unwrap();
    assert_eq!(host.workspace().read_text("src/b.ax").unwrap(), "changed");

    host.call("workspaces", "restore", &[(None, snapshot)])
        .unwrap();
    assert_eq!(host.workspace().read_text("src/b.ax").unwrap(), "hi");
}
