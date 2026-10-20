# Release readiness

Status: **not ready for an Axiom 1.0 release.** This audit covers the repository
at version `0.1.0` and records what is evidenced, what is partial, and what is
still missing. A green test suite is not a completeness claim.

Last audited: 2026-10-20. Audited source commit: `7f63c1e` (CI green); this
document was updated immediately after that verification.

## Decision

- `0.1.0` is a **development snapshot**, not a supported release.
- Phases 0-10 are implemented to the extent recorded in
  `docs/implementation-ledger.md`; Phase 11 is partial; Phase 12 is this audit.
- The blocking gaps for 1.0 are listed under "Definition of Done" and
  "Known limitations". The largest are: no function-level WASM codegen or real
  browser run, no HTTP server/browser API stubs, no networked provider adapters,
  no LSP, no persistent state/journal backends, and no PostgreSQL wire adapter.

## Environment

| Item | Value |
| --- | --- |
| OS | Windows 11 Home 10.0.26200, x86_64 |
| Rust | 1.99.0, host `x86_64-pc-windows-gnu` (pinned by `rust-toolchain.toml`) |
| Cargo | 1.99.0 |
| Node | v25.6.1 |
| CI | GitHub Actions, Ubuntu latest, `cargo fmt`/`clippy`/`test` |

Windows Application Control may block freshly built executables with OS error
4551. The audit ran test binaries through a strip/sign/temp-copy helper; this is
a local workaround, not part of the product.

## Commands and results

Run from the repository root unless noted.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | pass (after replacing `chunks_exact` with `as_chunks` in `axiom-hash`) |
| `cargo test --workspace` | pass: 52 test executables, 274 tests, 1 ignored (Node smoke requires `--ignored`); executed via the Windows sign helper |
| `cargo tree --workspace --duplicates` | no duplicate dependencies reported |
| `cargo metadata --no-deps` | 16 workspace packages, all `MIT OR Apache-2.0` |
| `axiom config check examples/axiom.yaml` | 0 errors, 1 warning (`AXIOM_LOCAL_API_KEY` unset; optional integration) |
| `axiom check examples/src/*.ax --config examples/axiom.yaml` | 0 errors, same warning; 5 files checked |
| `axiom fmt examples/src/*.ax --check` | pass after the audit reformatted all shipped examples |
| `axiom run examples/src/deterministic.ax` | `"positive: 5 a"` |
| `axiom workspace check examples/src/agents.ax` | 1 workspace, 0 errors |
| `axiom sandbox plan --config examples/axiom.yaml --command cargo_check` | plan printed, network `false` |
| `axiom artifacts hash examples/axiom.yaml` | digest printed, `verified: true` |
| `axiom db check --config examples/axiom.yaml` | SQLite least-privilege roles; PostgreSQL entry policy-only |
| `axiom db migrations plan --config examples/axiom.yaml --database app` | 1 pending migration, no destructive changes |
| `axiom build --target wasm_browser --out ... examples/src/deterministic.ax` | artifact written |
| `node browser/host-smoke.mjs ...` | `ok` (hash, ABI v1, IR v1) |
| `axiom build --target wasm_browser examples/src/{shared,frontend,server}.ax` | **fails**: `get_dashboard` uses `db.read`, which is not available on `wasm_browser` |
| Clean checkout: `cargo build --workspace --all-targets --all-features` | pass at `7f63c1e` (see below) |
| GitHub Actions CI (`fmt`, `clippy --all-features`, `test`) | pass on `7f63c1e` at https://github.com/jvdhtm-lab/axiom/actions/runs/39422548796 |

The `check`/`config check` warning is expected: `examples/axiom.yaml` references
integration keys by environment variable, and the audit used placeholder values
for the required aliases only.

## Supported platforms, providers, and features

### Platforms

- **Tested:** Windows 11 x86_64 with the GNU host. The workspace pins Rust 1.99.0.
- **CI-covered:** Ubuntu latest for format, clippy, and tests.
- **Not verified:** macOS; Windows MSVC; 32-bit targets.
- SQLite uses the system `winsqlite3.dll` on Windows. PostgreSQL is configuration
  and policy only; no wire adapter exists.

### Providers

- Offline mock provider only (`axiom-providers`). Model aliases and deterministic
  mock requests/responses are tested.
- No networked Anthropic, DeepSeek, Kimi, or local adapter is implemented. The
  example config names them, but runtime calls do not reach a network.

### Feature status

