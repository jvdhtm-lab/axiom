# Remaining phases

This roadmap covers the work still needed after the Phase 12 release audit. It
is ordered by dependency and value, not by an estimate. A phase is complete only
when the implementation ledger cites passing evidence.

Status: `done`, `in progress`, `not started`.

| Phase | Work | Status |
| --- | --- | --- |
| 13 | Type checker completion: conservative expression inference, record/list/enum/`Option`/`Result` typing, user/imported function signatures, match exhaustiveness | done |
| 14 | HTTP API server + typed browser stubs: enforce `ApiGuard` on every protected route and generate the caller-side contract | not started |
| 15 | Browser runtime: function-level WASM codegen or IR execution, trusted host shell, DOM renderer, event loop, real headless-browser test, Pages deployment | not started |
| 16 | Providers and persistence: a networked provider adapter, persistent state/release/cache stores, durable journal, trace export, timeout scheduling | not started |
| 17 | Tooling: LSP diagnostics/completion/hover/definition, `axiom test`, `dev`, `status`, `inspect`, `resume`, `cancel` | not started |
| 18 | Hardening: coverage-guided fuzzing, concurrency stress, license files, clean-checkout release audit | not started |

## Phase 13 — Type checker completion

Implemented:

- A conservative `Ty` model for primitives, lists, records, named types,
  `Option`, and `Result`.
- Type resolution across local and imported declarations, including records,
  enums, and function signatures.
- Expression inference for literals, lists, records, fields, indexing, calls,
  unary/binary operators, `await`, `if`, `match`, and blocks.
- Argument count/name/type checks for user functions.
- `Some`/`Ok`/`Err` constructor typing and `Option.unwrap_or`, `to_string`, and
  `Result` payload binding.
- Match exhaustiveness for user enums, `Option`, `Result`, and `bool`; duplicate
  arms, unknown variants, and payload-bearing enum arms are rejected.
- Stable diagnostics `AX0401`–`AX0412`.

Conservative boundaries (documented rather than guessed):

- Unknown standard-library members become `Ty::Unknown` and do not cascade.
- Generic type parameters are not substituted through user-defined generic
  records; they remain opaque named types.
- The checker does not attempt structural subtyping.

Evidence: `crates/axiom-types/tests/semantics.rs` (25 tests), full
`cargo test --workspace`, `cargo clippy -p axiom-types --all-targets
--all-features -- -D warnings`.

## Phase 14 — HTTP API server + typed browser stubs

Next. The server must call `ApiGuard` on every protected route, and the browser
build must emit only a typed stub with caller-side `api.call`. The reference
`get_dashboard` flow should run over HTTP rather than a mock runtime boundary.

## Phase 15 — Browser runtime

Depends on Phase 14 for real API calls. The compiler emits a verifiable WASM
artifact today, but it embeds IR and has no DOM renderer. This phase must produce
a running UI, a real headless-browser test, and a Pages deployment that satisfies
`docs/github-pages.md`.

## Phase 16 — Providers and persistence

Add at least one networked provider adapter with gated tests, and move state,
release registry, caches, durable journals, and traces out of process memory.
Do not claim exactly-once side effects.

## Phase 17 — Tooling

Add the LSP and the missing CLI lifecycle commands. Diagnostics must stay stable
and machine-readable.

## Phase 18 — Hardening

Add a coverage-guided fuzz target, concurrency stress tests, license files, and
re-run the release audit with clean-checkout evidence.
