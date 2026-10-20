use std::cell::RefCell;
use std::collections::BTreeMap;

use axiom_ir::{lower, ModuleInput, Program, Target};
use axiom_runtime::{
    effect_fingerprint, run_with_host, value_fingerprint, CancelToken, DurableConfig, DurableHost,
    EffectRecord, EffectStatus, Host, ResumeApproval, RunRecord, RunStatus, RunStore, Value,
};
use axiom_syntax::parse;

const RESUME_SOURCE: &str = r#"
module t;

fn work() -> Int {
  fs.write_text("a.txt", "one");
  let text = fs.read_text("a.txt");
  return 1;
}

fn entry() -> Int {
  return runtime.run_durable(run: work, run_id: "r1");
}
"#;

const WRITE_SOURCE: &str = r#"
module t;

fn work() -> Int {
  fs.write_text("a.txt", "one");
  return 1;
}

fn entry() -> Int {
  return runtime.run_durable(run: work, run_id: "r1");
}
"#;

const KEY_SOURCE: &str = r#"
module t;

fn work() -> Int {
  fs.write_text("a.txt", "one", idempotency_key: "write-a");
  return 1;
}

fn entry() -> Int {
  return runtime.run_durable(run: work, run_id: "r1");
}
"#;

const CHECKPOINT_SOURCE: &str = r#"
module t;

fn work() -> Int {
  let secret = fs.read_text("secret.txt");
  runtime.checkpoint(label: "after-read", value: secret);
  return 1;
}

fn entry() -> Int {
  return runtime.run_durable(run: work, run_id: "r1");
}
"#;

const NESTED_SOURCE: &str = r#"
module t;

fn child() -> Int {
  fs.read_text("child-a");
  fs.read_text("child-b");
  return 1;
}

fn parent() -> Int {
  let child_run = runtime.run_durable(run: child, run_id: "child");
  fs.read_text("parent-a");
  return 2;
}

fn entry() -> Int {
  return runtime.run_durable(run: parent, run_id: "parent");
}
"#;

const BUDGET_SOURCE: &str = r#"
module t;

fn work() -> Int {
  fs.write_text("a.txt", "one");
  fs.write_text("b.txt", "two");
  return 1;
}

fn entry() -> Int {
  return runtime.run_durable(run: work, run_id: "r1");
}
"#;

fn load(src: &str) -> Program {
    let file = parse(src).file;
    let inputs = [ModuleInput {
        name: "t".to_string(),
        display: "t.ax".to_string(),
        file: &file,
    }];
    lower(&inputs, Target::Server)
}

#[derive(Default)]
struct RecordingHost {
    calls: RefCell<Vec<(String, String)>>,
    counts: RefCell<BTreeMap<(String, String), u32>>,
    fail_on: Option<(String, String, u32)>,
    cancel_on: Option<(String, String, u32)>,
    cancel: Option<CancelToken>,
    read_value: String,
}

impl RecordingHost {
    fn new() -> Self {
        Self {
            read_value: "secret-value".to_string(),
            ..Self::default()
        }
    }

    fn failing(namespace: &str, method: &str, attempt: u32) -> Self {
        Self {
            fail_on: Some((namespace.to_string(), method.to_string(), attempt)),
            ..Self::new()
        }
    }

    fn count(&self, namespace: &str, method: &str) -> u32 {
        self.counts
            .borrow()
            .get(&(namespace.to_string(), method.to_string()))
            .copied()
            .unwrap_or(0)
    }

    fn total_calls(&self) -> usize {
        self.calls.borrow().len()
    }
}

