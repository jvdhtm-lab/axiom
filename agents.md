# Axiom 1.0 — Simplified Implementation Prompt

## 1. Mission

Build **Axiom**, a small, strongly typed language and runtime for building full-stack applications with AI agents operating inside explicit boundaries and isolated workspaces.

Axiom has one source language and one shared type/effect system. It compiles application code for two required targets:

- `server`: trusted backend execution, database access, and authorized API operations.
- `wasm_browser`: application frontend code compiled to WebAssembly and run inside a trusted browser host.

The browser frontend is written in Axiom, not generated React, TypeScript, JSX, or application-specific JavaScript. An authorized agent can create and replace frontend modules at runtime. Accepted frontend modules are versioned artifacts, can be reused by exact reference, and can be rolled back. The frontend can change its UI and local behavior, but cannot grant itself backend authority.

Axiom's defining idea is:

> **Boundaries define authority. Workspaces define where work happens. Agents propose changes. Validation determines what is accepted.**

The purpose is not to eliminate constraints so an agent can do anything. It is to make the allowed area clear and enforceable enough that an agent can work freely inside it.

### 1.1 Required capabilities

Axiom 1.0 must support:

- Ordinary application code: types, functions, modules, records, enums, expressions, and bounded control flow.
- Typed AI agents with model profiles, tools, limits, and workspace permissions.
- Typed validation and bounded agent repair loops.
- Isolated workspaces, patch proposals, artifacts, and explicit integration/publish operations.
- A common compiler pipeline and typed IR for server and browser WASM.
- Typed server API functions callable from the browser through a trusted bridge.
- Frontend rendering, events, state, persistence, and state migration.
- A versioned frontend artifact registry, a separate WASM build cache, and an optional server-data cache.
- Typed, parameterized database access with least privilege, auditing, and guarded writes/migrations.
- Durable execution, cancellation, traces, budgets, tests, and security enforcement.

### 1.2 Design constraint: keep the language small

Do not create a dedicated language declaration for every runtime concept. Prefer a normal typed function, a built-in library call, a type, or `axiom.yaml` configuration.

The core declaration forms are:

`module`, `import`, `export`, `type`, `enum`, `fn`, `agent`, and `workspace`.

`api` is a modifier for a function exported as a server operation. `async` marks a function that can await asynchronous work. Other features—agent iteration, validation, caching, persistence, retries, checkpoints, database queries, tools, artifact publishing, and parallel work—are standard-library APIs or runtime services, not new control-flow keywords.

Do **not** implement separate source-language declarations or keywords for `team`, `goal`, `task`, `contract`, `policy`, `tool`, `query`, `operation`, `workflow`, `state`, `store`, `transition`, or `migration`. Those concepts are represented using ordinary types/functions, `agent`/`workspace` declarations, library APIs, and configuration. They may remain names for runtime concepts or data types, but not independent grammar constructs.

Do not introduce a second frontend language, a second type system, implicit global mutable state, or hidden source-language behavior implemented only in the browser host.

**Scope rule:** the required features in this prompt are the 1.0 target. If all features cannot be completed in one session, implement in phases, keep the implementation ledger accurate, and never claim a phase or release is complete without evidence.

## 2. How to work

1. Inspect the repository before changing it. Create `docs/implementation-ledger.md` listing each requirement, phase, status, test evidence, documentation, and known limitations.
2. Write a short design and grammar before implementing the parser. Keep an ADR only when an important choice needs a rationale; do not create paperwork for obvious details.
3. Make the language surface small and consistent. Update grammar, AST, typed IR, type/effect checker, formatter, diagnostics, examples, tests, and docs together.
4. Every phase ends with a compiling repository and passing tests.
5. Treat source, config, model outputs, artifact manifests, cache entries, and restored browser state as untrusted input.
6. Never claim a feature works unless it is implemented and tested. A compiling WASM binary alone is not proof that its UI works.
7. Verify current provider APIs/dependencies before fixing endpoints or versions. Record verification dates and the supported provider behavior.
8. Keep the browser host small and trusted. Application-specific UI and business logic belong in Axiom, not in handwritten host code.

## 3. The small language

### 3.1 Source format

- Source files use `.ax` and start with one `module` declaration.
- Imports are explicit and acyclic. No code executes at import time.
- Use braces for blocks. Use semicolons at the end of statements and declaration members.
- Use named arguments for built-in APIs. Use immutable local bindings by default: `let name = value;`. Do not support general reassignment in 1.0; state changes return a new value.
- `await` is required for asynchronous calls. Async functions declare `async`.
- Functions are pure unless they declare effects, e.g. `effects [api.call]`.
- Diagnostics include stable error codes and source spans.

### 3.2 Core syntax example

```ts
module example.dashboard;

export type DashboardInput = {
  team_id: string;
};

export type DashboardSummary = {
  team_id: string;
  completed: int;
};

export enum AppError { NotFound; DatabaseUnavailable; }

export api async fn get_dashboard(
  input: DashboardInput
) -> Result<DashboardSummary, AppError>
  effects [db.read] {
  let result = await db.query_one<DashboardSummary>(
    database: "app",
    sql: "SELECT team_id, completed FROM v_team_performance WHERE team_id = :team_id LIMIT 1",
    params: { team_id: input.team_id },
    timeout: 5s
  );
  match result {
    Ok(summary) => return Ok(summary);
    Err(_) => return Err(AppError.DatabaseUnavailable);
  }
}
```

This example illustrates the intended surface. Phase 0 must finalize the exact grammar and type signatures; afterward, examples must parse and run. Do not leave “illustrative” syntax in the final implementation.

### 3.3 Core declarations

- `type`: record/type alias. Records use named fields.
- `enum`: a finite set of tagged values. Use enums instead of arbitrary string flags when values are known.
- `fn`: a normal function. It may be `async` and may declare effects.
- `api fn`: a function published as a typed server operation. The server executes the body; browser code sees only a generated typed stub/contract.
- `agent`: a named AI worker with typed input/output, model profile, permitted tools, workspace, and limits.
- `workspace`: a bounded read/write area and its isolation policy.
- `module`, `import`, `export`: module organization.

