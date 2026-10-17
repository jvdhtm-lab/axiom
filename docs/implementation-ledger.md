# Implementation ledger

Tracks Axiom 1.0 requirements against implementation status. **Do not mark an
item complete without test evidence.** Statuses: `done` (implemented + tested),
`partial`, `not started`.

Last updated: 2026-10-18 (after re-baselining to the Axiom 1.0 specification).

## Phase status

| Phase | Scope | Status |
| --- | --- | --- |
| 0 | Reconnaissance, spec, grammar/EBNF, architecture, ADRs | partial — spec/grammar/ADRs drafted; conformance matrix incomplete |
| 1 | Lexer, parser, AST, source maps, diagnostics (all 1.0 syntax) | partial — a pre-1.0 grammar is implemented; re-baseline to the 1.0 grammar is the next increment |
| 2 | Modules, types, symbols, effects | not started |
| 3 | Deterministic evaluator, typed IR, explain/plan | not started |
| 4 | Configuration + model integration layer/adapter registry | partial — `axiom.yaml` schema/validation only |
| 5 | Tools, secure filesystem, workspaces, artifacts, commands | not started |
| 6 | Agent runtime, teams, goals, contracts, invocation | not started |
| 7 | Hybrid workflow engine | not started |
| 8 | Durable state, recovery, observability | not started |
| 9 | Standard library (esp. filesystem example §8a) | not started |
| 10 | CLI, formatter, linter, LSP | partial — `check` and `config check` only |
| 11 | Conformance, security, fuzzing, performance | not started |
| 12 | Documentation, examples, packaging, compatibility | partial — core docs only |
| 13 | Release candidate / 1.0 sign-off | not started |

## Requirement ledger

| Requirement | Phase | Status | Test evidence | Documentation | Known limitations |
| --- | --- | --- | --- | --- | --- |
| `.ax` lexer with spans (pre-1.0 grammar) | 1 | done | `crates/axiom-syntax`: 10 lexer unit tests | `docs/language-spec.md` | Grammar predates 1.0 (no `module`/`import`/`export`, lowercase primitives) |
| `.ax` parser + AST + recovery (pre-1.0 grammar) | 1 | done | `parse_valid`/`parse_invalid` (15 tests) | `docs/language-spec.md` | Same; missing many 1.0 productions |
| No panic on malformed/fuzz input | 1 | done | `parser_never_panics_on_token_soup` | `docs/decisions/0007` | Property/fuzz targets not set up |
| `axiom check` CLI | 1,10 | partial | CLI smoke tests | `README.md` | Single command; no machine-readable output yet |
| `axiom.yaml` load + `${...}` expansion + validation | 4 | done | `crates/axiom-config` (22 tests) | `docs/configuration.md` | Value diagnostics use approximate spans |
| YAML keys match 1.0 spec (`integrations`/`integration`/`timeout_seconds`) | 4 | done | config integration tests | `docs/configuration.md` | — |
| `filesystem:` config section parsing/validation | 4 | partial | config tests | `docs/configuration.md` | Values parsed but not yet enforced (Phase 5) |
| `commands:` allowlisted command profiles (schema) | 4,5 | partial | config tests | `docs/configuration.md` | Parsed/validated only; no execution yet (Phase 5) |
| Modules, imports, exports, multi-file projects | 1,2 | not started | — | `docs/grammar.ebnf` | Pre-1.0 grammar only |
| Static type system (primitives, generics, unions, Option/Result, inference) | 2 | not started | — | `docs/language-spec.md` | — |
| Effects/capabilities static checking | 2,4 | not started | — | `docs/architecture.md` | — |
| Typed IR + `explain`/`plan`/dry-run | 3 | not started | — | — | — |
| Adapter registry + native/generic/mock adapters | 4 | not started | — | `docs/providers.md` | Config schema only; no HTTP/adapters |
| Typed tools + command profiles (enforced) | 5 | not started | — | — | — |
| Filesystem/workspace/artifact subsystem (§10.7) | 5,9 | not started | — | — | — |
| End-to-end filesystem example (§8a) | 9 | not started | — | — | Must become a compiling integration test |
| Agent runtime, teams, goals, contracts | 6 | not started | — | — | — |
| Hybrid workflow engine | 7 | not started | — | — | — |
| Durable state, checkpoints, resume | 8 | not started | — | — | — |
| Standard library modules (§10.10) | 9 | not started | — | — | — |
| Formatter, linter, LSP | 10 | not started | — | — | — |
| Conformance/security/fuzz/property suites | 11 | not started | — | — | — |
| Full docs set (§10.14) | 12 | partial | — | `docs/` | Several required docs missing |
| Release-readiness report | 13 | not started | — | — | — |

## Truthfulness note

Axiom 1.0 is **not** complete. The current repository is a re-baselined Phase 0/1
+ partial Phase 4: a working `.ax` front end for a *pre-1.0* grammar and a
validated `axiom.yaml` loader. The next increment is reworking the lexer, parser,
and AST to the full 1.0 grammar in `docs/grammar.ebnf`. Nothing outside
`docs/language-spec.md`, `docs/configuration.md`, and the CLI/test evidence above
should be assumed to work.
