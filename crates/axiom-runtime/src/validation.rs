//! Runtime-owned validation for agent candidates.
//!
//! Validation is ordinary typed work registered with the runtime. Only the
//! runtime constructs [`ValidationResult`]s; a model's text claiming a test
//! passed never substitutes for a validator that actually ran. Required
//! validation fails closed: a missing validator, an errored validator, an
//! unsupported check, or a failure does not pass. `ModelEvaluated` and
//! `HumanReviewed` results are advisory and never satisfy a required runtime
//! check.

use std::collections::BTreeMap;

use crate::agents::Candidate;

/// Who performed a validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationKind {
    /// Deterministic, machine-executed (compile, scope, schema, tests).
    RuntimeCheck,
    /// A model's assessment against a rubric; useful, not proof.
    ModelEvaluated,
    /// An explicit human decision.
    HumanReviewed,
}

impl ValidationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ValidationKind::RuntimeCheck => "runtime_check",
            ValidationKind::ModelEvaluated => "model_evaluated",
            ValidationKind::HumanReviewed => "human_reviewed",
        }
    }
}

/// The outcome of one validator run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationStatus {
    Passed,
    Failed,
    /// The validator itself errored; fails closed.
    Error,
    /// The check is not supported in this environment; fails closed.
    Unsupported,
    /// No validator is registered under the requested name; fails closed.
    Missing,
}

impl ValidationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ValidationStatus::Passed => "passed",
            ValidationStatus::Failed => "failed",
            ValidationStatus::Error => "error",
            ValidationStatus::Unsupported => "unsupported",
            ValidationStatus::Missing => "missing",
        }
    }
}

/// A reference to machine-checkable evidence (never a model's assertion).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRef {
    pub kind: String,
    pub reference: String,
    pub detail: String,
}

impl EvidenceRef {
    pub fn new(
        kind: impl Into<String>,
        reference: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        EvidenceRef {
            kind: kind.into(),
            reference: reference.into(),
            detail: detail.into(),
        }
    }
}

/// The runtime-defined validation result handed back to the iteration loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationResult {
    pub validator: String,
    pub kind: ValidationKind,
    /// Whether a failure blocks acceptance (required) or is advisory.
    pub required: bool,
    pub status: ValidationStatus,
    /// Whether a failure is repairable by another attempt.
    pub repairable: bool,
    pub findings: Vec<String>,
    pub evidence: Vec<EvidenceRef>,
}

impl ValidationResult {
    pub fn new(
        validator: impl Into<String>,
        kind: ValidationKind,
        required: bool,
        status: ValidationStatus,
    ) -> Self {
        ValidationResult {
            validator: validator.into(),
            kind,
            required,
            status,
            repairable: true,
            findings: Vec::new(),
            evidence: Vec::new(),
        }
    }

    pub fn passed(validator: impl Into<String>, kind: ValidationKind, required: bool) -> Self {
        ValidationResult::new(validator, kind, required, ValidationStatus::Passed)
    }

    pub fn failed(
        validator: impl Into<String>,
        kind: ValidationKind,
        required: bool,
        finding: impl Into<String>,
    ) -> Self {
        ValidationResult::new(validator, kind, required, ValidationStatus::Failed)
            .with_finding(finding)
    }

    pub fn missing(validator: impl Into<String>) -> Self {
        let validator = validator.into();
        ValidationResult::new(
            validator.clone(),
            ValidationKind::RuntimeCheck,
            true,
            ValidationStatus::Missing,
        )
        .with_finding(format!("no validator registered as `{validator}`"))
    }

    pub fn with_finding(mut self, finding: impl Into<String>) -> Self {
        self.findings.push(finding.into());
        self
    }

    pub fn with_evidence(mut self, evidence: EvidenceRef) -> Self {
        self.evidence.push(evidence);
        self
    }

    pub fn with_repairable(mut self, repairable: bool) -> Self {
        self.repairable = repairable;
        self
    }

    /// Whether this result blocks acceptance.
    pub fn blocks_acceptance(&self) -> bool {
        self.required && self.status != ValidationStatus::Passed
    }
}

