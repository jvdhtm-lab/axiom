//! The deterministic server evaluator.
//!
//! Phase 3 executes the pure, portable subset of the IR with no model and no
//! ambient I/O. Effectful standard-library calls fail closed (they require a
//! runtime host introduced in later phases); nothing is silently stubbed.

use std::collections::BTreeMap;

use axiom_ir::{
    IrArm, IrBinaryOp, IrBlock, IrExpr, IrExprKind, IrFunction, IrPattern, IrStmtKind, IrUnaryOp,
    Program,
};
use axiom_syntax::span::Span;

use crate::host::Host;
use crate::value::Value;

/// A runtime failure with the IR span it occurred at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeError {
    pub message: String,
    pub span: Span,
}

impl RuntimeError {
    fn new(span: Span, message: impl Into<String>) -> Self {
        RuntimeError {
            message: message.into(),
            span,
        }
    }
}

type Frame = BTreeMap<String, Value>;

/// Evaluation control: a produced value, or an early `return`.
enum Ctl {
    Val(Value),
    Ret(Value),
}

impl Ctl {
    fn value(self) -> Value {
        match self {
            Ctl::Val(v) | Ctl::Ret(v) => v,
        }
    }
}

enum Flow {
    Value(Value),
    Return(Value),
}

/// The runtime entry point context.
pub struct Evaluator<'a> {
    functions: BTreeMap<String, &'a IrFunction>,
    host: Option<&'a dyn Host>,
}

/// Run `entry` with `args` in a host-less, ambient-I/O-free evaluator.
pub fn run(program: &Program, entry: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
    run_impl(program, entry, args, None)
}

/// Run `entry` with `args`, resolving effectful standard-library calls through
/// `host`. Without a host those calls still fail closed.
pub fn run_with_host(
    program: &Program,
    entry: &str,
    args: Vec<Value>,
    host: &dyn Host,
) -> Result<Value, RuntimeError> {
    run_impl(program, entry, args, Some(host))
}

fn run_impl(
    program: &Program,
    entry: &str,
    args: Vec<Value>,
    host: Option<&dyn Host>,
) -> Result<Value, RuntimeError> {
    let mut functions = BTreeMap::new();
    for module in &program.modules {
        for f in &module.functions {
            functions.insert(f.name.clone(), f);
        }
    }
    let evaluator = Evaluator { functions, host };
    let f =
        evaluator.functions.get(entry).copied().ok_or_else(|| {
            RuntimeError::new(Span::empty(0), format!("no entry function `{entry}`"))
        })?;
    evaluator.call(f, args)
}

