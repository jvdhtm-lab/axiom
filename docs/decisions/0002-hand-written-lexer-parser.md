# ADR-0002: Hand-written lexer and recursive-descent parser

**Status:** accepted (Phase 0)

## Context

The language is small (`integration`, `model`, `agent`, `type`, `contract`,
`goal`, `tool`, `runtime`, `workflow`, instances, and a modest expression
grammar) and its bodies are line-break sensitive. Candidate parser libraries
(`chumsky`, `logos` + parser, `lalrpop`, `pest`) each impose an API, a version
compatibility surface, and build-time cost. The project must also minimize
dependencies and avoid the Application Control content-block risk (see ADR-0001).

## Decision

Implement the lexer and a **hand-written recursive-descent parser** with no
third-party dependencies in `axiom-syntax`. The grammar is explicit in
`docs/language-spec.md`, so it is easy to keep parser and spec in sync, and the
hand-written parser gives full control over spans, diagnostics, and error
recovery — including the line-break sensitivity that is awkward in most
combinator/lexer generators.

## Consequences

- Zero dependencies in the front end: fast, reproducible, CI-trivial builds.
- Full control of spans and recovery; multiple diagnostics per file.
- Some maintenance cost, and fuzz-style robustness must be tested explicitly
  (the parser must not panic on malformed input).
- A later phase may revisit this if the grammar grows large; that would be a new
  superseding ADR, not an edit.
