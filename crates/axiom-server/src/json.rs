//! Minimal JSON bridge for runtime values.
//!
//! The server deliberately has no serialization dependency. Values use a small,
//! stable mapping:
//!
//! - `Unit` → `null`, primitives → JSON primitives
//! - `Bytes` → `{"$bytes":"hex"}` (or an array of numbers on input)
//! - `Duration` → `{"$duration_ms":n}`
//! - `Timestamp` → `{"$timestamp":n}`
//! - `Variant` → `{"$variant":"Name","$args":[...]}`
//! - `Record` → object, `List` → array
//!
//! Function values are not serializable.

use std::fmt::{self, Write as _};

use axiom_runtime::Value;

/// A JSON encode/decode error with a stable, non-secret message body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonError {
    pub message: String,
}

impl JsonError {
    fn new(message: impl Into<String>) -> Self {
        JsonError {
            message: message.into(),
        }
    }
}

impl fmt::Display for JsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for JsonError {}

/// Encode a runtime value as JSON.
pub fn to_json(value: &Value) -> Result<String, JsonError> {
    let mut out = String::new();
    write_value(&mut out, value)?;
    Ok(out)
}

fn write_value(out: &mut String, value: &Value) -> Result<(), JsonError> {
    match value {
        Value::Unit => out.push_str("null"),
        Value::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        Value::Int(value) => {
            let _ = write!(out, "{value}");
        }
        Value::Float(value) => {
            if !value.is_finite() {
                return Err(JsonError::new("non-finite float is not valid JSON"));
            }
            let _ = write!(out, "{value:?}");
        }
        Value::Str(value) => write_string(out, value),
        Value::Bytes(bytes) => {
            out.push_str("{\"$bytes\":\"");
            for byte in bytes {
                let _ = write!(out, "{byte:02x}");
            }
            out.push_str("\"}");
        }
        Value::Duration(millis) => {
            let _ = write!(out, "{{\"$duration_ms\":{millis}}}");
        }
        Value::Timestamp(timestamp) => {
            let _ = write!(out, "{{\"$timestamp\":{timestamp}}}");
        }
        Value::List(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_value(out, item)?;
            }
            out.push(']');
        }
        Value::Record(fields) => {
            out.push('{');
            for (index, (name, value)) in fields.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_string(out, name);
                out.push(':');
                write_value(out, value)?;
            }
            out.push('}');
        }
        Value::Variant(name, args) => {
            out.push_str("{\"$variant\":");
            write_string(out, name);
            if !args.is_empty() {
                out.push_str(",\"$args\":[");
                for (index, arg) in args.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    write_value(out, arg)?;
                }
                out.push(']');
            }
            out.push('}');
        }
        Value::Function(_) => {
            return Err(JsonError::new("function values cannot be encoded as JSON"));
        }
    }
    Ok(())
}

fn write_string(out: &mut String, value: &str) {
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            character if character < '\u{20}' => {
                let _ = write!(out, "\\u{:04x}", character as u32);
            }
            character => out.push(character),
        }
    }
    out.push('"');
}

/// Decode JSON into a runtime value.
pub fn from_json(text: &str) -> Result<Value, JsonError> {
    let mut parser = Parser {
        chars: text.chars().collect(),
        position: 0,
    };
    parser.skip_whitespace();
    let value = parser.value()?;
    parser.skip_whitespace();
    if parser.position != parser.chars.len() {
        return Err(JsonError::new("trailing characters after JSON value"));
    }
    Ok(value)
}

