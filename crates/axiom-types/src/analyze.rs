//! Phase 2 analysis passes: name resolution, effect propagation, target
//! compatibility, and database-SQL policy.
//!
//! These passes are deliberately conservative and fail closed: anything they
//! cannot prove is reported rather than assumed safe.

use std::collections::{BTreeMap, BTreeSet};

use axiom_syntax::ast::*;
use axiom_syntax::diagnostic::Diagnostic;

use crate::check::{CheckOptions, ModuleDiagnostic};
use crate::graph::{ModuleGraph, ModuleId, Project};
use crate::symbols::SymbolKind;

/// The compilation target a module is checked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Server,
    WasmBrowser,
}

/// Prelude value constructors and functions always in scope.
pub const PRELUDE_VALUES: &[&str] = &["Some", "None", "Ok", "Err", "capability"];

/// Standard-library namespaces.
pub const NAMESPACES: &[&str] = &[
    "ui",
    "db",
    "fs",
    "agents",
    "runtime",
    "frontend",
    "workspaces",
    "artifacts",
    "patches",
    "commands",
    "checks",
    "browser",
];

/// Built-in types always in scope.
pub const BUILTIN_TYPES: &[&str] = &[
    "string",
    "bool",
    "int",
    "float",
    "bytes",
    "duration",
    "timestamp",
    "Option",
    "Result",
    "Map",
    "UiTree",
    "UiError",
    "JsonValue",
    "Capability",
    "ValidationResult",
    "ValidationFinding",
    "ValidationContext",
    "IterationOutcome",
    "IterationAttempt",
    "Validator",
    "WriteResult",
    "DbError",
    "AuthenticationRequirement",
];

/// Effects allowed on the `wasm_browser` target.
const BROWSER_ALLOWED_EFFECTS: &[&str] = &["api.call", "browser.storage", "browser.navigation"];

