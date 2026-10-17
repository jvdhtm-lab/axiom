# Axiom language specification (Axiom 1.0)

Axiom is a TypeScript-inspired, domain-specific language for describing AI
agents. A program is portable and environment-independent: it describes *what*
agents are, what they may do, and how work flows. Deployment details (endpoints,
credentials, model ids) live in `axiom.yaml`, never in `.ax` source.

The **normative** grammar is `docs/grammar.ebnf`. This document is the human
reading of it. Where they disagree, `grammar.ebnf` wins and this file is a bug.

> **Implementation status.** The Axiom 1.0 surface grammar is implemented by
> `crates/axiom-syntax` (lexer, parser, AST, spans, diagnostics): modules,
> imports, exports; interfaces/types/enums; agents, tools, goals, teams,
> contracts, policies, workspaces, workflows; functions; statements
> (`let`/`let mut`/`const`, `if`, `match`, `for`, `while … max`, `return`,
> `yield`, `break`, `continue`, `throw`, `try/catch/finally`, `require … else
> escalate`, `parallel for … [max]`, `retry`, `timeout`, `on_error`,
> `checkpoint`, `emit`); and the full expression/type grammars. Every example in
> `agents.md` §11 and §5.7 parses cleanly. **Partial semantic analysis** exists
> in `crates/axiom-types` (module graph, symbols, declaration/aggregate/effect
> checks, source/config cross-validation); full type inference, expression
> typing, `match` exhaustiveness, and effect propagation are not implemented.
> There is no IR or execution yet (Phases 3+). See
> `docs/implementation-ledger.md` for phase-by-phase truth.

## Program structure

```ts
module reviewer.agents;

import { FeatureSpec, Review } from "./types.ax";

export type AgentId = string;
export type Verdict = "pass" | "fail" | "escalate";

export interface FeatureSpec {
  title: string;
  criteria: string[];
}

export agent Reviewer implements Agent<FeatureSpec, Review> {
  purpose: "Review a feature spec against its acceptance criteria.";
  model: "reasoning";
  tools: [workspace.read_file, workspace.write_file];
  workspace: ProjectReadOnly;
  constraints {
    must: ["cite every criterion", "explain each failure"];
    must_not: ["modify the project"];
  }
}
```

A file is a `module` header, an optional run of `import` statements, and a run of
top-level declarations, each optionally prefixed with `export`. **Statements and
member entries end with `;`; newlines are insignificant whitespace.** A closing
`}` may act as the final separator of a block, so the last member need not be
followed by `;`.

## Lexical structure

- **Encoding:** UTF-8. Identifiers are ASCII (`[A-Za-z_][A-Za-z0-9_]*`).
- **Line comments:** `// ...` to end of line.
- **Block comments:** `/* ... */` (not nested).
- **Strings:** double-quoted, with escapes `\" \\ \n \r \t`. An unknown escape is
  a diagnostic.
- **Numbers:** `123` (int) or `0.2` (float).
- **Durations:** an integer immediately followed by `ms`, `s`, `m`, or `h`
  (e.g. `120s`, `30s`). Stored internally as milliseconds.
- **Sizes:** an integer immediately followed by `B`, `kB`/`KB`, `MB`, `GB`, `TB`
  (decimal) or `KiB`, `MiB`, `GiB`, `TiB` (binary) — e.g. `2MB` = 10^6 bytes,
  `2MiB` = 2^20 bytes. Stored internally as a byte count.
- **Booleans / null:** `true`, `false`, `null` (lexed as identifiers and
  recognised by the parser unless immediately followed by `(` or `.`).
- **Terminator:** `;`.

## Types

Lower-case primitives: `string`, `bool`, `int`, `float`, `bytes`, `duration`,
`timestamp`, `null`. Built-ins include `Option<T>`, `Result<T, E>`, `Map<K, V>`,
`JsonValue`, and the error/enum types named in `agents.md`. Composite forms:

