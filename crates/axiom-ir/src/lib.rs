//! Axiom shared typed intermediate representation.
//!
//! Phase 3: lower the checked AST into a stable, inspectable typed IR that
//! records types, effects, required capabilities, target restrictions, and
//! source provenance, and render it as deterministic JSON. The same IR feeds
//! both the server evaluator and (in Phase 4) the browser WASM backend.

pub mod ir;
pub mod lower;
pub mod print;

pub use ir::{
    IrAgent, IrArg, IrArm, IrBinaryOp, IrBlock, IrEnum, IrExpr, IrExprKind, IrFunction, IrImport,
    IrModule, IrPattern, IrStmt, IrStmtKind, IrType, IrTypeDecl, IrUnaryOp, IrWorkspace, Program,
    Target,
};
pub use lower::{lower, ModuleInput};
pub use print::program_to_json;

/// The IR format version, surfaced in emitted artifacts and manifests.
pub const IR_VERSION: u32 = 1;
