//! Pure state migrations, configured by `from`/`to` schema versions.
//!
//! A migration is an ordinary pure function over a supplied state value. It has
//! no access to models, the network, the filesystem, the database, or
//! permissions; the runtime applies and validates it before committing.

use std::collections::BTreeMap;

use crate::value::StateValue;
use crate::StateError;

/// One versioned transformation.
struct Migration {
    from: u32,
    to: u32,
    name: String,
    apply: Box<dyn Fn(StateValue) -> Result<StateValue, String>>,
}

/// A contiguous chain of state migrations.
#[derive(Default)]
pub struct Migrator {
    steps: BTreeMap<u32, Migration>,
}

impl Migrator {
    pub fn new() -> Self {
        Migrator::default()
    }

    /// Register `apply` as the migration from `from` to `from + 1`.
    pub fn add(
        &mut self,
        from: u32,
        to: u32,
        name: impl Into<String>,
        apply: impl Fn(StateValue) -> Result<StateValue, String> + 'static,
    ) -> Result<(), StateError> {
        let name = name.into();
        if to != from + 1 {
            return Err(StateError::InvalidMigration(format!(
                "migration `{name}` must advance exactly one version ({from} -> {to})"
            )));
        }
        if self.steps.contains_key(&from) {
            return Err(StateError::InvalidMigration(format!(
                "two migrations start at version {from}"
            )));
        }
        self.steps.insert(
            from,
            Migration {
                from,
                to,
                name,
                apply: Box::new(apply),
            },
        );
        Ok(())
    }

    /// Migration names in version order.
    pub fn names(&self) -> Vec<&str> {
        self.steps.values().map(|step| step.name.as_str()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// The names of the migrations that would run from `from` to `to`.
    pub fn names_between(&self, from: u32, to: u32) -> Result<Vec<String>, StateError> {
        let mut names = Vec::new();
        let mut version = from;
        while version < to {
            let Some(step) = self.steps.get(&version) else {
                return Err(StateError::MissingMigration {
                    from: version,
                    to: version + 1,
                });
            };
            names.push(step.name.clone());
            version = step.to;
        }
        Ok(names)
    }

    /// Apply the chain from `from` to `to`, failing closed on a missing step or
    /// an erroring migration.
    pub fn migrate(
        &self,
        mut value: StateValue,
        from: u32,
        to: u32,
    ) -> Result<StateValue, StateError> {
        if to < from {
            return Err(StateError::DowngradeRefused { from, to });
        }
        let mut version = from;
        while version < to {
            let Some(step) = self.steps.get(&version) else {
                return Err(StateError::MissingMigration {
                    from: version,
                    to: version + 1,
                });
            };
            value = (step.apply)(value).map_err(|reason| StateError::MigrationFailed {
                from: step.from,
                to: step.to,
                reason,
            })?;
            version = step.to;
        }
        Ok(value)
    }
}
