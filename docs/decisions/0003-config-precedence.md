# ADR-0003: Configuration precedence

**Status:** accepted (Phase 0)

## Context

A model can be chosen at several levels: a runtime default, a project default, an
agent's own declaration, an instance override, and a single-call override.
Without one deterministic rule, "which model actually runs?" becomes guesswork.

## Decision

Resolve configuration deterministically, highest wins:

```
per-invocation override > agent-instance override > agent declaration
  > project default > runtime default
```

Integration settings and model settings are **distinct**. A model profile
references an integration **by id** plus a concrete model id. Profile aliases are
resolved **at runtime**, so changing an integration endpoint or a concrete model
id does not require recompiling agent definitions. Concrete model ids are opaque
user-supplied strings; the compiler does not hardcode an allowlist.

## Consequences

- The same `Architect` definition runs against different providers via profile
  changes, not source rewrites.
- Precedence is a pure function and is unit-tested per level
  (`axiom-syntax` Phase 1 has no config resolution yet; this is implemented and
  tested in Phase 4).
- Defaults require no credentials in source files.
