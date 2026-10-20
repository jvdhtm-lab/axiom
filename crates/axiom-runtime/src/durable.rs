//! Durable execution: effect journals, checkpoints, resume, cancellation, and
//! redacted traces for long-running agent work.
//!
//! Durability is at-least-once. A completed effect is replayed from the journal
//! and never invoked twice. An effect left `Started` or `Failed` by a crashed
//! attempt is indeterminate: it is retried only when it is read-only, carries an
//! idempotency key, or a matching [`ResumeApproval`] is supplied. This module
//! never claims exactly-once delivery.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use axiom_hash::hex_digest;
use axiom_ir::Program;

use crate::eval;
use crate::host::Host;
use crate::value::Value;

/// Lifecycle of a durable run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Running => "running",
            RunStatus::Completed => "completed",
            RunStatus::Failed => "failed",
            RunStatus::Cancelled => "cancelled",
        }
    }
}

/// Journal status of one external effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectStatus {
    Started,
    Completed,
    Failed,
}

impl EffectStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            EffectStatus::Started => "started",
            EffectStatus::Completed => "completed",
            EffectStatus::Failed => "failed",
        }
    }
}

/// One journaled effect. Payloads are stored in the run record; traces keep
/// only fingerprints and sizes.
#[derive(Debug, Clone)]
pub struct EffectRecord {
    pub index: u32,
    pub namespace: String,
    pub method: String,
    pub fingerprint: String,
    pub idempotency_key: Option<String>,
    pub status: EffectStatus,
    pub result: Option<Value>,
    pub error: Option<String>,
}

/// One checkpoint recorded at a declared safe boundary. Values are reduced to a
/// fingerprint; the raw value is never persisted.
#[derive(Debug, Clone)]
pub struct CheckpointRecord {
    pub sequence: u64,
    pub label: String,
    pub fingerprint: String,
}

/// Kinds of trace events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceKind {
    Started,
    Resume,
    Effect,
    Checkpoint,
    Approval,
    Completed,
    Failed,
    Cancelled,
}

impl TraceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TraceKind::Started => "started",
            TraceKind::Resume => "resume",
            TraceKind::Effect => "effect",
            TraceKind::Checkpoint => "checkpoint",
            TraceKind::Approval => "approval",
            TraceKind::Completed => "completed",
            TraceKind::Failed => "failed",
            TraceKind::Cancelled => "cancelled",
        }
    }
}

/// A redacted trace event: identifier, status, fingerprint, and byte count.
#[derive(Debug, Clone)]
pub struct TraceEvent {
    pub sequence: u64,
    pub run_id: String,
    pub kind: TraceKind,
    pub name: String,
    pub status: String,
    pub fingerprint: Option<String>,
    pub bytes: Option<u64>,
}

/// The durable journal for one run.
#[derive(Debug, Clone)]
pub struct RunRecord {
    pub id: String,
    pub function: String,
    pub input_fingerprint: String,
    pub status: RunStatus,
    pub effects: Vec<EffectRecord>,
    pub checkpoints: Vec<CheckpointRecord>,
    pub trace: Vec<TraceEvent>,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub revision: u64,
}

impl RunRecord {
    pub fn new(id: String, function: String, input_fingerprint: String) -> Self {
        Self {
            id,
            function,
            input_fingerprint,
            status: RunStatus::Running,
            effects: Vec::new(),
            checkpoints: Vec::new(),
            trace: Vec::new(),
            result: None,
            error: None,
            revision: 0,
        }
    }
}

/// An in-memory journal that can be handed between processes for resume.
#[derive(Debug, Clone, Default)]
pub struct RunStore {
    runs: BTreeMap<String, RunRecord>,
}

