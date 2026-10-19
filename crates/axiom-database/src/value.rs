//! Database values and deterministic parameter hashing.

use std::collections::BTreeMap;

use axiom_hash::digest;

use crate::hex;

/// A scalar database value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl Value {
    /// A stable type name for diagnostics and result metadata.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Integer(_) => "integer",
            Value::Real(_) => "real",
            Value::Text(_) => "text",
            Value::Blob(_) => "blob",
        }
    }

    /// The SQLite storage class.
    pub fn storage_class(&self) -> i32 {
        match self {
            Value::Null => 5,
            Value::Integer(_) => 1,
            Value::Real(_) => 2,
            Value::Text(_) => 3,
            Value::Blob(_) => 4,
        }
    }

    /// A deterministic, non-reversible description used in hashes.
    pub fn canonical(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Value::Null => out.push(0),
            Value::Integer(value) => {
                out.push(1);
                out.extend_from_slice(&value.to_le_bytes());
            }
            Value::Real(value) => {
                out.push(2);
                out.extend_from_slice(&value.to_bits().to_le_bytes());
            }
            Value::Text(value) => {
                out.push(3);
                out.extend_from_slice(&(value.len() as u64).to_le_bytes());
                out.extend_from_slice(value.as_bytes());
            }
            Value::Blob(value) => {
                out.push(4);
                out.extend_from_slice(&(value.len() as u64).to_le_bytes());
                out.extend_from_slice(value);
            }
        }
        out
    }
}

/// Named parameters. Keys never include the leading colon.
pub type Params = BTreeMap<String, Value>;

/// A result row with column order preserved.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    columns: Vec<(String, Value)>,
}

impl Row {
    /// Build a row from ordered column/value pairs.
    pub fn new(columns: Vec<(String, Value)>) -> Self {
        Row { columns }
    }

    /// The ordered columns.
    pub fn columns(&self) -> &[(String, Value)] {
        &self.columns
    }

    /// Look up a column by name.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.columns
            .iter()
            .find(|(column, _)| column == name)
            .map(|(_, value)| value)
    }

    /// The number of columns.
    pub fn len(&self) -> usize {
        self.columns.len()
    }

    /// Whether the row has no columns.
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }
}

/// A stable hash of a parameter set that does not reveal the values.
pub fn params_hash(params: &Params) -> String {
    let mut canonical = Vec::new();
    canonical.extend_from_slice(&(params.len() as u64).to_le_bytes());
    for (name, value) in params {
        canonical.extend_from_slice(&(name.len() as u64).to_le_bytes());
        canonical.extend_from_slice(name.as_bytes());
        let value = value.canonical();
        canonical.extend_from_slice(&(value.len() as u64).to_le_bytes());
        canonical.extend_from_slice(&value);
    }
    hex(&digest(&canonical))
}
