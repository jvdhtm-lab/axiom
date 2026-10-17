# Axiom — Rust Agent Language: Master Implementation Prompt

## Mission

Build **Axiom**, a TypeScript-inspired domain-specific language (DSL) for defining AI agents, goals, abstractions, contracts, tools, policies, and hybrid workflows. Implement its compiler and runtime in Rust.

Axiom must combine:
- **Procedural control** for deterministic operations, checks, branching, retries, and security boundaries.
- **Agentic control** for planning, interpretation, bounded decisions, decomposition, and delegated work.
- **Contract-driven collaboration** so agents exchange typed inputs, outputs, task definitions, artifacts, and verification evidence.
- **Model portability** so an agent's role and source definition do not need to change when its underlying model changes.

Do not build a generic chatbot, a prompt-template wrapper, a framework-specific DSL, or a prototype that stops after a narrow vertical slice. Build **Axiom 1.0 as a complete, coherent, production-oriented domain-specific programming language for agentic systems**. It must have a published language specification, a complete grammar for its declared 1.0 feature set, a compiler, semantic/type analysis, a typed intermediate representation, a deterministic execution core, agent/workflow runtime, provider-neutral integrations, filesystem/workspace system, standard library, CLI, formatter, linter, language-server support, test/conformance suite, security model, examples, and operational documentation.

“Complete language” means complete for Axiom's domain: users must be able to author multi-module programs with types, functions, expressions, reusable declarations, filesystem operations, tools, agents, goals, contracts, policies, and hybrid workflows without hand-editing generated Rust or relying on hidden prompt conventions. It does **not** mean cloning all of TypeScript or allowing arbitrary Rust/JavaScript execution inside the DSL. Do not label core language features as optional future work, silently defer them, or redefine success as a prototype. If the work cannot be finished in one execution, continue through the phases in order as far as possible, maintain a truthful implementation ledger, and never claim Axiom 1.0 is complete until its entire definition of done passes.

Work incrementally, but the phases are a delivery plan for the **full Axiom 1.0 language**, not a reduced subset of the language. Each phase must leave the repository compiling and tests passing. Do not claim functionality is complete unless it is implemented and verified. When blocked, make the smallest reasonable design decision, document it in an ADR, and continue rather than repeatedly asking the user questions.

---

## Core architecture decisions

### 1. Separate the concepts

These are different entities and must not be collapsed into a single `model` field:

1. **Agent definition** — role, system behavior, typed input/output, tools, constraints, and authority.
2. **Integration connection** — a named endpoint/configuration that selects a protocol adapter and contains endpoint, authentication reference, timeouts, headers, and transport options.
3. **Model profile** — a reusable logical alias such as `reasoning`, `fast`, `coding`, or `local_default`, resolved to an integration connection and concrete model identifier.
4. **Agent instance** — an agent definition bound to a specific model profile and optional runtime overrides.
5. **Goal/task/contract** — desired outcome, bounded work, invariants, and acceptance criteria.
6. **Workflow** — sequencing, parallelism, delegation, retries, verification, and escalation.

A protocol adapter is the implementation that speaks an API protocol; an integration is a configured connection using that adapter; a model profile binds a named model ID to that integration. For example, `anthropic_messages` is an adapter, `claude_api` is an integration, and `claude_architecture` is a model profile. Keep these concepts separate. An agent's identity and logic should remain stable when its model profile changes.

### 2. Canonical Axiom source syntax

The syntax below is the target experience. Establish a consistent grammar and normative semantics in `docs/language-spec.md`; if a syntax choice must change, update all examples and the specification together. **Provider endpoints and model profiles belong in `axiom.yaml`; `.ax` files refer to profile aliases but do not embed vendor credentials or integration connection definitions.**

```ts
// Syntax fragment: an example of the .ax source style, not the full conformance fixture.
module example.agents;

export type AgentId = string;

export interface FeatureSpec {
  name: string;
  objective: string;
  required_files: string[];
  acceptance: AcceptanceCriterion[];
}

export interface AcceptanceCriterion {
  id: string;
  description: string;
  verification: "unit_test" | "integration_test" | "security_check" | "human_review";
}

export type ExecutionPlan = {
  tasks: Task[];
  dependency_edges: Dependency[];
  risks: string[];
};

export interface Task {
  id: string;
  objective: string;
  owned_paths: string[];
  dependencies: string[];
  contract: string;
}

export interface PatchArtifact {
  artifact_id: string;
  task_id: string;
  changed_paths: string[];
  diff_hash: string;
}

export agent Architect implements Agent<FeatureSpec, ExecutionPlan> {
  purpose: "Decompose requirements and define interfaces";
  model: "reasoning"; // resolved from axiom.yaml
  tools: [fs.list, fs.search, fs.read_text, artifact.publish];
  workspace: ProjectReadOnly;

  constraints {
    must: ["Define explicit interfaces", "Identify dependencies"];
    must_not: ["Write application files", "Change shared contracts implicitly"];
  }
}

export agent Implementer implements Agent<Task, PatchArtifact> {
  purpose: "Implement one bounded task and submit a patch";
  model: "coding";
  tools: [fs.list, fs.search, fs.read_text, fs.write_text, fs.apply_patch, fs.diff];
  workspace: ImplementationWorkspace;
  constraints {
    must: ["Follow the assigned contract", "Add or update tests"];
  }
}
```

This block is a syntax fragment and intentionally relies on supporting declarations such as `Dependency` and the standard `Agent` interface; it is not a stand-alone conformance fixture. The compiler must support modules/imports/exports, type declarations, expressions, functions, agents, teams, tools, policies, contracts, goals, workspaces, and workflows as specified in the complete-language requirements below. Keep the TypeScript-inspired appearance, but make Axiom semantics explicit and do not inherit JavaScript quirks unintentionally.

`env()`, `secret()`, `fs.*`, `workspace.*`, `artifact.*`, and other standard-library operations must have documented types, effects, error behavior, and permission checks. Secrets must never be logged or written into compiled plans, traces, IR dumps, snapshots, artifacts, or error messages.

### 3. Configuration precedence

Define and test a deterministic precedence rule:

**per-invocation override > agent-instance override > agent declaration > project default > runtime default**.

Integration settings and model settings are distinct. A model profile references an integration by ID and a concrete model identifier. The model profile alias should be resolved at runtime so integration or concrete model changes do not require recompiling every agent definition, unless the chosen compilation model makes that impractical. Document the behavior.

Allow global/project defaults and per-agent/per-instance overrides. Do not require API credentials in source files.

### 3a. Canonical split: TypeScript-inspired DSL plus YAML configuration

Use **TypeScript-inspired syntax for the Axiom language** and **YAML for project/runtime configuration**. Do not make the core language YAML-shaped, and do not put credentials or environment-specific endpoint configuration into `.ax` source files.

Canonical file responsibilities:
- `*.ax`: typed declarations and executable semantics — types, agents, goals, contracts, tools, policies, workflows, delegation, parallelism, branching, retries, and verification gates.
- `axiom.yaml`: deployment/environment configuration — provider integrations, endpoint URLs, secret/environment references, model profiles, defaults, runtime limits, and logging settings.
- Multiple `.ax` modules, imports, exports, and reusable modules are required Axiom 1.0 features and must be implemented and tested.

Illustrative `axiom.yaml`:

```yaml
version: 1

integrations:
  claude:
    adapter: anthropic_messages
    base_url: "${ANTHROPIC_BASE_URL:-https://api.anthropic.com}"
    api_key: "${ANTHROPIC_API_KEY}"

  deepseek:
    adapter: openai_compatible
    base_url: "${DEEPSEEK_BASE_URL:-https://api.deepseek.com}"
    api_key: "${DEEPSEEK_API_KEY}"

  kimi:
    adapter: openai_compatible
    base_url: "${KIMI_BASE_URL}"
    api_key: "${KIMI_API_KEY}"

  local:
    adapter: openai_compatible
    base_url: "${AXIOM_LOCAL_BASE_URL:-http://localhost:11434/v1}"
    api_key: "${AXIOM_LOCAL_API_KEY}"
    api_key_optional: true

models:
  reasoning:
    integration: claude
    model: "${CLAUDE_MODEL}"
  coding:
    integration: deepseek
    model: "${DEEPSEEK_MODEL}"
  review:
    integration: kimi
    model: "${KIMI_MODEL}"
  local_coding:
    integration: local
    model: "${LOCAL_MODEL}"

runtime:
  default_model: reasoning
  max_parallel_agents: 4
  max_retries: 2
  request_timeout_seconds: 120
```

The exact YAML structure may be refined during Phase 0, but its semantics must remain clear and documented. Environment-variable expansion must be implemented explicitly; do not assume a YAML parser expands `${...}` automatically. Secret references must be resolved at runtime and redacted from logs, diagnostics, IR dumps, and traces. Avoid supporting arbitrary YAML tags or executable expressions.

