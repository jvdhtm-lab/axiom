# Axiom — Rust Agent Language: Master Implementation Prompt

## Mission

Build **Axiom**, a TypeScript-inspired domain-specific language (DSL) for defining AI agents, goals, abstractions, contracts, tools, policies, and hybrid workflows. Implement its compiler and runtime in Rust.

Axiom must combine:
- **Procedural control** for deterministic operations, checks, branching, retries, and security boundaries.
- **Agentic control** for planning, interpretation, bounded decisions, decomposition, and delegated work.
- **Contract-driven collaboration** so agents exchange typed inputs, outputs, task definitions, artifacts, and verification evidence.
- **Model portability** so an agent's role and source definition do not need to change when its underlying model changes.

Do not build a generic chatbot, a prompt-template wrapper, or a framework-specific DSL. Build a small but real language with a lexer/parser, AST, type checking, intermediate representation (IR), configuration system, model-adapter abstraction, runtime, CLI, tests, and documented examples.

Work incrementally. Complete the phases below in order. At each phase, leave the repository compiling and tests passing. Do not claim functionality is complete unless it is implemented and verified. When blocked, make the smallest reasonable design decision, document it, and continue rather than repeatedly asking the user questions.

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

### 2. Example source syntax (provisional)

Support a readable TypeScript-like syntax. Treat this as a target syntax, and refine it only when a decision is needed for a coherent, implementable grammar. Document changes in `docs/decisions/`.

```ts
// axiom.ax
integration local_llm {
  adapter: openai_compatible
  base_url: env("AXIOM_LOCAL_BASE_URL", "http://localhost:11434/v1")
  api_key: secret("AXIOM_LOCAL_API_KEY", optional: true)
  request_timeout: 120s
}

integration hosted_llm {
  adapter: openai_compatible
  base_url: env("AXIOM_HOSTED_BASE_URL")
  api_key: secret("AXIOM_HOSTED_API_KEY")
  request_timeout: 90s
}

model reasoning {
  integration: "hosted_llm"
  model: env("AXIOM_REASONING_MODEL", "model-name")
  capabilities: [chat, structured_output, tool_calling]
  defaults: { temperature: 0.2 }
}

model local_coding {
  integration: "local_llm"
  model: env("AXIOM_LOCAL_MODEL", "qwen3-coder:30b")
  capabilities: [chat]
}

agent Architect implements Agent<FeatureSpec, ExecutionPlan> {
  model: "reasoning"
  purpose: "Decompose requirements and design interfaces"

  tools: [repository.read, documentation.search]

  permissions {
    read: ["src/**", "docs/**"]
    write: []
  }

  constraints {
    must: ["Define explicit interfaces", "Preserve architectural boundaries"]
    must_not: ["Implement application code directly"]
  }
}

// Bind this instance to another profile without changing Architect itself.
let localArchitect = Architect.with { model: "local_coding" }
```

`env()` and `secret()` are illustrative built-ins; implement safe behavior, diagnostics, and tests. Secrets must never be logged or written into compiled plans, traces, or error messages.

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
- Optional imported `.ax` modules may define reusable agents and contracts; keep module/import behavior out of the MVP unless the phase plan explicitly includes it.

Illustrative `axiom.yaml`:

```yaml
version: 1

providers:
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
    provider: claude
    model: "${CLAUDE_MODEL}"
  coding:
    provider: deepseek
    model: "${DEEPSEEK_MODEL}"
  review:
    provider: kimi
    model: "${KIMI_MODEL}"
  local_coding:
    provider: local
    model: "${LOCAL_MODEL}"

runtime:
  default_model: reasoning
  max_parallel_agents: 4
  max_retries: 2
  request_timeout_seconds: 120
```

The exact YAML structure may be refined during Phase 0, but its semantics must remain clear and documented. Environment-variable expansion must be implemented explicitly; do not assume a YAML parser expands `${...}` automatically. Secret references must be resolved at runtime and redacted from logs, diagnostics, IR dumps, and traces. Avoid supporting arbitrary YAML tags or executable expressions.

The associated `.ax` file should refer to logical model aliases only:

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

Do not implement every vendor on earth in the MVP. Instead, make the adapter/integration boundary universal and test it with the named initial vendors. The stated goal is that **any model can be integrated through a supported protocol, a gateway, a configuration-only compatible endpoint, or a custom adapter**—not that every model's every feature is magically interchangeable.

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

