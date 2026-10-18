# ADR 0014: Re-baseline Axiom 1.0 to the small language

- **Status:** accepted
- **Date:** 2026-10-18

## Context

Earlier revisions of `agents.md` described Axiom with a large declaration
surface: `interface`, `team`, `tool`, `contract`, `goal`, `policy`, `team`,
`workflow`, `operation`, and `query` declarations, a `while` loop, `throw`/
`try`/`catch`/`finally`, `yield`, `require`, `parallel for`, `retry`, `timeout`,
`on_error`, `checkpoint`, `emit`, `delegate`, and `iterate` expressions.

The current `agents.md` deliberately reverses that direction. Section 3 fixes the
core declaration forms to **`module`, `import`, `export`, `type`, `enum`, `fn`,
`agent`, `workspace`** (with `api` and `async` as function modifiers) and states
that agent iteration, validation, caching, persistence, retries, checkpoints,
database queries, tools, artifact publishing, and parallel work are
standard-library APIs or configuration — **not** grammar.

## Decision

1. The language surface is exactly what `docs/grammar.ebnf` defines. No other
   declaration or statement keyword exists.
2. Forbidden concepts are recognized only so they can be rejected with a stable
   diagnostic; they are never parsed into an AST node with meaning.
3. One shared parser, type/effect checker, and typed IR serve both the `server`
   and `wasm_browser` targets. Target-specific language semantics are not allowed.
4. Callbacks use named function references; there are no lambdas in 1.0.

This ADR **supersedes** and replaces the direction of:

- ADR 0005 (five concepts are never collapsed) — re-expressed as ordinary
  types/functions/config, so the dedicated declarations are removed.
- ADR 0010 (re-baseline to Axiom 1.0) — its large-surface reading is withdrawn;
  the 1.0 baseline is now the small language.
- ADR 0012 (semantic analysis scope) and ADR 0013 (strict resolution and
  multi-target) — their machinery is kept, but the `query`/`operation`/`iterate`
  declarations they introduced are removed.

## Consequences

- The Phase 1 parser, AST, `axiom-types` checks, examples, and docs are rewritten
  to the small surface.
- Future runtime concepts must be expressed as typed library APIs or config; a
  new grammar construct requires an ADR showing a normal function, type, library
  API, or manifest setting cannot express it clearly and safely.
