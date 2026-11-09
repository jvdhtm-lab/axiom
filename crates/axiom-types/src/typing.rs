//! Expression type inference and match exhaustiveness.
//!
//! This pass is deliberately conservative: unknown standard-library members and
//! unresolved names become [`Ty::Unknown`] and never produce a cascade of type
//! errors. Known records, enums, lists, `Option`, `Result`, user functions, and
//! imported types are checked.
//!
//! Diagnostic codes:
//!
//! - `AX0401` type mismatch
//! - `AX0402` condition/operand type error
//! - `AX0403` unknown field or record member
//! - `AX0404` expected a list
//! - `AX0405` list index must be an integer
//! - `AX0406` argument count/name mismatch
//! - `AX0407` argument type mismatch
//! - `AX0408` non-exhaustive match
//! - `AX0409` unknown or payload-bearing enum pattern
//! - `AX0410` duplicate match arm
//! - `AX0411` return type mismatch
//! - `AX0412` pattern type mismatch

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use axiom_syntax::ast::{
    DeclKind, EnumDecl, Expr, ExprKind, FunctionDecl, MatchArm, Pattern, StmtKind, TypeExpr,
    TypeKind,
};
use axiom_syntax::diagnostic::Diagnostic;

use crate::analyze::BUILTIN_TYPES;
use crate::check::ModuleDiagnostic;
use crate::graph::{Module, ModuleId, Project};

/// A conservative internal type representation.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Ty {
    Unknown,
    Int,
    Float,
    String,
    Bool,
    Bytes,
    Duration,
    Timestamp,
    List(Box<Ty>),
    Record(BTreeMap<String, Ty>),
    Named(String, Vec<Ty>),
}

impl Ty {
    fn option(inner: Ty) -> Ty {
        Ty::Named("Option".to_string(), vec![inner])
    }

    fn result(ok: Ty, err: Ty) -> Ty {
        Ty::Named("Result".to_string(), vec![ok, err])
    }

    fn is_unknown(&self) -> bool {
        matches!(self, Ty::Unknown)
    }

    fn is_numeric(&self) -> bool {
        matches!(self, Ty::Int | Ty::Float)
    }
}

fn primitive(name: &str) -> Option<Ty> {
    Some(match name {
        "int" => Ty::Int,
        "float" => Ty::Float,
        "string" => Ty::String,
        "bool" => Ty::Bool,
        "bytes" => Ty::Bytes,
        "duration" => Ty::Duration,
        "timestamp" => Ty::Timestamp,
        _ => return None,
    })
}

fn display(ty: &Ty) -> String {
    match ty {
        Ty::Unknown => "unknown".to_string(),
        Ty::Int => "int".to_string(),
        Ty::Float => "float".to_string(),
        Ty::String => "string".to_string(),
        Ty::Bool => "bool".to_string(),
        Ty::Bytes => "bytes".to_string(),
        Ty::Duration => "duration".to_string(),
        Ty::Timestamp => "timestamp".to_string(),
        Ty::List(inner) => format!("{}[]", display(inner)),
        Ty::Record(fields) => {
            let mut out = String::from("{ ");
            for (name, ty) in fields {
                out.push_str(name);
                out.push_str(": ");
                out.push_str(&display(ty));
                out.push_str("; ");
            }
            out.push('}');
            out
        }
        Ty::Named(name, args) if args.is_empty() => name.clone(),
        Ty::Named(name, args) => {
            let args: Vec<_> = args.iter().map(display).collect();
            format!("{name}<{}>", args.join(", "))
        }
    }
}

fn same(a: &Ty, b: &Ty) -> bool {
    if a.is_unknown() || b.is_unknown() {
        return true;
    }
    match (a, b) {
        (Ty::Int, Ty::Int)
        | (Ty::Float, Ty::Float)
        | (Ty::String, Ty::String)
        | (Ty::Bool, Ty::Bool)
        | (Ty::Bytes, Ty::Bytes)
        | (Ty::Duration, Ty::Duration)
        | (Ty::Timestamp, Ty::Timestamp) => true,
        (Ty::List(a), Ty::List(b)) => same(a, b),
        (Ty::Record(a), Ty::Record(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(name, ty)| b.get(name).is_some_and(|other| same(ty, other)))
        }
        (Ty::Named(a, aa), Ty::Named(b, ba)) => {
            a == b && aa.len() == ba.len() && aa.iter().zip(ba).all(|(a, b)| same(a, b))
        }
        _ => false,
    }
}

fn unify(a: &Ty, b: &Ty) -> Option<Ty> {
    if a.is_unknown() {
        return Some(b.clone());
    }
    if b.is_unknown() {
        return Some(a.clone());
    }
    if same(a, b) {
        return Some(a.clone());
    }
    match (a, b) {
        (Ty::List(a), Ty::List(b)) => Some(Ty::List(Box::new(unify(a, b)?))),
        _ => None,
    }
}

#[derive(Debug, Clone)]
enum RawType<'a> {
    Alias {
        ty: &'a TypeExpr,
        generics: Vec<String>,
    },
    Enum {
        variants: Vec<String>,
    },
}

#[derive(Debug, Clone)]
struct TypeInfo {
    ty: Ty,
}