impl Evaluator<'_> {
    fn call(&self, f: &IrFunction, args: Vec<Value>) -> Result<Value, RuntimeError> {
        if args.len() != f.params.len() {
            return Err(RuntimeError::new(
                f.span,
                format!(
                    "`{}` expects {} argument(s), got {}",
                    f.name,
                    f.params.len(),
                    args.len()
                ),
            ));
        }
        let mut frame: Frame = Frame::new();
        for ((name, _), value) in f.params.iter().zip(args) {
            frame.insert(name.clone(), value);
        }
        let mut env = vec![frame];
        match self.eval_block(&f.body, &mut env, f)? {
            Flow::Value(v) | Flow::Return(v) => Ok(v),
        }
    }

    fn eval_block(
        &self,
        block: &IrBlock,
        env: &mut Vec<Frame>,
        f: &IrFunction,
    ) -> Result<Flow, RuntimeError> {
        let mut value = Value::Unit;
        for stmt in &block.stmts {
            match &stmt.kind {
                IrStmtKind::Let {
                    name, value: ve, ..
                } => match self.eval(ve, env, f)? {
                    Ctl::Val(v) => {
                        env.last_mut().unwrap().insert(name.clone(), v);
                    }
                    Ctl::Ret(v) => return Ok(Flow::Return(v)),
                },
                IrStmtKind::Return(Some(e)) => {
                    let v = self.eval(e, env, f)?.value();
                    return Ok(Flow::Return(v));
                }
                IrStmtKind::Return(None) => return Ok(Flow::Return(Value::Unit)),
                IrStmtKind::Expr(e) => match self.eval(e, env, f)? {
                    Ctl::Val(v) => value = v,
                    Ctl::Ret(v) => return Ok(Flow::Return(v)),
                },
                IrStmtKind::For { var, iter, body } => {
                    let list = self.eval(iter, env, f)?.value();
                    let items = match list {
                        Value::List(items) => items,
                        other => {
                            return Err(RuntimeError::new(
                                iter.span,
                                format!("`for` expects a list, found {}", kind_of(&other)),
                            ))
                        }
                    };
                    for item in items {
                        let mut frame = Frame::new();
                        frame.insert(var.clone(), item);
                        env.push(frame);
                        let flow = self.eval_block(body, env, f);
                        env.pop();
                        if let Flow::Return(v) = flow? {
                            return Ok(Flow::Return(v));
                        }
                    }
                }
            }
        }
        Ok(Flow::Value(value))
    }

    fn eval(
        &self,
        expr: &IrExpr,
        env: &mut Vec<Frame>,
        f: &IrFunction,
    ) -> Result<Ctl, RuntimeError> {
        let span = expr.span;
        let value = match &expr.kind {
            IrExprKind::Unit => Value::Unit,
            IrExprKind::Bool(b) => Value::Bool(*b),
            IrExprKind::Int(i) => Value::Int(*i),
            IrExprKind::Float(x) => Value::Float(*x),
            IrExprKind::Str(s) => Value::Str(s.clone()),
            IrExprKind::Bytes(b) => Value::Bytes(b.clone()),
            IrExprKind::Duration(ms) => Value::Duration(*ms),
            IrExprKind::Local(name) => return Ok(Ctl::Val(self.lookup(name, env, span)?)),
            IrExprKind::Function(name) => Value::Function(name.clone()),
            IrExprKind::Namespace(name) => {
                return Err(RuntimeError::new(span, format!("`{name}` is not a value")));
            }
            IrExprKind::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(self.eval(item, env, f)?.value());
                }
                Value::List(out)
            }
            IrExprKind::Record(fields) => {
                let mut out = Vec::with_capacity(fields.len());
                for (name, value_expr) in fields {
                    out.push((name.clone(), self.eval(value_expr, env, f)?.value()));
                }
                Value::Record(out)
            }
            IrExprKind::Variant { name, args } => {
                let mut out = Vec::with_capacity(args.len());
                for arg in args {
                    out.push(self.eval(arg, env, f)?.value());
                }
                Value::Variant(name.clone(), out)
            }
            IrExprKind::Field(base, name) => {
                let base = self.eval(base, env, f)?.value();
                match base {
                    Value::Record(fields) => fields
                        .into_iter()
                        .find(|(n, _)| n == name)
                        .map(|(_, v)| v)
                        .ok_or_else(|| {
                            RuntimeError::new(span, format!("no field `{name}` on record"))
                        })?,
                    other => {
                        return Err(RuntimeError::new(
                            span,
                            format!("cannot access field `{name}` on {}", kind_of(&other)),
                        ))
                    }
                }
            }
            IrExprKind::Index(base, index) => {
                let base = self.eval(base, env, f)?.value();
                let index = self.eval(index, env, f)?.value();
                self.index(base, index, span)?
            }
            IrExprKind::Call { callee, args } => {
                return self.eval_call(callee, args, env, f, span).map(Ctl::Val);
            }
            IrExprKind::Unary { op, operand } => {
                let operand = self.eval(operand, env, f)?.value();
                self.unary(*op, operand, span)?
            }
            IrExprKind::Binary { op, lhs, rhs } => {
                let lhs = self.eval(lhs, env, f)?.value();
                let rhs = self.eval(rhs, env, f)?.value();
                self.binary(*op, lhs, rhs, span)?
            }
            IrExprKind::Await(inner) => {
                return self.eval(inner, env, f);
            }
            IrExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                let cond = self.eval(cond, env, f)?.value();
                match cond {
                    Value::Bool(true) => return self.block_ctl(then, env, f),
                    Value::Bool(false) => match otherwise {
                        Some(e) => return self.eval(e, env, f),
                        None => Value::Unit,
                    },
                    other => {
                        return Err(RuntimeError::new(
                            span,
                            format!("`if` condition must be bool, found {}", kind_of(&other)),
                        ))
                    }
                }
            }
            IrExprKind::Match { scrutinee, arms } => {
                let value = self.eval(scrutinee, env, f)?.value();
                return self.match_arms(&value, arms, env, f, span);
            }
            IrExprKind::Block(block) => return self.block_ctl(block, env, f),
        };
        Ok(Ctl::Val(value))
    }

    fn block_ctl(
        &self,
        block: &IrBlock,
        env: &mut Vec<Frame>,
        f: &IrFunction,
    ) -> Result<Ctl, RuntimeError> {
        Ok(match self.eval_block(block, env, f)? {
            Flow::Value(v) => Ctl::Val(v),
            Flow::Return(v) => Ctl::Ret(v),
        })
    }

    fn match_arms(
        &self,
        value: &Value,
        arms: &[IrArm],
        env: &mut Vec<Frame>,
        f: &IrFunction,
        span: Span,
    ) -> Result<Ctl, RuntimeError> {
        for arm in arms {
            let mut frame = Frame::new();
            if self.pattern_matches(&arm.pattern, value, &mut frame)? {
                env.push(frame);
                let flow = self.eval_block(&arm.body, env, f);
                env.pop();
                return Ok(match flow? {
                    Flow::Value(v) => Ctl::Val(v),
                    Flow::Return(v) => Ctl::Ret(v),
                });
            }
        }
        Err(RuntimeError::new(
            span,
            format!("non-exhaustive match: no arm matched {}", value.display()),
        ))
    }

    fn pattern_matches(
        &self,
        pattern: &IrPattern,
        value: &Value,
        frame: &mut Frame,
    ) -> Result<bool, RuntimeError> {
        Ok(match pattern {
            IrPattern::Wildcard => true,
            IrPattern::Binding(name) => {
                frame.insert(name.clone(), value.clone());
                true
            }
            IrPattern::Int(i) => matches!(value, Value::Int(v) if v == i),
            IrPattern::Str(s) => matches!(value, Value::Str(v) if v == s),
            IrPattern::Bool(b) => matches!(value, Value::Bool(v) if v == b),
            IrPattern::Variant { name, inner } => {
                if let Value::Variant(vname, vargs) = value {
                    if !variant_matches(name, vname) {
                        return Ok(false);
                    }
                    if let Some(items) = inner {
                        for (pat, arg) in items.iter().zip(vargs) {
                            if !self.pattern_matches(pat, arg, frame)? {
                                return Ok(false);
                            }
                        }
                    }
                    true
                } else {
                    false
                }
            }
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn eval_call(
        &self,
        callee: &IrExpr,
        args: &[axiom_ir::IrArg],
        env: &mut Vec<Frame>,
        f: &IrFunction,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let mut evaluated = Vec::with_capacity(args.len());
        for arg in args {
            evaluated.push((arg.name.clone(), self.eval(&arg.value, env, f)?.value()));
        }

        match &callee.kind {
            IrExprKind::Function(name) => self.call_named(name, evaluated, span),
            IrExprKind::Field(base, method) => match &base.kind {
                IrExprKind::Namespace(ns) => self.call_stdlib(ns, method, evaluated, span),
                _ => {
                    let recv = self.eval(base, env, f)?.value();
                    self.call_method(recv, method, evaluated, span)
                }
            },
            _ => Err(RuntimeError::new(span, "call target is not callable")),
        }
    }

    fn call_named(
        &self,
        name: &str,
        args: Vec<(Option<String>, Value)>,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        match name {
            "Some" | "Ok" | "Err" => {
                let values = args.into_iter().map(|(_, v)| v).collect();
                Ok(Value::Variant(name.to_string(), values))
            }
            "capability" => Ok(args
                .into_iter()
                .next()
                .map(|(_, v)| v)
                .unwrap_or(Value::Unit)),
            _ => {
                let f =
                    self.functions.get(name).copied().ok_or_else(|| {
                        RuntimeError::new(span, format!("unknown function `{name}`"))
                    })?;
                let values: Vec<Value> = args.into_iter().map(|(_, v)| v).collect();
                self.call(f, values)
            }
        }
    }

    fn call_stdlib(
        &self,
        namespace: &str,
        method: &str,
        args: Vec<(Option<String>, Value)>,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        match namespace {
            "ui" => {
                let values: Vec<Value> = args.into_iter().map(|(_, v)| v).collect();
                Ok(Value::Record(vec![
                    ("$node".to_string(), Value::Str(method.to_string())),
                    ("args".to_string(), Value::List(values)),
                ]))
            }
            "checks" => Ok(Value::Variant(
                "Validator".to_string(),
                vec![Value::Str(format!("checks.{method}"))],
            )),
            _ => {
                if let Some(host) = self.host {
                    return host
                        .call(namespace, method, &args)
                        .map_err(|message| RuntimeError::new(span, message));
                }
                match namespace {
                    "db" | "fs" | "agents" | "runtime" | "frontend" | "workspaces"
                    | "artifacts" | "patches" | "commands" | "browser" => Err(RuntimeError::new(
                        span,
                        format!(
                            "`{namespace}.{method}` requires a runtime host; \
                                 the deterministic evaluator has no ambient I/O"
                        ),
                    )),
                    other => Err(RuntimeError::new(
                        span,
                        format!("unknown namespace `{other}`"),
                    )),
                }
            }
        }
    }

    fn call_method(
        &self,
        recv: Value,
        method: &str,
        args: Vec<(Option<String>, Value)>,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        match method {
            "to_string" => Ok(Value::Str(recv.display())),
            "len" => match recv {
                Value::List(items) => Ok(Value::Int(items.len() as i64)),
                Value::Str(s) => Ok(Value::Int(s.chars().count() as i64)),
                Value::Bytes(b) => Ok(Value::Int(b.len() as i64)),
                Value::Record(fields) => Ok(Value::Int(fields.len() as i64)),
                other => Err(RuntimeError::new(
                    span,
                    format!("`len` is not defined for {}", kind_of(&other)),
                )),
            },
            "unwrap_or" => {
                let default = args
                    .into_iter()
                    .next()
                    .map(|(_, v)| v)
                    .unwrap_or(Value::Unit);
                match recv {
                    Value::Variant(name, mut values) if name == "Some" && values.len() == 1 => {
                        Ok(values.remove(0))
                    }
                    Value::Variant(name, _) if name == "None" => Ok(default),
                    other => Err(RuntimeError::new(
                        span,
                        format!("`unwrap_or` is not defined for {}", kind_of(&other)),
                    )),
                }
            }
            "unwrap" => match recv {
                Value::Variant(name, mut values) if name == "Some" && values.len() == 1 => {
                    Ok(values.remove(0))
                }
                Value::Variant(name, values) if name == "None" => Err(RuntimeError::new(
                    span,
                    format!("called `unwrap` on {}({})", name, values.len()),
                )),
                other => Err(RuntimeError::new(
                    span,
                    format!("`unwrap` is not defined for {}", kind_of(&other)),
                )),
            },
            "is_some" => Ok(Value::Bool(
                matches!(&recv, Value::Variant(n, _) if n == "Some"),
            )),
            "is_none" => Ok(Value::Bool(
                matches!(&recv, Value::Variant(n, _) if n == "None"),
            )),
            "is_ok" => Ok(Value::Bool(
                matches!(&recv, Value::Variant(n, _) if n == "Ok"),
            )),
            "is_err" => Ok(Value::Bool(
                matches!(&recv, Value::Variant(n, _) if n == "Err"),
            )),
            other => Err(RuntimeError::new(span, format!("unknown method `{other}`"))),
        }
    }

    fn index(&self, base: Value, index: Value, span: Span) -> Result<Value, RuntimeError> {
        match (base, index) {
            (Value::List(items), Value::Int(i)) => {
                let idx = usize::try_from(i).ok().filter(|&i| i < items.len());
                idx.map(|i| items[i].clone())
                    .ok_or_else(|| RuntimeError::new(span, format!("list index {i} out of bounds")))
            }
            (Value::Record(fields), Value::Str(key)) => fields
                .into_iter()
                .find(|(n, _)| *n == key)
                .map(|(_, v)| v)
                .ok_or_else(|| RuntimeError::new(span, format!("no field `{key}`"))),
            (other, _) => Err(RuntimeError::new(
                span,
                format!("cannot index {}", kind_of(&other)),
            )),
        }
    }

    fn unary(&self, op: IrUnaryOp, operand: Value, span: Span) -> Result<Value, RuntimeError> {
        match (op, operand) {
            (IrUnaryOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
            (IrUnaryOp::Neg, Value::Int(i)) => Ok(Value::Int(i.wrapping_neg())),
            (IrUnaryOp::Neg, Value::Float(x)) => Ok(Value::Float(-x)),
            (op, other) => Err(RuntimeError::new(
                span,
                format!("cannot apply {op:?} to {}", kind_of(&other)),
            )),
        }
    }

    fn binary(
        &self,
        op: IrBinaryOp,
        lhs: Value,
        rhs: Value,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        match op {
            IrBinaryOp::Add => match (lhs, rhs) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_add(b))),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
                (Value::Str(a), Value::Str(b)) => Ok(Value::Str(a + &b)),
                (Value::Duration(a), Value::Duration(b)) => Ok(Value::Duration(a + b)),
                (Value::List(mut a), Value::List(b)) => {
                    a.extend(b);
                    Ok(Value::List(a))
                }
                (a, b) => Err(self.mismatch("+", &a, &b, span)),
            },
            IrBinaryOp::Sub => numeric(lhs, rhs, span, |a, b| a.wrapping_sub(b), |a, b| a - b, "-"),
            IrBinaryOp::Mul => numeric(lhs, rhs, span, |a, b| a.wrapping_mul(b), |a, b| a * b, "*"),
            IrBinaryOp::Div => match (lhs, rhs) {
                (Value::Int(_), Value::Int(0)) => {
                    Err(RuntimeError::new(span, "integer division by zero"))
                }
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a / b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
                (a, b) => Err(self.mismatch("/", &a, &b, span)),
            },
            IrBinaryOp::Rem => match (lhs, rhs) {
                (Value::Int(_), Value::Int(0)) => {
                    Err(RuntimeError::new(span, "integer remainder by zero"))
                }
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a % b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a % b)),
                (a, b) => Err(self.mismatch("%", &a, &b, span)),
            },
            IrBinaryOp::Lt => compare(lhs, rhs, span, |o| o == std::cmp::Ordering::Less, "<"),
            IrBinaryOp::Le => compare(lhs, rhs, span, |o| o != std::cmp::Ordering::Greater, "<="),
            IrBinaryOp::Gt => compare(lhs, rhs, span, |o| o == std::cmp::Ordering::Greater, ">"),
            IrBinaryOp::Ge => compare(lhs, rhs, span, |o| o != std::cmp::Ordering::Less, ">="),
            IrBinaryOp::Eq => Ok(Value::Bool(lhs == rhs)),
            IrBinaryOp::Ne => Ok(Value::Bool(lhs != rhs)),
            IrBinaryOp::And => match (lhs, rhs) {
                (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a && b)),
                (a, b) => Err(self.mismatch("and", &a, &b, span)),
            },
            IrBinaryOp::Or => match (lhs, rhs) {
                (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a || b)),
                (a, b) => Err(self.mismatch("or", &a, &b, span)),
            },
        }
    }

    fn mismatch(&self, op: &str, a: &Value, b: &Value, span: Span) -> RuntimeError {
        RuntimeError::new(
            span,
            format!("cannot apply `{op}` to {} and {}", kind_of(a), kind_of(b)),
        )
    }

    fn lookup(&self, name: &str, env: &[Frame], span: Span) -> Result<Value, RuntimeError> {
        for frame in env.iter().rev() {
            if let Some(v) = frame.get(name) {
                return Ok(v.clone());
            }
        }
        Err(RuntimeError::new(span, format!("unbound name `{name}`")))
    }
}

