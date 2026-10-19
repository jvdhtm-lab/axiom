//! Library helpers shared by the `axiom` binary and its tests.
//!
//! The binary stays thin; logic that must be tested without executing a
//! freshly-built (and possibly Application-Control-blocked) binary lives here.
//! (See `docs/development.md` for the Windows Application Control note.)

pub mod database;
pub mod workspace;
