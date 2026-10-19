//! Convert a compiled `workspace` declaration into the runtime policy.
//!
//! The compiler's workspace model is descriptive; `axiom-filesystem` is the
//! enforcing boundary. This module is the bridge: it extracts `root`, `read`,
//! `write`, `deny`, `isolation`, and `limits` from the AST and builds a
//! [`WorkspacePolicy`], reporting malformed declarations instead of guessing.

use axiom_filesystem::{IsolationMode, Limits, WorkspacePolicy};
use axiom_syntax::ast::{Expr, ExprKind, WorkspaceDecl};
use axiom_syntax::Span;

/// Build the runtime policy denoted by a `workspace` declaration.
pub fn policy(workspace: &WorkspaceDecl) -> Result<WorkspacePolicy, Vec<(Span, String)>> {
    let mut issues: Vec<(Span, String)> = Vec::new();

    let root = match workspace.member("root") {
        Some(Expr {
            kind: ExprKind::Str(value),
            ..
        }) => value.clone(),
        Some(expr) => {
            issues.push((expr.span, "`root` must be a string literal".to_string()));
            ".".to_string()
        }
        None => {
            issues.push((
                workspace.span,
                "workspace is missing required `root`".to_string(),
            ));
            ".".to_string()
        }
    };

    let read = string_list(workspace, "read", &mut issues).unwrap_or_default();
    let write = string_list(workspace, "write", &mut issues).unwrap_or_default();
    let deny = string_list(workspace, "deny", &mut issues).unwrap_or_default();

    let isolation = match workspace.member("isolation") {
        Some(Expr {
            kind: ExprKind::Ident(name),
            span,
        }) => match IsolationMode::parse(name) {
            Ok(mode) => mode,
            Err(err) => {
                issues.push((*span, err.to_string()));
                IsolationMode::ReadOnly
            }
        },
        Some(expr) => {
            issues.push((expr.span, "`isolation` must be an identifier".to_string()));
            IsolationMode::ReadOnly
        }
        None => {
            issues.push((
                workspace.span,
                "workspace is missing required `isolation`".to_string(),
            ));
            IsolationMode::ReadOnly
        }
    };

    let limits = limits(workspace, &mut issues);

    if !issues.is_empty() {
        return Err(issues);
    }

    let read_refs: Vec<&str> = read.iter().map(String::as_str).collect();
    let write_refs: Vec<&str> = write.iter().map(String::as_str).collect();
    let deny_refs: Vec<&str> = deny.iter().map(String::as_str).collect();
    WorkspacePolicy::build(
        &root,
        &read_refs,
        &write_refs,
        &deny_refs,
        isolation,
        limits,
    )
    .map_err(|err| vec![(workspace.span, err.to_string())])
}

fn string_list(
    workspace: &WorkspaceDecl,
    field: &str,
    issues: &mut Vec<(Span, String)>,
) -> Option<Vec<String>> {
    let Some(expr) = workspace.member(field) else {
        return Some(Vec::new());
    };
    match &expr.kind {
        ExprKind::List(items) => {
            let mut out = Vec::new();
            for item in items {
                if let ExprKind::Str(value) = &item.value.kind {
                    out.push(value.clone());
                } else {
                    issues.push((
                        item.value.span,
                        format!("`{field}` entries must be string literals"),
                    ));
                }
            }
            Some(out)
        }
        _ => {
            issues.push((expr.span, format!("`{field}` must be a list of strings")));
            None
        }
    }
}

fn limits(workspace: &WorkspaceDecl, issues: &mut Vec<(Span, String)>) -> Limits {
    let Some(expr) = workspace.member("limits") else {
        return Limits::default();
    };
    let ExprKind::Record(members) = &expr.kind else {
        issues.push((expr.span, "`limits` must be a record".to_string()));
        return Limits::default();
    };
    let mut limits = Limits::default();
    for member in members {
        let value = match &member.value.kind {
            ExprKind::Int(value) if *value >= 0 => *value as u64,
            _ => {
                issues.push((
                    member.value.span,
                    format!("limit `{}` must be a non-negative integer", member.name),
                ));
                continue;
            }
        };
        match member.name.as_str() {
            "max_files" => limits.max_files = Some(value),
            "max_file_bytes" => limits.max_file_bytes = Some(value),
            "max_changed_files" => limits.max_changed_files = Some(value),
            other => issues.push((member.span, format!("unknown workspace limit `{other}`"))),
        }
    }
    limits
}
