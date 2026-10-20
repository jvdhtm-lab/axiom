//! Hot-swap, migration, rollback, and isolation for typed frontend state.

use axiom_state::{
    MigrationReport, Partition, StatePersistence, StateScope, StateSpec, StateStore, StateValue,
};

fn record(fields: &[(&str, StateValue)]) -> StateValue {
    StateValue::Record(
        fields
            .iter()
            .map(|(name, value)| ((*name).to_string(), value.clone()))
            .collect(),
    )
}

fn spec(version: u32) -> StateSpec {
    StateSpec::new("editor", "EditorState", version)
        .with_scope(StateScope::Route)
        .with_persistence(StatePersistence::Session)
}

fn partition(principal: &str) -> Partition {
    Partition::new("tenant-a", principal)
}

fn editor(count: i64) -> StateValue {
    record(&[
        ("count", StateValue::Int(count)),
        ("draft", StateValue::Str("hello".into())),
    ])
}

fn with_version(version: u32) -> StateStore {
    let mut store = StateStore::new(spec(version));
    store
        .migrator_mut()
        .add(1, 2, "migrate_v2", |value| {
            let mut fields = match value {
                StateValue::Record(fields) => fields,
                other => return Err(format!("expected a record, found {other:?}")),
            };
            fields.push(("schema".into(), StateValue::Int(2)));
            Ok(StateValue::Record(fields))
        })
        .unwrap();
    store
}

#[test]
fn same_version_activation_is_a_hot_swap() {
    let mut store = with_version(2);
    let partition = partition("alice");
    store.save(&partition, editor(3)).unwrap();

    let report: MigrationReport = store.activate(&partition, 2).unwrap();
    assert_eq!(report.from, 2);
    assert_eq!(report.to, 2);
    assert!(report.steps.is_empty());
    assert_eq!(store.load(&partition).unwrap().value, editor(3));
}

#[test]
fn activation_migrates_and_commits_a_new_version() {
    let mut store = with_version(1);
    let partition = partition("alice");
    let revision = store.save(&partition, editor(3)).unwrap();

    let report = store.activate(&partition, 2).unwrap();
    assert_eq!(report.from, 1);
    assert_eq!(report.to, 2);
    assert_eq!(report.steps, vec!["migrate_v2".to_string()]);
    assert!(report.revision > revision);

    let stored = store.load(&partition).unwrap();
    assert_eq!(stored.schema_version, 2);
    assert_eq!(stored.value.field("schema"), Some(&StateValue::Int(2)));
    assert_eq!(stored.value.field("count"), Some(&StateValue::Int(3)));
}

#[test]
fn a_failed_migration_keeps_the_previous_state() {
    let mut store = with_version(1);
    store
        .migrator_mut()
        .add(2, 3, "broken", |_| Err("boom".into()))
        .unwrap();
    let partition = partition("alice");
    store.save(&partition, editor(3)).unwrap();

    let error = store.activate(&partition, 3).unwrap_err();
    assert!(error.to_string().contains("boom"), "{error}");
    let stored = store.load(&partition).unwrap();
    assert_eq!(stored.schema_version, 1);
    assert_eq!(stored.value, editor(3));
}

#[test]
fn a_migration_producing_invalid_state_is_not_committed() {
    let mut store = with_version(1);
    store
        .migrator_mut()
        .add(2, 3, "leak_token", |mut value| {
            if let StateValue::Record(fields) = &mut value {
                fields.push(("access_token".into(), StateValue::Str("secret".into())));
            }
            Ok(value)
        })
        .unwrap();
    let partition = partition("alice");
    store.save(&partition, editor(3)).unwrap();

    let error = store.activate(&partition, 3).unwrap_err();
    assert!(
        error.to_string().contains("must never be persisted"),
        "{error}"
    );
    let stored = store.load(&partition).unwrap();
    assert_eq!(stored.schema_version, 1);
    assert_eq!(stored.value, editor(3));
}

#[test]
fn downgrades_are_refused() {
    let mut store = with_version(2);
    let partition = partition("alice");
    store.save(&partition, editor(1)).unwrap();

    let error = store.activate(&partition, 1).unwrap_err();
    assert!(
        error.to_string().contains("refusing to downgrade"),
        "{error}"
    );
    assert_eq!(store.load(&partition).unwrap().schema_version, 2);
}