impl RunStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, run: RunRecord) {
        self.runs.insert(run.id.clone(), run);
    }

    pub fn get(&self, id: &str) -> Option<&RunRecord> {
        self.runs.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut RunRecord> {
        self.runs.get_mut(id)
    }

    pub fn remove(&mut self, id: &str) -> Option<RunRecord> {
        self.runs.remove(id)
    }

    pub fn len(&self) -> usize {
        self.runs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    pub fn ids(&self) -> Vec<String> {
        self.runs.keys().cloned().collect()
    }
}

/// Bounds applied to a durable run.
#[derive(Debug, Clone)]
pub struct DurableConfig {
    pub max_effects: u32,
    pub max_checkpoints: u32,
    pub max_nesting: u32,
    pub max_output_bytes: u64,
    pub trace_limit: usize,
}

impl Default for DurableConfig {
    fn default() -> Self {
        Self {
            max_effects: 64,
            max_checkpoints: 64,
            max_nesting: 8,
            max_output_bytes: 1_000_000,
            trace_limit: 512,
        }
    }
}

/// A cancellation flag shared with hosts and child runs.
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    cancelled: Rc<Cell<bool>>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.set(true);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.get()
    }

    pub fn reset(&self) {
        self.cancelled.set(false);
    }
}

/// Permission to retry one indeterminate effect. Approvals are single-use and
/// bound to the run, effect index, and effect fingerprint.
#[derive(Debug, Clone)]
pub struct ResumeApproval {
    pub run_id: String,
    pub effect_index: u32,
    pub action_hash: String,
    pub expires_at: u64,
}

impl ResumeApproval {
    pub fn new(
        run_id: impl Into<String>,
        effect_index: u32,
        action_hash: impl Into<String>,
        expires_at: u64,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            effect_index,
            action_hash: action_hash.into(),
            expires_at,
        }
    }
}

struct ActiveRun {
    run_id: String,
    next_effect: u32,
    next_checkpoint: u32,
    executed: u32,
    checkpoints: u32,
    output_bytes: u64,
}

enum ExistingEffect {
    Completed(Value),
    Pending { idempotency_key: Option<String> },
}

/// A host that journals effects, enforces budgets, records checkpoints, and
/// supports resume. Effects in other namespaces are delegated to `inner`.
pub struct DurableHost<'a> {
    program: &'a Program,
    inner: Option<&'a dyn Host>,
    store: RefCell<RunStore>,
    config: DurableConfig,
    cancel: CancelToken,
    active: RefCell<Vec<ActiveRun>>,
    approvals: RefCell<Vec<ResumeApproval>>,
    now: u64,
}

impl<'a> DurableHost<'a> {
    pub fn new(program: &'a Program) -> Self {
        Self {
            program,
            inner: None,
            store: RefCell::new(RunStore::new()),
            config: DurableConfig::default(),
            cancel: CancelToken::new(),
            active: RefCell::new(Vec::new()),
            approvals: RefCell::new(Vec::new()),
            now: 0,
        }
    }

    pub fn with_inner(mut self, inner: &'a dyn Host) -> Self {
        self.inner = Some(inner);
        self
    }

    pub fn with_store(mut self, store: RunStore) -> Self {
        self.store = RefCell::new(store);
        self
    }

