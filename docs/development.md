# Development

## Toolchain

The workspace pins Rust **1.99.0** in `rust-toolchain.toml`. On Linux/macOS the
default host is used. On this Windows machine the **GNU** host is required: the
MSVC linker (`link.exe`) is not installed, and the Application Control policy can
content-block unsigned build artifacts.

```powershell
# one-time
rustup set default-host x86_64-pc-windows-gnu
rustup toolchain install 1.99.0-x86_64-pc-windows-gnu --profile minimal --component rustfmt,clippy
```

Keep Cargo caches off the small `C:` drive:

```powershell
$env:CARGO_HOME='E:\cargo-home'
$env:TEMP='E:\cargo-temp'; $env:TMP='E:\cargo-temp'
```

## Gate commands

These are the exact gate commands run in CI (`.github/workflows/ci.yml`), minus
the Windows env prefix:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Additional smoke commands:

```sh
cargo run -p axiom-cli -- check examples/src/*.ax
cargo run -p axiom-cli -- config check examples/axiom.yaml
```

## Application Control (`os error 4551`)

If a build fails with *"An Application Control policy has blocked this file"*, an
unsigned binary, build script, or proc-macro was content-blocked. Keep the default
build dependency-light and sign generated artifacts when needed:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/sign-windows.ps1
```

`axiom-syntax` has no third-party dependencies, so the `.ax` front end should not
trigger this. `axiom-config` adds `yaml_serde` (ADR-0009).

## Dependency policy

- Minimize dependencies; pin to currently-stable, compatible versions, and record
  the rationale in `docs/decisions/`.
- Do not assume crate APIs from memory — verify current documentation first, and
  record verification dates for provider APIs.
- External/network tests must be opt-in and never required for the offline
  default suite.

## Phase parity

Phase numbers follow `agents.md` §15; `docs/implementation-ledger.md` is the
source of truth.

| Phase | Deliverable | State |
| --- | --- | --- |
| 0 | Small grammar, types, effects, target matrix, browser ABI, threat model | in progress |
| 1 | Lexer, parser, AST, source maps, diagnostics, formatter skeleton | not started |
| 2 | Module resolution, types, generics, Option/Result, effects, target checks | not started |
| 3 | Typed IR + deterministic server evaluator + compiler commands | not started |
| 4 | WASM lowering, UI tree, host ABI, browser loader/renderer | not started |
| 5 | Config, model integrations, registered tools and commands | not started |
| 6 | Filesystem, workspaces, patch/artifact APIs, command sandbox | not started |
| 7 | Database APIs, roles, parameter checks, approval/audit, migrations | not started |
| 8 | Agent calls, iteration/validation APIs, budgets, typed results | not started |
| 9 | Frontend state, validation/migration, artifact registry and caches | not started |
| 10 | Durable async execution, checkpoints, resume, cancellation, tracing | not started |
| 11 | Full-stack reference app, tooling, docs, security hardening | not started |
| 12 | Release-readiness audit | not started |

Every phase ends with a compiling repository and passing relevant tests. Nothing
is marked done without evidence in the ledger.
