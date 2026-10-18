//! Content addressing, immutability, and integrity checks for artifacts.

use axiom_artifacts::{address, Artifact, ArtifactKind, ArtifactStore};

#[test]
fn address_is_deterministic_and_kind_scoped() {
    assert_eq!(
        address(ArtifactKind::Patch, b"abc"),
        address(ArtifactKind::Patch, b"abc")
    );
    assert_ne!(
        address(ArtifactKind::Patch, b"abc"),
        address(ArtifactKind::Other, b"abc")
    );
    assert_ne!(
        address(ArtifactKind::Patch, b"abc"),
        address(ArtifactKind::Patch, b"abd")
    );
}

#[test]
fn publish_fetch_and_inspect() {
    let mut store = ArtifactStore::new();
    let id = store.publish(ArtifactKind::WasmModule, b"\0asm".to_vec());

    let artifact = store.fetch(&id).expect("stored artifact");
    assert_eq!(artifact.kind, ArtifactKind::WasmModule);
    assert_eq!(artifact.bytes, b"\0asm");

    let info = store.inspect(&id).unwrap();
    assert_eq!(info.byte_len, 4);
    assert_eq!(info.kind, ArtifactKind::WasmModule);
    assert!(store.verify(&id));
}

#[test]
fn republishing_identical_content_is_idempotent() {
    let mut store = ArtifactStore::new();
    let first = store.publish(ArtifactKind::Patch, b"diff".to_vec());
    let second = store.publish(ArtifactKind::Patch, b"diff".to_vec());
    assert_eq!(first, second);
    assert_eq!(store.len(), 1);
}

#[test]
fn distinct_content_gets_distinct_ids() {
    let mut store = ArtifactStore::new();
    let a = store.publish(ArtifactKind::Patch, b"one".to_vec());
    let b = store.publish(ArtifactKind::Patch, b"two".to_vec());
    assert_ne!(a, b);
    assert_eq!(store.len(), 2);
}

#[test]
fn tampering_is_detected() {
    let mut store = ArtifactStore::new();
    let id = store.publish(ArtifactKind::Snapshot, b"state".to_vec());

    // A forged artifact whose bytes no longer match its claimed id.
    let forged = Artifact {
        id: id.clone(),
        kind: ArtifactKind::Snapshot,
        bytes: b"tampered".to_vec(),
    };
    assert!(!forged.verify());

    // The store's own copy still verifies.
    assert!(store.verify(&id));
    assert!(!store.verify("does-not-exist"));
    assert!(store.fetch("does-not-exist").is_none());
}
