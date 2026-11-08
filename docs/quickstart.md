---
title: Quickstart
description: Install the Axiom CLI, scaffold a project, check it, run a pure module, and build a browser artifact.
---

# Quickstart

This guide takes you from a fresh checkout to a checked two-target project. It
uses only features the repository tests today. Browser UI rendering and HTTP
serving are not wired end to end yet; the [status callout](#what-works-today)
below is explicit about that.

## 1. Prerequisites

| Tool | Why |
| --- | --- |
| Rust 1.99.0 | The workspace pins this toolchain in `rust-toolchain.toml`. |
| Git | To clone the repository. |
| Node.js | Only for the browser artifact smoke harness (`browser/host-smoke.mjs`). |
| VS Code (optional) | For `.ax` syntax highlighting from `editors/vscode`. |

On Windows this repository is developed with the GNU host
(`x86_64-pc-windows-gnu`). See [Development](development.md) for toolchain and
Application Control notes.

## 2. Install the CLI

From the repository root:

```sh
cargo install --path crates/axiom-cli --locked
axiom version
```

Expected:

```text
axiom 0.1.0
```

You can also run the CLI without installing it:

```sh
cargo run -p axiom-cli -- version
```

Every command in this guide works with either `axiom ...` or
`cargo run -p axiom-cli -- ...`.

## 3. Create a project

```sh
axiom createapp hello
cd hello
```

`createapp` writes three files and refuses to overwrite an existing directory:

```text
hello/
  config.yml          # two targets, entry modules
  server/main.ax      # module hello.server, api fn health
  frontend/main.ax    # module hello.frontend, fn main
```

`config.yml` declares both compilation targets:

```yaml
version: 1
language: "1.0"

sources:
  - "server/**/*.ax"
  - "frontend/**/*.ax"

compilation:
  targets: [server, wasm_browser]
  entrypoints:
    server:
      module: hello.server
    wasm_browser:
      module: hello.frontend
```

## 4. Check, format, and lint

```sh
axiom check server/main.ax frontend/main.ax --config config.yml
axiom config check config.yml
axiom fmt server/main.ax frontend/main.ax --check
axiom lint server/main.ax frontend/main.ax
```

`check` parses, resolves modules and names, and applies the effect and target
checks it implements. `lint` reports the same stable diagnostics. `fmt --check`
does not rewrite files; omit `--check` to format in place.

Example output:

```text
ok: server/main.ax (1 declaration(s))
ok: frontend/main.ax (1 declaration(s))
checked 2 files: 0 error(s), 0 warning(s)
```

## 5. Run a pure module

```sh
axiom run frontend/main.ax
```

Expected:

```text
"hello from hello"
```

`run` evaluates a pure entry function with the deterministic server evaluator.
Use `--fn NAME` to choose a different entry function. Effectful calls fail closed
in this evaluator unless a runtime host is supplied.

## 6. Inspect and build the typed IR

```sh
axiom explain frontend/main.ax
axiom build server/main.ax
axiom build --target wasm_browser --out dist frontend/main.ax
```

- `explain` prints the typed IR as deterministic JSON.
- `build` checks for the selected target and emits IR or an artifact manifest.
- The browser build writes `dist/app.wasm` and `dist/app.manifest.json`.

Verify the browser artifact from the repository root:

```sh
node browser/host-smoke.mjs dist/app.wasm dist/app.manifest.json
```

Expected shape:

```text
ok: sha256 … abi v1 ir v1 … bytes, 1 module(s)
```

This is an artifact/ABI smoke test, not a DOM or browser UI test yet.

## 7. Edit with syntax highlighting

The repository ships a grammar-only VS Code extension at `editors/vscode/`:

```sh
code editors/vscode
```

Press **F5** in VS Code to open an Extension Development Host, then open any
`.ax` file. To package it:

```sh
cd editors/vscode
npx @vscode/vsce package
code --install-extension axiom-language-0.1.0.vsix
```

## 8. Add a database connection

Add a named connection to `config.yml`. Code selects it by name, not vendor:

```yaml
databases:
  app:
    driver: sqlite
    path: "var/app.sqlite"
    data_class: internal
    allowed_tables: [teams, tasks]
    max_rows: 1000
    timeout_seconds: 5
    migrations: "db/migrations"
    require_write_approval: true
```

Validate the configured connections and the SQLite role boundaries:

```sh
axiom db check --config config.yml
```

The configuration shape is driver-independent. `sqlite` is the bundled
reference driver; other drivers are host-registered adapters. See
[Configuration](configuration.md) and the [examples](examples.md).

## 9. Where to go next

- [Language tour](language-tour.md) — types, enums, functions, control flow,
  `Option`/`Result`, modules, and effects.
- [Examples](examples.md) — server APIs, database access, frontend state,
  agents, workspaces, and validation.
- [CLI reference](cli.md) — every command and flag.
- [Language specification](language-spec.md) — the complete semantics.
- [Configuration](configuration.md) — the strict `axiom.yaml` schema.
- [Known limitations](release-readiness.md) — what is not implemented yet.

## What works today

| Area | Status |
| --- | --- |
| Lexer, parser, AST, spans, diagnostics | works |
| Module graph, symbol tables, name/type resolution, effect and target checks | works for the tested subset; general expression inference and match exhaustiveness are incomplete |
| Deterministic server evaluator (`axiom run`) | works for the pure subset |
| Typed IR (`axiom explain`, `axiom build`) | works |
| WASM artifact + ABI verification | artifact embeds IR; no per-function codegen or DOM renderer |
| SQLite roles, approvals, audit, migrations | works offline |
| Workspaces, patches, artifacts, command sandbox | works for the tested subset |
| Agent iteration and validation | library/runtime tests; networked providers are not implemented |
| Interactive browser app / HTTP server | not implemented |

A compiling WASM artifact is not proof that a UI works. The
[implementation ledger](implementation-ledger.md) records the evidence for every
claim.
