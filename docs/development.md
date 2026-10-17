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

| Phase | Deliverable | State |
| --- | --- | --- |
| 0 | Architecture, ADRs, grammar draft, workspace, CI | done |
| 1 | Lexer, parser, AST, diagnostics, `axiom check`; `axiom.yaml` load/validate, `axiom config check` | done |
| 2 | Semantic analysis and contracts | pending |
| 3 | IR + `explain`/`plan` | pending |
| 4 | Integration layer, adapters, model profiles | pending |
| 5 | Runtime + typed tools | pending |
| 6 | Workflows + verification gates | pending |
| 7 | Example app + hardening | pending |

Phase 1 is parsing and configuration validation only: no semantic checks, no
model calls, no execution. Semantic errors are intentionally deferred to Phase 2
and the CLI never contacts an integration. See `docs/implementation-ledger.md`
for the full Axiom 1.0 phase status.
