# Implementation ledger

Tracks Axiom 1.0 requirements against implementation status. **Do not mark an
item complete without test evidence.** Statuses: `done` (implemented + tested),
`partial`, `not started`. Phase numbers follow `agents.md` §14.

Last updated: 2026-10-18 (Phase 1 syntax reworked to the agents.md v4 grammar).

## Phase status

| # | Phase | Status |
| --- | --- | --- |
| 0 | Design: spec, EBNF, semantics, ADRs, threat model, conformance matrix | partial — spec/grammar/ADRs drafted; conformance matrix, threat model, provider verification pending |
| 1 | Syntax: lexer, parser, AST, source maps, diagnostics with recovery | done — 1.0 surface grammar implemented; every §11/§5.7 example parses |
| 2 | Semantics: module graph, symbols, type system, generics, Option/Result, effects, source/config cross-validation | not started |
| 3 | Compiler and evaluator: typed IR, deterministic evaluator, workflow graph validation, `explain`/`plan` | not started |
| 4 | Config and integrations: YAML schema, env/secret resolution, redaction, registry, profiles, precedence, adapters/dialects | partial — `axiom.yaml` schema/validation only; no adapters |
| 5 | Filesystem, tools, artifacts: secure FS, workspaces, Git worktrees, patches, artifacts, tool registry, command profiles | not started |
| 6 | Agent runtime: agents, teams, goals, contracts, tool loops, typed output validation | not started |
| 7 | Workflow engine and state: full semantics, durable state, checkpoints, resume, cancel, approvals, events, traces | not started |
| 8 | Stdlib and reference program: complete stdlib; §11 as a real integration test | not started |
| 9 | Tooling: CLI, formatter, linter, LSP | partial — `check` and `config check` only |
| 10 | Hardening and docs: conformance, property/fuzz/security/concurrency tests, performance, docs, examples, packaging | not started |
| 11 | Release sign-off: audit, `docs/release-readiness.md` | not started |

## Requirement ledger

| Requirement | Phase | Status | Test evidence | Documentation | Known limitations |
| --- | --- | --- | --- | --- | --- |
| `.ax` lexer with spans (1.0, operators, durations, sizes) | 1 | done | `crates/axiom-syntax`: 12 lexer unit tests | `docs/language-spec.md` | — |
| `.ax` parser + AST + recovery (1.0 grammar) | 1 | done | `parse_valid` (10) / `parse_invalid` (6) | `docs/grammar.ebnf` | — |
| Member blocks (agent/tool/goal/team/contract/policy/workspace) incl. labels and routing arrows | 1 | done | `tool_team_goal_and_contract_member_blocks`, `workspace_member_blocks_and_limits` | `docs/grammar.ebnf` | — |
| Full workflow statement set (`yield`, `timeout`, `on_error`, `retry`, `parallel for … [max]`, `require … else escalate`) | 1 | done | `reference_workflow_parses` | `docs/language-spec.md` | — |
| All `agents.md` §11/§5.7 examples parse cleanly | 1 | done | `examples_parse_cleanly` (8 files) | `examples/src/*.ax` | Parser only; type-checking/execution are later phases |
| No panic on malformed/fuzz input | 1 | done | `parser_never_panics_on_token_soup` | `docs/decisions/0007-parser-recovery.md` | Property/fuzz targets not set up |
| `axiom check` CLI | 1,9 | partial | CLI smoke tests | `README.md` | No machine-readable output yet |
| `axiom.yaml` load + `${...}` expansion + validation | 4 | done | `crates/axiom-config` (22 tests) | `docs/configuration.md` | Value diagnostics use approximate spans |
| YAML keys match 1.0 spec (`integrations`/`integration`/`timeout_seconds`) | 4 | done | config integration tests | `docs/configuration.md` | `language:`/`sources:` top-level keys not yet accepted |
| `filesystem:` config section parsing/validation | 4 | partial | config tests | `docs/configuration.md` | Parsed but not enforced (Phase 5) |
| `commands:` allowlisted command profiles (schema) | 4,5 | partial | config tests | `docs/configuration.md` | Parsed/validated only; no execution (Phase 5) |
| Modules, imports, exports, multi-file projects | 1,2 | partial | parser tests | `docs/grammar.ebnf` | Syntax only; resolution is Phase 2 |
| Static type system (primitives, generics, unions, Option/Result, inference) | 2 | not started | — | `docs/language-spec.md` | — |
| Effects/capabilities static checking | 2,4 | not started | — | `docs/architecture.md` | — |
| Typed IR + `explain`/`plan`/dry-run | 3 | not started | — | — | — |
| Adapter registry + native/generic/mock adapters | 4 | not started | — | `docs/providers.md` | Config schema only |
| Typed tools + command profiles (enforced) | 5 | not started | — | — | — |
| Filesystem/workspace/artifact subsystem | 5,8 | not started | — | — | — |
| §11 reference program as a compiling integration test | 8 | not started | — | `examples/src/*.ax` | Parses now; execution is Phase 8 |
| Agent runtime, teams, goals, contracts | 6 | not started | — | — | — |
| Hybrid workflow engine + durable state | 7 | not started | — | — | — |
| Standard library modules | 8 | not started | — | — | — |
| Formatter, linter, LSP | 9 | not started | — | — | — |
| Conformance/security/fuzz/property suites | 10 | not started | — | — | — |
| Full docs set | 10 | partial | — | `docs/` | Several required docs missing |
| Release-readiness report | 11 | not started | — | — | — |

## Truthfulness note

Axiom 1.0 is **not** complete. The current repository is a Phase 1 front end for
the 1.0 surface grammar plus a validated `axiom.yaml` (1.0 schema) loader. The
lexer, parser, and AST cover the full surface syntax — including member blocks,
positional/named arguments, `await`/`delegate`/`parallel for` expressions,
durations/sizes, and the complete workflow statement set — and every reference
example in `agents.md` §11 and §5.7 parses cleanly. There is no semantic
analysis, type checking, IR, or execution yet. The next increment is Phase 2:
the module graph, symbols, the type system, and effect checking. Nothing outside
the parser tests, configuration tests, and CLI smoke tests above should be
assumed to work.
