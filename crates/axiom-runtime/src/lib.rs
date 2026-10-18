//! Axiom runtime: the deterministic server evaluator.
//!
//! Phase 3 evaluates the pure, portable subset of the IR with no model and no
//! ambient I/O. Later phases add provider/tool hosts, durability, budgets, and
//! the browser host behind the same value model.

pub mod eval;
pub mod value;

pub use eval::{run, Evaluator, RuntimeError};
pub use value::Value;
