//! Semantic checks over a loaded [`Project`].
//!
//! Phase 2 scope: the checks that do not require a full type-inference engine
//! but do catch the classes the spec calls out — duplicate declarations and
//! module paths, malformed aggregates, generic arity for built-in types,
//! invalid or misused effects, and cross-validation of literal model aliases and
//! command names against `axiom.yaml`.
//!
//! Type *inference* and full expression typing are deliberately out of scope
//! here and tracked in `docs/implementation-ledger.md`.

use std::collections::{BTreeMap, BTreeSet};

use axiom_syntax::ast::*;
use axiom_syntax::diagnostic::Diagnostic;
use axiom_syntax::span::Span;

use crate::graph::{GraphDiagnostic, Module, Project};
use crate::symbols::Symbol;

/// The effect names the language defines (`agents.md` §5.5).
pub const KNOWN_EFFECTS: &[&str] = &[
    "fs.read",
    "fs.write",
    "fs.delete",
    "artifact",
    "model",
    "command",
    "workspace",
    "network",
    "integration",
    "db.read",
    "db.write",
    "db.schema",
    "db.provision",
    "api.call",
    "ui.render",
    "ui.event",
    "ui.state",
    "browser.storage",
    "browser.navigation",
    "browser.clipboard",
    "browser.download",
];

/// Contract clause keys may repeat, provided each occurrence has a distinct label.
const REPEATABLE_CLAUSES: &[&str] = &["precondition", "postcondition", "invariant", "criterion"];

/// Literal model aliases and command names to validate against configuration.
#[derive(Debug, Default, Clone)]
pub struct CheckOptions {
    pub model_aliases: Option<BTreeSet<String>>,
    pub command_names: Option<BTreeSet<String>>,
}

impl CheckOptions {
    /// No source/config cross-validation.
    pub fn none() -> Self {
        CheckOptions::default()
    }

    pub fn with_models(names: impl IntoIterator<Item = String>) -> Self {
        CheckOptions {
            model_aliases: Some(names.into_iter().collect()),
            command_names: None,
        }
    }
}

/// Run every semantic check over a loaded project.
///
/// The result includes graph/import diagnostics, parse diagnostics, and the
/// semantic diagnostics, each attributed to a module when one applies.
pub fn check_project(project: &Project, options: &CheckOptions) -> Vec<GraphDiagnostic> {
    let mut out: Vec<GraphDiagnostic> = Vec::new();
    out.extend(project.diagnostics.iter().map(|d| GraphDiagnostic {
        module: d.module,
        diagnostic: d.diagnostic.clone(),
    }));

    check_duplicate_module_paths(project, &mut out);

    for module in &project.graph.modules {
        for diagnostic in &module.parse_diagnostics {
            out.push(GraphDiagnostic {
                module: Some(module.id),
                diagnostic: diagnostic.clone(),
            });
        }
        for diagnostic in check_module(module, options) {
            out.push(GraphDiagnostic {
                module: Some(module.id),
                diagnostic,
            });
        }
    }
    out
}

/// True if any diagnostic in the report is an error.
pub fn has_errors(diagnostics: &[GraphDiagnostic]) -> bool {
    diagnostics.iter().any(|d| d.diagnostic.is_error())
}

fn check_duplicate_module_paths(project: &Project, out: &mut Vec<GraphDiagnostic>) {
    let mut by_path: BTreeMap<&str, Vec<&Module>> = BTreeMap::new();
    for module in &project.graph.modules {
        if let Some(path) = &module.module_path {
            by_path.entry(path.as_str()).or_default().push(module);
        }
    }
    for (path, modules) in by_path {
        if modules.len() < 2 {
            continue;
        }
        let first = modules[0];
        for module in &modules[1..] {
            out.push(GraphDiagnostic {
                module: Some(module.id),
                diagnostic: Diagnostic::error(
                    span_of_module(module),
                    format!("module path `{path}` is declared by more than one file"),
                )
                .with_note(format!("already declared by `{}`", first.display)),
            });
        }
    }
}

fn span_of_module(module: &Module) -> Span {
    module
        .file
        .module
        .as_ref()
        .map(|m| m.span)
        .unwrap_or(Span::empty(0))
}