The source language must make switching providers explicit and straightforward. Keep agent roles vendor-neutral; bind them to logical profiles, and bind those profiles to integration connections.

```ts
// Illustrative syntax; reconcile with the formal grammar in Phase 0.
integration claude_api {
  adapter: anthropic_messages
  base_url: env("ANTHROPIC_BASE_URL", "https://api.anthropic.com")
  api_key: secret("ANTHROPIC_API_KEY")
}

integration deepseek_api {
  adapter: openai_compatible
  base_url: env("DEEPSEEK_BASE_URL", "https://api.deepseek.com")
  api_key: secret("DEEPSEEK_API_KEY")
}

integration kimi_api {
  adapter: openai_compatible
  base_url: env("KIMI_BASE_URL", "https://api.moonshot.ai/v1")
  api_key: secret("KIMI_API_KEY")
}

integration local_models {
  adapter: openai_compatible
  base_url: env("AXIOM_LOCAL_BASE_URL", "http://localhost:11434/v1")
  api_key: secret("AXIOM_LOCAL_API_KEY", optional: true)
}

model deep_reasoning {
  integration: "deepseek_api"
  model: env("DEEPSEEK_MODEL", "deepseek-model-id")
}

model claude_architecture {
  integration: "claude_api"
  model: env("CLAUDE_MODEL", "claude-model-id")
}

model kimi_review {
  integration: "kimi_api"
  model: env("KIMI_MODEL", "kimi-model-id")
}

agent Architect implements Agent<FeatureSpec, ExecutionPlan> {
  model: "claude_architecture"
}

// Same agent role and contract, different model integration.
let localArchitect = Architect.with { model: "local_coding" }
let deepseekArchitect = Architect.with { model: "deep_reasoning" }
let kimiArchitect = Architect.with { model: "kimi_review" }
```

Exact endpoint URLs, model IDs, required headers, and API options must be validated against the current official documentation and overridable in configuration. Avoid baking changing model IDs into compiler logic.

Axiom should also allow a gateway profile, for example an OpenRouter or LiteLLM connection, as another integration—not as a replacement for direct integrations.

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

The MVP should represent at least:
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
4. **Runtime state store** — task state and workflow checkpoints. Keep this logically separate from source files; persistence backend selection may come later.

For the MVP, implement a provider-independent Rust filesystem interface with at least a rooted local-filesystem backend and an in-memory/fake backend for deterministic tests. Keep filesystem tools behind the same registered, typed, permission-checked tool system as every other tool.

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

Use isolated workspaces for parallel implementation. The MVP may use per-task temporary directories or a copy-on-write/patch-overlay abstraction; choose the simplest safe implementation and document it. Git worktrees can be an optional backend later, not a mandatory dependency. An agent's write scope should be the intersection of its workspace permissions and the workspace's actual filesystem capabilities.

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

### 9. Safe execution

Do not execute arbitrary shell commands or arbitrary Rust code just because an agent requests it. Tools must be registered, typed, permission checked, and dispatched by the runtime. Separate a tool's declaration from its implementation. Filesystem and terminal tools must use scoped workspaces, explicit allowlists, and a restricted execution environment. Secrets must be redacted. Set request and execution timeouts. Prevent runaway loops and unbounded agent spawning.

### 10. Scope discipline

Keep the MVP achievable. Avoid premature distributed execution, peer-to-peer negotiations, a self-hosted model server, a custom sandbox implementation, and an elaborate IDE. Define extension points without implementing every future feature. Keep all major design choices behind traits/modules where they meaningfully aid testing or future portability.

---

## Required Rust project structure (adjust if justified)

Use a Cargo workspace or a single well-modularized crate if that is the simpler MVP. Prefer clear boundaries resembling:

```text
axiom/
  Cargo.toml
  crates/
    axiom-syntax/       # lexer, parser, spans, diagnostics, AST
    axiom-types/        # type model and semantic validation
    axiom-ir/           # typed execution plan / intermediate representation
    axiom-providers/    # adapter trait, OpenAI-compatible adapter, mock provider
    axiom-filesystem/   # rooted filesystem, isolated workspaces, permissions, patches
    axiom-runtime/      # agent registry, tools, scheduler, verification, artifact handling
    axiom-cli/          # check, run, explain, validate-config commands
  examples/
    axiom.yaml
    hello_agent.ax
    local_and_cloud.ax
    feature_workflow.ax
  tests/
  docs/
    language-spec.md
    architecture.md
    providers.md
    filesystem-and-workspaces.md
    security-model.md
    decisions/
```

