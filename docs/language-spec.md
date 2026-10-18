# Axiom language specification (Axiom 1.0)

Axiom is a **small, strongly typed language and runtime** for building full-stack
applications in which AI agents operate inside explicit boundaries and isolated
workspaces. One source language and one shared type/effect system compile for two
required targets:

- `server`: trusted backend execution, database access, authorized API operations.
- `wasm_browser`: application frontend code compiled to WebAssembly and run in a
  trusted browser host.

The defining idea: **boundaries define authority; workspaces define where work
happens; agents propose changes; validation determines what is accepted.**

The normative grammar is `docs/grammar.ebnf`. This document is the human reading
of it. Where they disagree, `grammar.ebnf` wins and this file is a bug.

> **Implementation status.** See `docs/implementation-ledger.md` for the exact,
> evidence-backed status of every requirement. Do not assume a feature works
> unless the ledger cites a test.

## 1. Design constraint: keep the language small

Do not create a dedicated declaration for every runtime concept. Prefer a normal
typed function, a built-in library call, a type, or `axiom.yaml` configuration.

The **only** core declaration forms are:

`module`, `import`, `export`, `type`, `enum`, `fn`, `agent`, `workspace`.

`api` is a modifier for a function exported as a server operation. `async` marks
a function that can `await` asynchronous work. Everything else — agent iteration,
validation, caching, persistence, retries, checkpoints, database queries, tools,
artifact publishing, and parallel work — is a standard-library API or a runtime
service, **not** new control-flow or declaration syntax.

There is deliberately **no** source-level `team`, `goal`, `task`, `contract`,
`policy`, `tool`, `query`, `operation`, `workflow`, `state`, `store`,
`transition`, or `migration` declaration. Those concepts are ordinary
types/functions, `agent`/`workspace` declarations, library APIs, or config.

## 2. Source format

- Source files use `.ax` and start with one `module` declaration.
- Imports are explicit and acyclic. No code executes at import time.
- Braces delimit blocks. Statements and member entries end with `;` (a closing
  `}` may serve as the final separator).
- Bindings are immutable: `let name = value;`. General reassignment (`=`, `+=`,
  …) is not supported in 1.0; state changes return a new value.
- `await` is required for asynchronous calls. Async functions declare `async`.
- Functions are pure unless they declare effects, e.g. `effects [api.call]`.
- Diagnostics carry stable error codes and source spans.

## 3. Lexical structure

- **Encoding:** UTF-8. Identifiers are ASCII (`[A-Za-z_][A-Za-z0-9_]*`).
- **Comments:** `// …` line and `/* … */` block (not nested).
- **Strings:** double-quoted, escapes `\" \\ \n \r \t`; an unknown escape is an error.
- **Numbers:** integers (`42`) and floats (`0.2`).
- **Durations:** an integer immediately followed by `ms`, `s`, `m`, or `h`
  (`120s`, `250ms`), stored as milliseconds.
- **Sizes:** an integer followed by `B`, `kB`/`KB`, `MB`, `GB`, `TB`, `KiB`,
  `MiB`, `GiB`, `TiB`, stored as a byte count.
- **Booleans:** `true`, `false`.

## 4. Types

Primitive types: `string`, `bool`, `int`, `float`, `bytes`, `duration`,
`timestamp`.

Composite and built-in types:

| Form | Meaning |
| --- | --- |
| `T[]` | list of `T` |
| `{ name: T; ... }` | record type |
| `Foo<A, B>` | generic reference |
| `Option<T>` | `Some(value)` or `None` |
| `Result<T, E>` | `Ok(value)` or `Err(error)` |
| `Map<K, V>` | key/value map |

Records use named fields. Enums are a finite set of tagged values, declared with
`enum`; use enums instead of arbitrary string flags when the value set is known.
There is no implicit `any`, no implicit conversion from unvalidated JSON/model
output, and no tuple syntax. Local bindings are inferred; exported function
signatures are explicit.

`Option<T>` provides `unwrap_or(default)`; enums provide a stable `to_string()`.
Evaluation order and numeric/string behavior are defined in the semantics
(`docs/architecture.md`).

## 5. Declarations

```ts
module example.dashboard;

import { DashboardInput, AppError } from "./shared.ax";

export type DashboardInput = { team_id: string; };

export enum AppError { NotFound; DatabaseUnavailable; }

export fn render(state: DashboardState) -> UiTree { ... }

export api async fn get_dashboard(input: DashboardInput) -> Result<DashboardSummary, AppError>
  effects [db.read] { ... }

export agent FrontendDesigner(input: FrontendChange) -> FrontendPatch {
  model: "coding";
  workspace: frontend_edit;
  tools: [fs.read, fs.search, fs.write, compiler.check];
  limits: { max_calls: 8; timeout: 120s; max_output_bytes: 1000000; }
}

export workspace frontend_edit {
  root: ".";
  read: ["src/frontend/**", "src/shared/**"];
  write: ["src/frontend/**"];
  deny: ["src/server/**", "axiom.yaml", "db/migrations/**"];
  isolation: copy_on_write;
  limits: { max_files: 200; max_file_bytes: 1000000; max_changed_files: 30; }
}
```

