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
| 2 | Module resolution, types, generics, Option/Result, effect/target checking | partial |
| 3 | Typed IR and deterministic server evaluator; compiler commands | done |
| 4 | WASM lowering, UI tree, host ABI, browser loader/renderer | partial |
| 5 | Config, model integrations, registered tools and commands | partial |
| 6 | Filesystem, workspaces, patch/artifact APIs, command sandbox | partial |
| 7 | Database APIs, roles, parameter checks, approval/audit, migrations | not started |
| 8 | Agent calls, iteration/validation APIs, budgets, typed results | not started |
| 9 | Frontend state, validation/migration, artifact registry and caches | not started |
| 10 | Durable async execution, checkpoints, resume, cancellation, tracing | not started |
| 11 | Full-stack reference app, tooling, docs, security hardening | not started |
| 12 | Release-readiness audit | not started |

[`docs/implementation-ledger.md`](docs/implementation-ledger.md) is the source of
truth for what works and what has test evidence. Do not assume a feature works
unless the ledger cites a passing test.

## Documentation

- [`docs/language-spec.md`](docs/language-spec.md) — the language.
- [`docs/grammar.ebnf`](docs/grammar.ebnf) — the normative grammar.
- [`docs/configuration.md`](docs/configuration.md) — `axiom.yaml`.
- [`docs/architecture.md`](docs/architecture.md) — the pipeline and crates.
- [`docs/security-model.md`](docs/security-model.md) — threat model and invariants.
- [`docs/conformance.md`](docs/conformance.md) — requirement-to-test matrix.
- [`docs/decisions/`](docs/decisions/) — architecture decision records.

## Layout

```
crates/
  axiom-syntax/    lexer, parser, AST, spans, diagnostics
  axiom-format/    deterministic formatter
  axiom-types/     module graph, symbols, type/effect/target checks
  axiom-hash/      dependency-free SHA-256 for content addresses
  axiom-ir/        shared typed IR, lowering, deterministic JSON
  axiom-runtime/   deterministic server evaluator
  axiom-wasm/      browser WASM backend + artifact manifest (Phase 4 partial)
  axiom-browser-host/ trusted host ABI + artifact verification
  axiom-config/    axiom.yaml loading, expansion, validation
  axiom-providers/ model alias resolution + offline mock provider (Phase 5 partial)
  axiom-tools/     builtin + configured-command tool registry, command sandbox
  axiom-filesystem/ secure workspaces: path policy, copy-on-write, patches (Phase 6 partial)
  axiom-artifacts/ content-addressed immutable artifact store
  axiom-cli/       the `axiom` binary
browser/           trusted host shell + Node smoke harness
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
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p axiom-cli -- check examples/src/*.ax
cargo run -p axiom-cli -- config check examples/axiom.yaml
cargo run -p axiom-cli -- explain examples/src/deterministic.ax
cargo run -p axiom-cli -- run examples/src/deterministic.ax
cargo run -p axiom-cli -- build --target wasm_browser --out out examples/src/deterministic.ax
node browser/host-smoke.mjs out/app.wasm out/app.manifest.json
```

## License

Dual-licensed under MIT or Apache-2.0 (see `Cargo.toml`).