    pub fn with_config(mut self, config: DurableConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_now(mut self, now: u64) -> Self {
        self.now = now;
        self
    }

    pub fn with_cancel(mut self, cancel: CancelToken) -> Self {
        self.cancel = cancel;
        self
    }

    pub fn cancel_token(&self) -> CancelToken {
        self.cancel.clone()
    }

    pub fn approve(&self, approval: ResumeApproval) {
        self.approvals.borrow_mut().push(approval);
    }

    pub fn approvals(&self) -> usize {
        self.approvals.borrow().len()
    }

    pub fn run(&self, id: &str) -> Option<RunRecord> {
        self.store.borrow().get(id).cloned()
    }

    pub fn runs(&self) -> Vec<RunRecord> {
        self.store.borrow().runs.values().cloned().collect()
    }

    pub fn store(&self) -> RunStore {
        self.store.borrow().clone()
    }

    pub fn into_store(self) -> RunStore {
        self.store.into_inner()
    }

    fn forward(
        &self,
        namespace: &str,
        method: &str,
        args: &[(Option<String>, Value)],
    ) -> Result<Value, String> {
        match self.inner {
            Some(inner) => inner.call(namespace, method, args),
            None => Err(format!("`{namespace}.{method}` requires a runtime host")),
        }
    }

    fn begin_effect(&self) -> Result<Option<(String, u32)>, String> {
        let mut active = self.active.borrow_mut();
        let Some(frame) = active.last_mut() else {
            return Ok(None);
        };
        if self.cancel.is_cancelled() {
            return Err("durable run cancelled".to_string());
        }
        let index = frame.next_effect;
        frame.next_effect += 1;
        Ok(Some((frame.run_id.clone(), index)))
    }

    fn begin_checkpoint(&self) -> Result<Option<(String, u32)>, String> {
        let mut active = self.active.borrow_mut();
        let Some(frame) = active.last_mut() else {
            return Ok(None);
        };
        if self.cancel.is_cancelled() {
            return Err("durable run cancelled".to_string());
        }
        let index = frame.next_checkpoint;
        frame.next_checkpoint += 1;
        Ok(Some((frame.run_id.clone(), index)))
    }

    fn existing_effect(
        &self,
        run_id: &str,
        index: u32,
        namespace: &str,
        method: &str,
        fingerprint: &str,
    ) -> Result<Option<ExistingEffect>, String> {
        let store = self.store.borrow();
        let run = store
            .get(run_id)
            .ok_or_else(|| format!("run `{run_id}` is missing from the journal"))?;
        let Some(record) = run.effects.get(index as usize) else {
            return Ok(None);
        };
        if record.namespace != namespace
            || record.method != method
            || record.fingerprint != fingerprint
        {
            return Err(format!(
                "durable run `{run_id}` diverged at effect {index}: journaled `{}.{}` but the function attempted `{namespace}.{method}`",
                record.namespace, record.method
            ));
        }
        match record.status {
            EffectStatus::Completed => Ok(Some(ExistingEffect::Completed(
                record.result.clone().unwrap_or(Value::Unit),
            ))),
            EffectStatus::Started | EffectStatus::Failed => Ok(Some(ExistingEffect::Pending {
                idempotency_key: record.idempotency_key.clone(),
            })),
        }
    }

    fn push_effect(&self, run_id: &str, record: EffectRecord) {
        if let Some(run) = self.store.borrow_mut().get_mut(run_id) {
            run.effects.push(record);
        }
    }

    fn set_effect(
        &self,
        run_id: &str,
        index: u32,
        status: EffectStatus,
        result: Option<Value>,
        error: Option<String>,
    ) {
        if let Some(run) = self.store.borrow_mut().get_mut(run_id) {
            if let Some(record) = run.effects.get_mut(index as usize) {
                record.status = status;
                record.result = result;
                record.error = error;
            }
        }
    }

    fn spend_effect(&self, run_id: &str) -> Result<(), String> {
        let mut active = self.active.borrow_mut();
        let Some(frame) = active.last_mut() else {
            return Err("no active durable run".to_string());
        };
        if frame.run_id != run_id {
            return Err("active durable run changed unexpectedly".to_string());
        }
        frame.executed += 1;
        if frame.executed > self.config.max_effects {
            return Err(format!(
                "durable run exceeded max_effects ({})",
                self.config.max_effects
            ));
        }
        Ok(())
    }

    fn spend_output(&self, bytes: u64) -> Result<(), String> {
        let mut active = self.active.borrow_mut();
        let Some(frame) = active.last_mut() else {
            return Err("no active durable run".to_string());
        };
        frame.output_bytes += bytes;
        if frame.output_bytes > self.config.max_output_bytes {
            return Err(format!(
                "durable run exceeded max_output_bytes ({})",
                self.config.max_output_bytes
            ));
        }
        Ok(())
    }

    fn consume_approval(
        &self,
        run_id: &str,
        index: u32,
        fingerprint: &str,
    ) -> Result<bool, String> {
        let mut approvals = self.approvals.borrow_mut();
        let Some(position) = approvals.iter().position(|a| {
            a.run_id == run_id && a.effect_index == index && a.action_hash == fingerprint
        }) else {
            return Ok(false);
        };
        let approval = approvals.remove(position);
        drop(approvals);
        if self.now >= approval.expires_at {
            return Err(format!(
                "resume approval for effect {index} in run `{run_id}` expired"
            ));
        }
        self.trace(
            run_id,
            TraceKind::Approval,
            "resume".to_string(),
            "accepted",
            Some(fingerprint.to_string()),
            None,
        );
        Ok(true)
    }

    fn trace(
        &self,
        run_id: &str,
        kind: TraceKind,
        name: String,
        status: &str,
        fingerprint: Option<String>,
        bytes: Option<u64>,
    ) {
        let mut store = self.store.borrow_mut();
        let Some(run) = store.get_mut(run_id) else {
            return;
        };
        if run.trace.len() >= self.config.trace_limit {
            return;
        }
        let sequence = run.trace.len() as u64;
        run.trace.push(TraceEvent {
            sequence,
            run_id: run_id.to_string(),
            kind,
            name,
            status: status.to_string(),
            fingerprint,
            bytes,
        });
    }

    fn effect(
        &self,
        namespace: &str,
        method: &str,
        args: &[(Option<String>, Value)],
    ) -> Result<Value, String> {
        let Some((run_id, index)) = self.begin_effect()? else {
            return self.forward(namespace, method, args);
        };
        if self.cancel.is_cancelled() {
            return Err("durable run cancelled".to_string());
        }
        let fingerprint = effect_fingerprint(namespace, method, args);
        let key = idempotency_key(args);
        let existing = self.existing_effect(&run_id, index, namespace, method, &fingerprint)?;
        match existing {
            Some(ExistingEffect::Completed(value)) => {
                let bytes = value_bytes(&value);
                self.trace(
                    &run_id,
                    TraceKind::Effect,
                    format!("{namespace}.{method}"),
                    "replayed",
                    Some(fingerprint),
                    Some(bytes),
                );
                return Ok(value);
            }
            Some(ExistingEffect::Pending { idempotency_key }) => {
                let approved = idempotency_key.is_some()
                    || key.is_some()
                    || retry_safe(namespace, method)
                    || self.consume_approval(&run_id, index, &fingerprint)?;
                if !approved {
                    return Err(format!(
                        "indeterminate effect `{namespace}.{method}` at index {index} in run `{run_id}`: retry requires an idempotency key or a matching resume approval"
                    ));
                }
                self.set_effect(&run_id, index, EffectStatus::Started, None, None);
            }
            None => self.push_effect(
                &run_id,
                EffectRecord {
                    index,
                    namespace: namespace.to_string(),
                    method: method.to_string(),
                    fingerprint: fingerprint.clone(),
                    idempotency_key: key,
                    status: EffectStatus::Started,
                    result: None,
                    error: None,
                },
            ),
        }
        self.spend_effect(&run_id)?;
        if self.cancel.is_cancelled() {
            return Err("durable run cancelled".to_string());
        }
        let Some(inner) = self.inner else {
            let error = format!("`{namespace}.{method}` requires a runtime host");
            self.set_effect(
                &run_id,
                index,
                EffectStatus::Failed,
                None,
                Some(error.clone()),
            );
            self.trace(
                &run_id,
                TraceKind::Effect,
                format!("{namespace}.{method}"),
                "failed",
                Some(fingerprint),
                None,
            );
            return Err(error);
        };
        match inner.call(namespace, method, args) {
            Ok(value) => {
                let bytes = value_bytes(&value);
                self.set_effect(
                    &run_id,
                    index,
                    EffectStatus::Completed,
                    Some(value.clone()),
                    None,
                );
                self.spend_output(bytes)?;
                self.trace(
                    &run_id,
                    TraceKind::Effect,
                    format!("{namespace}.{method}"),
                    "completed",
                    Some(fingerprint),
                    Some(bytes),
                );
                Ok(value)
            }
            Err(error) => {
                self.set_effect(
                    &run_id,
                    index,
                    EffectStatus::Failed,
                    None,
                    Some(error.clone()),
                );
                self.trace(
                    &run_id,
                    TraceKind::Effect,
                    format!("{namespace}.{method}"),
                    "failed",
                    Some(fingerprint),
                    None,
                );
                Err(error)
            }
        }
    }

    fn checkpoint(&self, args: &[(Option<String>, Value)]) -> Result<Value, String> {
        let Some((run_id, index)) = self.begin_checkpoint()? else {
            return Err("`runtime.checkpoint` is only valid inside a durable run".to_string());
        };
        let Some(Value::Str(label)) = arg_value(args, "label", 0) else {
            return Err("`runtime.checkpoint` requires a string `label`".to_string());
        };
        if label.is_empty() {
            return Err("`runtime.checkpoint` requires a non-empty `label`".to_string());
        }
        let value = arg_value(args, "value", 1).cloned().unwrap_or(Value::Unit);
        let fingerprint = value_fingerprint(&value);
        let existing = self
            .store
            .borrow()
            .get(&run_id)
            .and_then(|run| run.checkpoints.get(index as usize))
            .cloned();
        if let Some(record) = existing {
            if record.label != *label || record.fingerprint != fingerprint {
                return Err(format!(
                    "durable run `{run_id}` diverged at checkpoint {index}"
                ));
            }
            self.trace(
                &run_id,
                TraceKind::Checkpoint,
                label.clone(),
                "replayed",
                Some(fingerprint.clone()),
                Some(value_bytes(&value)),
            );
            return Ok(checkpoint_value(label, &fingerprint));
        }
        {
            let mut active = self.active.borrow_mut();
            let Some(frame) = active.last_mut() else {
                return Err("no active durable run".to_string());
            };
            frame.checkpoints += 1;
            if frame.checkpoints > self.config.max_checkpoints {
                return Err(format!(
                    "durable run exceeded max_checkpoints ({})",
                    self.config.max_checkpoints
                ));
            }
        }
        let sequence = {
            let mut store = self.store.borrow_mut();
            let Some(run) = store.get_mut(&run_id) else {
                return Err(format!("run `{run_id}` is missing from the journal"));
            };
            let sequence = run.checkpoints.len() as u64;
            run.checkpoints.push(CheckpointRecord {
                sequence,
                label: label.clone(),
                fingerprint: fingerprint.clone(),
            });
            sequence
        };
        self.trace(
            &run_id,
            TraceKind::Checkpoint,
            label.clone(),
            "recorded",
            Some(fingerprint.clone()),
            Some(value_bytes(&value)),
        );
        let _ = sequence;
        Ok(checkpoint_value(label, &fingerprint))
    }

    fn run_durable(&self, args: &[(Option<String>, Value)]) -> Result<Value, String> {
        let function = match arg_value(args, "run", 0) {
            Some(Value::Function(name)) | Some(Value::Str(name)) => name.clone(),
            Some(_) => {
                return Err(
                    "`runtime.run_durable` requires `run` to be a function reference".to_string(),
                )
            }
            None => return Err("`runtime.run_durable` requires `run`".to_string()),
        };
        if !function_exists(self.program, &function) {
            return Err(format!("unknown function `{function}`"));
        }
        let input = arg_value(args, "input", 1).cloned().unwrap_or(Value::Unit);
        let parent = self
            .active
            .borrow()
            .last()
            .map(|frame| frame.run_id.clone());
        let run_id = match arg_value(args, "run_id", 2) {
            Some(Value::Str(id)) if !id.is_empty() => id.clone(),
            Some(Value::Str(_)) => return Err("`run_id` must be a non-empty string".to_string()),
            Some(_) => return Err("`run_id` must be a string".to_string()),
            None => default_run_id(parent.as_deref(), &function, &input),
        };
        if self.active.borrow().len() >= self.config.max_nesting as usize {
            return Err(format!(
                "durable nesting limit exceeded ({})",
                self.config.max_nesting
            ));
        }
        {
            let store = self.store.borrow();
            if let Some(run) = store.get(&run_id) {
                if matches!(run.status, RunStatus::Completed | RunStatus::Cancelled) {
                    return Ok(run_outcome(run));
                }
            }
        }
        let resuming = self.store.borrow().get(&run_id).is_some();
        {
            let mut store = self.store.borrow_mut();
            if let Some(run) = store.get_mut(&run_id) {
                if run.function != function || run.input_fingerprint != value_fingerprint(&input) {
                    return Err(format!(
                        "run `{run_id}` was journaled for a different function or input"
                    ));
                }
                run.status = RunStatus::Running;
                run.error = None;
                run.revision += 1;
            } else {
                store.insert(RunRecord::new(
                    run_id.clone(),
                    function.clone(),
                    value_fingerprint(&input),
                ));
            }
        }
        self.trace(
            &run_id,
            if resuming {
                TraceKind::Resume
            } else {
                TraceKind::Started
            },
            function.clone(),
            if resuming { "resumed" } else { "started" },
            None,
            None,
        );
        self.active.borrow_mut().push(ActiveRun {
            run_id: run_id.clone(),
            next_effect: 0,
            next_checkpoint: 0,
            executed: 0,
            checkpoints: 0,
            output_bytes: 0,
        });
        let args = match input {
            Value::Unit => Vec::new(),
            input => vec![input],
        };
        let outcome = if self.cancel.is_cancelled() {
            Err("durable run cancelled".to_string())
        } else {
            eval::run_with_host(self.program, &function, args, self).map_err(|error| error.message)
        };
        self.active.borrow_mut().pop();
        let (status, result, error) = if self.cancel.is_cancelled() {
            (
                RunStatus::Cancelled,
                None,
                Some("durable run cancelled".to_string()),
            )
        } else {
            match outcome {
                Ok(value) => (RunStatus::Completed, Some(value), None),
                Err(message) => (RunStatus::Failed, None, Some(message)),
            }
        };
        let value = {
            let mut store = self.store.borrow_mut();
            let Some(run) = store.get_mut(&run_id) else {
                return Err(format!("run `{run_id}` is missing from the journal"));
            };
            run.status = status;
            run.result = result;
            run.error = error;
            run.revision += 1;
            run_outcome(run)
        };
        self.trace(
            &run_id,
            match status {
                RunStatus::Completed => TraceKind::Completed,
                RunStatus::Failed => TraceKind::Failed,
                RunStatus::Cancelled => TraceKind::Cancelled,
                RunStatus::Running => TraceKind::Started,
            },
            function,
            status.as_str(),
            None,
            None,
        );
        Ok(value)
    }
}

impl Host for DurableHost<'_> {
    fn call(
        &self,
        namespace: &str,
        method: &str,
        args: &[(Option<String>, Value)],
    ) -> Result<Value, String> {
        match namespace {
            "runtime" => match method {
                "run_durable" => self.run_durable(args),
                "checkpoint" => self.checkpoint(args),
                "retry" => Err(
                    "`runtime.retry` does not retry implicitly; use `agents.iterate` or resume a durable run with an approval"
                        .to_string(),
                ),
                other => Err(format!("unknown runtime API `{other}`")),
            },
            _ => self.effect(namespace, method, args),
        }
    }
}