The associated `.ax` fragment should refer to logical model aliases only; referenced types and contracts are declared elsewhere in the complete project:

```ts
agent Architect implements Agent<FeatureSpec, ExecutionPlan> {
  model: "reasoning"
  purpose: "Design interfaces and decompose requirements"
}

agent Implementer implements Agent<Task, Patch> {
  model: "coding"
  purpose: "Implement an assigned task within its contract"
}

workflow DevelopFeature(spec: FeatureSpec) {
  let plan = delegate Architect.decompose(spec)
  let patches = parallel for task in plan.tasks {
    delegate Implementer.execute(task)
  }
  require verify(patches, FeatureContract)
}
```

Configuration validation must detect missing provider references, unknown model aliases, invalid adapter settings, missing required environment variables, and incompatible declared capabilities before an execution starts where possible. Support an explicit CLI command such as `axiom config check axiom.yaml`. Agent definitions must remain portable when a user changes a model profile or integration in configuration.

### 4. Model portability and default integration

Every agent should integrate with a shared runtime interface by default. The user should not need to write custom integration code for every agent or provider.

Implement a protocol-adapter abstraction similar in spirit to:

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

Treat this as illustrative, not a mandate to copy the exact trait if a better design emerges. Keep the adapter contract small and testable. A separate integration registry resolves configured connections to adapters, and a model profile resolves to an integration plus model ID. Use capability negotiation for optional features (streaming, tool calls, structured output, embeddings, multimodal input). Do not pretend all models support every feature. Unsupported capabilities must produce clear errors before execution when possible.

### Required integration layer: multiple protocols, direct providers, and gateways

Axiom must not equate a vendor with a single API protocol. The integration layer is a first-class subsystem and must support different vendors and protocol families behind one provider-neutral runtime interface.

Use three integration strategies:

1. **Protocol adapters** for common API shapes, such as OpenAI Chat Completions-compatible APIs and OpenAI Responses-compatible APIs, where implemented.
2. **Native vendor adapters** when a vendor's API semantics, request/response format, authentication, tool-call representation, streaming events, or reasoning fields differ materially. At minimum, design the initial adapter set for Anthropic Messages (Claude), DeepSeek, and Moonshot/Kimi, in addition to the generic OpenAI-compatible adapter. DeepSeek and Kimi may also be accessible through compatible endpoints, but Axiom must not assume that all endpoints or capabilities behave identically.
3. **Gateway adapters** for services such as OpenRouter or a self-hosted LiteLLM gateway. A gateway should let users reach many upstream model families through one configured connection, while direct adapters remain available for native capabilities and avoiding an extra routing layer.

The architecture must allow a new vendor or endpoint to be added by implementing a documented Rust adapter/plugin interface and registering an integration, without changing the DSL grammar, agent definitions, workflow IR, or core runtime. Support configuration-only custom OpenAI-compatible endpoints as the simplest extension path. A protocol that cannot be expressed through configuration alone must be implementable as a new adapter.

Initial vendor/protocol integration targets:
- **Anthropic / Claude**: native Anthropic Messages adapter, including its distinct tool-use representation and request requirements.
- **DeepSeek**: direct adapter/configuration for its documented compatible API, preserving provider-specific request options and response fields when needed.
- **Moonshot AI / Kimi**: direct Kimi/Moonshot adapter or a clearly supported OpenAI-compatible protocol mode for the exact endpoint being configured.
- **OpenAI-compatible endpoints**: configurable base URL, model ID, optional API key, custom headers, timeout, and relevant request options. This should support compatible services and local/self-hosted runtimes such as Ollama, vLLM, and LM Studio to the extent their selected endpoint supports the requested API shape.
- **OpenRouter and LiteLLM gateway endpoints**: usable through an appropriate adapter when configured as an upstream gateway.
- **Mock adapter**: deterministic, offline tests.

Do not claim that a provider name alone guarantees feature support. Maintain a capability matrix per adapter/model configuration for chat, structured output, tool/function calling, streaming, vision/multimodal input, embeddings, and reasoning controls. Some capabilities depend on the exact model, API endpoint, deployment, and adapter version.

Vendor-specific options should be available in a typed extension field or adapter-specific integration configuration namespace, validated by the selected adapter. Do not silently discard unknown options. Keep a provider-neutral common request/response representation while preserving raw provider metadata where it is useful and safe. Document normalization losses and feature differences.

Axiom 1.0 does not need a dedicated built-in adapter for every vendor on earth. Instead, make the adapter/integration boundary universal and test it with the named initial vendors. The stated goal is that **any model can be integrated through a supported protocol, a gateway, a configuration-only compatible endpoint, or a custom adapter**—not that every model's every feature is magically interchangeable.

For the first implementation, custom adapters may be registered in-process through Rust traits and feature-gated crates; do not load arbitrary native dynamic libraries or execute downloaded plugins. A user-defined endpoint that follows a supported protocol should need configuration only. A model on an unsupported protocol needs a new adapter or a gateway that translates it into a supported protocol. This is the honest boundary of "any model" support.

Use configuration like:

```ts
runtime {
  default_model: "reasoning"
}

// Use project default:
agent Reviewer { /* model omitted */ }

// Override in an agent definition:
agent Architect { model: "reasoning" }

// Override an instance:
let localReviewer = Reviewer.with { model: "local_coding" }

// Override for one call via CLI/config/API where supported:
run localReviewer(input, { model: "another_profile" })
```

The final syntax may vary, but all configuration levels must exist and their precedence must be tested. Concrete model IDs are opaque user-supplied values: do not hardcode an allowlist of model names in the compiler. If an endpoint supports model discovery, it may be used as an optional convenience; static IDs must always remain supported.

### Multi-provider integration example

Integration connections and model profiles are declared in `axiom.yaml`; agent definitions remain in `.ax` and refer only to logical profile aliases. Example configuration:

```yaml
version: 1

integrations:
  claude_api:
    adapter: anthropic_messages
    base_url: "${ANTHROPIC_BASE_URL:-https://api.anthropic.com}"
    api_key: "${ANTHROPIC_API_KEY}"
    timeout_seconds: 120

  deepseek_api:
    adapter: openai_compatible
    base_url: "${DEEPSEEK_BASE_URL:-https://api.deepseek.com}"
    api_key: "${DEEPSEEK_API_KEY}"
    timeout_seconds: 120

  kimi_api:
    adapter: openai_compatible
    base_url: "${KIMI_BASE_URL:-https://api.moonshot.ai/v1}"
    api_key: "${KIMI_API_KEY}"
    timeout_seconds: 120

  local_models:
    adapter: openai_compatible
    base_url: "${AXIOM_LOCAL_BASE_URL:-http://localhost:11434/v1}"
    api_key: "${AXIOM_LOCAL_API_KEY:-}"
    api_key_optional: true
    timeout_seconds: 180

models:
  reasoning:
    integration: claude_api
    model: "${CLAUDE_MODEL}"
    capabilities: [chat, structured_output, tool_calling]

  coding:
    integration: deepseek_api
    model: "${DEEPSEEK_MODEL}"
    capabilities: [chat, tool_calling]

  review:
    integration: kimi_api
    model: "${KIMI_MODEL}"
    capabilities: [chat]

  local_coding:
    integration: local_models
    model: "${LOCAL_MODEL}"
    capabilities: [chat]

runtime:
  default_model: reasoning
  max_parallel_agents: 8
  max_retries: 3
  request_timeout_seconds: 120
```

The exact endpoint defaults must be checked against current official provider documentation before implementation and recorded with a verification date in `docs/providers.md`. Environment expansion must be implemented explicitly; YAML parsers do not expand it automatically. Missing required variables, invalid adapter settings, unknown references, or incompatible capabilities must produce actionable diagnostics before a model call when possible.

```ts
// src/team.ax
module example.team;
import { FeatureSpec, ExecutionPlan, Task, PatchArtifact } from "./agents.ax";

export agent Architect implements Agent<FeatureSpec, ExecutionPlan> {
  model: "reasoning";
  purpose: "Design the architecture and decompose requirements";
  tools: [fs.list, fs.search, fs.read_text, artifact.publish];
  workspace: ProjectReadOnly;
}

export agent Implementer implements Agent<Task, PatchArtifact> {
  model: "coding";
  purpose: "Implement a task within its contract";
  tools: [fs.list, fs.search, fs.read_text, fs.write_text, fs.apply_patch, fs.diff];
  workspace: ImplementationWorkspace;
}

// This binding changes runtime configuration, not agent source.
let localImplementer = Implementer.with { model: "local_coding" };
```

The same agent definition must be usable with a native Claude adapter, DeepSeek, Kimi, a local compatible endpoint, or a gateway by changing configuration/profile binding. New compatible endpoints should require configuration only. New API protocols require an adapter implementing the public Rust extension interface, not modifications to the DSL grammar or workflow engine.

