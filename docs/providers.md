# Providers and the integration layer

> **Status: planned (Phase 4).** Phase 1 only *parses* `integration` and `model`
> declarations. No adapter exists yet, and the CLI never contacts a provider.

## The three-level model

Do not equate a vendor with a single API protocol. A vendor may expose several
protocols, and one protocol may be spoken by many vendors.

| Level | Meaning | Examples |
| --- | --- | --- |
| **Adapter** | code that speaks one API protocol | `openai_compatible`, `anthropic_messages` |
| **Integration** | a configured connection using an adapter | `claude_api`, `kimi_api`, `local_llm` |
| **Model profile** | a logical alias → integration + model id | `reasoning`, `local_coding` |

An `integration` declaration selects an adapter and supplies endpoint,
authentication reference, timeouts, headers, and transport options. A `model`
declaration names an integration and a concrete model id.

## Integration strategies

1. **Protocol adapters** — common API shapes (OpenAI Chat Completions-compatible,
   OpenAI Responses-compatible where implemented).
2. **Native vendor adapters** — where semantics differ materially (request shape,
   tool-call representation, streaming events, reasoning fields). Initial native
   target: **Anthropic Messages (Claude)**.
3. **Gateway adapters** — one configured connection fronting many upstream
   models (OpenRouter, self-hosted LiteLLM). A convenience, never a replacement
   for direct adapters.

A vendor that merely exposes an OpenAI-compatible endpoint (initial DeepSeek,
Kimi/Moonshot, local runtimes) is reachable via the generic adapter **when
configured for the exact endpoint**. Axiom must not assume every endpoint with a
compatible-looking URL behaves identically; the exact endpoint/protocol choice is
recorded here per provider with a retrieval date before an adapter is
implemented.

## Initial targets

- Anthropic / Claude — native `anthropic_messages` adapter.
- DeepSeek — direct/compatible configuration, preserving provider-specific options.
- Moonshot / Kimi — direct or documented compatible-protocol mode per endpoint.
- OpenAI-compatible endpoints — configurable base URL, model id, optional key,
  custom headers, timeout, request options (Ollama, vLLM, LM Studio, …).
- OpenRouter / LiteLLM — gateway connections.
- Mock adapter — deterministic, offline tests.

## Capabilities

Capabilities are declared expectations, not proof. Each adapter/model
configuration reports which of these it supports: `chat`, `structured_output`,
`tool_calling`, `streaming`, `vision` (multimodal input), `embeddings`,
`reasoning`. Support may depend on the exact model, endpoint, deployment, and
adapter version. **Unsupported capabilities must fail explicitly** before
execution where detectable — never silently degrade.

## Extension boundary

A new vendor follows one of:

1. A **configuration-only** custom OpenAI-compatible endpoint (no code).
2. A **new adapter** implementing the documented Rust trait, registered with the
   integration registry — without changing the DSL grammar, agent definitions,
   workflow IR, or core runtime.

For the MVP, custom adapters are registered **in-process** via Rust traits and
feature-gated crates. Axiom does **not** load arbitrary native dynamic libraries
or execute downloaded plugins. A model on an unsupported protocol needs a new
adapter or a gateway that translates it into a supported protocol. That is the
honest boundary of "any model".

## Normalization and losses

Provider-neutral request/response types are the common denominator. Raw provider
metadata is preserved where useful and safe. Normalization **losses and feature
differences are documented** here as adapters land, per provider, with links and
a retrieval date.