Do not split into many crates solely for appearance; keep dependency direction acyclic. If using one crate first, keep these as modules and preserve clear boundaries.

Potential ecosystem choices (choose based on current stable releases and compatibility; record exact decisions):
- Async runtime: Tokio.
- HTTP: Reqwest.
- Data/config serialization: Serde and serde_json.
- Async trait ergonomics: native async traits where suitable or `async-trait` if required for dynamic dispatch.
- Parser: choose a maintained parser approach such as `chumsky`, `logos` plus a parser, or a hand-written recursive-descent parser. Pick one only after reviewing grammar needs.
- Diagnostics: spans and precise source locations; consider Ariadne or miette.
- Errors: thiserror for library errors and anyhow at CLI/application boundaries if appropriate.
- Test doubles: mock provider and deterministic fake tools.

Do not assume crate names or APIs from memory. Verify current stable versions and inspect their documentation before choosing dependencies. Minimize dependencies and record the rationale.

---

# Phased implementation plan

Each phase must produce a commit-ready increment, tests, updated documentation, and a short completion report containing: implemented behavior, tests run and their results, known limitations, and exact next phase. Do not skip phase gates.

## Phase 0 — Repository reconnaissance and architecture decisions

Tasks:
1. Inspect the existing repository before changing it. Preserve useful code and conventions.
2. Write `docs/architecture.md`, `docs/language-spec.md` (initial draft), `docs/configuration.md`, and ADRs for core design decisions.
3. Define MVP features, non-goals, error-handling strategy, provider abstraction, config precedence, filesystem/workspace security boundaries, and provisional language grammar.
4. Formalize the division between TypeScript-inspired `.ax` source and YAML project configuration; document what belongs in each file.
5. Design the project filesystem, per-task workspace, artifact/evidence store, and runtime state-store boundaries. Specify path normalization, symlink policy, workspace isolation, patch submission, and integration conflict detection.
6. Choose and justify Rust dependencies after checking current stable documentation.
7. Establish formatting, linting, testing, and CI commands.

Acceptance gate:
- Architecture and grammar are understandable and mutually consistent.
- The `.ax` language and `axiom.yaml` configuration responsibilities are explicitly documented and do not overlap ambiguously.
- Project filesystem, isolated task workspaces, artifact/evidence storage, and workflow state are distinct concepts.
- Filesystem access rules and threat assumptions are documented before implementation begins.
- Integration connection, model profile, agent declaration, agent instance, and workflow are distinct.
- There is a thin vertical slice planned before broad feature development.
- No application runtime is claimed yet.

## Phase 1 — Lexer, parser, AST, and diagnostics

Implement:
1. `.ax` source loading.
2. YAML project configuration loading with strict schema validation, source locations where practical, and clear diagnostics.
3. Lexer/tokenizer with accurate byte spans or source spans.
3. Parser and AST for the initial grammar: integration, model, agent, type/interface, contract, goal, tool declaration, runtime config, and workflow declarations as selected in Phase 0.
4. Clear diagnostics for invalid characters, syntax errors, missing delimiters, duplicate declarations where detectable, and source locations.
5. CLI command `axiom check-syntax <file>` or an equivalent `axiom check` subcommand.

Tests:
- Valid `.ax` fixtures parse and valid YAML configuration fixtures load.
- Invalid fixtures produce stable, useful diagnostics for both source and configuration.
- Whitespace/comments and nesting are tested.
- YAML environment/secret placeholders are treated according to documented expansion rules, not implicit parser behavior.
- Parser has no panic on malformed input or fuzz-like generated invalid snippets.

Acceptance gate:
- Parsing is deterministic and has positive/negative tests.
- No model API calls are needed to parse a file.

## Phase 2 — Semantic analysis, symbols, types, and contract validation

