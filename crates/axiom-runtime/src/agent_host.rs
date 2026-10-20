//! A host that exposes agent iteration to Axiom code.
//!
//! Only `agents.iterate` and `runtime.parallel_map` are reachable. Acceptance
//! and validator evidence are produced here, never by the model or the calling
//! Axiom function. Durable execution is provided by [`crate::DurableHost`],
//! which composes over this host; `runtime.run_durable` and `runtime.checkpoint`
//! are refused here unless that host is in front. `runtime.retry` is never an
//! implicit retry.

use std::cell::RefCell;
use std::collections::BTreeMap;

use axiom_config::ProjectConfig;
use axiom_ir::Program;
use axiom_providers::ProviderRegistry;

use crate::agents::{
    run_iteration, AgentSpec, IterationOutcome, IterationRequest, IterationStatus,
};
use crate::budget::{BudgetLedger, Budgets};
use crate::host::Host;
use crate::validation::{Validator, ValidatorRegistry};
use crate::value::Value;

/// A record of one iteration the host performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IterationTrace {
    pub agent: String,
    pub status: String,
    pub attempts: u32,
    pub accepted_hash: Option<String>,
    pub reason: String,
}

/// A host backed by configured providers, registered agents, and validators.
pub struct AgentHost {
    config: ProjectConfig,
    providers: ProviderRegistry,
    agents: BTreeMap<String, AgentSpec>,
    validators: ValidatorRegistry,
    traces: RefCell<Vec<IterationTrace>>,
}

impl AgentHost {
    /// Build a host for a project configuration with mock providers and the
    /// offline builtin validators.
    pub fn new(config: ProjectConfig) -> Self {
        AgentHost {
            config,
            providers: ProviderRegistry::with_mock(),
            agents: BTreeMap::new(),
            validators: ValidatorRegistry::with_builtins(),
            traces: RefCell::new(Vec::new()),
        }
    }

    /// Replace the provider registry (e.g. to register networked adapters).
    pub fn with_providers(mut self, providers: ProviderRegistry) -> Self {
        self.providers = providers;
        self
    }

    pub fn register_agent(&mut self, spec: AgentSpec) {
        self.agents.insert(spec.name.clone(), spec);
    }

    /// Register every `agent` declaration in a lowered program.
    pub fn register_agents_from_program(&mut self, program: &Program) -> Result<(), String> {
        for module in &program.modules {
            for agent in &module.agents {
                let spec = AgentSpec::from_ir(agent)?;
                self.register_agent(spec);
            }
        }
        Ok(())
    }

    pub fn register_validator(&mut self, validator: Box<dyn Validator>) {
        self.validators.register(validator);
    }

    pub fn agent(&self, name: &str) -> Option<&AgentSpec> {
        self.agents.get(name)
    }

    pub fn validators(&self) -> &ValidatorRegistry {
        &self.validators
    }

    pub fn traces(&self) -> Vec<IterationTrace> {
        self.traces.borrow().clone()
    }

    fn iterate(&self, args: &[(Option<String>, Value)]) -> Result<Value, String> {
        let outcome = self.run(args)?;
        Ok(crate::agents::outcome_value(&outcome))
    }

    fn run(&self, args: &[(Option<String>, Value)]) -> Result<IterationOutcome, String> {
        let agent_name = string_arg(args, "agent", 0)?;
        let spec = self
            .agents
            .get(&agent_name)
            .ok_or_else(|| format!("unknown agent `{agent_name}`"))?;
        let mut provider = self
            .providers
            .build(&self.config, &spec.model)
            .map_err(|error| error.to_string())?;
        let input = arg(args, "input", 1).cloned().unwrap_or(Value::Unit);
        let max_attempts = match arg(args, "max_attempts", 2) {
            Some(Value::Int(value)) if *value >= 0 => *value as u32,
            Some(_) => return Err("`max_attempts` must be a non-negative integer".to_string()),
            None => 1,
        };
        let validate = string_list_arg(args, "validate", 3)?;
        let action_hash = optional_string_arg(args, "action_hash", 4);
        let request = IterationRequest {
            input,
            max_attempts,
            validate,
            action_hash,
        };

        let mut ledger = BudgetLedger::new(spec.limits);
        let outcome = run_iteration(
            spec,
            &request,
            provider.as_mut(),
            &self.validators,
            &mut ledger,
            &|_| false,
        );
        self.traces.borrow_mut().push(IterationTrace {
            agent: spec.name.clone(),
            status: outcome.status.as_str().to_string(),
            attempts: outcome.attempts,
            accepted_hash: outcome
                .accepted
                .as_ref()
                .map(|candidate| candidate.content_hash.clone()),
            reason: outcome.reason.clone(),
        });
        Ok(outcome)
    }

