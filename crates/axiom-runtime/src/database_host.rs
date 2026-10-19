//! A host that exposes parameterized database calls to Axiom code.
//!
//! Only `db.query`, `db.query_one`, and `db.execute` are reachable. Schema,
//! provisioning, migration, and role-audit APIs are deliberately not exposed:
//! they require separate authority and are driven by the operator tooling.
//! Write approvals are supplied out of band and bound to the exact request.

use std::cell::RefCell;

use axiom_database::sql::{self, SqlClass};
use axiom_database::value::{params_hash, Params, Row};
use axiom_database::{
    Actor, ApprovalLedger, AuditLog, Database, DatabaseSpec, DbError, Value as DbValue,
    WriteApproval, WriteBinding, WriteRequest,
};

use crate::host::Host;
use crate::value::Value;

/// A host backed by one configured database.
pub struct DatabaseHost {
    spec: DatabaseSpec,
    database: Database,
    actor: Actor,
    audit: AuditLog,
    ledger: RefCell<ApprovalLedger>,
    approvals: RefCell<Vec<WriteApproval>>,
}

impl DatabaseHost {
    /// Open a database and bind it to an actor and audit log.
    pub fn open(spec: DatabaseSpec, actor: Actor, audit: AuditLog) -> Result<Self, DbError> {
        let database = Database::open(spec.clone())?;
        Ok(DatabaseHost {
            spec,
            database,
            actor,
            audit,
            ledger: RefCell::new(ApprovalLedger::memory()),
            approvals: RefCell::new(Vec::new()),
        })
    }

    /// Register an approval the run may spend.
    pub fn approve(&self, approval: WriteApproval) {
        self.approvals.borrow_mut().push(approval);
    }

    /// The underlying database handle.
    pub fn database(&self) -> &Database {
        &self.database
    }

    /// The audit log this host writes to.
    pub fn audit(&self) -> &AuditLog {
        &self.audit
    }

    fn query(&self, args: &[(Option<String>, Value)]) -> Result<Value, String> {
        let (sql_text, params) = self.read_args(args)?;
        self.database
            .query(&self.actor, &sql_text, &params, &self.audit)
            .map(|rows| Value::List(rows.iter().map(row_value).collect()))
            .map_err(|error| error.message())
    }

    fn query_one(&self, args: &[(Option<String>, Value)]) -> Result<Value, String> {
        let (sql_text, params) = self.read_args(args)?;
        self.database
            .query_one(&self.actor, &sql_text, &params, &self.audit)
            .map(|row| row_value(&row))
            .map_err(|error| error.message())
    }

    fn execute(&self, args: &[(Option<String>, Value)]) -> Result<Value, String> {
        let database = string_arg(args, "database", 0)?;
        if database != self.spec.name {
            return Err(format!("unknown database `{database}`"));
        }
        let sql_text = string_arg(args, "sql", 1)?;
        let params = params_arg(args, "params", 2)?;
        let max_affected_rows = match arg(args, "max_affected_rows", 3) {
            Some(Value::Int(value)) if *value >= 0 => *value as u64,
            Some(_) => return Err("`max_affected_rows` must be a non-negative integer".to_string()),
            None => return Err("missing `max_affected_rows`".to_string()),
        };
        let idempotency_key = string_arg(args, "idempotency_key", 4)?;

        let statement = sql::validate(
            &sql_text,
            SqlClass::Write,
            &params,
            &self.spec.allowed_tables,
        )
        .map_err(|error| error.message())?;
        let action_hash = optional_string_arg(args, "action_hash", 5)
            .unwrap_or_else(|| statement.fingerprint.clone());
        let request = WriteRequest::new(
            &self.spec.name,
            &sql_text,
            params.clone(),
            max_affected_rows,
            idempotency_key,
            action_hash,
        );
        let binding = WriteBinding::new(
            &self.spec.name,
            &statement.fingerprint,
            params_hash(&request.params),
            request.max_affected_rows,
            &request.action_hash,
        );
        let approval = self
            .approvals
            .borrow()
            .iter()
            .find(|approval| {
                approval
                    .verify(&binding, axiom_database::unix_now())
                    .is_ok()
            })
            .cloned();
        let mut ledger = self.ledger.borrow_mut();
        self.database
            .execute(
                &self.actor,
                &request,
                approval.as_ref(),
                &mut ledger,
                &self.audit,
            )
            .map(|outcome| {
                Value::Record(vec![
                    (
                        "affected_rows".to_string(),
                        Value::Int(outcome.affected_rows as i64),
                    ),
                    ("replayed".to_string(), Value::Bool(outcome.replayed)),
                ])
            })
            .map_err(|error| error.message())
    }

