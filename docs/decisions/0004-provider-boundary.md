# ADR-0004: Adapter / integration / model-profile separation

**Status:** accepted (Phase 0)

## Context

Vendors are frequently conflated with protocols. But one vendor can speak several
protocols, and one protocol is spoken by many vendors (OpenAI-compatible
endpoints, gateways). Treating "provider" as a single concept makes feature
support unknowable and lock-in unavoidable.

## Decision

Three levels, distinct in the language and the runtime:

- **Adapter** — code that speaks one API protocol (`openai_compatible`,
  `anthropic_messages`, gateway, mock).
- **Integration** — a configured connection using an adapter (endpoint, auth
  reference, timeout, headers, options).
- **Model profile** — a logical alias resolving to an integration + model id.

Capability negotiation (`chat`, `structured_output`, `tool_calling`, `streaming`,
`vision`, `embeddings`, `reasoning`) is per adapter/model configuration and may
depend on the exact model/endpoint. **Unsupported capabilities fail explicitly**
where detectable — never silently degrade.

Vendor-specific options live in a namespaced, adapter-validated extension field;
unknown options are not silently discarded. New vendors are added either by
configuration (OpenAI-compatible endpoint) or by a new in-process adapter behind
the documented trait — without touching the grammar, agent definitions, workflow
IR, or core runtime.

## Consequences

The compiler and IR stay provider-neutral. Provider capability differences and
normalization losses are documented in `docs/providers.md`. Arbitrary native
plugin loading is explicitly out of scope for the MVP.
