//! Single-statement SQL analysis.
//!
//! This is a conservative scanner, not a full SQL parser. It is defense in
//! depth: the database role remains the security boundary. It refuses
//! multiple statements, anonymous placeholders, transaction control, engine
//! attachment, and tables outside the configured scope, and computes a stable
//! fingerprint used by write approvals.

use std::collections::{BTreeMap, BTreeSet};

use axiom_hash::digest;

use crate::hex;
use crate::value::Value;
use crate::DbError;

/// The class of a SQL statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlClass {
    /// Returns rows (`SELECT`, `VALUES`, read-only `EXPLAIN`).
    Read,
    /// Mutates data (`INSERT`, `UPDATE`, `DELETE`, `REPLACE`).
    Write,
    /// Changes schema or engine state; only migrations may use it.
    Schema,
}

impl SqlClass {
    /// The stable name used in diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            SqlClass::Read => "read",
            SqlClass::Write => "write",
            SqlClass::Schema => "schema",
        }
    }
}

/// A validated SQL statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlStatement {
    pub class: SqlClass,
    /// Placeholder names without the leading colon.
    pub placeholders: BTreeSet<String>,
    /// Referenced table names, lowercased.
    pub tables: BTreeSet<String>,
    /// Comment-free, whitespace-normalized SQL.
    pub normalized: String,
    /// SHA-256 of `normalized`.
    pub fingerprint: String,
}

/// Analyze SQL and require a specific statement class.
pub fn validate(
    sql: &str,
    required: SqlClass,
    params: &BTreeMap<String, Value>,
    allowed_tables: &[String],
) -> Result<SqlStatement, DbError> {
    let statement = analyze(sql)?;
    if statement.class != required {
        return Err(DbError::WrongClass {
            expected: required.as_str(),
            actual: statement.class.as_str(),
        });
    }
    if statement.class == SqlClass::Schema {
        return Err(DbError::RoleDenied {
            role: "writer",
            operation: "schema",
        });
    }
    for table in &statement.tables {
        if table.starts_with("sqlite_") {
            return Err(DbError::Forbidden("sqlite system catalog"));
        }
        if !allowed_tables.is_empty()
            && !allowed_tables
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(table))
        {
            return Err(DbError::UnknownTable(table.clone()));
        }
    }
    for placeholder in &statement.placeholders {
        if !params.contains_key(placeholder) {
            return Err(DbError::PlaceholderMismatch(format!(
                "missing parameter `{placeholder}`"
            )));
        }
    }
    for name in params.keys() {
        if !statement.placeholders.contains(name) {
            return Err(DbError::PlaceholderMismatch(format!(
                "unused parameter `{name}`"
            )));
        }
    }
    Ok(statement)
}

