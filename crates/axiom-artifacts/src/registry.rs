//! Immutable release registry, pinning, evidence binding, and rollback.
//!
//! A release is a manifest over content-addressed artifacts. Publishing is
//! keyed by the manifest's content hash, so identical inputs always produce the
//! same release id. A release can never be mutated or replaced; revocation is
//! recorded separately and blocks future activation. Activation re-verifies
//! every artifact and the ABI before switching, and rollback re-checks the
//! previous release rather than trusting cached state.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use axiom_hash::hex_digest;

use crate::{ArtifactKind, ArtifactStore};

/// A pinned reference to one artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactRef {
    pub id: String,
    pub kind: ArtifactKind,
}

impl ArtifactRef {
    pub fn new(kind: ArtifactKind, id: impl Into<String>) -> ArtifactRef {
        ArtifactRef {
            id: id.into(),
            kind,
        }
    }

    /// Whether the reference is a full content address rather than a movable
    /// tag such as `latest`.
    pub fn is_pinned(&self) -> bool {
        is_content_hash(&self.id)
    }
}

/// Whether `id` is a full lowercase/uppercase SHA-256 hex address.
pub fn is_content_hash(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceStatus {
    Passed,
    Failed,
}

impl EvidenceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            EvidenceStatus::Passed => "passed",
            EvidenceStatus::Failed => "failed",
        }
    }
}

/// Acceptance evidence bound to one exact candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEvidence {
    pub candidate_hash: String,
    pub validator: String,
    pub status: EvidenceStatus,
}

impl ReleaseEvidence {
    pub fn passed(candidate_hash: impl Into<String>, validator: impl Into<String>) -> Self {
        ReleaseEvidence {
            candidate_hash: candidate_hash.into(),
            validator: validator.into(),
            status: EvidenceStatus::Passed,
        }
    }

    pub fn failed(candidate_hash: impl Into<String>, validator: impl Into<String>) -> Self {
        ReleaseEvidence {
            candidate_hash: candidate_hash.into(),
            validator: validator.into(),
            status: EvidenceStatus::Failed,
        }
    }
}

/// A complete, immutable release description.
///
/// `candidate_hash` covers the manifest without evidence; `release_id` covers
/// the manifest including evidence, so attaching different evidence yields a
/// different release id while still naming the same candidate.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReleaseManifest {
    pub shell: Option<ArtifactRef>,
    pub modules: BTreeMap<String, ArtifactRef>,
    pub abi_version: u32,
    pub compiler_version: String,
    pub contract_versions: BTreeMap<String, String>,
    pub state_schema_version: u32,
    pub evidence: Vec<ReleaseEvidence>,
}

impl ReleaseManifest {
    pub fn new(
        shell: ArtifactRef,
        abi_version: u32,
        compiler_version: impl Into<String>,
        state_schema_version: u32,
    ) -> Self {
        ReleaseManifest {
            shell: Some(shell),
            modules: BTreeMap::new(),
            abi_version,
            compiler_version: compiler_version.into(),
            contract_versions: BTreeMap::new(),
            state_schema_version,
            evidence: Vec::new(),
        }
    }

    pub fn module(mut self, name: impl Into<String>, artifact: ArtifactRef) -> Self {
        self.modules.insert(name.into(), artifact);
        self
    }

    pub fn contract(mut self, name: impl Into<String>, version: impl Into<String>) -> Self {
        self.contract_versions.insert(name.into(), version.into());
        self
    }

    pub fn evidence(mut self, evidence: ReleaseEvidence) -> Self {
        self.evidence.push(evidence);
        self
    }

    /// The manifest hash without evidence, used to bind acceptance evidence to
    /// the exact candidate.
    pub fn candidate_hash(&self) -> String {
        hex_digest(self.encode(false).as_bytes())
    }

    /// The immutable release id, covering the manifest and its evidence.
    pub fn release_id(&self) -> String {
        hex_digest(self.encode(true).as_bytes())
    }

    fn encode(&self, with_evidence: bool) -> String {
        let mut out = String::new();
        match &self.shell {
            Some(shell) => {
                let _ = write!(out, "shell:{}:{};", shell.kind.as_str(), shell.id);
            }
            None => out.push_str("shell:none;"),
        }
        for (name, artifact) in &self.modules {
            let _ = write!(
                out,
                "module:{}:{}:{};",
                name,
                artifact.kind.as_str(),
                artifact.id
            );
        }
        let _ = write!(
            out,
            "abi:{};compiler:{};state:{};",
            self.abi_version, self.compiler_version, self.state_schema_version
        );
        for (name, version) in &self.contract_versions {
            let _ = write!(out, "contract:{name}:{version};");
        }
        if with_evidence {
            for evidence in &self.evidence {
                let _ = write!(
                    out,
                    "evidence:{}:{}:{};",
                    evidence.validator,
                    evidence.status.as_str(),
                    evidence.candidate_hash
                );
            }
        }
        out
    }
}

/// What a registry accepts as an activatable release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationPolicy {
    pub abi_version: u32,
    pub require_evidence: bool,
    pub require_pinned_references: bool,
}

