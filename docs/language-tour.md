---
title: Language tour
description: A guided tour of Axiom types, declarations, expressions, control flow, Option/Result, effects, and modules.
---

# Language tour

Axiom is deliberately small. There are eight declaration forms — `module`,
`import`, `export`, `type`, `enum`, `fn`, `agent`, and `workspace` — plus the
`api`/`async` function modifiers and an `effects` clause. Everything else is a
typed library call or configuration.

The normative grammar is [grammar.ebnf](grammar.ebnf). The full semantics are in
the [language specification](language-spec.md). This page is the practical tour;
the runnable source below is
[`examples/src/deterministic.ax`](https://github.com/jvdhtm-lab/axiom/blob/master/examples/src/deterministic.ax).

## A complete module

```axiom
module examples.deterministic;

// Enums are finite tagged sets.
export enum Sign { Negative; Zero; Positive }

// Records use named fields.
export type Point = { x: int; y: int; };

export fn add(a: int, b: int) -> int {
  return a + b;
}

export fn classify(n: int) -> Sign {
  if n > 0 {
    return Sign.Positive;
  }
  if n < 0 {
    return Sign.Negative;
  }
  return Sign.Zero;
}

export fn describe(n: int) -> string {
  match classify(n) {
    Sign.Positive => {
      return "positive";
    }
    Sign.Negative => {
      return "negative";
    }
    Sign.Zero => {
      return "zero";
    }
  }
}

export fn main() -> string {
  let p = { x: add(2, 3), y: 0 };
  return describe(p.x) + ": " + p.x.to_string();
}
```

Check and run it:

```sh
axiom check examples/src/deterministic.ax
axiom run examples/src/deterministic.ax
```

Expected:

```text
"positive: 5"
```

## Modules, imports, and exports

Every file starts with one module declaration:

```axiom
module example.dashboard;
```

Imports are explicit, acyclic, and use source-relative paths:

```axiom
import { DashboardInput, AppError } from "./shared.ax";
```

Only `export`ed declarations are visible to other modules. Nothing runs at import
time. Module paths are dotted (`examples.dashboard`), while import paths are
strings (`"./shared.ax"`).

## Types

Primitive types:

`string`, `bool`, `int`, `float`, `bytes`, `duration`, `timestamp`.

Composite and built-in types:

| Form | Meaning |
| --- | --- |
| `T[]` | list of `T` |
| `{ name: T; ... }` | record type |
| `Foo<A, B>` | generic reference |
| `Option<T>` | `Some(value)` or `None` |
| `Result<T, E>` | `Ok(value)` or `Err(error)` |
| `Map<K, V>` | key/value map |

Records use named fields:

```axiom
export type DashboardInput = { team_id: string; };

export type DashboardState = {
  team_id: string;
  completed: int;
  range: TimeRange;
  loading: bool;
};
```

Enums are finite tagged values. Prefer them over string flags:

```axiom
export enum TimeRange { Week; Month; Quarter; }

export enum AppError { NotFound; DatabaseUnavailable; ValidationFailed; Unauthorized; }
```

`Option<T>` provides `unwrap_or(default)`. Enums provide `to_string()`.

There is no tuple syntax, no implicit `any`, and no implicit conversion from
unvalidated JSON or model output.

## Functions and effects

Functions are pure unless they declare effects:

```axiom
export fn add(a: int, b: int) -> int {
  return a + b;
}
```

A function that may await asynchronous work must be `async`; `await` is required
at the call site:

```axiom
export async fn handle_event(
  state: DashboardState,
  event: DashboardEvent
) -> Result<DashboardState, AppError>
  effects [api.call] {
  match await get_dashboard({ team_id: state.team_id }) {
    Ok(summary) => return Ok({
      team_id: state.team_id,
      completed: summary.completed,
      range: state.range,
      loading: false
    });
    Err(error) => return Err(error);
  }
}
```

The checker enforces a subset rule: a call may not use effects the caller did not
declare, and `await` is only allowed in `async` functions. The effect families
are a fixed, small vocabulary:

`model`, `fs.read`, `fs.write`, `fs.delete`, `workspace.create`,
`artifact.read`, `artifact.publish`, `command.run`, `db.read`, `db.write`,
`db.schema`, `db.provision`, `api.call`, `browser.storage`,
`browser.navigation`, `runtime.checkpoint`.

## Statements and control flow

Statements are immutable bindings, `if`/`else`, `match`, finite `for`, `return`,
and expression statements.

```axiom
export fn first(xs: string[]) -> string {
  for x in xs {
    return x;
  }
  return "";
}
```

```axiom
export fn label(n: int) -> string {
  if n == 0 {
    return "zero";
  } else if n > 0 {
    return "positive";
  } else {
    return "negative";
  }
}
```

There is no `while` loop and no general reassignment. State changes return a new
value. Repetition, retries, agent repair, and parallel work use bounded
standard-library APIs with run-wide limits.

Use an exhaustive `match` for enums and `Option`/`Result` when every case must be
handled:

```axiom
match result {
  Ok(summary) => return Ok(summary);
  Err(_) => return Err(AppError.DatabaseUnavailable);
}
```

Match patterns can be `_`, literals, grouped patterns, bindings, dotted enum
cases such as `DashboardEventKind.Refresh`, and constructor patterns such as
`Ok(summary)` and `Err(_)`.

## Expressions

Supported expressions include literals, list and record construction,
member/index access, function calls with named arguments, named generic calls,
arithmetic/comparison/boolean operators, `if`/`else`, `match`, and `await`.

```axiom
let p = { x: add(2, 3), y: 0 };
let xs = ["a", "b"];
let head = xs[0];
let text = describe(p.x) + ": " + p.x.to_string();
```

Operators follow the usual precedence, tightest first:

```text
postfix (call, member, index)
unary   ! -
* / %
+ -
< <= > >=
== !=
&&
||
```

Literals include integers, floats, durations (`120s`, `250ms`), sizes (`2MB`,
`1GiB`), strings with `\" \\ \n \r \t`, and booleans.

## Option and Result

`Option<T>` is `Some(value)` or `None`:

```axiom
export fn maybe(flag: bool) -> Option<int> {
  if flag {
    return Some(42);
  }
  return None;
}
```

`Result<T, E>` is `Ok(value)` or `Err(error)`. It is the normal way to report
recoverable failure; Axiom has no `throw`/`try`/`catch` in 1.0.

```axiom
export api async fn get_dashboard(
  input: DashboardInput
) -> Result<DashboardSummary, AppError>
  effects [db.read] {
  match await db.query_one<DashboardSummary>(
    database: "app",
    sql: "SELECT team_id, completed FROM v_team_performance
          WHERE team_id = :team_id LIMIT 1",
    params: { team_id: input.team_id },
    timeout: 5s
  ) {
    Ok(summary) => return Ok(summary);
    Err(_) => return Err(AppError.DatabaseUnavailable);
  }
}
```

## Comments

```axiom
// Line comment.

/* Block comment.
   Block comments do not nest. */
```

## Diagnostics

`axiom check` and `axiom lint` parse and check source and report stable codes with
source spans:

```sh
axiom check examples/src/deterministic.ax
```

Parsing never panics on malformed input; it recovers and reports multiple
diagnostics where possible. See the [language specification](language-spec.md#11-diagnostics)
and [CLI reference](cli.md#check).

## Target compatibility

The same checked IR feeds both targets:

- `server` may use all effect families.
- `wasm_browser` may use `api.call`, `browser.storage`, and
  `browser.navigation` only.

A server-only effect in a browser build fails at check/build time; it is never
silently skipped. This is why the reference frontend cannot be built for
`wasm_browser` while it imports a handler whose body uses `db.read`.

## What the checker does not do yet

The implementation is phased. General expression type inference, `match`
exhaustiveness proofs, and some cross-module typing are still incomplete. Do not
assume a construct is fully verified because it parses; consult the
[implementation ledger](implementation-ledger.md) for test evidence.

Next: [Examples](examples.md) shows complete server, database, frontend, and
agent programs.
