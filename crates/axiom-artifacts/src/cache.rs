//! Build and server-data caches with explicit keys, freshness, and isolation.
//!
//! Build keys cover source, dependencies, compiler, target, options, ABI, and
//! contract versions, so a cache hit is reproducible rather than time-based.
//! Server-data keys hash their inputs (never raw credentials), include the
//! tenant/principal partition, and carry an explicit TTL and tags. Stale data
//! is never served on error unless the configuration opts in.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use axiom_hash::hex_digest;

/// Everything that can change a build's output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildKey {
    pub source_hash: String,
    pub dependency_hashes: BTreeMap<String, String>,
    pub compiler_version: String,
    pub target: String,
    pub options_hash: String,
    pub abi_version: u32,
    pub contract_versions: BTreeMap<String, String>,
}

impl BuildKey {
    pub fn new(
        source_hash: impl Into<String>,
        compiler_version: impl Into<String>,
        target: impl Into<String>,
        options_hash: impl Into<String>,
        abi_version: u32,
    ) -> Self {
        BuildKey {
            source_hash: source_hash.into(),
            dependency_hashes: BTreeMap::new(),
            compiler_version: compiler_version.into(),
            target: target.into(),
            options_hash: options_hash.into(),
            abi_version,
            contract_versions: BTreeMap::new(),
        }
    }

    pub fn dependency(mut self, name: impl Into<String>, hash: impl Into<String>) -> Self {
        self.dependency_hashes.insert(name.into(), hash.into());
        self
    }

    pub fn contract(mut self, name: impl Into<String>, version: impl Into<String>) -> Self {
        self.contract_versions.insert(name.into(), version.into());
        self
    }

    /// The deterministic cache key.
    pub fn digest(&self) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "source:{};compiler:{};target:{};options:{};abi:{};",
            self.source_hash,
            self.compiler_version,
            self.target,
            self.options_hash,
            self.abi_version
        );
        for (name, hash) in &self.dependency_hashes {
            let _ = write!(out, "dep:{name}:{hash};");
        }
        for (name, version) in &self.contract_versions {
            let _ = write!(out, "contract:{name}:{version};");
        }
        hex_digest(out.as_bytes())
    }
}

#[derive(Debug, Clone)]
struct BuildEntry {
    id: String,
    bytes: Vec<u8>,
    sequence: u64,
}

/// A reproducible, content-checked build cache.
#[derive(Debug, Default)]
pub struct BuildCache {
    max_bytes: Option<usize>,
    entries: BTreeMap<String, BuildEntry>,
    sequence: u64,
    bytes: usize,
}

impl BuildCache {
    pub fn new() -> Self {
        BuildCache::default()
    }

    pub fn with_max_bytes(mut self, max_bytes: usize) -> Self {
        self.max_bytes = Some(max_bytes);
        self
    }

    /// Store the output of a build. A second build under the same key must
    /// return identical bytes; anything else is a reproducibility failure.
    pub fn insert(&mut self, key: &BuildKey, bytes: Vec<u8>) -> Result<String, CacheError> {
        let cache_key = key.digest();
        let id = hex_digest(&bytes);
        if let Some(existing) = self.entries.get(&cache_key) {
            if existing.bytes == bytes {
                return Ok(id);
            }
            return Err(CacheError::NotReproducible(cache_key));
        }
        if let Some(max) = self.max_bytes {
            if bytes.len() > max {
                return Err(CacheError::TooLarge {
                    bytes: bytes.len(),
                    max,
                });
            }
        }
        self.sequence += 1;
        self.bytes += bytes.len();
        self.entries.insert(
            cache_key,
            BuildEntry {
                id: id.clone(),
                bytes,
                sequence: self.sequence,
            },
        );
        self.evict();
        Ok(id)
    }

    /// Fetch a cached build, re-checking its integrity before reuse.
    pub fn get(&self, key: &BuildKey) -> Option<&[u8]> {
        let entry = self.entries.get(&key.digest())?;
        if hex_digest(&entry.bytes) != entry.id {
            return None;
        }
        Some(&entry.bytes)
    }

    pub fn contains(&self, key: &BuildKey) -> bool {
        self.get(key).is_some()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }

    fn evict(&mut self) {
        let Some(max) = self.max_bytes else {
            return;
        };
        while self.bytes > max && !self.entries.is_empty() {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.sequence)
                .map(|(key, _)| key.clone());
            let Some(key) = oldest else { break };
            if let Some(entry) = self.entries.remove(&key) {
                self.bytes = self.bytes.saturating_sub(entry.bytes.len());
            }
        }
    }
}

/// The tenant/principal partition a cached server-data response belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct CachePartition {
    pub tenant: String,
    pub principal: String,
}

impl CachePartition {
    pub fn new(tenant: impl Into<String>, principal: impl Into<String>) -> Self {
        CachePartition {
            tenant: tenant.into(),
            principal: principal.into(),
        }
    }
}

/// A server-data cache key. Inputs are hashed at construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataCacheKey {
    pub operation: String,
    pub version: String,
    pub input_hash: String,
    pub partition: CachePartition,
    pub data_policy: String,
}

impl DataCacheKey {
    pub fn new(
        operation: impl Into<String>,
        version: impl Into<String>,
        input: &[u8],
        partition: CachePartition,
        data_policy: impl Into<String>,
    ) -> Self {
        DataCacheKey {
            operation: operation.into(),
            version: version.into(),
            input_hash: hex_digest(input),
            partition,
            data_policy: data_policy.into(),
        }
    }

    pub fn digest(&self) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "op:{};version:{};input:{};tenant:{};principal:{};policy:{};",
            self.operation,
            self.version,
            self.input_hash,
            self.partition.tenant,
            self.partition.principal,
            self.data_policy
        );
        hex_digest(out.as_bytes())
    }
}

#[derive(Debug, Clone)]
struct DataEntry {
    bytes: Vec<u8>,
    tags: BTreeSet<String>,
    partition: CachePartition,
    stored_at: u64,
    expires_at: u64,
    sequence: u64,
}

impl DataEntry {
    fn fresh(&self, now: u64) -> bool {
        now < self.expires_at
    }
}

/// A server-data cache with TTLs, tags, and optional stale-on-error serving.
#[derive(Debug)]
pub struct DataCache {
    enabled: bool,
    max_entries: usize,
    max_bytes: usize,
    default_ttl_seconds: u64,
    serve_stale_on_error: bool,
    entries: BTreeMap<String, DataEntry>,
    sequence: u64,
    bytes: usize,
}

impl Default for DataCache {
    fn default() -> Self {
        DataCache::new(true)
    }
}

impl DataCache {
    pub fn new(enabled: bool) -> Self {
        DataCache {
            enabled,
            max_entries: 128,
            max_bytes: 8 * 1024 * 1024,
            default_ttl_seconds: 60,
            serve_stale_on_error: false,
            entries: BTreeMap::new(),
            sequence: 0,
            bytes: 0,
        }
    }

    pub fn with_limits(mut self, max_entries: usize, max_bytes: usize) -> Self {
        self.max_entries = max_entries;
        self.max_bytes = max_bytes;
        self
    }

    pub fn with_default_ttl_seconds(mut self, seconds: u64) -> Self {
        self.default_ttl_seconds = seconds;
        self
    }