/// All semantic diagnostics for a single module.
pub fn check_module(module: &Module, options: &CheckOptions) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for (first, duplicate) in module.symbols.duplicates() {
        diagnostics.push(duplicate_declaration(first, duplicate));
    }
    for item in &module.file.items {
        check_item(item, options, &mut diagnostics);
    }
    crate::resolve::resolve_module(module, &mut diagnostics);
    diagnostics
}

fn duplicate_declaration(first: &Symbol, duplicate: &Symbol) -> Diagnostic {
    Diagnostic::error(
        duplicate.span,
        format!(
            "`{}` is declared more than once in this module",
            duplicate.name
        ),
    )
    .with_note(format!(
        "first declared as a {} at byte {}",
        first.kind.as_str(),
        first.span.start
    ))
}

// --- declarations ---------------------------------------------------------

fn check_item(item: &Item, options: &CheckOptions, out: &mut Vec<Diagnostic>) {
    match &item.kind {
        ItemKind::Type(decl) => check_type(&decl.value, out),
        ItemKind::Interface(decl) => {
            check_duplicate_fields(
                decl.properties
                    .iter()
                    .map(|p| (p.name.text.as_str(), p.name.span)),
                "interface property",
                out,
            );
            for property in &decl.properties {
                check_type(&property.ty, out);
            }
        }
        ItemKind::Enum(decl) => {
            check_duplicate_fields(
                decl.members
                    .iter()
                    .map(|m| (m.name.text.as_str(), m.name.span)),
                "enum member",
                out,
            );
            for member in &decl.members {
                if let Some(value) = &member.value {
                    check_expr(value, options, out);
                }
            }
        }
        ItemKind::Const(decl) => {
            if let Some(ty) = &decl.ty {
                check_type(ty, out);
            }
            check_expr(&decl.value, options, out);
        }
        ItemKind::Function(decl) => check_function(decl, options, out),
        ItemKind::Agent(decl) => {
            check_type(&decl.implements, out);
            check_members(&decl.members, options, out);
        }
        ItemKind::Tool(decl) => {
            check_params(&decl.params, out);
            check_type(&decl.ret, out);
            check_effects(&decl.effects, out);
            check_members(&decl.members, options, out);
        }
        ItemKind::Query(decl) => {
            check_params(&decl.params, out);
            check_type(&decl.ret, out);
            check_members(&decl.members, options, out);
            check_query(decl, out);
        }
        ItemKind::Operation(decl) => {
            check_params(&decl.params, out);
            check_type(&decl.ret, out);
            check_members(&decl.members, options, out);
            check_operation(decl, out);
        }
        ItemKind::Goal(decl) => {
            check_params(&decl.params, out);
            check_type(&decl.ret, out);
            check_members(&decl.members, options, out);
        }
        ItemKind::Workflow(decl) => {
            check_params(&decl.params, out);
            if let Some(ret) = &decl.ret {
                check_type(ret, out);
            }
            check_stmts(&decl.body, options, out);
        }
        ItemKind::MemberBlock(decl) => check_members(&decl.members, options, out),
    }
}

fn check_function(decl: &FunctionDecl, options: &CheckOptions, out: &mut Vec<Diagnostic>) {
    check_params(&decl.params, out);
    if let Some(ret) = &decl.ret {
        check_type(ret, out);
    }
    match &decl.effects {
        Some(effects) => {
            check_effects(effects, out);
            if decl.pure {
                out.push(
                    Diagnostic::error(decl.name.span, "a `pure` function cannot declare effects")
                        .with_note("remove `pure` or the `effects` clause"),
                );
            }
        }
        None => {
            if !decl.pure {
                out.push(
                    Diagnostic::error(
                        decl.name.span,
                        "a non-`pure` function must declare its effects",
                    )
                    .with_note("add `pure`, or an `effects [ ... ]` clause after the return type"),
                );
            }
        }
    }
    if decl.pure && stmts_are_effectful(&decl.body) {
        out.push(
            Diagnostic::error(
                decl.name.span,
                "a `pure` function cannot perform effectful work",
            )
            .with_note("`await`, `delegate`, and `parallel` are effectful"),
        );
    }
    check_stmts(&decl.body, options, out);
}

