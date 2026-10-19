//! A host that exposes secure workspaces and the artifact store to Axiom code.
//!
//! Authority comes from the workspace policy, which the host never bypasses.
//! Integration (`workspaces.integrate`) is deliberately not exposed: it requires
//! the privileged [`axiom_filesystem::IntegrationLock`] and an approved patch.

use std::cell::{Ref, RefCell, RefMut};
use std::collections::BTreeMap;

use axiom_artifacts::{ArtifactKind, ArtifactStore};
use axiom_filesystem::{FileChange, FileChangeKind, Patch, Snapshot, Workspace};

use crate::host::Host;
use crate::value::Value;

/// A host backed by one workspace and one artifact store.
pub struct WorkspaceHost {
    workspace: RefCell<Workspace>,
    artifacts: RefCell<ArtifactStore>,
    snapshots: RefCell<BTreeMap<String, Snapshot>>,
}

impl WorkspaceHost {
    pub fn new(workspace: Workspace, artifacts: ArtifactStore) -> WorkspaceHost {
        WorkspaceHost {
            workspace: RefCell::new(workspace),
            artifacts: RefCell::new(artifacts),
            snapshots: RefCell::new(BTreeMap::new()),
        }
    }

    pub fn workspace(&self) -> Ref<'_, Workspace> {
        self.workspace.borrow()
    }

    pub fn workspace_mut(&self) -> RefMut<'_, Workspace> {
        self.workspace.borrow_mut()
    }

    pub fn artifacts(&self) -> Ref<'_, ArtifactStore> {
        self.artifacts.borrow()
    }

    fn fs_call(&self, method: &str, args: &[(Option<String>, Value)]) -> Result<Value, String> {
        match method {
            "read_text" => {
                let path = string_arg(args, "path", 0)?;
                self.workspace
                    .borrow()
                    .read_text(&path)
                    .map(Value::Str)
                    .map_err(|err| err.to_string())
            }
            "list" => {
                let dir = optional_string_arg(args, "dir", 0).unwrap_or_default();
                self.workspace
                    .borrow()
                    .list(&dir)
                    .map(|paths| paths.into_iter().map(Value::Str).collect())
                    .map(Value::List)
                    .map_err(|err| err.to_string())
            }
            "search" => {
                let query = string_arg(args, "query", 0)?;
                self.workspace
                    .borrow()
                    .search(&query)
                    .map(|hits| {
                        hits.into_iter()
                            .map(|hit| {
                                Value::Record(vec![
                                    ("path".to_string(), Value::Str(hit.path)),
                                    ("line".to_string(), Value::Int(hit.line as i64)),
                                    ("text".to_string(), Value::Str(hit.text)),
                                ])
                            })
                            .collect()
                    })
                    .map(Value::List)
                    .map_err(|err| err.to_string())
            }
            "write_text" => {
                let path = string_arg(args, "path", 0)?;
                let contents = string_arg(args, "contents", 1)?;
                self.workspace
                    .borrow_mut()
                    .write_text(&path, &contents)
                    .map(|()| Value::Unit)
                    .map_err(|err| err.to_string())
            }
            "delete" => {
                let path = string_arg(args, "path", 0)?;
                self.workspace
                    .borrow_mut()
                    .delete(&path)
                    .map(|()| Value::Unit)
                    .map_err(|err| err.to_string())
            }
            "diff" => Ok(patch_value(&self.workspace.borrow().diff())),
            other => Err(format!("unsupported fs method `{other}`")),
        }
    }

    fn workspace_call(
        &self,
        method: &str,
        args: &[(Option<String>, Value)],
    ) -> Result<Value, String> {
        match method {
            "snapshot" => {
                let snapshot = self.workspace.borrow().snapshot();
                let revision = snapshot.revision().to_string();
                self.snapshots
                    .borrow_mut()
                    .insert(revision.clone(), snapshot);
                Ok(Value::Str(revision))
            }
            "restore" => {
                let revision = string_arg(args, "snapshot", 0)?;
                let snapshot = self
                    .snapshots
                    .borrow()
                    .get(&revision)
                    .cloned()
                    .ok_or_else(|| format!("unknown snapshot `{revision}`"))?;
                self.workspace.borrow_mut().restore(snapshot);
                Ok(Value::Unit)
            }
            "discard" => {
                self.workspace.borrow_mut().discard();
                Ok(Value::Unit)
            }
            "submit" => self
                .workspace
                .borrow()
                .submit()
                .map(|patch| patch_value(&patch))
                .map_err(|err| err.to_string()),
            "integrate" => Err(
                "integration requires the privileged IntegrationLock; it is not an agent API"
                    .to_string(),
            ),
            other => Err(format!("unsupported workspaces method `{other}`")),
        }
    }

    fn artifact_call(
        &self,
        method: &str,
        args: &[(Option<String>, Value)],
    ) -> Result<Value, String> {
        match method {
            "publish" => {
                let (kind, bytes) = if args.len() > 1 {
                    (
                        artifact_kind(&string_arg(args, "kind", 0)?)?,
                        bytes_arg(args, "bytes", 1)?,
                    )
                } else {
                    (ArtifactKind::Other, bytes_arg(args, "bytes", 0)?)
                };
                let id = self.artifacts.borrow_mut().publish(kind, bytes);
                Ok(Value::Str(id))
            }
            "fetch" => {
                let id = string_arg(args, "id", 0)?;
                self.artifacts
                    .borrow()
                    .fetch(&id)
                    .map(|artifact| Value::Bytes(artifact.bytes.clone()))
                    .ok_or_else(|| format!("unknown artifact `{id}`"))
            }
            "verify" => {
                let id = string_arg(args, "id", 0)?;
                Ok(Value::Bool(self.artifacts.borrow().verify(&id)))
            }
            "inspect" => {
                let id = string_arg(args, "id", 0)?;
                self.artifacts
                    .borrow()
                    .inspect(&id)
                    .map(|info| {
                        Value::Record(vec![
                            ("id".to_string(), Value::Str(info.id)),
                            (
                                "kind".to_string(),
                                Value::Str(info.kind.as_str().to_string()),
                            ),
                            ("byte_len".to_string(), Value::Int(info.byte_len as i64)),
                        ])
                    })
                    .ok_or_else(|| format!("unknown artifact `{id}`"))
            }
            other => Err(format!("unsupported artifacts method `{other}`")),
        }
    }

    fn patch_call(&self, method: &str, args: &[(Option<String>, Value)]) -> Result<Value, String> {
        match method {
            "check_conflicts" => {
                let patch = patch_from_value(
                    arg(args, "patch", 0).ok_or_else(|| "missing `patch`".to_string())?,
                )?;
                let workspace = self.workspace.borrow();
                let current = workspace.snapshot();
                let conflicts = patch.check_conflicts(&current);
                Ok(Value::List(
                    conflicts
                        .into_iter()
                        .map(|conflict| {
                            Value::Record(vec![
                                ("path".to_string(), Value::Str(conflict.path)),
                                ("reason".to_string(), Value::Str(conflict.reason)),
                            ])
                        })
                        .collect(),
                ))
            }
            "apply_set" => {
                Err("patches.apply_set is a privileged integration operation".to_string())
            }
            other => Err(format!("unsupported patches method `{other}`")),
        }
    }
}

