//! The module graph: deterministic multi-file resolution.
//!
//! A [`Project`] is built from one or more root `.ax` files. Imports
//! (`import { A, B } from "./x.ax";`) resolve relative to the importing file's
//! directory, lexically normalized so resolution is identical on every platform.
//! Loading is depth-first: each file is parsed once, imports are validated
//! (unknown names, non-exported names, cycles), and no file loads twice — even
//! across diamond imports.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use axiom_syntax::ast::{File, ImportDecl};
use axiom_syntax::diagnostic::Diagnostic;
use axiom_syntax::{parse, Span};

use crate::symbols::SymbolTable;

/// Index of a module within [`ModuleGraph::modules`].
pub type ModuleId = usize;

/// A loaded, parsed source file.
#[derive(Debug)]
pub struct Module {
    pub id: ModuleId,
    pub path: PathBuf,
    pub display: String,
    pub source: String,
    pub file: File,
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
                diagnostic: Diagnostic::error(Span::empty(0), message),
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
        let module_path = parsed.file.module.as_ref().map(|m| m.dotted());
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
            let target_path = normalize(&base.join(&decl.from));
            match self.load_file(&target_path) {
                Ok(target) => {
                    if self.stack.contains(&target) {
                        let target_display = self.modules[target].display.clone();
                        self.diagnostics.push(GraphDiagnostic {
                            module: Some(id),
                            diagnostic: Diagnostic::error_code(
                                decl.span,
                                "AX0201",
                                "import cycle detected",
                            )
                            .with_note(format!(
                                "`{target_display}` is already being loaded and imports this module back"
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
                        diagnostic: Diagnostic::error_code(
                            decl.span,
                            "AX0202",
                            format!("cannot resolve import `{}`: {message}", decl.from),
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
            match self.modules[target].symbols.get(name) {
                Some(symbol) if symbol.exported => {}
                Some(_) => {
                    let target_display = self.modules[target].display.clone();
                    self.diagnostics.push(GraphDiagnostic {
                        module: Some(from),
                        diagnostic: Diagnostic::error_code(
                            decl.span,
                            "AX0203",
                            format!("`{name}` is not exported by `{target_display}`"),
                        )
                        .with_note("only `export`ed declarations can be imported"),
                    });
                }
                None => {
                    let target_display = self.modules[target].display.clone();
                    self.diagnostics.push(GraphDiagnostic {
                        module: Some(from),
                        diagnostic: Diagnostic::error_code(
                            decl.span,
                            "AX0204",
                            format!("`{name}` is not declared in `{target_display}`"),
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