impl Default for ActivationPolicy {
    fn default() -> Self {
        ActivationPolicy {
            abi_version: 1,
            require_evidence: true,
            require_pinned_references: true,
        }
    }
}

impl ActivationPolicy {
    pub fn new(abi_version: u32) -> Self {
        ActivationPolicy {
            abi_version,
            ..ActivationPolicy::default()
        }
    }
}

#[derive(Debug, Clone)]
struct ReleaseEntry {
    manifest: ReleaseManifest,
    sequence: u64,
    revoked: Option<String>,
}

/// An in-memory release registry with active/previous tracking.
#[derive(Debug, Default)]
pub struct ReleaseRegistry {
    entries: BTreeMap<String, ReleaseEntry>,
    sequence: u64,
    active: Option<String>,
    previous: Option<String>,
}

impl ReleaseRegistry {
    pub fn new() -> Self {
        ReleaseRegistry::default()
    }

    /// Verify and record a release. Republishing an identical release is
    /// idempotent; reintroducing a revoked release is refused.
    pub fn publish(
        &mut self,
        store: &ArtifactStore,
        manifest: ReleaseManifest,
        policy: &ActivationPolicy,
    ) -> Result<String, RegistryError> {
        let id = manifest.release_id();
        if let Some(entry) = self.entries.get(&id) {
            if let Some(reason) = &entry.revoked {
                return Err(RegistryError::Revoked {
                    id,
                    reason: reason.clone(),
                });
            }
            return Ok(id);
        }

        check_manifest(store, &manifest, policy)?;
        let candidate = manifest.candidate_hash();
        if policy.require_evidence && manifest.evidence.is_empty() {
            return Err(RegistryError::EvidenceMissing);
        }
        for evidence in &manifest.evidence {
            if evidence.status != EvidenceStatus::Passed {
                return Err(RegistryError::EvidenceFailed(evidence.validator.clone()));
            }
            if evidence.candidate_hash != candidate {
                return Err(RegistryError::EvidenceNotBound {
                    validator: evidence.validator.clone(),
                    expected: candidate.clone(),
                    found: evidence.candidate_hash.clone(),
                });
            }
        }

        self.sequence += 1;
        self.entries.insert(
            id.clone(),
            ReleaseEntry {
                manifest,
                sequence: self.sequence,
                revoked: None,
            },
        );
        Ok(id)
    }

    /// Activate a release after re-verifying artifacts, ABI, and revocation.
    pub fn activate(
        &mut self,
        store: &ArtifactStore,
        policy: &ActivationPolicy,
        id: &str,
    ) -> Result<(), RegistryError> {
        self.check_activatable(store, policy, id)?;
        self.previous = self.active.replace(id.to_string());
        Ok(())
    }

    /// Re-activate the previously active release.
    pub fn rollback(
        &mut self,
        store: &ArtifactStore,
        policy: &ActivationPolicy,
    ) -> Result<String, RegistryError> {
        let target = self.previous.clone().ok_or(RegistryError::NoPrevious)?;
        self.activate(store, policy, &target)?;
        Ok(target)
    }

    fn check_activatable(
        &self,
        store: &ArtifactStore,
        policy: &ActivationPolicy,
        id: &str,
    ) -> Result<&ReleaseEntry, RegistryError> {
        let entry = self
            .entries
            .get(id)
            .ok_or_else(|| RegistryError::UnknownRelease(id.to_string()))?;
        if let Some(reason) = &entry.revoked {
            return Err(RegistryError::Revoked {
                id: id.to_string(),
                reason: reason.clone(),
            });
        }
        check_manifest(store, &entry.manifest, policy)?;
        if policy.require_evidence && entry.manifest.evidence.is_empty() {
            return Err(RegistryError::EvidenceMissing);
        }
        for evidence in &entry.manifest.evidence {
            if evidence.status != EvidenceStatus::Passed {
                return Err(RegistryError::EvidenceFailed(evidence.validator.clone()));
            }
            if evidence.candidate_hash != entry.manifest.candidate_hash() {
                return Err(RegistryError::EvidenceNotBound {
                    validator: evidence.validator.clone(),
                    expected: entry.manifest.candidate_hash(),
                    found: evidence.candidate_hash.clone(),
                });
            }
        }
        Ok(entry)
    }

    /// Mark a release revoked. Revocation never deletes the manifest and never
    /// mutates artifacts; it blocks all future activations and rollbacks.
    pub fn revoke(&mut self, id: &str, reason: impl Into<String>) -> Result<(), RegistryError> {
        let entry = self
            .entries
            .get_mut(id)
            .ok_or_else(|| RegistryError::UnknownRelease(id.to_string()))?;
        entry.revoked.get_or_insert_with(|| reason.into());
        Ok(())
    }

    pub fn is_revoked(&self, id: &str) -> bool {
        self.entries
            .get(id)
            .is_some_and(|entry| entry.revoked.is_some())
    }