Implement:
1. Symbol table and name resolution.
2. Duplicate name and unresolved reference diagnostics.
3. Type declarations and generic `Agent<Input, Output>` declarations.
4. Validation that agent inputs/outputs and workflow values align.
5. Integration/model-profile references and capability declarations across `.ax` source and `axiom.yaml` configuration.
6. Contract/task IDs or stable identifiers and contract-version representation.
7. Separate diagnostics for compile-time errors vs runtime-only checks.

Tests:
- Known-good and known-bad type fixtures.
- Unknown agent/model/integration/tool references fail clearly.
- Type mismatches and invalid overrides are caught.
- Contract revision compatibility rules are tested to the extent defined.

Acceptance gate:
- A syntactically valid but semantically invalid program is rejected before execution.

## Phase 3 — Intermediate representation and explain/dry-run mode

Implement:
1. Lower AST plus semantic output into a typed IR.
2. Represent agent nodes, deterministic steps, branches, dependencies, parallel groups, verification gates, retry policy, timeouts, and escalation where supported.
3. Validate graph consistency: invalid references, dependency cycles, impossible workflow edges, and missing required acceptance criteria.
4. Add CLI commands such as `axiom explain <file>` and `axiom plan <file>` (names may vary).
5. Support deterministic printing/export of the IR for testing and debugging.

Tests:
- Golden tests for IR.
- Cycle detection and invalid graph tests.
- Compilation of examples is stable.

Acceptance gate:
- Axiom can compile a source file into an inspectable plan without contacting an LLM.

## Phase 4 — Provider integration layer and model profiles

This is a core MVP phase, not an optional extra. The integration layer must make the same agent definition usable with Claude, DeepSeek, Kimi, compatible local models, and additional vendors added later.

Implement:
1. Model adapter trait and normalized request/response/error types.
2. Integration registry and configuration loading, distinguishing API protocol/adapter from provider name and model ID.
3. Generic OpenAI-compatible adapter with base URL, optional key, custom headers, timeout, generation parameters, and provider-specific options.
4. Native Anthropic Messages adapter for Claude, covering native request/response and tool-use semantics needed by supported features.
5. Direct DeepSeek configuration/adapter path based on the currently documented compatible API. Preserve options and response fields that matter for tool calls/reasoning where appropriate.
6. Direct Moonshot/Kimi configuration/adapter path, and/or a documented compatible-protocol mode for each supported endpoint. Validate exact endpoint behavior rather than assuming all Kimi APIs use the same URL/protocol.
7. Mock adapter for deterministic offline tests.
8. A plugin/extension interface so other integrations can be added without modifying parser, DSL grammar, IR, or workflow semantics.
9. Gateway connections for compatible services such as OpenRouter or self-hosted LiteLLM when they use a supported API protocol. Do not make gateway access the only integration method.
10. Adapter/model capability negotiation for chat, structured output, tool calling, streaming, vision/multimodal input, embeddings, and reasoning controls where in scope.
11. Profile aliases mapped to integration ID + model ID + optional settings; vendor-specific settings remain namespaced and validated by the adapter.
12. Global/project defaults and agent/instance/invocation overrides with precedence tests.
13. Environment variable/secret references, redaction, timeout, cancellation, bounded retry classification, and response-size limits.

Model IDs and integration endpoints must be configuration, not constants in compiler logic. Before implementing each direct adapter, check the provider's current official API documentation, record endpoint/protocol choices in `docs/providers.md`, and add links plus a retrieval date. Do not require online credentials for normal CI.

Tests:
- All unit and standard CI tests run offline with the mock adapter.
- Mock HTTP-server tests cover each adapter's request encoding, authentication, response normalization, tool-call decoding, streaming/event parsing if implemented, HTTP errors, rate limits, timeouts, and malformed responses.
- Contract tests run the same adapter-neutral test suite against every adapter where the feature applies.
- Verify that changing Claude to DeepSeek, Kimi, a local endpoint, or a gateway requires configuration/profile changes rather than editing the agent definition.
- Verify capabilities are reported accurately and unsupported operations fail explicitly.
- Verify secrets never appear in debug output, errors, trace events, or IR dumps.

Acceptance gate:
- The same `Architect` agent definition can be run against at least the native Claude adapter, DeepSeek integration, Kimi integration, and a compatible local endpoint by changing configuration/model profile only.
- The generic adapter supports custom compatible endpoints, and a documented adapter extension point exists for new vendors/protocols.
- Mock-provider tests prove integration portability without real API credentials.
- Unsupported capabilities fail clearly; do not silently degrade behavior.

