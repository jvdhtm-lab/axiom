//! Semantic-check tests for the module graph and declaration checks.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use axiom_types::{check_project, load, CheckOptions};

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct TempProject {
    dir: PathBuf,
}

impl TempProject {
    fn new(files: &[(&str, &str)]) -> TempProject {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("axiom-semantics-{}-{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        for (name, contents) in files {
            let path = dir.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("create parent");
            }
            std::fs::write(&path, contents).expect("write file");
        }
        TempProject { dir }
    }

    fn root(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn error_messages(roots: &[PathBuf], options: &CheckOptions) -> Vec<String> {
    let project = load(roots);
    check_project(&project, options)
        .into_iter()
        .filter(|d| d.diagnostic.is_error())
        .map(|d| {
            format!(
                "{} {}",
                d.diagnostic.code.unwrap_or("-"),
                d.diagnostic.message
            )
        })
        .collect()
}

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/src")
        .join(name)
}

#[test]
fn reference_examples_have_no_semantic_errors() {
    let roots = [
        example("shared.ax"),
        example("server.ax"),
        example("frontend.ax"),
        example("agents.ax"),
    ];
    let errors = error_messages(&roots, &CheckOptions::none());
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
}

#[test]
fn duplicate_declaration_is_reported() {
    let project = TempProject::new(&[(
        "a.ax",
        "module a;\nexport type X = int;\nexport type X = int;\n",
    )]);
    let errors = error_messages(&[project.root("a.ax")], &CheckOptions::none());
    assert!(errors.iter().any(|m| m.contains("AX0210")), "{errors:#?}");
}

#[test]
fn unknown_effect_is_reported() {
    let project = TempProject::new(&[(
        "a.ax",
        "module a;\nexport fn f() -> int effects [teleport] { return 1; }\n",
    )]);
    let errors = error_messages(&[project.root("a.ax")], &CheckOptions::none());
    assert!(errors.iter().any(|m| m.contains("AX0216")), "{errors:#?}");
}

#[test]
fn agent_requires_model() {
    let project = TempProject::new(&[(
        "a.ax",
        "module a;\nexport agent A(input: int) -> int { workspace: w; }\n",
    )]);
    let errors = error_messages(&[project.root("a.ax")], &CheckOptions::none());
    assert!(errors.iter().any(|m| m.contains("AX0211")), "{errors:#?}");
}

#[test]
fn unknown_isolation_mode_is_reported() {
    let project = TempProject::new(&[(
        "a.ax",
        "module a;\nexport workspace w { root: \".\"; isolation: teleport; }\n",
    )]);
    let errors = error_messages(&[project.root("a.ax")], &CheckOptions::none());
    assert!(errors.iter().any(|m| m.contains("AX0214")), "{errors:#?}");
}

#[test]
fn unknown_model_alias_is_reported_when_configured() {
    let project = TempProject::new(&[(
        "a.ax",
        "module a;\nexport agent A(input: int) -> int { model: \"ghost\"; }\n",
    )]);
    let options = CheckOptions {
        model_aliases: Some(BTreeSet::from(["coding".to_string()])),
        command_names: None,
    };
    let errors = error_messages(&[project.root("a.ax")], &options);
    assert!(errors.iter().any(|m| m.contains("AX0217")), "{errors:#?}");
}

#[test]
fn unexported_import_is_reported() {
    let project = TempProject::new(&[
        ("b.ax", "module b;\ntype Hidden = int;\n"),
        ("a.ax", "module a;\nimport { Hidden } from \"./b.ax\";\n"),
    ]);
    let errors = error_messages(&[project.root("a.ax")], &CheckOptions::none());
    assert!(errors.iter().any(|m| m.contains("AX0203")), "{errors:#?}");
}

#[test]
fn missing_import_is_reported() {
    let project = TempProject::new(&[
        ("b.ax", "module b;\nexport type X = int;\n"),
        ("a.ax", "module a;\nimport { Missing } from \"./b.ax\";\n"),
    ]);
    let errors = error_messages(&[project.root("a.ax")], &CheckOptions::none());
    assert!(errors.iter().any(|m| m.contains("AX0204")), "{errors:#?}");
}

#[test]
fn import_cycle_is_reported() {
    let project = TempProject::new(&[
        (
            "b.ax",
            "module b;\nimport { X } from \"./a.ax\";\nexport type X = int;\n",
        ),
        (
            "a.ax",
            "module a;\nimport { Y } from \"./b.ax\";\nexport type Y = int;\n",
        ),
    ]);
    let errors = error_messages(&[project.root("a.ax")], &CheckOptions::none());
    assert!(errors.iter().any(|m| m.contains("AX0201")), "{errors:#?}");
}
