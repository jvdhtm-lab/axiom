# Axiom 1.0 — Implementation Prompt

## 1. Mission

Build **Axiom**, a TypeScript-inspired domain-specific language for defining AI agents, contracts, tools, policies, and hybrid deterministic/agentic workflows. Implement its compiler and runtime in Rust.

Axiom combines:

- **Procedural control** for deterministic steps, branching, retries, and security boundaries.
- **Agentic control** for planning, interpretation, bounded decisions, and delegated work.
- **Typed contracts** so agents exchange validated inputs, outputs, artifacts, and verification evidence.
- **Model portability** so an agent's definition never changes when its model, provider, or endpoint changes.

Axiom is not a chatbot, a prompt-template wrapper, or a clone of TypeScript. It is a complete language *for its domain*: multi-module programs with types, functions, expressions, agents, tools, policies, workspaces, and workflows. The DSL never executes arbitrary Rust, JavaScript, or shell text.

**Scope rule.** Everything in this document is in scope for 1.0. Nothing is "optional" or "future work." If you cannot finish in one session, continue through the phases in order, keep the ledger truthful, and never call 1.0 complete until the Definition of Done passes.

## 2. How to work

1. Inspect the repository first. Create `docs/implementation-ledger.md` with columns: requirement, phase, status (implemented/tested, partial, unsupported), test evidence, docs, known limitations.
2. Work phase by phase (Section 14). Every phase ends with a compiling repo and passing tests.
3. When blocked, make the smallest conservative decision, record it as an ADR in `docs/decisions/`, and continue. Ask the user only if safe progress is genuinely impossible.
4. Update spec, grammar, fixtures, examples, and code in the same change. A construct that is documented but lacks parser, type-checker, and runtime support is a bug.
5. Check current official provider documentation before fixing endpoints, and record verification dates in `docs/providers.md`. Choose dependencies from current stable documentation; record the rationale and an update policy.
6. Never claim a feature is complete unless it is implemented and verified.

## 3. Core concepts (never collapse these into one `model` field)

| Concept | Meaning | Lives in |
|---|---|---|
| **Adapter** | Code that speaks one API protocol (`anthropic_messages`, `openai_compatible`, …) | Rust |
| **Integration** | A configured connection: adapter + endpoint + auth reference + timeouts + options | `axiom.yaml` |
| **Model profile** | A user-defined alias (`reasoning`, `coding`, …) → integration + opaque model ID | `axiom.yaml` |
| **Agent definition** | Purpose, typed input/output, tools, workspace, constraints, limits | `.ax` |
| **Agent instance** | An agent bound to a profile, with safe runtime overrides | `.ax` |
| **Contract / goal / task** | Outcome, bounded work, invariants, acceptance checks | `.ax` |
| **Workflow** | Sequencing, parallelism, delegation, retries, verification, escalation | `.ax` |

Profile names are user-defined labels, not claims about model ability. Model IDs are opaque strings; never hardcode an allowlist. Declared capabilities are expectations, not proof.

## 4. Source/config split

- **`*.ax`**: types, functions, agents, teams, tools, contracts, goals, policies, workspaces, workflows. Refers to model profiles by alias only. Never contains credentials, endpoints, or environment-specific paths.
- **`axiom.yaml`**: project manifest, integrations, model profiles, defaults, runtime limits, command profiles, filesystem limits, logging.

Canonical YAML (use `integrations:` and `models:` everywhere; a model profile references its connection with the key `integration`):

```yaml
version: 1                    # config schema version
language: "1.0"               # Axiom language version; unsupported versions are rejected
sources: ["src/**/*.ax"]      # module search set

integrations:
  claude_api:
    adapter: anthropic_messages
    base_url: "${ANTHROPIC_BASE_URL:-https://api.anthropic.com}"
    api_key: "${ANTHROPIC_API_KEY}"
    timeout_seconds: 120
  deepseek_api:
    adapter: openai_compatible
    dialect: deepseek         # enables documented DeepSeek-specific request/response fields
    base_url: "${DEEPSEEK_BASE_URL:-https://api.deepseek.com}"
    api_key: "${DEEPSEEK_API_KEY}"
  kimi_api:
    adapter: openai_compatible
    dialect: moonshot
    base_url: "${KIMI_BASE_URL:-https://api.moonshot.ai/v1}"
    api_key: "${KIMI_API_KEY}"
  local_models:
    adapter: openai_compatible        # dialect defaults to generic
    base_url: "${AXIOM_LOCAL_BASE_URL:-http://localhost:11434/v1}"
    api_key: "${AXIOM_LOCAL_API_KEY:-}"
    api_key_optional: true
    timeout_seconds: 180

models:
  reasoning:    { integration: claude_api,   model: "${CLAUDE_MODEL}",   capabilities: [chat, structured_output, tool_calling] }
  coding:       { integration: deepseek_api, model: "${DEEPSEEK_MODEL}", capabilities: [chat, tool_calling] }
  review:       { integration: kimi_api,     model: "${KIMI_MODEL}",     capabilities: [chat] }
  local_coding: { integration: local_models, model: "${LOCAL_MODEL}",    capabilities: [chat] }

runtime:
  default_model: reasoning
  max_parallel_agents: 4
  max_retries: 2
  request_timeout_seconds: 120

filesystem:
  project_root: "."                       # the value of the built-in `project` root in .ax
  default_agent_isolation: per_task_copy_on_write
  max_read_bytes: 2000000
  max_write_bytes: 1000000
  max_files_per_task: 500
  protect_vcs_metadata: true

commands:
  cargo_check:
    executable: cargo
    args: ["check", "--offline", "--workspace", "--all-targets"]
    working_directory: "."
    timeout_seconds: 300
    network: false
  unit_tests:
    executable: cargo
    args: ["test", "--offline", "--workspace", "--lib"]
    working_directory: "."
    timeout_seconds: 600
    network: false
  integration_tests:
    executable: cargo
    args: ["test", "--offline", "--workspace", "--tests"]
    working_directory: "."
    timeout_seconds: 600
    network: false
  full_ci:
    executable: cargo
    args: ["test", "--offline", "--workspace", "--all-features"]
    working_directory: "."
    timeout_seconds: 900
    network: false
```

