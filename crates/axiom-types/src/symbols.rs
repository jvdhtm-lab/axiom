//! Symbol tables for a single module.
//!
//! One namespace of top-level declarations per module. The table records each
//! declaration's kind, whether it is exported, and where it was declared, and
//! preserves duplicate pairs so the checker can report them.

use std::collections::BTreeMap;

use axiom_syntax::ast::{DeclKind, Declaration, File};
use axiom_syntax::span::Span;

/// The kind of a top-level declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Type,
    Enum,
    Function,
    Agent,
    Workspace,
}

impl SymbolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SymbolKind::Type => "type",
            SymbolKind::Enum => "enum",
            SymbolKind::Function => "fn",
            SymbolKind::Agent => "agent",
            SymbolKind::Workspace => "workspace",
        }
    }

    fn classify(kind: &DeclKind) -> SymbolKind {
        match kind {
            DeclKind::Type(_) => SymbolKind::Type,
            DeclKind::Enum(_) => SymbolKind::Enum,
            DeclKind::Function(_) => SymbolKind::Function,
            DeclKind::Agent(_) => SymbolKind::Agent,
            DeclKind::Workspace(_) => SymbolKind::Workspace,
        }
    }
}

/// A declaration visible by name within a module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub exported: bool,
    pub span: Span,
}

/// Top-level declarations of one module, plus any name collisions.
#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    by_name: BTreeMap<String, Symbol>,
    duplicates: Vec<(Symbol, Symbol)>,
}

impl SymbolTable {
    pub fn from_file(file: &File) -> SymbolTable {
        let mut table = SymbolTable::default();
        for decl in &file.declarations {
            table.insert(decl);
        }
        table
    }

    fn insert(&mut self, decl: &Declaration) {
        let Some((name, span)) = declaration_name(decl) else {
            return;
        };
        let symbol = Symbol {
            name,
            kind: SymbolKind::classify(&decl.kind),
            exported: decl.exported,
            span,
        };
        match self.by_name.get(&symbol.name).cloned() {
            Some(existing) => {
                if existing.span.start <= symbol.span.start {
                    self.duplicates.push((existing, symbol));
                } else {
                    self.duplicates.push((symbol, existing));
                }
            }
            None => {
                self.by_name.insert(symbol.name.clone(), symbol);
            }
        }
    }

    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.by_name.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Symbol> {
        self.by_name.values()
    }

    /// Declarations that share a name with an earlier declaration, as
    /// `(first, duplicate)` pairs.
    pub fn duplicates(&self) -> &[(Symbol, Symbol)] {
        &self.duplicates
    }
}

/// The declared name and its span, for any declaration kind.
pub fn declaration_name(decl: &Declaration) -> Option<(String, Span)> {
    match &decl.kind {
        DeclKind::Type(d) => Some((d.name.clone(), d.span)),
        DeclKind::Enum(d) => Some((d.name.clone(), d.span)),
        DeclKind::Function(d) => Some((d.name.clone(), d.span)),
        DeclKind::Agent(d) => Some((d.name.clone(), d.span)),
        DeclKind::Workspace(d) => Some((d.name.clone(), d.span)),
    }
}
