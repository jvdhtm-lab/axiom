# Configuration (`axiom.yaml`)

Axiom separates **source** from **configuration**:

| File | Responsibilities |
| --- | --- |
| `*.ax` | The language: types, agents, goals, contracts, tools, policies, workflows, delegation, parallelism, branching, retries, verification gates. Portable and environment-independent. |
| `axiom.yaml` | Deployment/environment configuration: provider connections (adapters, endpoints, secret references, timeouts, headers), model profiles, runtime defaults and limits. |

Neither file may contain the other's concerns. In particular, **do not put
credentials or environment-specific endpoints into `.ax` source**. The `.ax`
grammar also accepts `integration` and `model` declarations for portability and
small examples, but `axiom.yaml` is the canonical deployment configuration;
resolving how embedded declarations interact with configuration is a Phase 2+
concern (see ADR-0008).

Status: the `axiom-config` crate implements loading, `${...}` expansion, strict
validation, and reference checks for `axiom.yaml` (Phase 1). No provider is
contacted (Phase 4).

## Command

```console
$ axiom config check axiom.yaml
ok: axiom.yaml (4 provider(s), 4 model(s))
checked 1 file: 0 error(s), 1 warning(s)
```

Exit codes: `0` success (warnings allowed), `1` diagnostics, `2` usage error.

## Schema

The top level is a mapping with the keys `version`, `providers`, `models`, and
`runtime`. Unknown keys are errors, never silently ignored.

### `version`

Required to be `1` when present (defaults to `1`). Any other value is rejected.

### `providers`

A mapping of provider id → connection. A provider is an *integration
connection*: it names the protocol **adapter** and the endpoint.

| Key | Type | Notes |
| --- | --- | --- |
| `adapter` | string (required) | e.g. `anthropic_messages`, `openai_compatible` |
| `base_url` | string (required) | endpoint URL |
| `api_key` | string (optional) | usually a `${...}` reference to a secret |
| `api_key_optional` | bool (default `false`) | allow a missing/empty key (e.g. local endpoints) |
| `request_timeout_seconds` | integer (optional) | |
| `headers` | mapping string → string (optional) | values support `${...}` |

### `models`

A mapping of model profile id → profile. A profile is a *logical alias* bound to
a provider and a concrete model id; it makes no claim about model intelligence.

| Key | Type | Notes |
| --- | --- | --- |
| `provider` | string (required) | must reference a declared provider |
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

## Placeholder expansion

Expansion is implemented explicitly by Axiom; it is **not** delegated to the YAML
parser. Only string scalars are expanded:

- `${NAME}` → the value of `NAME`; **error** if `NAME` is unset or empty.
- `${NAME:-default}` → `NAME`, or `default` when `NAME` is unset or empty.
- `$$` → a literal `$`.
- Any other `$` is a malformed-placeholder error.

There is no recursion and no expression evaluation. A provider whose
`api_key` cannot be resolved is an error unless `api_key_optional: true`, in
which case it is a warning and the provider loads without a key.

## Secrets

`api_key` values are stored in a `Secret` type whose `Debug`/`Display` render
`<redacted>`. Secrets are never written into diagnostics, IR dumps, traces, or
error messages. No credentials are needed to parse either file type.

## Diagnostics

- YAML **syntax** errors carry a precise line/column from the parser.
- Value-level checks (unknown key, missing field, unknown capability, unresolved
  reference, missing environment variable) carry an **approximate** span: YAML
  values have no spans, so Axiom points at the first occurrence of the offending
  key (`providers.claude` → the `claude:` line), falling back to the start of the
  file. Precise value spans are a possible later improvement.

## Non-goals (Phase 1)

No provider calls, no capability negotiation, no vendor-specific option
validation, no model discovery. Vendor-namespaced options and the adapter
registry are Phase 4 concerns.
