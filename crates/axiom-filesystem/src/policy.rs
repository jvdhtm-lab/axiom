//! Workspace permission policy and isolation modes (agents.md §6.1).
//!
//! A [`WorkspacePolicy`] is the evaluated form of a `workspace` declaration:
//! a root, a read set, a write set, a deny set, an isolation mode, and limits.
//! Authority is the intersection of every policy that applies, so a derived
//! workspace can only narrow its base.

use crate::path::{normalize, Pattern};
use crate::FsError;

/// The required workspace isolation modes (agents.md §6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationMode {
    /// Shared immutable view; writes are refused.
    ReadOnly,
    /// Private overlay over a base snapshot; changes stay private until submitted.
    CopyOnWrite,
    /// Isolated Git worktree/branch.
    GitWorktree,
    /// A privileged integration lock for an authorized integration operation.
    ExclusiveIntegration,
}

impl IsolationMode {
    /// Parse an isolation mode, failing closed on unknown names.
    pub fn parse(name: &str) -> Result<IsolationMode, FsError> {
        match name {
            "read_only" => Ok(IsolationMode::ReadOnly),
            "copy_on_write" => Ok(IsolationMode::CopyOnWrite),
            "git_worktree" => Ok(IsolationMode::GitWorktree),
            "exclusive_integration" => Ok(IsolationMode::ExclusiveIntegration),
            other => Err(FsError::UnknownIsolation(other.to_string())),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            IsolationMode::ReadOnly => "read_only",
            IsolationMode::CopyOnWrite => "copy_on_write",
            IsolationMode::GitWorktree => "git_worktree",
            IsolationMode::ExclusiveIntegration => "exclusive_integration",
        }
    }

    /// Whether this mode forbids all writes.
    pub fn is_read_only(self) -> bool {
        matches!(self, IsolationMode::ReadOnly)
    }
}

/// Workspace quotas (agents.md §6.1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Limits {
    pub max_files: Option<u64>,
    pub max_file_bytes: Option<u64>,
    pub max_changed_files: Option<u64>,
}

/// Why an access was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DenyReason {
    OutsideRoot,
    NotInReadSet,
    NotInWriteSet,
    DeniedByPattern(String),
    Protected(String),
}

impl std::fmt::Display for DenyReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DenyReason::OutsideRoot => write!(f, "outside the workspace root"),
            DenyReason::NotInReadSet => write!(f, "not in the read set"),
            DenyReason::NotInWriteSet => write!(f, "not in the write set"),
            DenyReason::DeniedByPattern(p) => write!(f, "matched deny pattern `{p}`"),
            DenyReason::Protected(p) => {
                write!(f, "`{p}` is protected and needs separate authorization")
            }
        }
    }
}

/// The outcome of a permission check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Access {
    Allow,
    Deny(DenyReason),
}

impl Access {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Access::Allow)
    }
}

/// Files that require separate authorization by default (agents.md §6.1).
pub fn protected_reason(path: &str) -> Option<&'static str> {
    if path == "axiom.yaml" {
        return Some("axiom.yaml");
    }
    if path == ".git" || path.starts_with(".git/") {
        return Some("VCS metadata");
    }
    if path == "db/migrations" || path.starts_with("db/migrations/") {
        return Some("db/migrations");
    }
    None
}

/// An evaluated workspace declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePolicy {
    /// Normalized root, `""` for the project root.
    pub root: String,
    pub read: Vec<Pattern>,
    pub write: Vec<Pattern>,
    pub deny: Vec<Pattern>,
    pub isolation: IsolationMode,
    pub limits: Limits,
}

impl WorkspacePolicy {
    /// Build a policy from string patterns, normalizing and validating each.
    pub fn build(
        root: &str,
        read: &[&str],
        write: &[&str],
        deny: &[&str],
        isolation: IsolationMode,
        limits: Limits,
    ) -> Result<WorkspacePolicy, FsError> {
        let root = normalize_root(root)?;
        let read = parse_patterns(read)?;
        let write = parse_patterns(write)?;
        let deny = parse_patterns(deny)?;
        Ok(WorkspacePolicy {
            root,
            read,
            write,
            deny,
            isolation,
            limits,
        })
    }