| Area | Status |
| --- | --- |
| Parser, formatter, diagnostics, source maps | working; generated-input formatter fixed point |
| Module graph, symbols, effect/target checks | partial; expression inference and match exhaustiveness pending |
| Typed IR + deterministic server evaluator | working for the implemented subset |
| WASM artifact + host ABI | artifact embeds IR; no function-level WASM codegen or DOM renderer |
| Config schema and secret references | working; strict unknown-key checks |
| Filesystem, workspaces, patches, sandbox | working; in-memory/worktree based |
| SQLite database access, roles, approvals, audit, migrations | working offline |
| PostgreSQL | policy/URL validation only |
| Agents, validators, budgets, candidates | working with mock providers |
| Frontend state, migrations, snapshots, quarantine | library only; not browser-bound |
| Artifact registry and caches | library only; process-local |
| Durable execution | in-memory journal/replay, cancellation, budgets, redacted traces |
| API authorization | `ApiGuard` library boundary; no HTTP server or identity provider |
| CLI tooling | `init`, `fmt`, `lint`, `check`, `build`, `explain`, `run`, `config`, `workspace`, `sandbox`, `artifacts`, `db`; no `test`/`dev`/`status`/`inspect`/`resume`/`cancel` |
| LSP | not started |

## Definition of Done

Legend: **met** = implemented with passing evidence; **partial** = useful subset
with a named gap; **not met** = no credible implementation.

| # | Requirement | Status | Evidence | Gap |
| --- | --- | --- | --- | --- |
| 1 | Spec, EBNF, parser, formatter, type/effect checker, IR, and examples agree | partial | `parse_valid.rs`, `parse_invalid.rs`, `lower.rs`, `analyze.rs`, `check.rs`, formatter fixed-point test, examples check | Type checker lacks general expression inference and match exhaustiveness; spec covers more than the checker proves |
| 2 | Only the small declaration/expression/control-flow surface | met | `parse_valid.rs`, `parse_invalid.rs` (`AX0010`-`AX0012`), grammar | - |
| 3 | Shared IR compiles and runs on `server` and `wasm_browser` for portable constructs | partial | IR/lowering tests; server evaluator tests; deterministic WASM artifact + Node ABI smoke | WASM embeds IR, no per-function codegen; reference app WASM build refused |
| 4 | Server APIs compile to secure handlers and typed browser stubs, authorization on every request | partial | `ApiGuard` + `reference_app.rs` (fail-closed policy, auth, capability, tenant; mock DB); IR marks `api` | No HTTP server, no typed browser stubs, no identity-provider integration |
| 5 | Agents use registered tools and bounded run-wide budgets; cannot widen authority | partial | `agents.rs` (13 tests): specs, budgets, validators, forgery rejection; host defaults deny | Tools are registered but not executed through the agent host; mock provider only |
| 6 | Validation evidence is runtime-owned and hash-bound; unaccepted work cannot publish/integrate | met | `agents.rs` (evidence, exhaustion, forged claim), `registry.rs` (evidence binding), `integration.rs` (exclusive gate) | Evidence is a runtime value; no dedicated Axiom evidence type |
| 7 | Workspace/path/command/patch/artifact security passes | met | `workspace.rs` (8), `worktree.rs` (2), `sandbox.rs` (5), `process.rs` (6), `integration.rs` (5), `artifacts/store.rs` (5) | Process isolation only; network profiles refused, not isolated |
| 8 | Frontend state typed, versioned, validated, hot-swapped, migrated/rejected | partial | `state.rs` (12): partitions, migrations, snapshots, quarantine, forbidden fields | Not bound to a browser loader; no persistent backend |
| 9 | Registry, WASM cache, and server-data cache are separate, integrity-checked, version-aware, non-authoritative | partial | `registry.rs` (11), `cache.rs` (9) | In-memory/process-local; no HTTP auth integration; no CLI publish/activate |
| 10 | Agent-generated frontend compiles to WASM, passes real browser tests, publishes/rolls back atomically | not met | Registry rollback is tested; deterministic WASM ABI smoke passes | No real browser render/event test; reference frontend cannot build for WASM |
| 11 | Parameterized/typed DB access, least-privilege roles, approval/audit/migration tests, redacted secrets | partial | `sql.rs` (11), `sqlite.rs` (6), `write.rs` (6), `migrations.rs` (8), `postgres.rs` (4) | PostgreSQL has no wire adapter; gated test skips without a release URL |
| 12 | Durable runs: trace, budgets, cancellation, tested resume; no exactly-once claim | partial | `durable.rs` (10): replay, approvals, idempotency keys, child cancellation, redacted traces | In-memory journal, no trace exporter, no timeout scheduler, no `runtime.retry`; at-least-once only |
| 13 | Security, fuzz, property, concurrency, integration tests pass; clean-checkout CI succeeds | partial | Security/integration suites above; formatter property test; parser "token soup" test; clean-checkout build recorded below | No fuzz harness (`cargo-fuzz`), no true concurrency stress; CI covers Ubuntu only |
| 14 | All shipped examples parse, type-check, compile for declared targets, and run in tests | partial | Examples parse/check; deterministic runs; `reference_app.rs` runs the server handler against a mock DB | Reference app's `wasm_browser` target is refused; frontend has no browser test |
| 15 | `docs/release-readiness.md` lists commands/results, supported surface, limitations, risks | met | This document | - |

