# Architecture

## Goals

Axiom is a small but real language with a compiler and a runtime. It bridges two
kinds of control:

- **Procedural control** — deterministic steps, checks, branching, retries,
  security boundaries.
- **Agentic control** — planning, interpretation, bounded decisions,
  decomposition, delegation.

Collaboration is **contract-driven**: agents exchange typed inputs/outputs,
artifacts, and verification evidence. **Model portability** means an agent's role
and definition do not change when its model changes.

Non-goals (MVP): a generic chatbot, a prompt-template wrapper, a
framework-specific DSL, distributed execution, a self-hosted model server, a
custom sandbox, an IDE.

## Conceptual model

Five entities, kept distinct:

| Entity | What it is | Example |
| --- | --- | --- |
| Agent definition | role, typed I/O, tools, constraints, authority | `Architect` |
| Integration connection | a configured endpoint + protocol adapter | `local_llm` |
| Model profile | logical alias → integration + model id | `reasoning` |
| Agent instance | definition bound to a profile + overrides | `localArchitect` |
| Goal / contract / workflow | outcome, invariants, sequencing | `feature_workflow` |

A **protocol adapter** speaks an API protocol (`anthropic_messages`,
`openai_compatible`). An **integration** is a configured connection using an
adapter (`claude_api`). A **model profile** binds a named model id to that
integration (`claude_architecture`). These are never collapsed into one
`model` field.

## Pipeline

```
.ax source
  │  lexer            (axiom-syntax::lexer)
  ▼
tokens (+ byte spans)
  │  parser           (axiom-syntax::parser)
  ▼
AST                   (axiom-syntax::ast)
  │  semantic analysis          [Phase 2]
  ▼
typed AST + symbols
  │  lowering                   [Phase 3]
  ▼
IR / execution graph
  │  runtime + adapters         [Phase 4-6]
  ▼
results, artifacts, verification evidence

axiom.yaml
  │  load + ${...} expansion + schema validation   (axiom-config)
  ▼
ProjectConfig (integrations, model profiles, runtime/filesystem limits)
```

Phase 1 implements two slices: `.ax` source → tokens → AST, and `axiom.yaml` →
validated `ProjectConfig`, both with diagnostics and accurate spans (configuration
value diagnostics use an approximate span — see `docs/configuration.md`). Nothing
below the AST exists yet, and no integration is contacted.

## Source vs configuration

`.ax` is the portable language; `axiom.yaml` is deployment/environment
configuration (integrations, endpoints, secret references, model profiles,
runtime/filesystem limits, command profiles). They are validated independently
and never overlap: source refers to model profiles by alias only. See
`docs/configuration.md` and ADR-0008.

## Crates

```
crates/
  axiom-syntax/   lexer, parser, AST, spans, diagnostics   (no dependencies)
  axiom-config/   axiom.yaml loading, expansion, validation
  axiom-cli/      the `axiom` binary
```

Planned: `axiom-types` (Phase 2), `axiom-ir` (Phase 3), `axiom-providers`
(Phase 4), `axiom-runtime` (Phase 5-6). The workspace grows only when a crate
earns its keep; dependency direction stays acyclic and the syntax crate never
depends on the runtime. `axiom-config` depends on `axiom-syntax` for the shared
diagnostic type, and on `yaml_serde` (ADR-0009).

## Configuration precedence

Resolved deterministically, highest first:

```
per-invocation override > agent-instance override > agent declaration
  > project default > runtime default
```

Integration settings and model settings are distinct. A model profile references
an integration **by id** plus a concrete model id; profile aliases resolve at
runtime so changing an integration or model id does not require recompiling
agent definitions. See `docs/decisions/0003-config-precedence.md`.

## Error strategy

- Lexer/parser produce **diagnostics**, never panics, on malformed input.
- Diagnostics carry a byte `Span`; the source layer renders a line/column pointer.
- Parser uses error recovery (skip to the next declaration/entry) so one mistake
  yields several diagnostics instead of only the first.
- Compile-time errors, runtime-enforced policies, and model-evaluated criteria
  are documented separately; natural-language constraints are **not** claimed to
  be statically verified.

## Security boundaries (summary)

Secrets are referenced by name and never logged or written into compiled plans,
traces, or errors. Tools are declared, permission-checked, and dispatched by the
runtime; model output never grants permission. See
`docs/security-model.md`.