    /// Check read access, returning the normalized path.
    pub fn check_read(&self, path: &str) -> Result<String, FsError> {
        let normalized = normalize(path)?;
        match self.read_access(&normalized) {
            Access::Allow => Ok(normalized),
            Access::Deny(reason) => Err(FsError::Denied {
                path: normalized,
                reason,
            }),
        }
    }

    /// Check write access, returning the normalized path.
    pub fn check_write(&self, path: &str) -> Result<String, FsError> {
        let normalized = normalize(path)?;
        if self.isolation.is_read_only() {
            return Err(FsError::ReadOnly(normalized));
        }
        match self.write_access(&normalized) {
            Access::Allow => Ok(normalized),
            Access::Deny(reason) => Err(FsError::Denied {
                path: normalized,
                reason,
            }),
        }
    }

    fn read_access(&self, path: &str) -> Access {
        self.access(path, &self.read, DenyReason::NotInReadSet)
    }

    fn write_access(&self, path: &str) -> Access {
        self.access(path, &self.write, DenyReason::NotInWriteSet)
    }

    fn access(&self, path: &str, set: &[Pattern], missing: DenyReason) -> Access {
        if !self.under_root(path) {
            return Access::Deny(DenyReason::OutsideRoot);
        }
        if let Some(reason) = protected_reason(path) {
            return Access::Deny(DenyReason::Protected(reason.to_string()));
        }
        if let Some(pattern) = self.deny.iter().find(|p| p.matches(path)) {
            return Access::Deny(DenyReason::DeniedByPattern(pattern.as_str().to_string()));
        }
        if set.iter().any(|p| p.matches(path)) {
            Access::Allow
        } else {
            Access::Deny(missing)
        }
    }

    fn under_root(&self, path: &str) -> bool {
        self.root.is_empty() || path == self.root || path.starts_with(&format!("{}/", self.root))
    }
}

fn normalize_root(root: &str) -> Result<String, FsError> {
    if root.is_empty() || root == "." {
        return Ok(String::new());
    }
    let unified = root.replace('\\', "/");
    if unified == "./" {
        return Ok(String::new());
    }
    normalize(&unified)
}

fn parse_patterns(patterns: &[&str]) -> Result<Vec<Pattern>, FsError> {
    patterns.iter().map(|p| Pattern::parse(p)).collect()
}

/// The intersection of every policy that applies to a path.
///
/// Access is allowed only when every policy allows it, so combining policies can
/// only remove authority. This is the runtime's "derived workspaces narrow, they
/// do not widen" rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectivePolicy {
    policies: Vec<WorkspacePolicy>,
}

impl EffectivePolicy {
    pub fn single(policy: WorkspacePolicy) -> EffectivePolicy {
        EffectivePolicy {
            policies: vec![policy],
        }
    }

    /// Combine a base policy with a derived one, checking that the derived
    /// workspace does not widen authority.
    pub fn narrow(
        base: WorkspacePolicy,
        derived: WorkspacePolicy,
    ) -> Result<EffectivePolicy, FsError> {
        validate_narrowing(&base, &derived)?;
        Ok(EffectivePolicy {
            policies: vec![base, derived],
        })
    }

    pub fn check_read(&self, path: &str) -> Result<String, FsError> {
        let mut normalized = None;
        for policy in &self.policies {
            normalized = Some(policy.check_read(path)?);
        }
        normalized.ok_or_else(|| FsError::InvalidPath(path.to_string()))
    }

    pub fn check_write(&self, path: &str) -> Result<String, FsError> {
        let mut normalized = None;
        for policy in &self.policies {
            normalized = Some(policy.check_write(path)?);
        }
        normalized.ok_or_else(|| FsError::InvalidPath(path.to_string()))
    }

