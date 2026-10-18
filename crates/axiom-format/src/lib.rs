//! Deterministic formatter for Axiom.
//!
//! The formatter is a pretty-printer over the AST. It is intentionally simple:
//! it emits one canonical layout (two-space indentation, `;`-terminated
//! statements, records with `;` fields) so that formatting is stable — running
//! it twice yields identical output.

use axiom_syntax::ast::*;

/// Format a parsed file into canonical Axiom source.
pub fn format(file: &File) -> String {
    let mut printer = Printer::default();
    printer.file(file);
    printer.finish()
}

#[derive(Default)]
struct Printer {
    out: String,
    indent: usize,
}

impl Printer {
    fn finish(mut self) -> String {
        // Exactly one trailing newline.
        while self.out.ends_with('\n') {
            self.out.pop();
        }
        self.out.push('\n');
        self.out
    }

    fn line(&mut self, text: &str) {
        for _ in 0..self.indent {
            self.out.push_str("  ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn blank(&mut self) {
        if !self.out.ends_with("\n\n") && !self.out.is_empty() {
            self.out.push('\n');
        }
    }

    fn file(&mut self, file: &File) {
        if let Some(module) = &file.module {
            self.line(&format!("module {};", module.dotted()));
        }
        if !file.imports.is_empty() {
            self.blank();
            for import in &file.imports {
                self.line(&format!(
                    "import {{ {} }} from \"{}\";",
                    import.names.join(", "),
                    import.from
                ));
            }
        }
        for decl in &file.declarations {
            self.blank();
            self.declaration(decl);
        }
    }

    fn declaration(&mut self, decl: &Declaration) {
        let export = if decl.exported { "export " } else { "" };
        match &decl.kind {
            DeclKind::Type(t) => {
                let generics = generics(&t.generics);
                self.line(&format!(
                    "{export}type {}{generics} = {};",
                    t.name,
                    type_expr(&t.ty)
                ));
            }
            DeclKind::Enum(e) => {
                let variants: Vec<&str> = e.variants.iter().map(|v| v.name.as_str()).collect();
                self.line(&format!(
                    "{export}enum {} {{ {} }}",
                    e.name,
                    variants.join("; ")
                ));
            }
            DeclKind::Function(f) => self.function(export, f),
            DeclKind::Agent(a) => {
                self.line(&format!(
                    "{export}agent {}({}) -> {} {{",
                    a.name,
                    params(&a.params),
                    type_expr(&a.ret)
                ));
                self.indent += 1;
                for member in &a.members {
                    self.line(&format!("{}: {};", member.name, expr(&member.value, 0)));
                }
                self.indent -= 1;
                self.line("}");
            }
            DeclKind::Workspace(w) => {
                self.line(&format!("{export}workspace {} {{", w.name));
                self.indent += 1;
                for member in &w.members {
                    self.line(&format!("{}: {};", member.name, expr(&member.value, 0)));
                }
                self.indent -= 1;
                self.line("}");
            }
        }
    }

    fn function(&mut self, export: &str, f: &FunctionDecl) {
        let api = if f.api { "api " } else { "" };
        let async_kw = if f.is_async { "async " } else { "" };
        let generics = generics(&f.generics);
        let ret = f
            .ret
            .as_ref()
            .map(|t| format!(" -> {}", type_expr(t)))
            .unwrap_or_default();
        let effects = match &f.effects {
            Some(list) => {
                let names: Vec<String> = list.iter().map(|e| e.dotted()).collect();
                format!(" effects [{}]", names.join(", "))
            }
            None => String::new(),
        };
        self.line(&format!(
            "{export}{api}{async_kw}fn {}{generics}({}){ret}{effects} {{",
            f.name,
            params(&f.params)
        ));
        self.indent += 1;
        self.block_body(&f.body);
        self.indent -= 1;
        self.line("}");
    }

    fn block_body(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.statement(stmt);
        }
    }

    fn statement(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Let { name, ty, value } => {
                let ty = ty
                    .as_ref()
                    .map(|t| format!(": {}", type_expr(t)))
                    .unwrap_or_default();
                self.line(&format!("let {name}{ty} = {};", expr(value, 0)));
            }
            StmtKind::For { var, iter, body } => {
                self.line(&format!("for {var} in {} {{", expr(iter, 0)));
                self.indent += 1;
                self.block_body(body);
                self.indent -= 1;
                self.line("}");
            }
            StmtKind::Return(Some(value)) => self.line(&format!("return {};", expr(value, 0))),
            StmtKind::Return(None) => self.line("return;"),
            StmtKind::Expr(e) => match &e.kind {
                ExprKind::If { .. } | ExprKind::Match { .. } => self.expr_statement(e),
                _ => self.line(&format!("{};", expr(e, 0))),
            },
        }
    }

    fn expr_statement(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::If { .. } => self.if_chain(e, ""),
            ExprKind::Match { scrutinee, arms } => {
                self.line(&format!("match {} {{", expr(scrutinee, 0)));
                self.indent += 1;
                for arm in arms {
                    self.line(&format!("{} => {{", pattern(&arm.pattern)));
                    self.indent += 1;
                    self.block_body(&arm.body);
                    self.indent -= 1;
                    self.line("}");
                }
                self.indent -= 1;
                self.line("}");
            }
            _ => self.line(&format!("{};", expr(e, 0))),
        }
    }

    /// Print an `if` / `else if` / `else` chain. `prefix` is either "" (start)
    /// or "} else " (continued).
    fn if_chain(&mut self, e: &Expr, prefix: &str) {
        let ExprKind::If {
            cond,
            then,
            otherwise,
        } = &e.kind
        else {
            return;
        };
        self.line(&format!("{prefix}if {} {{", expr(cond, 0)));
        self.indent += 1;
        self.block_body(then);
        self.indent -= 1;
        match otherwise {
            None => self.line("}"),
            Some(alt) => match &alt.kind {
                ExprKind::Block(block) => {
                    self.line("} else {");
                    self.indent += 1;
                    self.block_body(block);
                    self.indent -= 1;
                    self.line("}");
                }
                ExprKind::If { .. } => self.if_chain(alt, "} else "),
                _ => self.line("}"),
            },
        }
    }
}

