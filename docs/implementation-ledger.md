# Implementation ledger

Tracks Axiom 1.0 requirements against implementation status. **Do not mark an
item complete without test evidence.** Statuses: `done` (implemented + tested),
`partial`, `not started`. Phase numbers follow `agents.md` §15.

Last updated: 2026-10-18 (Phase 1: small-language front end + formatter).

## Phase status

| # | Phase | Status |
| --- | --- | --- |
| 0 | Design: small grammar, core types, effects, IR plan, API modifier, workspace model, target matrix, browser ABI, threat model | done — `docs/grammar.ebnf`, `docs/language-spec.md`, `docs/conformance.md`, `docs/security-model.md`, ADR-0014 |
| 1 | Lexer, parser, AST, source maps, diagnostics, formatter skeleton | done — `axiom-syntax` + `axiom-format`; positive/negative fixtures |
| 2 | Module resolution, types, generics, Option/Result, effect and target checking | partial — module graph, symbols, duplicates, effect-name and member checks done; inference/effect-propagation/target checks pending |
| 3 | Typed IR and deterministic server evaluator; basic compiler commands | not started |
| 4 | WASM lowering, UI tree, host ABI, browser loader/renderer | not started |
| 5 | Config, model integrations, registered tools and commands | not started |
| 6 | Filesystem, workspaces, patch/artifact APIs and command sandbox | not started |
| 7 | Database APIs, roles, parameter checks, approval/audit and migrations | not started |
| 8 | Agent calls, iteration/validation APIs, budgets, typed results | not started |
| 9 | Frontend state, state validation/migration, artifact registry and build/data caches | not started |
| 10 | Durable async execution, checkpoints, resume, cancellation, tracing, approvals | not started |
| 11 | Full-stack reference app, tooling, docs, security hardening | not started |
| 12 | Release-readiness audit | not started |

## Requirement ledger

| Requirement | Phase | Status | Test evidence | Documentation | Known limitations |
| --- | --- | --- | --- | --- | --- |
| Small declaration surface (`module`/`import`/`export`/`type`/`enum`/`fn`/`agent`/`workspace`, `api`/`async`/`effects`) | 0,1 | done | `parse_valid.rs` (10), `examples` parse/check | `docs/grammar.ebnf`, `docs/language-spec.md` | — |
| Forbidden concepts rejected with stable codes | 1 | done | `parse_invalid.rs`: `AX0010` (decls), `AX0011` (stmts), `AX0012` (assign) | `docs/grammar.ebnf` | — |
| Statements/expressions (`let`, `if`/`else`, `match`, `for`, `return`, await, named args, generic calls) | 1 | done | `parse_valid.rs`: control flow, match, generic call | `docs/language-spec.md` §6 | — |
| Source spans + stable diagnostic codes | 1 | done | `axiom-syntax` lexer/parser tests; `render` includes code | `docs/language-spec.md` §11 | Config value spans still approximate |
| Parser never panics on malformed/fuzz input | 1 | done | `parser_never_panics_on_token_soup`, `parse_invalid.rs` | ADR-0007 | Property/fuzz targets not set up |
| Deterministic formatter | 1 | done | `axiom-format` tests (idempotent, re-parses, normalizes) | `docs/architecture.md` | Skeleton: canonical layout only |
| Module graph: import resolution, cycles, unexported/unknown names | 2 | done | `axiom-types/tests/semantics.rs` (`AX0201`–`AX0204`) | `docs/architecture.md` | Lexical path normalization; no `../` escape guard yet |
| Symbol tables + duplicate detection | 2 | done | `duplicate_declaration_is_reported` (`AX0210`) | `docs/architecture.md` | One namespace per module |
| Effect vocabulary (closed set) | 2 | partial | `unknown_effect_is_reported` (`AX0216`) | `docs/language-spec.md` §7 | No propagation or subset rule yet |
| Agent/workspace member shape + isolation modes | 2 | partial | `agent_requires_model` (`AX0211`), `unknown_isolation_mode_is_reported` (`AX0214`) | `docs/language-spec.md` §5 | `tools`/`limits` shape only |
| Config cross-validation (model aliases) | 2,5 | partial | `unknown_model_alias_is_reported_when_configured` (`AX0217`) | `docs/configuration.md` | `axiom.yaml` schema itself is Phase 5 |
| Static type system, inference, match exhaustiveness | 2 | not started | — | `docs/language-spec.md` §4 | — |
| Effect propagation + capability intersection | 2 | not started | — | `docs/security-model.md` | — |
| Target compatibility (server vs wasm_browser) | 2 | not started | — | `docs/language-spec.md` §8 | — |
| SQL literal + placeholder policy | 2,7 | not started | — | `docs/language-spec.md` §7 | — |
| Typed IR + deterministic server evaluator | 3 | not started | — | — | — |
| WASM backend + host ABI + browser host | 4 | not started | — | `docs/architecture.md` | — |
| Config (`axiom.yaml`), providers, tools | 5 | not started | — | `docs/configuration.md`, `docs/providers.md` | Config crate still parses the previous schema |
| Filesystem/workspaces/artifacts | 6 | not started | — | `docs/security-model.md` | — |
| Database subsystem | 7 | not started | — | `docs/language-spec.md` §7 | — |
| Agents/validation/budgets | 8 | not started | — | — | — |
| Frontend state/caches | 9 | not started | — | `docs/language-spec.md` §9 | — |
| Durable execution | 10 | not started | — | — | — |
| Reference app | 11 | partial | `reference_examples_parse_cleanly`, `reference_examples_have_no_semantic_errors` | `examples/src/*.ax` | Parses/checks; not executed |
| Tooling (CLI `fmt`/`lint`/`lsp`) | 11 | partial | `axiom check`, `axiom config check` | `docs/development.md` | Formatter not yet wired to the CLI |

## Truthfulness note

Axiom 1.0 is **not** complete. The repository implements the small-language front
end (lexer, parser, AST, spans, diagnostics), a deterministic formatter, the
module graph with symbol tables, and declaration-level checks. It does **not**
type-check expressions, propagate effects, enforce target compatibility, emit IR,
execute programs, compile WASM, or provide the runtime, database, filesystem,
artifact, or browser subsystems. Nothing should be assumed to work unless the
requirement ledger above cites a passing test.