impl Host for RecordingHost {
    fn call(
        &self,
        namespace: &str,
        method: &str,
        _args: &[(Option<String>, Value)],
    ) -> Result<Value, String> {
        let key = (namespace.to_string(), method.to_string());
        let attempt = {
            let mut counts = self.counts.borrow_mut();
            let count = counts.entry(key.clone()).or_insert(0);
            *count += 1;
            *count
        };
        self.calls.borrow_mut().push(key);
        if let Some((ns, name, expected)) = &self.fail_on {
            if ns == namespace && name == method && *expected == attempt {
                return Err("inner host failed".to_string());
            }
        }
        if let Some((ns, name, expected)) = &self.cancel_on {
            if ns == namespace && name == method && *expected == attempt {
                if let Some(token) = &self.cancel {
                    token.cancel();
                }
            }
        }
        match (namespace, method) {
            ("fs", "read_text") => Ok(Value::Str(self.read_value.clone())),
            ("fs", "write_text") => Ok(Value::Int(1)),
            _ => Ok(Value::Unit),
        }
    }
}

fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    match value {
        Value::Record(fields) => fields
            .iter()
            .find(|(field, _)| field == name)
            .map(|(_, value)| value)
            .unwrap_or_else(|| panic!("missing field `{name}`")),
        other => panic!("expected a record, found {other:?}"),
    }
}

fn status(value: &Value) -> &str {
    match field(value, "status") {
        Value::Str(status) => status,
        other => panic!("status is not a string: {other:?}"),
    }
}

#[test]
fn resume_skips_completed_effects_and_does_not_write_twice() {
    let program = load(RESUME_SOURCE);
    let first_host = RecordingHost::failing("fs", "read_text", 1);
    let first = DurableHost::new(&program).with_inner(&first_host);
    let outcome = run_with_host(&program, "entry", vec![], &first).expect("entry runs");
    assert_eq!(status(&outcome), "failed");
    assert_eq!(first_host.count("fs", "write_text"), 1);
    assert_eq!(first_host.count("fs", "read_text"), 1);
    let journal = first.run("r1").expect("run journaled");
    assert_eq!(journal.status, RunStatus::Failed);
    assert_eq!(journal.effects[0].status, EffectStatus::Completed);
    assert_eq!(journal.effects[1].status, EffectStatus::Failed);

    let second_host = RecordingHost::new();
    let second = DurableHost::new(&program)
        .with_inner(&second_host)
        .with_store(first.into_store());
    let outcome = run_with_host(&program, "entry", vec![], &second).expect("resume runs");
    assert_eq!(status(&outcome), "completed");
    assert_eq!(second_host.count("fs", "write_text"), 0);
    assert_eq!(second_host.count("fs", "read_text"), 1);
    let journal = second.run("r1").expect("run journaled");
    assert_eq!(journal.status, RunStatus::Completed);
    assert_eq!(journal.effects.len(), 2);
    assert!(journal
        .trace
        .iter()
        .any(|event| event.name == "fs.write_text" && event.status == "replayed"));
}

#[test]
fn indeterminate_write_requires_an_approval() {
    let program = load(WRITE_SOURCE);
    let first_host = RecordingHost::failing("fs", "write_text", 1);
    let first = DurableHost::new(&program).with_inner(&first_host);
    run_with_host(&program, "entry", vec![], &first).expect("entry runs");
    let journal = first.run("r1").expect("run journaled");
    assert_eq!(journal.status, RunStatus::Failed);
    assert_eq!(journal.effects[0].status, EffectStatus::Failed);

    let blocked_host = RecordingHost::new();
    let blocked = DurableHost::new(&program)
        .with_inner(&blocked_host)
        .with_store(first.store());
    let outcome = run_with_host(&program, "entry", vec![], &blocked).expect("resume runs");
    assert_eq!(status(&outcome), "failed");
    let reason = match field(&outcome, "reason") {
        Value::Str(reason) => reason,
        other => panic!("reason is not a string: {other:?}"),
    };
    assert!(reason.contains("indeterminate effect"), "{reason}");
    assert_eq!(blocked_host.total_calls(), 0);

    let fingerprint = journal.effects[0].fingerprint.clone();
    let approved_host = RecordingHost::new();
    let approved = DurableHost::new(&program)
        .with_inner(&approved_host)
        .with_store(first.store())
        .with_now(10);
    approved.approve(ResumeApproval::new("r1", 0, fingerprint, 100));
    let outcome =
        run_with_host(&program, "entry", vec![], &approved).expect("approved resume runs");
    assert_eq!(status(&outcome), "completed");
    assert_eq!(approved_host.count("fs", "write_text"), 1);
    assert_eq!(approved.approvals(), 0);
    let journal = approved.run("r1").expect("run journaled");
    assert_eq!(journal.effects[0].status, EffectStatus::Completed);
}

