# Implementation ledger

Tracks Axiom 1.0 requirements against implementation status. **Do not mark an
item complete without test evidence.** Statuses: `done` (implemented + tested),
`partial`, `not started`. Phase numbers follow `agents.md` §15.

Last updated: 2026-10-18 (Phase 6 partial: filesystem policy/overlays/patches, artifact store, command sandbox planning).

## Phase status

| # | Phase | Status |
| --- | --- | --- |
| 0 | Design: small grammar, core types, effects, IR plan, API modifier, workspace model, target matrix, browser ABI, threat model | done — `docs/grammar.ebnf`, `docs/language-spec.md`, `docs/conformance.md`, `docs/security-model.md`, ADR-0014 |
| 1 | Lexer, parser, AST, source maps, diagnostics, formatter skeleton | done — `axiom-syntax` + `axiom-format`; positive/negative fixtures |
| 2 | Module resolution, types, generics, Option/Result, effect and target checking | partial — module graph, symbols, duplicates, name/type resolution, effect propagation, target checks, and SQL literal/placeholder policy done; expression type inference and match exhaustiveness pending |
| 3 | Typed IR and deterministic server evaluator; basic compiler commands | done — `axiom-ir` (deterministic JSON IR with types/effects/capabilities/provenance), `axiom-runtime` evaluator, `axiom build/explain/run`; WASM emission explicitly refused until Phase 4 |
| 4 | WASM lowering, UI tree, host ABI, browser loader/renderer | partial — `axiom-wasm` emits a valid, content-addressed WASM module embedding the checked IR; `axiom-browser-host` verifies header/ABI/hash/imports; Node integration smoke harness passes. **Missing:** function-level WASM codegen, the DOM renderer/event loop, and the real headless-browser smoke test |
| 5 | Config, model integrations, registered tools and commands | partial — `axiom.yaml` now matches the §4 schema (integrations, models, runtime budgets, `api_policies`, `compilation`/`entrypoints`/`wasm`, `browser`, `frontend`, `filesystem`, `commands`) with strict unknown-key checks; `axiom-providers` resolves model aliases to a vendor-neutral spec and builds an offline mock provider; `axiom-tools` registers builtins + configured commands. **Missing:** networked adapters and real tool execution/sandbox (Phase 6) |
| 6 | Filesystem, workspaces, patch/artifact APIs and command sandbox | partial — `axiom-filesystem` implements path normalization/root-escape refusal, workspace read/write/deny/protected policy with narrowing, copy-on-write overlays with snapshot/restore/discard, quotas, and patch generation/conflict checks; `axiom-artifacts` is a content-addressed immutable store with integrity verification; `axiom-tools` plans shell-free, allowlisted commands. **Missing:** real OS/container sandboxing, Git worktrees, and wiring these APIs into the CLI/runtime |
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
| Effect vocabulary (closed set) | 2 | done | `unknown_effect_is_reported` (`AX0216`) | `docs/language-spec.md` §7 | — |
| Name + type resolution (values, types, generics, scopes) | 2 | done | `unresolved_name_is_reported` (`AX0301`), `unresolved_type_is_reported` (`AX0302`) | `docs/language-spec.md` §6 | Member/method shapes not fully modelled |
| Effect propagation + subset rule (pure discipline, `await`⇒`async`) | 2 | done | `undeclared_effect_is_reported` (`AX0303`), `pure_function_performing_effects_is_reported` (`AX0304`), `await_in_sync_function_is_reported` (`AX0305`) | `docs/security-model.md` | Unmodelled stdlib members conservatively treated as pure (documented) |
| Agent/workspace member shape + isolation modes | 2 | partial | `agent_requires_model` (`AX0211`), `unknown_isolation_mode_is_reported` (`AX0214`) | `docs/language-spec.md` §5 | `tools`/`limits` shape only |
| Config cross-validation (model aliases) | 2,5 | partial | `unknown_model_alias_is_reported_when_configured` (`AX0217`) | `docs/configuration.md` | `axiom.yaml` schema itself is Phase 5 |
| Static type system, inference, match exhaustiveness | 2 | partial | Name/type resolution tested; expression inference and `match` exhaustiveness not implemented | `docs/language-spec.md` §4 | No structural typing yet |
| Target compatibility (server vs wasm_browser) | 2 | partial | `browser_target_rejects_server_effects` (`AX0310`) | `docs/language-spec.md` §8 | Whole-project target option; per-module targets are Phase 4 |
| SQL literal + placeholder policy | 2,7 | partial | `non_literal_sql_is_reported` (`AX0320`), `sql_placeholder_mismatch_is_reported` (`AX0321`) | `docs/language-spec.md` §7 | Covers `db.query`/`db.query_one`/`db.execute`; full DB subsystem is Phase 7 |
| Typed IR (types, effects, capabilities, provenance, target) | 3 | done | `axiom-ir/tests/lower.rs`: determinism, effects, enum constructors, `api` | `docs/architecture.md` | Declared types only; no inference (Phase 2 gap) |
| Deterministic server evaluator (pure subset, fail-closed effects) | 3 | done | `axiom-runtime/tests/eval.rs` (7): entry point, calls, `match`/`Option`, finite `for`, effectful-call rejection, no panics | `docs/language-spec.md` §6 | No host I/O; effectful calls require later phases |
| CLI `build` / `explain` / `run` | 3 | done | `axiom run examples/src/deterministic.ax` → `"positive: 5 a"`; `build --target wasm_browser` refuses | `docs/development.md` | `lint`/`fmt` CLI wiring still Phase 11 |
| WASM artifact (content-addressed, IR-embedding) + fail-closed portability | 4 | partial | `axiom-wasm/tests/wasm.rs`: SHA-256 vectors, valid `\0asm` module, determinism, non-browser effect rejection | `docs/architecture.md` | Codegen embeds IR; no per-function WASM functions yet |
| Host ABI + artifact verification + Node integration harness | 4 | partial | `axiom-browser-host/tests/host.rs`; `node browser/host-smoke.mjs` → `ok: sha256 … abi v1 ir v1` | `docs/security-model.md` | Integration test, not a real browser test |
| Real headless-browser smoke test (render `UiTree`, forward event) | 4 | not started | — | `docs/architecture.md` | Requires browser harness + function codegen |
| Config schema (`axiom.yaml`) for §4 sections + strict unknown-key/placeholder checks | 5 | done | `axiom-config/tests/config.rs` (25): bundled example, api_policies/compilation/browser/frontend, unknown target/host-api/key, placeholder + redaction | `docs/configuration.md` | Value spans still approximate |
| Model alias resolution + mock provider | 5 | partial | `axiom-providers/tests/providers.rs` (7): spec resolution, alias switch without source edits, unknown alias, no secret leakage, mock determinism | `docs/configuration.md` | Only offline mocks; no networked adapters |
| Tool registry (builtins + configured commands) | 5 | partial | `axiom-tools/tests/tools.rs` (5): builtin kinds/effects, command tools, unknown tool/command refusal | `docs/configuration.md` | Registry only; execution/sandbox is Phase 6 |
| Path normalization + root-escape/symlink-safe policy, workspace read/write/deny/protected sets, narrowing | 6 | partial | `axiom-filesystem/tests/workspace.rs` (8) + `policy`/`path` unit tests: escape/absolute refusal, deny/protected win, derived cannot widen | `docs/security-model.md` | Symlink handling is by construction (no OS traversal yet) |
| Copy-on-write isolation, snapshot/restore/discard, quotas | 6 | partial | `workspace.rs`: base stays pristine, `read_only` refuses writes, `max_file_bytes`/`max_changed_files` limits | `docs/architecture.md` | No Git worktree or OS sandbox; exclusive-integration lock not implemented |
| Patch generation + integration conflict checks | 6 | partial | `workspace.rs`: `diff`/`submit`, conflict on moved base | `docs/security-model.md` | Text patch only; no merge |
| Immutable content-addressed artifact store | 6 | partial | `axiom-artifacts/tests/store.rs` (5): determinism, idempotent publish, tamper detection | `docs/architecture.md` | Registry versioning/retention/caches are Phase 9 |
| Command sandbox: allowlisted profile + structured argv (never a shell) | 6 | partial | `axiom-tools/tests/sandbox.rs` (5): argv build, metacharacters literal, unknown profile/unsafe dir refused, mock runner | `docs/security-model.md` | Planning/validation only; OS/container execution is Phase 6 hardening |
| Database subsystem | 7 | not started | — | `docs/language-spec.md` §7 | — |
| Agents/validation/budgets | 8 | not started | — | — | — |
| Frontend state/caches | 9 | not started | — | `docs/language-spec.md` §9 | — |
| Durable execution | 10 | not started | — | — | — |
| Reference app | 11 | partial | `reference_examples_parse_cleanly`, `reference_examples_have_no_semantic_errors` | `examples/src/*.ax` | Parses/checks; not executed |
| Tooling (CLI `fmt`/`lint`/`lsp`) | 11 | partial | `axiom check`, `axiom config check` | `docs/development.md` | Formatter not yet wired to the CLI |

