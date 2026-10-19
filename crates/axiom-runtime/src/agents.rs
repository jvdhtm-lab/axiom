//! Agents, iteration, and runtime-owned typed results.
//!
//! An `agent` declaration is metadata only: model alias, workspace, tools,
//! limits, and a context string. A model produces a *candidate*; only the
//! runtime decides acceptance after running required validators. A model's text
//! claiming a test passed is never evidence. Every attempt is bounded by the
//! agent's declared [`Budgets`], linked to a content-addressed candidate, and
//! recorded with its validation results.

use std::time::Duration;

use axiom_providers::{ModelProvider, ModelRequest};

use crate::budget::{BudgetExhausted, BudgetLedger, Budgets};
use crate::validation::{EvidenceRef, ValidationResult, ValidationStatus, ValidatorRegistry};
use crate::value::Value;

/// The metadata of one declared agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSpec {
    pub name: String,
    /// Model alias from `axiom.yaml` (`models.<alias>`); never a vendor name.
    pub model: String,
    /// Declared workspace name, if any. This is the agent's maximum authority.
    pub workspace: Option<String>,
    pub tools: Vec<String>,
    pub limits: Budgets,
    /// Untrusted context handed to the model as data, never instructions.
    pub context: Option<String>,
    pub input_type: String,
    pub output_type: String,
}

impl AgentSpec {
    pub fn new(name: impl Into<String>, model: impl Into<String>) -> Self {
        AgentSpec {
            name: name.into(),
            model: model.into(),
            workspace: None,
            tools: Vec::new(),
            limits: Budgets::default(),
            context: None,
            input_type: "Unknown".to_string(),
            output_type: "Unknown".to_string(),
        }
    }

    pub fn with_workspace(mut self, workspace: impl Into<String>) -> Self {
        self.workspace = Some(workspace.into());
        self
    }

    pub fn with_tools(mut self, tools: &[&str]) -> Self {
        self.tools = tools.iter().map(|tool| (*tool).to_string()).collect();
        self
    }

    pub fn with_limits(mut self, limits: Budgets) -> Self {
        self.limits = limits;
        self
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Build a spec from lowered IR, reporting malformed metadata instead of
    /// guessing. `model` is required; every other member is optional.
    pub fn from_ir(agent: &axiom_ir::IrAgent) -> Result<AgentSpec, String> {
        let model = agent
            .members
            .iter()
            .rev()
            .find(|(name, _)| name == "model")
            .map(|(_, expr)| expr)
            .ok_or_else(|| format!("agent `{}` is missing required `model`", agent.name))?;
        let model = match &model.kind {
            axiom_ir::IrExprKind::Str(value) => value.clone(),
            _ => return Err(format!("agent `{}` `model` must be a string", agent.name)),
        };

        let mut spec = AgentSpec::new(agent.name.clone(), model);
        spec.input_type = agent
            .params
            .first()
            .map(|(_, ty)| ty.name())
            .unwrap_or_else(|| "Unit".to_string());
        spec.output_type = agent.ret.name();
        let method = |name: &str| {
            agent
                .members
                .iter()
                .rev()
                .find(|(member, _)| member == name)
                .map(|(_, expr)| expr)
        };

        if let Some(expr) = method("workspace") {
            spec.workspace = Some(
                expr_name(expr)
                    .ok_or_else(|| format!("agent `{}` `workspace` must be a name", agent.name))?,
            );
        }
        if let Some(expr) = method("context") {
            if let axiom_ir::IrExprKind::Str(value) = &expr.kind {
                spec.context = Some(value.clone());
            }
        }
        if let Some(expr) = method("tools") {
            let axiom_ir::IrExprKind::List(items) = &expr.kind else {
                return Err(format!("agent `{}` `tools` must be a list", agent.name));
            };
            for item in items {
                let name = expr_name(item).ok_or_else(|| {
                    format!("agent `{}` has a non-name `tools` entry", agent.name)
                })?;
                spec.tools.push(name);
            }
        }
        if let Some(expr) = method("limits") {
            let axiom_ir::IrExprKind::Record(members) = &expr.kind else {
                return Err(format!("agent `{}` `limits` must be a record", agent.name));
            };
            for (name, value) in members {
                match (name.as_str(), &value.kind) {
                    ("max_calls", axiom_ir::IrExprKind::Int(value)) if *value >= 0 => {
                        spec.limits.max_calls = *value as u32;
                    }
                    ("max_output_bytes", axiom_ir::IrExprKind::Int(value)) if *value >= 0 => {
                        spec.limits.max_output_bytes = *value as u64;
                    }
                    ("timeout", axiom_ir::IrExprKind::Duration(millis)) => {
                        spec.limits.timeout = Duration::from_millis(*millis);
                    }
                    (other, _) => {
                        return Err(format!(
                            "agent `{}` has unknown limit `{other}`",
                            agent.name
                        ))
                    }
                }
            }
        }
        Ok(spec)
    }
}

fn expr_name(expr: &axiom_ir::IrExpr) -> Option<String> {
    match &expr.kind {
        axiom_ir::IrExprKind::Str(value) => Some(value.clone()),
        axiom_ir::IrExprKind::Function(name)
        | axiom_ir::IrExprKind::Local(name)
        | axiom_ir::IrExprKind::Namespace(name) => Some(name.clone()),
        axiom_ir::IrExprKind::Field(base, name) => Some(format!("{}.{}", expr_name(base)?, name)),
        axiom_ir::IrExprKind::Variant { name, .. } => Some(name.clone()),
        _ => None,
    }
}

/// One model output considered for acceptance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub attempt: u32,
    pub content: String,
    /// SHA-256 of the candidate content; approvals bind to this.
    pub content_hash: String,
}