#[derive(Debug, Clone)]
struct FnSig {
    params: Vec<(String, Ty)>,
    ret: Ty,
}

impl FnSig {
    fn param(&self, name: &str) -> Option<&Ty> {
        self.params
            .iter()
            .find(|(param, _)| param == name)
            .map(|(_, ty)| ty)
    }
}

/// Resolves declared type expressions and user/imported type names.
struct TypeResolver<'a> {
    modules: &'a [Module],
    raw: Vec<BTreeMap<String, RawType<'a>>>,
    imported: Vec<BTreeMap<String, (ModuleId, String)>>,
    cache: RefCell<Vec<BTreeMap<String, TypeInfo>>>,
    enums: RefCell<BTreeMap<String, Vec<String>>>,
    visiting: RefCell<BTreeSet<(ModuleId, String)>>,
}

impl<'a> TypeResolver<'a> {
    fn new(project: &'a Project) -> Self {
        let mut raw = Vec::with_capacity(project.graph.modules.len());
        let mut imported = Vec::with_capacity(project.graph.modules.len());
        for module in &project.graph.modules {
            let mut local = BTreeMap::new();
            for decl in &module.file.declarations {
                match &decl.kind {
                    DeclKind::Type(t) => {
                        local.insert(
                            t.name.clone(),
                            RawType::Alias {
                                ty: &t.ty,
                                generics: t.generics.clone(),
                            },
                        );
                    }
                    DeclKind::Enum(e) => {
                        local.insert(
                            e.name.clone(),
                            RawType::Enum {
                                variants: variant_names(e),
                            },
                        );
                    }
                    _ => {}
                }
            }
            raw.push(local);

            let mut imports = BTreeMap::new();
            for import in &module.imports {
                let Some(target) = import.target else {
                    continue;
                };
                for name in &import.decl.names {
                    imports.insert(name.clone(), (target, name.clone()));
                }
            }
            imported.push(imports);
        }
        TypeResolver {
            modules: &project.graph.modules,
            raw,
            imported,
            cache: RefCell::new(vec![BTreeMap::new(); project.graph.modules.len()]),
            enums: RefCell::new(BTreeMap::new()),
            visiting: RefCell::new(BTreeSet::new()),
        }
    }

    fn module_path(&self, module: ModuleId) -> String {
        self.modules[module]
            .file
            .module
            .as_ref()
            .map(|m| m.dotted())
            .unwrap_or_else(|| format!("module{module}"))
    }

    fn qualify(&self, module: ModuleId, name: &str) -> String {
        format!("{}::{name}", self.module_path(module))
    }

    fn module_by_path(&self, path: &str) -> Option<ModuleId> {
        self.modules
            .iter()
            .find(|m| m.module_path.as_deref() == Some(path))
            .map(|m| m.id)
    }

    fn info(&self, module: ModuleId, name: &str) -> Option<TypeInfo> {
        if let Some(info) = self.cache.borrow()[module].get(name).cloned() {
            return Some(info);
        }
        let raw = self.raw.get(module)?.get(name)?.clone();
        let key = (module, name.to_string());
        if !self.visiting.borrow_mut().insert(key.clone()) {
            return Some(TypeInfo {
                ty: Ty::Named(self.qualify(module, name), Vec::new()),
            });
        }
        let info = match raw {
            RawType::Enum { variants } => {
                let qualified = self.qualify(module, name);
                self.enums
                    .borrow_mut()
                    .insert(qualified.clone(), variants.clone());
                TypeInfo {
                    ty: Ty::Named(qualified, Vec::new()),
                }
            }
            RawType::Alias { ty, generics } => TypeInfo {
                ty: self.resolve_type(module, ty, &generics),
            },
        };
        self.visiting.borrow_mut().remove(&key);
        self.cache
            .borrow_mut()
            .get_mut(module)
            .expect("module cache")
            .insert(name.to_string(), info.clone());
        Some(info)
    }

    fn resolve_type(&self, module: ModuleId, ty: &TypeExpr, generics: &[String]) -> Ty {
        match &ty.kind {
            TypeKind::List(inner) => Ty::List(Box::new(self.resolve_type(module, inner, generics))),
            TypeKind::Record(fields) => Ty::Record(
                fields
                    .iter()
                    .map(|field| {
                        (
                            field.name.clone(),
                            self.resolve_type(module, &field.ty, generics),
                        )
                    })
                    .collect(),
            ),
            TypeKind::Named { path, args } => {
                let args: Vec<Ty> = args
                    .iter()
                    .map(|arg| self.resolve_type(module, arg, generics))
                    .collect();
                if path.len() == 1 {
                    let name = &path[0];
                    if generics.iter().any(|generic| generic == name) {
                        return Ty::Named(name.clone(), args);
                    }
                    if let Some(ty) = primitive(name) {
                        return ty;
                    }
                    if BUILTIN_TYPES.contains(&name.as_str()) {
                        return Ty::Named(name.clone(), args);
                    }
                    if self.raw[module].contains_key(name) {
                        if let Some(info) = self.info(module, name) {
                            return info.ty;
                        }
                    }
                    if let Some((target, target_name)) = self.imported[module].get(name) {
                        if let Some(info) = self.info(*target, target_name) {
                            return info.ty;
                        }
                    }
                    Ty::Named(name.clone(), args)
                } else {
                    let prefix = path[..path.len() - 1].join(".");
                    let last = &path[path.len() - 1];
                    if let Some(target) = self.module_by_path(&prefix) {
                        if let Some(info) = self.info(target, last) {
                            return info.ty;
                        }
                    }
                    Ty::Named(path.join("."), args)
                }
            }
        }
    }

