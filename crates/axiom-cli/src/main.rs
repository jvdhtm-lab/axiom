//! The `axiom` command-line interface.
//!
//! Phase 1 added `axiom check` and `axiom config check`. Phase 3 adds
//! `axiom build`, `axiom explain`, and `axiom run` over the typed IR and the
//! deterministic server evaluator. Phase 6 adds `axiom workspace check`,
//! `axiom sandbox plan`, and `axiom artifacts hash`. No command contacts a model
//! integration.

use std::path::PathBuf;
use std::process::ExitCode;

use axiom_artifacts::{ArtifactKind, ArtifactStore};
use axiom_cli::{database, tooling, workspace};
use axiom_config::parse_config;
use axiom_database::Database;
use axiom_ir::{lower, program_to_json, ModuleInput, Program, Target};
use axiom_runtime::run;
use axiom_syntax::ast::DeclKind;
use axiom_syntax::{Diagnostic, Source, VERSION};
use axiom_tools::{CommandRequest, ToolRegistry};
use axiom_types::{check_options_from_config, check_project, CheckOptions, Project};

const USAGE: &str = "\
axiom - the Axiom language toolchain

USAGE:
    axiom <COMMAND> [OPTIONS]

COMMANDS:
    init [DIR]                   Create a minimal project skeleton
    check <FILE>...              Load the module graph and report diagnostics
    lint <FILE>... [--config <FILE>]
                                 Report the same stable diagnostics as `check`
    fmt <FILE>... [--check]      Format source deterministically (or check only)
    build <FILE>... [--target server|wasm_browser] [--emit-ir]
                                 Lower to typed IR for a target
    explain <FILE>... [--target server|wasm_browser]
                                 Print the typed IR as JSON
    run <FILE>... [--fn NAME]    Evaluate an entry function deterministically
    config check <FILE>...       Validate an axiom.yaml project configuration
    workspace check <FILE>...    Validate workspace declarations against the runtime policy
    sandbox plan --config <FILE> --command <NAME> [--arg V]... [--dir DIR]
                                 Print the shell-free command plan for a profile
    artifacts hash <FILE> [--kind wasm_module|patch|ui_bundle|snapshot|other]
                                 Print a file's immutable content address
    db check --config <FILE>     Validate configured databases and role isolation
    db migrations plan --config <FILE> --database <NAME>
                                 Print pending reviewed migrations
    version                      Print the version
    help                         Print this message

OPTIONS:
    -h, --help         Print help
    -V, --version      Print version
    --config <FILE>    Cross-validate source against an axiom.yaml (for `check`)
    --target <T>       Build target: server (default) or wasm_browser
    --emit-ir          Print the typed IR JSON / artifact manifest (for `build`)
    --fn <NAME>        Entry function for `run` (default: main)
    --out <DIR>        Write `app.wasm` + `app.manifest.json` (for `build --target wasm_browser`)
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
        Some("init") => run_init(&args[1..]),
        Some("check") => run_check_as(&args[1..], "check"),
        Some("lint") => run_check_as(&args[1..], "lint"),
        Some("fmt") => run_fmt(&args[1..]),
        Some("build") => run_build(&args[1..]),
        Some("explain") => run_explain(&args[1..]),
        Some("run") => run_run(&args[1..]),
        Some("config") => run_config(&args[1..]),
        Some("workspace") => run_workspace(&args[1..]),
        Some("sandbox") => run_sandbox(&args[1..]),
        Some("artifacts") => run_artifacts(&args[1..]),
        Some("db") => run_db(&args[1..]),
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

/// Split `check` arguments into an optional `--config` path and positional files.
fn split_check_args(arguments: &[String]) -> Result<(Option<PathBuf>, Vec<PathBuf>), String> {
    let mut config = None;
    let mut files = Vec::new();
    let mut iter = arguments.iter();
    while let Some(arg) = iter.next() {
        if arg == "--config" {
            match iter.next() {
                Some(path) => config = Some(PathBuf::from(path)),
                None => return Err("`--config` needs a path".to_string()),
            }
        } else if let Some(path) = arg.strip_prefix("--config=") {
            config = Some(PathBuf::from(path));
        } else if !arg.starts_with('-') {
            files.push(PathBuf::from(arg));
        }
    }
    Ok((config, files))
}

