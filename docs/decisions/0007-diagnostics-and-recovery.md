# ADR-0007: Spanned diagnostics with error recovery

**Status:** accepted (Phase 0)

## Context

A compiler that panics or stops at the first mistake is unusable in practice and
dangerous under fuzzing. Errors must be precise and human-readable.

## Decision

- Every token, AST node, and diagnostic carries a byte `Span`. The source layer
  converts a span to line/column and renders an excerpt with a pointer.
- The lexer emits diagnostics for invalid characters and bad escapes and keeps
  going.
- The parser performs **error recovery**: at the top level it skips to the next
  plausible declaration; inside bodies it skips to the next separator. A single
  malformed file yields several diagnostics in one pass.
- The parser must **never panic** on user input; this is covered by tests over
  malformed and randomly generated snippets.
- Semantic (Phase 2) diagnostics reuse the same span/severity machinery.

## Consequences

`axiom check` is useful early and stays useful as phases add checks. The AST may
occasionally contain partially recovered nodes; semantic analysis (Phase 2) must
tolerate that and re-report rather than assume a perfect tree.
