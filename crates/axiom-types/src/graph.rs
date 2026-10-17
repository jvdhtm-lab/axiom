//! The module graph: deterministic multi-file resolution.
//!
//! A [`Project`] is built from one or more root `.ax` files. Imports
//! (`import { A, B } from "./x.ax";`) are resolved relative to the importing
//! file's directory, lexically normalized so resolution is identical on every
//! platform. Loading is a depth-first walk that:
//!
//! - parses each file once and collects its [`SymbolTable`],
//! - rejects imports that do not resolve, name an unknown declaration, or name
//!   a declaration the target module does not export,
//! - reports import cycles.
//!
//! No file is loaded twice, even across diamond imports.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use axiom_syntax::ast::{ImportDecl, SourceFile};
use axiom_syntax::diagnostic::Diagnostic;
use axiom_syntax::parse;

use crate::symbols::SymbolTable;

/// Index of a module within [`ModuleGraph::modules`].
pub type ModuleId = usize;

/// A loaded, parsed source file.
#[derive(Debug)]
pub struct Module {
    pub id: ModuleId,
    /// The path used to load the file (as resolved on disk).
    pub path: PathBuf,
    /// A human-readable name for diagnostics.
    pub display: String,
    pub source: String,
    pub file: SourceFile,
    pub parse_diagnostics: Vec<Diagnostic>,
    pub symbols: SymbolTable,
    pub imports: Vec<ResolvedImport>,
    /// The declared `module a.b.c;` path, if any.
    pub module_path: Option<String>,
}

/// An import statement together with its resolved target, if any.
#[derive(Debug)]
pub struct ResolvedImport {
    pub decl: ImportDecl,
    pub target: Option<ModuleId>,
}

#[derive(Debug)]
pub struct ModuleGraph {
    pub modules: Vec<Module>,
    /// The modules that were requested explicitly.
    pub roots: Vec<ModuleId>,
}

impl ModuleGraph {
    pub fn module(&self, id: ModuleId) -> &Module {
        &self.modules[id]
    }
}

/// A graph-level diagnostic, attributed to the module it occurs in (if any).
#[derive(Debug)]
pub struct GraphDiagnostic {
    pub module: Option<ModuleId>,
    pub diagnostic: Diagnostic,
}

/// A fully loaded project: the module graph plus loading diagnostics.
#[derive(Debug)]
pub struct Project {
    pub graph: ModuleGraph,
    pub diagnostics: Vec<GraphDiagnostic>,
}

/// Load a project from root files (a transitive module graph).
pub fn load(roots: &[PathBuf]) -> Project {
    let mut loader = Loader::default();
    for root in roots {
        match loader.load_file(root) {
            Ok(id) => {
                if !loader.roots.contains(&id) {
                    loader.roots.push(id);
                }
            }
            Err(message) => loader.diagnostics.push(GraphDiagnostic {
                module: None,
                diagnostic: Diagnostic::error(axiom_syntax::Span::empty(0), message),
            }),
        }
    }
    Project {
        graph: ModuleGraph {
            modules: loader.modules,
            roots: loader.roots,
        },
        diagnostics: loader.diagnostics,
    }
}

#[derive(Default)]
struct Loader {
    modules: Vec<Module>,
    by_path: BTreeMap<PathBuf, ModuleId>,
    stack: Vec<ModuleId>,
    roots: Vec<ModuleId>,
    diagnostics: Vec<GraphDiagnostic>,
}

impl Loader {
    fn load_file(&mut self, raw_path: &Path) -> Result<ModuleId, String> {
        let norm = normalize(raw_path);
        if let Some(&id) = self.by_path.get(&norm) {
            return Ok(id);
        }
        let canonical = std::fs::canonicalize(&norm).unwrap_or_else(|_| norm.clone());
        if let Some(&id) = self.by_path.get(&canonical) {
            return Ok(id);
        }

        let source = std::fs::read_to_string(&norm)
            .map_err(|err| format!("cannot read `{}`: {err}", norm.display()))?;
        let parsed = parse(&source);
        let symbols = SymbolTable::from_file(&parsed.file);
        let module_path = parsed.file.module.as_ref().map(|m| {
            m.path
                .iter()
                .map(|i| i.text.as_str())
                .collect::<Vec<_>>()
                .join(".")
        });
        let display = norm.display().to_string();

        let id = self.modules.len();
        self.modules.push(Module {
            id,
            path: norm.clone(),
            display,
            source,
            file: parsed.file,
            parse_diagnostics: parsed.diagnostics,
            symbols,
            imports: Vec::new(),
            module_path,
        });
        self.by_path.insert(norm.clone(), id);
        if canonical != norm {
            self.by_path.insert(canonical, id);
        }

        self.stack.push(id);
        let imports = self.modules[id].file.imports.clone();
        let base = norm.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut resolved = Vec::with_capacity(imports.len());
        for decl in imports {
            let target_path = normalize(&base.join(&decl.from.value));
            match self.load_file(&target_path) {
                Ok(target) => {
                    if self.stack.contains(&target) {
                        let target_display = self.modules[target].display.clone();
                        self.diagnostics.push(GraphDiagnostic {
                            module: Some(id),
                            diagnostic: Diagnostic::error(decl.from.span, "import cycle detected")
                                .with_note(format!(
                                    "`{}` is already being loaded and imports this module back",
                                    target_display
                                )),
                        });
                    } else {
                        self.validate_import(id, &decl, target);
                    }
                    resolved.push(ResolvedImport {
                        decl,
                        target: Some(target),
                    });
                }
                Err(message) => {
                    self.diagnostics.push(GraphDiagnostic {
                        module: Some(id),
                        diagnostic: Diagnostic::error(
                            decl.from.span,
                            format!("cannot resolve import `{}`: {message}", decl.from.value),
                        ),
                    });
                    resolved.push(ResolvedImport { decl, target: None });
                }
            }
        }
        self.stack.pop();
        self.modules[id].imports = resolved;
        Ok(id)
    }

    fn validate_import(&mut self, from: ModuleId, decl: &ImportDecl, target: ModuleId) {
        for name in &decl.names {
            match self.modules[target].symbols.get(&name.text) {
                Some(symbol) if symbol.exported => {}
                Some(_) => {
                    let target_display = self.modules[target].display.clone();
                    self.diagnostics.push(GraphDiagnostic {
                        module: Some(from),
                        diagnostic: Diagnostic::error(
                            name.span,
                            format!("`{}` is not exported by `{}`", name.text, target_display),
                        )
                        .with_note("only `export`ed declarations can be imported"),
                    });
                }
                None => {
                    let target_display = self.modules[target].display.clone();
                    self.diagnostics.push(GraphDiagnostic {
                        module: Some(from),
                        diagnostic: Diagnostic::error(
                            name.span,
                            format!("`{}` is not declared in `{}`", name.text, target_display),
                        ),
                    });
                }
            }
        }
    }
}

/// Lexically normalize a path: drop `.` segments and collapse `..`.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}
