//! API authorization policy enforced at the server boundary.
//!
//! Policies come from `axiom.yaml` (`api_policies`) and are checked before an
//! exported API handler runs. A missing policy fails closed: an API is not
//! callable just because it was exported.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use axiom_config::{ApiPolicyConfig, ProjectConfig};

/// The authenticated caller. Capabilities are granted by configuration or an
/// identity provider, never by the request body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub id: String,
    pub tenant: String,
    pub capabilities: BTreeSet<String>,
}

impl Principal {
    pub fn new(id: impl Into<String>, tenant: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            tenant: tenant.into(),
            capabilities: BTreeSet::new(),
        }
    }

    pub fn with_capability(mut self, capability: impl Into<String>) -> Self {
        self.capabilities.insert(capability.into());
        self
    }

    pub fn can(&self, capability: &str) -> bool {
        self.capabilities.contains(capability)
    }
}

/// The policy attached to one exported API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiPolicy {
    pub name: String,
    pub authentication_required: bool,
    pub required_capabilities: Vec<String>,
    pub data_class: Option<String>,
    pub version: Option<String>,
}

impl ApiPolicy {
    pub fn from_config(name: &str, config: &ApiPolicyConfig) -> Self {
        Self {
            name: name.to_string(),
            authentication_required: config.authentication.as_deref() == Some("required"),
            required_capabilities: config.requires.clone(),
            data_class: config.data_class.clone(),
            version: config.version.clone(),
        }
    }
}

/// Why an API call was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessError {
    MissingPolicy(String),
    Unauthenticated(String),
    Forbidden { api: String, capability: String },
    TenantMismatch { expected: String, actual: String },
}

impl fmt::Display for AccessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AccessError::MissingPolicy(api) => {
                write!(formatter, "`{api}` has no API policy and is not callable")
            }
            AccessError::Unauthenticated(api) => {
                write!(formatter, "`{api}` requires authentication")
            }
            AccessError::Forbidden { api, capability } => {
                write!(formatter, "`{api}` requires capability `{capability}`")
            }
            AccessError::TenantMismatch { expected, actual } => {
                write!(
                    formatter,
                    "tenant `{actual}` is not allowed to access `{expected}`"
                )
            }
        }
    }
}

/// Authorization policies keyed by exported API name.
#[derive(Debug, Clone, Default)]
pub struct ApiGuard {
    policies: BTreeMap<String, ApiPolicy>,
}

impl ApiGuard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_config(config: &ProjectConfig) -> Self {
        let mut guard = Self::new();
        for (name, policy) in &config.api_policies {
            guard.register(ApiPolicy::from_config(name, policy));
        }
        guard
    }

    pub fn register(&mut self, policy: ApiPolicy) {
        self.policies.insert(policy.name.clone(), policy);
    }

    pub fn policy(&self, api: &str) -> Option<&ApiPolicy> {
        self.policies.get(api)
    }

    pub fn len(&self) -> usize {
        self.policies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.policies.is_empty()
    }

    /// Authorize a call before the handler runs. Returns the policy so callers
    /// can apply its data class and cache rules.
    pub fn authorize(
        &self,
        api: &str,
        caller: Option<&Principal>,
    ) -> Result<&ApiPolicy, AccessError> {
        let policy = self
            .policies
            .get(api)
            .ok_or_else(|| AccessError::MissingPolicy(api.to_string()))?;
        let caller = match (policy.authentication_required, caller) {
            (true, None) => return Err(AccessError::Unauthenticated(api.to_string())),
            (_, None) => return Ok(policy),
            (_, Some(caller)) => caller,
        };
        for capability in &policy.required_capabilities {
            if !caller.can(capability) {
                return Err(AccessError::Forbidden {
                    api: api.to_string(),
                    capability: capability.clone(),
                });
            }
        }
        Ok(policy)
    }

    /// Authorize a call that reads one tenant's data.
    pub fn authorize_tenant(
        &self,
        api: &str,
        caller: Option<&Principal>,
        tenant: &str,
    ) -> Result<&ApiPolicy, AccessError> {
        let policy = self.authorize(api, caller)?;
        let Some(caller) = caller else {
            return Ok(policy);
        };
        if caller.tenant != tenant {
            return Err(AccessError::TenantMismatch {
                expected: tenant.to_string(),
                actual: caller.tenant.clone(),
            });
        }
        Ok(policy)
    }
}