/// Signature of a declared function, used for effect propagation.
#[derive(Debug, Clone)]
struct FnSig {
    api: bool,
    effects: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct ModuleIndex {
    functions: BTreeMap<String, FnSig>,
    /// Every top-level declaration name (values and types alike).
    names: BTreeSet<String>,
    /// Type-like declarations (types and enums).
    types: BTreeSet<String>,
    imported: BTreeMap<String, ImportedItem>,
}

#[derive(Debug, Clone)]
enum ImportedItem {
    Function { api: bool, effects: Vec<String> },
    Type,
    Other,
}

/// Run the Phase 2 passes over the project.
pub fn analyze(project: &Project, options: &CheckOptions, out: &mut Vec<ModuleDiagnostic>) {
    let indexes: Vec<ModuleIndex> = project
        .graph
        .modules
        .iter()
        .map(|m| build_index(&project.graph, m.id))
        .collect();

    for module in &project.graph.modules {
        let index = &indexes[module.id];
        for decl in &module.file.declarations {
            match &decl.kind {
                DeclKind::Function(f) => analyze_function(module.id, index, f, options, out),
                DeclKind::Type(t) => check_type_names(module.id, index, &t.ty, &t.generics, out),
                DeclKind::Agent(a) => {
                    for p in &a.params {
                        check_type_names(module.id, index, &p.ty, &[], out);
                    }
                    check_type_names(module.id, index, &a.ret, &[], out);
                }
                DeclKind::Workspace(_) | DeclKind::Enum(_) => {}
            }
        }
    }
}

fn build_index(graph: &ModuleGraph, id: ModuleId) -> ModuleIndex {
    let module = &graph.modules[id];
    let mut index = ModuleIndex::default();
    for decl in &module.file.declarations {
        let Some((name, _)) = crate::symbols::declaration_name(decl) else {
            continue;
        };
        index.names.insert(name.clone());
        match &decl.kind {
            DeclKind::Function(f) => {
                index.functions.insert(
                    name,
                    FnSig {
                        api: f.api,
                        effects: declared_effects(f),
                    },
                );
            }
            DeclKind::Type(_) | DeclKind::Enum(_) => {
                index.types.insert(name);
            }
            _ => {}
        }
    }

    for import in &module.imports {
        let Some(target) = import.target else {
            continue;
        };
        for name in &import.decl.names {
            let item = match graph.modules[target].symbols.get(name) {
                Some(symbol) => match symbol.kind {
                    SymbolKind::Function => {
                        match find_function(&graph.modules[target].file, name) {
                            Some((api, effects)) => ImportedItem::Function { api, effects },
                            None => ImportedItem::Other,
                        }
                    }
                    SymbolKind::Type | SymbolKind::Enum => ImportedItem::Type,
                    _ => ImportedItem::Other,
                },
                None => ImportedItem::Other,
            };
            index.imported.insert(name.clone(), item);
        }
    }

    index
}

fn find_function(file: &File, name: &str) -> Option<(bool, Vec<String>)> {
    file.declarations.iter().find_map(|d| match &d.kind {
        DeclKind::Function(f) if f.name == name => Some((f.api, declared_effects(f))),
        _ => None,
    })
}

fn declared_effects(f: &FunctionDecl) -> Vec<String> {
    match &f.effects {
        Some(list) => list.iter().map(EffectName::dotted).collect(),
        None => Vec::new(),
    }
}

fn analyze_function(
    module: ModuleId,
    index: &ModuleIndex,
    f: &FunctionDecl,
    options: &CheckOptions,
    out: &mut Vec<ModuleDiagnostic>,
) {
    let mut scope: Vec<BTreeSet<String>> = vec![BTreeSet::new()];
    for p in &f.params {
        scope.last_mut().unwrap().insert(p.name.clone());
    }
    let generics: BTreeSet<String> = f.generics.iter().cloned().collect();

    for p in &f.params {
        check_type_names(module, index, &p.ty, &f.generics, out);
    }
    if let Some(ret) = &f.ret {
        check_type_names(module, index, ret, &f.generics, out);
    }

    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut has_await = false;

    check_block(
        module,
        index,
        &generics,
        &mut scope,
        &f.body,
        &mut used,
        &mut has_await,
        out,
    );

    if has_await && !f.is_async {
        out.push(ModuleDiagnostic {
            module: Some(module),
            diagnostic: Diagnostic::error_code(
                f.span,
                "AX0305",
                format!("`{}` uses `await` but is not declared `async`", f.name),
            ),
        });
    }

    let declared: BTreeSet<String> = declared_effects(f).into_iter().collect();
    for effect in &used {
        if !declared.contains(effect) {
            let diagnostic = if f.effects.is_none() {
                Diagnostic::error_code(
                    f.span,
                    "AX0304",
                    format!(
                        "pure function `{}` performs effect `{effect}`; declare `effects [...]`",
                        f.name
                    ),
                )
            } else {
                Diagnostic::error_code(
                    f.span,
                    "AX0303",
                    format!(
                        "`{}` performs effect `{effect}` that it does not declare",
                        f.name
                    ),
                )
            };
            out.push(ModuleDiagnostic {
                module: Some(module),
                diagnostic,
            });
        }
    }

    if options.target == Some(Target::WasmBrowser) {
        let mut offenders = used.clone();
        offenders.extend(declared.iter().cloned());
        for effect in offenders {
            if !BROWSER_ALLOWED_EFFECTS.contains(&effect.as_str()) {
                out.push(ModuleDiagnostic {
                    module: Some(module),
                    diagnostic: Diagnostic::error_code(
                        f.span,
                        "AX0310",
                        format!("effect `{effect}` is not available on the `wasm_browser` target"),
                    )
                    .with_note(
                        "browser modules may only use `api.call`, `browser.storage`, and `browser.navigation`",
                    ),
                });
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn check_block(
    module: ModuleId,
    index: &ModuleIndex,
    generics: &BTreeSet<String>,
    scope: &mut Vec<BTreeSet<String>>,
    block: &Block,
    used: &mut BTreeSet<String>,
    has_await: &mut bool,
    out: &mut Vec<ModuleDiagnostic>,
) {
    for stmt in &block.stmts {
        match &stmt.kind {
            StmtKind::Let { name, ty, value } => {
                check_expr(module, index, generics, scope, value, used, has_await, out);
                if let Some(ty) = ty {
                    let names: Vec<String> = generics.iter().cloned().collect();
                    check_type_names(module, index, ty, &names, out);
                }
                scope.last_mut().unwrap().insert(name.clone());
            }
            StmtKind::For { var, iter, body } => {
                check_expr(module, index, generics, scope, iter, used, has_await, out);
                scope.push(BTreeSet::new());
                scope.last_mut().unwrap().insert(var.clone());
                check_block(module, index, generics, scope, body, used, has_await, out);
                scope.pop();
            }
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    check_expr(module, index, generics, scope, value, used, has_await, out);
                }
            }
            StmtKind::Expr(e) => {
                check_expr(module, index, generics, scope, e, used, has_await, out);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn check_expr(
    module: ModuleId,
    index: &ModuleIndex,
    generics: &BTreeSet<String>,
    scope: &mut Vec<BTreeSet<String>>,
    expr: &Expr,
    used: &mut BTreeSet<String>,
    has_await: &mut bool,
    out: &mut Vec<ModuleDiagnostic>,
) {
    let recurse = |e: &Expr,
                   scope: &mut Vec<BTreeSet<String>>,
                   used: &mut BTreeSet<String>,
                   has_await: &mut bool,
                   out: &mut Vec<ModuleDiagnostic>| {
        check_expr(module, index, generics, scope, e, used, has_await, out);
    };

    match &expr.kind {
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Str(_)
        | ExprKind::Bool(_)
        | ExprKind::Duration { .. }
        | ExprKind::Size { .. } => {}
        ExprKind::Ident(name) => {
            if !name.is_empty() && !resolves(name, index, scope) {
                out.push(ModuleDiagnostic {
                    module: Some(module),
                    diagnostic: Diagnostic::error_code(
                        expr.span,
                        "AX0301",
                        format!("cannot resolve name `{name}`"),
                    ),
                });
            }
        }
        ExprKind::List(items) => {
            for item in items {
                recurse(&item.value, scope, used, has_await, out);
            }
        }
        ExprKind::Record(members) => {
            for m in members {
                recurse(&m.value, scope, used, has_await, out);
            }
        }
        ExprKind::Field(base, _) => recurse(base, scope, used, has_await, out),
        ExprKind::Index(base, ix) => {
            recurse(base, scope, used, has_await, out);
            recurse(ix, scope, used, has_await, out);
        }
        ExprKind::Call { callee, args, .. } => match path_of(callee) {
            Some(path) => {
                add_call_effects(&path, index, used);
                check_sql(&path, args, module, out);
                let first = path.split('.').next().unwrap_or(path.as_str());
                if path.contains('.') && !NAMESPACES.contains(&first) {
                    if let ExprKind::Field(base, _) = &callee.kind {
                        recurse(base, scope, used, has_await, out);
                    }
                }
            }
            None => recurse(callee, scope, used, has_await, out),
        },
        ExprKind::Unary { operand, .. } => recurse(operand, scope, used, has_await, out),
        ExprKind::Binary { lhs, rhs, .. } => {
            recurse(lhs, scope, used, has_await, out);
            recurse(rhs, scope, used, has_await, out);
        }
        ExprKind::Await(inner) => {
            *has_await = true;
            recurse(inner, scope, used, has_await, out);
        }
        ExprKind::If {
            cond,
            then,
            otherwise,
        } => {
            recurse(cond, scope, used, has_await, out);
            check_block(module, index, generics, scope, then, used, has_await, out);
            if let Some(otherwise) = otherwise {
                recurse(otherwise, scope, used, has_await, out);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            recurse(scrutinee, scope, used, has_await, out);
            for arm in arms {
                scope.push(BTreeSet::new());
                bind_pattern(&arm.pattern, scope);
                check_block(
                    module, index, generics, scope, &arm.body, used, has_await, out,
                );
                scope.pop();
            }
        }
        ExprKind::Block(block) => {
            check_block(module, index, generics, scope, block, used, has_await, out);
        }
    }
}

fn bind_pattern(pattern: &Pattern, scope: &mut Vec<BTreeSet<String>>) {
    match pattern {
        Pattern::Binding(name) => {
            scope.last_mut().unwrap().insert(name.clone());
        }
        Pattern::Constructor {
            inner: Some(items), ..
        } => {
            for item in items {
                bind_pattern(item, scope);
            }
        }
        _ => {}
    }
}

fn resolves(name: &str, index: &ModuleIndex, scope: &[BTreeSet<String>]) -> bool {
    scope.iter().any(|frame| frame.contains(name))
        || PRELUDE_VALUES.contains(&name)
        || NAMESPACES.contains(&name)
        || index.names.contains(name)
        || index.imported.contains_key(name)
}

fn path_of(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Ident(name) => Some(name.clone()),
        ExprKind::Field(base, name) => path_of(base).map(|b| format!("{b}.{name}")),
        _ => None,
    }
}

fn add_call_effects(path: &str, index: &ModuleIndex, used: &mut BTreeSet<String>) {
    if let Some(effects) = stdlib_effects(path) {
        for e in effects {
            used.insert((*e).to_string());
        }
        return;
    }
    let first = path.split('.').next().unwrap_or(path);
    if NAMESPACES.contains(&first) {
        // A known namespace with an unmodelled member: real subsystems register
        // their signatures in later phases. Do not invent effects here.
        return;
    }
    if let Some(sig) = index.functions.get(path) {
        if sig.api {
            used.insert("api.call".to_string());
        } else {
            used.extend(sig.effects.iter().cloned());
        }
        return;
    }
    if let Some(ImportedItem::Function { api, effects }) = index.imported.get(path) {
        if *api {
            used.insert("api.call".to_string());
        } else {
            used.extend(effects.iter().cloned());
        }
    }
}

/// Effects of standard-library APIs. `Some(&[])` means "known and pure";
/// `None` means "not modelled".
fn stdlib_effects(path: &str) -> Option<&'static [&'static str]> {
    let effects: &'static [&'static str] = match path {
        "db.query" | "db.query_one" => &["db.read"],
        "db.execute" => &["db.write"],
        "db.schema.inspect" | "db.migrations.plan" | "db.migrations.apply" => &["db.schema"],
        "db.scratch.create" | "db.scratch.destroy" | "db.audit_roles" => &["db.provision"],
        "fs.list" | "fs.read_text" | "fs.search" | "fs.diff" => &["fs.read"],
        "fs.write_text" => &["fs.write"],
        "fs.delete" => &["fs.delete"],
        "workspaces.create"
        | "workspaces.snapshot"
        | "workspaces.restore"
        | "workspaces.submit"
        | "workspaces.discard"
        | "workspaces.integrate" => &["workspace.create"],
        "artifacts.publish" | "frontend.publish" => &["artifact.publish"],
        "artifacts.fetch" | "artifacts.verify" | "artifacts.inspect" => &["artifact.read"],
        "patches.check_conflicts" => &["fs.read"],
        "patches.apply_set" => &["fs.write"],
        "commands.run" => &["command.run"],
        "runtime.checkpoint" => &["runtime.checkpoint"],
        "agents.iterate" => &["model"],
        "browser.storage.get" | "browser.storage.set" | "browser.storage.remove" => {
            &["browser.storage"]
        }
        "browser.navigation.goto" | "browser.navigation.reload" => &["browser.navigation"],
        _ if path.starts_with("ui.")
            || path.starts_with("checks.")
            || path.starts_with("capability") =>
        {
            &[]
        }
        _ => return None,
    };
    Some(effects)
}

fn check_type_names(
    module: ModuleId,
    index: &ModuleIndex,
    ty: &TypeExpr,
    generics: &[String],
    out: &mut Vec<ModuleDiagnostic>,
) {
    match &ty.kind {
        TypeKind::Named { path, args } => {
            let dotted = path.join(".");
            let name = path.first().map(String::as_str).unwrap_or("");
            let known = BUILTIN_TYPES.contains(&dotted.as_str())
                || BUILTIN_TYPES.contains(&name)
                || generics.iter().any(|g| g == name)
                || index.types.contains(name)
                || matches!(index.imported.get(name), Some(ImportedItem::Type));
            if !known {
                out.push(ModuleDiagnostic {
                    module: Some(module),
                    diagnostic: Diagnostic::error_code(
                        ty.span,
                        "AX0302",
                        format!("cannot resolve type `{dotted}`"),
                    ),
                });
            }
            for arg in args {
                check_type_names(module, index, arg, generics, out);
            }
        }
        TypeKind::List(inner) => check_type_names(module, index, inner, generics, out),
        TypeKind::Record(fields) => {
            for field in fields {
                check_type_names(module, index, &field.ty, generics, out);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// SQL policy
// ---------------------------------------------------------------------------

fn check_sql(path: &str, args: &[Argument], module: ModuleId, out: &mut Vec<ModuleDiagnostic>) {
    if !matches!(path, "db.query" | "db.query_one" | "db.execute") {
        return;
    }
    let sql = named_arg(args, "sql");
    let params = named_arg(args, "params");

    let placeholders = match sql {
        Some(expr) => match &expr.kind {
            ExprKind::Str(text) => Some(parse_placeholders(text)),
            _ => {
                out.push(ModuleDiagnostic {
                    module: Some(module),
                    diagnostic: Diagnostic::error_code(
                        expr.span,
                        "AX0320",
                        "SQL text must be a compile-time string literal",
                    )
                    .with_note("model-generated or computed SQL is forbidden"),
                });
                return;
            }
        },
        None => None,
    };

    if let (
        Some(placeholders),
        Some(Expr {
            kind: ExprKind::Record(members),
            span,
        }),
    ) = (placeholders, params)
    {
        let provided: BTreeSet<String> = members.iter().map(|m| m.name.clone()).collect();
        for ph in &placeholders {
            if !provided.contains(ph) {
                out.push(ModuleDiagnostic {
                    module: Some(module),
                    diagnostic: Diagnostic::error_code(
                        *span,
                        "AX0321",
                        format!("SQL placeholder `:{ph}` has no matching `params` entry"),
                    ),
                });
            }
        }
        for key in &provided {
            if !placeholders.contains(key) {
                out.push(ModuleDiagnostic {
                    module: Some(module),
                    diagnostic: Diagnostic::error_code(
                        *span,
                        "AX0321",
                        format!("`params` entry `{key}` has no matching SQL placeholder"),
                    ),
                });
            }
        }
    }
}

fn named_arg<'a>(args: &'a [Argument], name: &str) -> Option<&'a Expr> {
    args.iter()
        .find(|a| a.name.as_deref() == Some(name))
        .map(|a| &a.value)
}

fn parse_placeholders(sql: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ':' && chars.get(i + 1) != Some(&':') && (i == 0 || chars[i - 1] != ':') {
            let mut j = i + 1;
            let mut name = String::new();
            while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                name.push(chars[j]);
                j += 1;
            }
            if !name.is_empty() && !name.chars().next().unwrap().is_ascii_digit() {
                out.insert(name);
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}
