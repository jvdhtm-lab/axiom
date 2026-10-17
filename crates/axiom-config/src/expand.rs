//! Explicit `${...}` placeholder expansion.
//!
//! Axiom never relies on a YAML parser to substitute environment references.
//! Expansion is a separate, testable step with documented rules:
//!
//! - `${NAME}` substitutes the variable `NAME`; it is an error if `NAME` is
//!   unset or empty.
//! - `${NAME:-default}` substitutes `NAME`, falling back to `default` when
//!   `NAME` is unset or empty.
//! - `$$` is an escaped literal `$`.
//! - Any other `$` is a malformed placeholder.
//!
//! Placeholders are only expanded in string scalars. There is no recursion and
//! no expression evaluation.

use std::fmt;

/// A variable name: a leading letter or underscore followed by letters,
/// digits, or underscores.
fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpandError {
    /// `${NAME}` was used but `NAME` is unset or empty.
    MissingVariable(String),
    /// A `$` was not followed by a valid placeholder or `$`.
    MalformedPlaceholder(String),
}

impl fmt::Display for ExpandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExpandError::MissingVariable(name) => write!(
                f,
                "environment variable `{name}` is not set; provide it or a default via `${{{name}:-...}}`"
            ),
            ExpandError::MalformedPlaceholder(text) => {
                write!(f, "malformed placeholder: {text}")
            }
        }
    }
}

/// Expand placeholders in `input` using `env` as the variable source.
pub fn expand(input: &str, env: &dyn Fn(&str) -> Option<String>) -> Result<String, ExpandError> {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'$' {
            // Copy the current char (up to the next `$`) verbatim.
            let ch = input[i..].chars().next().expect("valid char boundary");
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }

        match bytes.get(i + 1) {
            Some(b'$') => {
                out.push('$');
                i += 2;
            }
            Some(b'{') => {
                let rest = &input[i + 2..];
                let close = rest.find('}').ok_or_else(|| {
                    ExpandError::MalformedPlaceholder(format!("`${{{rest}` is missing `}}`"))
                })?;
                let body = &rest[..close];
                let (name, default) = match body.split_once(":-") {
                    Some((name, default)) => (name, Some(default)),
                    None => (body, None),
                };
                if !is_valid_name(name) {
                    return Err(ExpandError::MalformedPlaceholder(format!(
                        "`${{{body}}}` is not a valid `${{NAME}}` or `${{NAME:-default}}`"
                    )));
                }
                let value = env(name).filter(|v| !v.is_empty());
                match (value, default) {
                    (Some(value), _) => out.push_str(&value),
                    (None, Some(default)) => out.push_str(default),
                    (None, None) => return Err(ExpandError::MissingVariable(name.to_string())),
                }
                i += 2 + close + 1;
            }
            _ => {
                return Err(ExpandError::MalformedPlaceholder(format!(
                    "`{}` starts a placeholder with neither `{{` nor `$`",
                    &input[i..i + 1]
                )));
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_string())
        }
    }

    #[test]
    fn substitutes_set_variables() {
        let e = env(&[("HOST", "http://x")]);
        assert_eq!(expand("url=${HOST}", &e).unwrap(), "url=http://x");
    }

    #[test]
    fn default_is_used_when_unset_or_empty() {
        let e = env(&[("EMPTY", "")]);
        assert_eq!(expand("${MISSING:-fallback}", &e).unwrap(), "fallback");
        assert_eq!(expand("${EMPTY:-fallback}", &e).unwrap(), "fallback");
        assert_eq!(
            expand("${EMPTY:-fallback}", &env(&[("EMPTY", "x")])).unwrap(),
            "x"
        );
    }

    #[test]
    fn missing_required_variable_is_an_error() {
        let e = env(&[]);
        assert_eq!(
            expand("${NOPE}", &e),
            Err(ExpandError::MissingVariable("NOPE".to_string()))
        );
    }

    #[test]
    fn double_dollar_is_an_escape() {
        let e = env(&[("X", "1")]);
        assert_eq!(expand("cost=$$5 and ${X}", &e).unwrap(), "cost=$5 and 1");
    }

    #[test]
    fn malformed_placeholders_are_rejected() {
        let e = env(&[]);
        assert!(matches!(
            expand("${UNCLOSED", &e),
            Err(ExpandError::MalformedPlaceholder(_))
        ));
        assert!(matches!(
            expand("${1BAD}", &e),
            Err(ExpandError::MalformedPlaceholder(_))
        ));
        assert!(matches!(
            expand("a lone $ here", &e),
            Err(ExpandError::MalformedPlaceholder(_))
        ));
    }
}
