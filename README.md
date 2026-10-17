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

Axiom is re-baselined to the **Axiom 1.0** specification, but 1.0 is **not**
implemented yet. The repository today is a working *pre-1.0* `.ax` front end plus
a validated `axiom.yaml` (1.0 schema) loader. See
[`docs/implementation-ledger.md`](docs/implementation-ledger.md) for the truthful
phase-by-phase status; that file is the source of truth.

| Phase | Scope | State |
| --- | --- | --- |
| 0 | Reconnaissance, spec, grammar/EBNF, architecture, ADRs | partial |
| 1 | Lexer, parser, AST, diagnostics (all 1.0 syntax) | partial — pre-1.0 grammar |
| 2 | Modules, types, symbols, effects | not started |
| 3 | Deterministic evaluator, typed IR, explain/plan | not started |
| 4 | Configuration + model integration layer/adapter registry | partial — config only |
| 5 | Tools, secure filesystem, workspaces, artifacts, commands | not started |
| 6 | Agent runtime, teams, goals, contracts, invocation | not started |
| 7 | Hybrid workflow engine | not started |
| 8 | Durable state, recovery, observability | not started |
| 9 | Standard library | not started |
| 10 | CLI, formatter, linter, LSP | partial — `check`/`config check` |
| 11 | Conformance, security, fuzzing, performance | not started |
| 12 | Documentation, examples, packaging, compatibility | partial |
| 13 | Release candidate / 1.0 sign-off | not started |

See [`docs/development.md`](docs/development.md) for build/test commands and
parity notes, [`docs/language-spec.md`](docs/language-spec.md) for the language,
[`docs/grammar.ebnf`](docs/grammar.ebnf) for the normative 1.0 grammar, and
[`docs/configuration.md`](docs/configuration.md) for `axiom.yaml`.

## Layout

```
crates/
  axiom-syntax/   lexer, parser, AST, spans, diagnostics  (no dependencies)
  axiom-config/   axiom.yaml loading, expansion, validation
  axiom-cli/      the `axiom` binary (check, config check, ...)
examples/         .ax programs and axiom.yaml
docs/             language spec, grammar, configuration, architecture, providers,
                  security, implementation ledger, ADRs
```

## Example (Axiom 1.0 target syntax)

```ts
module architect;

import { FeatureSpec, ExecutionPlan } from "spec/types";

export agent Architect implements Agent<FeatureSpec, ExecutionPlan> {
  purpose: "Decompose requirements and design interfaces.";
  model: "reasoning";
  tools: [repository.read, documentation.search];
  workspace: ProjectReadOnly;
  constraints {
    must: ["define explicit interfaces"];
    must_not: ["implement application code directly"];
  }
}
```

Endpoints, credentials, and concrete model ids live in `axiom.yaml`; `model:
"reasoning"` names a model profile. The current parser implements only a
pre-1.0 subset of this syntax (see the ledger).

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