    fn lookup_type_name(&self, module: ModuleId, name: &str) -> Option<Ty> {
        if let Some(ty) = primitive(name) {
            return Some(ty);
        }
        if BUILTIN_TYPES.contains(&name) {
            return Some(Ty::Named(name.to_string(), Vec::new()));
        }
        if self.raw[module].contains_key(name) {
            return self.info(module, name).map(|info| info.ty);
        }
        if let Some((target, target_name)) = self.imported[module].get(name) {
            return self.info(*target, target_name).map(|info| info.ty);
        }
        None
    }

    fn variants(&self, ty: &Ty) -> Option<Vec<String>> {
        match ty {
            Ty::Bool => Some(vec!["true".to_string(), "false".to_string()]),
            Ty::Named(name, _) if name == "Option" => {
                Some(vec!["Some".to_string(), "None".to_string()])
            }
            Ty::Named(name, _) if name == "Result" => {
                Some(vec!["Ok".to_string(), "Err".to_string()])
            }
            Ty::Named(name, _) => self.enums.borrow().get(name).cloned(),
            _ => None,
        }
    }

    fn payload(&self, ty: &Ty, variant: &str) -> Option<Ty> {
        match (ty, variant) {
            (Ty::Named(name, args), "Some") if name == "Option" => args.first().cloned(),
            (Ty::Named(name, args), "Ok") if name == "Result" => args.first().cloned(),
            (Ty::Named(name, args), "Err") if name == "Result" => args.get(1).cloned(),
            _ => None,
        }
    }
}

fn variant_names(decl: &EnumDecl) -> Vec<String> {
    decl.variants.iter().map(|v| v.name.clone()).collect()
}

/// Run expression inference and match exhaustiveness for every function.
pub(crate) fn analyze_types(project: &Project, out: &mut Vec<ModuleDiagnostic>) {
    let types = TypeResolver::new(project);
    let functions = build_functions(project, &types);

    for module in &project.graph.modules {
        for decl in &module.file.declarations {
            if let DeclKind::Function(f) = &decl.kind {
                let mut checker = Checker {
                    module: module.id,
                    types: &types,
                    functions: &functions[module.id],
                    f,
                    scopes: vec![BTreeMap::new()],
                    expected_return: f
                        .ret
                        .as_ref()
                        .map(|ty| types.resolve_type(module.id, ty, &f.generics))
                        .unwrap_or(Ty::Unknown),
                    out,
                };
                checker.check();
            }
        }
    }
}

fn build_functions(project: &Project, types: &TypeResolver<'_>) -> Vec<BTreeMap<String, FnSig>> {
    let mut local = vec![BTreeMap::new(); project.graph.modules.len()];
    for module in &project.graph.modules {
        for decl in &module.file.declarations {
            if let DeclKind::Function(f) = &decl.kind {
                let params = f
                    .params
                    .iter()
                    .map(|param| {
                        (
                            param.name.clone(),
                            types.resolve_type(module.id, &param.ty, &f.generics),
                        )
                    })
                    .collect();
                let ret = f
                    .ret
                    .as_ref()
                    .map(|ty| types.resolve_type(module.id, ty, &f.generics))
                    .unwrap_or(Ty::Unknown);
                local[module.id].insert(f.name.clone(), FnSig { params, ret });
            }
        }
    }

    let mut merged = local.clone();
    for module in &project.graph.modules {
        for import in &module.imports {
            let Some(target) = import.target else {
                continue;
            };
            for name in &import.decl.names {
                if let Some(sig) = local[target].get(name) {
                    merged[module.id]
                        .entry(name.clone())
                        .or_insert_with(|| sig.clone());
                }
            }
        }
    }
    merged
}

struct Checker<'a> {
    module: ModuleId,
    types: &'a TypeResolver<'a>,
    functions: &'a BTreeMap<String, FnSig>,
    f: &'a FunctionDecl,
    scopes: Vec<BTreeMap<String, Ty>>,
    expected_return: Ty,
    out: &'a mut Vec<ModuleDiagnostic>,
}

impl<'a> Checker<'a> {
    fn check(&mut self) {
        for param in &self.f.params {
            let ty = self
                .types
                .resolve_type(self.module, &param.ty, &self.f.generics);
            self.scopes
                .last_mut()
                .expect("scope")
                .insert(param.name.clone(), ty);
        }
        let body = self.f.body.clone();
        let expected = self.expected_return.clone();
        self.check_block(&body, Some(&expected));
    }

    fn error(&mut self, span: axiom_syntax::span::Span, code: &'static str, message: String) {
        self.out.push(ModuleDiagnostic {
            module: Some(self.module),
            diagnostic: Diagnostic::error_code(span, code, message),
        });
    }