Config rules:

- Implement `${VAR}` and `${VAR:-default}` expansion explicitly; do not assume the YAML parser does it.
- Reject unknown fields, unknown YAML tags, and executable expressions. Never silently discard anything.
- Secrets resolve at runtime and are redacted from logs, diagnostics, IR dumps, traces, snapshots, artifacts, and errors.
- `axiom config check axiom.yaml` detects missing integrations, unknown aliases, invalid adapter settings, missing required variables, unknown dialects, and incompatible capabilities before any model call.
- **Source/config cross-validation:** literal model aliases (`model: "coding"`) and literal command names (`commands.run("unit_tests", ...)`) in `.ax` are checked against `axiom.yaml` at check time.
- **Model precedence:** per-invocation override > agent-instance override > agent declaration > project default > runtime default. Test it. Profile aliases resolve at runtime, so changing a profile never requires editing or recompiling agent source.
- **Limit precedence:** YAML limits and `.ax` limits combine by taking the **minimum**. YAML or `.ax` settings may narrow access but never widen it; contradictory or unsafe combinations are rejected, not silently resolved.

## 5. Language definition

### 5.1 Lexical and syntax conventions (decided)

- Statements and member entries end with `;`. Newlines are insignificant whitespace.
- Blocks use `{ }`. Named arguments use `name: value` everywhere. Member blocks inside declarations use `name { key: value; ... }` (for example `constraints { ... }` and `limits { ... }`).
- Every asynchronous or effectful operation (model, filesystem, artifact, command, workspace) must be explicitly `await`ed.
- `delegate`, `parallel for`, and `await` are **expressions**; an expression statement covers statement use.
- Literals: strings with documented escapes, `int`, `float`, `bool`, `null`, durations (`30s`, `5m`), timestamps, and size literals (`2MB` = 10^6 bytes, `2MiB` = 2^20 bytes).
- Reserved words: `module import export type interface enum fn pure let mut const agent team tool contract goal policy workspace workflow delegate parallel for in while if else match return yield break continue throw try catch finally require retry timeout on_error checkpoint emit escalate await true false null`.
- Property names inside declaration blocks (`root`, `base`, `isolation`, `read`, `write`, `deny`, `limits`, `model`, `tools`, `version`, `effects`, `max`) are contextual, not reserved. Provider names, model aliases, path strings, and enum members are never reserved.
- Standard-library namespaces must not collide with reserved words. Canonical names: `core collections json time env secrets fs workspaces artifacts patches commands agents workflows trace`.
- The spec must contain the complete operator precedence and associativity table, comment grammar, and string escape grammar.

### 5.2 Modules

Multiple files; `module` declarations; relative imports (`import { A, B } from "./x.ax";`); named exports; deterministic resolution identical on Windows, macOS, and Linux; cycle and duplicate-name diagnostics; no code execution at import; stable source locations in all diagnostics and traces.

### 5.3 Types

- Primitives: `string bool int float bytes duration timestamp null`.
- Lists `T[]`, maps `Map<K, V>`, tuples `(A, B)`, records/interfaces, enums, optional fields (`name?: T`), unions (`A | B`), string-literal types (`"a" | "b"`), aliases, generics for types and functions, local inference, explicit function/agent/workflow signatures.
- Tuple literals `(a, b)` are disambiguated from parenthesized expressions and lambdas by lookahead; the spec states the rule and conformance tests cover it.
- Built-in `Option<T>` and `Result<T, E>`; `match` with exhaustiveness checking.
- Dynamic data is the explicit type `JsonValue`, converted to typed values only through validating parse functions. No implicit `any`; no silent coercion of model output.
- Built-in types expose documented, typed methods (for example `T[].map`, `T[].all`, `T[].is_empty()`).
- Always distinguish compile-time guarantees, runtime schema validation, runtime policy enforcement, and model-evaluated claims. Natural-language constraints are never described as formally verified.

### 5.4 Declarations and statements

- **Declarations:** `type interface enum const fn agent team tool contract goal policy workspace workflow`, each optionally prefixed by `export`. Functions are pure by default when marked `pure`; otherwise they declare effects with `effects [ ... ]` after the return type.
- **Expressions:** literals, lists, objects, member/index access, calls (positional then named arguments), unary/binary/conditional operators, lambdas, option handling, `<agent>.with { ... }` instance overrides.
- **Bindings:** `let` is immutable, `let mut` is mutable, `const` is a compile-time constant. Assignment is only legal to `let mut`.
- **Control flow:** `if/else`, `match`, `for x in xs { }`, `while cond max N { }` (the `max` bound is mandatory), `return`, `break`, `continue`, `throw`, `try/catch/finally`.
- **Workflow statements:**
  - `require expr;` and `require expr else escalate expr;` — failure blocks progress; `escalate` pauses for a human or named handler.
  - `retry N { ... }` — retries the block only on errors classified retryable; backoff comes from runtime config.
  - `timeout D { ... }` — cancels the block after duration `D`.
  - `on_error(e: ErrorType) { ... }` — at most one per workflow body, last in the body; handles uncaught errors of that type. `try/catch` handles local errors.
  - `checkpoint "label";` and `emit expr;`.
  - `parallel for x in xs [max N] { ... yield value; }` — evaluates to `T[]` in input order. `yield` produces the element; `return` is illegal inside a parallel body. Default failure policy is fail-fast: cancel siblings, propagate the error. `max N` can only lower the runtime concurrency cap.
  - `delegate Agent.execute(input, context: ..., workspace: ..., options: ...)` (see 5.6). It returns the agent's declared output type on success and throws a typed error on failure.