fn run_init(arguments: &[String]) -> ExitCode {
    let root = arguments
        .iter()
        .find(|arg| !arg.starts_with('-'))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    match tooling::scaffold(&root) {
        Ok(paths) => {
            for path in paths {
                println!("created: {}", path.display());
            }
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(1)
        }
    }
}

fn run_fmt(arguments: &[String]) -> ExitCode {
    let check = arguments.iter().any(|arg| arg == "--check");
    let files = positional_files(arguments);
    if files.is_empty() {
        eprintln!("error: `fmt` needs at least one .ax file\n");
        eprint!("{USAGE}");
        return ExitCode::from(2);
    }
    let mut failed = false;
    let mut changed = 0usize;
    for path in &files {
        let display = path.display().to_string();
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                eprintln!("error: cannot read `{display}`: {error}");
                failed = true;
                continue;
            }
        };
        match tooling::format_source(&text) {
            Err(diagnostics) => {
                let source = Source::new(display.clone(), text);
                for diagnostic in diagnostics {
                    eprint!("{}", diagnostic.render(&source));
                }
                failed = true;
            }
            Ok(report) if !report.changed => {}
            Ok(report) => {
                changed += 1;
                if check {
                    println!("would reformat: {display}");
                    failed = true;
                } else if let Err(error) = std::fs::write(path, &report.formatted) {
                    eprintln!("error: cannot write `{display}`: {error}");
                    failed = true;
                } else {
                    println!("formatted: {display}");
                }
            }
        }
    }
    if check && !failed {
        let noun = if files.len() == 1 { "file" } else { "files" };
        println!("{} {noun} already formatted", files.len());
    } else if !check && !failed {
        println!("formatted {changed} of {} file(s)", files.len());
    }
    exit_code(failed)
}

fn run_check_as(arguments: &[String], command: &str) -> ExitCode {
    let (config_path, files) = match split_check_args(arguments) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("error: {message}\n");
            eprint!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    if files.is_empty() {
        eprintln!("error: `{command}` needs at least one .ax file\n");
        eprint!("{USAGE}");
        return ExitCode::from(2);
    }

    let mut error_count = 0usize;
    let mut warning_count = 0usize;
    let mut failed = false;

    // Optional source/config cross-validation.
    let mut options = CheckOptions::none();
    if let Some(path) = &config_path {
        let display = path.display().to_string();
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) => {
                eprintln!("error: cannot read `{display}`: {err}");
                return ExitCode::from(1);
            }
        };
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
        }
        if let Some(config) = &report.config {
            options = check_options_from_config(config);
        }
    }

    let project = axiom_types::load(&files);
    let diagnostics = check_project(&project, &options);

    let sources: Vec<Source> = project
        .graph
        .modules
        .iter()
        .map(|module| Source::new(module.display.clone(), module.source.clone()))
        .collect();
    let graph_source = Source::new("(project)", String::new());

    let mut modules_with_errors: std::collections::HashSet<usize> =
        std::collections::HashSet::new();
    for diagnostic in &diagnostics {
        if diagnostic.diagnostic.is_error() {
            if let Some(module) = diagnostic.module {
                modules_with_errors.insert(module);
            }
        }
    }

    for diagnostic in &diagnostics {
        if diagnostic.diagnostic.is_error() {
            error_count += 1;
            failed = true;
        } else {
            warning_count += 1;
        }
        match diagnostic.module {
            Some(id) => eprint!("{}", diagnostic.diagnostic.render(&sources[id])),
            None => eprint!("{}", diagnostic.diagnostic.render(&graph_source)),
        }
    }

    for &root in &project.graph.roots {
        if !modules_with_errors.contains(&root) {
            let module = &project.graph.modules[root];
            println!(
                "ok: {} ({} declaration(s))",
                module.display,
                module.file.declarations.len()
            );
        }
    }

    print_summary(files.len(), error_count, warning_count);
    exit_code(failed)
}

/// The value of `--name value` or `--name=value`.
fn flag_value<'a>(arguments: &'a [String], name: &str) -> Option<&'a str> {
    let prefix = format!("{name}=");
    let mut iter = arguments.iter();
    while let Some(arg) = iter.next() {
        if arg == name {
            return iter.next().map(String::as_str);
        }
        if let Some(value) = arg.strip_prefix(&prefix) {
            return Some(value);
        }
    }
    None
}