## Truthfulness note

Axiom 1.0 is **not** complete. The repository implements the small-language front
end (lexer, parser, AST, spans, diagnostics), a deterministic formatter, the
module graph with symbol tables, name/type resolution, effect propagation with
the caller-subset rule, target-compatibility and SQL-policy checks, a shared
typed IR with deterministic JSON rendering, a deterministic server evaluator for
the pure subset, the `check`/`build`/`explain`/`run` CLI, and a partial browser
backend that emits a content-addressed WASM module embedding the checked IR with
a verifying host ABI (Node integration smoke test only). It loads and validates
the full §4 `axiom.yaml` schema, resolves model aliases to a provider-neutral
spec with an offline mock provider, registers builtin and command tools, enforces
workspace path/permission/isolation/quota rules with copy-on-write overlays and
patch conflict checks, stores immutable content-addressed artifacts, and plans
shell-free allowlisted commands. It does **not** infer expression types, report
`match` exhaustiveness, generate function-level WASM or a DOM renderer, run a
real browser test, talk to a networked model provider, run commands in a real
OS/container sandbox or Git worktree, or provide the database, durability, or
budget subsystems; these filesystem/artifact/tool APIs are not yet wired into the
CLI/runtime. Nothing should be assumed to work unless the requirement ledger
above cites a passing test.