    fn parallel_map(&self, args: &[(Option<String>, Value)]) -> Result<Value, String> {
        let agent_name = string_arg(args, "agent", 0)?;
        let spec = self
            .agents
            .get(&agent_name)
            .ok_or_else(|| format!("unknown agent `{agent_name}`"))?;
        let inputs = match arg(args, "inputs", 1) {
            Some(Value::List(items)) => items.clone(),
            Some(_) => return Err("`inputs` must be a list".to_string()),
            None => return Err("missing `inputs`".to_string()),
        };
        let max_attempts = match arg(args, "max_attempts", 2) {
            Some(Value::Int(value)) if *value >= 0 => *value as u32,
            Some(_) => return Err("`max_attempts` must be a non-negative integer".to_string()),
            None => 1,
        };
        let validate = string_list_arg(args, "validate", 3)?;
        let fail_fast =
            matches!(arg(args, "policy", 4), Some(Value::Str(policy)) if policy == "fail_fast");

        let mut provider = self
            .providers
            .build(&self.config, &spec.model)
            .map_err(|error| error.to_string())?;
        let budgets = Budgets {
            max_calls: spec
                .limits
                .max_calls
                .saturating_mul(inputs.len().max(1) as u32),
            ..spec.limits
        };
        let mut ledger = BudgetLedger::new(budgets);
        let mut outcomes: Vec<IterationOutcome> = Vec::with_capacity(inputs.len());
        for input in inputs {
            let request = IterationRequest {
                input,
                max_attempts,
                validate: validate.clone(),
                action_hash: None,
            };
            let outcome = run_iteration(
                spec,
                &request,
                provider.as_mut(),
                &self.validators,
                &mut ledger,
                &|_| false,
            );
            let stop = fail_fast && outcome.status != IterationStatus::Accepted;
            outcomes.push(outcome);
            if stop {
                break;
            }
        }

        let accepted = outcomes
            .iter()
            .filter(|outcome| outcome.status == IterationStatus::Accepted)
            .count();
        Ok(Value::Record(vec![
            (
                "status".to_string(),
                Value::Str(if accepted == outcomes.len() {
                    "accepted".to_string()
                } else {
                    "failed".to_string()
                }),
            ),
            ("accepted".to_string(), Value::Int(accepted as i64)),
            (
                "results".to_string(),
                Value::List(outcomes.iter().map(crate::agents::outcome_value).collect()),
            ),
        ]))
    }
}

impl Host for AgentHost {
    fn call(
        &self,
        namespace: &str,
        method: &str,
        args: &[(Option<String>, Value)],
    ) -> Result<Value, String> {
        match namespace {
            "agents" => match method {
                "iterate" => self.iterate(args),
                other => Err(format!("unsupported agents method `{other}`")),
            },
            "runtime" => match method {
                "parallel_map" => self.parallel_map(args),
                "checkpoint" | "run_durable" => Err(format!(
                    "`runtime.{method}` requires the durable execution host (`axiom_runtime::DurableHost`)"
                )),
                "retry" => Err(
                    "`runtime.retry` does not retry implicitly; use `agents.iterate` or resume a durable run"
                        .to_string(),
                ),
                other => Err(format!("unsupported runtime method `{other}`")),
            },
            other => Err(format!(
                "namespace `{other}` is not supported by the agent host"
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

fn string_list_arg(
    args: &[(Option<String>, Value)],
    name: &str,
    index: usize,
) -> Result<Vec<String>, String> {
    match arg(args, name, index) {
        Some(Value::List(items)) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Value::Str(value) => out.push(value.clone()),
                    _ => return Err(format!("`{name}` entries must be strings")),
                }
            }
            Ok(out)
        }
        Some(_) => Err(format!("`{name}` must be a list of strings")),
        None => Ok(Vec::new()),
    }
}
