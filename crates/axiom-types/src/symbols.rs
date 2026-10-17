//! Symbol tables for a single module.
//!
//! Phase 2 collects one namespace of top-level declarations per module. The
//! table records each declaration's kind, whether it is exported, and where it
//! was declared, and preserves duplicates so the checker can report them.

use std::collections::BTreeMap;

use axiom_syntax::ast::{Item, ItemKind, SourceFile};
use axiom_syntax::span::Span;

/// The kind of a top-level declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Type,
    Interface,
    Enum,
    Const,
    Function,
    Agent,
    Team,
    Tool,
    Contract,
    Goal,
    Policy,
    Workspace,
    Workflow,
}

impl SymbolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SymbolKind::Type => "type",
            SymbolKind::Interface => "interface",
            SymbolKind::Enum => "enum",
            SymbolKind::Const => "const",
            SymbolKind::Function => "fn",
            SymbolKind::Agent => "agent",
            SymbolKind::Team => "team",
            SymbolKind::Tool => "tool",
            SymbolKind::Contract => "contract",
            SymbolKind::Goal => "goal",
            SymbolKind::Policy => "policy",
            SymbolKind::Workspace => "workspace",
            SymbolKind::Workflow => "workflow",
        }
    }

    fn classify(kind: &ItemKind) -> SymbolKind {
        match kind {
            ItemKind::Type(_) => SymbolKind::Type,
            ItemKind::Interface(_) => SymbolKind::Interface,
            ItemKind::Enum(_) => SymbolKind::Enum,
            ItemKind::Const(_) => SymbolKind::Const,
            ItemKind::Function(_) => SymbolKind::Function,
            ItemKind::Agent(_) => SymbolKind::Agent,
            ItemKind::Tool(_) => SymbolKind::Tool,
            ItemKind::Goal(_) => SymbolKind::Goal,
            ItemKind::Workflow(_) => SymbolKind::Workflow,
            ItemKind::MemberBlock(d) => match d.kind {
                "team" => SymbolKind::Team,
                "contract" => SymbolKind::Contract,
                "policy" => SymbolKind::Policy,
                "workspace" => SymbolKind::Workspace,
                _ => SymbolKind::Type,
            },
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
    /// Collect the top-level declarations of a parsed file.
    pub fn from_file(file: &SourceFile) -> SymbolTable {
        let mut table = SymbolTable::default();
        for item in &file.items {
            table.insert(item);
        }
        table
    }

    fn insert(&mut self, item: &Item) {
        let Some(name) = item.name() else {
            return;
        };
        let symbol = Symbol {
            name: name.text.clone(),
            kind: SymbolKind::classify(&item.kind),
            exported: item.exported,
            span: name.span,
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
    /// `(first, duplicate)` pairs ordered by source position.
    pub fn duplicates(&self) -> &[(Symbol, Symbol)] {
        &self.duplicates
    }
}