Everything else uses normal functions, typed data, library calls, or config.

### 3.4 Types and expressions

Support a deliberately modest but useful type system:

- Primitive types: `string`, `bool`, `int`, `float`, `bytes`, `duration`, `timestamp`.
- Records, enums, lists (`T[]`), maps (`Map<K,V>`), and generic types/functions where needed for reusable standard-library APIs.
- `Option<T>` (`Some(value)` / `None`) and `Result<T,E>` (`Ok(value)` / `Err(error)`); no implicit `any` and no implicit conversion from unvalidated JSON/model output.
- Local type inference; explicit public/exported function signatures.
- Literals, list/record construction, member/index access, function calls, arithmetic/comparison/boolean operators, `if/else`, `match`, and `for` over a finite collection.
- No `while` loop or unbounded recursion in 1.0. Repetition, retries, agent repair, and parallel work use bounded standard-library APIs with run-wide limits.
- Do not support a general exception system (`throw`, `try/catch/finally`) in 1.0. Use typed `Result` values and `match`/documented error propagation.
- No lambdas are required in the first language surface. Standard-library functions that need callbacks can accept named function references.
- No tuple syntax, overloaded operators, macros, reflection, `eval`, dynamic imports, or executable source strings in 1.0.

Use an exhaustive `match` for enums and `Option`/`Result` when a branch must handle each case. `Option<T>` provides a documented `unwrap_or(default)` helper; enums provide a stable `to_string()` representation for display. Define evaluation order and numeric/string behavior in the spec.

### 3.5 Effects

The checker recognizes a small fixed set of effect families:

`model`, `fs.read`, `fs.write`, `fs.delete`, `workspace.create`, `artifact.read`, `artifact.publish`, `command.run`, `db.read`, `db.write`, `db.schema`, `db.provision`, `api.call`, `browser.storage`, `browser.navigation`, and `runtime.checkpoint`. UI tree construction and returning new UI state are pure; the host performs rendering and commits validated state.

A function declares the effects it may perform. Calls must not exceed the caller's effects. The compiler checks target compatibility; runtime hosts enforce authorization again. Avoid creating new effect names for every individual helper—use a stable small vocabulary and typed APIs inside each effect family.

The effective authority of a call is the intersection of the function's declared effects, agent tool grants, workspace permissions, project configuration, current principal/policy, and host capabilities. A model response, UI event, cache hit, or artifact reference cannot widen authority.

## 4. Configuration

Keep source code and deployment configuration separate:

- `*.ax`: types, functions, agents, workspaces, UI, business logic, and validation logic.
- `axiom.yaml`: provider connections, model aliases, build targets, entry points, runtime budgets, registered tools/commands, database connections, browser-host limits, state/cache policy, artifact retention, and environment-specific settings.
- Build output: immutable compiler-generated artifacts and manifests. Never hand-edit generated WASM or release manifests.

A compact illustrative configuration:

```yaml
version: 1
language: "1.0"
sources: ["src/**/*.ax"]

integrations:
  default_cloud:
    adapter: openai_compatible
    base_url: "${AXIOM_MODEL_BASE_URL}"
    api_key: "${AXIOM_MODEL_API_KEY}"
    timeout_seconds: 120

models:
  coding:
    integration: default_cloud
    model: "${AXIOM_CODING_MODEL}"

runtime:
  max_parallel_agents: 4
  max_model_calls: 100
  max_wall_clock_seconds: 1800
  max_agent_attempts: 4
  max_run_cost: null

api_policies:
  get_dashboard:
    version: "1.0.0"
    authentication: required
    requires: ["dashboard:read"]
    data_class: internal
    cache: none

compilation:
  targets: [server, wasm_browser]
  entrypoints:
    server:
      module: "src.server.main"
    wasm_browser:
      module: "src.frontend.dashboard"
      init: "init"
      render: "render"
      handle_event: "handle_event"
  wasm:
    target: wasm32-unknown-unknown
    host_api_version: 1
    max_module_bytes: 5000000
    reproducible: true

browser:
  isolation: dedicated_worker
  max_memory_pages: 256
  max_compute_ms_per_event: 100
  max_ui_nodes: 10000
  max_event_bytes: 65536
  max_storage_bytes: 1048576
  max_snapshot_bytes: 4194304
  preserve_previous_release_on_failure: true
  state:
    default_scope: route
    default_persistence: none
  data_cache:
    enabled: true
    max_entries: 500
    max_bytes: 10485760
    default_ttl_seconds: 30
    serve_stale_on_error: false

frontend:
  registry:
    content_addressed: true
    retain_accepted_versions: 10
    require_pinned_references: true
  build_cache:
    enabled: true
    max_bytes: 1073741824

filesystem:
  project_root: "."
  default_agent_isolation: copy_on_write
  max_file_bytes: 2000000
  max_files_per_task: 500
  protect_vcs_metadata: true

commands:
  unit_tests:
    executable: cargo
    args: ["test", "--offline", "--workspace"]
    working_directory: "."
    timeout_seconds: 600
    network: false
```

The committed config must validate against a strict schema. Unknown keys, executable YAML tags, invalid target names, missing required environment variables, invalid model aliases, and unsafe combinations fail with clear diagnostics. Expand `${VAR}` and `${VAR:-default}` explicitly; do not rely on the YAML library to do it. Resolve secrets at runtime and redact them from logs, diagnostics, traces, cache keys, snapshots, and agent-visible results.

Config limits are hard ceilings. Source declarations may narrow them but never widen them. Nested agents and loops share the outer run's hard budgets for model calls, time, cost (when known), concurrency, output bytes, and artifact count.

## 5. Agents, validation, and orchestration through APIs

