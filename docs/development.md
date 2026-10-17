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

`Cargo.toml` + `rust-toolchain.toml` resolve `channel = "1.99.0"` to the GNU
host automatically. Keep Cargo caches off the small `C:` drive:

```powershell
$env:CARGO_HOME='E:\cargo-home'
$env:TEMP='E:\cargo-temp'; $env:TMP='E:\cargo-temp'
```

## Commands

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p axiom-cli -- check examples/hello_agent.ax
cargo run -p axiom-cli -- config check examples/axiom.yaml
```

These are the exact gate commands run in CI
(`.github/workflows/ci.yml`), minus the Windows env prefix.

## Application Control (`os error 4551`)

If a build fails with *"An Application Control policy has blocked this file"*, an
unsigned binary, build script, or proc-macro was content-blocked. The mitigation
is to keep the default build dependency-light and sign generated artifacts when
needed:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/sign-windows.ps1
```

Phase 1 keeps `axiom-syntax` free of third-party dependencies, so the `.ax`
front end should not trigger this. `axiom-config` adds `yaml_serde` (see
ADR-0009); it becomes more relevant from Phase 4 onward (async + HTTP).

## Dependency policy

- Minimize dependencies; pin to currently-stable, compatible versions, and
  record the rationale in `docs/decisions/`.
- Do not assume crate APIs from memory — check the current documentation.
- External/network tests must be opt-in and never required for normal CI.

## Phase parity

Phase numbers follow `agents.md` §14; `docs/implementation-ledger.md` is the
source of truth.

| Phase | Deliverable | State |
| --- | --- | --- |
| 0 | Architecture, ADRs, grammar, workspace, CI | partial |
| 1 | Lexer, parser, AST, diagnostics; `axiom.yaml` load/validate | done |
| 2 | Module graph, symbols, type system, effects, cross-validation | partial |
| 3 | Typed IR + `explain`/`plan` | pending |
| 4 | Integrations, adapters, model profiles | pending |
| 5 | Filesystem, typed tools, artifacts | pending |
| 6 | Agent runtime, teams, goals, contracts | pending |
| 7 | Workflow engine + durable state | pending |
| 8 | Standard library + reference program | pending |
| 9 | CLI, formatter, linter, LSP | pending |
| 10 | Hardening and docs | pending |
| 11 | Release sign-off | pending |

Phase 1 is parsing and configuration validation. Phase 2 is in progress: the
module graph, symbol tables, and declaration/aggregate/effect checks run under
`axiom check` (optionally with `--config`); full type inference is not
implemented. The CLI never contacts an integration and no `.ax` code is executed.