#[test]
fn idempotency_key_allows_a_retry_without_approval() {
    let program = load(KEY_SOURCE);
    let first_host = RecordingHost::failing("fs", "write_text", 1);
    let first = DurableHost::new(&program).with_inner(&first_host);
    run_with_host(&program, "entry", vec![], &first).expect("entry runs");

    let second_host = RecordingHost::new();
    let second = DurableHost::new(&program)
        .with_inner(&second_host)
        .with_store(first.into_store());
    let outcome = run_with_host(&program, "entry", vec![], &second).expect("resume runs");
    assert_eq!(status(&outcome), "completed");
    assert_eq!(second_host.count("fs", "write_text"), 1);
    let journal = second.run("r1").expect("run journaled");
    assert_eq!(
        journal.effects[0].idempotency_key.as_deref(),
        Some("write-a")
    );
}

#[test]
fn checkpoints_are_recorded_and_traces_are_redacted() {
    let program = load(CHECKPOINT_SOURCE);
    let host = RecordingHost::new();
    let durable = DurableHost::new(&program).with_inner(&host);
    let outcome = run_with_host(&program, "entry", vec![], &durable).expect("entry runs");
    assert_eq!(status(&outcome), "completed");

    let journal = durable.run("r1").expect("run journaled");
    assert_eq!(journal.checkpoints.len(), 1);
    assert_eq!(journal.checkpoints[0].label, "after-read");
    assert_eq!(
        journal.checkpoints[0].fingerprint,
        value_fingerprint(&Value::Str("secret-value".to_string()))
    );
    let trace = format!("{:?}", journal.trace);
    assert!(!trace.contains("secret-value"), "{trace}");
    assert!(journal
        .trace
        .iter()
        .any(|event| event.kind == axiom_runtime::TraceKind::Checkpoint));
}

#[test]
fn cancellation_stops_children_and_parent() {
    let program = load(NESTED_SOURCE);
    let token = CancelToken::new();
    let host = RecordingHost {
        cancel_on: Some(("fs".to_string(), "read_text".to_string(), 1)),
        cancel: Some(token.clone()),
        ..RecordingHost::new()
    };
    let durable = DurableHost::new(&program)
        .with_inner(&host)
        .with_cancel(token.clone())
        .with_config(DurableConfig {
            max_nesting: 4,
            ..DurableConfig::default()
        });
    let outcome = run_with_host(&program, "entry", vec![], &durable).expect("entry runs");
    assert_eq!(status(&outcome), "cancelled");

    let parent = durable.run("parent").expect("parent journaled");
    let child = durable.run("child").expect("child journaled");
    assert_eq!(parent.status, RunStatus::Cancelled);
    assert_eq!(child.status, RunStatus::Cancelled);
    assert_eq!(host.count("fs", "read_text"), 1);
    assert!(host
        .calls
        .borrow()
        .iter()
        .all(|(_, method)| method != "write_text"));
}

#[test]
fn completed_run_replays_without_executing_effects() {
    let program = load(NESTED_SOURCE);
    let first_host = RecordingHost::new();
    let first = DurableHost::new(&program).with_inner(&first_host);
    let outcome = run_with_host(&program, "entry", vec![], &first).expect("entry runs");
    assert_eq!(status(&outcome), "completed");

    let second_host = RecordingHost::new();
    let second = DurableHost::new(&program)
        .with_inner(&second_host)
        .with_store(first.into_store());
    let outcome = run_with_host(&program, "entry", vec![], &second).expect("replay runs");
    assert_eq!(status(&outcome), "completed");
    assert_eq!(second_host.total_calls(), 0);
    let journal = second.run("child").expect("child journaled");
    assert_eq!(
        journal.result,
        Some(Value::Int(1)),
        "child result comes from the journal"
    );
}