fn check_query(decl: &QueryDecl, out: &mut Vec<Diagnostic>) {
    let member = |name: &str| decl.members.iter().find(|m| m.name.text == name);

    match member("sql") {
        None => out.push(
            Diagnostic::error(
                decl.name.span,
                "query must declare a single `sql` string literal",
            )
            .with_note("add `sql: \"<statement>\";`"),
        ),
        Some(entry) => match &entry.value {
            MemberValue::Expr(Expr {
                kind: ExprKind::Literal(Literal::String(sql)),
                span,
            }) => check_query_placeholders(decl, sql, *span, out),
            _ => out.push(
                Diagnostic::error(entry.span, "`sql` must be one string literal")
                    .with_note("interpolated, concatenated, or computed SQL is rejected"),
            ),
        },
    }

    if !matches!(
        member("database").map(|m| &m.value),
        Some(MemberValue::Expr(_))
    ) {
        out.push(Diagnostic::error(
            decl.name.span,
            "query must declare `database`",
        ));
    }

    if let Some(access) = member("access").and_then(member_access_path) {
        let is_write = access == "DbAccess.Write";
        if is_write && decl.ret.display_name() != "WriteResult" {
            out.push(
                Diagnostic::error(decl.name.span, "a write query must return `WriteResult`")
                    .with_note(format!("found `{}`", decl.ret.display_name())),
            );
        }
    } else {
        out.push(
            Diagnostic::error(decl.name.span, "query must declare `access`")
                .with_note("use `access: DbAccess.Read;` or `access: DbAccess.Write;`"),
        );
    }
}

/// Check a backend `operation` declaration's metadata members.
fn check_operation(decl: &OperationDecl, out: &mut Vec<Diagnostic>) {
    let member = |name: &str| decl.members.iter().find(|m| m.name.text == name);

    match member("version") {
        Some(entry) => {
            if !matches!(
                &entry.value,
                MemberValue::Expr(Expr {
                    kind: ExprKind::Literal(Literal::String(_)),
                    ..
                })
            ) {
                out.push(Diagnostic::error(
                    entry.span,
                    "`version` must be a string literal",
                ));
            }
        }
        None => out.push(Diagnostic::error(
            decl.name.span,
            "operation must declare `version`",
        )),
    }

    match member("authentication").and_then(member_access_path) {
        Some(path) if path.starts_with("AuthenticationRequirement.") => {}
        Some(_) | None => out.push(
            Diagnostic::error(decl.name.span, "operation must declare `authentication`")
                .with_note("use `AuthenticationRequirement.Required;` or `...Anonymous;`"),
        ),
    }

    let handler = member("handler").map(|m| &m.value);
    match handler {
        Some(MemberValue::Expr(Expr {
            kind: ExprKind::Path(path),
            ..
        })) if !path.is_empty() => {}
        _ => out.push(
            Diagnostic::error(decl.name.span, "operation must declare a `handler`")
                .with_note("`handler: <function name>;`"),
        ),
    }
}

/// Check an `iterate` block: a positive `max_iterations`, a `validate` list, and
/// an `until` condition.
fn check_iterate(members: &[Member], fallback: Span, out: &mut Vec<Diagnostic>) {
    let member = |name: &str| members.iter().find(|m| m.name.text == name);

    match member("max_iterations").map(|m| &m.value) {
        Some(MemberValue::Expr(Expr {
            kind: ExprKind::Literal(Literal::Int(n)),
            ..
        })) if *n > 0 => {}
        Some(MemberValue::Expr(e)) => out.push(
            Diagnostic::error(
                e.span,
                "`max_iterations` must be a positive integer literal",
            )
            .with_note("the iteration bound is a compile-time constant"),
        ),
        Some(_) => out.push(Diagnostic::error(
            fallback,
            "`max_iterations` must be a positive integer literal",
        )),
        None => out.push(
            Diagnostic::error(fallback, "`iterate` requires a `max_iterations` bound")
                .with_note("for example `max_iterations: 4;`"),
        ),
    }

    match member("validate").map(|m| &m.value) {
        Some(MemberValue::Expr(Expr {
            kind: ExprKind::List(_),
            ..
        })) => {}
        _ => out.push(Diagnostic::error(
            fallback,
            "`iterate` requires a `validate` list",
        )),
    }

    if member("until").is_none() {
        out.push(Diagnostic::error(
            fallback,
            "`iterate` requires an `until` condition",
        ));
    }
}

