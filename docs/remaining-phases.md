# Remaining phases

This roadmap covers the work still needed after the Phase 12 release audit. It
is ordered by dependency and value, not by an estimate. A phase is complete only
when the implementation ledger cites passing evidence.

Status: `done`, `in progress`, `not started`.

| Phase | Work | Status |
| --- | --- | --- |
| 13 | Type checker completion: conservative expression inference, record/list/enum/`Option`/`Result` typing, user/imported function signatures, match exhaustiveness | done |
| 14 | HTTP API server + typed browser stubs: enforce `ApiGuard` on every protected route and generate the caller-side contract | in progress ? server core and reference route test done; production server binary, identity provider, and typed browser stubs pending |
| 15 | Browser runtime: function-level WASM codegen or IR execution, trusted host shell, DOM renderer, event loop, real headless-browser test, Pages deployment | in progress ? IR-interpreting host, DOM renderer, event loop, headless browser test, and pure-demo Pages path done; function-level codegen, typed API stubs, and reference-frontend deployment pending |
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

The server core and a reference `get_dashboard` route are done. Still pending: a
production server binary, an identity-provider adapter, CORS/TLS integration, and
typed browser stubs with caller-side `api.call`.

## Phase 15 — Browser runtime

The IR-interpreting host, UiTree renderer, event loop, and real headless test are
done, and a pure-demo Pages path works. Still pending: function-level WASM
codegen, typed API stubs, storage/navigation imports, worker isolation, and the
reference frontend build.

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