    /// Retain the newest `keep` inactive releases. Active and previous releases
    /// are always retained. Returns the removed release ids.
    pub fn retain(&mut self, keep: usize) -> Vec<String> {
        let mut candidates = self
            .entries
            .iter()
            .filter(|(id, _)| {
                Some(id.as_str()) != self.active.as_deref()
                    && Some(id.as_str()) != self.previous.as_deref()
            })
            .map(|(id, entry)| (entry.sequence, id.clone()))
            .collect::<Vec<_>>();
        candidates.sort_by_key(|a| std::cmp::Reverse(a.0));
        let removed = candidates
            .into_iter()
            .skip(keep)
            .map(|(_, id)| id)
            .collect::<Vec<_>>();
        for id in &removed {
            self.entries.remove(id);
        }
        removed
    }

    pub fn active_id(&self) -> Option<&str> {
        self.active.as_deref()
    }

    pub fn active(&self) -> Option<&ReleaseManifest> {
        self.active
            .as_deref()
            .and_then(|id| self.entries.get(id))
            .map(|entry| &entry.manifest)
    }

    pub fn previous_id(&self) -> Option<&str> {
        self.previous.as_deref()
    }

    pub fn previous(&self) -> Option<&ReleaseManifest> {
        self.previous
            .as_deref()
            .and_then(|id| self.entries.get(id))
            .map(|entry| &entry.manifest)
    }

    pub fn release(&self, id: &str) -> Option<&ReleaseManifest> {
        self.entries.get(id).map(|entry| &entry.manifest)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn check_manifest(
    store: &ArtifactStore,
    manifest: &ReleaseManifest,
    policy: &ActivationPolicy,
) -> Result<(), RegistryError> {
    if manifest.abi_version != policy.abi_version {
        return Err(RegistryError::AbiMismatch {
            release: manifest.abi_version,
            expected: policy.abi_version,
        });
    }
    let Some(shell) = &manifest.shell else {
        return Err(RegistryError::MissingArtifact("shell".to_string()));
    };
    check_reference(store, "shell", shell, policy)?;
    for (name, artifact) in &manifest.modules {
        check_reference(store, name, artifact, policy)?;
    }
    Ok(())
}

fn check_reference(
    store: &ArtifactStore,
    name: &str,
    artifact: &ArtifactRef,
    policy: &ActivationPolicy,
) -> Result<(), RegistryError> {
    if policy.require_pinned_references && !artifact.is_pinned() {
        return Err(RegistryError::UnpinnedReference {
            name: name.to_string(),
            id: artifact.id.clone(),
        });
    }
    let Some(stored) = store.fetch(&artifact.id) else {
        return Err(RegistryError::MissingArtifact(artifact.id.clone()));
    };
    if stored.kind != artifact.kind {
        return Err(RegistryError::KindMismatch {
            id: artifact.id.clone(),
            expected: artifact.kind.as_str(),
            found: stored.kind.as_str(),
        });
    }
    if !stored.verify() {
        return Err(RegistryError::Integrity(artifact.id.clone()));
    }
    Ok(())
}

/// Reasons a release cannot be published, activated, or rolled back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    UnknownRelease(String),
    Revoked {
        id: String,
        reason: String,
    },
    MissingArtifact(String),
    UnpinnedReference {
        name: String,
        id: String,
    },
    KindMismatch {
        id: String,
        expected: &'static str,
        found: &'static str,
    },
    Integrity(String),
    EvidenceMissing,
    EvidenceFailed(String),
    EvidenceNotBound {
        validator: String,
        expected: String,
        found: String,
    },
    AbiMismatch {
        release: u32,
        expected: u32,
    },
    NoPrevious,
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::UnknownRelease(id) => write!(f, "unknown release `{id}`"),
            RegistryError::Revoked { id, reason } => {
                write!(f, "release `{id}` is revoked: {reason}")
            }
            RegistryError::MissingArtifact(id) => write!(f, "missing artifact `{id}`"),
            RegistryError::UnpinnedReference { name, id } => {
                write!(f, "`{name}` is not pinned to a content address: `{id}`")
            }
            RegistryError::KindMismatch {
                id,
                expected,
                found,
            } => write!(
                f,
                "artifact `{id}` is a {found}, but the manifest expects a {expected}"
            ),
            RegistryError::Integrity(id) => {
                write!(f, "artifact `{id}` failed its integrity check")
            }
            RegistryError::EvidenceMissing => {
                write!(f, "release has no acceptance evidence")
            }
            RegistryError::EvidenceFailed(validator) => {
                write!(f, "acceptance evidence from `{validator}` did not pass")
            }
            RegistryError::EvidenceNotBound {
                validator,
                expected,
                found,
            } => write!(
                f,
                "evidence from `{validator}` names candidate `{found}`, not `{expected}`"
            ),
            RegistryError::AbiMismatch { release, expected } => write!(
                f,
                "release targets ABI {release}, but this host expects ABI {expected}"
            ),
            RegistryError::NoPrevious => write!(f, "no previous release to roll back to"),
        }
    }
}

impl std::error::Error for RegistryError {}