fn function_exists(program: &Program, name: &str) -> bool {
    program.modules.iter().any(|module| {
        module
            .functions
            .iter()
            .any(|function| function.name == name)
    })
}

fn arg_value<'v>(
    args: &'v [(Option<String>, Value)],
    name: &str,
    index: usize,
) -> Option<&'v Value> {
    args.iter()
        .find(|(field, _)| field.as_deref() == Some(name))
        .map(|(_, value)| value)
        .or_else(|| {
            args.get(index)
                .filter(|(field, _)| field.is_none())
                .map(|(_, value)| value)
        })
}

fn idempotency_key(args: &[(Option<String>, Value)]) -> Option<String> {
    args.iter()
        .find(|(name, _)| name.as_deref() == Some("idempotency_key"))
        .and_then(|(_, value)| match value {
            Value::Str(key) if !key.is_empty() => Some(key.clone()),
            _ => None,
        })
}

fn retry_safe(namespace: &str, method: &str) -> bool {
    matches!(
        (namespace, method),
        ("db", "query")
            | ("db", "query_one")
            | ("fs", "read")
            | ("fs", "read_text")
            | ("workspaces", "read")
            | ("workspaces", "read_text")
            | ("artifacts", "fetch")
            | ("artifacts", "inspect")
            | ("artifacts", "verify")
            | ("checks", "ax_compile")
            | ("compiler", "check")
    )
}