## Phase 5 — Agent runtime, typed tools, and default integration

Implement:
1. An `AgentRuntime`/registry that turns compiled agent declarations into callable runtime agent instances.
2. Every agent uses the same runtime integration interface by default; custom integration is only needed for genuinely custom behavior.
3. A filesystem abstraction with a rooted local backend and an in-memory/fake backend. Implement isolated per-task workspaces, canonical root containment checks, default-deny path scopes, protected paths, bounded reads/writes, and safe patch creation/application primitives.
4. Typed filesystem tools (`fs.read`, `fs.list`, `fs.search`, `fs.write`, `fs.apply_patch`, `fs.diff`, and explicitly gated `fs.delete`) registered with input/output schemas and authorization checks. Provide fixture-backed tools for tests; do not expose unrestricted host filesystem access.
5. A logical artifact/evidence store for submitted patches, test reports, review results, and contract-versioned outputs. Keep artifacts distinct from untrusted model-generated text and record provenance.
6. Typed, registered non-filesystem tools with schemas, permissions, and dispatch. Do not add unrestricted shell access.
7. Prompt assembly from declared purpose/instructions, task input, tool descriptions, constraints, workspace summary, and relevant contract information.
8. Typed output parsing and schema validation with bounded, opt-in repair/retry behavior.
9. Structured tracing of agent run, selected provider/profile/model, latency, usage when provided, filesystem/tool operations, artifact IDs, and result state, without secrets or sensitive file contents.
10. Agent invocation through a Rust API and CLI command such as `axiom run <file> --agent Architect --input ...`.

Tests:
- Mock provider returns predictable outputs.
- Tool calls are only dispatched when declared and allowed.
- Filesystem tools cannot read or write outside the configured root, bypass deny rules, or exceed declared scopes.
- Path traversal, absolute-path escapes, symlink escapes, oversized files, and unauthorized deletion are rejected.
- Each concurrent task gets its own isolated writable workspace; one agent cannot see unsubmitted writes from another task unless an explicit shared artifact is passed.
- Submitted patches are validated and stored as artifacts before shared-project integration.
- Malformed model output produces a typed validation failure.
- Permission-denied actions are rejected by runtime.
- Invocation override beats instance, declaration, project, and runtime defaults in the specified order.

Acceptance gate:
- A basic agent can run end-to-end against the mock provider.

## Phase 6 — Goals, contracts, and hybrid workflow execution

Implement:
1. First-class goals with objectives, constraints, and explicit acceptance criteria.
2. Workflow execution for deterministic steps, agent delegation, conditional branches, bounded parallel tasks, verification gates, retries, timeout/cancellation, and escalation.
3. Per-task isolated workspace allocation for parallel implementation; agents submit patches/artifacts rather than directly mutating a common writable repository.
4. Conflict detection and an integration gate to apply accepted patches to the project workspace only after contract checks and required review/verification.
5. Plan/decomposition support where a planning agent may propose subtasks, but runtime validates subtask schemas, dependencies, depth, budget, workspace scopes, permissions, and contracts before scheduling.
6. State transitions controlled by the runtime, not by model-generated text. At minimum support planned/ready/running/submitted/verifying/accepted/rejected/blocked/escalated or a documented equivalent.
7. Event/artifact records for submitted patches/results and verification evidence, with contract version and task provenance.
8. Max depth, max parallel workers, token/cost limits when usage/pricing data are available, wall-clock limits, filesystem quotas, and retry ceilings.

Tests:
- Mixed deterministic + LLM-driven workflows.
- Parallelism respects bounds and dependency order.
- Parallel agents cannot overwrite one another's work through a shared writable directory.
- Conflicting patches are detected and routed to rework/resolution, not silently applied.
- Failed verification blocks acceptance.
- Agents cannot override policies or skip gates via generated output.
- Cancellation/timeouts and failure recovery are tested.

Acceptance gate:
- A small multi-agent feature workflow can run end-to-end with mock providers and deterministic tools.

## Phase 7 — Example application, documentation, and hardening

