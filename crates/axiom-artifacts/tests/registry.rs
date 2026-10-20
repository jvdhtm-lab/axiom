//! Release immutability, evidence binding, activation, revocation, and rollback.

use axiom_artifacts::{
    is_content_hash, ActivationPolicy, ArtifactKind, ArtifactRef, ArtifactStore, EvidenceStatus,
    RegistryError, ReleaseEvidence, ReleaseManifest, ReleaseRegistry,
};

fn manifest(store: &mut ArtifactStore, shell: &[u8], module: &[u8]) -> ReleaseManifest {
    let shell_id = store.publish(ArtifactKind::UiBundle, shell.to_vec());
    let module_id = store.publish(ArtifactKind::WasmModule, module.to_vec());
    let mut manifest = ReleaseManifest::new(
        ArtifactRef::new(ArtifactKind::UiBundle, shell_id),
        1,
        "compiler-1.0",
        1,
    )
    .module("app", ArtifactRef::new(ArtifactKind::WasmModule, module_id));
    let evidence = ReleaseEvidence::passed(manifest.candidate_hash(), "checks.ax_compile");
    manifest = manifest.evidence(evidence);
    manifest
}

fn publish(
    registry: &mut ReleaseRegistry,
    store: &mut ArtifactStore,
    shell: &[u8],
    module: &[u8],
) -> String {
    let manifest = manifest(store, shell, module);
    registry
        .publish(store, manifest, &ActivationPolicy::default())
        .unwrap()
}

#[test]
fn publish_is_content_addressed_and_idempotent() {
    let mut store = ArtifactStore::new();
    let mut registry = ReleaseRegistry::new();
    let manifest = manifest(&mut store, b"shell-a", b"module-a");
    let id = manifest.release_id();

    let first = registry
        .publish(&store, manifest.clone(), &ActivationPolicy::default())
        .unwrap();
    assert_eq!(first, id);
    assert!(is_content_hash(&first));
    assert_eq!(registry.len(), 1);

    let second = registry
        .publish(&store, manifest.clone(), &ActivationPolicy::default())
        .unwrap();
    assert_eq!(second, id);
    assert_eq!(registry.len(), 1);
    assert_eq!(registry.release(&id).unwrap(), &manifest);
}

#[test]
fn evidence_changes_the_release_id_but_not_the_candidate() {
    let mut store = ArtifactStore::new();
    let mut registry = ReleaseRegistry::new();
    let manifest = manifest(&mut store, b"shell", b"module");
    let candidate = manifest.candidate_hash();

    let mut reviewed = manifest.clone();
    reviewed
        .evidence
        .push(ReleaseEvidence::passed(candidate.clone(), "human.review"));

    assert_eq!(candidate, reviewed.candidate_hash());
    assert_ne!(manifest.release_id(), reviewed.release_id());

    registry
        .publish(&store, manifest, &ActivationPolicy::default())
        .unwrap();
    registry
        .publish(&store, reviewed, &ActivationPolicy::default())
        .unwrap();
    assert_eq!(registry.len(), 2);
}

#[test]
fn activation_switches_and_rollback_restores_the_previous_release() {
    let mut store = ArtifactStore::new();
    let mut registry = ReleaseRegistry::new();
    let policy = ActivationPolicy::default();
    let a = publish(&mut registry, &mut store, b"shell-a", b"module-a");
    let b = publish(&mut registry, &mut store, b"shell-b", b"module-b");

    registry.activate(&store, &policy, &a).unwrap();
    assert_eq!(registry.active_id(), Some(a.as_str()));
    assert_eq!(registry.previous_id(), None);

    registry.activate(&store, &policy, &b).unwrap();
    assert_eq!(registry.active_id(), Some(b.as_str()));
    assert_eq!(registry.previous_id(), Some(a.as_str()));

    let restored = registry.rollback(&store, &policy).unwrap();
    assert_eq!(restored, a);
    assert_eq!(registry.active_id(), Some(a.as_str()));
    assert_eq!(registry.previous_id(), Some(b.as_str()));

    let error = ReleaseRegistry::new()
        .rollback(&store, &policy)
        .unwrap_err();
    assert_eq!(error, RegistryError::NoPrevious);
}

#[test]
fn revoked_releases_cannot_be_activated_or_republished() {
    let mut store = ArtifactStore::new();
    let mut registry = ReleaseRegistry::new();
    let policy = ActivationPolicy::default();
    let manifest = manifest(&mut store, b"shell", b"module");
    let id = registry.publish(&store, manifest.clone(), &policy).unwrap();
    registry.activate(&store, &policy, &id).unwrap();

    registry.revoke(&id, "security regression").unwrap();
    assert!(registry.is_revoked(&id));

    let error = registry.activate(&store, &policy, &id).unwrap_err();
    assert!(matches!(error, RegistryError::Revoked { .. }), "{error}");
    assert_eq!(registry.active_id(), Some(id.as_str()));

    let error = registry.publish(&store, manifest, &policy).unwrap_err();
    assert!(matches!(error, RegistryError::Revoked { .. }), "{error}");

    let error = registry.revoke("missing", "nope").unwrap_err();
    assert!(matches!(error, RegistryError::UnknownRelease(_)));
}

