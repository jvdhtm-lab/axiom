//! Deterministic JSON rendering of the IR.
//!
//! The printer is deterministic by construction: object keys are emitted in the
//! order the IR stores them (already stably ordered) and there is no hashing or
//! floating-point ambiguity for the values Axiom produces.

use axiom_syntax::span::Span;

use crate::ir::*;

/// A minimal JSON value with deterministic rendering.
#[derive(Debug, Clone)]
pub enum J {
    Str(String),
    Num(String),
    Bool(bool),
    Null,
    Arr(Vec<J>),
    Obj(Vec<(&'static str, J)>),
}

impl J {
    fn render(&self, out: &mut String, indent: usize) {
        match self {
            J::Str(s) => {
                out.push('"');
                for c in s.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\r' => out.push_str("\\r"),
                        '\t' => out.push_str("\\t"),
                        c if (c as u32) < 0x20 => {
                            out.push_str(&format!("\\u{:04x}", c as u32));
                        }
                        c => out.push(c),
                    }
                }
                out.push('"');
            }
            J::Num(n) => out.push_str(n),
            J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            J::Null => out.push_str("null"),
            J::Arr(items) => {
                if items.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push_str("[\n");
                for (i, item) in items.iter().enumerate() {
                    pad(out, indent + 1);
                    item.render(out, indent + 1);
                    if i + 1 < items.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                pad(out, indent);
                out.push(']');
            }
            J::Obj(fields) => {
                if fields.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push_str("{\n");
                for (i, (key, value)) in fields.iter().enumerate() {
                    pad(out, indent + 1);
                    J::Str((*key).to_string()).render(out, indent + 1);
                    out.push_str(": ");
                    value.render(out, indent + 1);
                    if i + 1 < fields.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                pad(out, indent);
                out.push('}');
            }
        }
    }
}

fn pad(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("  ");
    }
}

fn str(s: impl Into<String>) -> J {
    J::Str(s.into())
}

fn num(n: impl ToString) -> J {
    J::Num(n.to_string())
}

fn arr(items: impl IntoIterator<Item = J>) -> J {
    J::Arr(items.into_iter().collect())
}

fn span_json(span: Span) -> J {
    J::Arr(vec![num(span.start), num(span.end)])
}

/// Render a program to stable, indented JSON.
pub fn program_to_json(program: &Program) -> String {
    let mut out = String::new();
    program_json(program).render(&mut out, 0);
    out.push('\n');
    out
}

fn program_json(program: &Program) -> J {
    J::Obj(vec![
        ("ir_version", num(1)),
        ("target", str(program.target.as_str())),
        ("modules", arr(program.modules.iter().map(module_json))),
    ])
}

fn module_json(module: &IrModule) -> J {
    J::Obj(vec![
        ("name", str(&module.name)),
        ("display", str(&module.display)),
        (
            "imports",
            arr(module.imports.iter().map(|i| {
                J::Obj(vec![
                    ("names", arr(i.names.iter().map(|n| str(n.clone())))),
                    ("from", str(&i.from)),
                    ("span", span_json(i.span)),
                ])
            })),
        ),
        (
            "types",
            arr(module.types.iter().map(|t| {
                J::Obj(vec![
                    ("name", str(&t.name)),
                    ("generics", arr(t.generics.iter().map(|g| str(g.clone())))),
                    ("type", str(t.ty.name())),
                    ("span", span_json(t.span)),
                ])
            })),
        ),
        (
            "enums",
            arr(module.enums.iter().map(|e| {
                J::Obj(vec![
                    ("name", str(&e.name)),
                    ("variants", arr(e.variants.iter().map(|v| str(v.clone())))),
                    ("span", span_json(e.span)),
                ])
            })),
        ),
        ("functions", arr(module.functions.iter().map(function_json))),
        ("agents", arr(module.agents.iter().map(agent_json))),
        (
            "workspaces",
            arr(module.workspaces.iter().map(workspace_json)),
        ),
    ])
}

fn function_json(f: &IrFunction) -> J {
    J::Obj(vec![
        ("name", str(&f.name)),
        ("exported", J::Bool(f.exported)),
        ("api", J::Bool(f.api)),
        ("async", J::Bool(f.is_async)),
        (
            "params",
            arr(f
                .params
                .iter()
                .map(|(n, t)| J::Obj(vec![("name", str(n.clone())), ("type", str(t.name()))]))),
        ),
        ("returns", str(f.ret.name())),
        ("effects", arr(f.effects.iter().map(|e| str(e.clone())))),
        (
            "required_capabilities",
            arr(f.required_capabilities.iter().map(|c| str(c.clone()))),
        ),
        ("span", span_json(f.span)),
        ("body", block_json(&f.body)),
    ])
}

fn agent_json(a: &IrAgent) -> J {
    J::Obj(vec![
        ("name", str(&a.name)),
        ("exported", J::Bool(a.exported)),
        (
            "params",
            arr(a
                .params
                .iter()
                .map(|(n, t)| J::Obj(vec![("name", str(n.clone())), ("type", str(t.name()))]))),
        ),
        ("returns", str(a.ret.name())),
        ("members", members_json(&a.members)),
        ("span", span_json(a.span)),
    ])
}

fn workspace_json(w: &IrWorkspace) -> J {
    J::Obj(vec![
        ("name", str(&w.name)),
        ("exported", J::Bool(w.exported)),
        ("members", members_json(&w.members)),
        ("span", span_json(w.span)),
    ])
}

fn members_json(members: &[(String, IrExpr)]) -> J {
    arr(members
        .iter()
        .map(|(n, e)| J::Obj(vec![("name", str(n.clone())), ("value", expr_json(e))])))
}

fn block_json(block: &IrBlock) -> J {
    J::Obj(vec![
        ("stmts", arr(block.stmts.iter().map(stmt_json))),
        ("span", span_json(block.span)),
    ])
}

fn stmt_json(stmt: &IrStmt) -> J {
    let kind = match &stmt.kind {
        IrStmtKind::Let { name, ty, value } => J::Obj(vec![
            ("kind", str("let")),
            ("name", str(name.clone())),
            (
                "type",
                match ty {
                    Some(t) => str(t.name()),
                    None => J::Null,
                },
            ),
            ("value", expr_json(value)),
        ]),
        IrStmtKind::For { var, iter, body } => J::Obj(vec![
            ("kind", str("for")),
            ("var", str(var.clone())),
            ("iter", expr_json(iter)),
            ("body", block_json(body)),
        ]),
        IrStmtKind::Return(value) => J::Obj(vec![
            ("kind", str("return")),
            (
                "value",
                match value {
                    Some(v) => expr_json(v),
                    None => J::Null,
                },
            ),
        ]),
        IrStmtKind::Expr(e) => J::Obj(vec![("kind", str("expr")), ("value", expr_json(e))]),
    };
    J::Obj(vec![("stmt", kind), ("span", span_json(stmt.span))])
}

fn expr_json(expr: &IrExpr) -> J {
    let kind = match &expr.kind {
        IrExprKind::Unit => J::Obj(vec![("kind", str("unit"))]),
        IrExprKind::Bool(b) => J::Obj(vec![("kind", str("bool")), ("value", J::Bool(*b))]),
        IrExprKind::Int(i) => J::Obj(vec![("kind", str("int")), ("value", num(i))]),
        IrExprKind::Float(f) => J::Obj(vec![("kind", str("float")), ("value", num(f))]),
        IrExprKind::Str(s) => J::Obj(vec![("kind", str("string")), ("value", str(s.clone()))]),
        IrExprKind::Bytes(b) => J::Obj(vec![("kind", str("bytes")), ("value", num(b.len()))]),
        IrExprKind::Duration(ms) => J::Obj(vec![("kind", str("duration")), ("millis", num(ms))]),
        IrExprKind::Local(name) => {
            J::Obj(vec![("kind", str("local")), ("name", str(name.clone()))])
        }
        IrExprKind::Function(name) => {
            J::Obj(vec![("kind", str("function")), ("name", str(name.clone()))])
        }
        IrExprKind::Namespace(name) => J::Obj(vec![
            ("kind", str("namespace")),
            ("name", str(name.clone())),
        ]),
        IrExprKind::List(items) => J::Obj(vec![
            ("kind", str("list")),
            ("items", arr(items.iter().map(expr_json))),
        ]),
        IrExprKind::Record(fields) => J::Obj(vec![
            ("kind", str("record")),
            (
                "fields",
                arr(fields
                    .iter()
                    .map(|(n, v)| J::Obj(vec![("name", str(n.clone())), ("value", expr_json(v))]))),
            ),
        ]),
        IrExprKind::Variant { name, args } => J::Obj(vec![
            ("kind", str("variant")),
            ("name", str(name.clone())),
            ("args", arr(args.iter().map(expr_json))),
        ]),
        IrExprKind::Field(base, name) => J::Obj(vec![
            ("kind", str("field")),
            ("base", expr_json(base)),
            ("name", str(name.clone())),
        ]),
        IrExprKind::Index(base, index) => J::Obj(vec![
            ("kind", str("index")),
            ("base", expr_json(base)),
            ("index", expr_json(index)),
        ]),
        IrExprKind::Call { callee, args } => J::Obj(vec![
            ("kind", str("call")),
            ("callee", expr_json(callee)),
            (
                "args",
                arr(args.iter().map(|a| {
                    J::Obj(vec![
                        (
                            "name",
                            match &a.name {
                                Some(n) => str(n.clone()),
                                None => J::Null,
                            },
                        ),
                        ("value", expr_json(&a.value)),
                    ])
                })),
            ),
        ]),
        IrExprKind::Unary { op, operand } => J::Obj(vec![
            ("kind", str("unary")),
            ("op", str(unary_str(*op))),
            ("operand", expr_json(operand)),
        ]),
        IrExprKind::Binary { op, lhs, rhs } => J::Obj(vec![
            ("kind", str("binary")),
            ("op", str(binary_str(*op))),
            ("lhs", expr_json(lhs)),
            ("rhs", expr_json(rhs)),
        ]),
        IrExprKind::Await(inner) => {
            J::Obj(vec![("kind", str("await")), ("value", expr_json(inner))])
        }
        IrExprKind::If {
            cond,
            then,
            otherwise,
        } => J::Obj(vec![
            ("kind", str("if")),
            ("cond", expr_json(cond)),
            ("then", block_json(then)),
            (
                "else",
                match otherwise {
                    Some(e) => expr_json(e),
                    None => J::Null,
                },
            ),
        ]),
        IrExprKind::Match { scrutinee, arms } => J::Obj(vec![
            ("kind", str("match")),
            ("scrutinee", expr_json(scrutinee)),
            (
                "arms",
                arr(arms.iter().map(|arm| {
                    J::Obj(vec![
                        ("pattern", pattern_json(&arm.pattern)),
                        ("body", block_json(&arm.body)),
                        ("span", span_json(arm.span)),
                    ])
                })),
            ),
        ]),
        IrExprKind::Block(block) => {
            J::Obj(vec![("kind", str("block")), ("value", block_json(block))])
        }
    };
    J::Obj(vec![("expr", kind), ("span", span_json(expr.span))])
}

fn pattern_json(pattern: &IrPattern) -> J {
    match pattern {
        IrPattern::Wildcard => J::Obj(vec![("kind", str("wildcard"))]),
        IrPattern::Binding(name) => {
            J::Obj(vec![("kind", str("binding")), ("name", str(name.clone()))])
        }
        IrPattern::Int(i) => J::Obj(vec![("kind", str("int")), ("value", num(i))]),
        IrPattern::Str(s) => J::Obj(vec![("kind", str("string")), ("value", str(s.clone()))]),
        IrPattern::Bool(b) => J::Obj(vec![("kind", str("bool")), ("value", J::Bool(*b))]),
        IrPattern::Variant { name, inner } => J::Obj(vec![
            ("kind", str("variant")),
            ("name", str(name.clone())),
            (
                "inner",
                match inner {
                    Some(items) => arr(items.iter().map(pattern_json)),
                    None => J::Null,
                },
            ),
        ]),
    }
}

fn unary_str(op: IrUnaryOp) -> &'static str {
    match op {
        IrUnaryOp::Not => "not",
        IrUnaryOp::Neg => "neg",
    }
}

fn binary_str(op: IrBinaryOp) -> &'static str {
    match op {
        IrBinaryOp::Mul => "*",
        IrBinaryOp::Div => "/",
        IrBinaryOp::Rem => "%",
        IrBinaryOp::Add => "+",
        IrBinaryOp::Sub => "-",
        IrBinaryOp::Lt => "<",
        IrBinaryOp::Le => "<=",
        IrBinaryOp::Gt => ">",
        IrBinaryOp::Ge => ">=",
        IrBinaryOp::Eq => "==",
        IrBinaryOp::Ne => "!=",
        IrBinaryOp::And => "and",
        IrBinaryOp::Or => "or",
    }
}
