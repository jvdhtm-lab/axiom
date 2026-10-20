//! Typed frontend state: validation, migration, hot-swap, and rollback.
//!
//! State is stored per tenant/principal partition. Updates are validated and
//! staged before they replace the active value, so a failed migration or an
//! invariant violation leaves the previous healthy state untouched. Client
//! state is never a substitute for server-side authorization.

use std::collections::BTreeMap;

use axiom_hash::hex_digest;

use crate::migrations::Migrator;
use crate::value::StateValue;
use crate::StateError;

type Invariant = Box<dyn Fn(&StateValue) -> Result<(), String>>;

/// Where a piece of state logically lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateScope {
    Component,
    Route,
    Application,
}

impl StateScope {
    pub fn parse(value: &str) -> Option<StateScope> {
        match value {
            "component" => Some(StateScope::Component),
            "route" => Some(StateScope::Route),
            "application" => Some(StateScope::Application),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            StateScope::Component => "component",
            StateScope::Route => "route",
            StateScope::Application => "application",
        }
    }
}

/// How state survives across visits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatePersistence {
    None,
    Session,
    Persistent,
}

impl StatePersistence {
    pub fn parse(value: &str) -> Option<StatePersistence> {
        match value {
            "none" => Some(StatePersistence::None),
            "session" => Some(StatePersistence::Session),
            "persistent" => Some(StatePersistence::Persistent),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            StatePersistence::None => "none",
            StatePersistence::Session => "session",
            StatePersistence::Persistent => "persistent",
        }
    }
}

/// The tenant/principal partition state belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Partition {
    pub tenant: String,
    pub principal: String,
}

impl Partition {
    pub fn new(tenant: impl Into<String>, principal: impl Into<String>) -> Self {
        Partition {
            tenant: tenant.into(),
            principal: principal.into(),
        }
    }

    pub fn label(&self) -> String {
        format!("{}/{}", self.tenant, self.principal)
    }
}

/// The declared metadata of one logical state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateSpec {
    pub id: String,
    pub record_type: String,
    pub schema_version: u32,
    pub scope: StateScope,
    pub persistence: StatePersistence,
    pub max_bytes: u64,
}

impl StateSpec {
    pub fn new(id: impl Into<String>, record_type: impl Into<String>, schema_version: u32) -> Self {
        StateSpec {
            id: id.into(),
            record_type: record_type.into(),
            schema_version,
            scope: StateScope::Route,
            persistence: StatePersistence::None,
            max_bytes: 1_048_576,
        }
    }

    pub fn with_scope(mut self, scope: StateScope) -> Self {
        self.scope = scope;
        self
    }

    pub fn with_persistence(mut self, persistence: StatePersistence) -> Self {
        self.persistence = persistence;
        self
    }

    pub fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
    }
}

/// A committed state value.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredState {
    pub schema_version: u32,
    pub revision: u64,
    pub byte_len: usize,
    pub value: StateValue,
}

/// A restorable snapshot of one partition's state.
#[derive(Debug, Clone, PartialEq)]
pub struct StateSnapshot {
    pub id: String,
    pub partition: Partition,
    pub schema_version: u32,
    pub revision: u64,
    pub value: StateValue,
}

/// The result of an activation (hot-swap or migration).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    pub from: u32,
    pub to: u32,
    pub steps: Vec<String>,
    pub revision: u64,
}

/// In-memory state storage with validation and versioned activation.
pub struct StateStore {
    spec: StateSpec,
    migrator: Migrator,
    invariants: Vec<(String, Invariant)>,
    entries: BTreeMap<Partition, StoredState>,
    quarantine: BTreeMap<Partition, StoredState>,
    revision: u64,
}

impl StateStore {
    pub fn new(spec: StateSpec) -> Self {
        StateStore {
            spec,
            migrator: Migrator::new(),
            invariants: Vec::new(),
            entries: BTreeMap::new(),
            quarantine: BTreeMap::new(),
            revision: 0,
        }
    }

    pub fn spec(&self) -> &StateSpec {
        &self.spec
    }

    pub fn migrator_mut(&mut self) -> &mut Migrator {
        &mut self.migrator
    }