/// Analyze SQL without checking parameters or table scope.
pub fn analyze(sql: &str) -> Result<SqlStatement, DbError> {
    let tokens = tokenize(sql)?;
    if tokens.is_empty() {
        return Err(DbError::InvalidSql);
    }
    check_statement_count(&tokens)?;

    let words: Vec<&Token> = tokens.iter().collect();
    let first = first_word(&words).ok_or(DbError::InvalidSql)?;
    let class = match first.as_str() {
        "select" | "values" | "explain" => SqlClass::Read,
        "with" => {
            if has_top_level_write(&words) {
                SqlClass::Write
            } else {
                SqlClass::Read
            }
        }
        "insert" | "update" | "delete" | "replace" | "upsert" => SqlClass::Write,
        "create" | "alter" | "drop" | "pragma" | "vacuum" | "reindex" | "analyze" => {
            SqlClass::Schema
        }
        "attach" => return Err(DbError::Forbidden("attach")),
        "detach" => return Err(DbError::Forbidden("detach")),
        "begin" | "commit" | "end" | "rollback" | "savepoint" | "release" => {
            return Err(DbError::Forbidden("transaction control"))
        }
        _ => return Err(DbError::InvalidSql),
    };

    for (index, token) in words.iter().enumerate() {
        if token.word().as_deref() == Some("load_extension")
            && words
                .get(index + 1)
                .is_some_and(|next| next.symbol() == Some('('))
        {
            return Err(DbError::Forbidden("load_extension"));
        }
    }

    let placeholders = collect_placeholders(&words)?;
    let ctes = collect_ctes(&words);
    let tables = collect_tables(&words, &ctes);
    let normalized = normalize(&words);

    Ok(SqlStatement {
        class,
        placeholders,
        tables,
        fingerprint: hex(&digest(normalized.as_bytes())),
        normalized,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    Word(String),
    Quoted(String),
    Text(String),
    Number(String),
    Symbol(char),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    kind: Kind,
}

impl Token {
    fn word(&self) -> Option<String> {
        match &self.kind {
            Kind::Word(word) if is_keyword_like(word) => Some(word.to_ascii_lowercase()),
            _ => None,
        }
    }

    fn ident(&self) -> Option<&str> {
        match &self.kind {
            Kind::Word(word) => Some(word.as_str()),
            Kind::Quoted(word) => Some(word.as_str()),
            _ => None,
        }
    }

    fn symbol(&self) -> Option<char> {
        match &self.kind {
            Kind::Symbol(symbol) => Some(*symbol),
            _ => None,
        }
    }
}

/// Keywords are compared case-insensitively; identifiers keep their case.
fn is_keyword_like(word: &str) -> bool {
    !word.is_empty() && word.chars().all(|c| c.is_ascii_alphabetic() || c == '_')
}

fn first_word(tokens: &[&Token]) -> Option<String> {
    tokens
        .iter()
        .find_map(|token| token.ident().map(|word| word.to_ascii_lowercase()))
}

fn has_top_level_write(tokens: &[&Token]) -> bool {
    let mut depth = 0i32;
    for token in tokens {
        match token.symbol() {
            Some('(') => depth += 1,
            Some(')') => depth = (depth - 1).max(0),
            _ => {
                if depth == 0
                    && matches!(
                        token.word().as_deref(),
                        Some("insert" | "update" | "delete" | "replace")
                    )
                {
                    return true;
                }
            }
        }
    }
    false
}

fn check_statement_count(tokens: &[Token]) -> Result<(), DbError> {
    if tokens.iter().all(|token| token.symbol() == Some(';')) {
        return Err(DbError::InvalidSql);
    }
    let mut seen = false;
    for token in tokens {
        if token.symbol() == Some(';') {
            seen = true;
        } else if seen {
            return Err(DbError::MultipleStatements);
        }
    }
    Ok(())
}

fn collect_placeholders(tokens: &[&Token]) -> Result<BTreeSet<String>, DbError> {
    let mut out = BTreeSet::new();
    for (index, token) in tokens.iter().enumerate() {
        match token.symbol() {
            Some('?') => {
                return Err(DbError::PlaceholderMismatch(
                    "anonymous `?` placeholders are forbidden; use `:name`".to_string(),
                ))
            }
            Some(':') => {
                let Some(next) = tokens.get(index + 1) else {
                    return Err(DbError::PlaceholderMismatch(
                        "dangling `:` placeholder".to_string(),
                    ));
                };
                let Some(name) = next.ident() else {
                    return Err(DbError::PlaceholderMismatch(
                        "placeholder must be `:name`".to_string(),
                    ));
                };
                if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                    return Err(DbError::PlaceholderMismatch(
                        "placeholder must start with a letter or underscore".to_string(),
                    ));
                }
                out.insert(name.to_string());
            }
            _ => {}
        }
    }
    Ok(out)
}

/// Names defined by a top-level `WITH name AS (...)` list.
fn collect_ctes(tokens: &[&Token]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut index = 0;
    let mut saw_with = false;
    let mut depth = 0i32;
    while index < tokens.len() {
        let token = tokens[index];
        match token.word().as_deref() {
            Some("with") if depth == 0 => {
                saw_with = true;
                index += 1;
                continue;
            }
            _ => {}
        }
        if !saw_with {
            index += 1;
            continue;
        }
        match token.symbol() {
            Some('(') => {
                depth += 1;
                index += 1;
            }
            Some(')') => {
                depth = (depth - 1).max(0);
                index += 1;
            }
            _ => {
                if depth == 0 {
                    if let Some(name) = token.ident() {
                        let mut look = index + 1;
                        if tokens.get(look).and_then(|t| t.symbol()) == Some('(') {
                            look += 1;
                            while look < tokens.len()
                                && tokens[look].symbol() != Some(')')
                                && tokens[look].symbol() != Some('(')
                            {
                                look += 1;
                            }
                            if tokens.get(look).and_then(|t| t.symbol()) == Some(')') {
                                look += 1;
                            }
                        }
                        if tokens
                            .get(look)
                            .and_then(|t| t.word())
                            .is_some_and(|word| word == "as")
                        {
                            out.insert(name.to_ascii_lowercase());
                        }
                    }
                }
                index += 1;
            }
        }
    }
    out
}

fn collect_tables(tokens: &[&Token], ctes: &BTreeSet<String>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (index, token) in tokens.iter().enumerate() {
        let keyword = match token.word() {
            Some(word) => word,
            None => continue,
        };
        if !matches!(
            keyword.as_str(),
            "from" | "join" | "into" | "update" | "table"
        ) {
            continue;
        }
        let mut look = index + 1;
        while tokens
            .get(look)
            .is_some_and(|t| t.word().as_deref() == Some("if"))
        {
            look += 2;
        }
        let Some(name) = tokens.get(look).and_then(|t| t.ident()) else {
            continue;
        };
        if matches!(
            name.to_ascii_lowercase().as_str(),
            "select" | "values" | "exists" | "table"
        ) {
            continue;
        }
        if tokens.get(look).and_then(|t| t.symbol()) == Some('(') {
            continue;
        }
        let mut table = name.to_string();
        if tokens.get(look + 1).and_then(|t| t.symbol()) == Some('.') {
            if let Some(actual) = tokens.get(look + 2).and_then(|t| t.ident()) {
                table = actual.to_string();
            }
        }
        let lower = table.to_ascii_lowercase();
        if ctes.contains(&lower) {
            continue;
        }
        out.insert(lower);
    }
    out
}

fn normalize(tokens: &[&Token]) -> String {
    let mut out = String::new();
    let mut previous_was_word = false;
    for token in tokens {
        if token.symbol() == Some(';') {
            continue;
        }
        let text = match &token.kind {
            Kind::Word(word) => {
                if is_keyword_like(word) {
                    word.to_ascii_lowercase()
                } else {
                    word.clone()
                }
            }
            Kind::Quoted(inner) => format!("\"{}\"", inner.to_ascii_lowercase()),
            Kind::Text(text) => format!("'{text}'"),
            Kind::Number(number) => number.clone(),
            Kind::Symbol(symbol) => symbol.to_string(),
        };
        let is_word = matches!(
            token.kind,
            Kind::Word(_) | Kind::Quoted(_) | Kind::Number(_)
        );
        if !out.is_empty() {
            let previous = out.chars().last().unwrap_or(' ');
            let needs_space = (previous_was_word && is_word)
                || (is_word && previous.is_alphanumeric())
                || (previous_is_ident_char(previous) && is_word);
            if needs_space {
                out.push(' ');
            }
        }
        out.push_str(&text);
        previous_was_word = is_word;
    }
    out
}

fn previous_is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '"'
}