    fn lookup(&self, name: &str) -> Option<Ty> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty.clone());
            }
        }
        None
    }

    fn bind(&mut self, name: String, ty: Ty) {
        self.scopes.last_mut().expect("scope").insert(name, ty);
    }

    fn check_block(&mut self, block: &axiom_syntax::ast::Block, expected: Option<&Ty>) -> Ty {
        self.scopes.push(BTreeMap::new());
        let mut last = Ty::Unknown;
        for statement in &block.stmts {
            if let Some(ty) = self.check_stmt(statement, expected) {
                last = ty;
            }
        }
        self.scopes.pop();
        last
    }

    fn check_stmt(
        &mut self,
        statement: &axiom_syntax::ast::Stmt,
        expected: Option<&Ty>,
    ) -> Option<Ty> {
        match &statement.kind {
            StmtKind::Let { name, ty, value } => {
                let annotation = ty
                    .as_ref()
                    .map(|ty| self.types.resolve_type(self.module, ty, &self.f.generics));
                let actual = self.infer(value, annotation.as_ref());
                if let Some(annotation) = &annotation {
                    if !same(annotation, &actual) {
                        self.error(
                            value.span,
                            "AX0401",
                            format!(
                                "cannot initialize `{name}` as `{}`; expected `{}`",
                                display(&actual),
                                display(annotation)
                            ),
                        );
                    }
                }
                self.bind(name.clone(), annotation.unwrap_or(actual));
                None
            }
            StmtKind::For { var, iter, body } => {
                let iter_ty = self.infer(iter, None);
                let element = match iter_ty {
                    Ty::List(element) => *element,
                    Ty::Unknown => Ty::Unknown,
                    other => {
                        self.error(
                            iter.span,
                            "AX0404",
                            format!("`for` needs a list, found `{}`", display(&other)),
                        );
                        Ty::Unknown
                    }
                };
                self.scopes.push(BTreeMap::new());
                self.bind(var.clone(), element);
                self.check_block(body, None);
                self.scopes.pop();
                None
            }
            StmtKind::Return(value) => {
                let expected = self.expected_return.clone();
                match value {
                    Some(value) => {
                        let actual = self.infer(value, Some(&expected));
                        if !same(&expected, &actual) {
                            self.error(
                                value.span,
                                "AX0411",
                                format!(
                                    "function `{}` returns `{}`, found `{}`",
                                    self.f.name,
                                    display(&expected),
                                    display(&actual)
                                ),
                            );
                        }
                    }
                    None => {
                        if !expected.is_unknown() {
                            self.error(
                                statement.span,
                                "AX0411",
                                format!(
                                    "function `{}` must return a value of type `{}`",
                                    self.f.name,
                                    display(&expected)
                                ),
                            );
                        }
                    }
                }
                None
            }
            StmtKind::Expr(expr) => Some(self.infer(expr, expected)),
        }
    }

    fn infer(&mut self, expr: &Expr, expected: Option<&Ty>) -> Ty {
        match &expr.kind {
            ExprKind::Int(_) => Ty::Int,
            ExprKind::Float(_) => Ty::Float,
            ExprKind::Str(_) => Ty::String,
            ExprKind::Bool(_) => Ty::Bool,
            ExprKind::Duration { .. } => Ty::Duration,
            ExprKind::Size { .. } => Ty::Int,
            ExprKind::Ident(name) => {
                if let Some(ty) = self.lookup(name) {
                    return ty;
                }
                if name == "None" {
                    return Ty::option(Ty::Unknown);
                }
                if let Some(ty) = self.types.lookup_type_name(self.module, name) {
                    return ty;
                }
                Ty::Unknown
            }
            ExprKind::List(items) => {
                let expected_element = match expected {
                    Some(Ty::List(inner)) => Some(inner.as_ref().clone()),
                    _ => None,
                };
                let mut element: Option<Ty> = expected_element;
                for item in items {
                    let ty = self.infer(&item.value, element.as_ref());
                    element = match element {
                        Some(existing) => {
                            if !same(&existing, &ty) {
                                self.error(
                                    item.value.span,
                                    "AX0401",
                                    format!(
                                        "list element has type `{}`; expected `{}`",
                                        display(&ty),
                                        display(&existing)
                                    ),
                                );
                            }
                            Some(unify(&existing, &ty).unwrap_or(existing))
                        }
                        None => Some(ty),
                    };
                }
                Ty::List(Box::new(element.unwrap_or(Ty::Unknown)))
            }
            ExprKind::Record(members) => {
                let expected_fields = match expected {
                    Some(Ty::Record(fields)) => Some(fields.clone()),
                    _ => None,
                };
                let mut inferred = BTreeMap::new();
                for member in members {
                    let expected_field = expected_fields
                        .as_ref()
                        .and_then(|fields| fields.get(&member.name))
                        .cloned();
                    let ty = self.infer(&member.value, expected_field.as_ref());
                    if let Some(expected_field) = &expected_field {
                        if !same(expected_field, &ty) {
                            self.error(
                                member.value.span,
                                "AX0401",
                                format!(
                                    "field `{}` has type `{}`; expected `{}`",
                                    member.name,
                                    display(&ty),
                                    display(expected_field)
                                ),
                            );
                        }
                    } else if let Some(fields) = &expected_fields {
                        if !fields.contains_key(&member.name) {
                            self.error(
                                member.span,
                                "AX0403",
                                format!("unknown field `{}`", member.name),
                            );
                        }
                    }
                    inferred.insert(member.name.clone(), ty);
                }
                if let Some(fields) = expected_fields {
                    for name in fields.keys() {
                        if !inferred.contains_key(name) {
                            self.error(expr.span, "AX0401", format!("missing field `{name}`"));
                        }
                    }
                    Ty::Record(fields)
                } else {
                    Ty::Record(inferred)
                }
            }
            ExprKind::Field(base, name) => self.infer_field(base, name),
            ExprKind::Index(base, index) => {
                let base_ty = self.infer(base, None);
                let index_ty = self.infer(index, Some(&Ty::Int));
                if !same(&Ty::Int, &index_ty) {
                    self.error(
                        index.span,
                        "AX0405",
                        format!("list index must be `int`, found `{}`", display(&index_ty)),
                    );
                }
                match base_ty {
                    Ty::List(element) => *element,
                    Ty::Unknown => Ty::Unknown,
                    other => {
                        self.error(
                            base.span,
                            "AX0404",
                            format!("cannot index `{}`; expected a list", display(&other)),
                        );
                        Ty::Unknown
                    }
                }
            }
            ExprKind::Call {
                callee,
                type_args,
                args,
            } => self.infer_call(callee, type_args, args, expected),
            ExprKind::Unary { op, operand } => {
                let operand_ty = self.infer(operand, None);
                match op {
                    axiom_syntax::ast::UnaryOp::Not => {
                        if !same(&Ty::Bool, &operand_ty) {
                            self.error(
                                operand.span,
                                "AX0402",
                                format!("`!` needs `bool`, found `{}`", display(&operand_ty)),
                            );
                        }
                        Ty::Bool
                    }
                    axiom_syntax::ast::UnaryOp::Neg => {
                        if !operand_ty.is_numeric() && !operand_ty.is_unknown() {
                            self.error(
                                operand.span,
                                "AX0402",
                                format!("`-` needs a number, found `{}`", display(&operand_ty)),
                            );
                        }
                        operand_ty
                    }
                }
            }
            ExprKind::Binary { op, lhs, rhs } => self.infer_binary(*op, lhs, rhs),
            ExprKind::Await(inner) => self.infer(inner, expected),
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                let cond_ty = self.infer(cond, Some(&Ty::Bool));
                if !same(&Ty::Bool, &cond_ty) {
                    self.error(
                        cond.span,
                        "AX0402",
                        format!(
                            "`if` condition must be `bool`, found `{}`",
                            display(&cond_ty)
                        ),
                    );
                }
                let then_ty = self.check_block(then, expected);
                let else_ty = otherwise
                    .as_ref()
                    .map(|expr| self.infer(expr, expected))
                    .unwrap_or(Ty::Unknown);
                if let Some(expected) = expected {
                    if !same(expected, &then_ty) {
                        self.error(
                            then.span,
                            "AX0401",
                            format!(
                                "`if` branch has type `{}`; expected `{}`",
                                display(&then_ty),
                                display(expected)
                            ),
                        );
                    }
                    if !same(expected, &else_ty) {
                        self.error(
                            otherwise.as_ref().map(|e| e.span).unwrap_or(expr.span),
                            "AX0401",
                            format!(
                                "`else` branch has type `{}`; expected `{}`",
                                display(&else_ty),
                                display(expected)
                            ),
                        );
                    }
                    return expected.clone();
                }
                unify(&then_ty, &else_ty).unwrap_or_else(|| {
                    if !then_ty.is_unknown() && !else_ty.is_unknown() {
                        self.error(
                            expr.span,
                            "AX0401",
                            format!(
                                "`if` branches have incompatible types `{}` and `{}`",
                                display(&then_ty),
                                display(&else_ty)
                            ),
                        );
                    }
                    Ty::Unknown
                })
            }
            ExprKind::Match { scrutinee, arms } => self.infer_match(scrutinee, arms, expected),
            ExprKind::Block(block) => self.check_block(block, expected),
        }
    }

    fn infer_field(&mut self, base: &Expr, name: &str) -> Ty {
        if let ExprKind::Ident(type_name) = &base.kind {
            if let Some(ty) = self.types.lookup_type_name(self.module, type_name) {
                if self
                    .types
                    .variants(&ty)
                    .is_some_and(|variants| variants.iter().any(|variant| variant == name))
                {
                    return ty;
                }
            }
        }
        let base_ty = self.infer(base, None);
        match &base_ty {
            Ty::Record(fields) => match fields.get(name) {
                Some(ty) => ty.clone(),
                None => {
                    self.error(
                        base.span,
                        "AX0403",
                        format!("`{}` has no field `{name}`", display(&base_ty)),
                    );
                    Ty::Unknown
                }
            },
            Ty::Named(_, _) => {
                if self
                    .types
                    .variants(&base_ty)
                    .is_some_and(|variants| variants.iter().any(|variant| variant == name))
                {
                    base_ty.clone()
                } else {
                    Ty::Unknown
                }
            }
            Ty::Unknown => Ty::Unknown,
            other => {
                self.error(
                    base.span,
                    "AX0403",
                    format!("`{}` has no field `{name}`", display(other)),
                );
                Ty::Unknown
            }
        }
    }

    fn infer_call(
        &mut self,
        callee: &Expr,
        type_args: &[TypeExpr],
        args: &[axiom_syntax::ast::Argument],
        expected: Option<&Ty>,
    ) -> Ty {
        match &callee.kind {
            ExprKind::Ident(name) => self.infer_named_call(name, args, expected, callee.span),
            ExprKind::Field(base, method) => self.infer_method_call(base, method, type_args, args),
            _ => {
                self.infer(callee, None);
                for arg in args {
                    self.infer(&arg.value, None);
                }
                Ty::Unknown
            }
        }
    }

    fn infer_named_call(
        &mut self,
        name: &str,
        args: &[axiom_syntax::ast::Argument],
        expected: Option<&Ty>,
        span: axiom_syntax::span::Span,
    ) -> Ty {
        match name {
            "Some" => {
                let expected_inner = match expected {
                    Some(Ty::Named(name, inner)) if name == "Option" => inner.first().cloned(),
                    _ => None,
                };
                let actual = args
                    .first()
                    .map(|arg| self.infer(&arg.value, expected_inner.as_ref()))
                    .unwrap_or(Ty::Unknown);
                Ty::option(actual)
            }
            "Ok" => {
                let expected_inner = match expected {
                    Some(Ty::Named(name, inner)) if name == "Result" => inner.first().cloned(),
                    _ => None,
                };
                let actual = args
                    .first()
                    .map(|arg| self.infer(&arg.value, expected_inner.as_ref()))
                    .unwrap_or(Ty::Unknown);
                Ty::result(actual, Ty::Unknown)
            }
            "Err" => {
                let expected_inner = match expected {
                    Some(Ty::Named(name, inner)) if name == "Result" => inner.get(1).cloned(),
                    _ => None,
                };
                let actual = args
                    .first()
                    .map(|arg| self.infer(&arg.value, expected_inner.as_ref()))
                    .unwrap_or(Ty::Unknown);
                Ty::result(Ty::Unknown, actual)
            }
            _ => {
                let Some(sig) = self.functions.get(name).cloned() else {
                    for arg in args {
                        self.infer(&arg.value, None);
                    }
                    return Ty::Unknown;
                };
                self.check_arguments(&sig, args, span);
                sig.ret
            }
        }
    }

    fn check_arguments(
        &mut self,
        sig: &FnSig,
        args: &[axiom_syntax::ast::Argument],
        span: axiom_syntax::span::Span,
    ) {
        if args.len() != sig.params.len() {
            self.error(
                span,
                "AX0406",
                format!(
                    "expected {} argument(s), found {}",
                    sig.params.len(),
                    args.len()
                ),
            );
        }
        let mut positional = 0usize;
        for arg in args {
            let expected = match &arg.name {
                Some(name) => match sig.param(name) {
                    Some(ty) => ty.clone(),
                    None => {
                        self.error(
                            arg.value.span,
                            "AX0406",
                            format!("unknown argument `{name}`"),
                        );
                        Ty::Unknown
                    }
                },
                None => {
                    let ty = sig
                        .params
                        .get(positional)
                        .map(|(_, ty)| ty.clone())
                        .unwrap_or(Ty::Unknown);
                    positional += 1;
                    ty
                }
            };
            let actual = self.infer(&arg.value, Some(&expected));
            if !same(&expected, &actual) {
                self.error(
                    arg.value.span,
                    "AX0407",
                    format!(
                        "argument has type `{}`; expected `{}`",
                        display(&actual),
                        display(&expected)
                    ),
                );
            }
        }
    }

    fn infer_method_call(
        &mut self,
        base: &Expr,
        method: &str,
        type_args: &[TypeExpr],
        args: &[axiom_syntax::ast::Argument],
    ) -> Ty {
        if method == "to_string" {
            self.infer(base, None);
            for arg in args {
                self.infer(&arg.value, None);
            }
            return Ty::String;
        }
        if method == "unwrap_or" {
            let base_ty = self.infer(base, None);
            let inner = match &base_ty {
                Ty::Named(name, args) if name == "Option" => args.first().cloned(),
                _ => None,
            };
            if let Some(inner) = &inner {
                if let Some(arg) = args.first() {
                    let actual = self.infer(&arg.value, Some(inner));
                    if !same(inner, &actual) {
                        self.error(
                            arg.value.span,
                            "AX0407",
                            format!(
                                "`unwrap_or` argument has type `{}`; expected `{}`",
                                display(&actual),
                                display(inner)
                            ),
                        );
                    }
                }
                return inner.clone();
            }
            for arg in args {
                self.infer(&arg.value, None);
            }
            return Ty::Unknown;
        }
        if method == "accepted_artifact" {
            self.infer(base, None);
            for arg in args {
                self.infer(&arg.value, None);
            }
            return Ty::result(Ty::Unknown, Ty::Unknown);
        }

        let namespace = match &base.kind {
            ExprKind::Ident(name) => Some(name.as_str()),
            _ => None,
        };
        match namespace {
            Some("ui") => {
                for arg in args {
                    self.infer(&arg.value, None);
                }
                Ty::Named("UiTree".to_string(), Vec::new())
            }
            Some("db") => {
                for arg in args {
                    self.infer(&arg.value, None);
                }
                let arg_ty = type_args
                    .first()
                    .map(|ty| self.types.resolve_type(self.module, ty, &self.f.generics))
                    .unwrap_or(Ty::Unknown);
                match method {
                    "query" => Ty::List(Box::new(arg_ty)),
                    "query_one" => Ty::result(arg_ty, Ty::Named("DbError".to_string(), Vec::new())),
                    "execute" => Ty::Named("WriteResult".to_string(), Vec::new()),
                    _ => Ty::Unknown,
                }
            }
            Some("agents") => {
                for arg in args {
                    self.infer(&arg.value, None);
                }
                if method == "iterate" {
                    Ty::Named("IterationOutcome".to_string(), vec![Ty::Unknown])
                } else {
                    Ty::Unknown
                }
            }
            Some("runtime") => {
                for arg in args {
                    self.infer(&arg.value, None);
                }
                if method == "parallel_map" {
                    Ty::List(Box::new(Ty::Unknown))
                } else {
                    Ty::Unknown
                }
            }
            _ => {
                self.infer(base, None);
                for arg in args {
                    self.infer(&arg.value, None);
                }
                Ty::Unknown
            }
        }
    }

    fn infer_binary(&mut self, op: axiom_syntax::ast::BinaryOp, lhs: &Expr, rhs: &Expr) -> Ty {
        use axiom_syntax::ast::BinaryOp;
        let left = self.infer(lhs, None);
        let right = self.infer(rhs, None);
        match op {
            BinaryOp::Add => {
                if same(&Ty::String, &left) || same(&Ty::String, &right) {
                    if !same(&Ty::String, &left) && !left.is_unknown() {
                        self.error(
                            lhs.span,
                            "AX0401",
                            format!("cannot add `{}` and `string`", display(&left)),
                        );
                    }
                    if !same(&Ty::String, &right) && !right.is_unknown() {
                        self.error(
                            rhs.span,
                            "AX0401",
                            format!("cannot add `string` and `{}`", display(&right)),
                        );
                    }
                    Ty::String
                } else {
                    self.numeric_binary("+", &left, &right, lhs, rhs)
                }
            }
            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                self.numeric_binary("arithmetic", &left, &right, lhs, rhs)
            }
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                self.numeric_binary("comparison", &left, &right, lhs, rhs);
                Ty::Bool
            }
            BinaryOp::Eq | BinaryOp::Ne => {
                if !same(&left, &right) {
                    self.error(
                        lhs.span,
                        "AX0401",
                        format!(
                            "cannot compare `{}` with `{}`",
                            display(&left),
                            display(&right)
                        ),
                    );
                }
                Ty::Bool
            }
            BinaryOp::And | BinaryOp::Or => {
                if !same(&Ty::Bool, &left) {
                    self.error(
                        lhs.span,
                        "AX0402",
                        format!("logical operator needs `bool`, found `{}`", display(&left)),
                    );
                }
                if !same(&Ty::Bool, &right) {
                    self.error(
                        rhs.span,
                        "AX0402",
                        format!("logical operator needs `bool`, found `{}`", display(&right)),
                    );
                }
                Ty::Bool
            }
        }
    }

    fn numeric_binary(&mut self, what: &str, left: &Ty, right: &Ty, lhs: &Expr, rhs: &Expr) -> Ty {
        if !left.is_numeric() && !left.is_unknown() {
            self.error(
                lhs.span,
                "AX0402",
                format!("{what} needs a number, found `{}`", display(left)),
            );
        }
        if !right.is_numeric() && !right.is_unknown() {
            self.error(
                rhs.span,
                "AX0402",
                format!("{what} needs a number, found `{}`", display(right)),
            );
        }
        if matches!(left, Ty::Float) || matches!(right, Ty::Float) {
            Ty::Float
        } else {
            Ty::Int
        }
    }

    fn infer_match(&mut self, scrutinee: &Expr, arms: &[MatchArm], expected: Option<&Ty>) -> Ty {
        let scrutinee_ty = self.infer(scrutinee, None);
        let variants = self.types.variants(&scrutinee_ty);
        let mut covered = BTreeSet::new();
        let mut wildcard = false;
        let mut result = Ty::Unknown;

        for arm in arms {
            self.scopes.push(BTreeMap::new());
            self.bind_pattern(
                &arm.pattern,
                arm.span,
                &scrutinee_ty,
                &mut covered,
                &mut wildcard,
            );
            let arm_ty = self.check_block(&arm.body, expected);
            self.scopes.pop();
            result = match unify(&result, &arm_ty) {
                Some(ty) => ty,
                None => {
                    if !result.is_unknown() && !arm_ty.is_unknown() {
                        self.error(
                            arm.span,
                            "AX0401",
                            format!(
                                "match arms have incompatible types `{}` and `{}`",
                                display(&result),
                                display(&arm_ty)
                            ),
                        );
                    }
                    Ty::Unknown
                }
            };
        }

        if !wildcard {
            if let Some(variants) = variants {
                let missing: Vec<_> = variants
                    .into_iter()
                    .filter(|variant| !covered.contains(variant))
                    .collect();
                if !missing.is_empty() {
                    self.error(
                        scrutinee.span,
                        "AX0408",
                        format!("non-exhaustive match; missing `{}`", missing.join("`, `")),
                    );
                }
            } else if !scrutinee_ty.is_unknown() {
                self.error(
                    scrutinee.span,
                    "AX0408",
                    "match needs a wildcard (`_`) arm or an exhaustive enum/Option/Result"
                        .to_string(),
                );
            }
        }

        if let Some(expected) = expected {
            if !same(expected, &result) {
                self.error(
                    scrutinee.span,
                    "AX0401",
                    format!(
                        "match has type `{}`; expected `{}`",
                        display(&result),
                        display(expected)
                    ),
                );
            }
            return expected.clone();
        }
        result
    }

    fn bind_pattern(
        &mut self,
        pattern: &Pattern,
        span: axiom_syntax::span::Span,
        scrutinee: &Ty,
        covered: &mut BTreeSet<String>,
        wildcard: &mut bool,
    ) {
        match pattern {
            Pattern::Wildcard => *wildcard = true,
            Pattern::Binding(name) => {
                *wildcard = true;
                self.bind(name.clone(), scrutinee.clone());
            }
            Pattern::Literal(literal) => {
                let (name, ty) = match literal {
                    axiom_syntax::ast::PatternLiteral::Int(_) => ("literal", Ty::Int),
                    axiom_syntax::ast::PatternLiteral::Str(_) => ("literal", Ty::String),
                    axiom_syntax::ast::PatternLiteral::Bool(value) => {
                        (if *value { "true" } else { "false" }, Ty::Bool)
                    }
                };
                if !same(scrutinee, &ty) {
                    self.error(
                        span,
                        "AX0412",
                        format!(
                            "pattern of type `{}` cannot match `{}`",
                            display(&ty),
                            display(scrutinee)
                        ),
                    );
                }
                if matches!(ty, Ty::Bool) && !covered.insert(name.to_string()) {
                    self.error(span, "AX0410", format!("duplicate match arm `{name}`"));
                }
            }
            Pattern::Constructor { path, inner } => {
                let variant = path.last().map(String::as_str).unwrap_or("");
                if let Some(payload) = self.types.payload(scrutinee, variant) {
                    if !covered.insert(variant.to_string()) {
                        self.error(span, "AX0410", format!("duplicate match arm `{variant}`"));
                    }
                    if let Some(patterns) = inner {
                        if let Some(pattern) = patterns.first() {
                            self.bind_nested(pattern, span, &payload);
                        }
                    }
                } else if path.len() >= 2 {
                    let known = self
                        .types
                        .variants(scrutinee)
                        .is_some_and(|variants| variants.iter().any(|known| known == variant));
                    if !known {
                        self.error(span, "AX0409", format!("unknown match variant `{variant}`"));
                    } else if inner.is_some() {
                        self.error(
                            span,
                            "AX0409",
                            format!("enum variant `{variant}` has no payload"),
                        );
                    } else if !covered.insert(variant.to_string()) {
                        self.error(span, "AX0410", format!("duplicate match arm `{variant}`"));
                    }
                } else if self
                    .types
                    .variants(scrutinee)
                    .is_some_and(|variants| variants.iter().any(|known| known == variant))
                {
                    if inner.is_some() {
                        self.error(
                            span,
                            "AX0409",
                            format!("enum variant `{variant}` has no payload"),
                        );
                    } else if !covered.insert(variant.to_string()) {
                        self.error(span, "AX0410", format!("duplicate match arm `{variant}`"));
                    }
                } else {
                    self.error(span, "AX0409", format!("unknown match variant `{variant}`"));
                }
            }
        }
    }

    fn bind_nested(&mut self, pattern: &Pattern, span: axiom_syntax::span::Span, ty: &Ty) {
        match pattern {
            Pattern::Wildcard => {}
            Pattern::Binding(name) => self.bind(name.clone(), ty.clone()),
            Pattern::Literal(_) => {}
            Pattern::Constructor { path, inner } => {
                let variant = path.last().map(String::as_str).unwrap_or("");
                if let Some(payload) = self.types.payload(ty, variant) {
                    if let Some(pattern) = inner.as_ref().and_then(|inner| inner.first()) {
                        self.bind_nested(pattern, span, &payload);
                    }
                } else if self
                    .types
                    .variants(ty)
                    .is_some_and(|variants| variants.iter().any(|known| known == variant))
                {
                    if inner.is_some() {
                        self.error(
                            span,
                            "AX0409",
                            format!("enum variant `{variant}` has no payload"),
                        );
                    }
                } else {
                    self.error(span, "AX0409", format!("unknown match variant `{variant}`"));
                }
            }
        }
    }
}