Agents are declared once and invoked through the standard library. There are no dedicated `team`, `goal`, `task`, `contract`, `workflow`, `delegate`, `iterate`, `retry`, or `parallel for` language forms.

### 5.1 Agent declaration

```ts
module app.agents;

export agent FrontendDesigner(input: FrontendChange) -> FrontendPatch {
  model: "coding";
  workspace: frontend_edit;
  tools: [fs.read, fs.search, fs.write, compiler.check];
  limits: {
    max_calls: 8;
    timeout: 120s;
    max_output_bytes: 1000000;
  }
}
```

An agent without a workspace has no filesystem access. An agent's declared workspace is its maximum authority. A call-time workspace can narrow it, never widen it. `context` sent to a model is untrusted data, not instructions. An agent cannot add tools, edit its own grants, skip required validation, mark itself accepted, or publish a release unless it separately receives that exact trusted runtime capability.

The `agent` declaration supports only metadata: input/output types, model alias, workspace, tools, constraints text, and limits. Its body is not executable application logic. Registered tool implementations live in the Rust runtime; tool configs name their registry bindings and policies.

### 5.2 Use normal functions for workflows

A workflow is an ordinary `async fn` that calls standard-library APIs. For example:

```ts
export async fn safely_update_frontend(
  request: FrontendChange
) -> Result<FrontendRelease, AppError>
  effects [model, fs.read, fs.write, workspace.create, command.run, artifact.publish] {

  let candidate = await agents.iterate(
    agent: FrontendDesigner,
    input: request,
    max_attempts: 3,
    validate: [
      checks.ax_compile,
      checks.wasm_build,
      checks.api_compatibility,
      checks.browser_tests,
      checks.workspace_scope
    ]
  );

  match candidate.accepted_artifact() {
    Ok(artifact) => return await frontend.publish(artifact);
    Err(_) => return Err(AppError.ValidationFailed);
  }
}
```

`agents.iterate` is a library API, not a language keyword. It returns a runtime-owned `IterationOutcome<T>` containing status, attempts, candidate reference, validation summary, and evidence references. Only the runtime can set acceptance and validator evidence. A model's claim that a test passed never substitutes for actually running the test.

Required outcomes include `Accepted`, `Exhausted`, `Escalated`, `Failed`, and `Cancelled`. Every attempt is bounded, traceable, and linked to its candidate hash and validation results. A required validator failure cannot be overridden by an agent or an advisory model reviewer. If a failure is repairable and attempts remain, send structured findings back to the agent. Non-repairable failures stop or escalate.

Use other ordinary APIs for control needs:

- `runtime.retry(...)` for retrying a classified transient failure.
- `runtime.parallel_map(...)` for bounded concurrent work over a finite input list. It returns results in input order and follows an explicit failure policy.
- `runtime.checkpoint(...)` to record durable state at safe execution points.
- `runtime.run_durable(...)` to run an async function with persisted progress, resume, cancellation, and trace semantics.

Do not use an AI repair loop to hide provider instability. Do not automatically retry a non-idempotent side effect.

### 5.3 Validation

Validation is ordinary typed functions registered with the runtime. A validator returns a runtime-defined `ValidationResult`, which includes status, kind, findings, evidence references, and whether the failure can be repaired.

Check categories are:

- `RuntimeCheck`: deterministic, machine-executed validation such as type checking, scope checks, schema checks, or actual test commands.
- `ModelEvaluated`: a model's assessment against a rubric; useful but not proof.
- `HumanReviewed`: an explicit human decision.

Required validation fails closed. Missing evidence, unknown status, validator error, unsupported test, or incompatible candidate does not pass. Approval binds to exact candidate hash, contract/API versions, run ID, and relevant action parameters; any material change invalidates it.

## 6. Workspaces, boundaries, and artifacts

### 6.1 Workspace declaration

```ts
export workspace frontend_edit {
  root: ".";
  read: ["src/frontend/**", "src/shared/**"];
  write: ["src/frontend/**"];
  deny: ["src/server/**", "axiom.yaml", "db/migrations/**"];
  isolation: copy_on_write;
  limits: {
    max_files: 200;
    max_file_bytes: 1000000;
    max_changed_files: 30;
  }
}
```

Paths are project-relative patterns constrained by the workspace root. Effective permissions are the intersection of the declaration, base workspace, runtime policy, task-owned files, and operating-system capabilities. Derived workspaces can narrow access, not widen it. Unknown isolation modes fail closed.

Required isolation modes:

- `read_only`: shared immutable view.
- `copy_on_write`: private overlay over a base snapshot; changes remain private until submitted.
- `git_worktree`: isolated Git worktree/branch.
- `exclusive_integration`: a privileged integration lock used only by an authorized integration operation.

The runtime enforces filesystem access; compiler checks alone are not the security boundary. Normalize paths before matching. Prevent root escapes, deny symlink traversal/creation by default, protect secrets and VCS metadata, apply quotas, and validate base revision/conflicts before integration. A patch is a proposal, not permission to modify the shared repository.

For agent-managed source trees, protect `axiom.yaml`, policy/configuration, database migrations, and source files that define permissions by default. Changes touching protected files require separate authorization and human review. Never run a shell string assembled from model output.

### 6.2 Small filesystem and artifact API

Provide typed library APIs rather than additional syntax:

- `fs.list`, `fs.read_text`, `fs.search`, `fs.write_text`, `fs.diff`, `fs.delete`.
- `workspaces.create`, `workspaces.snapshot`, `workspaces.restore`, `workspaces.submit`, `workspaces.discard`, `workspaces.integrate`.
- `artifacts.publish`, `artifacts.fetch`, `artifacts.verify`, `artifacts.inspect`.
- `patches.check_conflicts`, `patches.apply_set`.
- `commands.run("registered_name", args: ...)` for configured command profiles only.