### 5. Model profile is not model intelligence

Names like `reasoning`, `fast`, and `coding` are **user-defined profiles**, not hard-coded provider types or claims about a model's abilities. Let users map each profile to any compatible provider/model. Capabilities must come from explicit configuration, provider metadata, or runtime detection where available. Treat declared capabilities as expectations, not proof that a model will behave correctly.

### 6. Typed outputs and contracts

Provide typed agent inputs and outputs at the language level. At the model boundary, decode and validate generated output against the requested schema. Never trust model-produced JSON merely because it parses. Return structured errors on invalid outputs; allow bounded retry/repair policies only if explicitly configured.

Contracts should support:
- Input/output types and schema validation.
- Preconditions, postconditions, and invariants where practical.
- Acceptance checks and named verification commands/functions.
- Contract versions attached to tasks and artifacts.
- Proposals for contract changes, rather than silent modification of shared contracts.

Do not claim that natural-language constraints can be fully statically verified. Distinguish compile-time checks, runtime-enforced policies, and model-evaluated criteria in documentation.

### 7. Hybrid workflows

Axiom 1.0 must support all of the following as normative language/runtime features:
- Deterministic steps and typed function/tool calls.
- Conditional branches.
- Agent invocation and delegation.
- Parallel independent tasks with bounded concurrency.
- Typed output assignment.
- `require`/verification gates.
- Retry limits and escalation to a human or named handler.
- Timeout and cancellation propagation.

A workflow must be compiled to an explicit execution graph/IR. Do not have the LLM invent runtime control flow that bypasses the compiled plan or permissions. Agent planning may propose subgoals or plans, but validate them against contracts, depth limits, dependency rules, allowed tools, and budgets before execution.

### 8. Filesystem is a first-class runtime subsystem

Axiom must have a real filesystem abstraction. Do not model filesystem access merely as a handful of prompt instructions or assume that an agent can safely edit the host repository because it has an `fs.write` tool.

Distinguish four concepts:

1. **Project filesystem** — the user's repository and source files, addressed through a configured project root.
2. **Agent/task workspace** — an isolated working view assigned to an agent or task. Concurrent implementation agents must not silently write into the same mutable working directory.
3. **Artifact/evidence store** — submitted patches, generated files, plans, test reports, review results, and verification evidence. Artifacts have stable IDs, provenance, task IDs, and contract versions.
4. **Runtime state store** — task state and workflow checkpoints. Keep this logically separate from source files and implement durable state/resume support as part of Axiom 1.0.

For Axiom 1.0, implement a provider-independent Rust filesystem interface with at least a rooted local-filesystem backend and an in-memory/fake backend for deterministic tests. Keep filesystem tools behind the same registered, typed, permission-checked tool system as every other tool.

The runtime must enforce:
- Every workspace has an explicit root; reject path traversal (`..`), unexpected absolute paths, and paths that escape the root.
- Normalize and validate paths before checking allow/deny rules. Do not rely on string-prefix checks to prove containment.
- Protect against symlink escapes and document any remaining local-filesystem race limitations. By default, do not permit agents to create or follow symlinks.
- Deny access by default outside declared read/write scopes. Deny sensitive paths such as `.env`, private keys, credentials, `.git/**`, and runtime secret files unless explicitly and safely configured.
- Enforce separate read/list/search/write/patch/delete permissions. Delete should be denied unless explicitly granted. Never infer permissions from model output.
- Bound file size, number of files, diff size, and tool-call time. Do not allow an agent to read arbitrary host files just because it knows a path.
- Record auditable operations and results without logging secret contents.
- Use atomic writes where practical, and submit changes as patch/artifact output for verification before integration.
- Validate every generated patch and detect conflicts before applying it to the shared project workspace.

Use isolated workspaces for parallel implementation. Implement per-task copy-on-write or patch-overlay workspaces and a Git worktree backend for real repositories. An in-memory backend is required for deterministic tests. An agent's write scope is the intersection of its declared workspace permissions, current runtime policy, task-owned paths, and the workspace's actual filesystem capabilities.

Illustrative `.ax` syntax:

```ts
workspace Project {
  root: project
  default_access: read_only
  deny: [".git/**", ".env", "**/.env", "**/*.pem", "**/*credentials*"]
}

workspace ImplementationWorkspace {
  base: Project
  isolation: per_task
  read: ["src/**", "tests/**", "docs/**"]
  write: ["src/**", "tests/**"]
  deny: ["src/auth/**", "**/.env", "**/*.pem"]
}

agent Implementer implements Agent<Task, Patch> {
  model: "coding"
  workspace: ImplementationWorkspace
  tools: [fs.read, fs.list, fs.search, fs.apply_patch, fs.diff, terminal.run_tests]
}
```

Exact syntax may be refined in Phase 0. Define whether permission patterns are relative to the project root or the workspace root; choose one consistent rule and show it in diagnostics. The runtime—not the LLM—must enforce all permissions.

Illustrative `axiom.yaml` settings:

```yaml
filesystem:
  project_root: "."
  default_agent_isolation: "per_task"
  max_read_bytes: 2000000
  max_write_bytes: 1000000
  max_files_per_task: 500
  protect_vcs_metadata: true
```

Provider configuration belongs in YAML as before. Filesystem authorization policy and workflow-level workspace assignments belong in `.ax`; environment-specific root paths and operational limits may be configured in YAML. Document precedence and reject contradictory or unsafe configuration rather than silently widening access.

Add a minimal first-class filesystem tool set:
- `fs.read(path)`
- `fs.list(path)`
- `fs.search(query, scope)`
- `fs.write(path, content)` only where write permission is granted
- `fs.apply_patch(patch)` with path/scope validation
- `fs.diff()`
- `fs.delete(path)` only where explicitly granted; disabled by default

Avoid unrestricted shell access. If `terminal.run_tests` is added, it must execute only registered commands or validated command templates within the agent's workspace and a restricted environment. Filesystem isolation is not a substitute for process sandboxing.

### 8a. Mandatory end-to-end filesystem example (the runtime must actually execute this)

Declarations of `workspace` permissions are not enough. The language specification, implementation, tests, and final examples must include a workflow in which deterministic Axiom code and delegated agents actively discover files, read content, create isolated workspaces, write changes, generate diffs, run registered verification commands, submit artifacts, and integrate accepted changes through a gate.

The following is a normative design example. Refine minor syntax only if necessary, then keep the example compilable and exercised by an integration test:

```ts
// examples/filesystem_feature_workflow.ax
module examples.filesystem_feature_workflow;

import { FeatureSpec, ImplementationTask, PatchArtifact, RepositoryReport,
         RepositoryReportContract, WorkflowResult } from "./types.ax";
import { Architect, Reviewer } from "./agents.ax";

workspace ProjectReadOnly {
  root: project;
  isolation: shared_read_only;
  read: ["src/**", "tests/**", "docs/**", "Cargo.toml", "Cargo.lock"];
  write: [];
  deny: [".git/**", ".env", "**/.env", "**/*.pem", "**/*credentials*", "**/secrets/**"];
  limits {
    max_file_bytes: 2MB;
    max_total_read_bytes: 20MB;
    max_files: 5000;
  }
}

workspace ProjectIntegration {
  root: project;
  isolation: exclusive_integration;
  read: ["src/**", "tests/**", "docs/**", "Cargo.toml", "Cargo.lock"];
  write: ["src/**", "tests/**", "docs/**", "Cargo.toml", "Cargo.lock"];
  deny: [".git/**", ".env", "**/.env", "**/*.pem", "**/*credentials*", "**/secrets/**"];
  write_authority: ["integration.apply_patchset"];
}

workspace ImplementationWorkspace {
  base: ProjectReadOnly;
  isolation: per_task_copy_on_write;
  read: ["src/**", "tests/**", "docs/**", "Cargo.toml", "Cargo.lock"];
  write: ["src/**", "tests/**"];
  deny: ["src/auth/**", ".git/**", ".env", "**/.env", "**/*.pem", "**/*credentials*"];
  limits {
    max_file_bytes: 2MB;
    max_total_write_bytes: 10MB;
    max_files_changed: 100;
  }
}

policy SourceChangePolicy {
  deny_path_patterns: [".git/**", "**/.env", "**/*.pem", "**/*credentials*"];
  require_patch_review: true;
  require_tests_before_integration: true;
  permit_delete: false;
}

agent RepositoryAnalyst implements Agent<FeatureSpec, RepositoryReport> {
  model: "reasoning";
  workspace: ProjectReadOnly;
  tools: [fs.list, fs.walk, fs.search, fs.read_text, fs.hash, artifact.publish];
  purpose: "Inspect the repository and report relevant files without modifying them";
}

agent Implementer implements Agent<ImplementationTask, PatchArtifact> {
  model: "coding";
  workspace: ImplementationWorkspace;
  tools: [fs.list, fs.walk, fs.search, fs.read_text, fs.write_text,
          fs.apply_patch, fs.diff, artifact.publish];
  purpose: "Implement the assigned task inside the isolated task workspace";
  constraints {
    must: ["Read the relevant existing code before changing it",
           "Preserve unrelated changes",
           "Return a patch artifact and change summary"];
    must_not: ["Write outside the assigned workspace",
               "Modify denied paths",
               "Claim tests passed without command evidence"];
  }
}

workflow ImplementFeature(spec: FeatureSpec) -> WorkflowResult {
  // 1. Discover repository structure through the permission-checked filesystem API.
  let rootEntries = await fs.list(ProjectReadOnly, path: ".", depth: 2);
  let sourceFiles = await fs.walk(ProjectReadOnly, include: ["src/**", "tests/**"]);
  let relevant = await fs.search(
    ProjectReadOnly,
    query: spec.objective,
    include: ["src/**", "tests/**", "docs/**"],
    max_results: 100
  );

  // 2. Read selected files; each read is checked and audited by the runtime.
  let projectManifest = await fs.read_text(ProjectReadOnly, path: "Cargo.toml");
  let sourceSamples = await fs.read_many_text(
    ProjectReadOnly,
    paths: relevant.top_paths(10),
    max_total_bytes: 1000000
  );

  // 3. The analyst proposes a task plan. The runtime validates paths,
  //    dependencies, task schemas, budgets, and contract versions.
  let report = await delegate RepositoryAnalyst.execute(spec, context: {
    root_entries: rootEntries,
    source_files: sourceFiles,
    relevant_matches: relevant,
    manifest: projectManifest,
    source_samples: sourceSamples
  });
  require report.satisfies(RepositoryReportContract);

  let plan = await delegate Architect.decompose(spec, context: report);
  require validate_plan(plan, policy: SourceChangePolicy);

  // 4. Each independent task receives a separate copy-on-write workspace.
  let submitted = parallel for task in plan.tasks {
    let ws = await workspace.create(
      base: ProjectReadOnly,
      task_id: task.id,
      template: ImplementationWorkspace,
      isolation: per_task_copy_on_write
    );

    // An explicitly granted deterministic API can read files from that workspace.
    let taskFiles = await fs.list(ws, path: ".", depth: 4);
    let taskContext = {
      task: task,
      workspace_summary: taskFiles,
      contract: task.contract,
      source_change_policy: SourceChangePolicy
    };

    let result = await delegate Implementer.execute(taskContext, workspace: ws);

    // An agent can request these operations through tools; the runtime checks
    // every operation. The orchestrator independently collects the final diff.
    let diff = await fs.diff(ws, base: ProjectReadOnly);
    let patch = await artifact.publish_patch(
      workspace: ws,
      diff: diff,
      task_id: task.id,
      contract: task.contract
    );

    require set.is_subset(patch.changed_paths, task.owned_paths);
    require SourceChangePolicy.accepts_paths(patch.changed_paths);
    return { task: task, workspace: ws, result: result, patch: patch };
  };

  // 5. Check for conflicts before applying any patch to the shared project.
  let conflictReport = await patchset.detect_conflicts(submitted.map(x => x.patch));
  require conflictReport.conflicts.is_empty
    else escalate "Conflicting changes require replanning or explicit resolution";

  // 6. Execute allowlisted verification commands in each isolated workspace.
  let checks = parallel for item in submitted {
    let typecheck = await command.run("cargo_check", workspace: item.workspace);
    let unitTests = await command.run("unit_tests", workspace: item.workspace);
    let integrationTests = await command.run("integration_tests", workspace: item.workspace);
    await artifact.publish_verification(item.patch, {
      typecheck: typecheck,
      unit_tests: unitTests,
      integration_tests: integrationTests
    });
    require typecheck.passed && unitTests.passed && integrationTests.passed;
    return { patch: item.patch, checks: [typecheck, unitTests, integrationTests] };
  };

  // 7. Independent review receives immutable patch artifacts, not writable access
  //    to the implementer's workspace.
  let reviews = parallel for item in checks {
    await delegate Reviewer.inspect(item.patch, context: item.checks)
  };
  require reviews.all(review => review.approved);

  // 8. Integration is an explicit privileged runtime operation. It rechecks
  //    patch base revisions, permissions, conflicts, and required evidence.
  let release = await integration.apply_patchset(
    target: ProjectIntegration,
    patches: checks.map(x => x.patch),
    policy: SourceChangePolicy,
    require_clean_base: true
  );

  // 9. Run final verification against the integrated project, then publish evidence.
  let finalCheck = await command.run("full_ci", workspace: release.candidate_workspace);
  require finalCheck.passed;
  return await artifact.publish_release(release, evidence: {
    plan: plan,
    patches: checks.map(x => x.patch),
    reviews: reviews,
    checks: finalCheck
  });
}
```

This example requires corresponding standard-library interfaces and semantics; it must not be treated as pseudocode outside the implementation plan. If a chosen grammar renames a construct, update the example, standard library, specification, and tests together. Add an end-to-end integration test using a temporary fixture repository and fake model provider to prove that file discovery, reads, workspace creation, writes/patches, diffs, conflict checks, registered commands, artifact publication, and integration gates genuinely work.

`axiom.yaml` must define each permitted command as an allowlisted command template rather than enabling arbitrary shell execution:

```yaml
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

The implementation must resolve executable paths safely, validate command profiles, prevent argument injection, bound CPU/memory/time/output, and use OS/container sandboxing where available. If a command template cannot safely represent a command (for example, shell globs may be interpreted by the executable differently), use a tested command-template schema rather than passing its string to a shell. Never execute a shell command assembled from LLM output.

### 9. Safe execution

Do not execute arbitrary shell commands or arbitrary Rust code just because an agent requests it. Tools must be registered, typed, permission checked, and dispatched by the runtime. Separate a tool's declaration from its implementation. Filesystem and terminal tools must use scoped workspaces, explicit allowlists, and a restricted execution environment. Secrets must be redacted. Set request and execution timeouts. Prevent runaway loops and unbounded agent spawning.

### 10. Full language scope and non-negotiable 1.0 requirements

Axiom 1.0 is a complete domain-specific programming language, not just a prototype. The following are required for the 1.0 release. Do not move them into a “future work” section to simplify the implementation. Implement them in phases, keep the repository usable at each gate, and make trade-offs explicit in ADRs.

#### 10.1 Modules and project structure

- Multiple `.ax` source files per project.
- `module` declarations and relative imports; named imports and exports.
- Deterministic module resolution, cycle detection, duplicate-name diagnostics, and no arbitrary code execution during import.
- A documented project manifest / configuration convention and standard library module namespace.
- Stable source locations in errors, diagnostics, traces, and editor integrations.
- A documented path/module resolution rule that works identically on Windows, macOS, and Linux.

#### 10.2 Type system

Implement a statically checked type system with:
- Primitive types: `string`, `bool`, `int`, `float`, `bytes`, `duration`, `timestamp`, and `null`.
- Arrays/lists, tuples, maps, records/objects, enums, optional fields, and union types.
- Named aliases, interfaces/structural records, generic types and functions, type inference for local values, and explicit function/agent input/output types.
- Built-in `Option<T>` and `Result<T, E>` types, pattern matching, and exhaustiveness checks where possible.
- Function types, tool signatures, agent signatures, and task/workflow result types.
- Type checking across module boundaries and at model/JSON/provider boundaries.
- Explicit conversion/parsing APIs; do not silently coerce arbitrary model output to a declared type.
- Clear distinction between compile-time type guarantees, runtime schema validation, policy enforcement, and model-evaluated claims.

Do not attempt to clone every TypeScript feature. Axiom’s type system must be internally coherent and fully specified. Avoid implicit `any`/dynamic escape hatches. If dynamic JSON is needed, make it explicit as `JsonValue` with validation when converted to a typed value.

#### 10.3 Declarations and execution semantics

The normative grammar must include and the implementation must support:
- `module`, `import`, `export`.
- `type`, `interface`, `enum`, `const`/`let` bindings.
- Pure functions and effectful functions with declared effects.
- `agent`, `team`, `tool`, `contract`, `goal`, `policy`, `workspace`, and `workflow` declarations.
- Expressions: literals, lists, objects, identifiers, member access, indexing, calls, unary/binary operators, comparisons, boolean logic, null/option handling, lambdas, and conditional expressions.
- Statements: local declarations, assignment where permitted, expression statements, `if/else`, `match`, `for`, bounded `while`, `return`, `break`, `continue`, `throw`, `try/catch/finally`, `require`, and explicit `await` for async operations.
- Generic declarations and calls, type annotations, optional arguments/default values where specified, named arguments, and overload rules if overloads are chosen.
- Workflow statements: `delegate`, `parallel for`, `require`, `retry`, `timeout`, `on_error`, `escalate`, `checkpoint`, `emit`, and verified integration operations.
- Deterministic evaluation order, variable scope rules, error propagation rules, and cancellation semantics.
- Bounded loops, recursion depth, call depth, wall-clock time, and overall instruction/step limits to stop runaway execution.

Use a coherent semicolon/newline policy, fully document operator precedence and associativity, and test ambiguous syntactic forms. No syntax feature may exist in documentation without parser/type-checker/runtime semantics or an explicit compile-time diagnostic explaining that the feature is unsupported in the current language version.

#### 10.4 Effects and capabilities

- Distinguish pure expressions from effects such as filesystem access, artifact writes, external model calls, command execution, workspace changes, network access, and integration operations.
- Declare required effects on functions/tools/workflows where applicable and check them statically when possible.
- At runtime, intersect declared effects with actual tool registry capabilities, workspace permissions, configuration policy, and invocation authority.
- A model response must never add a capability or widen permissions.
- The compiler must distinguish a tool being declared, a tool being available to a specific agent, and a tool being authorized in the current execution context.

#### 10.5 Agent, team, goal, contract, and policy semantics

- Agent definitions have purpose/instructions, typed input and output, model profile alias, tools, workspace, constraints, output validation, limits, and authority.
- Agent instances may override a profile and safe runtime parameters but cannot widen security policy unless an authorized policy explicitly allows it.
- Teams compose agents and define task routing, delegation, handoff rules, and shared policy.
- Goals include objective, typed inputs/outputs, constraints, completion criteria, decomposition strategy, and responsible authority.
- Tasks have stable IDs, explicit dependencies, input/output contracts, owned paths/artifacts, budgets, and accepted/rejected/blocked states.
- Contracts support preconditions, postconditions, invariants, schemas, version identifiers, and executable evidence checks.
- Contract changes are proposed and versioned; an existing task may not silently continue under changed assumptions.
- Policies support tool access, filesystem scopes, human approval, budget limits, network/command restrictions, retention, and data-handling rules.
- Subjective constraints must be identified as model-evaluated or human-reviewed, not advertised as formally proven.

#### 10.6 Complete workflow behavior

- Deterministic steps, agent decisions, conditional branches, pattern matching, bounded loops, task decomposition, dependencies, bounded parallelism, retries, error handlers, timeouts, cancellation, checkpoints, and escalation.
- Declarative workflow graph inspection before execution, deterministic scheduling rules where possible, and rejection of cyclic or invalid task dependencies.
- Runtime-governed task state transitions; model output cannot mark its own task accepted.
- Explicit distinction between retryable provider failures, invalid model output, policy violations, test failures, and permanent errors.
- Backpressure and maximum concurrent calls; budget enforcement for tokens/cost when usage/pricing data are available, plus hard wall-clock and call-count limits.
- Durable execution state and resume support for supported workflows; idempotency keys and documented semantics for retries around external side effects.
- Human approval gates and a safe pause/resume path.
- A run must have a durable ID and a trace linking source version, config version (with secrets redacted), task graph, provider/model aliases, tool calls, artifacts, and final state.

#### 10.7 Full filesystem, workspace, and artifact standard library

Axiom 1.0 must expose a typed filesystem/workspace API, not merely mention filesystem support in the prompt. Required surface area includes:
- `fs.list`, `fs.walk`, `fs.stat`, `fs.exists`, `fs.read_text`, `fs.read_bytes`, `fs.read_many_text`, `fs.search`, `fs.hash`, `fs.write_text`, `fs.write_bytes`, `fs.create_dir`, `fs.apply_patch`, `fs.diff`, `fs.move`, and `fs.delete` (delete denied unless explicitly authorized).
- `workspace.create`, `workspace.fork`, `workspace.inspect`, `workspace.submit`, `workspace.discard`, `workspace.snapshot`, `workspace.restore`, and `workspace.integrate` with policy-controlled strategies.
- Artifact publication and retrieval for patches, source snapshots, test logs, test reports, plans, reviews, and release records.
- Stable artifact IDs, content hashes, provenance, task/contract versions, size limits, retention rules, and access policy.
- Local rooted workspace implementation, per-task isolated workspace implementation, in-memory test backend, and a Git worktree backend for real repositories.
- Conflict detection, base revision checks, unsubmitted-write isolation, validated patch application, and atomic integration gates.
- Scoped permissions and protected paths enforced below the language/tool layer, with symlink and TOCTOU considerations addressed using capability-oriented filesystem APIs where possible.
- No default host-root access; no implicit access to `.git`, `.env`, key files, credentials, or secrets.
- File/directory counts, read/write bytes, diff size, search result size, path depth, and tool duration quotas.
- Text encoding and binary handling rules, newline preservation behavior where practical, deterministic diff format, and stable file ordering.

The implementation must include the full end-to-end example in Section 8a as a compiling, type-checked program and integration test. Do not merely keep it as pseudocode. The filesystem API must be usable both by deterministic workflow code and as registered tools invoked by model agents. In both cases, Rust runtime policy is authoritative.

#### 10.8 Model integration and multimodel execution

- A provider-neutral request/response/error model and a documented Rust adapter extension contract.
- Separate integration connection, adapter protocol, model profile, agent declaration, instance override, and per-call override.
- Configuration-only support for endpoints implementing a supported protocol.
- Direct/native adapters and contract tests for Anthropic Messages (Claude), DeepSeek's documented endpoint, Moonshot/Kimi's documented endpoint, a generic OpenAI-compatible API, and a mock provider.
- A gateway-compatible path for services such as OpenRouter or LiteLLM through supported protocols.
- Runtime capability negotiation for structured outputs, tool calling, streaming, vision, embeddings, and reasoning controls; unsupported features must fail explicitly.
- Config precedence and credential resolution rules, secret redaction, request limits, rate-limit handling, cancellation, timeouts, retry classification, and response-size limits.
- Per-task model selection, profile fallback rules if configured, and optional route selection based on capability/cost/latency constraints, with traceability.
- Model IDs are arbitrary user-supplied identifiers, not compiler enums or hardcoded model-name lists.
- Provider integration tests use mock HTTP servers; real credentials are optional and never needed for standard CI.

#### 10.9 Tools, commands, and extension points

- Typed tool declarations with input/output schemas, effects, permissions, timeout, idempotency, output limits, and error types.
- Tool registry and per-agent allowlist; runtime independently checks that the tool is declared, registered, and authorized.
- Command profiles are named allowlisted executable plus argument-template definitions in configuration. Do not execute arbitrary shell strings from a model response.
- Commands run in the agent’s workspace with bounded execution and a restricted environment; network defaults to disabled.
- A documented in-process Rust extension interface for custom tools and model adapters, versioned compatibility expectations, and test harnesses.
- Do not execute arbitrary dynamic libraries downloaded at runtime or run arbitrary Rust/JavaScript source from `.ax` files.

#### 10.10 Standard library

Publish typed, documented modules for at least:
- `core`: primitive utilities, equality, conversions, result/option helpers, assertions.
- `collections`: list, map, set-like utilities, stable sorting/filtering/grouping.
- `json`: typed parse, serialize, schema validation, safe error reporting.
- `time`: duration, timestamp, deadline and timeout helpers.
- `env`/`secrets`: explicitly scoped environment reads and secret references with redaction.
- `fs`: the filesystem API defined above.
- `workspace`: task workspace lifecycle and integration.
- `artifact`: publish, fetch, verify, and query artifacts.
- `agent`: invocation metadata and safe delegation helpers.
- `workflow`: state, checkpoint, events, cancellation and escalation helpers.
- `command`: execute only named command profiles.
- `logging`/`trace`: structured logs and trace events with redaction.

Standard library functions must have declared types and effects, error behavior, documentation, and unit tests.

#### 10.11 Developer experience and tooling

Axiom 1.0 requires:
- CLI commands for project initialization, syntax/type/config check, format, lint, explain/IR, plan/dry-run, run, inspect/status, resume, cancel, test, provider/capability inspection, and artifact inspection.
- Deterministic formatter (`axiom fmt`) with check mode and golden tests.
- Linter for unused declarations/imports, suspicious configuration, unbounded loops, insecure filesystem scopes, unhandled errors, missing contract criteria, unsafe tool grants, and unreachable workflow branches.
- Language Server Protocol support for diagnostics, completion for declarations/standard-library APIs/config aliases, go-to-definition, references, hover/type info, formatting, and symbol outline.
- Human-readable and machine-readable diagnostics; stable error codes suitable for CI.
- Debug/trace inspection for workflow graph, task state, provider/model selection, tool calls, filesystem operation metadata, artifact provenance, costs/latency when available, and rejection reasons.
- No secrets or sensitive file contents in debug traces by default.

#### 10.12 Persistence, recovery, and observability

- Durable run/task state storage behind a trait, with SQLite as a practical default local backend and an in-memory test backend.
- Checkpointing at safe workflow boundaries and resumability after process restart for workflows declared resumable.
- Explicit treatment of at-least-once retries and side effects; use idempotency keys or require explicit acknowledgement where a side effect cannot be made idempotent.
- Structured events and trace spans for run creation, agent call, tool call, task state transition, filesystem operation, artifact publication, verification, approval, integration and failure.
- Retention policy and redaction for sensitive data, configurable trace payload policies, and trace correlation IDs.
- Durable state schema versioning and tested migrations.

#### 10.13 Security and reliability

- Threat model covers prompt injection via repository files, model output as untrusted data, path traversal, symlink/TOCTOU attacks, command injection, credential leakage, unbounded spending, denial of service, unsafe integration, concurrent edit conflicts, malicious tool implementations, and compromised provider responses.
- Enforce access control in Rust runtime and filesystem backend, never only in prompts or DSL declarations.
- Validate all model output before use in typed values, plans, file patches, paths, command arguments, or permissions.
- Keep model-produced repository content untrusted; repository text cannot override system policy.
- Redact secrets and restrict trace content; avoid embedding raw secret values in errors/config dumps.
- Fuzz lexer/parser/config loader, property-test path normalization and permission matching, and test concurrency/races where feasible.
- Security decisions and residual risks are documented, not hand-waved.

#### 10.14 Language specification and compatibility

- `docs/language-spec.md` is normative: grammar, lexical rules, types, modules, execution order, effects, errors, async semantics, workflow state transitions, filesystem APIs, contracts, config binding, and version compatibility.
- `docs/grammar.ebnf` or equivalent machine-readable grammar.
- `docs/standard-library.md`, `docs/providers.md`, `docs/configuration.md`, `docs/filesystem-and-workspaces.md`, `docs/runtime-and-workflows.md`, `docs/security-model.md`, `docs/tooling.md`, and `docs/migration-and-compatibility.md`.
- Language version declaration and compatibility policy. Reject unsupported language versions clearly.
- Conformance tests for each normative language feature; every documented example compiles and runs using mock/local test facilities.
- A feature matrix that labels each item implemented/tested, partially supported, or unsupported. Axiom 1.0 cannot be called complete while a required 1.0 feature is marked partial/unsupported.

### 10.15 Baseline grammar and syntax consistency

The complete normative grammar must be written to `docs/grammar.ebnf`, maintained with parser implementation, and validated by grammar/conformance fixtures. The following is a baseline production set to make the design concrete; Phase 0 must expand it into a complete grammar including every declaration, expression, type, statement, and lexical rule named above. Do not leave productions implied by examples.

```ebnf
program          = [ module_decl ], { import_decl }, { declaration } ;
module_decl      = "module", qualified_name, terminator ;
import_decl      = "import", "{", identifier_list, "}", "from", string_literal, terminator ;
export_prefix    = [ "export" ] ;
declaration      = export_prefix,
                   ( type_decl | interface_decl | enum_decl | const_decl |
                     function_decl | agent_decl | team_decl | tool_decl |
                     contract_decl | goal_decl | policy_decl | workspace_decl |
                     workflow_decl ) ;