fn default_run_id(parent: Option<&str>, function: &str, input: &Value) -> String {
    let mut seed = String::new();
    if let Some(parent) = parent {
        seed.push_str(parent);
        seed.push('|');
    }
    seed.push_str(function);
    seed.push('|');
    seed.push_str(&value_fingerprint(input));
    let digest = hex_digest(seed.as_bytes());
    format!("run-{}", &digest[..16])
}

fn checkpoint_value(label: &str, fingerprint: &str) -> Value {
    Value::Record(vec![
        ("checkpoint".to_string(), Value::Str(label.to_string())),
        (
            "fingerprint".to_string(),
            Value::Str(fingerprint.to_string()),
        ),
    ])
}

fn run_outcome(run: &RunRecord) -> Value {
    Value::Record(vec![
        ("run_id".to_string(), Value::Str(run.id.clone())),
        (
            "status".to_string(),
            Value::Str(run.status.as_str().to_string()),
        ),
        (
            "result".to_string(),
            match &run.result {
                Some(value) => Value::Variant("Some".to_string(), vec![value.clone()]),
                None => Value::Variant("None".to_string(), Vec::new()),
            },
        ),
        (
            "reason".to_string(),
            Value::Str(run.error.clone().unwrap_or_default()),
        ),
        ("effects".to_string(), Value::Int(run.effects.len() as i64)),
        (
            "checkpoints".to_string(),
            Value::List(
                run.checkpoints
                    .iter()
                    .map(|checkpoint| Value::Str(checkpoint.label.clone()))
                    .collect(),
            ),
        ),
    ])
}

