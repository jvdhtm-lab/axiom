//! Valid `.ax` programs must parse without errors and have the expected shape.

use std::fs;
use std::path::{Path, PathBuf};

use axiom_syntax::ast::{
    BinaryOp, Expr, ExprKind, ItemKind, Literal, MemberLabel, MemberValue, StmtKind, TypeExprKind,
    UnaryOp,
};
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

const EXAMPLE_SOURCES: &[(&str, &str)] = &[
    ("types.ax", include_str!("../../../examples/src/types.ax")),
    (
        "contracts.ax",
        include_str!("../../../examples/src/contracts.ax"),
    ),
    (
        "workspaces.ax",
        include_str!("../../../examples/src/workspaces.ax"),
    ),
    ("agents.ax", include_str!("../../../examples/src/agents.ax")),
    ("team.ax", include_str!("../../../examples/src/team.ax")),
    (
        "filesystem_feature_workflow.ax",
        include_str!("../../../examples/src/filesystem_feature_workflow.ax"),
    ),
    (
        "support_db.ax",
        include_str!("../../../examples/src/support_db.ax"),
    ),
    (
        "local_and_cloud.ax",
        include_str!("../../../examples/src/local_and_cloud.ax"),
    ),
    (
        "frontend/frontend_contracts.ax",
        include_str!("../../../examples/src/frontend/frontend_contracts.ax"),
    ),
    (
        "frontend/performance_dashboard.ax",
        include_str!("../../../examples/src/frontend/performance_dashboard.ax"),
    ),
    (
        "frontend/server_dashboard.ax",
        include_str!("../../../examples/src/frontend/server_dashboard.ax"),
    ),
];

