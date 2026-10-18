//! Declaration-level semantic checks (Phase 1 scalar; Phase 2 adds typing).
//!
//! These checks need no type inference: parse diagnostics, duplicate
//! declarations, known effect names, agent/workspace member shape, and the
//! static fixtures required by the small language. Expression typing, effect
//! propagation, and target checks live in the Phase 2 passes.

use std::collections::BTreeSet;

use axiom_syntax::ast::{DeclKind, Expr, ExprKind};
use axiom_syntax::diagnostic::Diagnostic;

use crate::graph::{ModuleId, Project};

/// The closed effect vocabulary (agents.md §3.5).
pub const KNOWN_EFFECTS: &[&str] = &[
    "model",
    "fs.read",
    "fs.write",
    "fs.delete",
    "workspace.create",
    "artifact.read",
    "artifact.publish",
    "command.run",
    "db.read",
    "db.write",
    "db.schema",
    "db.provision",
    "api.call",
    "browser.storage",
    "browser.navigation",
    "runtime.checkpoint",
];

/// Workspace isolation modes (agents.md §6.1).
pub const ISOLATION_MODES: &[&str] = &[
    "read_only",
    "copy_on_write",
    "git_worktree",
    "exclusive_integration",
];

/// Options that let source references be cross-validated against configuration.
#[derive(Debug, Default, Clone)]
pub struct CheckOptions {
    /// When set, `model: "alias"` strings must name a configured model alias.
    pub model_aliases: Option<BTreeSet<String>>,
    /// When set, `commands.run("name", ...)` must name a configured command.
    pub command_names: Option<BTreeSet<String>>,
    /// When set, Phase 2 target-compatibility rules are applied.
    pub target: Option<crate::analyze::Target>,
}

impl CheckOptions {
    pub fn none() -> Self {
        CheckOptions::default()
    }
}

/// A diagnostic attributed to a module (or the whole project when `None`).
#[derive(Debug)]
pub struct ModuleDiagnostic {
    pub module: Option<ModuleId>,
    pub diagnostic: Diagnostic,
}

/// Run all declaration-level checks over a loaded project.
pub fn check_project(project: &Project, options: &CheckOptions) -> Vec<ModuleDiagnostic> {
    let mut out = Vec::new();

    for diagnostic in &project.diagnostics {
        out.push(ModuleDiagnostic {
            module: diagnostic.module,
            diagnostic: diagnostic.diagnostic.clone(),
        });
    }

    for module in &project.graph.modules {
        for diagnostic in &module.parse_diagnostics {
            out.push(ModuleDiagnostic {
                module: Some(module.id),
                diagnostic: diagnostic.clone(),
            });
        }

        for (first, duplicate) in module.symbols.duplicates() {
            out.push(ModuleDiagnostic {
                module: Some(module.id),
                diagnostic: Diagnostic::error_code(
                    duplicate.span,
                    "AX0210",
                    format!("duplicate declaration of `{}`", duplicate.name),
                )
                .with_note(format!(
                    "first declared as {} at byte {}",
                    first.kind.as_str(),
                    first.span.start
                )),
            });
        }

        for decl in &module.file.declarations {
            match &decl.kind {
                DeclKind::Function(f) => {
                    check_effects(module.id, f.effects.as_deref(), &mut out);
                }
                DeclKind::Agent(a) => {
                    if a.member("model").is_none() {
                        out.push(ModuleDiagnostic {
                            module: Some(module.id),
                            diagnostic: Diagnostic::error_code(
                                a.span,
                                "AX0211",
                                format!("agent `{}` must declare a `model` alias", a.name),
                            ),
                        });
                    }
                    if let Some(model) = a.member("model") {
                        check_model_alias(module.id, model, options, &mut out);
                    }
                    if let Some(tools) = a.member("tools") {
                        if !matches!(tools.kind, ExprKind::List(_)) {
                            out.push(ModuleDiagnostic {
                                module: Some(module.id),
                                diagnostic: Diagnostic::error_code(
                                    tools.span,
                                    "AX0212",
                                    "`tools` must be a list of tool references",
                                ),
                            });
                        }
                    }
                    if let Some(limits) = a.member("limits") {
                        if !matches!(limits.kind, ExprKind::Record(_)) {
                            out.push(ModuleDiagnostic {
                                module: Some(module.id),
                                diagnostic: Diagnostic::error_code(
                                    limits.span,
                                    "AX0213",
                                    "`limits` must be a record",
                                ),
                            });
                        }
                    }
                }
                DeclKind::Workspace(w) => {
                    if let Some(isolation) = w.member("isolation") {
                        if let ExprKind::Ident(name) = &isolation.kind {
                            if !ISOLATION_MODES.contains(&name.as_str()) {
                                out.push(ModuleDiagnostic {
                                    module: Some(module.id),
                                    diagnostic: Diagnostic::error_code(
                                        isolation.span,
                                        "AX0214",
                                        format!("unknown isolation mode `{name}`"),
                                    )
                                    .with_note(format!(
                                        "expected one of: {}",
                                        ISOLATION_MODES.join(", ")
                                    )),
                                });
                            }
                        }
                    }
                    for key in ["read", "write", "deny"] {
                        if let Some(value) = w.member(key) {
                            if !matches!(value.kind, ExprKind::List(_)) {
                                out.push(ModuleDiagnostic {
                                    module: Some(module.id),
                                    diagnostic: Diagnostic::error_code(
                                        value.span,
                                        "AX0215",
                                        format!("`{key}` must be a list of path globs"),
                                    ),
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    crate::analyze::analyze(project, options, &mut out);

    out
}

fn check_effects(
    module: ModuleId,
    effects: Option<&[axiom_syntax::ast::EffectName]>,
    out: &mut Vec<ModuleDiagnostic>,
) {
    let Some(effects) = effects else { return };
    for effect in effects {
        let name = effect.dotted();
        if !KNOWN_EFFECTS.contains(&name.as_str()) {
            out.push(ModuleDiagnostic {
                module: Some(module),
                diagnostic: Diagnostic::error_code(
                    effect.span,
                    "AX0216",
                    format!("unknown effect `{name}`"),
                )
                .with_note("effects use the closed vocabulary in agents.md §3.5"),
            });
        }
    }
}

fn check_model_alias(
    module: ModuleId,
    model: &Expr,
    options: &CheckOptions,
    out: &mut Vec<ModuleDiagnostic>,
) {
    let Some(aliases) = &options.model_aliases else {
        return;
    };
    if let ExprKind::Str(alias) = &model.kind {
        if !aliases.contains(alias) {
            out.push(ModuleDiagnostic {
                module: Some(module),
                diagnostic: Diagnostic::error_code(
                    model.span,
                    "AX0217",
                    format!("unknown model alias `{alias}`"),
                )
                .with_note("declare it under `models:` in axiom.yaml"),
            });
        }
    }
}