type_decl        = "type", identifier, [ type_parameters ], "=", type_expr, terminator ;
interface_decl   = "interface", identifier, [ type_parameters ], "{", { property_decl }, "}" ;
enum_decl        = "enum", identifier, "{", enum_member_list, "}" ;
function_decl    = [ "pure" ], "fn", identifier, [ type_parameters ], "(", [ parameter_list ], ")",
                   [ "->", type_expr ], [ effect_clause ], block ;
agent_decl       = "agent", identifier, "implements", type_expr, block ;
team_decl        = "team", identifier, block ;
tool_decl        = "tool", identifier, "(", [ parameter_list ], ")", "->", type_expr, effect_clause, block ;
contract_decl    = "contract", identifier, block ;
goal_decl        = "goal", identifier, "(", [ parameter_list ], ")", "->", type_expr, block ;
policy_decl      = "policy", identifier, block ;
workspace_decl   = "workspace", identifier, block ;
workflow_decl    = "workflow", identifier, "(", [ parameter_list ], ")", [ "->", type_expr ], block ;

block            = "{", { statement | declaration }, "}" ;
statement        = binding_stmt | assignment_stmt | if_stmt | match_stmt | for_stmt |
                   while_stmt | return_stmt | break_stmt | continue_stmt | throw_stmt |
                   try_stmt | require_stmt | await_stmt | delegation_stmt |
                   parallel_stmt | retry_stmt | checkpoint_stmt | emit_stmt |
                   expression_stmt ;
binding_stmt     = ( "let" | "const" ), identifier, [ ":", type_expr ], "=", expression, terminator ;
assignment_stmt  = assignable, assignment_operator, expression, terminator ;
if_stmt          = "if", expression, block, [ "else", ( block | if_stmt ) ] ;
match_stmt       = "match", expression, "{", { match_arm }, "}" ;
for_stmt         = "for", identifier, "in", expression, block ;
while_stmt       = "while", expression, block ;
return_stmt      = "return", [ expression ], terminator ;
throw_stmt       = "throw", expression, terminator ;
try_stmt         = "try", block, { "catch", [ "(", identifier, ")" ], block }, [ "finally", block ] ;
require_stmt     = "require", expression, [ "else", ( expression | "escalate", expression ) ], terminator ;
await_stmt       = "await", expression, terminator ;
delegation_stmt  = "delegate", qualified_name, ".", identifier, "(", [ argument_list ], ")", terminator ;
parallel_stmt    = "parallel", "for", identifier, "in", expression, block ;
retry_stmt       = "retry", expression, block ;
checkpoint_stmt  = "checkpoint", [ expression ], terminator ;
emit_stmt        = "emit", expression, terminator ;
expression_stmt  = expression, terminator ;

expression       = conditional_expr ;
conditional_expr = binary_expr, [ "?", expression, ":", expression ] ;
binary_expr      = unary_expr, { binary_operator, unary_expr } ;
unary_expr       = [ unary_operator ], postfix_expr ;
postfix_expr     = primary_expr, { member_access | index_access | call_suffix } ;
primary_expr     = literal | identifier | list_literal | object_literal |
                   lambda_expr | await_expr | delegate_expr | parallel_expr |
                   "(", expression, ")" ;
await_expr       = "await", expression ;
delegate_expr    = "delegate", qualified_name, ".", identifier, "(", [ argument_list ], ")" ;
parallel_expr    = "parallel", "for", identifier, "in", expression, block ;
list_literal     = "[", [ argument_list ], "]" ;
object_literal   = "{", [ object_entry_list ], "}" ;
lambda_expr      = "(", [ parameter_list ], ")", "=>", ( expression | block ) ;
member_access   = ".", identifier ;
index_access    = "[", expression, "]" ;
call_suffix      = "(", [ argument_list ], ")" ;

type_expr        = union_type ;
union_type       = generic_type, { "|", generic_type } ;
generic_type     = qualified_name, [ "<", type_argument_list, ">" ]
                 | "[", type_expr, "]"
                 | "{", [ property_type_list ], "}" ;