impl Candidate {
    pub fn new(attempt: u32, content: impl Into<String>) -> Self {
        let content = content.into();
        let content_hash = axiom_hash::hex_digest(content.as_bytes());
        Candidate {
            attempt,
            content,
            content_hash,
        }
    }

    /// A stable, content-addressed reference safe to record in traces.
    pub fn reference(&self) -> String {
        format!(
            "candidate:{}",
            &self.content_hash[..32.min(self.content_hash.len())]
        )
    }
}

/// The request that drives an iteration.
#[derive(Debug, Clone, PartialEq)]
pub struct IterationRequest {
    pub input: Value,
    pub max_attempts: u32,
    /// Validator names that must pass before acceptance. An empty list never
    /// accepts (unvalidated candidates fail closed).
    pub validate: Vec<String>,
    /// Hash of the action this iteration is authorized to perform, if any.
    pub action_hash: Option<String>,
}

impl IterationRequest {
    pub fn new(input: Value, max_attempts: u32, validate: &[&str]) -> Self {
        IterationRequest {
            input,
            max_attempts,
            validate: validate.iter().map(|name| (*name).to_string()).collect(),
            action_hash: None,
        }
    }
}

/// How an iteration ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IterationStatus {
    Accepted,
    /// Attempts ran out with repairable failures.
    Exhausted,
    /// A required, non-repairable failure needs a human or other intervention.
    Escalated,
    /// A provider error or budget exhaustion.
    Failed,
    Cancelled,
}

impl IterationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            IterationStatus::Accepted => "accepted",
            IterationStatus::Exhausted => "exhausted",
            IterationStatus::Escalated => "escalated",
            IterationStatus::Failed => "failed",
            IterationStatus::Cancelled => "cancelled",
        }
    }
}

/// Per-attempt validation evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptValidation {
    pub attempt: u32,
    pub candidate: String,
    pub results: Vec<ValidationResult>,
}

/// The runtime-owned outcome of an iteration. Only the runtime constructs it.
#[derive(Debug, Clone, PartialEq)]
pub struct IterationOutcome {
    pub status: IterationStatus,
    pub attempts: u32,
    pub accepted: Option<Candidate>,
    pub validations: Vec<AttemptValidation>,
    pub evidence: Vec<EvidenceRef>,
    pub reason: String,
}

impl IterationOutcome {
    /// Whether an artifact was accepted after required validation passed.
    pub fn is_accepted(&self) -> bool {
        self.status == IterationStatus::Accepted
    }

    /// The accepted candidate's content, if any.
    pub fn accepted_content(&self) -> Option<&str> {
        self.accepted
            .as_ref()
            .map(|candidate| candidate.content.as_str())
    }
}

