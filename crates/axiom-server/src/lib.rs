//! Axiom HTTP server core.
//!
//! The server is a trusted boundary: every route names an exported API, the
//! injected authenticator supplies the principal, and [`axiom_runtime::ApiGuard`]
//! authorizes the call before the handler runs. Missing policies and failed
//! authentication fail closed.
//!
//! This crate is transport and authorization only. It does not compile Axiom
//! handlers, serve TLS, or generate browser stubs; see `docs/remaining-phases.md`.

pub mod http;
pub mod json;

pub use http::{
    Authenticator, Handler, Request, Response, Route, Server, ServerError, MAX_REQUEST_BYTES,
};
pub use json::{from_json, to_json, JsonError};

/// The crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