- `type`: record/type alias.
- `enum`: tagged finite set.
- `fn`: normal function; may be `async` and may declare `effects`.
- `api fn`: a function published as a typed server operation. The server executes
  the body; browser compilation sees only a generated typed client stub.
- `agent`: an AI worker with typed input/output, model alias, permitted tools,
  workspace, and limits. Its body is metadata only, never executable logic.
- `workspace`: a bounded read/write area and its isolation policy.

An agent without a workspace has no filesystem access. An agent's declared
workspace is its **maximum** authority; a call-time workspace may narrow it but
never widen it.

## 6. Statements and expressions

Statements: immutable `let` bindings, `if`/`else`, `match`, finite `for … in …`,
`return`, and expression statements. There is no `while`, no unbounded recursion,
no `throw`/`try`/`catch`/`finally`, and no general reassignment. Repetition,
retries, agent repair, and parallel work use **bounded standard-library APIs**
with run-wide limits.

Expressions: literals, list/record construction, member/index access, function
calls (with named arguments), named generic calls (`db.query_one<Row>(…)`),
arithmetic/comparison/boolean operators, `if`/`else`, `match`, and `await`.
Standard-library callbacks take **named function references**, not lambdas.

`match` patterns: `_`, literals, `(`-grouped patterns, bindings, and dotted enum
cases such as `DashboardEventKind.Refresh`, including constructor patterns like
`Ok(summary)` and `Err(_)`.

## 7. Effects

The checker recognizes a small fixed set of effect families:

`model`, `fs.read`, `fs.write`, `fs.delete`, `workspace.create`, `artifact.read`,
`artifact.publish`, `command.run`, `db.read`, `db.write`, `db.schema`,
`db.provision`, `api.call`, `browser.storage`, `browser.navigation`,
`runtime.checkpoint`.

A function declares the effects it may perform. A call must not exceed the
caller's declared effects. UI tree construction and returning new UI state are
pure; the host performs rendering and commits validated state. Runtime hosts
re-check authorization; compiler checks are defense in depth, not the boundary.

The effective authority of a call is the intersection of the function's declared
effects, agent tool grants, workspace permissions, project configuration, the
current principal/policy, and host capabilities. A model response, UI event,
cache hit, or artifact reference cannot widen authority.

## 8. Target compatibility

The two targets share the parser, type checker, module model, contracts, and IR.
An unsupported target feature **fails at compile/check time**; it is never
stubbed or silently skipped.

- `server` may use all effect families.
- `wasm_browser` may use `api.call`, `browser.storage`, and `browser.navigation`;
  it may **not** use `db.*`, `fs.*`, `command.run`, `model`, `workspace.*`,
  `artifact.*`, or `runtime.checkpoint`.

`export api fn` is the only source-level way to publish a server operation.
On `wasm_browser`, importing an API exposes only its checked signature and a
generated client stub with caller-side effect `api.call`; the server handler body
must never be pulled into the browser module.

## 9. Frontend convention

A frontend module exports three ordinary functions, identified by the compiler
manifest entry points:

- `init(input) -> State` — initial typed state.
- `render(state) -> UiTree` — a pure function describing the UI.
- `handle_event(state, event) -> Result<State, AppError>` — computes new state and
  may perform declared API effects.

`UiTree` is a typed value built with UI standard-library functions (`ui.column`,
`ui.row`, `ui.text`, `ui.heading`, `ui.button`, `ui.input`, `ui.metric`,
`ui.table`, …). It is not HTML or executable markup. The browser host performs
actual DOM rendering and validates all nodes, events, and serialized values.

## 10. Configuration

Source code and deployment configuration are separate:

- `*.ax`: types, functions, agents, workspaces, UI, and business logic.
- `axiom.yaml`: provider connections, model aliases, build targets, entry points,
  budgets, registered tools/commands, database connections, browser limits,
  state/cache policy, and retention.
- Build output: immutable compiler-generated artifacts and manifests; never
  hand-edited.

Config limits are hard ceilings. Source declarations may narrow them but never
widen them. `${VAR}` and `${VAR:-default}` expand explicitly; secrets resolve at
runtime and are redacted from logs, diagnostics, traces, cache keys, snapshots,
and agent-visible results.

## 11. Diagnostics

Parsing never panics on malformed input. Every diagnostic carries a stable code
and a byte span, rendered with a line/column pointer and a source excerpt. The
parser performs error recovery, so one file can yield multiple diagnostics in a
single pass.
