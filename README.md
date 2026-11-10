# Axiom

Axiom is a small, strongly typed language and runtime for building full-stack
applications with AI agents operating inside explicit boundaries and isolated
workspaces. One source language and one shared type/effect system compile for two
required targets: `server` (trusted backend, database, authorized API operations)
and `wasm_browser` (Axiom frontend compiled to WebAssembly running in a trusted
browser host).

> **Boundaries define authority. Workspaces define where work happens. Agents
> propose changes. Validation determines what is accepted.**

The language surface is deliberately small: `module`, `import`, `export`, `type`,
`enum`, `fn`, `agent`, `workspace`, plus the `api`/`async` function modifiers and
an `effects` clause. Everything else — agent iteration, validation, caching,
persistence, retries, checkpoints, database queries, tools, artifact publishing,
and parallel work — is a standard-library API or `axiom.yaml` configuration, not
grammar.

## Status

Axiom 1.0 is **not** complete. This repository is being built phase by phase from
the current [`agents.md`](agents.md) specification. The phase numbers below follow
`agents.md` §15.

| # | Phase | State |
| --- | --- | --- |
| 0 | Design: small grammar, types, effects, target matrix, ABI, threat model | done |
| 1 | Lexer, parser, AST, source maps, diagnostics, formatter skeleton | done |
| 2 | Module resolution, types, generics, Option/Result, effect/target checking | done (conservative inference) |
| 3 | Typed IR and deterministic server evaluator; compiler commands | done |
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
| 14 | HTTP server core + ApiGuard + typed stubs | done (tested reference slice) |
| 15 | Browser host, UiTree renderer, event loop, headless test | done (IR host + stubs + reference test) |

[`docs/implementation-ledger.md`](docs/implementation-ledger.md) is the source of
truth for what works and what has test evidence. Do not assume a feature works
unless the ledger cites a passing test.

## Documentation

- [`docs/quickstart.md`](docs/quickstart.md) — install, scaffold, check, run, and build.
- [`docs/language-tour.md`](docs/language-tour.md) — types, control flow, Option/Result, effects, modules.
- [`docs/examples.md`](docs/examples.md) — server, database, frontend, agent, and configuration examples.
- [`docs/cli.md`](docs/cli.md) — every CLI command and flag.
- [`docs/language-spec.md`](docs/language-spec.md) — the language.
- [`docs/grammar.ebnf`](docs/grammar.ebnf) — the normative grammar.
- [`docs/configuration.md`](docs/configuration.md) — `axiom.yaml`.
- [`docs/architecture.md`](docs/architecture.md) — the pipeline and crates.
- [`docs/security-model.md`](docs/security-model.md) — threat model and invariants.
- [`docs/conformance.md`](docs/conformance.md) — requirement-to-test matrix.
- [`docs/release-readiness.md`](docs/release-readiness.md) — release audit, evidence, and open gaps.
- [`docs/remaining-phases.md`](docs/remaining-phases.md) — the ordered plan for the remaining work.
- [`docs/github-pages.md`](docs/github-pages.md) — publish the docs site and deploy Axiom frontends to GitHub Pages.
- [`docs/decisions/`](docs/decisions/) — architecture decision records.

## Layout

```
crates/
  axiom-syntax/    lexer, parser, AST, spans, diagnostics
  axiom-format/    deterministic formatter
  axiom-types/     module graph, symbols, type/effect/target checks
  axiom-hash/      dependency-free SHA-256 for content addresses
  axiom-ir/        shared typed IR, lowering, deterministic JSON
  axiom-runtime/   deterministic evaluator, hosts, agents, budgets
  axiom-wasm/      browser WASM backend + artifact manifest (Phase 4 partial)
  axiom-browser-host/ trusted host ABI + artifact verification
  axiom-config/    axiom.yaml loading, expansion, validation
  axiom-providers/ model alias resolution + offline mock provider (Phase 5 partial)
  axiom-tools/     tool registry, shell-free sandbox plans, process command runner
  axiom-filesystem/ secure workspaces: policy, overlays, patches, worktrees, integration lock
  axiom-artifacts/ content-addressed immutable artifact store
  axiom-database/  parameterized SQL, roles, audit, approvals, migrations
  axiom-server/    HTTP server core, routes, ApiGuard boundary, JSON bridge
  axiom-cli/       the `axiom` binary
browser/           trusted browser host, IR interpreter, headless smoke harness
editors/vscode/    VS Code syntax highlighting for .ax
examples/          axiom.yaml and src/ (the reference project)
docs/              spec, grammar, configuration, architecture, security, ledger, ADRs
```

## Building

The workspace pins Rust **1.99.0** (see `docs/development.md` for the Windows GNU
toolchain notes).

```sh
cargo check --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo run -p axiom-cli -- check examples/src/*.ax
cargo run -p axiom-cli -- createapp my_app
cargo run -p axiom-cli -- check my_app/server/main.ax my_app/frontend/main.ax --config my_app/config.yml
cargo run -p axiom-cli -- config check examples/axiom.yaml
cargo run -p axiom-cli -- workspace check examples/src/agents.ax
cargo run -p axiom-cli -- sandbox plan --config examples/axiom.yaml --command cargo_check
cargo run -p axiom-cli -- artifacts hash examples/axiom.yaml
cargo run -p axiom-cli -- explain examples/src/deterministic.ax
cargo run -p axiom-cli -- run examples/src/deterministic.ax
cargo run -p axiom-cli -- build --target wasm_browser --out out examples/src/deterministic.ax
node browser/host-smoke.mjs out/app.wasm out/app.manifest.json
cargo run -p axiom-cli -- build --target wasm_browser --out dist examples/src/ui_demo.ax
node browser/headless-smoke.mjs dist/app.wasm dist/app.manifest.json
cargo run -p axiom-cli -- build --target wasm_browser --out dist_ref examples/src/shared.ax examples/src/frontend.ax examples/src/server.ax
node browser/headless-smoke.mjs dist_ref/app.wasm dist_ref/app.manifest.json --input-file browser/fixtures/reference_input.json --api-file browser/fixtures/reference_api.json --expect "TeamBoard" --expect "Completed tasks" --expect "7"
```

`config check` and `sandbox plan` fail closed on `examples/axiom.yaml` until the
referenced integration API-key environment variables are set (or given defaults
in the config).

On Windows, build, sign, and install the CLI so Application Control does not
block it:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/install-axiom.ps1
axiom version
```

## Editor support

`editors/vscode/` is a syntax-highlighting extension for `.ax` files. Open that
folder in VS Code and press **F5**, or package it:

```sh
cd editors/vscode
npx @vscode/vsce package
code --install-extension axiom-language-0.1.0.vsix
```

## License

Dual-licensed under MIT or Apache-2.0 (see `Cargo.toml`).
