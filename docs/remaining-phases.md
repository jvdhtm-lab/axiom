# Remaining phases

This roadmap covers the work still needed after the Phase 12 release audit. It
is ordered by dependency and value, not by an estimate. A phase is complete only
when the implementation ledger cites passing evidence.

Status: `done`, `in progress`, `not started`.

| Phase | Work | Status |
| --- | --- | --- |
| 13 | Type checker completion: conservative expression inference, record/list/enum/`Option`/`Result` typing, user/imported function signatures, match exhaustiveness | done |
| 14 | HTTP API server + typed browser stubs: enforce `ApiGuard` on every protected route and generate the caller-side contract | done (server core + reference route + typed stubs); production server binary, identity provider, TLS/CORS remain hardening |
| 15 | Browser runtime: function-level WASM codegen or IR execution, trusted host shell, DOM renderer, event loop, real headless-browser test, Pages deployment | done (IR host, DOM/events, headless test, typed API stubs, reference frontend build); function-level codegen, worker isolation, and storage/navigation remain hardening |
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

The server core, a reference `get_dashboard` route, and typed browser stubs are
done. The tests cover `ApiGuard` refusals, the real handler over HTTP with a mock
database, and the browser stub calling a mock API. Production server binary,
identity-provider adapter, and CORS/TLS wiring move to hardening.

## Phase 15 — Browser runtime

The IR-interpreting host, UiTree renderer, event loop, real headless test, typed
API stubs, and the reference frontend build are done. The pure demo and the
reference frontend both run in a real browser; the reference test uses a mock API
response. Function-level WASM codegen, worker isolation, and storage/navigation
imports move to hardening.

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