#[test]
fn rollback_refuses_a_revoked_previous_release() {
    let mut store = ArtifactStore::new();
    let mut registry = ReleaseRegistry::new();
    let policy = ActivationPolicy::default();
    let a = publish(&mut registry, &mut store, b"shell-a", b"module-a");
    let b = publish(&mut registry, &mut store, b"shell-b", b"module-b");
    registry.activate(&store, &policy, &a).unwrap();
    registry.activate(&store, &policy, &b).unwrap();

    registry.revoke(&a, "withdrawn").unwrap();
    let error = registry.rollback(&store, &policy).unwrap_err();
    assert!(matches!(error, RegistryError::Revoked { .. }), "{error}");
    assert_eq!(registry.active_id(), Some(b.as_str()));
}

#[test]
fn activation_rechecks_artifacts_in_the_supplied_store() {
    let mut store = ArtifactStore::new();
    let mut registry = ReleaseRegistry::new();
    let policy = ActivationPolicy::default();
    let id = publish(&mut registry, &mut store, b"shell", b"module");

    let empty = ArtifactStore::new();
    let error = registry.activate(&empty, &policy, &id).unwrap_err();
    assert!(
        matches!(error, RegistryError::MissingArtifact(_)),
        "{error}"
    );
    assert_eq!(registry.active_id(), None);
}

#[test]
fn references_must_be_pinned_and_match_their_kind() {
    let mut store = ArtifactStore::new();
    let mut registry = ReleaseRegistry::new();
    let policy = ActivationPolicy::default();

    let mut floating = ReleaseManifest::new(
        ArtifactRef::new(ArtifactKind::UiBundle, "latest"),
        1,
        "compiler-1.0",
        1,
    );
    floating.evidence = vec![ReleaseEvidence::passed(
        floating.candidate_hash(),
        "checks.ax_compile",
    )];
    let error = registry.publish(&store, floating, &policy).unwrap_err();
    assert!(
        matches!(error, RegistryError::UnpinnedReference { .. }),
        "{error}"
    );

    let shell = store.publish(ArtifactKind::UiBundle, b"shell".to_vec());
    let mut mismatched = ReleaseManifest::new(
        ArtifactRef::new(ArtifactKind::WasmModule, shell.clone()),
        1,
        "compiler-1.0",
        1,
    );
    mismatched.evidence = vec![ReleaseEvidence::passed(
        mismatched.candidate_hash(),
        "checks.ax_compile",
    )];
    let error = registry.publish(&store, mismatched, &policy).unwrap_err();
    assert!(
        matches!(error, RegistryError::KindMismatch { .. }),
        "{error}"
    );
}

#[test]
fn accept_evidence_must_be_present_passed_and_bound() {
    let mut store = ArtifactStore::new();
    let mut registry = ReleaseRegistry::new();
    let policy = ActivationPolicy::default();
    let shell = store.publish(ArtifactKind::UiBundle, b"shell".to_vec());
    let base = ReleaseManifest::new(
        ArtifactRef::new(ArtifactKind::UiBundle, shell),
        1,
        "compiler-1.0",
        1,
    );

    let error = registry.publish(&store, base.clone(), &policy).unwrap_err();
    assert_eq!(error, RegistryError::EvidenceMissing);

    let mut failed = base.clone();
    failed.evidence = vec![ReleaseEvidence::failed(
        failed.candidate_hash(),
        "checks.ax_compile",
    )];
    let error = registry.publish(&store, failed, &policy).unwrap_err();
    assert_eq!(
        error,
        RegistryError::EvidenceFailed("checks.ax_compile".into())
    );

    let mut unbound = base;
    unbound.evidence = vec![ReleaseEvidence::passed("deadbeef", "checks.ax_compile")];
    let error = registry.publish(&store, unbound, &policy).unwrap_err();
    assert!(
        matches!(error, RegistryError::EvidenceNotBound { .. }),
        "{error}"
    );
}

#[test]
fn abi_mismatch_is_refused() {
    let mut store = ArtifactStore::new();
    let mut registry = ReleaseRegistry::new();
    let manifest = manifest(&mut store, b"shell", b"module");
    let error = registry
        .publish(&store, manifest, &ActivationPolicy::new(2))
        .unwrap_err();
    assert_eq!(
        error,
        RegistryError::AbiMismatch {
            release: 1,
            expected: 2
        }
    );
}

#[test]
fn retention_keeps_active_and_previous_releases() {
    let mut store = ArtifactStore::new();
    let mut registry = ReleaseRegistry::new();
    let policy = ActivationPolicy::default();
    let ids = (0..4)
        .map(|index| {
            let shell = format!("shell-{index}");
            let module = format!("module-{index}");
            publish(
                &mut registry,
                &mut store,
                shell.as_bytes(),
                module.as_bytes(),
            )
        })
        .collect::<Vec<_>>();

    registry.activate(&store, &policy, &ids[2]).unwrap();
    registry.activate(&store, &policy, &ids[3]).unwrap();

    let removed = registry.retain(1);
    assert_eq!(removed, vec![ids[0].clone()]);
    assert_eq!(registry.len(), 3);
    assert!(registry.release(&ids[1]).is_some());
    assert_eq!(registry.active_id(), Some(ids[3].as_str()));
    assert_eq!(registry.previous_id(), Some(ids[2].as_str()));
}

#[test]
fn evidence_status_labels_are_stable() {
    assert_eq!(EvidenceStatus::Passed.as_str(), "passed");
    assert_eq!(EvidenceStatus::Failed.as_str(), "failed");
    assert!(!is_content_hash("latest"));
    assert!(!is_content_hash("abc"));
    assert!(is_content_hash(&"a".repeat(64)));
}