fn tokenize(sql: &str) -> Result<Vec<Token>, DbError> {
    let chars: Vec<char> = sql.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        if c.is_whitespace() {
            index += 1;
            continue;
        }
        if c == '-' && chars.get(index + 1) == Some(&'-') {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if c == '/' && chars.get(index + 1) == Some(&'*') {
            index += 2;
            let mut closed = false;
            while index + 1 < chars.len() {
                if chars[index] == '*' && chars[index + 1] == '/' {
                    index += 2;
                    closed = true;
                    break;
                }
                index += 1;
            }
            if !closed {
                return Err(DbError::InvalidSql);
            }
            continue;
        }
        if c == '\'' {
            let mut text = String::new();
            index += 1;
            loop {
                if index >= chars.len() {
                    return Err(DbError::InvalidSql);
                }
                if chars[index] == '\'' {
                    if chars.get(index + 1) == Some(&'\'') {
                        text.push('\'');
                        index += 2;
                        continue;
                    }
                    index += 1;
                    break;
                }
                text.push(chars[index]);
                index += 1;
            }
            tokens.push(Token {
                kind: Kind::Text(text),
            });
            continue;
        }
        if c == '"' || c == '`' || c == '[' {
            let close = match c {
                '"' => '"',
                '`' => '`',
                _ => ']',
            };
            let mut text = String::new();
            index += 1;
            loop {
                if index >= chars.len() {
                    return Err(DbError::InvalidSql);
                }
                if chars[index] == close {
                    if close != ']' && chars.get(index + 1) == Some(&close) {
                        text.push(close);
                        index += 2;
                        continue;
                    }
                    index += 1;
                    break;
                }
                text.push(chars[index]);
                index += 1;
            }
            tokens.push(Token {
                kind: Kind::Quoted(text),
            });
            continue;
        }
        if c.is_ascii_digit() {
            let mut number = String::new();
            while index < chars.len() && (chars[index].is_ascii_digit() || chars[index] == '.') {
                number.push(chars[index]);
                index += 1;
            }
            tokens.push(Token {
                kind: Kind::Number(number),
            });
            continue;
        }
        if is_ident_start(c) {
            let mut word = String::new();
            while index < chars.len() && is_ident_char(chars[index]) {
                word.push(chars[index]);
                index += 1;
            }
            tokens.push(Token {
                kind: Kind::Word(word),
            });
            continue;
        }
        if c == ':' && chars.get(index + 1) == Some(&':') {
            index += 2;
            continue;
        }
        tokens.push(Token {
            kind: Kind::Symbol(c),
        });
        index += 1;
    }
    Ok(tokens)
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}