Implement:
1. A full `feature_workflow.ax` example: architect plans work, implementation agents produce independently scoped artifacts, a reviewer assesses changes, and a quality gate verifies evidence.
2. A `local_and_cloud.ax` example showing that the same agent can use a local model profile or a hosted endpoint through configuration.
3. Setup docs, language quick start, integration guide, filesystem/workspace guide, security model, and contributor guide.
4. Better diagnostics and a structured machine-readable output mode for CI.
5. Formatting/lint checks, integration tests, dependency review, and a small performance baseline.
6. Document current limitations, including provider capability differences and model-output nondeterminism.

Acceptance gate:
- A new user can parse, type-check, inspect, and run a documented example with mock configuration.
- A local compatible integration can be configured without modifying source.
- CI has reproducible, offline tests.

## Phase 8 — Optional future work (do not block MVP)

Only after phases 0–7 pass:
- Additional native adapters beyond the initial Claude, DeepSeek, and Kimi integration targets.
- Streaming output and streaming tool events if not already required by the MVP adapters.
- Persistent execution/checkpointing and resumable workflows.
- A workflow backend adapter such as LangGraph interoperability, Temporal, or another durable scheduler, if justified.
- Additional model classes such as embeddings/multimodal support.
- Sandboxed code execution service and stronger OS-level filesystem/process isolation.
- Alternative workspace backends such as Git worktrees, remote workspaces, object storage, or copy-on-write snapshots.
- A language formatter and language-server protocol support.
- Distributed workers, remote execution, and peer-to-peer coordination.

Treat these as independent extensions with their own ADRs, not implicit MVP requirements.

---

## Testing and engineering rules

- Run `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` where applicable, and `cargo test --workspace` at phase gates; document exceptions.
- Prefer unit tests for lexer/parser/type-checking/IR and integration tests for adapter/runtime boundaries.
- External API tests must be opt-in and never required for normal CI.
- Do not test real models when a mock gives deterministic coverage.
- Do not swallow provider errors or convert failures into apparently successful text.
- Use structured errors and diagnostics with useful source spans when available.
- Avoid panics on user-written Axiom source or provider responses.
- Keep async cancellation, timeout, retry, and cost semantics explicit.
- Do not log credentials, authorization headers, or secret values.
- Never infer permission to use a tool from an LLM response; check the runtime registry and policy first.
- Filesystem authorization must be enforced in Rust and tested independently of prompts. Do not treat glob strings or natural-language instructions as a sandbox.
- Never expose the host filesystem by default. Resolve paths relative to an explicitly configured root and check canonical containment, including symlink behavior.
- Keep concurrent tasks' writes isolated; integrate validated patch artifacts through a controlled merge/apply gate.
- Do not build custom cryptography or roll your own secret vault.
- Use exact, pinned or compatible dependency versions according to the repository's policy and check current stable docs before selecting them.
- Do not add elaborate abstractions without a concrete use in the MVP.
- Keep API boundaries small and testable; explain deviations from this prompt in ADRs.

## Definition of Done for the MVP

The MVP is done only when all are true:

1. The `.ax` language parses and type-checks.
2. It compiles to an inspectable IR.
3. Model profiles and integration connections are separate from agent definitions.
4. An agent can use a project default or an agent/instance override.
5. The model integration layer supports a generic OpenAI-compatible adapter, the initial Claude/Anthropic integration, DeepSeek, Kimi, and a mock adapter through the same runtime contract.
6. Typed tools and output validation are implemented.
7. A rooted filesystem abstraction, scoped filesystem tools, isolated task workspaces, artifact/evidence storage, and patch validation are implemented and tested.
8. A mixed deterministic/agentic workflow runs with state transitions and verification gates.
9. Parallel agents work in isolated workspaces; shared-project integration requires verification and conflict checks.
10. The same agent definition can switch among the initial providers or a local compatible endpoint using profiles/configuration, not source rewrites.
11. Offline tests pass and examples are documented.
12. Secrets, filesystem access, and tool permissions are handled safely.
13. The docs explicitly label implemented, simulated, and future features.

## First response and working style expected from the implementation agent

Before editing code, inspect the repo and report a concise phased implementation plan with any repository-specific constraints. Then implement Phase 0 and Phase 1 only. Run the tests, summarize the exact files changed and test results, and stop for review before beginning Phase 2 unless the caller explicitly directs continuous execution through all phases.