Each API checks effects, path permissions, size/output quotas, and identity. Commands are allowlisted executable plus structured arguments—never arbitrary shell text. Use OS/container sandboxing for command execution; filesystem workspace isolation is not a substitute for process isolation.

## 7. Compiler, targets, and WASM host

### 7.1 One compiler pipeline

Required pipeline:

1. Load source and resolve the module graph.
2. Parse to AST with stable source spans and recoverable diagnostics.
3. Resolve names and types; validate API versions and imports.
4. Check effects, target compatibility, and declared capabilities.
5. Lower into a stable, inspectable typed IR containing types, effects, source provenance, target restrictions, and required capabilities.
6. Compile the same checked IR to server execution and browser WASM.
7. Emit reproducible artifacts, manifests, source maps, and diagnostics.

The two targets must share the parser, type checker, module model, contracts, and IR. Target-specific code generation is allowed; target-specific language semantics are not. An unsupported target feature fails at compile/check time. It cannot be stubbed or silently skipped.

Build only the module graph reachable from each configured entry point. Server-only code is not bundled into WASM. Browser-target code cannot access database connections, server credentials, filesystem/command APIs, secret resolution, or unrestricted networking.

### 7.2 `api` functions define the boundary

An `export api fn` is the only source-level way to publish a server operation. Its input/output types, version, authentication requirement, capability requirement, data class, error mapping, and optional cache policy are represented in a checked API manifest. The server compiles the real function body; the browser compiles a typed client stub that calls through the trusted host bridge.

Do not create a separate `operation` declaration. Do not expose a server handler body to the browser. The server independently checks authentication, authorization, input, tenant scope, data classification, and current policy on every call. A client stub or shared type is not an authorization grant.

The server implementation lives in the API function body, as shown in the complete TeamBoard example in Section 13.2. On `wasm_browser`, importing that API exposes only its checked signature and generated client stub; the compiler must not pull the server handler or its database dependencies into the browser module. The imported stub has the caller-side effect `api.call`; the server body is checked against its server-side effects.

### 7.3 Browser host

WASM runs in a worker or equally isolated context, under explicit memory, execution-time, event-size, node-count, and storage quotas. WASM has no ambient filesystem, network, database, or DOM access. The trusted host exposes a versioned, narrow ABI for:

- Rendering a typed `UiTree` into accessible DOM elements.
- Forwarding typed UI events to exported Axiom functions.
- Calling declared API functions through the authenticated server bridge.
- Managing the declared frontend state store and permitted browser storage.
- Loading version-pinned frontend artifacts and reporting structured runtime errors.

The host may use JavaScript/TypeScript internally to call browser APIs, but must not become a second frontend framework containing application-specific code. Validate all host imports, UI nodes, event payloads, and serialized values. Fail closed on unknown ABI versions or imports.

## 8. Frontend code, state, and caching

### 8.1 Minimal frontend convention

A frontend module exports a small conventional interface rather than a special UI language:

- `init(input) -> State`: creates initial typed state.
- `render(state) -> UiTree`: pure function that describes the UI.
- `handle_event(state, event) -> Result<State, AppError>`: computes a new state and may perform explicitly declared API effects.

These are ordinary Axiom functions. The compiler/manifest identifies the entry-point exports. `UiTree` is a typed data structure built with UI standard-library functions (`ui.column`, `ui.row`, `ui.text`, `ui.button`, `ui.input`, `ui.table`, etc.); it is not HTML or executable markup. The browser host performs actual DOM rendering. Provide accessible controls, semantic structure, responsive layout primitives, theme tokens, loading/error states, and stable keys for repeated nodes.

Example:

```ts
module app.frontend.dashboard;

import { DashboardInput, TimeRange, AppError } from "./shared.ax";
import { get_dashboard } from "./server.ax";

export type DashboardState = {
  team_id: string;
  completed: int;
  range: TimeRange;
  loading: bool;
};

export enum DashboardEventKind { Refresh; SetRange; }

export type DashboardEvent = {
  kind: DashboardEventKind;
  range: Option<TimeRange>;
};

export fn init(input: DashboardInput) -> DashboardState {
  return {
    team_id: input.team_id,
    completed: 0,
    range: TimeRange.Month,
    loading: false
  };
}

export fn render(state: DashboardState) -> UiTree {
  return ui.column([
    ui.heading("Team performance"),
    ui.text("Team: " + state.team_id),
    ui.metric("Completed", state.completed),
    ui.button("Refresh", { kind: DashboardEventKind.Refresh, range: None })
  ]);
}

export async fn handle_event(
  state: DashboardState,
  event: DashboardEvent
) -> Result<DashboardState, AppError>
  effects [api.call] {
  match event.kind {
    DashboardEventKind.Refresh => {
      match await get_dashboard({ team_id: state.team_id }) {
        Ok(summary) => return Ok({
          team_id: state.team_id,
          completed: summary.completed,
          range: state.range,
          loading: false
        });
        Err(error) => return Err(error);
      }
    }
    DashboardEventKind.SetRange => {
      return Ok({
        team_id: state.team_id,
        completed: state.completed,
        range: event.range.unwrap_or(state.range),
        loading: state.loading
      });
    }
  }
}
```

If the final parser uses slightly different enum or `Option` syntax, record the decision in Phase 0 and update this example plus all tests to the actual grammar. This code must ultimately compile and run; it may not remain a pseudocode fixture.

### 8.2 State management: keep it typed and boring

Do not add `state`, `store`, `transition`, or `migration` declarations to the language. State is a normal record type; `init`, `render`, `handle_event`, and optional migration functions are ordinary functions. The runtime owns and validates the state between calls.

Each frontend entry point declares state metadata in `axiom.yaml` or its generated frontend manifest:

- stable logical state ID;
- record type and schema version;
- scope: `component`, `route`, or `application`;
- persistence: `none`, `session`, or `persistent`;
- optional migration function when upgrading a stored state schema.

