//! Diagnostics and source rendering.
//!
//! A [`Diagnostic`] is a message with a severity and a byte [`Span`]. A [`Source`]
//! turns a span into a line/column pointer and an excerpt, so errors are precise
//! and readable. Rendering never panics, even for spans past the end of input.

use std::fmt::Write as _;

use crate::span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    pub notes: Vec<String>,
}

impl Diagnostic {
    pub fn error(span: Span, message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Error,
            message: message.into(),
            span,
            notes: Vec::new(),
        }
    }

    pub fn warning(span: Span, message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            message: message.into(),
            span,
            notes: Vec::new(),
        }
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }

    /// Render with a `file:line:col` header and a source excerpt.
    pub fn render(&self, source: &Source) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "{}: {}", self.severity.as_str(), self.message);
        let (line, col) = source.line_col(self.span.start);
        let _ = writeln!(out, "  --> {}:{}:{}", source.name, line, col);

        if let Some(excerpt) = source.excerpt(self.span) {
            let gutter = line.to_string();
            let pad = " ".repeat(gutter.len());
            let _ = writeln!(out, "{pad} |");
            let _ = writeln!(out, "{gutter} | {excerpt}");
            let caret_pad = " ".repeat(col.saturating_sub(1));
            let carets = "^".repeat(self.span.len().max(1) as usize);
            let _ = writeln!(out, "{pad} | {caret_pad}{carets}");
        }

        for note in &self.notes {
            let _ = writeln!(out, "  = note: {note}");
        }
        out
    }
}

/// A named piece of source text with cached line starts.
#[derive(Debug, Clone)]
pub struct Source {
    name: String,
    text: String,
    line_starts: Vec<u32>,
}

impl Source {
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        let mut line_starts = vec![0u32];
        for (idx, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push((idx + 1) as u32);
            }
        }
        Source {
            name: name.into(),
            text,
            line_starts,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// 1-based `(line, column)` for a byte `offset`. Column counts characters.
    pub fn line_col(&self, offset: u32) -> (usize, usize) {
        let offset = offset.min(self.text.len() as u32);
        let line_idx = match self.line_starts.binary_search(&offset) {
            Ok(exact) => exact,
            Err(next) => next.saturating_sub(1),
        };
        let line_start = *self.line_starts.get(line_idx).unwrap_or(&0);
        let col = self
            .text
            .get(line_start as usize..offset as usize)
            .map(|slice| slice.chars().count() + 1)
            .unwrap_or(1);
        (line_idx + 1, col)
    }

    /// The text of the line containing `offset`, without the trailing newline.
    pub fn line_text(&self, offset: u32) -> &str {
        let offset = offset.min(self.text.len() as u32);
        let line_idx = match self.line_starts.binary_search(&offset) {
            Ok(exact) => exact,
            Err(next) => next.saturating_sub(1),
        };
        let start = *self.line_starts.get(line_idx).unwrap_or(&0) as usize;
        let end = self
            .line_starts
            .get(line_idx + 1)
            .map(|s| (*s as usize).saturating_sub(1))
            .unwrap_or(self.text.len());
        self.text.get(start..end).unwrap_or("")
    }

    fn excerpt(&self, span: Span) -> Option<String> {
        if self.text.is_empty() {
            return None;
        }
        Some(self.line_text(span.start).to_string())
    }
}
