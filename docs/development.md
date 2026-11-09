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
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Additional smoke commands:

```sh
cargo run -p axiom-cli -- init .axiom-smoke
cargo run -p axiom-cli -- createapp my_app
cargo run -p axiom-cli -- check my_app/server/main.ax my_app/frontend/main.ax --config my_app/config.yml
cargo run -p axiom-cli -- build --target wasm_browser my_app/frontend/main.ax
cargo run -p axiom-cli -- fmt examples/src/deterministic.ax --check
cargo run -p axiom-cli -- lint examples/src/deterministic.ax
cargo run -p axiom-cli -- check examples/src/*.ax
cargo run -p axiom-cli -- config check examples/axiom.yaml
cargo run -p axiom-cli -- workspace check examples/src/agents.ax
cargo run -p axiom-cli -- sandbox plan --config examples/axiom.yaml --command cargo_check
cargo run -p axiom-cli -- artifacts hash examples/axiom.yaml
cargo run -p axiom-cli -- db check --config examples/axiom.yaml
cargo run -p axiom-cli -- db migrations plan --config examples/axiom.yaml --database app
```

`config check` and `sandbox plan` fail closed on `examples/axiom.yaml` until the
referenced integration API-key environment variables are set (or given defaults
in the config).

## Editor support

`editors/vscode/` contains a grammar-only VS Code extension for `.ax` files. Open
that folder and press **F5** for an Extension Development Host, or package it:

```sh
cd editors/vscode
npx @vscode/vsce package
code --install-extension axiom-language-0.1.0.vsix
```

It provides syntax highlighting, comment toggling, bracket matching, and
indentation only; diagnostics still come from `axiom check`/`axiom lint`.

The documentation site is a Jekyll site rooted at `docs/` (`docs/index.md`,
`docs/_config.yml`). GitHub Pages setup, local preview, custom domains, and the
(currently blocked) Axiom frontend deployment are documented in
[`github-pages.md`](github-pages.md).

The site highlights Axiom code with a small Prism grammar
(`docs/assets/prism-axiom.js`, vendored core under `docs/assets/vendor/`).
Validate the docs plumbing with:

```sh
node scripts/check-axiom-highlight.mjs
node scripts/check-docs-links.mjs
```

## Application Control (`os error 4551`)

If a build fails with *"An Application Control policy has blocked this file"*, an
unsigned binary, build script, proc-macro, or the MinGW linker was content-blocked.
Set up the local signing workaround once (and re-run it after a WinGet/MinGW
upgrade):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/setup-windows.ps1
```

That script:

- signs the MinGW `x86_64-w64-mingw32\bin\ld.exe` with the already-trusted dev
  certificate (backing up the original as `ld.exe.unsigned.bak`), which fixes
  `collect2.exe: fatal error: CreateProcess: No such file or directory`;
- writes a local `.cargo/config.toml` runner that copies each test/binary out of
  `target/`, strips debug info, signs it, and retries when the policy blocks it.

The local Cargo config is machine-specific and excluded from git. Use
`-SkipToolchain` if the linker already works. Override the certificate with
`-CertThumbprint`. On CI and unaffected machines, `cargo test --workspace` runs
normally without these scripts.

The policy is content-based and intermittent; a signed copy can still be blocked
once. The runner retries with a fresh copy, which is why it is preferred over
running `target/` binaries directly.

### Build, sign, and install the CLI

Use the installer instead of running an unsigned `target/` binary:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/install-axiom.ps1
```

It builds `axiom-cli` in release mode, signs `target/release/axiom.exe` with the
trusted `Axiom Dev Signing`/`MeshCloud Dev Signing` certificate, copies it to
`%USERPROFILE%\.cargo\bin\axiom.exe`, and verifies the installed signature. Then:

```powershell
axiom version
```

Use `-SkipBuild` to sign and install an existing release build. `sign-axiom.ps1`
can sign other Axiom binaries directly:

```powershell
powershell -File scripts/sign-axiom.ps1 -Path target\release\axiom.exe
```

If no trusted signing certificate exists, create one, trust it in
`CurrentUser\TrustedPublisher`, and pass its thumbprint with `-CertThumbprint`.

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
| 0 | Small grammar, types, effects, target matrix, browser ABI, threat model | done |
| 1 | Lexer, parser, AST, source maps, diagnostics, formatter skeleton | done |
| 2 | Module resolution, types, generics, Option/Result, effects, target checks | done (conservative inference) |
| 3 | Typed IR + deterministic server evaluator + compiler commands | done |
| 4 | WASM lowering, UI tree, host ABI, browser loader/renderer | partial |
| 5 | Config, model integrations, registered tools and commands | partial |
| 6 | Filesystem, workspaces, patch/artifact APIs, command sandbox | done |
| 7 | Database APIs, roles, parameter checks, approval/audit, migrations | done (SQLite driver) |
| 8 | Agent calls, iteration/validation APIs, budgets, typed results | partial |
| 9 | Frontend state, validation/migration, artifact registry and caches | partial |
| 10 | Durable async execution, checkpoints, resume, cancellation, tracing | partial |
| 11 | Full-stack reference app, tooling, docs, security hardening | partial |
| 12 | Release-readiness audit | done (audit; release not ready) |
| 13 | Expression inference and match exhaustiveness | done |
| 14 | HTTP server core + ApiGuard boundary | partial (core + reference route tested) |

Every phase ends with a compiling repository and passing relevant tests. Nothing
is marked done without evidence in the ledger.
