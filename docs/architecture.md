# Architecture

## Goals

Axiom is a small, strongly typed language **and runtime** for building full-stack
applications in which AI agents work inside explicit boundaries and isolated
workspaces. One source language and one shared type/effect system compile for two
required targets:

- `server` — trusted backend execution, database access, authorized API operations.
- `wasm_browser` — application frontend compiled to WebAssembly and run in a
  trusted browser host.

The defining idea: **boundaries define authority; workspaces define where work
happens; agents propose changes; validation determines what is accepted.**

Non-goals for 1.0: a general-purpose programming language, a second frontend
language (no generated React/TS/JSX), a self-hosted model server, a distributed
scheduler, arbitrary plugin loading, and any hidden source-language behavior
implemented only in the browser host.

## Pipeline (one shared front end)

```
.ax source
  │  lexer                 (axiom-syntax::lexer)
  ▼  tokens (+ byte spans)
  │  parser                (axiom-syntax::parser)
  ▼  AST                   (axiom-syntax::ast)
  │  resolve + check       (axiom-types)
  ▼  module graph + symbols + typed/effect/target diagnostics
  │  lower                 (axiom-ir)
  ▼  typed IR
  │  codegen               (axiom-compiler)
  ├──────────────► server execution  (axiom-runtime)
  └──────────────► wasm_browser      (axiom-wasm + axiom-browser-host)

axiom.yaml
  │  load + ${...} expansion + strict schema   (axiom-config)
  ▼  ProjectConfig (integrations, model aliases, targets, budgets, policies)
```

The two targets share the parser, type checker, module model, contracts, and IR.
Target-specific code generation is allowed; **target-specific language semantics
are not**. An unsupported target feature fails at check time and is never stubbed
or silently skipped.

## Language model

- Declaration forms are exactly `module`, `import`, `export`, `type`, `enum`,
  `fn`, `agent`, `workspace`, with `api`/`async` modifiers and an `effects`
  clause. See `docs/grammar.ebnf`.
- Runtime concepts (iteration, validation, caching, persistence, retries,
  checkpoints, database access, tools, artifact publishing, parallelism) are
  **standard-library APIs or configuration**, not grammar.
- Effects are a small fixed vocabulary (`model`, `fs.*`, `workspace.*`,
  `artifact.*`, `command.run`, `db.*`, `api.call`, `browser.*`,
  `runtime.checkpoint`). A callee's effects must be a subset of the caller's.
- `export api fn` is the only way to publish a server operation; browser
  compilation produces a typed client stub, never the server body.

## Crates

```
crates/
  axiom-syntax/      lexer, parser, AST, spans, diagnostics   (no dependencies)
  axiom-format/      deterministic formatter
  axiom-types/       module graph, symbols, type/effect/target/SQL checks
  axiom-ir/          shared typed IR, lowering, deterministic JSON
  axiom-runtime/     deterministic server evaluator
  axiom-wasm/        browser WASM backend + artifact manifest (Phase 4 partial)
  axiom-browser-host/ trusted host ABI + artifact verification
  axiom-config/      axiom.yaml loading, expansion, validation
  axiom-compiler/    target pipeline and code generation (planned)
  axiom-cli/         the `axiom` binary
browser/             trusted minimal host shell + Node smoke harness
```

Dependency direction stays acyclic: the type system and IR never depend on
provider adapters; providers never depend on the parser; the database crate never
depends on model adapters; the browser host never contains app-specific business
logic.

## Source vs configuration

`.ax` is the portable language; `axiom.yaml` is deployment/environment
configuration (integrations, endpoints, secret references, model aliases,
targets, entrypoints, budgets, database connections, browser-host limits,
state/cache policy, and registered command profiles). Source refers to model
aliases only, never to credentials or endpoints. See `docs/configuration.md` and
ADR-0008.

## Error strategy

- Lexer/parser produce **diagnostics**, never panics, on malformed input.
- Diagnostics carry a stable code and a byte `Span`; the source layer renders a
  line/column pointer and an excerpt.
- The parser recovers (skip to the next declaration/entry), so one file yields
  several diagnostics in one pass.
- A failed provider/tool/check is never converted into apparent success.
- Compile-time errors, runtime-enforced policies, and model-evaluated criteria
  are kept distinct; natural-language constraints are never claimed to be
  statically verified.

## Security boundaries (summary)

Authority is the intersection of a function's declared effects, agent tool
grants, workspace permissions, project configuration, the current
principal/policy, and host capabilities. Model output, UI events, cache hits, and
artifact references cannot widen it. Secrets are referenced by name and never
logged, traced, cached, or embedded in artifacts. See `docs/security-model.md`.
