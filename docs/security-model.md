# Axiom 1.0 security model

> **Status: design + partial.** The language front end, type/effect checker, and
> deterministic server evaluator enforce the *static* rules described here.
> Runtime, filesystem, database, artifacts, and browser-host enforcement are
> implemented in later phases; where a control is not yet implemented the ledger
> records it as `not started`. This document states the invariants the whole
> system must uphold, and honestly records residual risk.

## Core principle

Authority is defined by **boundaries**, not by what a model or artifact claims.
The runtime owns permissions, validation evidence, state, and acceptance. No
source construct, model response, UI event, cache hit, or artifact reference can
widen authority.

## Threat model

We threat-model at least:

- malicious or prompt-injected task text and model output;
- malicious source files and configuration;
- an agent attempting to widen its own permissions;
- path traversal and symlink/TOCTOU escapes;
- command injection;
- secret exfiltration;
- corrupted or malicious provider responses;
- forged validator results or validation evidence;
- cache poisoning;
- stale or revoked artifacts;
- browser ABI/import abuse;
- oversized UI or state payloads;
- runaway loops or task spawning;
- cross-tenant state or cache leakage;
- concurrent activation races;
- unsafe migrations;
- database injection or over-privilege;
- resume/cancellation bugs.

## Trust boundaries

| Input | Trusted? | Rule |
| --- | --- | --- |
| `.ax` source | user-authored | validate; never panic; never grant authority by itself |
| `axiom.yaml` | operator-authored | strict schema; unknown keys fail; secrets by reference |
| model output / `context` | **untrusted** | schema-validate; treated as data, never instructions |
| provider responses | **untrusted** | validate structured output before use |
| tool/command request from model | **untrusted** | registry + policy + effect + workspace check first |
| patches / workspace overlays | **untrusted** | a proposal, not permission to modify the shared repo |
| artifact manifests | **untrusted** | verify hashes/versions/compatibility before activation |
| restored browser state / cache entries | **untrusted** | validate schema, size, enums, invariants, tenant partition |
| secrets | **never serialized** | name-only references; redacted everywhere |

## Static invariants (enforced by the checker today)

1. **Small language.** Only the declarations in `docs/grammar.ebnf` exist. Forbidden
   concepts (`query`, `operation`, `team`, `task`, `contract`, `policy`, `tool`,
   `workflow`, `state`, `store`, `transition`, `migration`) are rejected with a
   stable error code, never silently accepted.
2. **Explicit effects.** A call may only use effects the caller declares. A pure
   function performs no effects. Unknown effect names are rejected.
3. **Closed capabilities.** Browser-target code may not reference server-only
   effects (`db.*`, `fs.*`, `command.run`, `model`, `workspace.*`, `artifact.*`,
   `runtime.checkpoint`); an unsupported target feature fails at check time.
4. **Parameterized SQL only.** SQL text must be a compile-time string literal;
   runtime/model-produced SQL text is rejected, and `:name` placeholders must
   match declared parameters.
5. **Explicit signatures.** Exported functions have explicit parameter and return
   types; there is no implicit `any` and no implicit conversion from unvalidated
   data.

## Runtime invariants (later phases — see the ledger)

- Runtime-owned permissions and validation evidence cannot be forged by agents.
- Least privilege and deny-by-default for workspaces, tools, host imports,
  databases, and browser storage.
- Compiler checks and static SQL/path checks are defense in depth; OS/runtime/DB
  enforcement is the security boundary.
- A generated frontend cannot access arbitrary network, filesystem, DB, secrets,
  or ambient DOM APIs.
- No cache is an authority source; no typed contract is an authorization grant.
- Artifacts and approvals bind to content hashes and versions and are revalidated
  at activation time.
- `axiom` never runs a shell string assembled from model output.
- `git_worktree` isolation creates a private checkout on a validated branch;
  unsafe branch names and non-repository roots are refused.
- Integration is only possible from an `exclusive_integration` workspace while
  holding the exclusive lock, with a clean base revision and no conflicts.
  Protected files (`axiom.yaml`, `.git/**`, `.axiom/**`, `db/migrations/**`)
  are refused at integration.
- `ProcessCommandRunner` clears the ambient environment (safe allowlist only),
  passes literal argv (no shell), caps stdout/stderr, and kills timed-out
  children. Network-enabled profiles are refused because process isolation is
  not a network boundary.
- `fs.*`/`workspaces.*`/`artifacts.*` host calls go through the workspace policy;
  `workspaces.integrate` is not exposed to Axiom code.
- Tests include malicious inputs and race conditions, not only success cases.

## Residual risks / honest limitations

- Static checks run on source; they are not the runtime security boundary.
- Provider authentication, TLS verification, and DB policy depend on
  OS/runtime/DB configuration that is not yet implemented.
- Command execution has process isolation but no container/kernel network
  sandbox; network-enabled profiles are refused rather than isolated. A child
  may still open sockets if a profile is misconfigured to allow them.
- Symlink/TOCTOU behavior at the OS layer is untested because the workspace model
  is in-memory; disk persistence arrives with the reference app.
- Model judgments ("this passed") are never deterministic proof; only
  machine-executed runtime checks count as evidence.
- Prompt injection through tool results and `context` is mitigated by treating
  all such data as untrusted and confining agents to workspaces, but is not
  eliminated.

## Error reporting

Errors are structured, carry stable codes and source spans, and never convert a
failed provider/tool/check into apparent success. Provider and driver errors are
reported through typed `Result` values, not exceptions.