- **Errors:** typed error kinds `ModelOutputInvalid`, `PolicyViolation`, `ProviderError`, `ToolError`, `VerificationFailed`, `Timeout`, `Cancelled`, `Escalated`. Retryable vs permanent classification is part of each kind's definition.
- Deterministic evaluation order, scope rules, error propagation, cancellation semantics, and hard limits on loops, recursion depth, call depth, steps, and wall-clock time.
- `docs/grammar.ebnf` must cover all lexical rules, patterns, effect clauses, generic constraints, destructuring, and member blocks, and must match the parser exactly. Any sugar needs a documented, tested desugaring into IR.

### 5.5 Effects and capabilities

- Effect names: `fs.read fs.write fs.delete artifact model command workspace network integration`.
- A pure function cannot call effectful code; check statically where possible.
- Distinguish *declared*, *available to this agent*, and *authorized in this execution context*. At runtime, intersect declared effects with the tool registry, workspace permissions, config policy, and invocation authority. A model response can never widen permissions.

### 5.6 Agents, teams, goals, tasks, contracts, policies, tools

- **Agent:** `purpose`, `model` alias (optional; falls back per precedence), `tools`, optional `workspace`, `constraints { must; must_not; }`, limits. An agent with no `workspace` has no filesystem access.
- **`Agent<I, O>`** is a built-in interface. Its single delegation entry point in 1.0 is `execute(input: I, context?: Map<string, JsonValue>, workspace?: WorkspaceRef, options?: InvokeOptions) -> O`, where `InvokeOptions = { model?: string, timeout?: duration, max_retries?: int }` (the per-invocation override layer).
- `context` is serialized into the prompt as **untrusted data**, never as instructions.
- **Workspace rule:** an agent's declared `workspace` is its maximum authority. A call-time `workspace:` argument must be an instance whose effective permissions are a subset of it; otherwise the call is rejected.
- **Instances:** `Agent.with { model: "...", ... }` may override profile and safe runtime parameters; it cannot widen tools, workspace, or policy unless an authorized policy explicitly allows it.
- **Team:** `members`, `routing`, `policy`. Routing maps an input type to a member (`FeatureSpec -> Architect;`); ambiguous routes are compile errors.
- **Goal:** `objective`, typed inputs/outputs, `constraints`, `completion` (contracts that must hold), `decomposition` strategy, `authority` (team or human handler), and the `workflow` that implements it.
- **Task:** stable ID, dependencies, input/output contracts, owned paths/artifacts, budgets, states `pending running accepted rejected blocked`. Only the runtime changes task state; a model can never mark its own task accepted.
- **Contract:** `version`, `subject` type, `precondition`/`postcondition`/`invariant` clauses, and `criterion` entries each labeled with a `CheckKind` (`RuntimeCheck`, `ModelEvaluated`, `HumanReviewed`). `value.satisfies(Contract)` evaluates only runtime-checkable clauses; model-evaluated and human-reviewed criteria are routed through review gates and reported separately, never presented as proven. Contract changes are proposed and versioned; a running task cannot silently continue under changed assumptions.
- **Policy:** tool access, filesystem scopes, human approval, budgets, network/command limits, retention, data handling. Every policy exposes `accepts_paths(paths) -> bool` derived from its path rules.
- **Tool:** the `.ax` declaration holds signature, `effects`, and metadata (`timeout`, `idempotent`, `max_output_bytes`, `binding`). `binding: "rust:<registered_name>"` links to a Rust registry entry; an unresolved binding is a config-check error. A tool declaration never contains executable code.

### 5.7 Example declarations (`examples/src/team.ax`; complete, acyclic imports)

```ts
module examples.team;

import { FeatureSpec, ImplementationTask, PatchArtifact, WorkflowResult } from "./types.ax";
import { Architect, Implementer, Reviewer } from "./agents.ax";
import { SourceChangePolicy } from "./workspaces.ax";
import { RepositoryReportContract } from "./contracts.ax";
import { ImplementFeature } from "./filesystem_feature_workflow.ax";

pure fn is_source_path(path: string) -> bool {
  return path.starts_with("src/");
}

export tool lint_summary(path: string) -> Result<string, ToolError> effects [fs.read] {
  timeout: 30s;
  idempotent: true;
  max_output_bytes: 1MB;
  binding: "rust:example_tools::lint_summary";
}

export team Delivery {
  members: [Architect, Implementer, Reviewer];
  routing {
    FeatureSpec -> Architect;
    ImplementationTask -> Implementer;
    PatchArtifact -> Reviewer;
  }
  policy: SourceChangePolicy;
}

export goal ShipFeature(spec: FeatureSpec) -> WorkflowResult {
  objective: "Implement the requested feature safely and with verification evidence";
  completion: [RepositoryReportContract];
  decomposition: DecompositionStrategy.ArchitectPlan;
  authority: Delivery;
  workflow: ImplementFeature;
}
```

Built-in enums available without declaration: `WorkspaceIsolation`, `CheckKind` (`RuntimeCheck`, `ModelEvaluated`, `HumanReviewed`), and `DecompositionStrategy` (`ArchitectPlan`, `Static`, `Delegated`). Built-in error type: `ToolError`.

## 6. Model integration

- **Provider-neutral** request, response, and error types; preserve raw provider metadata where safe; document normalization losses.
- **Three strategies:**
  1. Protocol adapters: `openai_compatible` (Chat Completions and, where implemented, Responses).
  2. Native adapters where semantics differ materially: `anthropic_messages`.
  3. Gateway use (OpenRouter, LiteLLM) through supported protocols, with `dialect` values for gateway-specific fields.
  Plus a deterministic `mock` adapter.
