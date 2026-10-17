# ADR-0010: Re-baseline to Axiom 1.0

- Status: accepted
- Date: 2026-10-18

## Context

The Phase 0/1 increment (commit `8279979`) shipped a working but deliberately
small front end and configuration loader built against a first-draft syntax and
a `providers`-based `axiom.yaml` schema. The master prompt (`agents.md`) was then
rewritten as **Axiom 1.0**: a production-oriented specification with a
TypeScript-styled surface syntax, a new configuration schema, and 13 phases.
Continuing to grow the pre-1.0 shapes would mean building on a grammar and schema
that the spec no longer describes.

Two questions had to be answered: how to reconcile the existing code with the new
spec, and how to name the crates relative to the spec's suggested names.

## Decision

1. **Re-baseline, don't rewrite.** The existing crate structure and the passing
   Phase 0/1 work are retained. The target syntax and configuration schema are
   recorded normatively in `docs/grammar.ebnf` and `docs/configuration.md`, and
   the code is migrated toward them incrementally. A truthful
   `docs/implementation-ledger.md` records what is actually done per phase.

2. **Crate layout.** Keep the current names rather than adopting the spec's
   suggested ones:

   | Crate | Role | Phase |
   | --- | --- | --- |
   | `axiom-syntax` (existing) | lexer, parser, AST, spans, diagnostics | 1 |
   | `axiom-config` (existing) | `axiom.yaml` loading/validation | 1 |
   | `axiom-cli` (existing) | `axiom` binary | 1+ |
   | `axiom-types` (planned) | type system / semantic analysis | 2 |
   | `axiom-ir` (planned) | typed intermediate representation | 3 |
   | `axiom-providers` (planned) | adapters and integration clients | 4 |
   | `axiom-runtime` (planned) | execution, filesystem, commands | 5–6 |

   Rationale: the names describe responsibility clearly, the split keeps
   `axiom-syntax` zero-dependency, and the boundaries are stable regardless of
   what the spec calls them. Renaming later is a mechanical change tracked in the
   ledger.

3. **Syntax changes (1.0).** `module`/`import`/`export` headers; lower-case
   primitives (`string`, `bool`, `int`, `float`, `duration`); `T[]` array types;
   `A | B` unions; `{ ... }` record types; `Agent<I, O>` generic references;
   `;`-or-newline terminators. The 1.0 grammar is the target; the current parser
   still implements a pre-1.0 subset (see the ledger and the implementation-status
   note in `docs/language-spec.md`).

4. **Configuration-key changes (1.0).** `providers` → `integrations`;
   `ModelConfig.provider` → `integration`; an integration's
   `request_timeout_seconds` → `timeout_seconds` (the `runtime` block keeps
   `request_timeout_seconds`). New top-level `filesystem` and `commands`
   sections. `ProviderConfig` is renamed `IntegrationConfig`. `integration` is
   the one reference key; there is no undocumented aliasing.

## Consequences

- `axiom-config` gains `IntegrationConfig`, `FilesystemConfig`, and
  `CommandProfile`; the sample config, fixtures, tests, and CLI summary text are
  updated to match.
- The `.ax` lexer/parser/AST rework to the full 1.0 grammar remains outstanding;
  until it lands, accelerator examples and fixtures stay in the pre-1.0 form.
- `docs/implementation-ledger.md` is the single source of truth for what is
  implemented. Later ADRs supersede this one rather than editing it.