/// Extract a dotted member-access path such as `DbAccess.Write` from a member value.
fn member_access_path(member: &Member) -> Option<String> {
    let MemberValue::Expr(Expr {
        kind: ExprKind::Member { base, name },
        ..
    }) = &member.value
    else {
        return None;
    };
    let ExprKind::Path(path) = &base.kind else {
        return None;
    };
    let mut segments: Vec<&str> = path.iter().map(|i| i.text.as_str()).collect();
    segments.push(name.text.as_str());
    Some(segments.join("."))
}

/// SQL parameters appear only as `:name` placeholders that match the declared
/// parameters exactly (none missing, none extra).
fn check_query_placeholders(decl: &QueryDecl, sql: &str, span: Span, out: &mut Vec<Diagnostic>) {
    let placeholders = sql_placeholders(sql);
    for placeholder in &placeholders {
        if !decl.params.iter().any(|p| &p.name.text == placeholder) {
            out.push(
                Diagnostic::error(
                    span,
                    format!("placeholder `:{placeholder}` has no parameter"),
                )
                .with_note("every `:name` placeholder must match a declared parameter"),
            );
        }
    }
    for param in &decl.params {
        if !placeholders.iter().any(|p| p == &param.name.text) {
            out.push(
                Diagnostic::error(
                    param.name.span,
                    format!("parameter `{}` is not used in `sql`", param.name.text),
                )
                .with_note("declared parameters must appear as `:name` placeholders"),
            );
        }
    }
}

/// Collect `:name` placeholders, ignoring casts (`::`) and anything inside a
/// placeholder-free context. This is a lexical pre-check; the real SQL scanner
/// is Phase 6.
fn sql_placeholders(sql: &str) -> Vec<String> {
    let bytes = sql.as_bytes();
    let mut placeholders = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' && bytes.get(i + 1) != Some(&b':') {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            if end > start {
                placeholders.push(sql[start..end].to_string());
                i = end;
                continue;
            }
        }
        i += 1;
    }
    placeholders
}

fn check_params(params: &[Param], out: &mut Vec<Diagnostic>) {
    check_duplicate_fields(
        params.iter().map(|p| (p.name.text.as_str(), p.name.span)),
        "parameter",
        out,
    );
    for param in params {
        if let Some(ty) = &param.ty {
            check_type(ty, out);
        }
        if let Some(default) = &param.default {
            check_expr(default, &CheckOptions::none(), out);
        }
    }
}

fn check_effects(effects: &[Ident], out: &mut Vec<Diagnostic>) {
    for effect in effects {
        if !KNOWN_EFFECTS.contains(&effect.text.as_str()) {
            out.push(
                Diagnostic::error(effect.span, format!("unknown effect `{}`", effect.text))
                    .with_note(format!("known effects: {}", KNOWN_EFFECTS.join(", "))),
            );
        }
    }
}

// --- member blocks --------------------------------------------------------

fn check_members(members: &[Member], options: &CheckOptions, out: &mut Vec<Diagnostic>) {
    let mut seen: BTreeMap<&str, Span> = BTreeMap::new();
    let mut clauses: BTreeMap<&str, BTreeMap<String, Span>> = BTreeMap::new();

    for member in members {
        let key = member.name.text.as_str();
        if REPEATABLE_CLAUSES.contains(&key) {
            match member.label.as_ref().map(label_text) {
                Some(label) => {
                    let entry = clauses.entry(key).or_default();
                    if entry.insert(label.clone(), member.span).is_some() {
                        out.push(Diagnostic::error(
                            member.name.span,
                            format!("duplicate `{key}` clause labeled \"{label}\""),
                        ));
                    }
                }
                None => out.push(
                    Diagnostic::error(member.name.span, format!("`{key}` requires a label"))
                        .with_note(format!("write `{key} name: ...;`")),
                ),
            }
        } else if let Some(first) = seen.get(key) {
            out.push(
                Diagnostic::error(
                    member.name.span,
                    format!("duplicate member `{key}` in this block"),
                )
                .with_note(format!("first declared at byte {}", first.start)),
            );
        } else {
            seen.insert(key, member.span);
        }

        match &member.value {
            MemberValue::Expr(expr) | MemberValue::Arrow(expr) => {
                check_expr(expr, options, out);
                if key == "model" {
                    if let ExprKind::Literal(Literal::String(alias)) = &expr.kind {
                        check_model_alias(alias, expr.span, options, out);
                    }
                }
            }
            MemberValue::Block(inner) => check_members(inner, options, out),
        }
    }
}

