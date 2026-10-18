# Configuration (`axiom.yaml`)

> **Status: re-baselining (Phase 0 → Phase 5).** The normative target schema is
> `agents.md` §4 (`version`, `language`, `sources`, `integrations`, `models`,
> `runtime`, `api_policies`, `compilation`, `browser`, `frontend`, `filesystem`,
> `commands`). The `axiom-config` crate currently implements the previous schema;
> it is rewritten in Phase 5. Until then, treat the sections below as legacy and
> see `agents.md` §4 for the target. `docs/implementation-ledger.md` records the
> exact status.

Axiom separates **source** from **configuration**:

| File | Responsibilities |
| --- | --- |
| `*.ax` | The language: modules, types, agents, goals, contracts, tools, policies, workflows, delegation, parallelism, branching, retries, verification gates. Portable and environment-independent. |
| `axiom.yaml` | Deployment/environment configuration: integration connections (adapters, endpoints, secret references, timeouts, headers), model profiles, runtime defaults and limits, filesystem limits, and allowlisted command profiles. |

Neither file may contain the other's concerns. In particular, **do not put
credentials or environment-specific endpoints into `.ax` source**. `axiom.yaml`
is the canonical deployment configuration; the `.ax` language (Axiom 1.0) does
not embed integration or model declarations.

Status: the `axiom-config` crate implements loading, `${...}` expansion, strict
validation, and reference checks for `axiom.yaml` (Phase 1). No integration is
contacted (Phase 4), and filesystem/command limits are parsed and validated but
not yet enforced (Phase 5).

## Command

```console
$ axiom config check axiom.yaml
ok: axiom.yaml (4 integration(s), 4 model(s))
checked 1 file: 0 error(s), 1 warning(s)
```

Exit codes: `0` success (warnings allowed), `1` diagnostics, `2` usage error.

## Schema

The top level is a mapping with the keys `version`, `integrations`, `models`,
`runtime`, `filesystem`, and `commands`. Unknown keys are errors, never silently
ignored. The reference key for a model's connection is named `integration`; there
is no undocumented aliasing.

### `version`

Required to be `1` when present (defaults to `1`). Any other value is rejected.

### `integrations`

A mapping of integration id → connection. An integration is a *connection*: it
names the protocol **adapter** and the endpoint. It is not an adapter, and not a
model profile.

| Key | Type | Notes |
| --- | --- | --- |
| `adapter` | string (required) | e.g. `anthropic_messages`, `openai_compatible` |
| `base_url` | string (required) | endpoint URL |
| `api_key` | string (optional) | usually a `${...}` reference to a secret |
| `api_key_optional` | bool (default `false`) | allow a missing/empty key (e.g. local endpoints) |
| `timeout_seconds` | integer (optional) | |
| `headers` | mapping string → string (optional) | values support `${...}` |

### `models`

A mapping of model profile id → profile. A profile is a *logical alias* bound to
an integration and a concrete model id; it makes no claim about model
intelligence.

| Key | Type | Notes |
| --- | --- | --- |
| `integration` | string (required) | must reference a declared integration |
| `model` | string (required) | a concrete, user-supplied model id |
| `capabilities` | sequence (optional) | `chat`, `structured_output`, `tool_calling`, `streaming`, `vision`, `embeddings`, `reasoning` |

Declared capabilities are expectations, not proof. Adapters negotiate real
support in Phase 4; unsupported operations must fail explicitly.

### `runtime`

| Key | Type | Notes |
| --- | --- | --- |
| `default_model` | string (optional) | must reference a declared model profile |
| `max_parallel_agents` | integer ≥ 1 (optional) | |
| `max_retries` | integer (optional) | |
| `request_timeout_seconds` | integer (optional) | |

### `filesystem`

Environment-independent operational limits for the filesystem/workspace
subsystem. Parsed in Phase 1; enforced in Phase 5.

| Key | Type | Notes |
| --- | --- | --- |
| `project_root` | string (optional) | root the runtime is confined to |
| `default_agent_isolation` | string (optional) | e.g. `per_task`, `shared_read_only` |
| `max_read_bytes` | integer (optional) | per-read limit |
| `max_write_bytes` | integer (optional) | per-write limit |
| `max_files_per_task` | integer (optional) | |
| `protect_vcs_metadata` | bool (optional) | keep VCS metadata out of agent reach |

### `commands`

A mapping of command profile id → profile. The runtime may run only these named
profiles; command lines are never assembled from model output.

| Key | Type | Notes |
| --- | --- | --- |
| `executable` | string (required) | |
| `args` | sequence of string (optional) | fixed arguments |
| `working_directory` | string (optional) | |
| `timeout_seconds` | integer (optional) | |
| `network` | bool (optional) | whether the command may use the network |

## Placeholder expansion

Expansion is implemented explicitly by Axiom; it is **not** delegated to the YAML
parser. Only string scalars are expanded:

- `${NAME}` → the value of `NAME`; **error** if `NAME` is unset or empty.
- `${NAME:-default}` → `NAME`, or `default` when `NAME` is unset or empty.
- `$$` → a literal `$`.
- Any other `$` is a malformed-placeholder error.

There is no recursion and no expression evaluation. An integration whose
`api_key` cannot be resolved is an error unless `api_key_optional: true`, in
which case it is a warning and the integration loads without a key.

## Secrets

`api_key` values are stored in a `Secret` type whose `Debug`/`Display` render
`<redacted>`. Secrets are never written into diagnostics, IR dumps, traces, or
error messages. No credentials are needed to parse either file type.

## Diagnostics

- YAML **syntax** errors carry a precise line/column from the parser.
- Value-level checks (unknown key, missing field, unknown capability, unresolved
  reference, missing environment variable) carry an **approximate** span: YAML
  values have no spans, so Axiom points at the first occurrence of the offending
  key (`integrations.claude` → the `claude:` line), falling back to the start of
  the file. Precise value spans are a possible later improvement.

## Non-goals (Phase 1)

No integration calls, no capability negotiation, no vendor-specific option
validation, no model discovery. Vendor-namespaced options, the adapter registry,
and enforcement of the `filesystem`/`commands` limits are later-phase concerns.