/// All values of a repeatable `--name value` / `--name=value` flag.
fn repeat_flag_values(arguments: &[String], name: &str) -> Vec<String> {
    let prefix = format!("{name}=");
    let mut values = Vec::new();
    let mut iter = arguments.iter();
    while let Some(arg) = iter.next() {
        if arg == name {
            if let Some(value) = iter.next() {
                values.push(value.clone());
            }
        } else if let Some(value) = arg.strip_prefix(&prefix) {
            values.push(value.to_string());
        }
    }
    values
}

fn target_from(arguments: &[String]) -> Result<Target, String> {
    match flag_value(arguments, "--target") {
        None | Some("server") => Ok(Target::Server),
        Some("wasm_browser") => Ok(Target::WasmBrowser),
        Some(other) => Err(format!(
            "unknown target `{other}` (expected `server` or `wasm_browser`)"
        )),
    }
}

/// Positional file arguments, skipping flags and the values of value-flags.
fn positional_files(arguments: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut iter = arguments.iter();
    while let Some(arg) = iter.next() {
        if matches!(arg.as_str(), "--target" | "--fn" | "--config" | "--out") {
            iter.next();
            continue;
        }
        if arg.starts_with('-') {
            continue;
        }
        out.push(PathBuf::from(arg));
    }
    out
}

/// Load and check files, printing diagnostics. Returns the project and whether
/// any error diagnostic was reported.
fn load_and_check(files: &[PathBuf]) -> (Project, bool) {
    let project = axiom_types::load(files);
    let diagnostics = check_project(&project, &CheckOptions::none());
    let sources: Vec<Source> = project
        .graph
        .modules
        .iter()
        .map(|module| Source::new(module.display.clone(), module.source.clone()))
        .collect();
    let graph_source = Source::new("(project)", String::new());
    let mut failed = false;
    for diagnostic in &diagnostics {
        if diagnostic.diagnostic.is_error() {
            failed = true;
        }
        match diagnostic.module {
            Some(id) => eprint!("{}", diagnostic.diagnostic.render(&sources[id])),
            None => eprint!("{}", diagnostic.diagnostic.render(&graph_source)),
        }
    }
    (project, failed)
}

fn lower_project(project: &Project, target: Target) -> Program {
    let inputs: Vec<ModuleInput> = project
        .graph
        .modules
        .iter()
        .map(|module| ModuleInput {
            name: module
                .module_path
                .clone()
                .unwrap_or_else(|| module.display.clone()),
            display: module.display.clone(),
            file: &module.file,
        })
        .collect();
    lower(&inputs, target)
}