fn label_text(label: &MemberLabel) -> String {
    match label {
        MemberLabel::Ident(ident) => ident.text.clone(),
        MemberLabel::String(string) => string.value.clone(),
    }
}

fn check_model_alias(alias: &str, span: Span, options: &CheckOptions, out: &mut Vec<Diagnostic>) {
    if let Some(models) = &options.model_aliases {
        if !models.contains(alias) {
            out.push(
                Diagnostic::error(
                    span,
                    format!("model alias `{alias}` is not defined in axiom.yaml"),
                )
                .with_note(format!("known aliases: {}", join_or_none(models))),
            );
        }
    }
}

// --- types ----------------------------------------------------------------

fn check_type(ty: &TypeExpr, out: &mut Vec<Diagnostic>) {
    match &ty.kind {
        TypeExprKind::Ref { path, args } => {
            let name = path
                .iter()
                .map(|i| i.text.as_str())
                .collect::<Vec<_>>()
                .join(".");
            if let Some(expected) = builtin_arity(&name) {
                if args.len() != expected {
                    out.push(Diagnostic::error(
                        ty.span,
                        format!(
                            "type `{name}` expects {expected} type argument(s), found {}",
                            args.len()
                        ),
                    ));
                }
            }
            for arg in args {
                check_type(arg, out);
            }
        }
        TypeExprKind::Array(inner) => check_type(inner, out),
        TypeExprKind::Union(parts) => {
            for part in parts {
                check_type(part, out);
            }
        }
        TypeExprKind::Tuple(parts) => {
            for part in parts {
                check_type(part, out);
            }
        }
        TypeExprKind::Record(fields) => {
            check_duplicate_fields(
                fields.iter().map(|f| (f.name.text.as_str(), f.name.span)),
                "field",
                out,
            );
            for field in fields {
                check_type(&field.ty, out);
            }
        }
        TypeExprKind::Str(_) => {}
    }
}

/// Arity of the type constructors the language defines.
fn builtin_arity(name: &str) -> Option<usize> {
    match name {
        "Option" => Some(1),
        "Result" => Some(2),
        "Map" => Some(2),
        "Agent" => Some(2),
        _ => None,
    }
}

// --- statements -----------------------------------------------------------

fn check_stmts(stmts: &[Stmt], options: &CheckOptions, out: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        check_stmt(stmt, options, out);
    }
}

