//! The `axiom` command-line interface.
//!
//! Phase 1 exposes `axiom check` for `.ax` source and `axiom config check` for
//! `axiom.yaml`. Neither command contacts a model integration.

use std::path::PathBuf;
use std::process::ExitCode;

use axiom_config::parse_config;
use axiom_syntax::{parse, Source, VERSION};

const USAGE: &str = "\
axiom - the Axiom language toolchain

USAGE:
    axiom <COMMAND> [OPTIONS]

COMMANDS:
    check <FILE>...            Parse and report diagnostics for .ax source files
    config check <FILE>...     Validate an axiom.yaml project configuration
    version                    Print the version
    help                       Print this message

OPTIONS:
    -h, --help         Print help
    -V, --version      Print version
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => {
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
        Some("--version") | Some("-V") | Some("version") => {
            println!("axiom {VERSION}");
            ExitCode::SUCCESS
        }
        Some("--help") | Some("-h") | Some("help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some("check") => run_check(&args[1..]),
        Some("config") => run_config(&args[1..]),
        Some(other) => {
            eprintln!("error: unknown command `{other}`\n");
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Collect positional file arguments, ignoring flags.
fn file_arguments(arguments: &[String]) -> Vec<PathBuf> {
    arguments
        .iter()
        .filter(|arg| !arg.starts_with('-'))
        .map(PathBuf::from)
        .collect()
}

fn run_check(arguments: &[String]) -> ExitCode {
    let files = file_arguments(arguments);
    if files.is_empty() {
        eprintln!("error: `check` needs at least one .ax file\n");
        eprint!("{USAGE}");
        return ExitCode::from(2);
    }

    let mut error_count = 0usize;
    let mut warning_count = 0usize;
    let mut checked = 0usize;
    let mut failed = false;

    for path in &files {
        let display = path.display().to_string();
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) => {
                eprintln!("error: cannot read `{display}`: {err}");
                error_count += 1;
                failed = true;
                continue;
            }
        };
        checked += 1;

        let result = parse(&text);
        let source = Source::new(display.clone(), text);

        let mut file_errors = 0usize;
        for diagnostic in &result.diagnostics {
            if diagnostic.is_error() {
                error_count += 1;
                file_errors += 1;
            } else {
                warning_count += 1;
            }
            eprint!("{}", diagnostic.render(&source));
        }

        if file_errors == 0 {
            println!("ok: {display} ({} declaration(s))", result.file.decls.len());
        } else {
            failed = true;
        }
    }

    print_summary(checked, error_count, warning_count);
    exit_code(failed)
}

fn run_config(arguments: &[String]) -> ExitCode {
    match arguments.first().map(String::as_str) {
        Some("check") => run_config_check(&arguments[1..]),
        _ => {
            eprintln!("error: expected `axiom config check <FILE>`\n");
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn run_config_check(arguments: &[String]) -> ExitCode {
    let files = file_arguments(arguments);
    if files.is_empty() {
        eprintln!("error: `config check` needs at least one axiom.yaml file\n");
        eprint!("{USAGE}");
        return ExitCode::from(2);
    }

    let mut error_count = 0usize;
    let mut warning_count = 0usize;
    let mut checked = 0usize;
    let mut failed = false;

    for path in &files {
        let display = path.display().to_string();
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) => {
                eprintln!("error: cannot read `{display}`: {err}");
                error_count += 1;
                failed = true;
                continue;
            }
        };
        checked += 1;

        let report = parse_config(&display, &text);
        let source = Source::new(display.clone(), text);
        for diagnostic in &report.diagnostics {
            if diagnostic.is_error() {
                error_count += 1;
            } else {
                warning_count += 1;
            }
            eprint!("{}", diagnostic.render(&source));
        }

        if report.has_errors() {
            failed = true;
        } else if let Some(config) = &report.config {
            println!(
                "ok: {display} ({} integration(s), {} model(s))",
                config.integrations.len(),
                config.models.len()
            );
        }
    }

    print_summary(checked, error_count, warning_count);
    exit_code(failed)
}

fn print_summary(checked: usize, errors: usize, warnings: usize) {
    let noun = if checked == 1 { "file" } else { "files" };
    println!("checked {checked} {noun}: {errors} error(s), {warnings} warning(s)");
}

fn exit_code(failed: bool) -> ExitCode {
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