struct Parser {
    chars: Vec<char>,
    position: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.position).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.position += 1;
        Some(character)
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(' ' | '\n' | '\r' | '\t')) {
            self.position += 1;
        }
    }

    fn expect(&mut self, expected: char) -> Result<(), JsonError> {
        match self.bump() {
            Some(actual) if actual == expected => Ok(()),
            Some(actual) => Err(JsonError::new(format!(
                "expected `{expected}`, found `{actual}` at character {}",
                self.position
            ))),
            None => Err(JsonError::new(format!(
                "expected `{expected}`, found end of input"
            ))),
        }
    }

    fn literal(&mut self, text: &str, value: Value) -> Result<Value, JsonError> {
        for expected in text.chars() {
            self.expect(expected)?;
        }
        Ok(value)
    }

    fn value(&mut self) -> Result<Value, JsonError> {
        self.skip_whitespace();
        match self.peek() {
            Some('n') => self.literal("null", Value::Unit),
            Some('t') => self.literal("true", Value::Bool(true)),
            Some('f') => self.literal("false", Value::Bool(false)),
            Some('"') => Ok(Value::Str(self.string()?)),
            Some('[') => self.array(),
            Some('{') => self.object(),
            Some('-' | '0'..='9') => self.number(),
            Some(other) => Err(JsonError::new(format!(
                "unexpected character `{other}` at {}",
                self.position
            ))),
            None => Err(JsonError::new("unexpected end of input")),
        }
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.expect('"')?;
        let mut out = String::new();
        loop {
            match self.bump() {
                Some('"') => return Ok(out),
                Some('\\') => match self.bump() {
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some('/') => out.push('/'),
                    Some('b') => out.push('\u{08}'),
                    Some('f') => out.push('\u{0c}'),
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('t') => out.push('\t'),
                    Some('u') => {
                        let code = self.hex4()?;
                        let character = if (0xD800..=0xDBFF).contains(&code) {
                            if self.bump() != Some('\\') || self.bump() != Some('u') {
                                return Err(JsonError::new("unpaired high surrogate"));
                            }
                            let low = self.hex4()?;
                            if !(0xDC00..=0xDFFF).contains(&low) {
                                return Err(JsonError::new("invalid low surrogate"));
                            }
                            let value = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                            char::from_u32(value)
                        } else {
                            char::from_u32(code)
                        };
                        out.push(
                            character.ok_or_else(|| JsonError::new("invalid unicode escape"))?,
                        );
                    }
                    Some(other) => {
                        return Err(JsonError::new(format!("invalid escape `\\{other}`")));
                    }
                    None => return Err(JsonError::new("unterminated escape")),
                },
                Some(character) if character < '\u{20}' => {
                    return Err(JsonError::new("control character in string"));
                }
                Some(character) => out.push(character),
                None => return Err(JsonError::new("unterminated string")),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let mut value = 0u32;
        for _ in 0..4 {
            let character = self
                .bump()
                .ok_or_else(|| JsonError::new("short unicode escape"))?;
            let digit = character
                .to_digit(16)
                .ok_or_else(|| JsonError::new("invalid unicode escape digit"))?;
            value = (value << 4) | digit;
        }
        Ok(value)
    }

    fn number(&mut self) -> Result<Value, JsonError> {
        let start = self.position;
        if self.peek() == Some('-') {
            self.position += 1;
        }
        let mut float = false;
        while let Some(character) = self.peek() {
            match character {
                '0'..='9' => self.position += 1,
                '.' | 'e' | 'E' | '+' | '-' => {
                    float = true;
                    self.position += 1;
                }
                _ => break,
            }
        }
        let text: String = self.chars[start..self.position].iter().collect();
        if text.is_empty() || text == "-" {
            return Err(JsonError::new("invalid number"));
        }
        if float {
            text.parse::<f64>()
                .map(Value::Float)
                .map_err(|_| JsonError::new("invalid float"))
        } else {
            text.parse::<i64>()
                .map(Value::Int)
                .map_err(|_| JsonError::new("invalid integer"))
        }
    }

    fn array(&mut self) -> Result<Value, JsonError> {
        self.expect('[')?;
        self.skip_whitespace();
        let mut items = Vec::new();
        if self.peek() == Some(']') {
            self.position += 1;
            return Ok(Value::List(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_whitespace();
            match self.bump() {
                Some(',') => continue,
                Some(']') => return Ok(Value::List(items)),
                Some(other) => {
                    return Err(JsonError::new(format!(
                        "expected `,` or `]`, found `{other}`"
                    )));
                }
                None => return Err(JsonError::new("unterminated array")),
            }
        }
    }

    fn object(&mut self) -> Result<Value, JsonError> {
        self.expect('{')?;
        self.skip_whitespace();
        let mut fields = Vec::new();
        if self.peek() == Some('}') {
            self.position += 1;
            return Ok(Value::Record(fields));
        }
        loop {
            self.skip_whitespace();
            let name = self.string()?;
            self.skip_whitespace();
            self.expect(':')?;
            let value = self.value()?;
            fields.push((name, value));
            self.skip_whitespace();
            match self.bump() {
                Some(',') => continue,
                Some('}') => return tagged(fields),
                Some(other) => {
                    return Err(JsonError::new(format!(
                        "expected `,` or `}}`, found `{other}`"
                    )));
                }
                None => return Err(JsonError::new("unterminated object")),
            }
        }
    }
}

fn tagged(fields: Vec<(String, Value)>) -> Result<Value, JsonError> {
    let variant = fields
        .iter()
        .find(|(name, _)| name == "$variant")
        .and_then(|(_, value)| match value {
            Value::Str(name) => Some(name.clone()),
            _ => None,
        });
    if let Some(name) = variant {
        let args = fields
            .iter()
            .find(|(name, _)| name == "$args")
            .and_then(|(_, value)| match value {
                Value::List(items) => Some(items.clone()),
                _ => None,
            })
            .unwrap_or_default();
        return Ok(Value::Variant(name, args));
    }
    if let Some((_, Value::Str(hex))) = fields.iter().find(|(name, _)| name == "$bytes") {
        let bytes = decode_hex(hex)?;
        return Ok(Value::Bytes(bytes));
    }
    if let Some((_, Value::Int(millis))) = fields.iter().find(|(name, _)| name == "$duration_ms") {
        if *millis >= 0 {
            return Ok(Value::Duration(*millis as u64));
        }
    }
    if let Some((_, Value::Int(timestamp))) = fields.iter().find(|(name, _)| name == "$timestamp") {
        return Ok(Value::Timestamp(*timestamp));
    }
    Ok(Value::Record(fields))
}

fn decode_hex(text: &str) -> Result<Vec<u8>, JsonError> {
    if !text.len().is_multiple_of(2) {
        return Err(JsonError::new("byte hex string has odd length"));
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for index in (0..bytes.len()).step_by(2) {
        let high = (bytes[index] as char)
            .to_digit(16)
            .ok_or_else(|| JsonError::new("invalid byte hex digit"))?;
        let low = (bytes[index + 1] as char)
            .to_digit(16)
            .ok_or_else(|| JsonError::new("invalid byte hex digit"))?;
        out.push(((high << 4) | low) as u8);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_round_trip() {
        for value in [
            Value::Unit,
            Value::Bool(true),
            Value::Int(-42),
            Value::Float(1.5),
            Value::Str("hello \"world\"\n".to_string()),
            Value::List(vec![Value::Int(1), Value::Str("two".to_string())]),
            Value::Record(vec![
                ("id".to_string(), Value::Int(7)),
                ("ok".to_string(), Value::Bool(false)),
            ]),
            Value::Variant("Ok".to_string(), vec![Value::Int(1)]),
            Value::Bytes(vec![0, 10, 255]),
            Value::Duration(120_000),
            Value::Timestamp(1_700_000_000),
        ] {
            let text = to_json(&value).unwrap();
            let decoded = from_json(&text).unwrap();
            assert_eq!(value, decoded, "{text}");
        }
    }
}
