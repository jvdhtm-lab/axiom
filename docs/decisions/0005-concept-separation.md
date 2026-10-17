# ADR-0005: Five concepts are never collapsed

**Status:** accepted (Phase 0)

## Context

Most agent frameworks overload a single `model`/`provider` field with role,
identity, endpoint, credentials, and vendor. This makes agents fragile and
non-portable.

## Decision

Keep five entities separate throughout the language, AST, and (planned) IR:

1. **Agent definition** — role, typed I/O, tools, constraints, authority.
2. **Integration connection** — configured endpoint + adapter.
3. **Model profile** — logical alias → integration + model id.
4. **Agent instance** — definition bound to a profile + overrides.
5. **Goal / contract / workflow** — outcome, invariants, sequencing.

The AST models these as distinct declarations (`Agent`, `Integration`, `Model`,
instance `let`, `Goal`/`Contract`/`Workflow`). A `model` node in Axiom is a
*profile*, not a capabilities claim or a provider type. Names such as
`reasoning`, `fast`, `coding` are user-defined profiles.

## Consequences

An agent's identity and logic are stable when its model changes. This separation
is a compiler invariant from Phase 2 (a `model:` on an agent must resolve to a
declared profile, not an integration or a raw vendor string).
