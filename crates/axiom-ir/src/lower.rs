//! Lowering from the checked AST into the shared IR.
//!
//! Lowering is total: the checker has already rejected anything malformed, so
//! this pass never fails. It resolves type names, enum constructors, namespaces,
//! and function references, and attaches source spans to every node.

use std::collections::BTreeSet;

use axiom_syntax::ast::*;

use crate::ir::*;

/// One input module for lowering: a display name and a parsed file.
pub struct ModuleInput<'a> {
    pub name: String,
    pub display: String,
    pub file: &'a File,
}

const NAMESPACES: &[&str] = &[
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

/// Lower a set of modules into a whole program for `target`.
pub fn lower(modules: &[ModuleInput], target: Target) -> Program {
    let enums: BTreeSet<String> = modules
        .iter()
        .flat_map(|m| m.file.declarations.iter())
        .filter_map(|d| match &d.kind {
            DeclKind::Enum(e) => Some(e.name.clone()),
            _ => None,
        })
        .collect();

    Program {
        target,
        modules: modules.iter().map(|m| lower_module(m, &enums)).collect(),
    }
}

fn lower_module(module: &ModuleInput, enums: &BTreeSet<String>) -> IrModule {
    let mut types = Vec::new();
    let mut ir_enums = Vec::new();
    let mut functions = Vec::new();
    let mut agents = Vec::new();
    let mut workspaces = Vec::new();

    for decl in &module.file.declarations {
        match &decl.kind {
            DeclKind::Type(t) => types.push(IrTypeDecl {
                name: t.name.clone(),
                generics: t.generics.clone(),
                ty: lower_type(&t.ty, &t.generics.iter().cloned().collect()),
                span: t.span,
            }),
            DeclKind::Enum(e) => ir_enums.push(IrEnum {
                name: e.name.clone(),
                variants: e.variants.iter().map(|v| v.name.clone()).collect(),
                span: e.span,
            }),
            DeclKind::Function(f) => {
                functions.push(lower_function(decl.exported, f, enums));
            }
            DeclKind::Agent(a) => agents.push(lower_agent(decl.exported, a, enums)),
            DeclKind::Workspace(w) => workspaces.push(lower_workspace(decl.exported, w, enums)),
        }
    }

    IrModule {
        name: module.name.clone(),
        display: module.display.clone(),
        imports: module
            .file
            .imports
            .iter()
            .map(|i| IrImport {
                names: i.names.clone(),
                from: i.from.clone(),
                span: i.span,
            })
            .collect(),
        types,
        enums: ir_enums,
        functions,
        agents,
        workspaces,
    }
}

fn lower_function(exported: bool, f: &FunctionDecl, enums: &BTreeSet<String>) -> IrFunction {
    let generics: BTreeSet<String> = f.generics.iter().cloned().collect();
    let mut scope: BTreeSet<String> = f.params.iter().map(|p| p.name.clone()).collect();
    let effects: Vec<String> = f
        .effects
        .as_ref()
        .map(|e| e.iter().map(EffectName::dotted).collect())
        .unwrap_or_default();

    let body = lower_block(&f.body, &generics, &mut scope, enums);

    IrFunction {
        name: f.name.clone(),
        exported,
        api: f.api,
        is_async: f.is_async,
        generics: f.generics.clone(),
        params: f
            .params
            .iter()
            .map(|p| (p.name.clone(), lower_type(&p.ty, &generics)))
            .collect(),
        ret: f
            .ret
            .as_ref()
            .map(|t| lower_type(t, &generics))
            .unwrap_or(IrType::Unit),
        required_capabilities: effects.clone(),
        effects,
        body,
        span: f.span,
    }
}

fn lower_agent(exported: bool, a: &AgentDecl, enums: &BTreeSet<String>) -> IrAgent {
    let generics: BTreeSet<String> = BTreeSet::new();
    IrAgent {
        name: a.name.clone(),
        exported,
        params: a
            .params
            .iter()
            .map(|p| (p.name.clone(), lower_type(&p.ty, &generics)))
            .collect(),
        ret: lower_type(&a.ret, &generics),
        members: a
            .members
            .iter()
            .map(|m| {
                (
                    m.name.clone(),
                    lower_expr(&m.value, &generics, &mut BTreeSet::new(), enums),
                )
            })
            .collect(),
        span: a.span,
    }
}

fn lower_workspace(exported: bool, w: &WorkspaceDecl, enums: &BTreeSet<String>) -> IrWorkspace {
    let generics: BTreeSet<String> = BTreeSet::new();
    IrWorkspace {
        name: w.name.clone(),
        exported,
        members: w
            .members
            .iter()
            .map(|m| {
                (
                    m.name.clone(),
                    lower_expr(&m.value, &generics, &mut BTreeSet::new(), enums),
                )
            })
            .collect(),
        span: w.span,
    }
}

fn lower_block(
    block: &Block,
    generics: &BTreeSet<String>,
    scope: &mut BTreeSet<String>,
    enums: &BTreeSet<String>,
) -> IrBlock {
    let mut stmts = Vec::with_capacity(block.stmts.len());
    for stmt in &block.stmts {
        match &stmt.kind {
            StmtKind::Let { name, ty, value } => {
                let value = lower_expr(value, generics, scope, enums);
                scope.insert(name.clone());
                stmts.push(IrStmt {
                    kind: IrStmtKind::Let {
                        name: name.clone(),
                        ty: ty.as_ref().map(|t| lower_type(t, generics)),
                        value,
                    },
                    span: stmt.span,
                });
            }
            StmtKind::For { var, iter, body } => {
                let iter = lower_expr(iter, generics, scope, enums);
                let mut inner = scope.clone();
                inner.insert(var.clone());
                let body = lower_block(body, generics, &mut inner, enums);
                stmts.push(IrStmt {
                    kind: IrStmtKind::For {
                        var: var.clone(),
                        iter,
                        body,
                    },
                    span: stmt.span,
                });
            }
            StmtKind::Return(value) => stmts.push(IrStmt {
                kind: IrStmtKind::Return(
                    value
                        .as_ref()
                        .map(|v| lower_expr(v, generics, scope, enums)),
                ),
                span: stmt.span,
            }),
            StmtKind::Expr(e) => stmts.push(IrStmt {
                kind: IrStmtKind::Expr(lower_expr(e, generics, scope, enums)),
                span: stmt.span,
            }),
        }
    }
    IrBlock {
        stmts,
        span: block.span,
    }
}

fn lower_expr(
    expr: &Expr,
    generics: &BTreeSet<String>,
    scope: &mut BTreeSet<String>,
    enums: &BTreeSet<String>,
) -> IrExpr {
    let kind = match &expr.kind {
        ExprKind::Int(i) => IrExprKind::Int(*i),
        ExprKind::Float(f) => IrExprKind::Float(*f),
        ExprKind::Str(s) => IrExprKind::Str(s.clone()),
        ExprKind::Bool(b) => IrExprKind::Bool(*b),
        ExprKind::Duration { millis, .. } => IrExprKind::Duration(*millis),
        ExprKind::Size { bytes, .. } => IrExprKind::Int(*bytes as i64),
        ExprKind::Ident(name) => lower_ident(name, scope),
        ExprKind::List(items) => IrExprKind::List(
            items
                .iter()
                .map(|a| lower_expr(&a.value, generics, scope, enums))
                .collect(),
        ),
        ExprKind::Record(members) => IrExprKind::Record(
            members
                .iter()
                .map(|m| (m.name.clone(), lower_expr(&m.value, generics, scope, enums)))
                .collect(),
        ),
        ExprKind::Field(base, name) => lower_field(base, name, generics, scope, enums),
        ExprKind::Index(base, index) => IrExprKind::Index(
            Box::new(lower_expr(base, generics, scope, enums)),
            Box::new(lower_expr(index, generics, scope, enums)),
        ),
        ExprKind::Call {
            callee,
            args,
            type_args: _,
        } => {
            if let ExprKind::Ident(ctor) = &callee.kind {
                if matches!(ctor.as_str(), "Some" | "None" | "Ok" | "Err") {
                    return IrExpr {
                        kind: IrExprKind::Variant {
                            name: ctor.clone(),
                            args: args
                                .iter()
                                .map(|a| lower_expr(&a.value, generics, scope, enums))
                                .collect(),
                        },
                        span: expr.span,
                    };
                }
            }
            IrExprKind::Call {
                callee: Box::new(lower_expr(callee, generics, scope, enums)),
                args: args
                    .iter()
                    .map(|a| IrArg {
                        name: a.name.clone(),
                        value: lower_expr(&a.value, generics, scope, enums),
                    })
                    .collect(),
            }
        }
        ExprKind::Unary { op, operand } => IrExprKind::Unary {
            op: lower_unary(*op),
            operand: Box::new(lower_expr(operand, generics, scope, enums)),
        },
        ExprKind::Binary { op, lhs, rhs } => IrExprKind::Binary {
            op: lower_binary(*op),
            lhs: Box::new(lower_expr(lhs, generics, scope, enums)),
            rhs: Box::new(lower_expr(rhs, generics, scope, enums)),
        },
        ExprKind::Await(inner) => {
            IrExprKind::Await(Box::new(lower_expr(inner, generics, scope, enums)))
        }
        ExprKind::If {
            cond,
            then,
            otherwise,
        } => IrExprKind::If {
            cond: Box::new(lower_expr(cond, generics, scope, enums)),
            then: lower_block(then, generics, scope, enums),
            otherwise: otherwise
                .as_ref()
                .map(|e| Box::new(lower_expr(e, generics, scope, enums))),
        },
        ExprKind::Match { scrutinee, arms } => IrExprKind::Match {
            scrutinee: Box::new(lower_expr(scrutinee, generics, scope, enums)),
            arms: arms
                .iter()
                .map(|arm| {
                    let mut inner = scope.clone();
                    bind_pattern(&arm.pattern, &mut inner);
                    IrArm {
                        pattern: lower_pattern(&arm.pattern),
                        body: lower_block(&arm.body, generics, &mut inner, enums),
                        span: arm.span,
                    }
                })
                .collect(),
        },
        ExprKind::Block(block) => {
            let mut inner = scope.clone();
            IrExprKind::Block(lower_block(block, generics, &mut inner, enums))
        }
    };
    IrExpr {
        kind,
        span: expr.span,
    }
}

fn lower_ident(name: &str, scope: &BTreeSet<String>) -> IrExprKind {
    if name == "None" {
        return IrExprKind::Variant {
            name: "None".to_string(),
            args: Vec::new(),
        };
    }
    if scope.contains(name) {
        IrExprKind::Local(name.to_string())
    } else if NAMESPACES.contains(&name) {
        IrExprKind::Namespace(name.to_string())
    } else {
        IrExprKind::Function(name.to_string())
    }
}

fn lower_field(
    base: &Expr,
    name: &str,
    generics: &BTreeSet<String>,
    scope: &mut BTreeSet<String>,
    enums: &BTreeSet<String>,
) -> IrExprKind {
    if let ExprKind::Ident(root) = &base.kind {
        if enums.contains(root) {
            return IrExprKind::Variant {
                name: format!("{root}.{name}"),
                args: Vec::new(),
            };
        }
        if NAMESPACES.contains(&root.as_str()) && !scope.contains(root) {
            return IrExprKind::Field(
                Box::new(IrExpr {
                    kind: IrExprKind::Namespace(root.clone()),
                    span: base.span,
                }),
                name.to_string(),
            );
        }
    }
    IrExprKind::Field(
        Box::new(lower_expr(base, generics, scope, enums)),
        name.to_string(),
    )
}

fn lower_pattern(pattern: &Pattern) -> IrPattern {
    match pattern {
        Pattern::Wildcard => IrPattern::Wildcard,
        Pattern::Binding(name) => IrPattern::Binding(name.clone()),
        Pattern::Literal(PatternLiteral::Int(i)) => IrPattern::Int(*i),
        Pattern::Literal(PatternLiteral::Str(s)) => IrPattern::Str(s.clone()),
        Pattern::Literal(PatternLiteral::Bool(b)) => IrPattern::Bool(*b),
        Pattern::Constructor { path, inner } => IrPattern::Variant {
            name: path.join("."),
            inner: inner
                .as_ref()
                .map(|items| items.iter().map(lower_pattern).collect()),
        },
    }
}

fn bind_pattern(pattern: &Pattern, scope: &mut BTreeSet<String>) {
    match pattern {
        Pattern::Binding(name) => {
            scope.insert(name.clone());
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

fn lower_type(ty: &TypeExpr, generics: &BTreeSet<String>) -> IrType {
    match &ty.kind {
        TypeKind::Named { path, args } => {
            let dotted = path.join(".");
            match dotted.as_str() {
                "string" if args.is_empty() => IrType::Str,
                "bool" => IrType::Bool,
                "int" => IrType::Int,
                "float" => IrType::Float,
                "bytes" => IrType::Bytes,
                "duration" => IrType::Duration,
                "timestamp" => IrType::Timestamp,
                "Option" if args.len() == 1 => {
                    IrType::Option(Box::new(lower_type(&args[0], generics)))
                }
                "Result" if args.len() == 2 => IrType::Result(
                    Box::new(lower_type(&args[0], generics)),
                    Box::new(lower_type(&args[1], generics)),
                ),
                "Map" if args.len() == 2 => IrType::Map(
                    Box::new(lower_type(&args[0], generics)),
                    Box::new(lower_type(&args[1], generics)),
                ),
                _ if generics.contains(&dotted) => IrType::Generic(dotted),
                _ => IrType::Named(dotted),
            }
        }
        TypeKind::List(inner) => IrType::List(Box::new(lower_type(inner, generics))),
        TypeKind::Record(fields) => IrType::Record(
            fields
                .iter()
                .map(|f| (f.name.clone(), lower_type(&f.ty, generics)))
                .collect(),
        ),
    }
}

fn lower_unary(op: UnaryOp) -> IrUnaryOp {
    match op {
        UnaryOp::Not => IrUnaryOp::Not,
        UnaryOp::Neg => IrUnaryOp::Neg,
    }
}

fn lower_binary(op: BinaryOp) -> IrBinaryOp {
    match op {
        BinaryOp::Mul => IrBinaryOp::Mul,
        BinaryOp::Div => IrBinaryOp::Div,
        BinaryOp::Rem => IrBinaryOp::Rem,
        BinaryOp::Add => IrBinaryOp::Add,
        BinaryOp::Sub => IrBinaryOp::Sub,
        BinaryOp::Lt => IrBinaryOp::Lt,
        BinaryOp::Le => IrBinaryOp::Le,
        BinaryOp::Gt => IrBinaryOp::Gt,
        BinaryOp::Ge => IrBinaryOp::Ge,
        BinaryOp::Eq => IrBinaryOp::Eq,
        BinaryOp::Ne => IrBinaryOp::Ne,
        BinaryOp::And => IrBinaryOp::And,
        BinaryOp::Or => IrBinaryOp::Or,
    }
}