| Form | Meaning |
| --- | --- |
| `T[]` | array of `T` (normative; `[T]` is a legacy prefix form) |
| `A \| B` | union |
| `(A, B)` | tuple |
| `{ name: T; ... }` | record / object type |
| `"a" \| "b"` | string-literal union |
| `ident` / `pkg.Name` | a named type reference |
| `Foo<A, B>` | generic reference, e.g. `Agent<I, O>` |

`export interface` and `export type` both introduce named types. An interface
lists fields; a `type` alias introduces a name for any type expression.

## Declarations

Declarations are `type interface enum const fn agent team tool contract goal
policy workspace workflow`, each optionally prefixed with `export`.

- **Agents** (`agent X implements Agent<I, O> { ... }`) are member blocks with
  `purpose`, `model`, `tools`, optional `workspace`, `constraints { must; must_not; }`,
  and limits.
- **Tools** (`tool f(p: T) -> R effects [fs.read] { ... }`) declare a signature,
  effects, and a metadata member block (`timeout`, `idempotent`,
  `max_output_bytes`, `binding`). A tool declaration never contains executable
  code.
- **Goals** (`goal G(x: T) -> R { ... }`) are a signature plus a member block
  (`objective`, `completion`, `decomposition`, `authority`, `workflow`).
- **Teams** (`team T { members: [...]; routing { Input -> Member; } policy: P; }`)
  map input types to members.
- **Contracts** (`contract C { version: "..."; subject: T; postcondition label:
  expr; criterion "text": CheckKind.X; }`) carry labelled clauses.
- **Policies** and **workspaces** are member blocks. A workspace has exactly one
  of `root`/`base`, an `isolation`, `read`/`write`/`deny` globs, an optional
  nested `limits { ... }`, and optional `write_authority`.

`purpose`, `model`, `tools`, and `workspace` on an agent are ordinary fields. The
`constraints` block holds `must` / `must_not` natural-language lists.
Natural-language constraints are **model-evaluated**, not statically verified;
the language makes no claim otherwise.

## Expressions and statements

Expressions use the usual precedence tiers: postfix (calls, member access
`.name`, indexing), unary, multiplicative, additive, comparison, equality, `&&`,
`||`, then the conditional `? :`. `await`, `delegate Agent.method(...)`, and
`parallel for x in xs [max N] { ... yield v; }` are expressions. A lambda is
either `(x: T, y: U) => body`, `(x) => body`, or the single-parameter `x => body`.
`Agent.with { model: "..." }` is sugar for a call with an object argument.

Statements include `let`/`let mut`/`const` bindings, `if`/`else`, `match`,
`for`, `while cond max N`, `return`, `yield`, `break`, `continue`, `throw`,
`try/catch/finally`, `require expr;` / `require expr else escalate expr;`,
`parallel for`, `retry N { }`, `timeout D { }`, `on_error(e: T) { }`,
`checkpoint "label";`, and `emit expr;`. See `grammar.ebnf` for exact
productions.

`env(...)` and `secret(...)` are ordinary calls in source. A secret argument is a
*name*, never a value: secrets are never written into compiled plans, traces, or
error messages.

## Configuration precedence

```
per-invocation override > agent-instance override > agent declaration
  > project default > runtime default
```

Model ids, endpoints, and limits all live in `axiom.yaml`; `model: "reasoning"`
in source names a *model profile*, resolved by the runtime.

## Diagnostics

Parsing never panics on malformed input. Every diagnostic carries a byte span and
is rendered with a line/column pointer and a source excerpt. The parser performs
error recovery, so a file can yield multiple diagnostics in one pass.

## What is *not* here yet

- Full type inference, expression typing, generics checking beyond built-in
  arity, and `match` exhaustiveness (Phase 2, in progress).
- Inter-procedural effect propagation and capability intersection (Phase 2/4).
- No IR, no `explain`/`plan`, no execution (Phase 3+).
- No runtime, filesystem, artifacts, or provider adapters (Phases 4+).