- **DeepSeek and Moonshot/Kimi** are served by `openai_compatible` with the `deepseek` and `moonshot` dialects and an adapter-validated `extensions:` namespace. Phase 0 verifies against current official documentation whether either needs a separate native adapter; if so, record the decision in an ADR and add the crate. Either way, each has its own contract tests.
- Configuration-only support for any endpoint that follows a supported protocol. A new protocol requires a documented in-process Rust adapter (feature-gated crate). Never load downloaded dynamic libraries.
- Keep the adapter trait small and testable (illustrative):

```rust
#[async_trait::async_trait]
pub trait ModelAdapter: Send + Sync {
    fn adapter_kind(&self) -> &str;
    fn capabilities(&self, model_id: &str) -> ModelCapabilities;
    async fn generate(
        &self,
        integration: &ResolvedIntegration,
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> Result<ModelResponse, ModelAdapterError>;
}
```

- A capability matrix per adapter/dialect/model covers chat, structured output, tool calling, streaming, vision, embeddings, and reasoning controls. Unsupported features fail explicitly and as early as possible. Do not claim a feature for a vendor without a test covering its real protocol behavior.
- Vendor-specific options go in the typed `extensions:` namespace; unknown options are rejected.
- Timeouts, cancellation, rate-limit handling, retry classification (retryable provider failure vs invalid output vs policy violation vs permanent error), response-size limits, usage metadata, request IDs, optional profile fallback, and optional capability/cost/latency routing, all traced.
- Tests use local mock HTTP servers. Real credentials are never needed for CI.

## 7. Typed outputs

Decode and validate every model output against the requested schema; successful parsing is not validation. Return structured errors. Bounded repair/retry only when explicitly configured. Agent-produced plans are untrusted until validated against contracts, depth limits, dependency rules, allowed tools, and budgets.

## 8. Filesystem, workspaces, artifacts

### 8.1 Four separate concepts

1. **Project filesystem:** the repository, addressed through the configured project root.
2. **Task workspace:** an isolated working view per agent or task.
3. **Artifact store:** patches, plans, reports, reviews, logs, release records, with stable IDs, content hashes, provenance, task IDs, contract versions, size limits, retention, and access policy.
4. **Runtime state store:** task state and checkpoints, logically separate from source files.

### 8.2 Workspace values, roots, and isolation

**Types.**

- `WorkspaceRoot` is a built-in opaque type. The prelude provides exactly one constant of this type, **`project`**, whose value is the directory configured by `filesystem.project_root` in `axiom.yaml`. `project` is a prelude constant, not a reserved word; a local declaration that shadows it is a lint error. A typo such as `root: projcet;` is an unresolved-name error.
- `WorkspaceRef` is the type of every value accepted as the first argument to `fs.*`, `commands.run`, and `workspaces.*`. A `workspace` declaration evaluates to a `WorkspaceRef`, as does the result of `workspaces.create`.
- A `SharedReadOnly` or `ExclusiveIntegration` declaration is bound at run start to an immutable snapshot (or, for integration, to the live project under the exclusive lock). The snapshot's **base revision** (a content-hash manifest, or a commit ID for the Git backend) is recorded in the trace.

**Enum** (built in; projects do not redeclare it):

```ts
enum WorkspaceIsolation {
  SharedReadOnly,
  PerTaskCopyOnWrite,
  PerTaskGitWorktree,
  ExclusiveIntegration
}
```

Use it as `WorkspaceIsolation.SharedReadOnly`. YAML uses `shared_read_only`, `per_task_copy_on_write`, `per_task_git_worktree`, `exclusive_integration`. A bare `shared_read_only` in `.ax` is an unresolved name.

**Semantics.**

- `SharedReadOnly`: many readers on an immutable snapshot; no mutation.
- `PerTaskCopyOnWrite`: a private writable overlay on a base snapshot; reads check the overlay first, then the base; writes stay private until exported as a patch.
- `PerTaskGitWorktree`: its own worktree and branch; no shared index or working tree.
- `ExclusiveIntegration`: one operation holds write authority while validating and applying approved patches. This never authorizes ordinary agents to bypass the integration gate.

Reject unsupported modes; never treat two modes as equivalent.

### 8.3 Workspace declaration properties

| Property | Type | Rule |
|---|---|---|
| `root` | `WorkspaceRoot` | Anchors a top-level workspace. Exactly one of `root` or `base` is required. |
| `base` | workspace name | Derives a workspace from another one. |
| `isolation` | `WorkspaceIsolation` | With `root`: only `SharedReadOnly` or `ExclusiveIntegration`. With `base`: only `PerTaskCopyOnWrite` or `PerTaskGitWorktree`. |
| `read`, `write`, `deny` | `string[]` globs | Relative to the **workspace root**; diagnostics print the workspace-relative path. `write` must be a subset of `read`. |
| `limits { }` | size/count limits | `max_file_bytes`, `max_total_read_bytes`, `max_total_write_bytes`, `max_files`, `max_files_changed`. Combined with YAML limits by minimum. |
| `write_authority` | `string[]` | Only valid with `ExclusiveIntegration`. Names the privileged operations allowed to write (for example `"patches.apply_set"`). |

Derived workspaces inherit the base's `deny` list and may add to it but never remove from it. A derived workspace's `read` must be a subset of the base's `read`.

### 8.4 Backends and enforcement