fn check_stmt(stmt: &Stmt, options: &CheckOptions, out: &mut Vec<Diagnostic>) {
    match &stmt.kind {
        StmtKind::Const(decl) => check_item(
            &Item {
                exported: false,
                kind: ItemKind::Const(decl.clone()),
                span: decl.span,
            },
            options,
            out,
        ),
        StmtKind::Assignment { target, value, .. } => {
            check_expr(target, options, out);
            check_expr(value, options, out);
        }
        StmtKind::If {
            cond,
            then_body,
            else_body,
        } => {
            check_expr(cond, options, out);
            check_stmts(then_body, options, out);
            check_stmts(else_body, options, out);
        }
        StmtKind::Match { scrutinee, arms } => {
            check_expr(scrutinee, options, out);
            for arm in arms {
                match &arm.body {
                    MatchBody::Expr(expr) => check_expr(expr, options, out),
                    MatchBody::Block(body) => check_stmts(body, options, out),
                }
            }
        }
        StmtKind::For { iter, body, .. } | StmtKind::Parallel { iter, body, .. } => {
            check_expr(iter, options, out);
            if let StmtKind::Parallel { max: Some(max), .. } = &stmt.kind {
                check_expr(max, options, out);
            }
            check_stmts(body, options, out);
        }
        StmtKind::While { cond, max, body } => {
            check_expr(cond, options, out);
            if let Some(max) = max {
                check_expr(max, options, out);
            }
            check_stmts(body, options, out);
        }
        StmtKind::Return(Some(expr))
        | StmtKind::Yield(Some(expr))
        | StmtKind::Throw(expr)
        | StmtKind::Emit(expr)
        | StmtKind::Checkpoint(Some(expr)) => check_expr(expr, options, out),
        StmtKind::Try {
            body,
            catches,
            finally,
        } => {
            check_stmts(body, options, out);
            for catch in catches {
                check_stmts(&catch.body, options, out);
            }
            check_stmts(finally, options, out);
        }
        StmtKind::Require {
            cond,
            else_expr,
            escalate,
        } => {
            check_expr(cond, options, out);
            if let Some(expr) = else_expr {
                check_expr(expr, options, out);
            }
            if let Some(expr) = escalate {
                check_expr(expr, options, out);
            }
        }
        StmtKind::Retry { attempts, body } => {
            check_expr(attempts, options, out);
            check_stmts(body, options, out);
        }
        StmtKind::Timeout { duration, body } => {
            check_expr(duration, options, out);
            check_stmts(body, options, out);
        }
        StmtKind::OnError { body, .. } => check_stmts(body, options, out),
        StmtKind::Expr(expr) => check_expr(expr, options, out),
        StmtKind::Item(item) => check_item(item, options, out),
        StmtKind::Return(None)
        | StmtKind::Yield(None)
        | StmtKind::Break
        | StmtKind::Continue
        | StmtKind::Checkpoint(None) => {}
    }
}

fn stmts_are_effectful(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_is_effectful)
}

fn stmt_is_effectful(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Parallel { .. } => true,
        StmtKind::If {
            then_body,
            else_body,
            ..
        } => stmts_are_effectful(then_body) || stmts_are_effectful(else_body),
        StmtKind::For { body, .. } | StmtKind::While { body, .. } => stmts_are_effectful(body),
        StmtKind::Try {
            body,
            catches,
            finally,
        } => {
            stmts_are_effectful(body)
                || catches.iter().any(|c| stmts_are_effectful(&c.body))
                || stmts_are_effectful(finally)
        }
        StmtKind::Retry { body, .. }
        | StmtKind::Timeout { body, .. }
        | StmtKind::OnError { body, .. } => stmts_are_effectful(body),
        StmtKind::Match { arms, .. } => arms.iter().any(|arm| match &arm.body {
            MatchBody::Expr(expr) => expr_is_effectful(expr),
            MatchBody::Block(body) => stmts_are_effectful(body),
        }),
        StmtKind::Assignment { target, value, .. } => {
            expr_is_effectful(target) || expr_is_effectful(value)
        }
        StmtKind::Const(decl) => expr_is_effectful(&decl.value),
        StmtKind::Emit(expr) | StmtKind::Throw(expr) => expr_is_effectful(expr),
        StmtKind::Require {
            cond,
            else_expr,
            escalate,
        } => {
            expr_is_effectful(cond)
                || else_expr.as_ref().is_some_and(expr_is_effectful)
                || escalate.as_ref().is_some_and(expr_is_effectful)
        }
        StmtKind::Expr(expr) => expr_is_effectful(expr),
        _ => false,
    }
}

fn expr_is_effectful(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Await(_) | ExprKind::Delegate { .. } | ExprKind::Parallel { .. } => true,
        ExprKind::Iterate { .. } => true,
        ExprKind::Unary { operand, .. } => expr_is_effectful(operand),
        ExprKind::Binary { left, right, .. } => expr_is_effectful(left) || expr_is_effectful(right),
        ExprKind::Conditional {
            cond,
            then_expr,
            else_expr,
        } => {
            expr_is_effectful(cond) || expr_is_effectful(then_expr) || expr_is_effectful(else_expr)
        }
        ExprKind::Member { base, .. } => expr_is_effectful(base),
        ExprKind::Index { base, index } => expr_is_effectful(base) || expr_is_effectful(index),
        ExprKind::Call { callee, args } => {
            expr_is_effectful(callee) || args.iter().any(|a| expr_is_effectful(&a.value))
        }
        ExprKind::List(items) => items.iter().any(expr_is_effectful),
        ExprKind::Object(entries) => entries.iter().any(|e| expr_is_effectful(&e.value)),
        ExprKind::Lambda { body, .. } => match body {
            LambdaBody::Expr(expr) => expr_is_effectful(expr),
            LambdaBody::Block(body) => stmts_are_effectful(body),
        },
        _ => false,
    }
}

