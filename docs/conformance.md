# Axiom 1.0 conformance matrix

Maps each language/runtime requirement to the fixtures and tests that exercise
it. This is the Phase 0 conformance artifact; it is updated as phases land. An
entry is only meaningful when the cited test exists and passes.

| Area | Positive fixtures / tests | Negative fixtures / tests |
| --- | --- | --- |
| Declarations (§3.3) | `crates/axiom-syntax/tests/parse_valid.rs`: `module/import/type/enum/fn/agent/workspace` | `parse_invalid.rs`: `unknown_decl`, `forbidden_decl_keyword` |
| Forbidden concepts (§3.3) | — | `parse_invalid.rs`: `query/operation/team/goal/task/contract/policy/tool/workflow/state` rejected |
| `api`/`async`/`effects` (§3.5, §7.2) | `parse_valid.rs`: `api_async_fn_with_effects` | `parse_invalid.rs`: `effect_clause_after_body` |
| Statements (§3.4) | `let`, `if`, `match`, `for`, `return`, call statements | `parse_invalid.rs`: `while_is_rejected`, `throw_is_rejected`, `assignment_is_rejected` |
| Patterns (§3.4) | `Ok(x)`, `Err(_)`, `DashboardEventKind.Refresh`, `_`, literals | `parse_invalid.rs`: `malformed_pattern` |
| Expressions (§3.4) | member/index/call/named args/generic call (`db.query_one<T>(…)`) | `parse_invalid.rs`: `lambda_is_rejected`, `tuple_type_is_rejected` |
| Types (§3.4) | primitives, `T[]`, records, `Option`/`Result`/`Map`, generics | `parse_invalid.rs`: `unknown_type_syntax` |
| Module graph (§7.1) | `axiom-types`: import resolution, cycles | `axiom-types`: `missing_import`, `unexported_name`, `import_cycle` |
| Name resolution (§7.1) | `axiom-types`: local/module/import/builtin/namespace | `axiom-types`: `unresolved_name` |
| Types/inference | `axiom-types`: `type_check.rs` | `axiom-types`: `type_mismatch`, `arity_mismatch` |
| Effects (§3.5) | `axiom-types`: declared/subset/min intersection | `axiom-types`: `undeclared_effect`, `unknown_effect` |
| Target compatibility (§7.1) | `axiom-types`: server uses `db.read` | `axiom-types`: `browser_uses_db_read` |
| SQL policy (§10) | `axiom-database`/checker: literal SQL + matched placeholders | `axiom-types`: `nonliteral_sql`, `placeholder_mismatch` |
| Typed IR (§7.1) | `axiom-ir`: round-trip / inspectable stable IR | `axiom-ir`: `unknown_effect_fails` |
| Deterministic evaluator (§7.1) | `axiom-runtime`: reference expressions/`match`/`for` | `axiom-runtime`: `nonliteral_sql_rejected_at_runtime` |
| Reference app (§13) | `tests/reference_app.rs`: compiles + evaluates | — |
| WASM backend (§7.3) | Phase 4 | Phase 4: `forbidden_import_rejected` |
| Config (§4) | `axiom-config`: schema, `${VAR}`, `${VAR:-default}` | `axiom-config`: `unknown_key`, `missing_env`, `bad_target` |
| Agents/validation (§5) | Phase 8 | Phase 8: `forged_evidence_rejected`, `exhausted_does_not_publish` |
| Filesystem/workspaces (§6) | Phase 6 | Phase 6: `path_escape_rejected`, `deny_glob` |
| Database roles/approval (§10) | Phase 7 | Phase 7: `injection_rejected`, `approval_hash_mismatch` |
| State/migration (§8) | `axiom-state/tests/state.rs`: hot-swap, migration chain, snapshot/restore, per-partition isolation | `state.rs`: forbidden credential/authorization fields, failed migration keeps old state, downgrade refused, tampered snapshot |
| Artifacts/caches (§8.5) | `axiom-artifacts/tests/registry.rs`: content-addressed publish, evidence binding, activation/rollback, retention; `tests/cache.rs`: reproducible build keys, TTL/tag invalidation, partition isolation | `registry.rs`: unpinned refs, ABI mismatch, revoked activation/republish, unbound evidence; `cache.rs`: non-reproducible key, missing TTL, stale-on-error default |
| Durable execution (§11) | `axiom-runtime/tests/durable.rs`: journal replay, checkpoints, resume, cancellation, budgets | `durable.rs`: `resume_skips_completed_effects_and_does_not_write_twice`, `indeterminate_write_requires_an_approval`, `idempotency_key_allows_a_retry_without_approval`, `cancellation_stops_children_and_parent`, `checkpoints_are_recorded_and_traces_are_redacted`, `effect_budget_fails_closed`, `diverged_journals_are_refused` |