- Backends: rooted local filesystem, per-task overlay, Git worktree, in-memory (deterministic tests).
- Every workspace has an explicit root. Reject `..`, unexpected absolute paths, and escapes. Normalize and validate paths before matching rules; never rely on string-prefix containment. Use capability-oriented filesystem APIs. No symlink creation or following by default. Document remaining TOCTOU limits.
- Deny by default outside declared scopes. Protected by default: `.env`, private keys, credentials, `.git/**`, runtime secret files.
- Separate permissions for read, list, search, write, patch, and delete. Delete is off unless explicitly granted. Permissions are never inferred from model output.
- **Effective write scope** = declared workspace permissions ∩ runtime policy ∩ task-owned paths ∩ backend capabilities.
- Quotas: file size, file count, total read/write bytes, diff size, search result size, path depth, tool duration.
- Atomic writes where practical. Deterministic unified-diff format and stable file ordering. Documented text/binary and newline rules. Audited operations that never record secret contents.
- Every patch is validated, base-revision-checked, and conflict-checked before it touches the shared project.

### 8.5 Filesystem API

- `fs`: `list walk stat exists read_text read_bytes read_many_text search hash write_text write_bytes create_dir apply_patch diff move delete`.
- `workspaces`: `create fork inspect submit discard snapshot restore integrate`. `workspaces.create(base:, task_id:, template:, isolation:)` requires `isolation` to equal the template's isolation; a mismatch is an error.
- `artifacts`: `publish publish_patch publish_verification publish_release fetch verify query`.
- `patches`: `detect_conflicts apply_set`.
- Built-in result types: `DirEntry`, `SearchResult` (with `top_paths(n)`), `CommandResult` (`passed`, `exit_code`, `duration`, log artifact IDs), `ConflictReport` (`conflicts`, `is_empty()`), `IntegrationResult` (`candidate_workspace`, applied patch IDs, base revision), `ArtifactRef`.
- **Two call forms, one policy.** Deterministic workflow code passes a `WorkspaceRef` explicitly: `fs.list(ws, path: ".")`. Agent tool calls (`tools: [fs.list]`) have the agent's bound workspace implied. Both go through the same Rust enforcement.
- `commands.run("name", workspace: ws)` executes only a command profile defined in `axiom.yaml`.

## 9. Tools and commands

- Tools have typed input/output schemas, effects, permissions, timeout, idempotency, output limits, and error types.
- Check three things independently: declared, registered, authorized for this agent in this context.
- Command profiles are named allowlisted executables with argument templates. Resolve executables safely, prevent argument injection, bound CPU, memory, time, and output, use a restricted environment, default network off, and sandbox with OS or container features where available. Never run a shell string assembled from model output. Filesystem isolation is not process sandboxing.
- Custom tools and adapters register in-process through documented, versioned Rust interfaces with test harnesses.

## 10. Workflows and runtime

- Workflows compile to an explicit, inspectable execution graph in typed IR. The model cannot invent control flow or bypass IR gates.
- Support: deterministic steps, typed calls, conditionals, `match`, bounded loops, delegation, bounded parallelism, dependency scheduling (reject cycles), retries, error handlers, timeouts, cancellation propagation, checkpoints, human approval with pause/resume, escalation, verification gates, contract-version checks, conflict routing.
- Parallel task outputs stay isolated artifacts until explicit integration.
- Backpressure, concurrency caps, token/cost budgets where pricing data exists, hard wall-clock and call-count limits.
- Retries have at-least-once semantics. Use idempotency keys; require explicit acknowledgement for non-idempotent side effects.
- Every run has a durable ID and a trace linking source version, redacted config version, task graph, model aliases, tool calls, artifacts, and final state.
- **State:** a store trait with SQLite as the default and an in-memory test backend; schema versioning and tested migrations; resume after process restart for workflows declared resumable.
- **Observability:** structured events and spans for runs, agent calls, tool calls, state transitions, filesystem operations, artifact publication, verification, approval, integration, and failure. Configurable retention and redaction. No secrets or file contents in traces by default.

## 11. Reference program (must compile, type-check, and run in an integration test)

The example set is a multi-module project under `examples/src/`. Every file is complete and committed; no imported symbol is undeclared. Module graph: `types.ax` ← `contracts.ax`, `workspaces.ax` ← `agents.ax` ← `filesystem_feature_workflow.ax`.

### 11.1 `types.ax`

```ts
module examples.types;

export interface ContractRef { id: string; version: string; }

export interface AcceptanceCriterion {
  id: string;
  description: string;
  verification: "unit_test" | "integration_test" | "security_check" | "human_review";
}

export interface FeatureSpec {
  name: string;
  objective: string;
  required_files: string[];
  acceptance: AcceptanceCriterion[];
}

export interface ImplementationTask {
  id: string;
  objective: string;
  owned_paths: string[];
  dependencies: string[];
  contract: ContractRef;
}

export interface ExecutionPlan { tasks: ImplementationTask[]; risks: string[]; }

export interface RepositoryReport {
  relevant_files: string[];
  summary: string;
  risks: string[];
}

export interface PatchArtifact {
  artifact_id: string;
  task_id: string;
  changed_paths: string[];
  diff_hash: string;
  contract: ContractRef;
}

export interface ReviewResult { approved: bool; findings: string[]; }
export interface WorkflowResult { release_artifact_id: string; patch_ids: string[]; }
```

### 11.2 `contracts.ax`

```ts
module examples.contracts;
import { RepositoryReport } from "./types.ax";

export contract RepositoryReportContract {
  version: "1.0.0";
  subject: RepositoryReport;
  postcondition non_empty: output.relevant_files.length > 0;
  criterion "Summary explains why each file is relevant": CheckKind.ModelEvaluated;
}
```

### 11.3 `workspaces.ax`