#[test]
fn partitions_are_isolated() {
    let mut store = with_version(2);
    let alice = partition("alice");
    let bob = partition("bob");
    store.save(&alice, editor(1)).unwrap();
    store.save(&bob, editor(2)).unwrap();

    assert_eq!(
        store.load(&alice).unwrap().value.field("count"),
        Some(&StateValue::Int(1))
    );
    assert_eq!(
        store.load(&bob).unwrap().value.field("count"),
        Some(&StateValue::Int(2))
    );

    assert!(store.quarantine(&alice));
    assert!(store.load(&alice).is_none());
    assert!(store.quarantined(&alice).is_some());
    assert_eq!(
        store.load(&bob).unwrap().value.field("count"),
        Some(&StateValue::Int(2))
    );

    assert!(store.clear_partition(&alice));
    assert!(store.quarantined(&alice).is_none());
    assert!(store.load(&bob).is_some());
}

#[test]
fn forbidden_fields_are_rejected_but_similar_names_are_allowed() {
    let mut store = with_version(2);
    let partition = partition("alice");

    let leaky = record(&[
        (
            "auth",
            record(&[("access_token", StateValue::Str("t".into()))]),
        ),
        (
            "items",
            StateValue::List(vec![record(&[("api_key", StateValue::Str("k".into()))])]),
        ),
    ]);
    let error = store.save(&partition, leaky).unwrap_err();
    assert_eq!(
        error.to_string(),
        "state field `access_token` must never be persisted"
    );
    assert!(store.load(&partition).is_none());

    let authorization = record(&[("authorization", StateValue::Str("allow".into()))]);
    let error = store.save(&partition, authorization).unwrap_err();
    assert!(error.to_string().contains("`authorization`"));

    let benign = record(&[
        ("token_count", StateValue::Int(12)),
        ("author", StateValue::Str("alice".into())),
        ("capability_name", StateValue::Str("viewer".into())),
    ]);
    store.save(&partition, benign).unwrap();
}

#[test]
fn state_must_be_a_record_and_within_its_limit() {
    let mut store = StateStore::new(spec(1).with_persistence(StatePersistence::Session));
    let partition = partition("alice");
    assert!(store.save(&partition, StateValue::Int(1)).is_err());

    let mut small = StateStore::new(
        spec(1)
            .with_persistence(StatePersistence::Session)
            .with_max_bytes(16),
    );
    let error = small.save(&partition, editor(1)).unwrap_err();
    assert!(error.to_string().contains("the limit is 16"), "{error}");
}

#[test]
fn non_persistent_state_cannot_be_saved() {
    let mut store = StateStore::new(StateSpec::new("hover", "HoverState", 1));
    let partition = partition("alice");
    let error = store.save(&partition, editor(1)).unwrap_err();
    assert!(error.to_string().contains("persistence `none`"), "{error}");
}

#[test]
fn invariants_run_before_commit() {
    let mut store = with_version(2);
    store.add_invariant("count_non_negative", |value| match value.field("count") {
        Some(StateValue::Int(count)) if *count >= 0 => Ok(()),
        _ => Err("count must be non-negative".into()),
    });
    let partition = partition("alice");
    let error = store.save(&partition, editor(-1)).unwrap_err();
    assert!(error.to_string().contains("count_non_negative"), "{error}");
    assert!(store.load(&partition).is_none());
    store.save(&partition, editor(0)).unwrap();
}

#[test]
fn snapshots_restore_and_detect_tampering() {
    let mut store = with_version(2);
    let partition = partition("alice");
    store.save(&partition, editor(1)).unwrap();
    let snapshot = store.snapshot(&partition).unwrap();

    store.save(&partition, editor(9)).unwrap();
    assert_eq!(
        store.load(&partition).unwrap().value.field("count"),
        Some(&StateValue::Int(9))
    );

    store.restore(snapshot.clone()).unwrap();
    assert_eq!(
        store.load(&partition).unwrap().value.field("count"),
        Some(&StateValue::Int(1))
    );

    let mut tampered = snapshot;
    tampered.value = editor(7);
    let error = store.restore(tampered).unwrap_err();
    assert!(
        error.to_string().contains("does not match its hash"),
        "{error}"
    );
    assert_eq!(
        store.load(&partition).unwrap().value.field("count"),
        Some(&StateValue::Int(1))
    );
}

#[test]
fn migration_registration_rejects_gaps_and_duplicates() {
    let mut store = with_version(1);
    let error = store.migrator_mut().add(2, 4, "skip", Ok).unwrap_err();
    assert!(error.to_string().contains("one version"), "{error}");

    let error = store.migrator_mut().add(1, 2, "dup", Ok).unwrap_err();
    assert!(error.to_string().contains("two migrations"), "{error}");

    let error = store.activate(&partition("alice"), 3).unwrap_err();
    assert!(error.to_string().contains("no state exists"), "{error}");
}
