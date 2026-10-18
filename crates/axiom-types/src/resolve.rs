//! Strict name resolution (Phase 2).
//!
//! Every identifier reference in a value or type position must resolve to a
//! local binding, a module-level declaration, an imported name, a built-in
//! type/enum/error, a prelude constant, or a standard-library namespace.
//! Anything else is an "unresolved name" error.
//!
//! Member access is intentionally shallow: `fs.list` resolves because `fs` is a
//! known namespace, but the member `list` is not checked (the standard library
//! defines it, Phase 8). `report.relevant_files` resolves because `report` is a
//! binding. This catches typos and undeclared names without a type-inference
//! engine, which remains future work (see ADR-0012).

use std::collections::BTreeSet;

use axiom_syntax::ast::*;
use axiom_syntax::diagnostic::Diagnostic;

use crate::graph::Module;

/// Standard-library namespaces (`agents.md` §5.1 / §14).
pub const NAMESPACES: &[&str] = &[
    "core",
    "collections",
    "json",
    "time",
    "env",
    "secrets",
    "fs",
    "workspaces",
    "artifacts",
    "patches",
    "commands",
    "db",
    "agents",
    "workflows",
    "trace",
    "ui",
    "api",
    "browser",
];

/// Built-in types, enums, and error names (`agents.md` §5.3, §5.6, §8, §10).
pub const BUILTIN_TYPES: &[&str] = &[
    // primitives
    "string",
    "bool",
    "int",
    "float",
    "bytes",
    "duration",
    "timestamp",
    "null",
    // containers / core
    "JsonValue",
    "Option",
    "Result",
    "Map",
    "Agent",
    // workspaces and artifacts
    "WorkspaceRoot",
    "WorkspaceRef",
    "WorkspaceIsolation",
    "DirEntry",
    "SearchResult",
    "CommandResult",
    "ConflictReport",
    "IntegrationResult",
    "ArtifactRef",
    // contracts / iteration
    "CheckKind",
    "DecompositionStrategy",
    "ValidationResult",
    "ValidationFinding",
    "ValidationContext",
    "Validator",
    "IterationOutcome",
    "IterationAttempt",
    "InvokeOptions",
    // frontend / browser
    "UiTree",
    "UiError",
    "Capability",
    "AuthenticationRequirement",
    // databases
    "DbAccess",
    "DataClass",
    "WriteResult",
    "MigrationPlan",
    "MigrationResult",
    "ScratchDb",
    // error kinds
    "ToolError",
    "ModelOutputInvalid",
    "PolicyViolation",
    "ProviderError",
    "VerificationFailed",
    "Timeout",
    "Cancelled",
    "Escalated",
    "IterationExhausted",
];

/// Names available in every scope without a declaration.
pub const BUILTIN_VALUES: &[&str] = &[
    // prelude constants
    "project", // WorkspaceRoot (§8.2)
    // contract context bindings (§5.6)
    "output",
    "value",
    // iteration context (§7.1)
    "iteration",
    // built-in constructors
    "capability",
    "Ok",
    "Err",
    "Some",
    "None",
];

fn is_builtin(name: &str) -> bool {
    NAMESPACES.contains(&name) || BUILTIN_TYPES.contains(&name) || BUILTIN_VALUES.contains(&name)
}

/// Resolve all value and type references in a single module.
pub fn resolve_module(module: &Module, out: &mut Vec<Diagnostic>) {
    let mut resolver = Resolver::new(module, out);
    for item in &module.file.items {
        resolver.item(item);
    }
}

struct Resolver<'a> {
    scopes: Vec<BTreeSet<String>>,
    out: &'a mut Vec<Diagnostic>,
}