#[test]
fn examples_parse_cleanly() {
    for (name, source) in EXAMPLE_SOURCES {
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
fn module_header_and_item_order() {
    let result = parse_ok(
        "header.ax",
        "module hello;\nexport interface I { x: string; }\nexport type T = string;\nexport agent A implements Agent<I, T> { purpose: \"p\"; }\n",
    );
    let module = result.file.module.as_ref().expect("a module header");
    let path: Vec<_> = module.path.iter().map(|i| i.text.as_str()).collect();
    assert_eq!(path, ["hello"]);

    let kinds: Vec<_> = result.file.items.iter().map(|i| i.kind_name()).collect();
    assert_eq!(kinds, ["interface", "type", "agent"]);
    assert!(result.file.items.iter().all(|i| i.exported));
}

#[test]
fn agent_declaration_is_structured() {
    let result = parse_ok(
        "local_and_cloud.ax",
        include_str!("../../../examples/src/local_and_cloud.ax"),
    );
    let agent = result
        .file
        .items
        .iter()
        .find_map(|i| match &i.kind {
            ItemKind::Agent(a) if a.name.text == "Architect" => Some(a),
            _ => None,
        })
        .expect("an agent declaration");

    assert_eq!(agent.name.text, "Architect");
    assert_eq!(agent.implements.as_ref_path().as_deref(), Some("Agent"));
    match &agent.implements.kind {
        TypeExprKind::Ref { args, .. } => {
            assert_eq!(args.len(), 2);
            assert_eq!(args[0].as_ref_path().as_deref(), Some("FeatureSpec"));
            assert_eq!(args[1].as_ref_path().as_deref(), Some("ExecutionPlan"));
        }
        other => panic!("expected a type reference, got {other:?}"),
    }

    let field_names: Vec<_> = agent
        .members
        .iter()
        .filter(|m| matches!(m.value, MemberValue::Expr(_)))
        .map(|m| m.name.text.as_str())
        .collect();
    assert_eq!(field_names, ["purpose", "model", "tools"]);

    let constraints = agent
        .members
        .iter()
        .find(|m| m.name.text == "constraints")
        .expect("a constraints block");
    match &constraints.value {
        MemberValue::Block(entries) => {
            let names: Vec<_> = entries.iter().map(|e| e.name.text.as_str()).collect();
            assert_eq!(names, ["must", "must_not"]);
        }
        other => panic!("expected a member block, got {other:?}"),
    }
}

#[test]
fn imports_and_enums_are_captured() {
    let result = parse_ok(
        "local_and_cloud.ax",
        include_str!("../../../examples/src/local_and_cloud.ax"),
    );
    assert_eq!(result.file.imports.len(), 1);
    let import = &result.file.imports[0];
    let names: Vec<_> = import.names.iter().map(|i| i.text.as_str()).collect();
    assert_eq!(
        names,
        [
            "FeatureSpec",
            "ExecutionPlan",
            "PatchArtifact",
            "ReviewResult"
        ]
    );
    assert_eq!(import.from.value, "./types.ax");

    let enumeration = result
        .file
        .items
        .iter()
        .find_map(|i| match &i.kind {
            ItemKind::Enum(e) => Some(e),
            _ => None,
        })
        .expect("an enum");
    assert_eq!(enumeration.name.text, "Profile");
    assert_eq!(enumeration.members.len(), 3);
    assert_eq!(enumeration.members[0].name.text, "Local");
}

#[test]
fn reference_workflow_parses() {
    use axiom_syntax::ast::WorkflowDecl;

    let result = parse_ok(
        "filesystem_feature_workflow.ax",
        include_str!("../../../examples/src/filesystem_feature_workflow.ax"),
    );
    let kinds: Vec<_> = result.file.items.iter().map(|i| i.kind_name()).collect();
    assert_eq!(kinds, ["workflow"]);

    let wf = result
        .file
        .items
        .iter()
        .find_map(|i| match &i.kind {
            ItemKind::Workflow(w) => Some(w),
            _ => None,
        })
        .expect("a workflow declaration") as &WorkflowDecl;
    assert_eq!(wf.name.text, "ImplementFeature");
    assert_eq!(wf.params.len(), 1);
    assert_eq!(wf.params[0].name.text, "spec");

    let has = |pred: fn(&StmtKind) -> bool| wf.body.iter().any(|s| pred(&s.kind));
    assert!(has(|k| matches!(k, StmtKind::Const(_))));
    assert!(has(|k| matches!(k, StmtKind::Require { .. })));
    assert!(has(|k| matches!(k, StmtKind::Return(_))));

    // A `let submitted = parallel for ... { yield ...; }` binding.
    let parallel_value = wf.body.iter().any(|s| match &s.kind {
        StmtKind::Const(c) => matches!(c.value.kind, ExprKind::Parallel { .. }),
        _ => false,
    });
    assert!(parallel_value, "expected a parallel-for expression");
}

#[test]
fn tool_team_goal_and_contract_member_blocks() {
    let result = parse_ok("team.ax", include_str!("../../../examples/src/team.ax"));

    let tool = result
        .file
        .items
        .iter()
        .find_map(|i| match &i.kind {
            ItemKind::Tool(t) => Some(t),
            _ => None,
        })
        .expect("a tool declaration");
    assert_eq!(tool.name.text, "lint_summary");
    assert_eq!(tool.effects.len(), 1);
    assert_eq!(tool.effects[0].text, "fs.read");
    assert!(tool.members.iter().any(|m| m.name.text == "binding"));

    let team = result
        .file
        .items
        .iter()
        .find_map(|i| match &i.kind {
            ItemKind::MemberBlock(d) if d.kind == "team" => Some(d),
            _ => None,
        })
        .expect("a team declaration");
    let routing = team
        .members
        .iter()
        .find(|m| m.name.text == "routing")
        .expect("a routing block");
    match &routing.value {
        MemberValue::Block(entries) => {
            assert_eq!(entries.len(), 3);
            assert!(matches!(entries[0].value, MemberValue::Arrow(_)));
        }
        other => panic!("expected a routing block, got {other:?}"),
    }

    let goal = result
        .file
        .items
        .iter()
        .find_map(|i| match &i.kind {
            ItemKind::Goal(g) => Some(g),
            _ => None,
        })
        .expect("a goal declaration");
    assert_eq!(goal.name.text, "ShipFeature");
    assert_eq!(goal.params.len(), 1);

    let contracts = parse_ok(
        "contracts.ax",
        include_str!("../../../examples/src/contracts.ax"),
    );
    let contract = contracts
        .file
        .items
        .iter()
        .find_map(|i| match &i.kind {
            ItemKind::MemberBlock(d) if d.kind == "contract" => Some(d),
            _ => None,
        })
        .expect("a contract declaration");
    let postcondition = contract
        .members
        .iter()
        .find(|m| m.name.text == "postcondition")
        .expect("a postcondition");
    assert!(matches!(
        &postcondition.label,
        Some(MemberLabel::Ident(i)) if i.text == "non_empty"
    ));
}

#[test]
fn workspace_member_blocks_and_limits() {
    let result = parse_ok(
        "workspaces.ax",
        include_str!("../../../examples/src/workspaces.ax"),
    );
    let workspace = result
        .file
        .items
        .iter()
        .find_map(|i| match &i.kind {
            ItemKind::MemberBlock(d)
                if d.kind == "workspace" && d.name.text == "ProjectReadOnly" =>
            {
                Some(d)
            }
            _ => None,
        })
        .expect("the ProjectReadOnly workspace");
    assert!(workspace
        .members
        .iter()
        .any(|m| m.name.text == "isolation" && matches!(m.value, MemberValue::Expr(_))));
    let limits = workspace
        .members
        .iter()
        .find(|m| m.name.text == "limits")
        .expect("a limits block");
    match &limits.value {
        MemberValue::Block(entries) => {
            assert!(entries.iter().any(|e| e.name.text == "max_file_bytes"));
        }
        other => panic!("expected a limits block, got {other:?}"),
    }
}

#[test]
fn values_and_precedence() {
    let result = parse_ok("values.ax", include_str!("fixtures/valid/values.ax"));
    assert_eq!(result.file.items.len(), 9);

    let value_of = |name: &str| -> Expr {
        result
            .file
            .items
            .iter()
            .find_map(|i| match &i.kind {
                ItemKind::Const(c) if c.name.text == name => Some(c.value.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("const `{name}`"))
    };

    assert!(matches!(
        value_of("n").kind,
        ExprKind::Literal(Literal::Int(42))
    ));
    assert!(matches!(
        value_of("f").kind,
        ExprKind::Literal(Literal::Float(_))
    ));
    assert!(matches!(
        value_of("b").kind,
        ExprKind::Literal(Literal::Bool(true))
    ));
    assert!(matches!(
        value_of("z").kind,
        ExprKind::Literal(Literal::Null)
    ));
    match value_of("d").kind {
        ExprKind::Literal(Literal::Duration { millis, .. }) => assert_eq!(millis, 120_000),
        other => panic!("expected a duration, got {other:?}"),
    }
    assert!(matches!(value_of("list").kind, ExprKind::List(_)));
    assert!(matches!(value_of("record").kind, ExprKind::Object(_)));

    // `1 + 2 * 3 < 10 && !false` parses as `((1 + (2 * 3)) < 10) && (!false)`.
    match value_of("calc").kind {
        ExprKind::Binary {
            op: BinaryOp::And,
            right,
            ..
        } => assert!(matches!(
            right.kind,
            ExprKind::Unary {
                op: UnaryOp::Not,
                ..
            }
        )),
        other => panic!("expected a top-level `&&`, got {other:?}"),
    }
}

#[test]
fn comments_and_blank_lines_are_ignored_between_items() {
    let source = "\n// leading comment\n\n/* block\n   comment */\nmodule m;\nagent A implements Agent<In, Out> {\n  purpose: \"x\";\n}\n";
    let result = parse_ok("comments.ax", source);
    assert_eq!(result.file.items.len(), 1);
}