/// A deterministic fingerprint for an effect call: namespace, method, argument
/// names, and argument values.
pub fn effect_fingerprint(
    namespace: &str,
    method: &str,
    args: &[(Option<String>, Value)],
) -> String {
    let mut bytes = Vec::new();
    encode_str(&mut bytes, namespace);
    encode_str(&mut bytes, method);
    bytes.extend_from_slice(&(args.len() as u64).to_le_bytes());
    for (name, value) in args {
        encode_str(&mut bytes, name.as_deref().unwrap_or(""));
        encode_value(&mut bytes, value);
    }
    hex_digest(&bytes)
}

/// A deterministic fingerprint for any runtime value.
pub fn value_fingerprint(value: &Value) -> String {
    let mut bytes = Vec::new();
    encode_value(&mut bytes, value);
    hex_digest(&bytes)
}

fn value_bytes(value: &Value) -> u64 {
    value.display().len() as u64
}

fn encode_str(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u64).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn encode_value(out: &mut Vec<u8>, value: &Value) {
    match value {
        Value::Unit => out.push(b'u'),
        Value::Bool(flag) => {
            out.push(b'b');
            out.push(u8::from(*flag));
        }
        Value::Int(number) => {
            out.push(b'i');
            out.extend_from_slice(&number.to_le_bytes());
        }
        Value::Float(number) => {
            out.push(b'f');
            out.extend_from_slice(&number.to_bits().to_le_bytes());
        }
        Value::Str(text) => {
            out.push(b's');
            encode_str(out, text);
        }
        Value::Bytes(bytes) => {
            out.push(b'y');
            out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
            out.extend_from_slice(bytes);
        }
        Value::Duration(ms) => {
            out.push(b'd');
            out.extend_from_slice(&ms.to_le_bytes());
        }
        Value::Timestamp(timestamp) => {
            out.push(b't');
            out.extend_from_slice(&timestamp.to_le_bytes());
        }
        Value::List(items) => {
            out.push(b'l');
            out.extend_from_slice(&(items.len() as u64).to_le_bytes());
            for item in items {
                encode_value(out, item);
            }
        }
        Value::Record(fields) => {
            out.push(b'r');
            out.extend_from_slice(&(fields.len() as u64).to_le_bytes());
            for (name, field) in fields {
                encode_str(out, name);
                encode_value(out, field);
            }
        }
        Value::Variant(name, args) => {
            out.push(b'v');
            encode_str(out, name);
            out.extend_from_slice(&(args.len() as u64).to_le_bytes());
            for arg in args {
                encode_value(out, arg);
            }
        }
        Value::Function(name) => {
            out.push(b'c');
            encode_str(out, name);
        }
    }
}