impl Host for WorkspaceHost {
    fn call(
        &self,
        namespace: &str,
        method: &str,
        args: &[(Option<String>, Value)],
    ) -> Result<Value, String> {
        match namespace {
            "fs" => self.fs_call(method, args),
            "workspaces" => self.workspace_call(method, args),
            "artifacts" => self.artifact_call(method, args),
            "patches" => self.patch_call(method, args),
            "commands" => Err(
                "commands.run requires the ProcessCommandRunner and an approved tool grant"
                    .to_string(),
            ),
            other => Err(format!(
                "namespace `{other}` is not supported by the workspace host"
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

fn bytes_arg(
    args: &[(Option<String>, Value)],
    name: &str,
    index: usize,
) -> Result<Vec<u8>, String> {
    match arg(args, name, index) {
        Some(Value::Bytes(bytes)) => Ok(bytes.clone()),
        Some(Value::Str(text)) => Ok(text.as_bytes().to_vec()),
        Some(_) => Err(format!("`{name}` must be bytes or a string")),
        None => Err(format!("missing bytes `{name}`")),
    }
}

fn artifact_kind(name: &str) -> Result<ArtifactKind, String> {
    match name {
        "wasm_module" => Ok(ArtifactKind::WasmModule),
        "patch" => Ok(ArtifactKind::Patch),
        "ui_bundle" => Ok(ArtifactKind::UiBundle),
        "snapshot" => Ok(ArtifactKind::Snapshot),
        "other" => Ok(ArtifactKind::Other),
        other => Err(format!("unknown artifact kind `{other}`")),
    }
}

fn patch_value(patch: &Patch) -> Value {
    Value::Record(vec![
        (
            "base_revision".to_string(),
            Value::Str(patch.base_revision.clone()),
        ),
        (
            "changes".to_string(),
            Value::List(
                patch
                    .changes
                    .iter()
                    .map(|change| {
                        Value::Record(vec![
                            ("path".to_string(), Value::Str(change.path.clone())),
                            (
                                "kind".to_string(),
                                Value::Str(change.kind.as_str().to_string()),
                            ),
                            ("base_hash".to_string(), optional_str(&change.base_hash)),
                            ("new_hash".to_string(), optional_str(&change.new_hash)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

fn optional_str(value: &Option<String>) -> Value {
    value
        .as_ref()
        .map(|text| Value::Str(text.clone()))
        .unwrap_or(Value::Unit)
}

fn patch_from_value(value: &Value) -> Result<Patch, String> {
    let Value::Record(fields) = value else {
        return Err("patch must be a record".to_string());
    };
    let base_revision = field_str(fields, "base_revision")?;
    let Value::List(changes) = field(fields, "changes")? else {
        return Err("`changes` must be a list".to_string());
    };
    let mut parsed = Vec::new();
    for change in changes {
        let Value::Record(fields) = change else {
            return Err("patch change must be a record".to_string());
        };
        let path = field_str(fields, "path")?;
        let kind = match field_str(fields, "kind")?.as_str() {
            "added" => FileChangeKind::Added,
            "modified" => FileChangeKind::Modified,
            "deleted" => FileChangeKind::Deleted,
            other => return Err(format!("unknown patch change kind `{other}`")),
        };
        let base_hash = field_optional_str(fields, "base_hash")?;
        let new_hash = field_optional_str(fields, "new_hash")?;
        parsed.push(FileChange {
            path,
            kind,
            base_hash,
            new_hash,
        });
    }
    Ok(Patch::new(base_revision, parsed))
}

fn field<'a>(fields: &'a [(String, Value)], name: &str) -> Result<&'a Value, String> {
    fields
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
        .ok_or_else(|| format!("missing field `{name}`"))
}

fn field_str(fields: &[(String, Value)], name: &str) -> Result<String, String> {
    match field(fields, name)? {
        Value::Str(value) => Ok(value.clone()),
        _ => Err(format!("field `{name}` must be a string")),
    }
}

fn field_optional_str(fields: &[(String, Value)], name: &str) -> Result<Option<String>, String> {
    match field(fields, name) {
        Ok(Value::Str(value)) => Ok(Some(value.clone())),
        Ok(Value::Unit) => Ok(None),
        Ok(_) => Err(format!("field `{name}` must be a string or unit")),
        Err(_) => Ok(None),
    }
}