fn generics(list: &[String]) -> String {
    if list.is_empty() {
        String::new()
    } else {
        format!("<{}>", list.join(", "))
    }
}

fn params(params: &[Param]) -> String {
    params
        .iter()
        .map(|p| format!("{}: {}", p.name, type_expr(&p.ty)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn type_expr(ty: &TypeExpr) -> String {
    match &ty.kind {
        TypeKind::Named { path, args } => {
            let base = path.join(".");
            if args.is_empty() {
                base
            } else {
                format!(
                    "{base}<{}>",
                    args.iter().map(type_expr).collect::<Vec<_>>().join(", ")
                )
            }
        }
        TypeKind::List(inner) => format!("{}[]", type_expr(inner)),
        TypeKind::Record(fields) => {
            let body = fields
                .iter()
                .map(|f| format!("{}: {}", f.name, type_expr(&f.ty)))
                .collect::<Vec<_>>()
                .join("; ");
            format!("{{ {body}; }}")
        }
    }
}

fn pattern(pat: &Pattern) -> String {
    match pat {
        Pattern::Wildcard => "_".to_string(),
        Pattern::Binding(name) => name.clone(),
        Pattern::Literal(lit) => match lit {
            PatternLiteral::Int(v) => v.to_string(),
            PatternLiteral::Str(v) => format!("\"{v}\""),
            PatternLiteral::Bool(v) => v.to_string(),
        },
        Pattern::Constructor { path, inner } => {
            let base = path.join(".");
            match inner {
                Some(patterns) => format!(
                    "{base}({})",
                    patterns.iter().map(pattern).collect::<Vec<_>>().join(", ")
                ),
                None => base,
            }
        }
    }
}

fn prec(e: &Expr) -> u8 {
    match &e.kind {
        ExprKind::Binary { op, .. } => match op {
            BinaryOp::Or => 1,
            BinaryOp::And => 2,
            BinaryOp::Eq | BinaryOp::Ne => 3,
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => 4,
            BinaryOp::Add | BinaryOp::Sub => 5,
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => 6,
        },
        ExprKind::Unary { .. } => 7,
        ExprKind::Await(_) => 7,
        _ => 9,
    }
}

fn expr(e: &Expr, parent: u8) -> String {
    let this = prec(e);
    let text = match &e.kind {
        ExprKind::Int(v) => v.to_string(),
        ExprKind::Float(v) => format!("{v}"),
        ExprKind::Str(v) => format!("\"{v}\""),
        ExprKind::Bool(v) => v.to_string(),
        ExprKind::Duration { millis, unit } => duration(*millis, unit),
        ExprKind::Size { bytes, unit } => format!("{bytes}{unit}"),
        ExprKind::Ident(name) => name.clone(),
        ExprKind::List(items) => format!(
            "[{}]",
            items
                .iter()
                .map(|a| expr(&a.value, 0))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ExprKind::Record(members) => format!(
            "{{ {} }}",
            members
                .iter()
                .map(|m| format!("{}: {}", m.name, expr(&m.value, 0)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ExprKind::Field(base, name) => format!("{}.{}", expr(base, 8), name),
        ExprKind::Index(base, index) => format!("{}[{}]", expr(base, 8), expr(index, 0)),
        ExprKind::Call {
            callee,
            type_args,
            args,
        } => {
            let generics = if type_args.is_empty() {
                String::new()
            } else {
                format!(
                    "<{}>",
                    type_args
                        .iter()
                        .map(type_expr)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            format!(
                "{}{}({})",
                expr(callee, 8),
                generics,
                args.iter()
                    .map(|a| match &a.name {
                        Some(name) => format!("{name}: {}", expr(&a.value, 0)),
                        None => expr(&a.value, 0),
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        ExprKind::Unary { op, operand } => {
            let op = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
            };
            format!("{op}{}", expr(operand, 7))
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let symbol = match op {
                BinaryOp::Mul => "*",
                BinaryOp::Div => "/",
                BinaryOp::Rem => "%",
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Lt => "<",
                BinaryOp::Le => "<=",
                BinaryOp::Gt => ">",
                BinaryOp::Ge => ">=",
                BinaryOp::Eq => "==",
                BinaryOp::Ne => "!=",
                BinaryOp::And => "&&",
                BinaryOp::Or => "||",
            };
            format!("{} {symbol} {}", expr(lhs, this), expr(rhs, this + 1))
        }
        ExprKind::Await(inner) => format!("await {}", expr(inner, 7)),
        ExprKind::If { .. } | ExprKind::Match { .. } | ExprKind::Block(_) => {
            // Multi-line constructs are only emitted in statement position.
            "/* block expression */".to_string()
        }
    };
    if this < parent {
        format!("({text})")
    } else {
        text
    }
}

fn duration(millis: u64, unit: &str) -> String {
    match unit {
        "ms" => format!("{millis}ms"),
        "s" => format!("{}s", millis / 1000),
        "m" => format!("{}m", millis / 60_000),
        "h" => format!("{}h", millis / 3_600_000),
        _ => format!("{millis}ms"),
    }
}