    fn read_args(&self, args: &[(Option<String>, Value)]) -> Result<(String, Params), String> {
        let database = string_arg(args, "database", 0)?;
        if database != self.spec.name {
            return Err(format!("unknown database `{database}`"));
        }
        let sql_text = string_arg(args, "sql", 1)?;
        let params = params_arg(args, "params", 2)?;
        Ok((sql_text, params))
    }
}

impl Host for DatabaseHost {
    fn call(
        &self,
        namespace: &str,
        method: &str,
        args: &[(Option<String>, Value)],
    ) -> Result<Value, String> {
        match namespace {
            "db" => match method {
                "query" => self.query(args),
                "query_one" => self.query_one(args),
                "execute" => self.execute(args),
                "schema" | "schema.inspect" | "migrations" | "migrations.plan"
                | "migrations.apply" | "scratch" | "scratch.create" | "scratch.destroy"
                | "audit_roles" => Err(format!(
                    "`db.{method}` is a separately authorized operator API, not an agent API"
                )),
                other => Err(format!("unsupported db method `{other}`")),
            },
            other => Err(format!(
                "namespace `{other}` is not supported by the database host"
            )),
        }
    }
}

fn arg<'a>(args: &'a [(Option<String>, Value)], name: &str, index: usize) -> Option<&'a Value> {
    if let Some((_, value)) = args.iter().find(|(key, _)| key.as_deref() == Some(name)) {
        return Some(value);
    }
    args.iter()
        .filter(|(key, _)| key.is_none())
        .nth(index)
        .map(|(_, value)| value)
}

fn string_arg(
    args: &[(Option<String>, Value)],
    name: &str,
    index: usize,
) -> Result<String, String> {
    optional_string_arg(args, name, index).ok_or_else(|| format!("missing string `{name}`"))
}

fn optional_string_arg(
    args: &[(Option<String>, Value)],
    name: &str,
    index: usize,
) -> Option<String> {
    match arg(args, name, index) {
        Some(Value::Str(value)) => Some(value.clone()),
        _ => None,
    }
}

fn params_arg(
    args: &[(Option<String>, Value)],
    name: &str,
    index: usize,
) -> Result<Params, String> {
    match arg(args, name, index) {
        Some(Value::Record(fields)) => {
            let mut params = Params::new();
            for (key, value) in fields {
                params.insert(key.clone(), to_db_value(value)?);
            }
            Ok(params)
        }
        Some(Value::Unit) | None => Ok(Params::new()),
        Some(_) => Err(format!("`{name}` must be a record of parameter values")),
    }
}

fn to_db_value(value: &Value) -> Result<DbValue, String> {
    Ok(match value {
        Value::Unit => DbValue::Null,
        Value::Bool(value) => DbValue::Integer(i64::from(*value)),
        Value::Int(value) => DbValue::Integer(*value),
        Value::Float(value) => DbValue::Real(*value),
        Value::Str(value) => DbValue::Text(value.clone()),
        Value::Bytes(value) => DbValue::Blob(value.clone()),
        Value::Duration(ms) => DbValue::Integer(*ms as i64),
        Value::Timestamp(value) => DbValue::Integer(*value),
        _ => return Err("parameter values must be scalars".to_string()),
    })
}

fn row_value(row: &Row) -> Value {
    Value::Record(
        row.columns()
            .iter()
            .map(|(name, value)| (name.clone(), from_db_value(value)))
            .collect(),
    )
}

fn from_db_value(value: &DbValue) -> Value {
    match value {
        DbValue::Null => Value::Unit,
        DbValue::Integer(value) => Value::Int(*value),
        DbValue::Real(value) => Value::Float(*value),
        DbValue::Text(value) => Value::Str(value.clone()),
        DbValue::Blob(value) => Value::Bytes(value.clone()),
    }
}
