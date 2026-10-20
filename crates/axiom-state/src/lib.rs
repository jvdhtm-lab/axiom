//! Axiom typed frontend state.
//!
//! Frontend state is a normal record value with declared metadata: an id, a
//! record type, a schema version, a scope, and a persistence mode. The runtime
//! validates state before committing it, runs registered pure migrations when
//! the schema version advances, and never persists credentials, capability
//! grants, or authorization decisions.
//!
//! ```
//! use axiom_state::{Partition, StatePersistence, StateSpec, StateStore, StateValue};
//!
//! let spec = StateSpec::new("editor", "EditorState", 1)
//!     .with_persistence(StatePersistence::Session);
//! let mut store = StateStore::new(spec);
//! let partition = Partition::new("tenant", "alice");
//! let state = StateValue::Record(vec![("count".into(), StateValue::Int(1))]);
//! store.save(&partition, state).unwrap();
//! assert_eq!(store.load(&partition).unwrap().value, StateValue::Record(vec![("count".into(), StateValue::Int(1))]));
//! ```

mod migrations;
mod store;
mod value;

pub use migrations::Migrator;
pub use store::{
    MigrationReport, Partition, StatePersistence, StateScope, StateSnapshot, StateSpec, StateStore,
    StoredState,
};
pub use value::{is_forbidden, StateValue};

/// Reasons state validation, migration, or activation can fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    /// The value is not a record.
    NotRecord,
    /// A forbidden field name was found.
    ForbiddenField(String),
    /// The encoded value exceeds the configured limit.
    TooLarge { bytes: u64, max: u64 },
    /// A registered invariant rejected the value.
    Invariant { name: String, reason: String },
    /// The state declares `persistence: none`, so it cannot be stored.
    PersistenceDisabled(String),
    /// No state exists for the partition.
    NoState,
    /// Migrations must advance exactly one version.
    InvalidMigration(String),
    /// The migration chain is missing a step.
    MissingMigration { from: u32, to: u32 },
    /// A migration function failed.
    MigrationFailed { from: u32, to: u32, reason: String },
    /// Downgrades are refused; restore an older snapshot instead.
    DowngradeRefused { from: u32, to: u32 },
    /// A snapshot no longer matches its content hash.
    SnapshotMismatch,
}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateError::NotRecord => write!(f, "frontend state must be a record"),
            StateError::ForbiddenField(field) => {
                write!(f, "state field `{field}` must never be persisted")
            }
            StateError::TooLarge { bytes, max } => {
                write!(f, "state is {bytes} bytes; the limit is {max}")
            }
            StateError::Invariant { name, reason } => {
                write!(f, "state invariant `{name}` failed: {reason}")
            }
            StateError::PersistenceDisabled(id) => {
                write!(f, "state `{id}` has persistence `none`")
            }
            StateError::NoState => write!(f, "no state exists for this partition"),
            StateError::InvalidMigration(reason) => write!(f, "invalid state migration: {reason}"),
            StateError::MissingMigration { from, to } => {
                write!(f, "no state migration from version {from} to {to}")
            }
            StateError::MigrationFailed { from, to, reason } => {
                write!(f, "state migration {from} -> {to} failed: {reason}")
            }
            StateError::DowngradeRefused { from, to } => {
                write!(f, "refusing to downgrade state from version {from} to {to}")
            }
            StateError::SnapshotMismatch => write!(f, "state snapshot does not match its hash"),
        }
    }
}

impl std::error::Error for StateError {}