Start with whole-store persistence policy, not a different persistence policy for every field. Default persistence is `none`. Persist only values that are explicitly approved. Never persist credentials, bearer tokens, secret data, capability grants, authorization decisions, or server-only connection information.

State updates are immutable and validated before commit. Rendering is a pure projection from state to `UiTree`; render does not write to the database or storage. Subscriptions cause affected UI to rerender, without arbitrary repeated effect execution. Define ordering for events and asynchronous responses. Stale responses must not overwrite newer state without an explicit policy. Optimistic updates are opt-in and must roll back/reconcile.

State identity is independent of visual position and component-tree index. Use stable keys for repeated UI nodes and stable logical IDs for meaningful state. Replacing or moving a component must not reset compatible page/application state. Temporary component state may be discarded when the component is truly removed.

### 8.3 State migrations

A migration is an ordinary pure function, configured by `from` and `to` schema versions in the manifest. Do not create a `migration` keyword.

A migration may transform only the supplied state value. It cannot call a model, access network/files/database, or change permissions. Validate and stage the new state before atomically activating the new frontend artifact. If migration fails, keep the old release and state active or require an explicit approved reset; never silently discard user drafts. Rollback must restore a state snapshot compatible with the target release.

Validate state on restoration, including schema/version, size, enums, invariants, and tenant/principal partition. Logout, tenant change, or permission changes clear/quarantine relevant state. A browser state cache is not durable storage for important business records; those belong on an authorized server operation.

### 8.4 Three separate caches

Do not conflate these stores:

1. **Frontend artifact registry:** immutable accepted Axiom source revisions, compiled frontend artifacts, manifests, dependency references, and validation evidence. This defines what a release is.
2. **WASM build cache:** compiled outputs keyed by all relevant source/dependency hashes, compiler/version/options, target, ABI, and contract inputs. This improves compilation speed; it does not mean an artifact is accepted.
3. **Server-data cache:** optional cached API results with freshness and invalidation rules. This is neither UI state nor a permission check.

A frontend release pins exact artifact versions/content hashes for its shell, route pages, shared components, and themes. Do not resolve production references as “latest”. Reuse unchanged artifacts, and rebuild only changed modules and their dependents. A cache hit never bypasses integrity, compatibility, current authorization, revocation, or validation checks.

Server-data cache keys include operation identity/version, canonical input, principal/tenant partition where relevant, and data-policy context. Never use raw tokens as cache keys. Protected operations still authenticate and authorize on every request. Writes invalidate their declared tags/keys. Sensitive data is not cached by default; stale-on-error defaults to false.

### 8.5 Frontend generation and release lifecycle

When a user requests a UI change:

1. Resolve the current accepted frontend and pin its exact artifact/dependency versions.
2. Create an isolated frontend workspace based on that release.
3. Give the frontend agent only the required source scope and tools.
4. Let the agent propose a candidate. It cannot modify the active release or publish directly.
5. Compile and validate source, types, effects, API compatibility, workspace scope, WASM ABI/imports, resource limits, browser tests, and migration behavior where relevant.
6. If repairable checks fail, return structured findings to a bounded agent loop. Otherwise reject/escalate.
7. Publish an immutable candidate artifact and manifest with hashes and evidence.
8. Verify again before an atomic active-release switch. Preserve the previous healthy release for rollback.

The artifact manifest records source hash, compiler version, target/options, dependency hashes, ABI version, exported entry points, capabilities/imports, API contract versions, state schema version, and validator evidence bound to the exact candidate hash. No agent writes the active-release pointer. Rollback rechecks current compatibility and revocation policy; it must not revive an artifact revoked for a security issue.

## 9. Model providers and tools

### 9.1 Providers

Keep provider adapters in Rust, outside the language grammar. `axiom.yaml` names integrations (protocol, endpoint, auth reference, timeout, options) and model aliases. `.ax` references an alias, never hardcoded credentials or provider-specific request syntax.

Support a mock provider for deterministic tests and a documented extension interface. Implement at least one OpenAI-compatible adapter and the provider adapters committed to the project's support matrix. Provider changes must not require rewriting agent definitions. Treat provider responses as untrusted; validate structured output against expected types before using it.

### 9.2 Tools and commands

Tools are registered Rust implementations or safe standard-library functions. Do not create a `tool` declaration. The agent's `tools: [...]` list grants references to registered tools only. For every call, check that the tool exists, the agent declares it, the current runtime authorizes it, and its effects fit the target/workspace/policy.

Command profiles in config contain an executable, structured argument list, working directory, environment allowlist, timeout, output limits, and network policy. Never execute arbitrary commands proposed as shell strings by the model. Use a sandbox where available; disable network by default for test/build commands.

## 10. Database subsystem

Database behavior is a library/runtime feature, not a `query` language declaration. Axiom code calls typed database APIs, for example `db.query<Row>(database: "app", sql: "...", params: {...})` or `db.execute(...)`.

### 10.1 Required database rules

- SQLite for offline deterministic tests and PostgreSQL support in release CI.
- Separate connection roles for read, write, schema/migration, and provisioning. Ordinary agents/tools do not receive administrator credentials.
- Parameterized SQL only. SQL text must be a source string literal or another compile-time constant that passes validation; model-generated SQL text at runtime is forbidden.
- Check SQL statement class, placeholders, parameter names/types, row type mapping, row/affected-row limits, timeout, and allowed schema/table scope. The parser/scanner is defense in depth; actual DB roles and server configuration are the security boundary.
- Enforce data-class rules and allowed integration policy. Never leak credentials, raw connection URLs, bound parameter values, protected schema details, or raw driver messages into agent-visible diagnostics, traces, or artifacts.
- Record audited reads/writes with actor/run ID, operation, query fingerprint, data class, rows affected, and outcome, without persisting secrets or sensitive values.
- Write approval binds to the exact SQL fingerprint, parameter hash, database/role, row limit, candidate/action hash, and expiry. Any change invalidates approval. Use idempotency keys and crash-safe ledger semantics where writes can be retried.
- Migrations run only in a dedicated migration workspace and use reviewed, versioned migration files. Plan and test against a scratch database before applying. Destructive production changes require explicit flag and approval.
- Provisioning is disabled by default, requires separate authority, and is isolated from ordinary agents.
- Reject SSRF/private metadata access in connection setup unless explicitly configured; require TLS verification for remote PostgreSQL.