fn run_build(arguments: &[String]) -> ExitCode {
    let target = match target_from(arguments) {
        Ok(target) => target,
        Err(message) => {
            eprintln!("error: {message}\n");
            eprint!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let emit_ir = arguments.iter().any(|arg| arg == "--emit-ir");
    let files = positional_files(arguments);
    if files.is_empty() {
        eprintln!("error: `build` needs at least one .ax file\n");
        eprint!("{USAGE}");
        return ExitCode::from(2);
    }

    let (project, failed) = load_and_check(&files);
    if failed {
        return ExitCode::from(1);
    }

    let program = lower_project(&project, target);

    if target == Target::WasmBrowser {
        return emit_browser_artifact(&program, arguments, emit_ir);
    }

    if emit_ir {
        print!("{}", program_to_json(&program));
    } else {
        println!(
            "built {} module(s) for target {} (IR v{})",
            program.modules.len(),
            target.as_str(),
            axiom_ir::IR_VERSION
        );
    }
    ExitCode::SUCCESS
}

fn emit_browser_artifact(program: &Program, arguments: &[String], emit_ir: bool) -> ExitCode {
    let artifact = match axiom_wasm::build(program) {
        Ok(artifact) => artifact,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(1);
        }
    };

    if let Some(dir) = flag_value(arguments, "--out") {
        let dir = PathBuf::from(dir);
        if let Err(err) = std::fs::create_dir_all(&dir) {
            eprintln!("error: cannot create `{}`: {err}", dir.display());
            return ExitCode::from(1);
        }
        let wasm_path = dir.join("app.wasm");
        let manifest_path = dir.join("app.manifest.json");
        if let Err(err) = std::fs::write(&wasm_path, &artifact.module) {
            eprintln!("error: cannot write `{}`: {err}", wasm_path.display());
            return ExitCode::from(1);
        }
        if let Err(err) = std::fs::write(&manifest_path, artifact.manifest_json()) {
            eprintln!("error: cannot write `{}`: {err}", manifest_path.display());
            return ExitCode::from(1);
        }
        println!(
            "wrote {} and {} (sha256 {}, {} bytes)",
            wasm_path.display(),
            manifest_path.display(),
            &artifact.content_hash[..16],
            artifact.byte_len
        );
    } else if emit_ir {
        print!("{}", artifact.manifest_json());
    } else {
        println!(
            "built wasm_browser artifact (abi v{}, ir v{}, sha256 {}…, {} bytes, {} entry point(s))",
            artifact.abi_version,
            artifact.ir_version,
            &artifact.content_hash[..16],
            artifact.byte_len,
            artifact.entry_points.len()
        );
    }
    ExitCode::SUCCESS
}

fn run_explain(arguments: &[String]) -> ExitCode {
    let mut arguments = arguments.to_vec();
    if !arguments.iter().any(|arg| arg == "--emit-ir") {
        arguments.push("--emit-ir".to_string());
    }
    run_build(&arguments)
}

fn run_run(arguments: &[String]) -> ExitCode {
    let entry = flag_value(arguments, "--fn").unwrap_or("main").to_string();
    let files = positional_files(arguments);
    if files.is_empty() {
        eprintln!("error: `run` needs at least one .ax file\n");
        eprint!("{USAGE}");
        return ExitCode::from(2);
    }

    let (project, failed) = load_and_check(&files);
    if failed {
        return ExitCode::from(1);
    }

    let program = lower_project(&project, Target::Server);
    match run(&program, &entry, Vec::new()) {
        Ok(value) => {
            println!("{}", value.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!(
                "runtime error: {} (bytes {}..{})",
                err.message, err.span.start, err.span.end
            );
            ExitCode::from(1)
        }
    }
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

fn run_workspace(arguments: &[String]) -> ExitCode {
    match arguments.first().map(String::as_str) {
        Some("check") => run_workspace_check(&arguments[1..]),
        _ => {
            eprintln!("error: expected `axiom workspace check <FILE>...`\n");
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Validate every `workspace` declaration by building the runtime policy the
/// workspace would get. Compiler checks are not the security boundary, so this
/// surfaces exactly the paths/isolation/limits the runtime would enforce.
fn run_workspace_check(arguments: &[String]) -> ExitCode {
    let files = file_arguments(arguments);
    if files.is_empty() {
        eprintln!("error: `workspace check` needs at least one .ax file\n");
        eprint!("{USAGE}");
        return ExitCode::from(2);
    }

    let project = axiom_types::load(&files);
    let mut checked = 0usize;
    let mut errors = 0usize;

    for module in &project.graph.modules {
        let source = Source::new(module.display.clone(), module.source.clone());
        for declaration in &module.file.declarations {
            let DeclKind::Workspace(workspace) = &declaration.kind else {
                continue;
            };
            checked += 1;
            match workspace::policy(workspace) {
                Ok(policy) => {
                    println!(
                        "ok: {}::{} (isolation {}, read {}, write {}, deny {})",
                        module.display,
                        workspace.name,
                        policy.isolation.as_str(),
                        policy.read.len(),
                        policy.write.len(),
                        policy.deny.len()
                    );
                    let limits = policy.limits;
                    if limits.max_files.is_some()
                        || limits.max_file_bytes.is_some()
                        || limits.max_changed_files.is_some()
                    {
                        println!(
                            "    limits: max_files={:?} max_file_bytes={:?} max_changed_files={:?}",
                            limits.max_files, limits.max_file_bytes, limits.max_changed_files
                        );
                    }
                }
                Err(issues) => {
                    for (span, message) in issues {
                        errors += 1;
                        eprint!("{}", Diagnostic::error(span, message).render(&source));
                    }
                }
            }
        }
    }

    if checked == 0 {
        println!("no workspace declarations found");
    }
    println!("checked {checked} workspace(s): {errors} error(s)");
    exit_code(errors > 0)
}

fn run_sandbox(arguments: &[String]) -> ExitCode {
    match arguments.first().map(String::as_str) {
        Some("plan") => run_sandbox_plan(&arguments[1..]),
        _ => {
            eprintln!("error: expected `axiom sandbox plan --config <FILE> --command <NAME>`\n");
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn run_sandbox_plan(arguments: &[String]) -> ExitCode {
    let Some(config_path) = flag_value(arguments, "--config") else {
        eprintln!("error: `sandbox plan` needs `--config <FILE>`\n");
        eprint!("{USAGE}");
        return ExitCode::from(2);
    };
    let Some(command) = flag_value(arguments, "--command") else {
        eprintln!("error: `sandbox plan` needs `--command <NAME>`\n");
        eprint!("{USAGE}");
        return ExitCode::from(2);
    };

    let text = match std::fs::read_to_string(config_path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("error: cannot read `{config_path}`: {err}");
            return ExitCode::from(1);
        }
    };
    let report = parse_config(config_path, &text);
    let source = Source::new(config_path.to_string(), text);
    for diagnostic in &report.diagnostics {
        eprint!("{}", diagnostic.render(&source));
    }
    if report.has_errors() {
        return ExitCode::from(1);
    }
    let Some(config) = report.config else {
        return ExitCode::from(1);
    };

    let registry = ToolRegistry::from_config(&config);
    let mut request = CommandRequest::new(command);
    request.args = repeat_flag_values(arguments, "--arg");
    request.working_directory = flag_value(arguments, "--dir").map(str::to_string);

    match registry.plan(&request) {
        Ok(plan) => {
            println!("profile: {}", plan.profile);
            println!("executable: {}", plan.executable);
            println!("argv: {:?}", plan.argv);
            println!(
                "working_directory: {}",
                plan.working_directory.as_deref().unwrap_or(".")
            );
            println!(
                "timeout_seconds: {}",
                plan.timeout_seconds
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "none".to_string())
            );
            println!("network: {}", plan.network);
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(1)
        }
    }
}

fn run_artifacts(arguments: &[String]) -> ExitCode {
    match arguments.first().map(String::as_str) {
        Some("hash") => run_artifacts_hash(&arguments[1..]),
        _ => {
            eprintln!("error: expected `axiom artifacts hash <FILE>`\n");
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn run_artifacts_hash(arguments: &[String]) -> ExitCode {
    let mut path: Option<PathBuf> = None;
    let mut kind = ArtifactKind::Other;
    let mut iter = arguments.iter();
    while let Some(arg) = iter.next() {
        if arg == "--kind" {
            let Some(value) = iter.next() else {
                eprintln!("error: `--kind` needs a value");
                return ExitCode::from(2);
            };
            match artifact_kind(value) {
                Some(parsed) => kind = parsed,
                None => {
                    eprintln!("error: unknown artifact kind `{value}`");
                    return ExitCode::from(2);
                }
            }
        } else if let Some(value) = arg.strip_prefix("--kind=") {
            match artifact_kind(value) {
                Some(parsed) => kind = parsed,
                None => {
                    eprintln!("error: unknown artifact kind `{value}`");
                    return ExitCode::from(2);
                }
            }
        } else if !arg.starts_with('-') {
            path = Some(PathBuf::from(arg));
        }
    }

    let Some(path) = path else {
        eprintln!("error: `artifacts hash` needs a file\n");
        eprint!("{USAGE}");
        return ExitCode::from(2);
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("error: cannot read `{}`: {err}", path.display());
            return ExitCode::from(1);
        }
    };

    let mut store = ArtifactStore::new();
    let id = store.publish(kind, bytes);
    let info = store.inspect(&id).expect("just published");
    println!("{info}");
    println!("verified: {}", store.verify(&id));
    ExitCode::SUCCESS
}

fn run_db(arguments: &[String]) -> ExitCode {
    match arguments.first().map(String::as_str) {
        Some("check") => run_db_check(&arguments[1..]),
        Some("migrations") => {
            if arguments.get(1).map(String::as_str) == Some("plan") {
                run_db_migrations_plan(&arguments[2..])
            } else {
                eprintln!("error: expected `axiom db migrations plan --config <FILE> --database <NAME>`\n");
                eprint!("{USAGE}");
                ExitCode::from(2)
            }
        }
        _ => {
            eprintln!("error: expected `axiom db check --config <FILE>`\n");
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Read and parse `--config`, returning the project config and its root.
fn database_config(config_path: &str) -> Result<(axiom_config::ProjectConfig, PathBuf), ExitCode> {
    let text = match std::fs::read_to_string(config_path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("error: cannot read `{config_path}`: {err}");
            return Err(ExitCode::from(1));
        }
    };
    let report = parse_config(config_path, &text);
    let source = Source::new(config_path.to_string(), text);
    for diagnostic in &report.diagnostics {
        eprint!("{}", diagnostic.render(&source));
    }
    if report.has_errors() {
        return Err(ExitCode::from(1));
    }
    let Some(config) = report.config else {
        return Err(ExitCode::from(1));
    };
    let root = database::project_root(std::path::Path::new(config_path));
    Ok((config, root))
}

/// Validate configured databases: engine settings, the PostgreSQL target
/// policy, and (for SQLite) that the least-privilege roles behave as claimed.
fn run_db_check(arguments: &[String]) -> ExitCode {
    let Some(config_path) = flag_value(arguments, "--config") else {
        eprintln!("error: `db check` needs `--config <FILE>`\n");
        eprint!("{USAGE}");
        return ExitCode::from(2);
    };
    let (config, root) = match database_config(config_path) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    if config.databases.is_empty() {
        println!("no databases configured");
        return ExitCode::SUCCESS;
    }

    let mut failed = false;
    for (name, database_config) in &config.databases {
        println!("{}", database::describe(name, database_config));
        match database_config.engine.as_str() {
            "postgres" => {
                if let Err(message) = database::postgres_policy(name, database_config) {
                    eprintln!("error: {message}");
                    failed = true;
                }
            }
            "sqlite" => {
                let spec = match database::spec_from_config(&root, name, database_config) {
                    Ok(spec) => spec,
                    Err(message) => {
                        eprintln!("error: {message}");
                        failed = true;
                        continue;
                    }
                };
                match Database::open(spec) {
                    Ok(database) => match database.audit_roles() {
                        Ok(roles) => {
                            for role in roles {
                                println!(
                                    "    {}: read={} write={} schema={}",
                                    role.role.as_str(),
                                    role.can_read,
                                    role.can_write,
                                    role.can_schema
                                );
                            }
                        }
                        Err(error) => {
                            eprintln!("error: database `{name}`: {}", error.message());
                            failed = true;
                        }
                    },
                    Err(error) => {
                        eprintln!("error: database `{name}`: {}", error.message());
                        failed = true;
                    }
                }
            }
            other => {
                eprintln!("error: database `{name}` uses unknown engine `{other}`");
                failed = true;
            }
        }
    }
    exit_code(failed)
}

/// Print the pending reviewed migrations for one configured database.
fn run_db_migrations_plan(arguments: &[String]) -> ExitCode {
    let Some(config_path) = flag_value(arguments, "--config") else {
        eprintln!("error: `db migrations plan` needs `--config <FILE>`\n");
        eprint!("{USAGE}");
        return ExitCode::from(2);
    };
    let Some(name) = flag_value(arguments, "--database") else {
        eprintln!("error: `db migrations plan` needs `--database <NAME>`\n");
        eprint!("{USAGE}");
        return ExitCode::from(2);
    };
    let (config, root) = match database_config(config_path) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    let Some(database_config) = config.databases.get(name) else {
        eprintln!("error: no database named `{name}` is configured");
        return ExitCode::from(1);
    };
    match database::plan(&root, name, database_config) {
        Ok(plan) => {
            let destructive = axiom_database::migrations::destructive_changes(&plan);
            println!(
                "database: {name}\nfiles: {}\npending: {}\napplied version: {}",
                plan.files.len(),
                plan.pending.len(),
                plan.applied_version
            );
            for file in &plan.pending {
                println!("  pending {} {}", file.version, file.name);
            }
            if destructive.is_empty() {
                println!("destructive changes: none");
            } else {
                println!("destructive changes: {}", destructive.len());
                for change in destructive {
                    println!("  {}: {}", change.version, change.kinds.join(", "));
                }
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {}", error.message());
            ExitCode::from(1)
        }
    }
}

fn artifact_kind(name: &str) -> Option<ArtifactKind> {
    match name {
        "wasm_module" => Some(ArtifactKind::WasmModule),
        "patch" => Some(ArtifactKind::Patch),
        "ui_bundle" => Some(ArtifactKind::UiBundle),
        "snapshot" => Some(ArtifactKind::Snapshot),
        "other" => Some(ArtifactKind::Other),
        _ => None,
    }
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