#[test]
fn effect_budget_fails_closed() {
    let program = load(BUDGET_SOURCE);
    let host = RecordingHost::new();
    let durable = DurableHost::new(&program)
        .with_inner(&host)
        .with_config(DurableConfig {
            max_effects: 1,
            ..DurableConfig::default()
        });
    let outcome = run_with_host(&program, "entry", vec![], &durable).expect("entry runs");
    assert_eq!(status(&outcome), "failed");
    let reason = match field(&outcome, "reason") {
        Value::Str(reason) => reason,
        other => panic!("reason is not a string: {other:?}"),
    };
    assert!(reason.contains("max_effects"), "{reason}");
    assert_eq!(host.count("fs", "write_text"), 1);
}

#[test]
fn unknown_functions_are_refused() {
    let program = load(
        r#"
module t;

fn entry() -> Int {
  return runtime.run_durable(run: missing);
}
"#,
    );
    let host = RecordingHost::new();
    let durable = DurableHost::new(&program).with_inner(&host);
    let error = run_with_host(&program, "entry", vec![], &durable).unwrap_err();
    assert!(
        error.message.contains("unknown function"),
        "{}",
        error.message
    );
}

#[test]
fn diverged_journals_are_refused() {
    let program = load(WRITE_SOURCE);
    let fingerprint = effect_fingerprint(
        "fs",
        "read_text",
        &[(Some("path".to_string()), Value::Str("a.txt".to_string()))],
    );
    let mut store = RunStore::new();
    let mut record = RunRecord::new(
        "r1".to_string(),
        "work".to_string(),
        value_fingerprint(&Value::Unit),
    );
    record.effects.push(EffectRecord {
        index: 0,
        namespace: "fs".to_string(),
        method: "read_text".to_string(),
        fingerprint,
        idempotency_key: None,
        status: EffectStatus::Started,
        result: None,
        error: None,
    });
    store.insert(record);

    let host = RecordingHost::new();
    let durable = DurableHost::new(&program)
        .with_inner(&host)
        .with_store(store);
    let outcome = run_with_host(&program, "entry", vec![], &durable).expect("entry runs");
    assert_eq!(status(&outcome), "failed");
    let reason = match field(&outcome, "reason") {
        Value::Str(reason) => reason,
        other => panic!("reason is not a string: {other:?}"),
    };
    assert!(reason.contains("diverged"), "{reason}");
    assert_eq!(host.total_calls(), 0);
}

#[test]
fn run_ids_are_derived_when_omitted_and_reused_on_resume() {
    let program = load(
        r#"
module t;

fn work(seed: Int) -> Int {
  fs.read_text("a.txt");
  return seed + 7;
}

fn entry() -> Int {
  return runtime.run_durable(run: work, input: 3);
}
"#,
    );
    let first_host = RecordingHost::new();
    let first = DurableHost::new(&program).with_inner(&first_host);
    let outcome = run_with_host(&program, "entry", vec![], &first).expect("entry runs");
    assert_eq!(status(&outcome), "completed");
    let runs = first.runs();
    assert_eq!(runs.len(), 1);
    assert!(runs[0].id.starts_with("run-"));
    let run_id = runs[0].id.clone();

    let second_host = RecordingHost::new();
    let second = DurableHost::new(&program)
        .with_inner(&second_host)
        .with_store(first.into_store());
    let outcome = run_with_host(&program, "entry", vec![], &second).expect("replay runs");
    assert_eq!(status(&outcome), "completed");
    assert_eq!(second_host.total_calls(), 0);
    assert!(second.run(&run_id).is_some());
}
