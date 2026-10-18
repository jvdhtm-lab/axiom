//! Semantic-analysis tests: module graph, symbols, and declaration checks.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use axiom_types::check::{check_module, check_project};
use axiom_types::{load, CheckOptions, GraphDiagnostic};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> TempDir {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut path = std::env::temp_dir();
        path.push(format!("axiom-types-{}-{tag}-{n}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create temp dir");
        TempDir { path }
    }

    fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.path.join(name);
        std::fs::write(&path, contents).expect("write fixture");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// All error/warning messages from a check, for substring assertions.
fn messages(diagnostics: &[GraphDiagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|d| d.diagnostic.message.clone())
        .collect()
}

fn has_error_containing(diagnostics: &[GraphDiagnostic], needle: &str) -> bool {
    diagnostics
        .iter()
        .any(|d| d.diagnostic.is_error() && d.diagnostic.message.contains(needle))
}

fn reference_options() -> CheckOptions {
    CheckOptions {
        model_aliases: Some(
            ["reasoning", "coding", "review", "local_coding"]
                .into_iter()
                .map(String::from)
                .collect(),
        ),
        command_names: Some(
            [
                "cargo_check",
                "unit_tests",
                "integration_tests",
                "full_ci",
                "db_tests",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        ),
    }
}

fn examples_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/src")
        .canonicalize()
        .expect("examples/src exists")
}

#[test]
fn reference_project_has_no_semantic_errors() {
    let root = examples_src();
    let files = vec![
        root.join("types.ax"),
        root.join("contracts.ax"),
        root.join("workspaces.ax"),
        root.join("agents.ax"),
        root.join("filesystem_feature_workflow.ax"),
        root.join("support_db.ax"),
        root.join("local_and_cloud.ax"),
        root.join("team.ax"),
        root.join("frontend/frontend_contracts.ax"),
        root.join("frontend/performance_dashboard.ax"),
        root.join("frontend/server_dashboard.ax"),
    ];
    let project = load(&files);
    let diagnostics = check_project(&project, &reference_options());
    assert!(
        !axiom_types::has_errors(&diagnostics),
        "unexpected diagnostics: {:#?}",
        messages(&diagnostics)
    );
}

#[test]
fn duplicate_declaration_is_reported() {
    let dir = TempDir::new("dup-decl");
    let file = dir.write(
        "a.ax",
        "module a;\nexport type T = string;\nexport type T = int;\n",
    );
    let project = load(&[file]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(
        &diagnostics,
        "`T` is declared more than once"
    ));
}

#[test]
fn duplicate_module_path_is_reported() {
    let dir = TempDir::new("dup-module");
    let a = dir.write("a.ax", "module shared;\nexport type A = string;\n");
    let b = dir.write("b.ax", "module shared;\nexport type B = string;\n");
    let project = load(&[a, b]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(
        &diagnostics,
        "module path `shared` is declared by more than one file"
    ));
}

#[test]
fn importing_unknown_name_is_reported() {
    let dir = TempDir::new("unknown-import");
    dir.write("types.ax", "module types;\nexport type T = string;\n");
    let main = dir.write(
        "main.ax",
        "module main;\nimport { Missing } from \"./types.ax\";\nexport type A = Missing;\n",
    );
    let project = load(&[main]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(
        &diagnostics,
        "`Missing` is not declared in"
    ));
}

#[test]
fn importing_unexported_name_is_reported() {
    let dir = TempDir::new("unexported");
    dir.write("types.ax", "module types;\ntype Hidden = string;\n");
    let main = dir.write(
        "main.ax",
        "module main;\nimport { Hidden } from \"./types.ax\";\nexport type A = Hidden;\n",
    );
    let project = load(&[main]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(
        &diagnostics,
        "`Hidden` is not exported by"
    ));
}

#[test]
fn missing_import_target_is_reported() {
    let dir = TempDir::new("missing-target");
    let main = dir.write(
        "main.ax",
        "module main;\nimport { T } from \"./nope.ax\";\nexport type A = T;\n",
    );
    let project = load(&[main]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(&diagnostics, "cannot resolve import"));
}

#[test]
fn import_cycle_is_reported() {
    let dir = TempDir::new("cycle");
    dir.write(
        "a.ax",
        "module a;\nimport { B } from \"./b.ax\";\nexport type A = B;\n",
    );
    dir.write(
        "b.ax",
        "module b;\nimport { A } from \"./a.ax\";\nexport type B = A;\n",
    );
    let project = load(&[dir.path.join("a.ax")]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(&diagnostics, "import cycle detected"));
}

#[test]
fn duplicate_enum_members_are_reported() {
    let dir = TempDir::new("dup-enum");
    let file = dir.write("a.ax", "module a;\nexport enum E { A, A }\n");
    let project = load(&[file]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(
        &diagnostics,
        "duplicate enum member `A`"
    ));
}

#[test]
fn duplicate_interface_properties_are_reported() {
    let dir = TempDir::new("dup-prop");
    let file = dir.write(
        "a.ax",
        "module a;\nexport interface I { x: string; x: int; }\n",
    );
    let project = load(&[file]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(
        &diagnostics,
        "duplicate interface property `x`"
    ));
}

#[test]
fn builtin_generic_arity_is_checked() {
    let dir = TempDir::new("arity");
    let file = dir.write(
        "a.ax",
        "module a;\nexport type Bad = Option<string, int>;\n",
    );
    let project = load(&[file]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(
        &diagnostics,
        "type `Option` expects 1"
    ));
}

#[test]
fn pure_function_cannot_declare_effects() {
    let dir = TempDir::new("pure-effects");
    let file = dir.write(
        "a.ax",
        "module a;\npure fn f() -> int effects [fs.read] { return 1; }\n",
    );
    let project = load(&[file]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(
        &diagnostics,
        "a `pure` function cannot declare effects"
    ));
}

#[test]
fn pure_function_cannot_await() {
    let dir = TempDir::new("pure-await");
    let file = dir.write(
        "a.ax",
        "module a;\npure fn f() -> int { let x = await g(); return x; }\n",
    );
    let project = load(&[file]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(
        &diagnostics,
        "a `pure` function cannot perform effectful work"
    ));
}

#[test]
fn non_pure_function_must_declare_effects() {
    let dir = TempDir::new("impure");
    let file = dir.write("a.ax", "module a;\nfn f() -> int { return 1; }\n");
    let project = load(&[file]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(
        &diagnostics,
        "a non-`pure` function must declare its effects"
    ));
}

#[test]
fn unknown_effect_is_reported() {
    let dir = TempDir::new("effect");
    let file = dir.write(
        "a.ax",
        "module a;\nfn f() -> int effects [fs.magic] { return 1; }\n",
    );
    let project = load(&[file]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(
        &diagnostics,
        "unknown effect `fs.magic`"
    ));
}

#[test]
fn unknown_model_alias_is_reported() {
    let dir = TempDir::new("model");
    let file = dir.write(
        "a.ax",
        "module a;\nexport agent X implements Agent<I, O> { purpose: \"p\"; model: \"ghost\"; }\n",
    );
    let project = load(&[file]);
    let options = CheckOptions::with_models(["reasoning".to_string()]);
    let diagnostics = check_project(&project, &options);
    assert!(has_error_containing(
        &diagnostics,
        "model alias `ghost` is not defined"
    ));
}

#[test]
fn unknown_command_name_is_reported() {
    let dir = TempDir::new("command");
    let file = dir.write(
        "a.ax",
        "module a;\nexport workflow W() {\n  let r = await commands.run(\"ghost_cmd\");\n}\n",
    );
    let project = load(&[file]);
    let options = CheckOptions {
        model_aliases: None,
        command_names: Some(["unit_tests".to_string()].into_iter().collect()),
    };
    let diagnostics = check_project(&project, &options);
    assert!(has_error_containing(
        &diagnostics,
        "command `ghost_cmd` is not defined"
    ));
}

#[test]
fn duplicate_clause_label_is_reported() {
    let dir = TempDir::new("clause");
    let file = dir.write(
        "a.ax",
        "module a;\nexport contract C {\n  version: \"1.0.0\";\n  postcondition a: x > 0;\n  postcondition a: y > 0;\n}\n",
    );
    let project = load(&[file]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(
        &diagnostics,
        "duplicate `postcondition` clause labeled \"a\""
    ));
}

#[test]
fn distinct_clause_labels_are_accepted() {
    let dir = TempDir::new("clause-ok");
    let file = dir.write(
        "a.ax",
        "module a;\nexport contract C {\n  version: \"1.0.0\";\n  postcondition non_empty: output != null;\n  postcondition bounded: value != null;\n}\n",
    );
    let project = load(&[file]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(
        !axiom_types::has_errors(&diagnostics),
        "unexpected diagnostics: {:#?}",
        messages(&diagnostics)
    );
}

#[test]
fn module_check_is_usable_without_a_project() {
    let dir = TempDir::new("module-check");
    let file = dir.write("a.ax", "module a;\nexport type T = string;\n");
    let project = load(&[file]);
    let module = project.graph.module(0);
    let diagnostics = check_module(module, &CheckOptions::none());
    assert!(diagnostics.is_empty());
}

#[test]
fn unresolved_value_name_is_reported() {
    let dir = TempDir::new("unresolved-value");
    let file = dir.write(
        "a.ax",
        "module a;\nexport workflow W() {\n  let x = mystery.name;\n}\n",
    );
    let project = load(&[file]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(
        &diagnostics,
        "cannot resolve name `mystery`"
    ));
}

#[test]
fn unresolved_type_name_is_reported() {
    let dir = TempDir::new("unresolved-type");
    let file = dir.write("a.ax", "module a;\nexport type T = Ghost;\n");
    let project = load(&[file]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(
        &diagnostics,
        "cannot resolve type `Ghost`"
    ));
}

#[test]
fn namespaces_and_builtins_resolve() {
    let dir = TempDir::new("resolves");
    let file = dir.write(
        "a.ax",
        "module a;\nexport const root: WorkspaceRoot = project;\nexport workflow W() {\n  let x = fs.list(project);\n  let y = db.count(\"support\");\n}\n",
    );
    let project = load(&[file]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(
        !axiom_types::has_errors(&diagnostics),
        "unexpected diagnostics: {:#?}",
        messages(&diagnostics)
    );
}

#[test]
fn query_without_sql_is_reported() {
    let dir = TempDir::new("query-no-sql");
    let file = dir.write(
        "a.ax",
        "module a;\nexport query Q(id: int) -> JsonValue {\n  database: \"support\";\n  access: DbAccess.Read;\n}\n",
    );
    let project = load(&[file]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(
        &diagnostics,
        "query must declare a single `sql` string literal"
    ));
}

#[test]
fn query_placeholder_mismatch_is_reported() {
    let dir = TempDir::new("query-params");
    let file = dir.write(
        "a.ax",
        "module a;\nexport query Q(id: int) -> JsonValue {\n  database: \"support\";\n  access: DbAccess.Read;\n  sql: \"SELECT * FROM t WHERE id = :other\";\n}\n",
    );
    let project = load(&[file]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(
        &diagnostics,
        "placeholder `:other` has no parameter"
    ));
    assert!(has_error_containing(
        &diagnostics,
        "parameter `id` is not used in `sql`"
    ));

    let file2 = dir.write(
        "b.ax",
        "module a;\nexport query Q(id: int) -> JsonValue {\n  database: \"support\";\n  access: DbAccess.Read;\n  sql: \"SELECT * FROM t WHERE id = :id\";\n}\n",
    );
    let project = load(&[file2]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(
        !axiom_types::has_errors(&diagnostics),
        "unexpected diagnostics: {:#?}",
        messages(&diagnostics)
    );
}

#[test]
fn write_query_must_return_write_result() {
    let dir = TempDir::new("query-write");
    let file = dir.write(
        "a.ax",
        "module a;\nexport query Q(id: int) -> JsonValue {\n  database: \"support\";\n  access: DbAccess.Write;\n  sql: \"UPDATE t SET x = 1 WHERE id = :id\";\n}\n",
    );
    let project = load(&[file]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(
        &diagnostics,
        "a write query must return `WriteResult`"
    ));

    let file2 = dir.write(
        "b.ax",
        "module a;\nexport query Q(id: int) -> WriteResult {\n  database: \"support\";\n  access: DbAccess.Write;\n  sql: \"UPDATE t SET x = 1 WHERE id = :id\";\n}\n",
    );
    let project = load(&[file2]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(
        !axiom_types::has_errors(&diagnostics),
        "unexpected diagnostics: {:#?}",
        messages(&diagnostics)
    );
}

#[test]
fn non_execute_agent_entry_point_is_rejected() {
    let dir = TempDir::new("agent-entry");
    let file = dir.write(
        "a.ax",
        "module a;\nexport workflow W() {\n  await delegate Helper.run();\n}\n",
    );
    let project = load(&[file]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(
        &diagnostics,
        "`run` is not an agent entry point"
    ));
}

#[test]
fn operation_declaration_is_checked() {
    let dir = TempDir::new("operation");
    let missing_version = dir.write(
        "a.ax",
        "module a;\nexport operation Op(input: In) -> Out {\n  authentication: AuthenticationRequirement.Required;\n  handler: Handle;\n}\ninterface In { x: string; }\ninterface Out { y: string; }\n",
    );
    let project = load(&[missing_version]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(
        &diagnostics,
        "operation must declare `version`"
    ));

    let valid = dir.write(
        "b.ax",
        "module a;\nexport operation Op(input: In) -> Out {\n  version: \"1.0.0\";\n  authentication: AuthenticationRequirement.Required;\n  requires: capability(\"x:read\");\n  data_classes: [DataClass.Internal];\n  handler: Handle;\n}\ninterface In { x: string; }\ninterface Out { y: string; }\n",
    );
    let project = load(&[valid]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(
        !axiom_types::has_errors(&diagnostics),
        "unexpected diagnostics: {:#?}",
        messages(&diagnostics)
    );
}

#[test]
fn iterate_bound_must_be_positive() {
    let dir = TempDir::new("iterate");
    let zero = dir.write(
        "a.ax",
        "module a;\nexport workflow W() {\n  await iterate g() {\n    max_iterations: 0;\n    validate: [];\n    until: iteration.validation.accepted;\n  };\n}\n",
    );
    let project = load(&[zero]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(has_error_containing(
        &diagnostics,
        "`max_iterations` must be a positive integer literal"
    ));

    let valid = dir.write(
        "b.ax",
        "module a;\nexport fn g() -> int effects [model] {\n  return 1;\n}\nexport workflow W() {\n  await iterate g() {\n    max_iterations: 4;\n    validate: [SomeCheck];\n    until: iteration.validation.accepted;\n  };\n}\n",
    );
    let project = load(&[valid]);
    let diagnostics = check_project(&project, &CheckOptions::none());
    assert!(
        !axiom_types::has_errors(&diagnostics),
        "unexpected diagnostics: {:#?}",
        messages(&diagnostics)
    );
}
