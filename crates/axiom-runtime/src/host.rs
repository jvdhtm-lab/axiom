//! Optional runtime host for standard-library calls.
//!
//! The deterministic evaluator has no ambient I/O. A [`Host`] supplies typed
//! implementations of `fs.*`, `workspaces.*`, `artifacts.*`, and other effects.
//! Without a host, every effectful namespace fails closed.

use crate::value::Value;

/// A runtime host. Implementations are trusted; they must enforce authority
/// themselves (the evaluator only checks that a host is present).
pub trait Host {
    fn call(
        &self,
        namespace: &str,
        method: &str,
        args: &[(Option<String>, Value)],
    ) -> Result<Value, String>;
}
