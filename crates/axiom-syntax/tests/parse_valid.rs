//! Positive parser fixtures for the small Axiom 1.0 grammar.

use axiom_syntax::ast::*;
use axiom_syntax::{parse, TokenKind};

fn parse_ok(source: &str) -> File {
    let result = parse(source);
    let errors: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| format!("{:?} {}", d.code, d.message))
        .collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
    result.file
}

fn kind<'a>(file: &'a File, name: &str) -> &'a DeclKind {
    file.declarations
        .iter()
        .find(|d| match &d.kind {
            DeclKind::Type(t) => t.name == name,
            DeclKind::Enum(e) => e.name == name,
            DeclKind::Function(f) => f.name == name,
            DeclKind::Agent(a) => a.name == name,
            DeclKind::Workspace(w) => w.name == name,
        })
        .map(|d| &d.kind)
        .unwrap_or_else(|| panic!("no declaration named `{name}`"))
}

#[test]
fn minimal_module_parses() {
    let file = parse_ok("module a.b;\n");
    assert_eq!(file.module.as_ref().unwrap().dotted(), "a.b");
    assert!(file.declarations.is_empty());
}

#[test]
fn imports_parse() {
    let file = parse_ok("module a;\nimport { X, Y } from \"./b.ax\";\n");
    assert_eq!(file.imports.len(), 1);
    assert_eq!(file.imports[0].names, vec!["X", "Y"]);
    assert_eq!(file.imports[0].from, "./b.ax");
}

#[test]
fn type_and_enum_declarations_parse() {
    let file = parse_ok(
        "module a;\n\
         export type Point = { x: int; y: int; };\n\
         export type Names = string[];\n\
         export enum Color { Red; Green; Blue; }\n",
    );
    match kind(&file, "Point") {
        DeclKind::Type(t) => match &t.ty.kind {
            TypeKind::Record(fields) => assert_eq!(fields.len(), 2),
            other => panic!("expected record, got {other:?}"),
        },
        other => panic!("expected type, got {other:?}"),
    }
    match kind(&file, "Names") {
        DeclKind::Type(t) => assert!(matches!(t.ty.kind, TypeKind::List(_))),
        other => panic!("expected type, got {other:?}"),
    }
    match kind(&file, "Color") {
        DeclKind::Enum(e) => assert_eq!(e.variants.len(), 3),
        other => panic!("expected enum, got {other:?}"),
    }
}

#[test]
fn api_async_fn_with_effects_parses() {
    let file = parse_ok(
        "module a;\n\
         export api async fn get(input: In) -> Result<Out, E>\n\
           effects [db.read] {\n\
           let x = await db.query_one<Out>(database: \"app\", sql: \"SELECT 1\", params: { id: input.id }, timeout: 5s);\n\
           return Ok(x);\n\
         }\n",
    );
    match kind(&file, "get") {
        DeclKind::Function(f) => {
            assert!(f.api);
            assert!(f.is_async);
            assert_eq!(f.effects.as_ref().unwrap()[0].dotted(), "db.read");
            assert_eq!(f.body.stmts.len(), 2);
        }
        other => panic!("expected fn, got {other:?}"),
    }
}

#[test]
fn named_generic_call_and_match_parse() {
    let file = parse_ok(
        "module a;\n\
         export fn f(x: int) -> int {\n\
           match Ok(x) {\n\
             Ok(v) => return v;\n\
             Err(_) => return 0;\n\
           }\n\
         }\n",
    );
    match kind(&file, "f") {
        DeclKind::Function(f) => match &f.body.stmts[0].kind {
            StmtKind::Expr(Expr {
                kind: ExprKind::Match { arms, .. },
                ..
            }) => {
                assert_eq!(arms.len(), 2);
                assert!(matches!(
                    arms[0].pattern,
                    Pattern::Constructor { ref path, .. } if path == &vec!["Ok".to_string()]
                ));
            }
            other => panic!("expected match, got {other:?}"),
        },
        other => panic!("expected fn, got {other:?}"),
    }
}

#[test]
fn agent_and_workspace_member_blocks_parse() {
    let file = parse_ok(
        "module a;\n\
         export agent Designer(input: Change) -> Patch {\n\
           model: \"coding\";\n\
           workspace: edit;\n\
           tools: [fs.read, fs.write];\n\
           limits: { max_calls: 6; timeout: 120s; }\n\
         }\n\
         export workspace edit {\n\
           root: \".\";\n\
           read: [\"src/**\"];\n\
           write: [\"src/front/**\"];\n\
           deny: [\"src/server/**\"];\n\
           isolation: copy_on_write;\n\
         }\n",
    );
    match kind(&file, "Designer") {
        DeclKind::Agent(a) => {
            assert_eq!(a.name, "Designer");
            assert!(a.member("model").is_some());
            assert!(a.member("tools").is_some());
            assert!(a.member("limits").is_some());
        }
        other => panic!("expected agent, got {other:?}"),
    }
    match kind(&file, "edit") {
        DeclKind::Workspace(w) => {
            assert_eq!(w.members.len(), 5);
            assert!(w.member("isolation").is_some());
        }
        other => panic!("expected workspace, got {other:?}"),
    }
}

#[test]
fn control_flow_and_literals_parse() {
    let file = parse_ok(
        "module a;\n\
         export fn f(items: int[]) -> int {\n\
           let total = 0;\n\
           for item in items {\n\
             if item > 0 { total + item; } else { 0; }\n\
           }\n\
           return total;\n\
         }\n",
    );
    match kind(&file, "f") {
        DeclKind::Function(f) => {
            assert_eq!(f.body.stmts.len(), 3);
            assert!(matches!(f.body.stmts[0].kind, StmtKind::Let { .. }));
            assert!(matches!(f.body.stmts[1].kind, StmtKind::For { .. }));
            assert!(matches!(f.body.stmts[2].kind, StmtKind::Return(Some(_))));
        }
        other => panic!("expected fn, got {other:?}"),
    }
}

#[test]
fn reference_examples_parse_cleanly() {
    let files = ["shared.ax", "server.ax", "frontend.ax", "agents.ax"];
    for name in files {
        let path = format!("{}/../../examples/src/{name}", env!("CARGO_MANIFEST_DIR"));
        let source = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let result = parse(&source);
        let errors: Vec<_> = result.diagnostics.iter().filter(|d| d.is_error()).collect();
        assert!(
            errors.is_empty(),
            "{name} has errors: {errors:#?}\nsource:\n{source}"
        );
    }
}

#[test]
fn parser_never_panics_on_token_soup() {
    for soup in [
        "",
        "module",
        "module a; type",
        "}}}",
        "export export export",
        "fn (",
        "match { => }",
        "let = ;",
        "@#$%^&*",
    ] {
        let _ = parse(soup);
    }
}

#[test]
fn lexer_still_recognizes_durations_and_sizes() {
    let (tokens, _) = axiom_syntax::tokenize("5s 2MB");
    assert!(matches!(
        tokens[0].kind,
        TokenKind::Duration { millis: 5000, .. }
    ));
    assert!(matches!(
        tokens[1].kind,
        TokenKind::Size {
            bytes: 2_000_000,
            ..
        }
    ));
}