property_decl    = identifier, [ "?" ], ":", type_expr, terminator ;
parameter        = identifier, [ "?" ], ":", type_expr, [ "=", expression ] ;
type_parameters  = "<", identifier_list, ">" ;
parameter_list   = parameter, { ",", parameter }, [ "," ] ;
argument_list    = argument, { ",", argument }, [ "," ] ;
argument         = [ identifier, ":" ], expression ;
qualified_name   = identifier, { ".", identifier } ;
terminator       = ";" | NEWLINE ;
```

This excerpt deliberately omits lexical character classes, operator precedence tiers, string escape grammar, comment grammar, generic constraints, destructuring rules, patterns, effect clauses, special agent/workflow member blocks, and configuration grammar. Those are mandatory parts of the completed specification, not reasons to leave the grammar partial. The completed EBNF must match the final parser exactly. If a construct is syntactic sugar, specify its desugaring into IR and test that transformation.

Syntax consistency requirements:
- Use `integrations:` in YAML for endpoint connections and `models:` for model profiles throughout all documentation; do not use `providers:` for the same concept in one example and `integrations:` in another.
- Keep `integration` as the reference key inside a model profile; do not alternate with `provider` without a documented backward-compatible alias.
- Use semicolons or newline terminators consistently. The lexer must distinguish newline terminators from newlines inside nested expressions and comments.
- Define whether `await` is required for filesystem, model, artifact, and command operations. The recommended rule is that every asynchronous/effectful operation is explicitly awaited in workflow/function execution contexts.
- Define named argument syntax consistently (e.g. `max_results: 100`) and use it in all examples.
- Every imported symbol, workspace name, agent, type, contract, model alias, tool, command profile, and standard-library method in a normative example must either be declared locally or imported from a file that declares it. The files referenced by `filesystem_feature_workflow.ax` (including `types.ax` and `agents.ax`) must be committed as complete compilable example modules; do not leave placeholder imports for undeclared symbols.
- Examples must type-check and execute in conformance tests; if an example is intentionally abbreviated, label it as a fragment and never use it as an acceptance-gate fixture.

---

## Required Rust project structure

Use a Cargo workspace with meaningful, acyclic boundaries unless repository constraints clearly justify a modular single crate. Target a structure along these lines:

```text
axiom/
  Cargo.toml
  crates/
    axiom-source/       # source maps, spans, lexer, parser, AST, grammar tests
    axiom-diagnostics/  # diagnostics, error codes, CLI/editor formatting
    axiom-types/        # types, symbols, module graph, semantic/effect checks
    axiom-ir/           # typed IR, workflow graph, validation and serialization
    axiom-config/       # YAML schema, env/secret resolution, config validation
    axiom-providers/    # provider-neutral types, registry, capability model
    axiom-adapter-openai-compatible/
    axiom-adapter-anthropic/
    axiom-adapter-mock/
    axiom-filesystem/   # capability FS, scopes, workspaces, patches, git worktrees
    axiom-artifacts/    # artifact/evidence model and store
    axiom-tools/        # typed tool registry and command profiles
    axiom-runtime/      # interpreter/scheduler, agents, workflows, policy gates
    axiom-state/        # durable state store, checkpoints, migrations
    axiom-stdlib/       # built-in functions/modules
    axiom-format/       # formatter and formatter tests
    axiom-lsp/          # language server
    axiom-cli/          # CLI commands
  examples/
    axiom.yaml
    src/types.ax
    src/agents.ax
    src/team.ax
    filesystem_feature_workflow.ax
    local_and_cloud.ax
  tests/
    conformance/
    fixtures/
    integration/
    security/
  docs/
    language-spec.md
    grammar.ebnf
    architecture.md
    configuration.md
    providers.md
    filesystem-and-workspaces.md
    runtime-and-workflows.md
    standard-library.md
    security-model.md
    tooling.md
    migration-and-compatibility.md
    decisions/
