---
title: CLI reference
description: Every command, flag, input, and output supported by the axiom binary.
---

# CLI reference

`axiom` is the Axiom toolchain. This page lists every command the binary supports
today. Commands not listed here (for example `axiom test`, `axiom dev`,
`axiom status`, `axiom inspect`, `axiom resume`, and `axiom cancel`) are not
implemented yet.

## Invocation

```sh
axiom <COMMAND> [OPTIONS]
```

Without an installed binary:

```sh
cargo run -p axiom-cli -- <COMMAND> [OPTIONS]
```

## Global help and version

```sh
axiom help
axiom --help
axiom -h
axiom version
axiom --version
axiom -V
```

`version` prints the package version, currently `axiom 0.1.0`.

## `init`

Create a minimal single-module project skeleton.

```sh
axiom init [DIR]
```

Without `DIR`, the project is created in the current directory. The command
writes `axiom.yaml` and `src/main.ax`, then refuses to overwrite an existing
project.

## `createapp`

Create a two-target application skeleton.

```sh
axiom createapp <NAME>
```

Writes:

```text
<NAME>/
  config.yml
  server/main.ax
  frontend/main.ax
```

`NAME` must be a valid module identifier (ASCII letter first, then letters,
digits, or underscores). The command refuses to overwrite an existing directory.

## `check`

Parse the module graph, resolve names and types, infer expression types, check
match exhaustiveness, and apply the implemented effect, target, and SQL-policy
checks.

```sh
axiom check <FILE>... [--config <FILE>]
```

- `<FILE>...` may be one or more `.ax` files. Pass every file in the module graph.
- `--config <FILE>` cross-validates source against `axiom.yaml` where the checker
  supports it (for example model aliases).
- Exit code `0`: no errors. Exit code `1`: diagnostics reported.

```sh
axiom check examples/src/shared.ax examples/src/server.ax --config examples/axiom.yaml
```

Diagnostics carry stable codes such as `AX0010` (forbidden declaration),
`AX0201` (missing import), `AX0303` (undeclared effect), `AX0320` (non-literal
SQL), `AX0401` (type mismatch), and `AX0408` (non-exhaustive match).

## `lint`

Report the same stable diagnostics as `check`.

```sh
axiom lint <FILE>... [--config <FILE>]
```

There is no separate lint rule set or autofix yet.

## `fmt`

Format source deterministically.

```sh
axiom fmt <FILE>... [--check]
```

- Without `--check`: rewrite files in the canonical layout.
- With `--check`: report which files would change and exit non-zero.
- The formatter is idempotent: formatting an already formatted file is a no-op.

```sh
axiom fmt examples/src/deterministic.ax --check
axiom fmt examples/src/deterministic.ax
```

## `build`

Check for a target and lower to typed IR or a WASM artifact.

```sh
axiom build <FILE>... [--target server|wasm_browser] [--emit-ir] [--out <DIR>]
```

| Flag | Meaning |
| --- | --- |
| `--target server` | Default target. |
| `--target wasm_browser` | Browser target; server-only effects are refused. |
| `--emit-ir` | Print the lowered IR/manifest JSON. |
| `--out <DIR>` | With `wasm_browser`, write `app.wasm` and `app.manifest.json`. |

```sh
axiom build examples/src/deterministic.ax
axiom build --target wasm_browser --out dist my_app/frontend/main.ax
```

A `wasm_browser` artifact embeds the checked IR. Exported `api` functions are
replaced by typed caller-side `api.call` stubs, so the server handler body and its
database effects do not enter the artifact. The trusted host interprets the IR
and renders `UiTree`; there is no per-function WASM codegen yet.

## `explain`

Print the typed IR as deterministic JSON.

```sh
axiom explain <FILE>... [--target server|wasm_browser]
```

```sh
axiom explain examples/src/deterministic.ax
```

The IR contains module/type/function declarations, effects, capabilities,
provenance, and the target.

## `run`

Evaluate an entry function with the deterministic server evaluator.

```sh
axiom run <FILE>... [--fn <NAME>]
```

- The default entry function is `main`.
- Use `--fn` to select another exported function.
- The evaluator is deterministic and fail-closed for effectful calls: pure code
  runs; host-dependent effects require a runtime host and are rejected here.
- There is no `while` loop and no general recursion in the language surface.

```sh
axiom run examples/src/deterministic.ax
axiom run examples/src/deterministic.ax --fn describe
```

## `config check`

Validate an `axiom.yaml` configuration.

```sh
axiom config check <FILE>...
```

Checks version/language compatibility, strict unknown keys, target names,
entrypoints, placeholder expansion, model aliases, database connection shapes,
browser limits, and other supported sections.

```sh
axiom config check examples/axiom.yaml
```

`examples/axiom.yaml` references integration keys through environment variables.
`config check` reports a warning when an optional key is missing and an error
when a required key cannot resolve.

## `workspace check`

Parse `workspace` declarations and build the enforcing runtime policy.

```sh
axiom workspace check <FILE>...
```

```sh
axiom workspace check examples/src/agents.ax
```

It validates read/write/deny/protected patterns, isolation modes, and limits, and
prints the resulting policy summary.

## `sandbox plan`

Turn a configured command profile plus arguments into a shell-free command plan.

```sh
axiom sandbox plan --config <FILE> --command <NAME> [--arg <VALUE>]... [--dir <DIR>]
```

```sh
axiom sandbox plan --config examples/axiom.yaml --command cargo_check
```

The plan prints the executable and structured argv. Unknown profiles, unsafe
working directories, and network-enabled profiles are refused. Commands are
never assembled from a shell string.

## `artifacts hash`

Print a file's immutable content address.

```sh
axiom artifacts hash <FILE> [--kind wasm_module|patch|ui_bundle|snapshot|other]
```

```sh
axiom artifacts hash examples/axiom.yaml
```

The output includes the digest and whether it verified against its content.

## `db check`

Validate configured database connections and probe the bundled SQLite driver's
least-privilege roles.

```sh
axiom db check --config <FILE>
```

```sh
axiom db check --config examples/axiom.yaml
```

It prints a redacted one-line description per connection and, for `sqlite`,
`read`/`write`/`schema` role capabilities. A connection whose `driver` is not
registered in the current build fails.

## `db migrations plan`

Print pending reviewed migrations for one database.

```sh
axiom db migrations plan --config <FILE> --database <NAME>
```

```sh
axiom db migrations plan --config examples/axiom.yaml --database app
```

It prints pending migration versions and destructive changes that would require
an explicit gate.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Success. |
| `1` | Diagnostics, validation failure, or a refused operation. |
| `2` | Usage error (missing/malformed arguments). |

## Environment and configuration

- `--config` points at an `axiom.yaml` file; relative paths inside it resolve
  against that file's directory.
- `${VAR}` and `${VAR:-default}` are expanded explicitly by Axiom, not by the
  YAML parser.
- Secrets resolved from the environment are redacted from diagnostics, traces,
  and artifacts.
- The CLI does not contact model providers. Agent and provider behavior is
  exercised through the runtime library with the offline mock provider.

## Not implemented yet

These are tracked in the [implementation ledger](implementation-ledger.md):

- `axiom test`, `axiom dev`, `axiom status`, `axiom inspect`, `axiom resume`,
  `axiom cancel`
- LSP diagnostics/completion/hover/definition
- HTTP server and browser API stubs
- Function-level WASM codegen and DOM rendering
- Published release binaries