// --- expressions ----------------------------------------------------------

fn check_expr(expr: &Expr, options: &CheckOptions, out: &mut Vec<Diagnostic>) {
    match &expr.kind {
        ExprKind::Object(entries) => {
            for entry in entries {
                if entry.key_text() == "model" {
                    if let ExprKind::Literal(Literal::String(alias)) = &entry.value.kind {
                        check_model_alias(alias, entry.value.span, options, out);
                    }
                }
                check_expr(&entry.value, options, out);
            }
        }
        ExprKind::Call { callee, args } => {
            if let ExprKind::Member { base, name } = &callee.kind {
                if name.text == "run" && path_text(base).as_deref() == Some("commands") {
                    if let Some(first) = args.first() {
                        if let ExprKind::Literal(Literal::String(command)) = &first.value.kind {
                            check_command_name(command, first.value.span, options, out);
                        }
                    }
                }
            }
            check_expr(callee, options, out);
            for arg in args {
                check_expr(&arg.value, options, out);
            }
        }
        ExprKind::List(items) => {
            for item in items {
                check_expr(item, options, out);
            }
        }
        ExprKind::Unary { operand, .. } => check_expr(operand, options, out),
        ExprKind::Binary { left, right, .. } => {
            check_expr(left, options, out);
            check_expr(right, options, out);
        }
        ExprKind::Conditional {
            cond,
            then_expr,
            else_expr,
        } => {
            check_expr(cond, options, out);
            check_expr(then_expr, options, out);
            check_expr(else_expr, options, out);
        }
        ExprKind::Member { base, .. } => check_expr(base, options, out),
        ExprKind::Index { base, index } => {
            check_expr(base, options, out);
            check_expr(index, options, out);
        }
        ExprKind::Await(inner) => check_expr(inner, options, out),
        ExprKind::Delegate { args, .. } => {
            for arg in args {
                check_expr(&arg.value, options, out);
            }
        }
        ExprKind::Parallel {
            iter, max, body, ..
        } => {
            check_expr(iter, options, out);
            if let Some(max) = max {
                check_expr(max, options, out);
            }
            check_stmts(body, options, out);
        }
        ExprKind::Lambda { body, .. } => match body {
            LambdaBody::Expr(expr) => check_expr(expr, options, out),
            LambdaBody::Block(body) => check_stmts(body, options, out),
        },
        ExprKind::Iterate { call, members } => {
            check_expr(call, options, out);
            check_iterate(members, call.span, out);
            check_members(members, options, out);
        }
        ExprKind::Literal(_) | ExprKind::Path(_) => {}
    }
}

fn check_command_name(name: &str, span: Span, options: &CheckOptions, out: &mut Vec<Diagnostic>) {
    if let Some(commands) = &options.command_names {
        if !commands.contains(name) {
            out.push(
                Diagnostic::error(
                    span,
                    format!("command `{name}` is not defined in axiom.yaml"),
                )
                .with_note(format!("known commands: {}", join_or_none(commands))),
            );
        }
    }
}

fn path_text(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Path(path) => Some(
            path.iter()
                .map(|i| i.text.as_str())
                .collect::<Vec<_>>()
                .join("."),
        ),
        _ => None,
    }
}

// --- helpers --------------------------------------------------------------

fn check_duplicate_fields<'a>(
    items: impl Iterator<Item = (&'a str, Span)>,
    what: &str,
    out: &mut Vec<Diagnostic>,
) {
    let mut seen: BTreeMap<&str, Span> = BTreeMap::new();
    for (name, span) in items {
        if let Some(first) = seen.get(name) {
            out.push(
                Diagnostic::error(span, format!("duplicate {what} `{name}`"))
                    .with_note(format!("first declared at byte {}", first.start)),
            );
        } else {
            seen.insert(name, span);
        }
    }
}

fn join_or_none(names: &BTreeSet<String>) -> String {
    if names.is_empty() {
        "(none)".to_string()
    } else {
        names.iter().cloned().collect::<Vec<_>>().join(", ")
    }
}