    /// The tightest limits across all policies.
    pub fn limits(&self) -> Limits {
        let mut limits = Limits::default();
        let mut seen = false;
        for policy in &self.policies {
            limits = merge_limits(limits, policy.limits, seen);
            seen = true;
        }
        limits
    }
}

fn merge_limits(a: Limits, b: Limits, seen: bool) -> Limits {
    if !seen {
        return b;
    }
    Limits {
        max_files: min_opt(a.max_files, b.max_files),
        max_file_bytes: min_opt(a.max_file_bytes, b.max_file_bytes),
        max_changed_files: min_opt(a.max_changed_files, b.max_changed_files),
    }
}

fn min_opt(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, b) => b,
    }
}

fn validate_narrowing(base: &WorkspacePolicy, derived: &WorkspacePolicy) -> Result<(), FsError> {
    if base.isolation.is_read_only() && !derived.isolation.is_read_only() {
        return Err(FsError::Widening(
            "base is read_only but derived allows writes".to_string(),
        ));
    }
    if !derived.root.is_empty()
        && !base.root.is_empty()
        && !(derived.root == base.root || derived.root.starts_with(&format!("{}/", base.root)))
    {
        return Err(FsError::Widening(format!(
            "derived root `{}` is outside base root `{}`",
            derived.root, base.root
        )));
    }
    for pattern in &derived.read {
        if !base.read.iter().any(|p| p.covers(pattern)) {
            return Err(FsError::Widening(format!(
                "read pattern `{}` is not covered by the base",
                pattern.as_str()
            )));
        }
    }
    for pattern in &derived.write {
        if !base.write.iter().any(|p| p.covers(pattern)) {
            return Err(FsError::Widening(format!(
                "write pattern `{}` is not covered by the base",
                pattern.as_str()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> WorkspacePolicy {
        WorkspacePolicy::build(
            ".",
            &["src/**"],
            &["src/frontend/**"],
            &["src/server/**"],
            IsolationMode::CopyOnWrite,
            Limits::default(),
        )
        .unwrap()
    }

    #[test]
    fn deny_and_protected_win_over_allow() {
        let policy = base();
        assert!(policy.check_read("src/frontend/app.ax").is_ok());
        assert!(policy.check_read("src/server/app.ax").is_err());
        assert!(policy.check_read("axiom.yaml").is_err());
        assert!(policy.check_read("README.md").is_err());
        assert!(policy.check_read("../escape").is_err());
    }

    #[test]
    fn read_only_refuses_writes() {
        let ro = WorkspacePolicy::build(
            ".",
            &["**"],
            &["**"],
            &[],
            IsolationMode::ReadOnly,
            Limits::default(),
        )
        .unwrap();
        assert!(matches!(
            ro.check_write("src/a.ax"),
            Err(FsError::ReadOnly(_))
        ));
    }

    #[test]
    fn derived_must_not_widen() {
        let wide = WorkspacePolicy::build(
            ".",
            &["src/**"],
            &["src/**"],
            &[],
            IsolationMode::CopyOnWrite,
            Limits::default(),
        )
        .unwrap();
        let narrow = WorkspacePolicy::build(
            ".",
            &["src/frontend/**"],
            &["src/frontend/**"],
            &[],
            IsolationMode::CopyOnWrite,
            Limits::default(),
        )
        .unwrap();
        assert!(EffectivePolicy::narrow(wide.clone(), narrow).is_ok());

        let wider = WorkspacePolicy::build(
            ".",
            &["src/**", "db/**"],
            &["src/**"],
            &[],
            IsolationMode::CopyOnWrite,
            Limits::default(),
        )
        .unwrap();
        assert!(matches!(
            EffectivePolicy::narrow(wide, wider),
            Err(FsError::Widening(_))
        ));
    }
}