### 10.2 API surface

Keep the database API small:

- `db.query<T>(database, sql, params, max_rows, timeout)` for reads returning a bounded list.
- `db.query_one<T>(database, sql, params, timeout)` for a required single row, returning `Result<T, DbError>`.
- `db.execute(database, sql, params, max_affected_rows, idempotency_key)` for writes.
- `db.schema.inspect`, `db.migrations.plan`, `db.migrations.apply` for separately authorized schema work.
- `db.scratch.create`, `db.scratch.destroy` for isolated test databases.
- `db.audit_roles` for checking that configured roles have only intended privileges.

Schema/provision/migration APIs have their own effects and are never available to browser modules. `db.execute` does not imply approval; policy determines whether approval is needed.

### 10.3 Required database tests

Test SQL injection strings, multiple-statement rejection, nonliteral/model-produced SQL rejection, placeholder mismatch, unknown tables/columns, result-type mismatch, row/time limits, restricted role enforcement, write approval hash mismatch/expiry/replay, idempotency across crashes, concurrent migrations, destructive migration gates, scratch isolation, secret redaction, SSRF restrictions, TLS failure, and fuzzing of SQL/config parsing. Postgres release tests use `AXIOM_TEST_POSTGRES_URL`; do not make a real database necessary for the offline default test suite.

## 11. Runtime, durability, and errors

Normal async functions can run in a simple process or through `runtime.run_durable`. The runtime owns task IDs, state snapshots, cancellation, checkpoints, event traces, budgets, and resume logic. Durable execution must not require a separate `workflow` language declaration.

Required runtime behavior:

- Typed errors and predictable propagation through `Result`.
- Explicit retry classification; no implicit retry of arbitrary operations.
- Hard caps for model calls, total duration, concurrency, source/output/artifact sizes, memory, and repeated work.
- Finite collection loops, bounded standard-library iteration, call depth, and parallelism. Recursive calls are rejected in 1.0 unless a later version adds an explicit safe bound.
- Idempotency handling for retried external side effects.
- Checkpoints at declared safe boundaries; resume after process restart where supported; cancellation propagates to children.
- Traces link run ID, source revision, redacted config version, agent calls, tool/API calls, workspace changes, artifact hashes, validation evidence, approvals, and final status.
- Secret redaction and configurable retention. No source contents or sensitive data in traces by default.
- Failed validation blocks acceptance. Runtime state—not model prose—determines accepted/rejected/cancelled status.

Do not claim exactly-once semantics for external side effects. Use idempotency keys or document at-least-once behavior.

## 12. Security model

Threat-model at least: malicious/prompt-injected task text; malicious source files; agent attempts to widen permissions; path traversal; symlinks/TOCTOU; command injection; secret exfiltration; corrupted provider responses; forged validator results; cache poisoning; stale/revoked artifacts; ABI/import abuse; oversized UI/state payloads; runaway loops/task spawning; cross-tenant state/cache leakage; concurrent activation races; unsafe migrations; DB injection/overprivilege; resume/cancellation bugs.

Security rules:

- Runtime-owned permissions and validation evidence cannot be forged by agents.
- Least privilege and deny-by-default in workspaces, tools, host imports, databases, and browser storage.
- Compiler checks and static SQL/path checks are defense in depth, not replacements for OS/runtime/DB enforcement.
- A generated frontend cannot access arbitrary network, filesystem, DB, secrets, or ambient DOM APIs.
- No cache is an authority source. No typed contract is itself an authorization grant.
- Artifacts and approvals bind to content hashes and versions. Revalidate at activation time.
- Test malicious inputs and race conditions, not only successful examples.
- Document residual risks honestly in `docs/security-model.md`.

## 13. Reference application

The repository must include a small full-stack TeamBoard application demonstrating all core concepts with runnable tests.

### 13.1 Shared types — `examples/src/shared.ax`

```ts
module examples.shared;

export type DashboardInput = { team_id: string; };
export type DashboardSummary = { team_id: string; completed: int; };
export enum TimeRange { Week; Month; Quarter; }
export enum DashboardEventKind { Refresh; SetRange; }
export type DashboardEvent = {
  kind: DashboardEventKind;
  range: Option<TimeRange>;
};
export type DashboardState = {
  team_id: string;
  completed: int;
  range: TimeRange;
  loading: bool;
};
export enum AppError { NotFound; DatabaseUnavailable; ValidationFailed; Unauthorized; }
export type FrontendChange = { request: string; };
export type FrontendPatch = { candidate_id: string; workspace_id: string; };
export type FrontendRelease = { release_id: string; content_hash: string; };
```

### 13.2 Backend — `examples/src/server.ax`

```ts
module examples.server;

import { DashboardInput, DashboardSummary, AppError } from "./shared.ax";

export api async fn get_dashboard(
  input: DashboardInput
) -> Result<DashboardSummary, AppError>
  effects [db.read] {
  match await db.query_one<DashboardSummary>(
    database: "teamboard",
    sql: "SELECT team_id, completed FROM v_team_performance WHERE team_id = :team_id LIMIT 1",
    params: { team_id: input.team_id },
    timeout: 5s
  ) {
    Ok(summary) => return Ok(summary);
    Err(_) => return Err(AppError.DatabaseUnavailable);
  }
}
```

The server must enforce authentication and `dashboard:read` authorization before returning team data. That enforcement belongs to runtime policy attached to the exported API manifest, not to a hidden client-side condition.

### 13.3 Frontend — `examples/src/frontend.ax`