impl<'a> Resolver<'a> {
    fn new(module: &Module, out: &'a mut Vec<Diagnostic>) -> Resolver<'a> {
        let mut root = BTreeSet::new();
        for symbol in module.symbols.iter() {
            root.insert(symbol.name.clone());
        }
        for import in &module.imports {
            for name in &import.decl.names {
                root.insert(name.text.clone());
            }
        }
        Resolver {
            scopes: vec![root],
            out,
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(BTreeSet::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string());
        }
    }

    fn is_declared(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }

    fn resolves(&self, name: &str) -> bool {
        self.is_declared(name) || is_builtin(name)
    }

    // --- items -----------------------------------------------------------

    fn item(&mut self, item: &Item) {
        match &item.kind {
            ItemKind::Type(decl) => {
                self.with_type_params(&decl.type_params, |this| this.type_expr(&decl.value));
            }
            ItemKind::Interface(decl) => {
                self.with_type_params(&decl.type_params, |this| {
                    for property in &decl.properties {
                        this.type_expr(&property.ty);
                    }
                });
            }
            ItemKind::Enum(decl) => {
                for member in &decl.members {
                    if let Some(value) = &member.value {
                        self.expr(value);
                    }
                }
            }
            ItemKind::Const(decl) => {
                if let Some(ty) = &decl.ty {
                    self.type_expr(ty);
                }
                self.expr(&decl.value);
            }
            ItemKind::Function(decl) => {
                self.with_type_params(&decl.type_params, |this| {
                    this.push_scope();
                    this.params(&decl.params);
                    if let Some(ret) = &decl.ret {
                        this.type_expr(ret);
                    }
                    this.stmts(&decl.body);
                    this.pop_scope();
                });
            }
            ItemKind::Agent(decl) => {
                self.type_expr(&decl.implements);
                self.members(&decl.members, false);
            }
            ItemKind::Tool(decl) => {
                self.push_scope();
                self.params(&decl.params);
                self.type_expr(&decl.ret);
                self.members(&decl.members, false);
                self.pop_scope();
            }
            ItemKind::Query(decl) => {
                self.push_scope();
                self.params(&decl.params);
                self.type_expr(&decl.ret);
                self.members(&decl.members, false);
                self.pop_scope();
            }
            ItemKind::Operation(decl) => {
                self.push_scope();
                self.params(&decl.params);
                self.type_expr(&decl.ret);
                // `handler:` names a function bound by the server target, which may
                // live in another module; it is not a local value reference.
                for member in &decl.members {
                    if member.name.text == "handler" {
                        continue;
                    }
                    match &member.value {
                        MemberValue::Expr(expr) | MemberValue::Arrow(expr) => self.expr(expr),
                        MemberValue::Block(inner) => self.members(inner, false),
                    }
                }
                self.pop_scope();
            }
            ItemKind::Goal(decl) => {
                self.push_scope();
                self.params(&decl.params);
                self.type_expr(&decl.ret);
                self.members(&decl.members, false);
                self.pop_scope();
            }
            ItemKind::Workflow(decl) => {
                self.push_scope();
                self.params(&decl.params);
                if let Some(ret) = &decl.ret {
                    self.type_expr(ret);
                }
                self.stmts(&decl.body);
                self.pop_scope();
            }
            ItemKind::MemberBlock(decl) => self.members(&decl.members, false),
        }
    }

    fn with_type_params(&mut self, params: &[Ident], body: impl FnOnce(&mut Self)) {
        self.push_scope();
        for param in params {
            self.declare(&param.text);
        }
        body(self);
        self.pop_scope();
    }

    fn params(&mut self, params: &[Param]) {
        for param in params {
            if let Some(ty) = &param.ty {
                self.type_expr(ty);
            }
            if let Some(default) = &param.default {
                self.expr(default);
            }
            self.declare(&param.name.text);
        }
    }

    /// Resolve a member block. `routing` blocks treat entry names as type refs.
    fn members(&mut self, members: &[Member], routing: bool) {
        for member in members {
            if routing {
                self.type_name(&member.name);
            }
            match &member.value {
                MemberValue::Expr(expr) | MemberValue::Arrow(expr) => self.expr(expr),
                MemberValue::Block(inner) => {
                    let nested_routing = member.name.text == "routing";
                    self.members(inner, nested_routing);
                }
            }
        }
    }

    // --- statements ------------------------------------------------------

    fn stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Const(decl) => {
                if let Some(ty) = &decl.ty {
                    self.type_expr(ty);
                }
                self.expr(&decl.value);
                self.declare(&decl.name.text);
            }
            StmtKind::Assignment { target, value, .. } => {
                self.expr(target);
                self.expr(value);
            }
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                self.expr(cond);
                self.scoped_stmts(then_body);
                self.scoped_stmts(else_body);
            }
            StmtKind::Match { scrutinee, arms } => {
                self.expr(scrutinee);
                for arm in arms {
                    self.push_scope();
                    self.pattern(&arm.pattern);
                    match &arm.body {
                        MatchBody::Expr(expr) => self.expr(expr),
                        MatchBody::Block(body) => self.stmts(body),
                    }
                    self.pop_scope();
                }
            }
            StmtKind::For { var, iter, body } => {
                self.expr(iter);
                self.push_scope();
                self.declare(&var.text);
                self.stmts(body);
                self.pop_scope();
            }
            StmtKind::While { cond, max, body } => {
                self.expr(cond);
                if let Some(max) = max {
                    self.expr(max);
                }
                self.scoped_stmts(body);
            }
            StmtKind::Return(Some(expr))
            | StmtKind::Yield(Some(expr))
            | StmtKind::Throw(expr)
            | StmtKind::Emit(expr)
            | StmtKind::Checkpoint(Some(expr)) => self.expr(expr),
            StmtKind::Try {
                body,
                catches,
                finally,
            } => {
                self.scoped_stmts(body);
                for catch in catches {
                    self.push_scope();
                    if let Some(binding) = &catch.binding {
                        self.declare(&binding.text);
                    }
                    self.stmts(&catch.body);
                    self.pop_scope();
                }
                self.scoped_stmts(finally);
            }
            StmtKind::Require {
                cond,
                else_expr,
                escalate,
            } => {
                self.expr(cond);
                if let Some(expr) = else_expr {
                    self.expr(expr);
                }
                if let Some(expr) = escalate {
                    self.expr(expr);
                }
            }
            StmtKind::Parallel {
                var,
                iter,
                max,
                body,
            } => {
                self.expr(iter);
                if let Some(max) = max {
                    self.expr(max);
                }
                self.push_scope();
                self.declare(&var.text);
                self.stmts(body);
                self.pop_scope();
            }
            StmtKind::Retry { attempts, body } => {
                self.expr(attempts);
                self.scoped_stmts(body);
            }
            StmtKind::Timeout { duration, body } => {
                self.expr(duration);
                self.scoped_stmts(body);
            }
            StmtKind::OnError { param, body } => {
                self.push_scope();
                self.params(std::slice::from_ref(param));
                self.stmts(body);
                self.pop_scope();
            }
            StmtKind::Expr(expr) => self.expr(expr),
            StmtKind::Item(item) => self.item(item),
            StmtKind::Return(None)
            | StmtKind::Yield(None)
            | StmtKind::Break
            | StmtKind::Continue
            | StmtKind::Checkpoint(None) => {}
        }
    }

    fn scoped_stmts(&mut self, stmts: &[Stmt]) {
        self.push_scope();
        self.stmts(stmts);
        self.pop_scope();
    }

    fn pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Binding(ident) => self.declare(&ident.text),
            Pattern::Tuple(parts, _) => {
                for part in parts {
                    self.pattern(part);
                }
            }
            Pattern::Path(path, _) => {
                if let Some(first) = path.first() {
                    if !self.resolves(&first.text) {
                        self.out.push(Diagnostic::error(
                            first.span,
                            format!("cannot resolve name `{}`", first.text),
                        ));
                    }
                }
            }
            Pattern::Wildcard(_) | Pattern::Literal(_, _) => {}
        }
    }

    // --- expressions -----------------------------------------------------

    fn expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Literal(_) => {}
            ExprKind::Path(path) => self.value_path(path),
            ExprKind::List(items) => {
                for item in items {
                    self.expr(item);
                }
            }
            ExprKind::Object(entries) => {
                for entry in entries {
                    self.expr(&entry.value);
                }
            }
            ExprKind::Lambda { params, body } => {
                self.push_scope();
                self.params(params);
                match body {
                    LambdaBody::Expr(expr) => self.expr(expr),
                    LambdaBody::Block(body) => self.stmts(body),
                }
                self.pop_scope();
            }
            ExprKind::Await(inner) => self.expr(inner),
            ExprKind::Delegate {
                agent,
                method,
                args,
            } => {
                self.value_path(agent);
                if method.text != "execute" {
                    self.out.push(
                        Diagnostic::error(
                            method.span,
                            format!("`{}` is not an agent entry point", method.text),
                        )
                        .with_note("the only Agent entry point in 1.0 is `execute`"),
                    );
                }
                for arg in args {
                    self.expr(&arg.value);
                }
            }
            ExprKind::Parallel {
                var,
                iter,
                max,
                body,
            } => {
                self.expr(iter);
                if let Some(max) = max {
                    self.expr(max);
                }
                self.push_scope();
                self.declare(&var.text);
                self.stmts(body);
                self.pop_scope();
            }
            ExprKind::Iterate { call, members } => {
                self.expr(call);
                // `validate:` names registered validators, resolved against the
                // validator registry, not module values (Phase 8).
                for member in members {
                    if member.name.text == "validate" {
                        continue;
                    }
                    match &member.value {
                        MemberValue::Expr(expr) | MemberValue::Arrow(expr) => self.expr(expr),
                        MemberValue::Block(inner) => self.members(inner, false),
                    }
                }
            }
            ExprKind::Unary { operand, .. } => self.expr(operand),
            ExprKind::Binary { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            ExprKind::Conditional {
                cond,
                then_expr,
                else_expr,
            } => {
                self.expr(cond);
                self.expr(then_expr);
                self.expr(else_expr);
            }
            ExprKind::Member { base, .. } => self.expr(base),
            ExprKind::Index { base, index } => {
                self.expr(base);
                self.expr(index);
            }
            ExprKind::Call { callee, args } => {
                self.expr(callee);
                for arg in args {
                    self.expr(&arg.value);
                }
            }
        }
    }

    /// Resolve a value path; only the leading segment is checked.
    fn value_path(&mut self, path: &[Ident]) {
        if let Some(first) = path.first() {
            if !self.resolves(&first.text) {
                self.out.push(Diagnostic::error(
                    first.span,
                    format!("cannot resolve name `{}`", first.text),
                ));
            }
        }
    }

    fn type_name(&mut self, name: &Ident) {
        if !self.resolves(&name.text) {
            self.out.push(Diagnostic::error(
                name.span,
                format!("cannot resolve type `{}`", name.text),
            ));
        }
    }

    fn type_expr(&mut self, ty: &TypeExpr) {
        match &ty.kind {
            TypeExprKind::Ref { path, args } => {
                if let Some(first) = path.first() {
                    // `pkg.Name` and enum members (`WorkspaceIsolation.X`) resolve
                    // on the leading segment.
                    if !self.resolves(&first.text) {
                        self.out.push(Diagnostic::error(
                            first.span,
                            format!("cannot resolve type `{}`", first.text),
                        ));
                    }
                }
                for arg in args {
                    self.type_expr(arg);
                }
            }
            TypeExprKind::Array(inner) => self.type_expr(inner),
            TypeExprKind::Union(parts) | TypeExprKind::Tuple(parts) => {
                for part in parts {
                    self.type_expr(part);
                }
            }
            TypeExprKind::Record(fields) => {
                for field in fields {
                    self.type_expr(&field.ty);
                }
            }
            TypeExprKind::Str(_) => {}
        }
    }
}
