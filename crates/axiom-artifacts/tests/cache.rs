//! Reproducible build keys, integrity, freshness, tags, and partition isolation.

use axiom_artifacts::{BuildCache, BuildKey, CacheError, CachePartition, DataCache, DataCacheKey};

fn build_key() -> BuildKey {
    BuildKey::new(
        "src-hash",
        "compiler-1.0",
        "wasm_browser",
        "options-hash",
        1,
    )
    .dependency("core", "core-hash")
    .contract("api", "1.0.0")
}

#[test]
fn every_build_input_changes_the_key() {
    let base = build_key();
    let variations = [
        BuildKey::new(
            "other-src",
            "compiler-1.0",
            "wasm_browser",
            "options-hash",
            1,
        )
        .dependency("core", "core-hash")
        .contract("api", "1.0.0"),
        BuildKey::new(
            "src-hash",
            "compiler-2.0",
            "wasm_browser",
            "options-hash",
            1,
        )
        .dependency("core", "core-hash")
        .contract("api", "1.0.0"),
        BuildKey::new("src-hash", "compiler-1.0", "server", "options-hash", 1)
            .dependency("core", "core-hash")
            .contract("api", "1.0.0"),
        BuildKey::new(
            "src-hash",
            "compiler-1.0",
            "wasm_browser",
            "other-options",
            1,
        )
        .dependency("core", "core-hash")
        .contract("api", "1.0.0"),
        BuildKey::new(
            "src-hash",
            "compiler-1.0",
            "wasm_browser",
            "options-hash",
            2,
        )
        .dependency("core", "core-hash")
        .contract("api", "1.0.0"),
        BuildKey::new(
            "src-hash",
            "compiler-1.0",
            "wasm_browser",
            "options-hash",
            1,
        )
        .dependency("core", "other-core")
        .contract("api", "1.0.0"),
        BuildKey::new(
            "src-hash",
            "compiler-1.0",
            "wasm_browser",
            "options-hash",
            1,
        )
        .dependency("core", "core-hash")
        .contract("api", "2.0.0"),
    ];
    for variation in variations {
        assert_ne!(base.digest(), variation.digest());
    }
    assert_eq!(base.digest(), build_key().digest());
    assert!(base.digest().len() == 64);
}

#[test]
fn build_cache_is_reproducible_and_content_checked() {
    let mut cache = BuildCache::new();
    let key = build_key();
    let id = cache.insert(&key, b"wasm-bytes".to_vec()).unwrap();
    assert_eq!(id.len(), 64);
    assert_eq!(cache.get(&key), Some(b"wasm-bytes".as_slice()));

    let error = cache.insert(&key, b"different".to_vec()).unwrap_err();
    assert!(matches!(error, CacheError::NotReproducible(_)), "{error}");
    assert_eq!(cache.len(), 1);

    let mut other = key.clone();
    other.target = "server".into();
    assert!(cache.get(&other).is_none());
    cache.insert(&other, b"server-bytes".to_vec()).unwrap();
    assert_eq!(cache.len(), 2);
}

#[test]
fn build_cache_evicts_oldest_and_enforces_limit() {
    let mut cache = BuildCache::new().with_max_bytes(8);
    let error = cache
        .insert(&build_key(), b"0123456789".to_vec())
        .unwrap_err();
    assert!(matches!(error, CacheError::TooLarge { .. }), "{error}");

    let mut first = build_key();
    first.source_hash = "one".into();
    let mut second = build_key();
    second.source_hash = "two".into();
    cache.insert(&first, b"aaaa".to_vec()).unwrap();
    cache.insert(&second, b"bbbbb".to_vec()).unwrap();

    assert!(cache.get(&first).is_none());
    assert_eq!(cache.get(&second), Some(b"bbbbb".as_slice()));
    assert_eq!(cache.bytes(), 5);
}

fn keys() -> (DataCacheKey, DataCacheKey) {
    let alice = CachePartition::new("tenant-a", "alice");
    let bob = CachePartition::new("tenant-a", "bob");
    (
        DataCacheKey::new("tasks.list", "v1", b"filter=open", alice, "internal"),
        DataCacheKey::new("tasks.list", "v1", b"filter=open", bob, "internal"),
    )
}

