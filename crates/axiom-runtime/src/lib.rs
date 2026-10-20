//! Axiom runtime: the deterministic server evaluator.
//!
//! Phase 3 evaluates the pure, portable subset of the IR with no model and no
//! ambient I/O. Later phases add provider/tool hosts, durability, budgets, and
//! the browser host behind the same value model.

pub mod access;
pub mod agent_host;
pub mod agents;
pub mod budget;
pub mod database_host;
pub mod durable;
pub mod eval;
pub mod host;
pub mod validation;
pub mod value;
pub mod workspace_host;

pub use access::{AccessError, ApiGuard, ApiPolicy, Principal};
pub use agent_host::{AgentHost, IterationTrace};
pub use agents::{
    iterate, run_iteration, AgentSpec, Candidate, IterationOutcome, IterationRequest,
    IterationStatus,
};
pub use budget::{BudgetExhausted, BudgetLedger, Budgets};
pub use database_host::DatabaseHost;
pub use durable::{
    effect_fingerprint, value_fingerprint, CancelToken, CheckpointRecord, DurableConfig,
    DurableHost, EffectRecord, EffectStatus, ResumeApproval, RunRecord, RunStatus, RunStore,
    TraceEvent, TraceKind,
};
pub use eval::{run, run_with_host, Evaluator, RuntimeError};
pub use host::Host;
pub use validation::{
    EvidenceRef, ValidationKind, ValidationResult, ValidationStatus, Validator, ValidatorRegistry,
};
pub use value::Value;
pub use workspace_host::WorkspaceHost;
