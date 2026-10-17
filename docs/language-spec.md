# Axiom language specification (Axiom 1.0)

Axiom is a TypeScript-inspired, domain-specific language for describing AI
agents. A program is portable and environment-independent: it describes *what*
agents are, what they may do, and how work flows. Deployment details (endpoints,
credentials, model ids) live in `axiom.yaml`, never in `.ax` source.

The **normative** grammar is `docs/grammar.ebnf`. This document is the human
reading of it. Where they disagree, `grammar.ebnf` wins and this file is a bug.

> **Implementation status.** The Axiom 1.0 grammar below is the target. The
> current front end (`crates/axiom-syntax`) implements only a small, pre-1.0
> subset: declarations without `module`/`import`/`export`, upper-case primitive
> spellings, and blocks separated by newlines. Reworking the lexer, parser, and
> AST to this full grammar is the next increment. See
> `docs/implementation-ledger.md` for phase-by-phase truth.

## Program structure

```ts
module reviewer.agents;

import { FeatureSpec, Review } from "review/types";
import { read_file } from "workspace/fs";

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
top-level declarations, each optionally prefixed with `export`. Statements are
terminated by `;` or a newline.

## Lexical structure

- **Encoding:** UTF-8. Identifiers are ASCII (`[A-Za-z_][A-Za-z0-9_]*`).
- **Line comments:** `// ...` to end of line.
- **Block comments:** `/* ... */` (not nested). A block comment containing a line
  break acts as a line break.
- **Strings:** double-quoted, with escapes `\" \\ \n \r \t`. An unknown escape is
  a diagnostic.
- **Numbers:** `123` (int) or `0.2` (float).
- **Durations:** an integer immediately followed by `ms`, `s`, `m`, or `h`
  (e.g. `120s`). Stored internally as milliseconds.
- **Booleans:** `true` and `false`.
- **Terminator:** `;` or a newline.

## Types

The primitive type names are lower-case: `string`, `bool`, `int`, `float`,
`duration`. Composite forms:

| Form | Meaning |
| --- | --- |
| `T[]` | array of `T` |
| `A \| B` | union |
| `{ name: T; ... }` | record / object type |
| `ident` / `pkg.Name` | a named type reference |
| `Foo<A, B>` | generic reference, e.g. `Agent<I, O>` |

`export interface` and `export type` both introduce named types. An interface
lists fields; a `type` alias introduces a name for any type expression
(including unions and records).

## Agents

```ts
export agent X implements Agent<Input, Output> {
  purpose: "…";
  model: "reasoning";
  tools: [a.b, c.d];
  workspace: ProjectReadOnly;
  constraints { must: [...]; must_not: [...] }
}
```

`purpose`, `model`, `tools`, and `workspace` are ordinary fields. The
`constraints` block holds `must` / `must_not` natural-language lists. Natural-
language constraints are **model-evaluated**, not statically verified; the
language makes no claim otherwise.

## Expressions and statements

Expressions use the usual precedence tiers: calls, member access (`.name`),
unary, multiplicative, additive, comparison, equality, `&&`, `||`. Statements
include `let` bindings, `return`, `require`, `parallel` blocks, `if`/`else`, and
expression statements. See `grammar.ebnf` for exact productions.

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

- The 1.0 grammar above is not yet implemented; the current parser handles a
  pre-1.0 subset (see the implementation-status note).
- No semantic analysis, symbol resolution, or type checking (Phase 2).
- No IR, no `explain`/`plan`, no execution (Phase 3+).
