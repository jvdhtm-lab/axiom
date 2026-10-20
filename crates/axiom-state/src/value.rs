//! Canonical frontend-state values and deterministic encoding.

use std::fmt::Write as _;

/// A typed frontend-state value.
///
/// Persisted state is a normal record value, not a special language construct.
/// This mirrors the runtime value model so a record produced by `init` /
/// `handle_event` can be stored unchanged.
#[derive(Debug, Clone, PartialEq)]
pub enum StateValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<StateValue>),
    /// Ordered record fields.
    Record(Vec<(String, StateValue)>),
    /// A named constructor with arguments, e.g. `AppError.NotFound`.
    Variant(String, Vec<StateValue>),
}

impl StateValue {
    /// A canonical, deterministic encoding used for size limits and hashes.
    ///
    /// Floats are encoded as IEEE-754 bits so the encoding never depends on
    /// formatting or locale.
    pub fn encode(&self) -> String {
        let mut out = String::new();
        self.write_to(&mut out);
        out
    }

    fn write_to(&self, out: &mut String) {
        match self {
            StateValue::Null => out.push('n'),
            StateValue::Bool(value) => {
                out.push('b');
                out.push(if *value { '1' } else { '0' });
            }
            StateValue::Int(value) => {
                let _ = write!(out, "i{value}");
            }
            StateValue::Float(value) => {
                let _ = write!(out, "f{:016x}", value.to_bits());
            }
            StateValue::Str(value) => {
                let _ = write!(out, "s{}:{value}", value.len());
            }
            StateValue::List(items) => {
                let _ = write!(out, "l{}[", items.len());
                for item in items {
                    item.write_to(out);
                }
                out.push(']');
            }
            StateValue::Record(fields) => {
                let _ = write!(out, "r{}{{", fields.len());
                for (name, value) in fields {
                    let _ = write!(out, "k{}:{name}", name.len());
                    value.write_to(out);
                }
                out.push('}');
            }
            StateValue::Variant(name, args) => {
                let _ = write!(out, "v{}:{name}(", name.len());
                for arg in args {
                    arg.write_to(out);
                }
                out.push(')');
            }
        }
    }

    pub fn byte_len(&self) -> usize {
        self.encode().len()
    }

    /// Field names that must never be persisted as client state.
    pub fn forbidden_key(&self) -> Option<String> {
        match self {
            StateValue::Record(fields) => {
                for (name, value) in fields {
                    if is_forbidden(name) {
                        return Some(name.clone());
                    }
                    if let Some(found) = value.forbidden_key() {
                        return Some(found);
                    }
                }
                None
            }
            StateValue::List(items) => items.iter().find_map(StateValue::forbidden_key),
            StateValue::Variant(_, args) => args.iter().find_map(StateValue::forbidden_key),
            _ => None,
        }
    }

    /// The record field with `name`, if this is a record.
    pub fn field(&self, name: &str) -> Option<&StateValue> {
        match self {
            StateValue::Record(fields) => fields
                .iter()
                .find(|(field, _)| field == name)
                .map(|(_, value)| value),
            _ => None,
        }
    }
}

/// Whether a state field name denotes secret or authorization material.
///
/// Matching is case-insensitive and checks the whole name; state may still use
/// names like `author` or `token_count`, which are not credentials.
pub fn is_forbidden(name: &str) -> bool {
    const FORBIDDEN: &[&str] = &[
        "token",
        "access_token",
        "refresh_token",
        "id_token",
        "api_key",
        "apikey",
        "secret",
        "password",
        "passwd",
        "authorization",
        "bearer",
        "cookie",
        "credential",
        "credentials",
        "capability",
        "capabilities",
        "grant",
        "grants",
        "session_id",
    ];
    let lower = name.to_ascii_lowercase();
    FORBIDDEN.contains(&lower.as_str())
}