    /// Opt in to serving expired data when the origin errors. Off by default.
    pub fn with_serve_stale_on_error(mut self, enabled: bool) -> Self {
        self.serve_stale_on_error = enabled;
        self
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn serve_stale_on_error(&self) -> bool {
        self.serve_stale_on_error
    }

    /// Insert a response with the configured TTL, or `ttl_seconds` when given.
    pub fn put(
        &mut self,
        key: &DataCacheKey,
        bytes: Vec<u8>,
        now: u64,
        ttl_seconds: Option<u64>,
        tags: &[&str],
    ) -> Result<(), CacheError> {
        if !self.enabled {
            return Err(CacheError::Disabled);
        }
        let ttl = ttl_seconds.unwrap_or(self.default_ttl_seconds);
        if ttl == 0 {
            return Err(CacheError::TtlRequired);
        }
        if bytes.len() > self.max_bytes {
            return Err(CacheError::TooLarge {
                bytes: bytes.len(),
                max: self.max_bytes,
            });
        }
        self.sequence += 1;
        let digest = key.digest();
        if let Some(previous) = self.entries.remove(&digest) {
            self.bytes = self.bytes.saturating_sub(previous.bytes.len());
        }
        self.bytes += bytes.len();
        self.entries.insert(
            digest,
            DataEntry {
                bytes,
                tags: tags.iter().map(|tag| (*tag).to_string()).collect(),
                partition: key.partition.clone(),
                stored_at: now,
                expires_at: now.saturating_add(ttl),
                sequence: self.sequence,
            },
        );
        self.evict();
        Ok(())
    }

    /// Fetch fresh data only.
    pub fn get(&self, key: &DataCacheKey, now: u64) -> Option<&[u8]> {
        if !self.enabled {
            return None;
        }
        let entry = self.entries.get(&key.digest())?;
        entry.fresh(now).then_some(entry.bytes.as_slice())
    }

    /// Fetch data when the origin failed. Returns fresh data, or expired data
    /// only if `serve_stale_on_error` was enabled.
    pub fn get_on_origin_error(&self, key: &DataCacheKey, now: u64) -> Option<&[u8]> {
        if !self.enabled {
            return None;
        }
        let entry = self.entries.get(&key.digest())?;
        if entry.fresh(now) || self.serve_stale_on_error {
            Some(entry.bytes.as_slice())
        } else {
            None
        }
    }

    /// Invalidate every entry carrying at least one of `tags`.
    pub fn invalidate_tags(&mut self, tags: &[&str]) -> usize {
        let before = self.entries.len();
        let mut removed_bytes = 0;
        self.entries.retain(|_, entry| {
            let hit = tags.iter().any(|tag| entry.tags.contains(*tag));
            if hit {
                removed_bytes += entry.bytes.len();
            }
            !hit
        });
        self.bytes = self.bytes.saturating_sub(removed_bytes);
        before - self.entries.len()
    }

    /// Drop all cached data for one principal/tenant partition.
    pub fn invalidate_partition(&mut self, partition: &CachePartition) -> usize {
        let before = self.entries.len();
        let mut removed_bytes = 0;
        self.entries.retain(|_, entry| {
            if &entry.partition == partition {
                removed_bytes += entry.bytes.len();
                false
            } else {
                true
            }
        });
        self.bytes = self.bytes.saturating_sub(removed_bytes);
        before - self.entries.len()
    }

    /// Non-fresh entries older than the TTL, removed regardless of tags.
    pub fn purge_expired(&mut self, now: u64) -> usize {
        let before = self.entries.len();
        let mut removed_bytes = 0;
        self.entries.retain(|_, entry| {
            if entry.fresh(now) {
                true
            } else {
                removed_bytes += entry.bytes.len();
                false
            }
        });
        self.bytes = self.bytes.saturating_sub(removed_bytes);
        before - self.entries.len()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Stored-at time for an entry; useful for diagnostics.
    pub fn stored_at(&self, key: &DataCacheKey) -> Option<u64> {
        self.entries.get(&key.digest()).map(|entry| entry.stored_at)
    }

    fn evict(&mut self) {
        while (self.entries.len() > self.max_entries || self.bytes > self.max_bytes)
            && !self.entries.is_empty()
        {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.sequence)
                .map(|(key, _)| key.clone());
            let Some(key) = oldest else { break };
            if let Some(entry) = self.entries.remove(&key) {
                self.bytes = self.bytes.saturating_sub(entry.bytes.len());
            }
        }
    }
}

/// Cache failures that must not be silently swallowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheError {
    Disabled,
    TtlRequired,
    TooLarge { bytes: usize, max: usize },
    NotReproducible(String),
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheError::Disabled => write!(f, "the data cache is disabled"),
            CacheError::TtlRequired => write!(f, "cached data requires a positive TTL"),
            CacheError::TooLarge { bytes, max } => {
                write!(f, "cached entry is {bytes} bytes; the limit is {max}")
            }
            CacheError::NotReproducible(key) => {
                write!(f, "build `{key}` produced different bytes for the same key")
            }
        }
    }
}

impl std::error::Error for CacheError {}
