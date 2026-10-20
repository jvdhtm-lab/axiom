//! Immutable, content-addressed artifact store (agents.md §6.2, Phase 9).
//!
//! An artifact's address is the SHA-256 of its kind and bytes, so identical
//! content always has one identity and a stored artifact can never be replaced
//! by different bytes under the same id. Integrity is checked by re-hashing on
//! demand; a mismatch is reported, never silently trusted.
//!
//! Releases are immutable manifests over artifacts; activation re-verifies
//! integrity and ABI, and revocation blocks future activation. Build and
//! server-data caches use explicit, reproducible keys with TTL/tag
//! invalidation and per-principal isolation.

pub mod cache;
pub mod registry;

use std::collections::BTreeMap;
use std::fmt;

use axiom_hash::hex_digest;

pub use cache::{BuildCache, BuildKey, CacheError, CachePartition, DataCache, DataCacheKey};
pub use registry::{
    is_content_hash, ActivationPolicy, ArtifactRef, EvidenceStatus, RegistryError, ReleaseEvidence,
    ReleaseManifest, ReleaseRegistry,
};

/// What an artifact contains. The kind is part of the address so a WASM module
/// and a patch with identical bytes never collide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    WasmModule,
    Patch,
    UiBundle,
    Snapshot,
    Other,
}

impl ArtifactKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ArtifactKind::WasmModule => "wasm_module",
            ArtifactKind::Patch => "patch",
            ArtifactKind::UiBundle => "ui_bundle",
            ArtifactKind::Snapshot => "snapshot",
            ArtifactKind::Other => "other",
        }
    }
}

/// An immutable artifact. `id` is content-addressed; the fields are public so
/// callers can *inspect* an artifact and detect tampering, but a store never
/// mints one with a mismatched id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub id: String,
    pub kind: ArtifactKind,
    pub bytes: Vec<u8>,
}

impl Artifact {
    /// Create an artifact, computing its content address.
    pub fn new(kind: ArtifactKind, bytes: Vec<u8>) -> Artifact {
        let id = address(kind, &bytes);
        Artifact { id, kind, bytes }
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Recompute the address and compare it with the stored one.
    pub fn verify(&self) -> bool {
        self.id == address(self.kind, &self.bytes)
    }
}

/// The content address of `kind` + `bytes`.
pub fn address(kind: ArtifactKind, bytes: &[u8]) -> String {
    let mut input = Vec::with_capacity(kind.as_str().len() + 1 + bytes.len());
    input.extend_from_slice(kind.as_str().as_bytes());
    input.push(0);
    input.extend_from_slice(bytes);
    hex_digest(&input)
}

/// A short descriptor of a stored artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactInfo {
    pub id: String,
    pub kind: ArtifactKind,
    pub byte_len: usize,
}

impl fmt::Display for ArtifactInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({}, {} bytes)",
            self.id,
            self.kind.as_str(),
            self.byte_len
        )
    }
}

/// An in-memory content-addressed store.
#[derive(Debug, Default, Clone)]
pub struct ArtifactStore {
    artifacts: BTreeMap<String, Artifact>,
}

impl ArtifactStore {
    pub fn new() -> ArtifactStore {
        ArtifactStore::default()
    }

    /// Store an artifact and return its id. Storing identical content again is a
    /// no-op that returns the same id; existing content is never overwritten.
    pub fn publish(&mut self, kind: ArtifactKind, bytes: Vec<u8>) -> String {
        let artifact = Artifact::new(kind, bytes);
        let id = artifact.id.clone();
        self.artifacts.entry(id.clone()).or_insert(artifact);
        id
    }

    pub fn fetch(&self, id: &str) -> Option<&Artifact> {
        self.artifacts.get(id)
    }

    /// Re-hash a stored artifact. `false` for an unknown or tampered artifact.
    pub fn verify(&self, id: &str) -> bool {
        self.artifacts.get(id).is_some_and(Artifact::verify)
    }

    pub fn inspect(&self, id: &str) -> Option<ArtifactInfo> {
        self.artifacts.get(id).map(|artifact| ArtifactInfo {
            id: artifact.id.clone(),
            kind: artifact.kind,
            byte_len: artifact.bytes.len(),
        })
    }

    pub fn contains(&self, id: &str) -> bool {
        self.artifacts.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.artifacts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty()
    }
}