```ts
module examples.workspaces;

export workspace ProjectReadOnly {
  root: project;
  isolation: WorkspaceIsolation.SharedReadOnly;
  read: ["src/**", "tests/**", "docs/**", "Cargo.toml", "Cargo.lock"];
  write: [];
  deny: [".git/**", ".env", "**/.env", "**/*.pem", "**/*credentials*", "**/secrets/**"];
  limits { max_file_bytes: 2MB; max_total_read_bytes: 20MB; max_files: 5000; }
}

export workspace ProjectIntegration {
  root: project;
  isolation: WorkspaceIsolation.ExclusiveIntegration;
  read: ["src/**", "tests/**", "docs/**", "Cargo.toml", "Cargo.lock"];
  write: ["src/**", "tests/**", "docs/**", "Cargo.toml", "Cargo.lock"];
  deny: [".git/**", ".env", "**/.env", "**/*.pem", "**/*credentials*", "**/secrets/**"];
  write_authority: ["patches.apply_set"];
}

export workspace ImplementationWorkspace {
  base: ProjectReadOnly;
  isolation: WorkspaceIsolation.PerTaskCopyOnWrite;
  read: ["src/**", "tests/**", "docs/**", "Cargo.toml", "Cargo.lock"];
  write: ["src/**", "tests/**"];
  deny: ["src/auth/**", ".git/**", ".env", "**/.env", "**/*.pem", "**/*credentials*"];
  limits { max_file_bytes: 2MB; max_total_write_bytes: 10MB; max_files_changed: 100; }
}

export policy SourceChangePolicy {
  deny_path_patterns: [".git/**", "**/.env", "**/*.pem", "**/*credentials*"];
  require_patch_review: true;
  require_tests_before_integration: true;
  permit_delete: false;
}
```

### 11.4 `agents.ax`

```ts
module examples.agents;

import { FeatureSpec, ExecutionPlan, ImplementationTask, PatchArtifact,
         RepositoryReport, ReviewResult } from "./types.ax";
import { ProjectReadOnly, ImplementationWorkspace } from "./workspaces.ax";

export agent RepositoryAnalyst implements Agent<FeatureSpec, RepositoryReport> {
  purpose: "Inspect the repository and report relevant files without modifying them";
  model: "reasoning";
  workspace: ProjectReadOnly;
  tools: [fs.list, fs.walk, fs.search, fs.read_text, fs.hash, artifacts.publish];
}

export agent Architect implements Agent<FeatureSpec, ExecutionPlan> {
  purpose: "Decompose requirements and define interfaces";
  model: "reasoning";
  workspace: ProjectReadOnly;
  tools: [fs.list, fs.search, fs.read_text, artifacts.publish];
  constraints {
    must: ["Define explicit interfaces", "Identify dependencies"];
    must_not: ["Write application files", "Change shared contracts implicitly"];
  }
}

export agent Implementer implements Agent<ImplementationTask, PatchArtifact> {
  purpose: "Implement the assigned task inside the isolated task workspace";
  model: "coding";
  workspace: ImplementationWorkspace;
  tools: [fs.list, fs.walk, fs.search, fs.read_text, fs.write_text,
          fs.apply_patch, fs.diff, artifacts.publish];
  constraints {
    must: ["Read the relevant existing code before changing it",
           "Preserve unrelated changes",
           "Return a patch artifact and change summary"];
    must_not: ["Write outside the assigned workspace",
               "Modify denied paths",
               "Claim tests passed without command evidence"];
  }
}

// No workspace: the reviewer sees immutable patch artifacts only.
export agent Reviewer implements Agent<PatchArtifact, ReviewResult> {
  purpose: "Review a verified patch artifact for correctness and policy compliance";
  model: "review";
  tools: [artifacts.fetch];
}

// Instance override: changes runtime binding, not agent source.
export const localImplementer = Implementer.with { model: "local_coding" };
```

### 11.5 `filesystem_feature_workflow.ax`