/// Run a bounded, validated iteration loop.
///
/// Order per attempt: cancellation check, budget check, model call, candidate
/// construction, required validation. Required failure is repairable only when
/// **every** failing required validator says so; otherwise the run escalates.
/// Budget exhaustion or a provider error fails the run. An empty validator list
/// can never accept.
pub fn run_iteration(
    agent: &AgentSpec,
    request: &IterationRequest,
    provider: &mut dyn ModelProvider,
    validators: &ValidatorRegistry,
    ledger: &mut BudgetLedger,
    should_cancel: &dyn Fn(u32) -> bool,
) -> IterationOutcome {
    let mut validations = Vec::new();
    let mut repair_findings: Vec<String> = Vec::new();
    let mut attempts = 0u32;

    if request.validate.is_empty() {
        return IterationOutcome {
            status: IterationStatus::Failed,
            attempts,
            accepted: None,
            validations,
            evidence: Vec::new(),
            reason: "no validators configured; unvalidated candidates fail closed".to_string(),
        };
    }

    let max_attempts = request.max_attempts.max(1);
    while attempts < max_attempts {
        if should_cancel(attempts) {
            return IterationOutcome {
                status: IterationStatus::Cancelled,
                attempts,
                accepted: None,
                validations,
                evidence: Vec::new(),
                reason: "cancelled before the next attempt".to_string(),
            };
        }
        if let Err(exhausted) = ledger.allows(0) {
            return IterationOutcome {
                status: IterationStatus::Failed,
                attempts,
                accepted: None,
                validations,
                evidence: Vec::new(),
                reason: format!("budget exhausted: {}", exhausted.as_str()),
            };
        }

        attempts += 1;
        let prompt = build_prompt(agent, request, &repair_findings);
        let response = match provider.complete(&ModelRequest::new(&agent.model, prompt)) {
            Ok(response) => response,
            Err(error) => {
                return IterationOutcome {
                    status: IterationStatus::Failed,
                    attempts,
                    accepted: None,
                    validations,
                    evidence: Vec::new(),
                    reason: format!("provider error: {error}"),
                };
            }
        };

        let candidate = Candidate::new(attempts, response.text.clone());
        if let Err(exhausted) = ledger.spend_call(candidate.content.len() as u64) {
            return IterationOutcome {
                status: IterationStatus::Failed,
                attempts,
                accepted: None,
                validations,
                evidence: Vec::new(),
                reason: format!("budget exhausted: {}", exhausted.as_str()),
            };
        }

        let results: Vec<ValidationResult> = request
            .validate
            .iter()
            .map(|name| validators.validate(name, &candidate))
            .collect();
        let evidence: Vec<EvidenceRef> = results
            .iter()
            .flat_map(|result| result.evidence.iter().cloned())
            .collect();

        let blocking: Vec<&ValidationResult> = results
            .iter()
            .filter(|result| result.blocks_acceptance())
            .collect();
        validations.push(AttemptValidation {
            attempt: attempts,
            candidate: candidate.reference(),
            results: results.clone(),
        });

        if blocking.is_empty() {
            return IterationOutcome {
                status: IterationStatus::Accepted,
                attempts,
                accepted: Some(candidate),
                validations,
                evidence,
                reason: "all required validation passed".to_string(),
            };
        }

        let repairable = blocking
            .iter()
            .all(|result| result.repairable && result.status == ValidationStatus::Failed);
        repair_findings = blocking
            .iter()
            .flat_map(|result| {
                result
                    .findings
                    .iter()
                    .map(|finding| format!("{}: {finding}", result.validator))
            })
            .collect();
        repair_findings.insert(
            0,
            format!(
                "attempt {attempts} failed: {}",
                blocking
                    .iter()
                    .map(|result| result.validator.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );

        if !repairable {
            return IterationOutcome {
                status: IterationStatus::Escalated,
                attempts,
                accepted: None,
                validations,
                evidence,
                reason: "a required validation failed non-repairably".to_string(),
            };
        }

        if attempts == max_attempts {
            return IterationOutcome {
                status: IterationStatus::Exhausted,
                attempts,
                accepted: None,
                validations,
                evidence,
                reason: "attempts exhausted with repairable failures".to_string(),
            };
        }
    }

    IterationOutcome {
        status: IterationStatus::Exhausted,
        attempts,
        accepted: None,
        validations,
        evidence: Vec::new(),
        reason: "attempts exhausted".to_string(),
    }
}

/// Build the model prompt. Context and input are untrusted data; findings from
/// a failed attempt are structured runtime output.
fn build_prompt(agent: &AgentSpec, request: &IterationRequest, findings: &[String]) -> String {
    let mut prompt = String::new();
    prompt.push_str("You are the Axiom agent `");
    prompt.push_str(&agent.name);
    prompt.push_str("`.\nOutput type: ");
    prompt.push_str(&agent.output_type);
    prompt.push('\n');
    if let Some(workspace) = &agent.workspace {
        prompt.push_str("Declared workspace: ");
        prompt.push_str(workspace);
        prompt.push('\n');
    }
    if let Some(context) = &agent.context {
        prompt.push_str("\n<context untrusted=\"true\">\n");
        prompt.push_str(context);
        prompt.push_str("\n</context>\n");
    }
    prompt.push_str("\n<input>\n");
    prompt.push_str(&request.input.display());
    prompt.push_str("\n</input>\n");
    if !findings.is_empty() {
        prompt.push_str("\n<validation-findings>\n");
        for finding in findings {
            prompt.push_str(finding);
            prompt.push('\n');
        }
        prompt.push_str("</validation-findings>\n");
    }
    prompt
}

/// Convert an outcome into a runtime value for Axiom code.
pub fn outcome_value(outcome: &IterationOutcome) -> Value {
    let candidate = match &outcome.accepted {
        Some(candidate) => Value::Variant(
            "Some".to_string(),
            vec![Value::Record(vec![
                ("reference".to_string(), Value::Str(candidate.reference())),
                (
                    "content_hash".to_string(),
                    Value::Str(candidate.content_hash.clone()),
                ),
                ("content".to_string(), Value::Str(candidate.content.clone())),
            ])],
        ),
        None => Value::Variant("None".to_string(), Vec::new()),
    };
    Value::Record(vec![
        (
            "status".to_string(),
            Value::Str(outcome.status.as_str().to_string()),
        ),
        ("attempts".to_string(), Value::Int(outcome.attempts as i64)),
        ("candidate".to_string(), candidate),
        (
            "validation".to_string(),
            Value::List(outcome.validations.iter().map(attempt_value).collect()),
        ),
        (
            "evidence".to_string(),
            Value::List(
                outcome
                    .evidence
                    .iter()
                    .map(|evidence| {
                        Value::Record(vec![
                            ("kind".to_string(), Value::Str(evidence.kind.clone())),
                            (
                                "reference".to_string(),
                                Value::Str(evidence.reference.clone()),
                            ),
                            ("detail".to_string(), Value::Str(evidence.detail.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("reason".to_string(), Value::Str(outcome.reason.clone())),
    ])
}

fn attempt_value(attempt: &AttemptValidation) -> Value {
    Value::Record(vec![
        ("attempt".to_string(), Value::Int(attempt.attempt as i64)),
        (
            "candidate".to_string(),
            Value::Str(attempt.candidate.clone()),
        ),
        (
            "results".to_string(),
            Value::List(
                attempt
                    .results
                    .iter()
                    .map(|result| {
                        Value::Record(vec![
                            (
                                "validator".to_string(),
                                Value::Str(result.validator.clone()),
                            ),
                            (
                                "kind".to_string(),
                                Value::Str(result.kind.as_str().to_string()),
                            ),
                            (
                                "status".to_string(),
                                Value::Str(result.status.as_str().to_string()),
                            ),
                            ("required".to_string(), Value::Bool(result.required)),
                            (
                                "findings".to_string(),
                                Value::List(
                                    result
                                        .findings
                                        .iter()
                                        .map(|finding| Value::Str(finding.clone()))
                                        .collect(),
                                ),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

/// A convenience wrapper charging a fresh ledger.
pub fn iterate(
    agent: &AgentSpec,
    request: &IterationRequest,
    provider: &mut dyn ModelProvider,
    validators: &ValidatorRegistry,
) -> IterationOutcome {
    let mut ledger = BudgetLedger::new(agent.limits);
    run_iteration(agent, request, provider, validators, &mut ledger, &|_| {
        false
    })
}

/// Report a budget exhaustion as a stable string (shared with the host).
pub fn budget_reason(exhausted: &BudgetExhausted) -> String {
    format!("budget exhausted: {}", exhausted.as_str())
}