```

The names can be changed with a documented reason. Do not introduce crate boundaries that produce dependency cycles. The language source, type system, and IR must not depend on a specific provider adapter. Providers must not depend on the parser. Runtime policies are applied independently of the LLM. Do not let architecture diagrams substitute for working tests.

Use maintained dependencies appropriate to current stable Rust. Evaluate async runtime, HTTP client, serialization, parser/lexer, diagnostics, CLI, SQLite/state, filesystem capability APIs, Git worktree support, LSP, property testing, and fuzzing. Check current official documentation before selecting versions. Record dependency rationale and update policy.

---

# Full Axiom 1.0 implementation phases

These phases build the complete language, not just a demo. Every phase has deliverables and acceptance gates. Do not relabel required features as optional or stop after the first working vertical slice.

## Phase 0 — Repository reconnaissance, formal design, and language specification

1. Inspect the repository, preserve relevant code, and identify existing constraints.
2. Write the normative Axiom 1.0 language specification, grammar/EBNF, type-system rules, evaluation order, module resolution, effect model, error semantics, async semantics, workflow execution semantics, and standard-library contract.
3. Produce architecture and threat-model diagrams plus ADRs for compiler architecture, provider adapter design, filesystem isolation, workspace lifecycle, state persistence, and language versioning.
4. Define the canonical source/config split: `.ax` for program semantics; `axiom.yaml` for integration endpoints, profiles, secrets references, runtime limits, and command profiles.
5. Define complete syntax and semantics for every required 1.0 construct before implementation starts. Create an explicit conformance matrix.
6. Define filesystem security boundaries, workspace states, artifact provenance, patch integration rules, conflict resolution and symlink/TOCTOU strategy.
7. Verify current official provider API docs for the initial adapters and record access dates/endpoints/capability caveats.
8. Define CI, formatting, linting, unit/integration/property/fuzz testing strategy and offline developer workflow.

**Gate:** A consistent specification and grammar exist; all example programs can be expressed in the design; no unresolved contradiction remains between examples, configuration, type semantics, and runtime policies.

## Phase 1 — Lexer, parser, AST, source maps, and diagnostics

Implement all declared 1.0 syntax, not just a handful of declarations:
- Lexing, comments, literals, escaped strings, numeric/time/duration literals, operators, tokens, source spans and source maps.
- Parser and typed AST structures for modules/imports/exports, types, functions, enums/interfaces, binding declarations, agents, teams, tools, goals, contracts, policies, workspaces, workflows, expressions and all control-flow forms.
- Precise recovery and multiple diagnostics without panics on malformed input.
- Syntax/config version handling and good error context.
- Parser fixtures for all normative grammar productions and negative fixtures for invalid forms.

**Gate:** Every construct in the formal grammar has positive and negative tests; invalid input produces stable source-located diagnostics, and fuzz-generated input does not panic.

## Phase 2 — Module loading, type system, symbol resolution, and effects

Implement module graph resolution; imports/exports; duplicate, missing and cyclic references; type aliases, interfaces, enums, unions, collections, generics, inference, function types, Option/Result; function signatures; agent and tool signature checks; workflow value typing; contract schemas; capability/effect annotations; and source/config cross-validation.

Enforce effects statically where possible. For example, a pure function cannot call filesystem-write or external-model effects. Runtime policy remains authoritative for actual permissions.

**Gate:** Conformance tests reject every documented type/effect error class before execution and accept valid generic/module examples.

## Phase 3 — Deterministic evaluator, typed IR, compiler, and plan inspection

Implement a deterministic execution core for ordinary language expressions and statements, typed IR lowering, workflow graph validation, control-flow analysis, effect analysis, dead/unreachable code warnings, dependency validation, loop/recursion limits, and stable IR encoding.

Provide `axiom explain`, `axiom plan`, and dry-run output that show task graph, model aliases, tool permissions, workspaces, required effects, checks, and integration gates—without exposing secrets.

**Gate:** All deterministic language conformance examples execute without a model provider; compiler output is stable and inspectable.

## Phase 4 — Configuration system and full model-integration layer

Implement strict YAML configuration schema, explicit environment/secret resolution, validation, redaction, integration registry, model profiles, capability declarations, precedence rules, native/generic adapters and the extension contract. Implement the documented initial adapter targets (Anthropic Messages, DeepSeek, Kimi/Moonshot, generic OpenAI-compatible, mock) as described above, plus gateway configurations over supported protocols.

Support normalized chat/tool/structured-output/streaming behaviors where implemented, adapter-level extension fields, provider errors, cancellation, timeouts, rate limits, response limits, request IDs, usage metadata and safe retry classification. Do not claim a feature works for a vendor unless tests cover its actual protocol behavior or explicitly mark it unsupported.

**Gate:** Adapter contract tests pass offline with local mock HTTP servers; profile switching changes no `.ax` agent source; configuration errors are caught before execution where possible.

## Phase 5 — Typed tools, secure filesystem, workspaces, artifacts, and command profiles

Implement the complete standard filesystem/tool subsystem in Section 10.7, including rooted local FS, in-memory test FS, per-task isolated workspaces, Git worktree backend, scoped tools, patch/diff/merge operations, artifact/evidence storage, conflict checks, quotas, protected paths, path/symlink containment and audit events.

Implement typed tools and named, allowlisted command profiles with safe argument binding, timeouts, bounded output, restricted environment, and network-off defaults. Do not expose unrestricted `sh -c` or `cmd /c` execution.

**Gate:** Security/property tests show agents cannot escape roots, cross denied paths, observe unsubmitted task changes, exceed quotas, or apply conflicting/unreviewed patches. Tool checks are enforced by Rust even if an agent prompt asks to bypass them.

## Phase 6 — Agent runtime, teams, goals, contracts, and model invocation

Implement agent construction from declarations; default runtime integration; configured model profile resolution; prompt/context assembly; registered-tool request/response loops; typed output decoding/validation; per-agent policies, constraints and resource limits; task and goal evaluation; team routing; and auditable trace events.

Define a strict rule for model-directed tool use: a model can request only tools exposed to it, and each call is independently validated against schema, policy, workspace, budget, and state. Agent-produced plans are untrusted inputs until validated.

**Gate:** Agent, team, goal, tool and contract examples run end-to-end with a deterministic fake provider. Invalid output or unauthorized tool calls cannot result in acceptance.

## Phase 7 — Complete hybrid workflow engine

Implement deterministic steps alongside AI decisions, delegation and handoffs, sequential and parallel tasks, dependency scheduling, conditionals, pattern matches, loops, retries, exception handlers, timeouts, cancellation, human approval, escalation, checkpoints, plan validation, contract-version checks, conflict routing, verification gates, acceptance and release states.

Define idempotency, side-effect ordering, compensation/error behavior, and which operations can safely be retried. Parallel task outputs remain isolated artifacts until explicit sharing or integration.

**Gate:** Full mixed workflow conformance and integration tests pass; failed tests/contracts block acceptance; the model cannot alter runtime state or skip an IR gate through natural-language output.

## Phase 8 — Durable state, recovery, run inspection, and observability

Implement durable state storage (SQLite local backend plus in-memory tests), schema migrations, run/task IDs, checkpoints, event log, resume/cancel/status commands, retry idempotency, trace correlation, usage/cost/latency summaries where available, artifact provenance and configurable data retention/redaction.

**Gate:** Kill-and-resume integration tests demonstrate recovery at supported checkpoints; sensitive values are absent from persisted logs and traces; migrations and retry semantics are tested.

## Phase 9 — Standard library and repository-driven development capabilities

Complete and document all standard-library modules. In particular, make filesystem and workspace operations available both as deterministic typed calls in workflows and as permission-checked tools for agents. Implement the normative end-to-end filesystem example in Section 8a as a real integration test, not pseudocode.

**Gate:** The filesystem example compiles, runs using a temporary repository and mock model, creates/uses isolated task workspaces, generates patch artifacts, runs allowlisted checks, detects conflicts, blocks unauthorized paths, and integrates only after required evidence passes.

## Phase 10 — CLI, formatter, linter, language server, and debugging

Implement all Axiom 1.0 developer commands: project init, check, format, lint, explain, plan/dry-run, run, status/inspect, resume, cancel, test, provider/profile/capability inspection, artifact inspection, and trace inspection. Build a deterministic formatter, rules-based linter, and LSP diagnostics/completions/go-to-definition/references/hover/formatting/symbol-outline.

**Gate:** CLI snapshots, formatter golden tests, linter tests, and LSP protocol tests pass; documented examples work with editor diagnostics.

## Phase 11 — Conformance suite, security validation, fuzzing, and performance

Build language-wide conformance fixtures; property tests for types, configuration merging, path/permission enforcement, patch application, state transitions and workflow scheduling; parser/config fuzz targets; provider contract tests; concurrency/race tests; redaction tests; and performance baselines for parse/type-check/plan/runtime overhead.

Security cases must include malicious prompt-injection strings inside source files, model output attempting path traversal, command injection, secret exfiltration requests, unbounded task spawning, oversized responses, corrupted provider payloads, patch conflicts, timeouts, repeated retries, symlink and TOCTOU attacks, and malformed configuration.

**Gate:** All mandatory tests pass; limitations and residual threat assumptions are documented; no known critical permission bypass remains unaddressed.

## Phase 12 — Complete documentation, examples, packaging, and compatibility

Complete the normative language reference, grammar, tutorial, standard-library reference, provider integration guide, custom adapter/tool developer guide, filesystem/workspace guide, workflow/runtime guide, security model, deployment/configuration guide, LSP guide, troubleshooting, migration/versioning policy, and release notes.

Maintain runnable examples for:
- Hello-world deterministic program and a single agent.
- Multiple models/providers and local/cloud profile switching.
- Generic custom OpenAI-compatible integration.
- Agent/goal/contract/workflow composition.
- Full filesystem feature implementation and gated integration.
- Failure/retry/human approval and checkpoint/resume.
- Custom Rust tool and custom provider adapter.

**Gate:** CI compiles and exercises every documentation example; installation/package tests work from a clean environment; compatibility/version checks are covered.

## Phase 13 — Release candidate and Axiom 1.0 sign-off

1. Review all requirements in this prompt against the feature conformance matrix.
2. Confirm no normative Axiom 1.0 feature is marked future, simulated, or unsupported.
3. Run all formatting, lint, unit, integration, security, conformance, property, and available fuzz suites.
4. Verify a clean build from a clean checkout on supported platforms, or document platform scope precisely.
5. Produce `docs/release-readiness.md` with evidence, commands, results, version, known issues, residual risks, supported providers/capabilities, and reproducible setup.
6. Do not say “complete” if any required gate failed. State exactly what remains and keep the implementation ledger current.

**Gate:** Axiom 1.0 is complete only when every required 1.0 feature, test, documentation artifact, and release gate is implemented and verified.

---

## Testing and engineering rules

- The project must compile and tests must pass at the end of every phase.
- Run `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` where applicable, and `cargo test --workspace`; document any justified exceptions.
- Keep normal CI offline and deterministic through fake providers and deterministic tools. Real API tests are optional, separately gated, and must not be required for standard CI.
- Use mock HTTP servers for provider request/response contract testing.
- Add unit, integration, conformance, property, fuzz, security and concurrency tests in the phases where their behavior is introduced.
- Never swallow errors or convert provider/tool failures into apparent success.
- Avoid panics on user-authored source, YAML, model output, patch data, and provider responses.
- Use structured errors and stable error codes, with source spans when available.
- Make cancellation, timeout, retry, budget, side-effect and idempotency semantics explicit.
- Never log credentials, authorization headers, secret values, or sensitive file contents by default.
- Enforce tool and filesystem permissions in Rust; natural-language prompt instructions are not a sandbox.
- Never expose the host filesystem by default. Use capability-oriented rooted access and enforce containment, scopes, protected paths and quotas.
- Isolate concurrent writable workspaces. Integrate patch artifacts only through conflict detection and verification gates.
- Do not build custom cryptography or a homegrown secret vault. Use standard, reviewed mechanisms and secret references.
- Pin or constrain dependency versions according to repository policy and review current stable documentation before choosing APIs.
- Record architectural changes in ADRs and update spec, grammar, fixtures, examples, and implementation in the same change.
- Keep an implementation ledger with columns: requirement, phase, status, test evidence, documentation, known limitations.
- Do not optimize for minimum code size. Optimize for clear semantics, predictable execution, extensibility, security and maintainability.
- Do not silently discard unknown configuration fields or provider options; reject them or handle them through explicit adapter extension namespaces.
- No undocumented language construct, standard-library function, provider capability, or security guarantee may be represented as supported.

## Definition of Done — Axiom 1.0

Axiom 1.0 is done only when **all** of the following are true:

1. The normative language specification and grammar cover every supported syntax and runtime behavior.
2. The parser, AST, module system, symbol resolver, static type checker and effect checks support the complete declared 1.0 language.
3. Deterministic code compiles to typed IR, can be inspected/dry-run, and executes according to documented semantics.
4. Agents, teams, goals, tasks, contracts, policies, tools and workflows are real language constructs with tested semantics.
5. The same agent definition can use Claude, DeepSeek, Kimi, generic compatible endpoints, local models and gateways through profile/configuration changes or documented adapters—not source rewrites.
6. Provider extension APIs are documented, versioned, tested and independent of the DSL grammar and core workflow engine.
7. The full standard library exists with documented types, effects, errors and tests.
8. The filesystem/workspace/artifact system supports rooted local access, isolated per-task workspaces, Git worktrees, safe patches, conflict detection, evidence provenance, quotas and permission enforcement.
9. The end-to-end filesystem example in Section 8a is a compiling, tested workflow—not illustrative pseudocode—and actually reads/writes within scoped workspaces.
10. Hybrid workflows support bounded parallelism, retries, timeouts, cancellation, escalation, durable checkpoints/resume, contracts and verification gates.
11. Tools and command profiles are typed, registered, allowlisted and runtime-authorized. Model output cannot introduce permissions or execute arbitrary shell text.
12. Provider/model/profile override precedence, capability checks, secret resolution and redaction are tested.
13. CLI, formatter, linter and LSP functionality defined for 1.0 are implemented and tested.
14. Conformance, unit, integration, property, security and available fuzz suites pass; build and release checks are reproducible.
15. All documented examples compile and are exercised by CI where practical.
16. Security assumptions, unsupported provider capabilities, known limitations and residual risks are documented honestly.
17. The release-readiness report contains evidence for every required 1.0 feature. Nothing required is still marked prototype-only, simulated, optional, or deferred.

## Required behavior from the implementation agent

Begin by inspecting the repository and then create/update `docs/implementation-ledger.md`, `docs/language-spec.md`, `docs/grammar.ebnf`, and the architecture/ADR set. Do not stop after planning or after the first phase. Implement phases sequentially through Phase 13, committing or otherwise clearly isolating each phase's changes if repository practices allow it. Keep the ledger current and keep the project buildable. If the session or tooling limit prevents full completion, finish the current safe increment and report exact remaining requirements; do not relabel the work as complete. Ask the user a question only if a decision genuinely blocks safe progress and cannot be resolved with a documented, conservative default. Make reversible design choices autonomously and record them.