/// A registered validator.
pub trait Validator {
    fn name(&self) -> &str;
    fn kind(&self) -> ValidationKind;
    /// Whether a failure of this validator blocks acceptance.
    fn required(&self) -> bool;
    fn validate(&self, candidate: &Candidate) -> ValidationResult;
}

/// An in-process registry of validators the runtime may run.
#[derive(Default)]
pub struct ValidatorRegistry {
    validators: BTreeMap<String, Box<dyn Validator>>,
}

impl ValidatorRegistry {
    pub fn new() -> Self {
        ValidatorRegistry::default()
    }

    /// A registry with the offline builtin checks.
    pub fn with_builtins() -> Self {
        let mut registry = ValidatorRegistry::new();
        registry.register(Box::new(AxCompileValidator));
        registry
    }

    pub fn register(&mut self, validator: Box<dyn Validator>) {
        self.validators
            .insert(validator.name().to_string(), validator);
    }

    pub fn is_registered(&self, name: &str) -> bool {
        self.validators.contains_key(name)
    }

    /// Registered names, in sorted order.
    pub fn names(&self) -> Vec<&str> {
        self.validators.keys().map(String::as_str).collect()
    }

    /// Run the named validator, or return a fail-closed `Missing` result.
    pub fn validate(&self, name: &str, candidate: &Candidate) -> ValidationResult {
        match self.validators.get(name) {
            Some(validator) => validator.validate(candidate),
            None => ValidationResult::missing(name),
        }
    }
}

/// `checks.ax_compile`: the candidate must parse as Axiom source.
pub struct AxCompileValidator;

impl Validator for AxCompileValidator {
    fn name(&self) -> &str {
        "checks.ax_compile"
    }

    fn kind(&self) -> ValidationKind {
        ValidationKind::RuntimeCheck
    }

    fn required(&self) -> bool {
        true
    }

    fn validate(&self, candidate: &Candidate) -> ValidationResult {
        let parsed = axiom_syntax::parse(&candidate.content);
        let errors: Vec<_> = parsed
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.is_error())
            .collect();
        if errors.is_empty() {
            ValidationResult::passed(self.name(), self.kind(), self.required()).with_evidence(
                EvidenceRef::new(
                    "parse",
                    candidate.content_hash.clone(),
                    "candidate parses as an Axiom module",
                ),
            )
        } else {
            let mut result = ValidationResult::new(
                self.name(),
                self.kind(),
                self.required(),
                ValidationStatus::Failed,
            );
            for diagnostic in errors {
                result = result.with_finding(diagnostic.message.clone());
            }
            result
        }
    }
}

/// A validator with a scripted result, for tests and dry runs.
pub struct ScriptedValidator {
    name: String,
    kind: ValidationKind,
    required: bool,
    result: ValidationStatus,
    repairable: bool,
    findings: Vec<String>,
}

impl ScriptedValidator {
    pub fn new(name: impl Into<String>, result: ValidationStatus) -> Self {
        ScriptedValidator {
            name: name.into(),
            kind: ValidationKind::RuntimeCheck,
            required: true,
            result,
            repairable: true,
            findings: Vec::new(),
        }
    }

    pub fn advisory(mut self) -> Self {
        self.required = false;
        self
    }

    pub fn model_evaluated(mut self) -> Self {
        self.kind = ValidationKind::ModelEvaluated;
        self.required = false;
        self
    }

    pub fn with_repairable(mut self, repairable: bool) -> Self {
        self.repairable = repairable;
        self
    }

    pub fn with_finding(mut self, finding: impl Into<String>) -> Self {
        self.findings.push(finding.into());
        self
    }
}

impl Validator for ScriptedValidator {
    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ValidationKind {
        self.kind
    }

    fn required(&self) -> bool {
        self.required
    }

    fn validate(&self, _candidate: &Candidate) -> ValidationResult {
        let mut result =
            ValidationResult::new(self.name.clone(), self.kind, self.required, self.result)
                .with_repairable(self.repairable);
        for finding in &self.findings {
            result = result.with_finding(finding.clone());
        }
        result
    }
}