```ts
module examples.filesystem_feature_workflow;

import { FeatureSpec, WorkflowResult } from "./types.ax";
import { RepositoryReportContract } from "./contracts.ax";
import { ProjectReadOnly, ProjectIntegration, ImplementationWorkspace,
         SourceChangePolicy } from "./workspaces.ax";
import { Architect, Reviewer, RepositoryAnalyst, Implementer } from "./agents.ax";

export workflow ImplementFeature(spec: FeatureSpec) -> WorkflowResult {
  // 1. Discover and read through the permission-checked filesystem.
  let rootEntries = await fs.list(ProjectReadOnly, path: ".", depth: 2);
  let sourceFiles = await fs.walk(ProjectReadOnly, include: ["src/**", "tests/**"]);
  let relevant = await fs.search(ProjectReadOnly, query: spec.objective,
                                 include: ["src/**", "tests/**", "docs/**"], max_results: 100);
  let manifest = await fs.read_text(ProjectReadOnly, path: "Cargo.toml");
  let samples = await fs.read_many_text(ProjectReadOnly, paths: relevant.top_paths(10),
                                        max_total_bytes: 1000000);

  // 2. Analyze and plan. Agent output is untrusted until validated.
  let report = await delegate RepositoryAnalyst.execute(spec, context: {
    root_entries: rootEntries, source_files: sourceFiles, relevant_matches: relevant,
    manifest: manifest, source_samples: samples
  });
  require report.satisfies(RepositoryReportContract);

  let plan = await delegate Architect.execute(spec, context: { report: report });
  require workflows.validate_plan(plan, policy: SourceChangePolicy);

  // 3. Implement each task in its own copy-on-write workspace.
  let submitted = parallel for task in plan.tasks {
    let ws = await workspaces.create(base: ProjectReadOnly, task_id: task.id,
                                     template: ImplementationWorkspace,
                                     isolation: WorkspaceIsolation.PerTaskCopyOnWrite);
    let taskFiles = await fs.list(ws, path: ".", depth: 4);
    let result = await delegate Implementer.execute(task, context: {
      workspace_summary: taskFiles, source_change_policy: SourceChangePolicy
    }, workspace: ws);
    let diff = await fs.diff(ws, base: ProjectReadOnly);
    let patch = await artifacts.publish_patch(workspace: ws, diff: diff,
                                              task_id: task.id, contract: task.contract);
    require collections.is_subset(patch.changed_paths, task.owned_paths);
    require SourceChangePolicy.accepts_paths(patch.changed_paths);
    yield { task: task, workspace: ws, result: result, patch: patch };
  };

  // 4. Conflict check before anything touches the shared project.
  let conflicts = await patches.detect_conflicts(submitted.map(x => x.patch));
  require conflicts.is_empty()
    else escalate "Conflicting changes require replanning or explicit resolution";

  // 5. Allowlisted verification inside each task workspace.
  let checks = parallel for item in submitted {
    let typecheck = await commands.run("cargo_check", workspace: item.workspace);
    let unit = await commands.run("unit_tests", workspace: item.workspace);
    let integ = await commands.run("integration_tests", workspace: item.workspace);
    await artifacts.publish_verification(item.patch,
      { typecheck: typecheck, unit_tests: unit, integration_tests: integ });
    require typecheck.passed && unit.passed && integ.passed;
    yield { patch: item.patch, checks: [typecheck, unit, integ] };
  };

  // 6. Independent review of immutable patch artifacts.
  let reviews = parallel for item in checks {
    yield await delegate Reviewer.execute(item.patch, context: { checks: item.checks });
  };
  require reviews.all(r => r.approved);

  // 7. Privileged integration: rechecks base revisions, permissions, conflicts, evidence.
  let release = await patches.apply_set(target: ProjectIntegration,
                                        patches: checks.map(x => x.patch),
                                        policy: SourceChangePolicy, require_clean_base: true);

  // 8. Final verification against the integrated candidate, then publish evidence.
  let finalCheck = await commands.run("full_ci", workspace: release.candidate_workspace);
  require finalCheck.passed;
  return await artifacts.publish_release(release, evidence: {
    plan: plan, patches: checks.map(x => x.patch), reviews: reviews, checks: finalCheck
  });
}
```

### 11.6 Integration test

Uses a temporary fixture repository and a fake model provider, and proves: discovery, reads, workspace creation, writes and patches, diffs, conflict detection, registered command execution, artifact publication, and the integration gate. It must also prove that unauthorized paths (for example `src/auth/**`) are blocked, that a failing check blocks integration, and that one task's unsubmitted writes are invisible to another.

## 12. Security

The threat model covers: prompt injection via repository files, untrusted model output, path traversal, symlink and TOCTOU attacks, command injection, credential leakage, unbounded spending, denial of service, unsafe integration, concurrent-edit conflicts, malicious tools, and compromised provider responses.

- All access control is enforced in the Rust runtime and filesystem backend, never only in prompts or DSL declarations.
- Validate every model output before it becomes a typed value, plan, patch, path, command argument, or permission. Repository text is untrusted and cannot override policy.
- Never write secrets into errors, config dumps, traces, or artifacts. No custom cryptography or home-built secret vault.
- Fuzz the lexer, parser, and config loader. Property-test path normalization, permission matching, config merging, patch application, state transitions, workflow scheduling, and type checking. Test concurrency and races.
- Document residual risks honestly in `docs/security-model.md`.

## 13. Standard library and developer tooling

**Stdlib modules** (typed, with documented effects, error behavior, and unit tests): `core collections json time env secrets fs workspaces artifacts patches commands agents workflows trace`. Includes `workflows.validate_plan`, `collections.is_subset`, `json` parse/validate functions returning `Result`, and the built-in types named in 8.5.

**CLI:** `init`, `check`, `config check`, `fmt` (with `--check`), `lint`, `explain`/IR, `plan`/dry-run, `run`, `status`/`inspect`, `resume`, `cancel`, `test`, provider/capability inspection, artifact inspection, trace inspection. Human-readable and machine-readable diagnostics with stable error codes.

**Formatter:** deterministic; golden-tested.

**Linter:** unused declarations and imports, unbounded loops, insecure filesystem scopes, unsafe tool grants, unhandled errors, missing contract criteria, shadowed prelude names, unreachable workflow branches, suspicious config.

**LSP:** diagnostics, completion (declarations, stdlib, config aliases), go-to-definition, references, hover/types, formatting, symbol outline.

## 14. Phases

Each phase ends with a compiling repo and passing tests. Update the ledger at every gate.

