//! Valid `.ax` programs must parse without errors and have the expected shape.

use std::fs;
use std::path::{Path, PathBuf};

use axiom_syntax::ast::{AgentItem, Decl, TypeKind, Value};
use axiom_syntax::{parse, ParseResult};

fn fixtures(sub: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(sub)
}

fn parse_ok(name: &str, source: &str) -> ParseResult {
    let result = parse(source);
    let errors: Vec<_> = result.diagnostics.iter().filter(|d| d.is_error()).collect();
    assert!(
        errors.is_empty(),
        "{name} should parse cleanly but produced: {errors:#?}"
    );
    result
}

#[test]
fn examples_parse_cleanly() {
    for (name, source) in [
        (
            "hello_agent.ax",
            include_str!("../../../examples/hello_agent.ax"),
        ),
        (
            "local_and_cloud.ax",
            include_str!("../../../examples/local_and_cloud.ax"),
        ),
        (
            "feature_workflow.ax",
            include_str!("../../../examples/feature_workflow.ax"),
        ),
    ] {
        parse_ok(name, source);
    }
}

#[test]
fn every_valid_fixture_parses() {
    let dir = fixtures("valid");
    let mut count = 0;
    for entry in fs::read_dir(&dir).expect("valid fixtures directory exists") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("ax") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        parse_ok(&path.display().to_string(), &text);
        count += 1;
    }
    assert!(count >= 2, "expected at least two valid fixtures");
}

#[test]
fn hello_agent_declaration_order() {
    let result = parse_ok(
        "hello_agent.ax",
        include_str!("../../../examples/hello_agent.ax"),
    );
    let kinds: Vec<_> = result.file.decls.iter().map(Decl::kind_name).collect();
    assert_eq!(kinds, ["integration", "model", "type", "type", "agent"]);
}

#[test]
fn agent_declaration_is_structured() {
    let result = parse_ok(
        "hello_agent.ax",
        include_str!("../../../examples/hello_agent.ax"),
    );
    let agent = result
        .file
        .decls
        .iter()
        .find_map(|d| match d {
            Decl::Agent(a) => Some(a),
            _ => None,
        })
        .expect("an agent declaration");

    assert_eq!(agent.name.text, "Architect");
    let implements = agent.implements.as_ref().expect("implements clause");
    assert_eq!(implements.name(), "Agent");
    assert_eq!(implements.args.len(), 2);
    assert_eq!(implements.args[0].name(), "FeatureSpec");
    assert_eq!(implements.args[1].name(), "ExecutionPlan");

    let permissions = agent
        .items
        .iter()
        .find_map(|item| match item {
            AgentItem::Permissions(entries, _) => Some(entries),
            _ => None,
        })
        .expect("a permissions block");
    assert_eq!(permissions[0].key.text, "read");
    assert!(matches!(permissions[0].value, Value::Array(_, _)));

    let constraints = agent
        .items
        .iter()
        .find_map(|item| match item {
            AgentItem::Constraints(entries, _) => Some(entries),
            _ => None,
        })
        .expect("a constraints block");
    assert_eq!(constraints.len(), 2);
}

#[test]
fn durations_and_calls_are_typed_values() {
    let result = parse_ok(
        "hello_agent.ax",
        include_str!("../../../examples/hello_agent.ax"),
    );
    let integration = result
        .file
        .decls
        .iter()
        .find_map(|d| match d {
            Decl::Integration(i) => Some(i),
            _ => None,
        })
        .unwrap();
    let timeout = integration
        .entries
        .iter()
        .find(|e| e.key.text == "request_timeout")
        .unwrap();
    assert!(matches!(
        timeout.value,
        Value::Duration {
            millis: 120_000,
            ..
        }
    ));

    let adapter = integration
        .entries
        .iter()
        .find(|e| e.key.text == "adapter")
        .unwrap();
    assert!(matches!(adapter.value, Value::Path(_, _)));
}

#[test]
fn instance_overrides_parse_as_with_expressions() {
    let result = parse_ok(
        "local_and_cloud.ax",
        include_str!("../../../examples/local_and_cloud.ax"),
    );
    let instances: Vec<_> = result
        .file
        .decls
        .iter()
        .filter_map(|d| match d {
            Decl::Instance(i) => Some(i),
            _ => None,
        })
        .collect();
    assert_eq!(instances.len(), 2);
    assert_eq!(instances[0].name.text, "localArchitect");
    assert!(matches!(
        instances[0].value,
        axiom_syntax::ast::Expr::With { .. }
    ));
}

#[test]
fn workflow_has_mixed_items() {
    use axiom_syntax::ast::WorkflowItem;
    let result = parse_ok(
        "feature_workflow.ax",
        include_str!("../../../examples/feature_workflow.ax"),
    );
    let workflow = result
        .file
        .decls
        .iter()
        .find_map(|d| match d {
            Decl::Workflow(w) => Some(w),
            _ => None,
        })
        .expect("a workflow declaration");

    assert_eq!(workflow.params.len(), 1);
    assert_eq!(workflow.params[0].name.text, "spec");
    assert!(matches!(&workflow.items[0], WorkflowItem::Let { .. }));
    assert!(workflow
        .items
        .iter()
        .any(|i| matches!(i, WorkflowItem::Require { .. })));
    assert!(workflow
        .items
        .iter()
        .any(|i| matches!(i, WorkflowItem::Parallel { .. })));
    assert!(workflow
        .items
        .iter()
        .any(|i| matches!(i, WorkflowItem::If { .. })));
}

#[test]
fn type_declarations_capture_interfaces_and_fields() {
    let result = parse_ok(
        "feature_workflow.ax",
        include_str!("../../../examples/feature_workflow.ax"),
    );
    let types: Vec<_> = result
        .file
        .decls
        .iter()
        .filter_map(|d| match d {
            Decl::Type(t) => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(types.len(), 4);
    assert!(types.iter().all(|t| t.kind == TypeKind::Type));
    assert_eq!(types[0].name.text, "FeatureSpec");
    assert_eq!(types[0].fields.len(), 2);
    assert_eq!(types[0].fields[0].name.text, "title");
    assert_eq!(types[0].fields[0].ty.name(), "String");
}

#[test]
fn comments_and_blank_lines_are_ignored_between_declarations() {
    let source =
        "\n// leading comment\n\n/* block\n   comment */\nagent A {\n  purpose: \"x\"\n}\n";
    let result = parse_ok("comments.ax", source);
    assert_eq!(result.file.decls.len(), 1);
}
