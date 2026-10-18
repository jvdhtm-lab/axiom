# Implementation ledger

Tracks Axiom 1.0 requirements against implementation status. **Do not mark an
item complete without test evidence.** Statuses: `done` (implemented + tested),
`partial`, `not started`. Phase numbers follow `agents.md` §15.

Last updated: 2026-10-18 (Phase 0 re-baseline to the small language).

## Phase status

| # | Phase | Status |
| --- | --- | --- |
| 0 | Design: small grammar, core types, effects, IR plan, API modifier, workspace model, target matrix, browser ABI, threat model | in progress |
| 1 | Lexer, parser, AST, source maps, diagnostics, formatter skeleton | not started |
| 2 | Module resolution, types, generics, Option/Result, effect and target checking | not started |
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
| Small declaration surface (`module`/`import`/`export`/`type`/`enum`/`fn`/`agent`/`workspace`, `api`/`async`/`effects`) | 0,1 | not started | — | `docs/grammar.ebnf`, `docs/language-spec.md` | — |
| Forbidden concepts rejected with stable codes | 0,1 | not started | — | `docs/grammar.ebnf` | — |
| Effect vocabulary + caller-subset rule | 0,2 | not started | — | `docs/language-spec.md` §7 | — |
| Target matrix (server vs wasm_browser) | 0,2 | not started | — | `docs/language-spec.md` §8 | — |
| Browser ABI design | 0 | not started | — | `docs/architecture.md` | — |
| Threat model | 0 | partial | — | `docs/security-model.md` | Runtime controls not yet implemented |
| Conformance matrix | 0 | partial | — | `docs/conformance.md` | Cites phases not yet built |

## Truthfulness note

Axiom 1.0 is **not** complete. This repository is being re-baselined from the
previous (large-surface) grammar to the small language defined by the current
`agents.md` and `docs/grammar.ebnf`. Until Phase 1 lands, the parser and semantic
crates still implement the previous grammar; they are actively being replaced.
Nothing should be assumed to work unless the requirement ledger above cites a
passing test.