fn variant_matches(pattern: &str, value: &str) -> bool {
    pattern == value
}

fn numeric(
    lhs: Value,
    rhs: Value,
    span: Span,
    int_op: impl Fn(i64, i64) -> i64,
    float_op: impl Fn(f64, f64) -> f64,
    symbol: &str,
) -> Result<Value, RuntimeError> {
    match (lhs, rhs) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(int_op(a, b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(float_op(a, b))),
        (a, b) => Err(RuntimeError::new(
            span,
            format!(
                "cannot apply `{symbol}` to {} and {}",
                kind_of(&a),
                kind_of(&b)
            ),
        )),
    }
}

fn compare(
    lhs: Value,
    rhs: Value,
    span: Span,
    pred: impl Fn(std::cmp::Ordering) -> bool,
    symbol: &str,
) -> Result<Value, RuntimeError> {
    let ordering = match (&lhs, &rhs) {
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Less),
        (Value::Str(a), Value::Str(b)) => a.cmp(b),
        _ => {
            return Err(RuntimeError::new(
                span,
                format!(
                    "cannot compare {} and {} with `{symbol}`",
                    kind_of(&lhs),
                    kind_of(&rhs)
                ),
            ))
        }
    };
    Ok(Value::Bool(pred(ordering)))
}

fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Unit => "unit",
        Value::Bool(_) => "bool",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::Str(_) => "string",
        Value::Bytes(_) => "bytes",
        Value::Duration(_) => "duration",
        Value::Timestamp(_) => "timestamp",
        Value::List(_) => "list",
        Value::Record(_) => "record",
        Value::Variant(_, _) => "variant",
        Value::Function(_) => "function",
    }
}