    /// Register an additional invariant checked before every commit.
    pub fn add_invariant(
        &mut self,
        name: impl Into<String>,
        check: impl Fn(&StateValue) -> Result<(), String> + 'static,
    ) {
        self.invariants.push((name.into(), Box::new(check)));
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn partitions(&self) -> Vec<&Partition> {
        self.entries.keys().collect()
    }

    /// Validate a value without storing it.
    pub fn validate(&self, value: &StateValue) -> Result<(), StateError> {
        if !matches!(value, StateValue::Record(_)) {
            return Err(StateError::NotRecord);
        }
        if let Some(field) = value.forbidden_key() {
            return Err(StateError::ForbiddenField(field));
        }
        let byte_len = value.byte_len() as u64;
        if byte_len > self.spec.max_bytes {
            return Err(StateError::TooLarge {
                bytes: byte_len,
                max: self.spec.max_bytes,
            });
        }
        for (name, check) in &self.invariants {
            (check)(value).map_err(|reason| StateError::Invariant {
                name: name.clone(),
                reason,
            })?;
        }
        Ok(())
    }

    /// Validate and commit state for a partition. Returns the new revision.
    pub fn save(&mut self, partition: &Partition, value: StateValue) -> Result<u64, StateError> {
        if self.spec.persistence == StatePersistence::None {
            return Err(StateError::PersistenceDisabled(self.spec.id.clone()));
        }
        self.commit(partition, self.spec.schema_version, value)
    }

    fn commit(
        &mut self,
        partition: &Partition,
        schema_version: u32,
        value: StateValue,
    ) -> Result<u64, StateError> {
        self.validate(&value)?;
        self.revision += 1;
        let stored = StoredState {
            schema_version,
            revision: self.revision,
            byte_len: value.byte_len(),
            value,
        };
        self.entries.insert(partition.clone(), stored);
        Ok(self.revision)
    }

    pub fn load(&self, partition: &Partition) -> Option<&StoredState> {
        self.entries.get(partition)
    }

    /// Move stored state to quarantine (e.g. on logout or a tenant change).
    /// Quarantined state is not readable as active state.
    pub fn quarantine(&mut self, partition: &Partition) -> bool {
        match self.entries.remove(partition) {
            Some(stored) => {
                self.quarantine.insert(partition.clone(), stored);
                true
            }
            None => false,
        }
    }

    pub fn quarantined(&self, partition: &Partition) -> Option<&StoredState> {
        self.quarantine.get(partition)
    }

    /// Drop active and quarantined state for a partition.
    pub fn clear_partition(&mut self, partition: &Partition) -> bool {
        let active = self.entries.remove(partition).is_some();
        let held = self.quarantine.remove(partition).is_some();
        active || held
    }

    /// Snapshot the active value for rollback.
    pub fn snapshot(&self, partition: &Partition) -> Option<StateSnapshot> {
        let stored = self.entries.get(partition)?;
        let id = snapshot_id(partition, stored);
        Some(StateSnapshot {
            id,
            partition: partition.clone(),
            schema_version: stored.schema_version,
            revision: stored.revision,
            value: stored.value.clone(),
        })
    }

    /// Restore a snapshot after revalidating it and its hash.
    pub fn restore(&mut self, snapshot: StateSnapshot) -> Result<(), StateError> {
        let stored = StoredState {
            schema_version: snapshot.schema_version,
            revision: snapshot.revision,
            byte_len: snapshot.value.byte_len(),
            value: snapshot.value,
        };
        if snapshot_id(&snapshot.partition, &stored) != snapshot.id {
            return Err(StateError::SnapshotMismatch);
        }
        self.validate(&stored.value)?;
        self.entries.insert(snapshot.partition, stored);
        Ok(())
    }

    /// Activate the store's declared schema version for a partition.
    ///
    /// Same-version activation is a pure hot swap: the value is revalidated and
    /// kept. Upgrades run the registered migration chain and commit only after
    /// the migrated value validates. Failures leave the stored value untouched.
    pub fn activate(
        &mut self,
        partition: &Partition,
        target_version: u32,
    ) -> Result<MigrationReport, StateError> {
        let stored = self
            .entries
            .get(partition)
            .cloned()
            .ok_or(StateError::NoState)?;
        if target_version < stored.schema_version {
            return Err(StateError::DowngradeRefused {
                from: stored.schema_version,
                to: target_version,
            });
        }
        if target_version == stored.schema_version {
            self.validate(&stored.value)?;
            return Ok(MigrationReport {
                from: stored.schema_version,
                to: target_version,
                steps: Vec::new(),
                revision: stored.revision,
            });
        }

        let steps = self
            .migrator
            .names_between(stored.schema_version, target_version)?;
        let migrated =
            self.migrator
                .migrate(stored.value.clone(), stored.schema_version, target_version)?;
        let revision = self.commit(partition, target_version, migrated)?;
        Ok(MigrationReport {
            from: stored.schema_version,
            to: target_version,
            steps,
            revision,
        })
    }
}

fn snapshot_id(partition: &Partition, stored: &StoredState) -> String {
    let mut input = String::new();
    input.push_str(&partition.tenant);
    input.push('\u{0}');
    input.push_str(&partition.principal);
    input.push('\u{0}');
    input.push_str(&stored.schema_version.to_string());
    input.push('\u{0}');
    input.push_str(&stored.revision.to_string());
    input.push('\u{0}');
    input.push_str(&stored.value.encode());
    hex_digest(input.as_bytes())
}
