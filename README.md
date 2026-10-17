# Axiom

Axiom is a TypeScript-inspired domain-specific language (`.ax`) for defining AI
agents, goals, contracts, tools, policies, and hybrid (procedural + agentic)
workflows. This repository contains the language compiler and runtime, written
in Rust.

Axiom keeps five things deliberately separate, because collapsing them is the
root of most agent-framework lock-in:

1. **Agent definition** — role, typed input/output, tools, constraints, authority.
2. **Integration connection** — a configured endpoint that selects a protocol
   adapter (base URL, auth reference, timeouts, headers).
3. **Model profile** — a logical alias such as `reasoning` or `local_coding`,
   resolved to an integration + concrete model id.
4. **Agent instance** — an agent definition bound to a model profile plus
   optional overrides.
5. **Goal / contract / workflow** — desired outcome, bounded work with acceptance
   criteria, and the sequencing (deterministic steps, delegation, verification).

The point: an agent's role and source definition stay stable when the underlying
model changes. `Anthropic Messages` is an *adapter*; `claude_api` is an
*integration*; `claude_architecture` is a *model profile*.

## Status

| Phase | Scope | State |
| --- | --- | --- |
| 0 | Reconnaissance, architecture, ADRs, grammar draft, toolchain | done |
| 1 | Lexer, parser, AST, diagnostics, `axiom check`; `axiom.yaml` loading + `axiom config check` | done |
| 2 | Semantic analysis, symbols, types, contracts | not started |
| 3 | IR and explain/dry-run | not started |
| 4 | Provider integration layer and model profiles | not started |
| 5 | Agent runtime, typed tools | not started |
| 6 | Goals, contracts, hybrid workflow execution | not started |
| 7 | Example application, docs, hardening | not started |
| 8 | Optional future work | not started |

See [`docs/development.md`](docs/development.md) for build/test commands and
parity notes, [`docs/language-spec.md`](docs/language-spec.md) for the grammar,
and [`docs/configuration.md`](docs/configuration.md) for `axiom.yaml`. This
document labels what is **implemented**, **simulated**, and **future**; do not
read further than that.

## Layout

```
crates/
  axiom-syntax/   lexer, parser, AST, spans, diagnostics  (no dependencies)
  axiom-config/   axiom.yaml loading, expansion, validation
  axiom-cli/      the `axiom` binary (check, config check, ...)
examples/         .ax programs and axiom.yaml
docs/             language spec, configuration, architecture, providers, security, ADRs
```

## Example

```ts
integration local_llm {
  adapter: openai_compatible
  base_url: env("AXIOM_LOCAL_BASE_URL", "http://localhost:11434/v1")
  api_key: secret("AXIOM_LOCAL_API_KEY", optional: true)
}

model coding {
  integration: "local_llm"
  model: env("AXIOM_LOCAL_MODEL", "qwen3-coder:30b")
  capabilities: [chat, structured_output]
}

agent Architect implements Agent<FeatureSpec, ExecutionPlan> {
  model: "coding"
  purpose: "Decompose requirements and design interfaces"
  tools: [repository.read, documentation.search]

  permissions {
    read: ["src/**", "docs/**"]
    write: []
  }

  constraints {
    must: ["Define explicit interfaces"]
    must_not: ["Implement application code directly"]
  }
}

let localArchitect = Architect.with { model: "coding" }
```

## Building

The project builds with the GNU Rust toolchain on Windows (see
`docs/development.md` for the exact environment, since this machine has no MSVC
linker and a restrictive Application Control policy).

```sh
cargo check --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p axiom-cli -- check examples/hello_agent.ax
cargo run -p axiom-cli -- config check examples/axiom.yaml
```

## License

Dual-licensed under MIT or Apache-2.0 (see `Cargo.toml`).