## Clean-checkout verification

A fresh clone of `https://github.com/jvdhtm-lab/axiom` at commit `7f63c1e` was
created in a separate directory with `core.autocrlf=false` and an empty `target/`
directory, then checked with:

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | pass |
| `cargo build --workspace --all-targets --all-features` | pass |
| `cargo test --workspace` (signed test executables) | pass: 52 executables, 274 tests, 1 ignored |

GitHub Actions ran the same source on Ubuntu and passed (run `39422548796`).
The first CI attempt at `23cd597` failed because a test assumed Windows path
semantics (`C:/...` is absolute). `spec_from_config` now rejects drive-prefixed
paths on every platform, the test covers both POSIX and Windows absolute paths,
and the fix is in `7f63c1e`. This is why the clean-checkout result is recorded
against `7f63c1e`, not the audit commit.

## Known limitations

1. **Browser target is incomplete.** No function-level WASM codegen, DOM
   renderer, event loop, or real headless-browser test. The reference frontend
   imports the server API and fails the `wasm_browser` build.
2. **No HTTP server or browser API stubs.** `ApiGuard` is a library boundary;
   `get_dashboard` is exercised with a mock DB, not over a socket.
3. **Providers are offline mocks.** No network adapters, retries, streaming, or
   provider-specific tool calling.
4. **State, registry, caches, and durable journals are in memory.** Restart loses
   them; there is no external trace exporter or timeout scheduler.
5. **PostgreSQL is policy-only.** The gated test validates a URL then skips
   without a live server; no SQL wire protocol implementation exists.
6. **Type checker is partial.** General expression inference, match
   exhaustiveness, and some cross-module typing are not proven.
7. **Tool execution is not an agent-callable host path.** Plans and process
   execution are tested separately.
8. **No LSP and incomplete CLI lifecycle.** `axiom test`, `dev`, `status`,
   `inspect`, `resume`, and `cancel` are absent.
9. **Platform coverage is narrow.** Windows GNU is tested; Linux CI runs the
   suite, but macOS/MSVC are unverified.
10. **No LICENSE files.** `Cargo.toml` declares `MIT OR Apache-2.0`, but the
    repository does not ship the license texts.
11. **Windows signing workaround is local.** CI does not exercise the
    Application Control/sign path, and the helper is not a product feature.

## Residual risks

- **False confidence from mocks.** Mock providers/databases prove plumbing, not
  vendor or wire compatibility.
- **At-least-once delivery.** Callers must supply idempotency keys; a crash after
  an external side effect but before journal commit can repeat it.
- **Authorization is only as good as its wiring.** Until an HTTP server calls
  `ApiGuard` on every protected route, exported APIs are not production-secured.
- **Token-level SQL analysis.** The analyzer rejects by class and table scope; it
  is not a full SQL parser and does not prove query intent.
- **In-memory secrets and state.** Process memory is the only store; there is no
  encryption at rest or key management.
- **Unverified toolchains.** Newer/older Rust or a different C toolchain may fail
  to build; `rust-version` and the pinned toolchain are the supported contract.
- **Incomplete fuzzing/concurrency evidence.** Parser and formatter have
  generated/property tests, but there is no coverage-guided fuzzing or
  multi-threaded stress suite.

## Gates before calling this 1.0

1. Implement function-level WASM codegen, DOM rendering, event forwarding, and a
   real headless-browser test; make the reference frontend build for
   `wasm_browser`.
2. Add a server HTTP adapter that enforces `ApiGuard` on every protected route,
   plus typed browser stubs.
3. Implement at least one real provider adapter with gated integration tests.
4. Add persistent state, release registry, cache, and durable journal backends;
   add trace export and timeout scheduling.
5. Implement and test the PostgreSQL wire adapter, or explicitly drop the claim.
6. Complete type inference/exhaustiveness or narrow the documented type system.
7. Add LSP diagnostics and the missing CLI lifecycle commands.
8. Add a coverage-guided fuzz target and concurrency tests; run the full suite on
   Windows and Linux in CI.
9. Ship license files and verify `cargo package`/release artifacts.
10. Re-run this audit and update every DoD row with clean-checkout evidence.
