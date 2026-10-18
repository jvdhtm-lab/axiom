//! Axiom language front end: lexer, parser, AST, spans, and diagnostics.
//!
//! Phase 1 scope: turn `.ax` source into a spanned AST with useful diagnostics.
//! There is no semantic analysis, IR, or execution here (Phases 2+).

pub mod ast;
pub mod diagnostic;
pub mod lexer;
pub mod parser;
pub mod span;

pub use ast::File;
pub use diagnostic::{Diagnostic, Severity, Source};
pub use lexer::{tokenize, Token, TokenKind};
pub use parser::{parse, ParseResult};
pub use span::Span;

use std::io;
use std::path::Path;

/// Read and parse a file from disk.
pub fn parse_file(path: &Path) -> io::Result<ParseResult> {
    let text = std::fs::read_to_string(path)?;
    Ok(parse(&text))
}

/// The crate version, surfaced by the CLI.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
