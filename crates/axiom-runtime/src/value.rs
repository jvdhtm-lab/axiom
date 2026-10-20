//! Runtime values and their deterministic rendering.

use std::fmt::Write as _;

/// A runtime value produced by the deterministic evaluator.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Unit,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Bytes(Vec<u8>),
    Duration(u64),
    Timestamp(i64),
    List(Vec<Value>),
    /// Ordered record fields.
    Record(Vec<(String, Value)>),
    /// A named constructor with arguments, e.g. `Ok(v)` or `AppError.NotFound`.
    Variant(String, Vec<Value>),
    /// A reference to a declared function, passed to durable-entry APIs.
    Function(String),
}

impl Value {
    /// A stable, source-like rendering. Used for `.to_string()` and CLI output.
    pub fn display(&self) -> String {
        let mut out = String::new();
        self.write_to(&mut out);
        out
    }

    fn write_to(&self, out: &mut String) {
        match self {
            Value::Unit => out.push_str("()"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Int(i) => {
                let _ = write!(out, "{i}");
            }
            Value::Float(f) => {
                let _ = write!(out, "{f}");
            }
            Value::Str(s) => write_quoted(out, s),
            Value::Bytes(b) => {
                let _ = write!(out, "<{} bytes>", b.len());
            }
            Value::Duration(ms) => {
                let _ = write!(out, "{ms}ms");
            }
            Value::Timestamp(t) => {
                let _ = write!(out, "{t}");
            }
            Value::List(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    item.write_to(out);
                }
                out.push(']');
            }
            Value::Record(fields) => {
                out.push_str("{ ");
                for (i, (name, value)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    let _ = write!(out, "{name}: ");
                    value.write_to(out);
                }
                out.push_str(" }");
            }
            Value::Variant(name, args) => {
                out.push_str(name);
                if !args.is_empty() {
                    out.push('(');
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        arg.write_to(out);
                    }
                    out.push(')');
                }
            }
            Value::Function(name) => {
                let _ = write!(out, "<fn {name}>");
            }
        }
    }

    /// The `$node` kind if this value is a UI node built by `ui.*`.
    pub fn ui_node(&self) -> Option<&str> {
        match self {
            Value::Record(fields) => {
                fields
                    .iter()
                    .find(|(n, _)| n == "$node")
                    .and_then(|(_, v)| match v {
                        Value::Str(s) => Some(s.as_str()),
                        _ => None,
                    })
            }
            _ => None,
        }
    }
}

fn write_quoted(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
}