```ts
module examples.frontend;

import { DashboardInput, DashboardSummary, DashboardState, DashboardEvent, DashboardEventKind, TimeRange, AppError } from "./shared.ax";
import { get_dashboard } from "./server.ax";

export fn init(input: DashboardInput) -> DashboardState {
  return {
    team_id: input.team_id,
    completed: 0,
    range: TimeRange.Month,
    loading: false
  };
}

export fn render(state: DashboardState) -> UiTree {
  return ui.column([
    ui.heading("TeamBoard"),
    ui.text("Team: " + state.team_id),
    ui.metric("Completed tasks", state.completed),
    ui.text("Range: " + state.range.to_string()),
    ui.button("Refresh", { kind: DashboardEventKind.Refresh, range: None })
  ]);
}

export async fn handle_event(
  state: DashboardState,
  event: DashboardEvent
) -> Result<DashboardState, AppError>
  effects [api.call] {
  match event.kind {
    DashboardEventKind.Refresh => {
      match await get_dashboard({ team_id: state.team_id }) {
        Ok(summary) => return Ok({
          team_id: state.team_id,
          completed: summary.completed,
          range: state.range,
          loading: false
        });
        Err(error) => return Err(error);
      }
    }
    DashboardEventKind.SetRange => {
      return Ok({
        team_id: state.team_id,
        completed: state.completed,
        range: event.range.unwrap_or(state.range),
        loading: state.loading
      });
    }
  }
}
```

The browser compiler must compile the `get_dashboard` call as a typed API stub only. The server handler and DB dependency must not be present in the WASM artifact.

### 13.4 Agent and workspace — `examples/src/agents.ax`

```ts
module examples.agents;

import { AppError, FrontendChange, FrontendPatch, FrontendRelease } from "./shared.ax";

export workspace frontend_edit {
  root: ".";
  read: ["examples/src/frontend.ax", "examples/src/shared.ax"];
  write: ["examples/src/frontend.ax"];
  deny: ["examples/src/server.ax", "axiom.yaml", "db/migrations/**"];
  isolation: copy_on_write;
}

export agent FrontendDesigner(input: FrontendChange) -> FrontendPatch {
  model: "coding";
  workspace: frontend_edit;
  tools: [fs.read, fs.search, fs.write, compiler.check];
  limits: { max_calls: 6; timeout: 120s; max_output_bytes: 1000000; }
}

export async fn update_frontend(
  input: FrontendChange
) -> Result<FrontendRelease, AppError>
  effects [model, fs.read, fs.write, workspace.create, command.run, artifact.publish] {
  let candidate = await agents.iterate(
    agent: FrontendDesigner,
    input: input,
    max_attempts: 3,
    validate: [checks.ax_compile, checks.wasm_build, checks.api_compatibility, checks.workspace_scope, checks.browser_tests]
  );
  match candidate.accepted_artifact() {
    Ok(artifact) => return await frontend.publish(artifact);
    Err(_) => return Err(AppError.ValidationFailed);
  }
}
```

The runtime must make validation evidence immutable and bind it to the exact candidate/artifact hashes. `frontend.publish` verifies current authority and compatibility and atomically swaps the release pointer only after required validators pass. The agent cannot call the active-release swap directly.

### 13.5 Manifest and state

The browser entry point in `axiom.yaml` identifies the module and conventional exports:

```yaml
compilation:
  entrypoints:
    wasm_browser:
      module: examples.frontend
      init: init
      render: render
      handle_event: handle_event

frontend_state:
  id: teamboard.dashboard
  type: examples.shared.DashboardState
  schema_version: 1
  scope: route
  persistence: session
```

Frontend registry fixtures must use actual generated content hashes; placeholders may appear only in explanatory documentation. The release pins the shell, route/page modules, shared components, and their dependency hashes. Changing one page must reuse unchanged artifacts where compatible.

### 13.6 Required end-to-end tests

- Compile/type-check the complete reference app for both targets.
- Execute the backend test with a mock DB and verify authentication/authorization behavior.
- Verify the browser WASM makes a typed host API call and renders the returned data in a real browser test.
- Inspect the WASM imports and artifact to ensure server handler code, DB access, credentials, and forbidden host imports are absent.
- Generate a frontend candidate, force a validation failure, and prove a bounded repair attempt receives structured findings.
- Prove exhausted or invalid candidates do not publish.
- Prove accepted releases are immutable and pinned, and unchanged pages/build outputs are reused.
- Prove state survives layout-only replacement; incompatible state uses a checked migration or blocks activation; rollback restores compatible state.
- Prove one principal/tenant cannot read another's cached results or persisted client state.
- Prove revoked artifacts, stale approvals, changed hashes, or ABI mismatches block activation.

The exact fixture syntax must match the real grammar. Do not keep examples that cannot compile.

## 14. Developer experience and CLI

Provide:

- `axiom init`
- `axiom check` and `axiom config check`
- `axiom fmt` (deterministic; with `--check`)
- `axiom lint`
- `axiom explain` / typed IR inspection
- `axiom build --target server|wasm_browser`
- `axiom run`, `axiom test`, and `axiom dev`
- `axiom status`, `axiom inspect`, `axiom resume`, `axiom cancel`
- `axiom artifacts inspect|verify|list`
- database audit/plan/schema-check commands
- LSP diagnostics, completion, go-to-definition, references, hover/types, formatting, and symbol outline

Do not add CLI commands for concepts that can be handled by one predictable command with a subcommand. Use stable machine-readable diagnostics and error codes.

## 15. Implementation phases

Every phase ends with a compiling repository and passing relevant tests. Update the implementation ledger at every gate.