#[test]
fn data_cache_honours_ttl_and_stale_policy() {
    let mut cache = DataCache::new(true);
    let (alice, _) = keys();
    cache
        .put(&alice, b"payload".to_vec(), 100, Some(10), &["tasks"])
        .unwrap();

    assert_eq!(cache.get(&alice, 105), Some(b"payload".as_slice()));
    assert_eq!(cache.get(&alice, 110), None);
    assert_eq!(cache.get_on_origin_error(&alice, 111), None);

    let mut stale_ok = DataCache::new(true).with_serve_stale_on_error(true);
    stale_ok
        .put(&alice, b"payload".to_vec(), 100, Some(10), &[])
        .unwrap();
    assert_eq!(
        stale_ok.get_on_origin_error(&alice, 111),
        Some(b"payload".as_slice())
    );
}

#[test]
fn data_cache_invalidates_by_tag_and_partition() {
    let mut cache = DataCache::new(true);
    let alice = CachePartition::new("tenant-a", "alice");
    let bob = CachePartition::new("tenant-a", "bob");
    let tasks = DataCacheKey::new("tasks.list", "v1", b"a", alice.clone(), "internal");
    let teams = DataCacheKey::new("teams.list", "v1", b"b", alice.clone(), "internal");
    let bob_tasks = DataCacheKey::new("tasks.list", "v1", b"a", bob.clone(), "internal");

    cache
        .put(&tasks, b"tasks".to_vec(), 0, Some(60), &["tasks"])
        .unwrap();
    cache
        .put(&teams, b"teams".to_vec(), 0, Some(60), &["teams"])
        .unwrap();
    cache
        .put(&bob_tasks, b"bob".to_vec(), 0, Some(60), &["tasks"])
        .unwrap();

    assert_eq!(cache.invalidate_tags(&["tasks"]), 2);
    assert!(cache.get(&tasks, 0).is_none());
    assert!(cache.get(&bob_tasks, 0).is_none());
    assert_eq!(cache.get(&teams, 0), Some(b"teams".as_slice()));

    cache
        .put(&tasks, b"tasks".to_vec(), 0, Some(60), &[])
        .unwrap();
    assert_eq!(cache.invalidate_partition(&alice), 2);
    assert!(cache.get(&tasks, 0).is_none());
    assert!(cache.get(&teams, 0).is_none());
}

#[test]
fn partitions_cannot_read_each_others_data() {
    let mut cache = DataCache::new(true);
    let (alice, bob) = keys();
    cache
        .put(&alice, b"alice-data".to_vec(), 0, Some(60), &[])
        .unwrap();

    assert_eq!(cache.get(&alice, 0), Some(b"alice-data".as_slice()));
    assert!(cache.get(&bob, 0).is_none());
    assert!(cache.get_on_origin_error(&bob, 0).is_none());
}

#[test]
fn raw_inputs_are_hashed_not_stored_in_keys() {
    let partition = CachePartition::new("tenant-a", "alice");
    let key = DataCacheKey::new("search", "v1", b"secret-token-123", partition, "internal");
    assert_eq!(key.input_hash.len(), 64);
    assert!(!key.digest().contains("secret"));
    assert_eq!(key.digest().len(), 64);

    let same = DataCacheKey::new(
        "search",
        "v1",
        b"secret-token-123",
        CachePartition::new("tenant-a", "alice"),
        "internal",
    );
    assert_eq!(key, same);
}

#[test]
fn data_cache_requires_ttl_and_can_be_disabled() {
    let mut cache = DataCache::new(true);
    let (alice, _) = keys();
    let error = cache
        .put(&alice, b"x".to_vec(), 0, Some(0), &[])
        .unwrap_err();
    assert_eq!(error, CacheError::TtlRequired);

    let mut disabled = DataCache::new(false);
    let error = disabled
        .put(&alice, b"x".to_vec(), 0, Some(10), &[])
        .unwrap_err();
    assert_eq!(error, CacheError::Disabled);
    assert!(disabled.get(&alice, 0).is_none());
}

#[test]
fn purge_expired_and_counters() {
    let mut cache = DataCache::new(true);
    let (alice, bob) = keys();
    cache.put(&alice, b"a".to_vec(), 0, Some(10), &[]).unwrap();
    cache.put(&bob, b"bb".to_vec(), 0, Some(100), &[]).unwrap();
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.bytes(), 3);
    assert_eq!(cache.stored_at(&alice), Some(0));

    assert_eq!(cache.purge_expired(20), 1);
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.bytes(), 2);
    assert_eq!(cache.get(&alice, 0), None);
}
