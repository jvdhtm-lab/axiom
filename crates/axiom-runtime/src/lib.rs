//! Axiom runtime: the deterministic server evaluator.
//!
//! Phase 3 evaluates the pure, portable subset of the IR with no model and no
//! ambient I/O. Later phases add provider/tool hosts, durability, budgets, and
//! the browser host behind the same value model.

pub mod database_host;
pub mod eval;
pub mod host;
pub mod value;
pub mod workspace_host;

pub use database_host::DatabaseHost;
pub use eval::{run, run_with_host, Evaluator, RuntimeError};
pub use host::Host;
pub use value::Value;
pub use workspace_host::WorkspaceHost;