| Phase | Work | Evidence required |
|---|---|---|
| 0 | Finalize the small grammar, core types, effects, IR, API modifier, workspace model, target matrix, browser ABI, threat model | Grammar, positive/negative fixtures, type/effect examples, no duplicate syntax for runtime concepts |
| 1 | Lexer, parser, AST, source maps, diagnostics, formatter skeleton | Valid/invalid fixtures; fuzzing never panics |
| 2 | Module resolution, types, generics, `Option`/`Result`, effect and target checking | All documented type/effect/target errors rejected |
| 3 | Typed IR and deterministic server evaluator; basic compiler commands | Inspectable stable IR; no model required for deterministic examples |
| 4 | WASM lowering, UI tree, host ABI, browser loader/renderer | Real browser smoke tests; forbidden imports rejected; reproducible build fixture |
| 5 | Config, model integrations, registered tools and commands | Mock-provider tests; no secrets in output; model alias switch requires no source edits |
| 6 | Filesystem, workspaces, patch/artifact APIs and command sandbox | Path/security/property tests; isolation and integration gates pass |
| 7 | Database APIs, roles, parameter checks, approval/audit and migrations | Offline SQLite suite and gated Postgres release suite; security tests pass |
| 8 | Agent calls, iteration/validation APIs, budgets, typed results | Pass-first, repair-pass, exhaustion, forged-evidence rejection tests |
| 9 | Frontend state, state validation/migration, artifact registry and build/data caches | Hot-swap, invalidation, cache-isolation, reproducibility and rollback tests |
| 10 | Durable async execution, checkpoints, resume, cancellation, tracing, approvals | Crash/resume and cancellation tests; no duplicated non-idempotent action without idempotency handling |
| 11 | Full-stack reference app, tooling, docs, security hardening | Clean-checkout build; all examples compile; property/fuzz/security/concurrency tests |
| 12 | Release-readiness audit | Every Definition-of-Done item has evidence in `docs/release-readiness.md` |

## 16. Suggested Rust workspace layout

```text
axiom/
  crates/
    axiom-source/       # lexer/parser/AST/source maps
    axiom-diagnostics/  # stable errors and spans
    axiom-types/        # types, inference, effects, target checks
    axiom-ir/           # shared typed IR
    axiom-config/       # strict axiom.yaml schema and secret references
    axiom-compiler/     # target pipeline and code generation
    axiom-wasm/         # browser WASM backend and artifact manifest
    axiom-browser-host/ # trusted host ABI/loader integration
    axiom-providers/    # provider-neutral types and adapter registry
    axiom-runtime/      # evaluation, agents, budgets, errors
    axiom-state/        # durable runs, UI-state validation/migrations
    axiom-filesystem/   # secure workspaces and patches
    axiom-artifacts/    # immutable artifacts, registry, cache
    axiom-tools/        # safe registered tools/commands
    axiom-database/     # DB roles, typed calls, migrations/audit
    axiom-stdlib/       # UI, API, agents, validation, runtime, fs, db APIs
    axiom-format/
    axiom-lsp/
    axiom-cli/
  browser/              # trusted minimal browser shell/host
  examples/             # runnable TeamBoard reference app
  tests/                # unit, integration, conformance, browser, security
  docs/                 # language spec, grammar, architecture, API, security, tooling
```

Keep dependency direction acyclic. The type system and IR do not depend on provider adapters. Providers do not depend on the parser. The database crate does not depend on model adapters. The browser host does not implement app-specific business logic.

## 17. Engineering rules

- Run `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace`; record justified exceptions.
- Default CI is deterministic and offline. External API tests are separately gated.
- No panics on user source, configuration, model output, patches, provider responses, artifact manifests, or restored state.
- Do not swallow errors or convert a failed provider/tool/check into apparent success.
- Document language versioning and compatibility. Reject unsupported versions clearly.
- Never describe a model judgment as deterministic proof. Never describe WASM emission alone as application validation.
- Every supported language construct must have grammar, type/effect semantics, IR representation, runtime/compiler implementation, docs, and tests.
- Keep the public language small. Add a syntax construct only when a normal function, type, library API, or manifest setting cannot express it clearly and safely.

## 18. Definition of Done

Axiom 1.0 is complete only when all requirements below are evidenced:

1. The language specification, EBNF, parser, formatter, type checker, effect checker, IR, and examples agree.
2. The public language has only the small declaration, expression, `if`/`match`/finite `for` control flow, `effects`, `async`/`await`, `api fn`, `agent`, and `workspace` forms defined in Section 3; runtime services do not add hidden grammar or subsystem-specific declaration keywords.
3. Shared typed IR compiles and runs on the `server` and `wasm_browser` targets for all constructs marked portable.
4. Server API functions compile to secure server handlers and typed browser stubs, with server authorization on every protected request.
5. Agents run through registered tools and bounded run-wide budgets; agents cannot widen their own permissions.
6. Validation evidence is runtime-owned and bound to exact candidate hashes; unaccepted work cannot publish or integrate.
7. Workspace isolation, path restrictions, command sandboxing, patch conflict checks, and artifact immutability pass security tests.
8. Frontend state is typed, versioned, validated, stable across layout-only changes, and migrated or safely rejected across schema changes.
9. Artifact registry, WASM build cache, and server-data cache remain separate, integrity-checked, version-aware, and unable to bypass authorization.
10. A frontend generated by an agent compiles to WASM, passes actual browser tests, can be atomically published, and can be rolled back.
11. Database access is parameterized and typed, roles are least-privilege, approval/audit/migration rules are tested, and secrets are redacted.
12. Durable runs support trace, budgets, cancellation, and tested resume semantics without falsely claiming exactly-once side effects.
13. Security, fuzz, property, concurrency, and integration tests pass; clean-checkout CI succeeds.
14. All shipped examples parse, type-check, compile for their declared targets, and run in tests.
15. `docs/release-readiness.md` lists commands/results, supported providers/platforms/features, known limitations, and residual risks.

**Final rule:** Axiom should feel like a small programming language with a powerful, secure runtime—not a collection of special-purpose mini-languages. Keep the syntax small, put reusable behavior in typed APIs, and make authority and acceptance enforceable by the runtime.
