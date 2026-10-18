# Implementation ledger

Tracks Axiom 1.0 requirements against implementation status. **Do not mark an
item complete without test evidence.** Statuses: `done` (implemented + tested),
`partial`, `not started`. Phase numbers follow `agents.md` §14.

Last updated: 2026-10-18 (Phase 2: strict name resolution, `query`/`operation` declaration checks, `iterate` expressions, and config `language`/`sources`/`dialect`/`allowed_data_classes` landed).

## Phase status

| # | Phase | Status |
| --- | --- | --- |
| 0 | Design: spec, EBNF, semantics, ADRs, threat model, conformance matrix | partial — spec/grammar/ADRs drafted; conformance matrix, threat model, provider verification pending |
| 1 | Syntax: lexer, parser, AST, source maps, diagnostics with recovery | done — 1.0 surface grammar implemented; every §11/§5.7 example parses |
| 2 | Semantics: module graph, symbols, type system, generics, Option/Result, effects, source/config cross-validation, `query`/`operation` declarations, frontend/WASM multi-target surface | partial — module graph, symbol tables, declaration/aggregate/effect checks, strict name resolution, `query`/`operation` declaration checks, `iterate` expressions, multi-target/frontend parsing, and literal config cross-validation landed; full type inference and effect propagation pending |
| 3 | Compiler and evaluator: typed IR, deterministic evaluator, workflow graph validation, `explain`/`plan` | not started |
| 4 | Config and integrations: YAML schema, env/secret resolution, redaction, registry, profiles, precedence, adapters/dialects | partial — `axiom.yaml` schema/validation only; no adapters |
| 5 | Filesystem, tools, artifacts: secure FS, workspaces, Git worktrees, patches, artifacts, tool registry, command profiles | not started |
| 6 | Agent runtime: agents, teams, goals, contracts, tool loops, typed output validation | not started |
| 7 | Workflow engine and state: full semantics, durable state, checkpoints, resume, cancel, approvals, events, traces | not started |
| 8 | Stdlib and reference program: complete stdlib; §11 as a real integration test | not started |
| 9 | Tooling: CLI, formatter, linter, LSP | partial — `check` (with `--config`) and `config check` only |
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
| `axiom check` CLI (parse + module graph + semantics, optional `--config`) | 1,2,9 | partial | CLI smoke tests; `check` runs `axiom-types` | `README.md` | No machine-readable output yet |
| `axiom.yaml` load + `${...}` expansion + validation | 4 | done | `crates/axiom-config` (22 tests) | `docs/configuration.md` | Value diagnostics use approximate spans |
| YAML keys match 1.0 spec (`integrations`/`integration`/`timeout_seconds`) | 4 | done | config integration tests | `docs/configuration.md` | `language:`/`sources:` top-level keys not yet accepted |
| `filesystem:` config section parsing/validation | 4 | partial | config tests | `docs/configuration.md` | Parsed but not enforced (Phase 5) |
| `commands:` allowlisted command profiles (schema) | 4,5 | partial | config tests | `docs/configuration.md` | Parsed/validated only; no execution (Phase 5) |
| Module graph: relative import resolution, missing/unexported names, cycles, duplicate module paths | 2 | done | `crates/axiom-types` tests | `docs/architecture.md` | Deterministic path normalization; no `../` escape guard yet |
| Symbol tables per module (kind, export, duplicates) | 2 | done | `crates/axiom-types` tests | `docs/architecture.md` | One namespace per module; no nested scopes yet |
| Strict name resolution: every value/type reference resolves to a local binding, module declaration, import, built-in, prelude constant, or stdlib namespace | 2 | done | `unresolved_value_name_is_reported`, `unresolved_type_name_is_reported`, `namespaces_and_builtins_resolve` | `docs/language-spec.md` | Member access is shallow (namespaces/members not typed) |
| `query` declarations: required `sql` literal / `database` / `access`, `:name` placeholder↔parameter match, write ⇒ `WriteResult` | 2 | done | `query_without_sql_is_reported`, `query_placeholder_mismatch_is_reported`, `write_query_must_return_write_result` | `docs/language-spec.md` | Lexical placeholder scan; full SQL scanner/row typing is Phase 6 |
| `operation` declarations: required `version`, `authentication`, `handler` (resolved cross-module) | 2 | done | `operation_declaration_is_checked` | `docs/language-spec.md`, ADR-0013 | Handler target not type-checked against the signature |
| `iterate` expressions: positive `max_iterations`, `validate` list, `until`; effectful | 2 | done | `iterate_bound_must_be_positive` | `docs/language-spec.md`, ADR-0013 | Validator registry resolution is Phase 8 |
| Multi-target/frontend surface: `ui`/`api`/`browser` namespaces, `operation`/`iterate`, dotted enum patterns, frontend reference slice | 2 | partial | `reference_project_has_no_semantic_errors`, parse fixtures | `agents.md` §12.9, ADR-0013 | Parsed/checked only; IR/codegen crates not built |
| Config `language`/`sources` top-level; integration `dialect`/`allowed_data_classes` (`secret` rejected) | 4 | done | `language_and_sources_are_read`, `unsupported_language_version_is_reported`, `allowed_data_classes_default_and_validation` | `docs/configuration.md` | `sources` globs not yet used to discover modules |
| Declaration/aggregate checks (duplicates, record/interface/enum fields, generic arity) | 2 | partial | `axiom-types` tests | `docs/language-spec.md` | Only built-in generic arities checked; no user-generic arity |
| Effects: known effect names, `pure` discipline (no effects clause, no `await`/`delegate`/`parallel`) | 2 | partial | `axiom-types` tests | `docs/language-spec.md` | No inter-procedural effect propagation or capability intersection |
| Source/config cross-validation (literal model aliases, `commands.run` names) | 2,4 | partial | `axiom-types` tests; `axiom check --config` | `docs/configuration.md` | Tool `binding` strings not yet resolved |
| Static type system (generics, unions, Option/Result, inference, `match` exhaustiveness) | 2 | not started | — | `docs/language-spec.md` | — |
| Typed IR + `explain`/`plan`/dry-run | 3 | not started | — | — | — |
| Adapter registry + native/generic/mock adapters | 4 | not started | — | `docs/providers.md` | Config schema only |
| Typed tools + command profiles (enforced) | 5 | not started | — | — | — |
| Filesystem/workspace/artifact subsystem | 5,8 | not started | — | — | — |
| §11 reference program as a compiling integration test | 8 | not started | — | `examples/src/*.ax` | Parses and passes semantic checks; execution is Phase 8 |
| Agent runtime, teams, goals, contracts | 6 | not started | — | — | — |
| Hybrid workflow engine + durable state | 7 | not started | — | — | — |
| Standard library modules | 8 | not started | — | — | — |
| Formatter, linter, LSP | 9 | not started | — | — | — |
| Conformance/security/fuzz/property suites | 10 | not started | — | — | — |
| Full docs set | 10 | partial | — | `docs/` | Several required docs missing |
| Release-readiness report | 11 | not started | — | — | — |

## Truthfulness note

Axiom 1.0 is **not** complete. The repository is a Phase 1 front end for the 1.0
surface grammar, a validated `axiom.yaml` (1.0 schema) loader, and the **first
increment of Phase 2**: a multi-file module graph with import validation and
cycle detection, per-module symbol tables, and semantic checks for duplicate
declarations, malformed aggregates, built-in generic arity, effect validity,
`pure` discipline, and cross-validation of literal model aliases and
`commands.run` names against configuration. Full type inference, expression
typing, `match` exhaustiveness, and inter-procedural effect propagation are
**not** implemented. `axiom check` loads the module graph and runs these checks
(with an optional `--config`); it performs no analysis, IR, or execution beyond
that. Nothing outside the parser, semantic, configuration, and CLI tests above
should be assumed to work.

