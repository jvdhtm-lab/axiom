//! Deterministic server evaluator tests.

use std::path::Path;

use axiom_ir::{lower, ModuleInput, Program, Target};
use axiom_runtime::{run, RuntimeError, Value};
use axiom_syntax::parse;

fn load(src: &str) -> Program {
    let file = parse(src).file;
    let inputs = [ModuleInput {
        name: "t".to_string(),
        display: "t.ax".to_string(),
        file: &file,
    }];
    lower(&inputs, Target::Server)
}

fn program() -> Program {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/src/deterministic.ax");
    let src = std::fs::read_to_string(path).expect("read deterministic example");
    load(&src)
}

fn eval(entry: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
    run(&program(), entry, args)
}

#[test]
fn runs_main_entry_point() {
    let value = eval("main", vec![]).expect("main should run");
    assert_eq!(value, Value::Str("positive: 5 a".to_string()));
}

#[test]
fn calls_pure_functions() {
    assert_eq!(
        eval("add", vec![Value::Int(2), Value::Int(3)]).unwrap(),
        Value::Int(5)
    );
    assert_eq!(
        eval("describe", vec![Value::Int(0)]).unwrap(),
        Value::Str("zero".to_string())
    );
    assert_eq!(
        eval("describe", vec![Value::Int(-4)]).unwrap(),
        Value::Str("negative".to_string())
    );
}

#[test]
fn matches_option_variants() {
    assert_eq!(
        eval("maybe", vec![Value::Bool(true)]).unwrap(),
        Value::Variant("Some".to_string(), vec![Value::Int(42)])
    );
    assert_eq!(
        eval("maybe", vec![Value::Bool(false)]).unwrap(),
        Value::Variant("None".to_string(), vec![])
    );
}

#[test]
fn iterates_finite_for() {
    assert_eq!(
        eval(
            "first",
            vec![Value::List(vec![Value::Str("x".to_string())])]
        )
        .unwrap(),
        Value::Str("x".to_string())
    );
    assert_eq!(
        eval(
            "total",
            vec![Value::List(vec![Value::Int(10), Value::Int(20)])]
        )
        .unwrap(),
        Value::Int(11)
    );
}

#[test]
fn effectful_calls_fail_closed() {
    // A db call lowers fine; running it must fail because no host is attached.
    let src = "module a;\nexport type Row = { id: int; };\n\
               export async fn f() -> int effects [db.read] {\n\
                 await db.query_one<Row>(database: \"d\", sql: \"SELECT 1\");\n\
                 return 1;\n\
               }\n";
    let program = load(src);
    let err = run(&program, "f", vec![]).unwrap_err();
    assert!(err.message.contains("runtime host"), "{}", err.message);
}

#[test]
fn unknown_entry_is_an_error_not_a_panic() {
    let err = eval("does_not_exist", vec![]).unwrap_err();
    assert!(err.message.contains("no entry function"), "{}", err.message);
}

#[test]
fn arithmetic_errors_do_not_panic() {
    let src = "module a;\nexport fn div(x: int) -> int { return x / 0; }\n";
    let program = load(src);
    let err = run(&program, "div", vec![Value::Int(1)]).unwrap_err();
    assert!(err.message.contains("division by zero"), "{}", err.message);
}
