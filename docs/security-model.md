# Security model

> **Status: design (Phase 0).** Phase 1 parses source only; nothing here is
> executable yet. This document states the invariants the implementation must
> uphold from Phase 4 onward.

## Invariants

1. **No arbitrary execution.** Axiom does not run arbitrary shell commands or
   arbitrary Rust code because an agent asks. Tools are registered, typed,
   permission-checked, and dispatched by the runtime. A tool's *declaration* is
   separate from its *implementation*.
2. **Permission is never inferred from model output.** An LLM response cannot
   grant tool access. The runtime checks the registry and policy **first**; a
   permission-denied action is rejected.
3. **Secrets never leak.** Secrets are referenced by name. They must not appear
   in debug output, errors, trace events, IR dumps, compiled plans, or logs —
   including authorization headers.
4. **Bounded execution.** Request and execution timeouts, cancellation
   propagation, retry ceilings, depth limits, parallel-worker bounds, and
   (where usage/pricing is available) token/cost limits are explicit.
5. **Validated output.** Model-produced data is never trusted merely because it
   parses. It is decoded and validated against the requested schema; invalid
   output yields a structured error and, if configured, bounded repair.
6. **Validated plans.** Agent-proposed subgoals/plans are checked against
   contracts, depth limits, dependency rules, allowed tools, and budgets before
   scheduling. Model text never bypasses the compiled plan or permissions.
7. **No custom crypto, no custom vault.** Use established libraries; do not roll
   your own.

## Trust boundaries

| Boundary | Trusted? | Rule |
| --- | --- | --- |
| `.ax` source | user-authored | validate, never panic |
| Model output | **untrusted** | schema-validate; no control-flow authority |
| Tool request from model | **untrusted** | must pass registry + policy |
| Integration endpoint | configured | TLS, timeouts, size limits |
| Secrets | **never serialized** | redacted everywhere |

## Filesystem / terminal tools

If and when a filesystem or terminal tool is added, it must have explicit
allowlists (globs/paths) and a restricted execution environment. The MVP ships
only safe tools (deterministic calculator, fixture-backed repository reader),
not unrestricted shell access.

## Error reporting

Errors are structured and carry source spans where available. Provider errors are
never swallowed or converted into apparently-successful text.

## Out of scope (MVP)

A distributed sandbox, remote execution, and peer-to-peer coordination are
explicitly out of scope; they are independent extensions with their own ADRs.