| # | Phase | Gate |
|---|---|---|
| 0 | **Design:** spec, full EBNF, type/effect/execution semantics, ADRs (compiler, adapters, filesystem isolation, workspace lifecycle, state, versioning), threat model, provider doc verification, conformance matrix | No contradiction between examples, config, types, and runtime policy; every example in this prompt parses against the EBNF on paper |
| 1 | **Syntax:** lexer, parser, AST, source maps, diagnostics with recovery | Positive and negative fixtures for every production; fuzzing never panics |
| 2 | **Semantics:** module graph, symbols, type system, generics, Option/Result, effects, source/config cross-validation | Every documented type/effect error class is rejected; valid generic and module examples are accepted |
| 3 | **Compiler and evaluator:** typed IR, deterministic evaluator, workflow graph validation, `explain`/`plan` | All deterministic conformance examples run with no provider; IR is stable and inspectable |
| 4 | **Config and integrations:** YAML schema, env/secret resolution, redaction, registry, profiles, precedence, adapters and dialects (Anthropic, OpenAI-compatible with DeepSeek/Moonshot/gateway dialects, mock) | Offline contract tests pass against mock HTTP servers; switching a profile changes no `.ax` source |
| 5 | **Filesystem, tools, artifacts:** secure FS, workspaces, Git worktrees, patches, artifacts, tool registry, command profiles | Security and property tests show no root escape, denied-path crossing, leakage of unsubmitted changes, quota bypass, or unreviewed/conflicting patch application |
| 6 | **Agent runtime:** agents, teams, goals, contracts, tool loops, typed output validation | End-to-end run with a fake provider; invalid output or unauthorized tool calls can never be accepted |
| 7 | **Workflow engine and state:** full workflow semantics, durable state, checkpoints, resume, cancel, approvals, events, traces | Kill-and-resume tests pass; failed checks block acceptance; secrets absent from persisted data; migrations tested |
| 8 | **Stdlib and reference program:** complete stdlib; Section 11 as a real integration test | The reference workflow runs end to end and blocks unauthorized paths |
| 9 | **Tooling:** CLI, formatter, linter, LSP | CLI snapshots, formatter goldens, linter tests, and LSP protocol tests pass |
| 10 | **Hardening and docs:** conformance suite, property/fuzz/security/concurrency tests, performance baselines, documentation, runnable examples, packaging | CI compiles and runs every documentation example; clean-checkout build works; residual risks documented |
| 11 | **Release sign-off:** audit against this document; write `docs/release-readiness.md` (evidence, commands, results, supported platforms, known issues, supported providers and capabilities) | Every Definition-of-Done item has evidence |

**Phase 10 security cases:** injection strings inside source files; model-produced path traversal and command injection; secret-exfiltration requests; unbounded task spawning; oversized or corrupted provider payloads; patch conflicts; timeouts; repeated retries; symlink and TOCTOU attacks; malformed configuration.

**Phase 10 documentation:** normative language reference, tutorial, standard-library reference, provider integration guide, custom adapter and tool developer guide, filesystem/workspace guide, workflow/runtime guide, security model, configuration guide, LSP guide, troubleshooting, migration and versioning policy, release notes.

**Phase 10 runnable examples:** hello-world deterministic program; single agent; multiple providers with local/cloud profile switching; generic custom OpenAI-compatible integration; agent/goal/contract/workflow composition; the full filesystem feature workflow; failure, retry, human approval, and checkpoint/resume; custom Rust tool and custom provider adapter.

## 15. Repository layout

Cargo workspace with acyclic crates (rename only with a documented reason):

```text
axiom/
  crates/
    axiom-source/  axiom-diagnostics/  axiom-types/  axiom-ir/
    axiom-config/  axiom-providers/
    axiom-adapter-openai-compatible/  axiom-adapter-anthropic/  axiom-adapter-mock/
    axiom-filesystem/  axiom-artifacts/  axiom-tools/
    axiom-runtime/  axiom-state/  axiom-stdlib/
    axiom-format/  axiom-lsp/  axiom-cli/
  examples/
    axiom.yaml
    src/  types.ax  contracts.ax  workspaces.ax  agents.ax  team.ax
          filesystem_feature_workflow.ax  local_and_cloud.ax
  tests/      conformance/  fixtures/  integration/  security/
  docs/       language-spec.md  grammar.ebnf  architecture.md  configuration.md
              providers.md  filesystem-and-workspaces.md  runtime-and-workflows.md
              standard-library.md  security-model.md  tooling.md
              migration-and-compatibility.md  implementation-ledger.md
              release-readiness.md  decisions/
```

Dependency rules: language, types, and IR never depend on a provider adapter. Providers never depend on the parser. Runtime policy is applied independently of the model.

## 16. Engineering rules

- Run `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace`; document any justified exception.
- CI is offline and deterministic. Real API tests are separately gated and optional.
- Never swallow errors or turn provider or tool failures into apparent success. No panics on user source, YAML, model output, patches, or provider responses. Use structured errors with stable codes and spans.
- Make cancellation, timeout, retry, budget, side-effect, and idempotency semantics explicit.
- Optimize for clear semantics, security, and maintainability, not minimum code size.
- Declare a language version; reject unsupported versions clearly; document the compatibility policy.
- Nothing may be represented as supported without documentation and tests.

## 17. Definition of Done

Axiom 1.0 is done only when all of the following hold:

1. The spec and `grammar.ebnf` cover all syntax and runtime behavior and match the parser.
2. The parser, module system, resolver, type checker, and effect checker support the full 1.0 language.
3. Code compiles to inspectable typed IR and executes per the documented semantics.
4. Agents, teams, goals, tasks, contracts, policies, tools, and workflows are real constructs with tested semantics.
5. One agent definition works with Claude, DeepSeek, Kimi, generic compatible endpoints, local models, and gateways through configuration alone (or a documented adapter), with no source rewrite.
6. The adapter extension API is documented, versioned, tested, and independent of the grammar and the workflow engine.
7. The standard library is complete with documented types, effects, errors, and tests.
8. Workspaces support rooted local access, isolated per-task workspaces, Git worktrees, safe patches, conflict detection, provenance, quotas, and enforced permissions.
9. The Section 11 program compiles and runs as a tested integration test.
10. Workflows support bounded parallelism, retries, timeouts, cancellation, escalation, approvals, checkpoints and resume, contracts, and verification gates.
11. Tools and command profiles are typed, allowlisted, and runtime-authorized; model output cannot add permissions or run arbitrary shell text.
12. Override precedence, capability checks, secret resolution, and redaction are tested.
13. CLI, formatter, linter, and LSP are implemented and tested.
14. Conformance, unit, integration, property, security, and available fuzz suites pass; builds are reproducible.
15. Every documented example compiles and is exercised by CI.
16. Limitations, unsupported provider capabilities, and residual risks are documented honestly.
17. `docs/release-readiness.md` has evidence for every required feature, with nothing marked prototype, simulated, or deferred.

If any gate fails, say exactly what remains. Do not call the work complete.
