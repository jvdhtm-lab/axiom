# Configuration (`axiom.yaml`)

> **Status: Phase 5 (partial).** The `axiom-config` crate now implements the
> `agents.md` §4 schema: `version`, `language`, `sources`, `integrations`,
> `models`, `runtime`, `api_policies`, `compilation`, `browser`, `frontend`,
> `filesystem`, `commands`. Unknown keys are errors. Model alias resolution and
> the offline mock provider live in `axiom-providers`; the builtin/command tool
> registry lives in `axiom-tools`. Networked adapters and tool execution are
> later phases. `docs/implementation-ledger.md` records the exact status.

Axiom separates **source** from **configuration**:

| File | Responsibilities |
| --- | --- |
| `*.ax` | The language: modules, types, enums, functions, agents, workspaces, `api`/`async`/`effects`. Portable and environment-independent. |
| `axiom.yaml` | Deployment/environment configuration: integration connections, model aliases, runtime budgets, API policies, compilation targets, browser/tooling quotas, filesystem limits, allowlisted commands. |

Neither file may contain the other's concerns. In particular, **do not put
credentials or environment-specific endpoints into `.ax` source**. The `.ax`
language (Axiom 1.0) does not embed integration or model declarations.

## Command

```console
$ axiom config check axiom.yaml
ok: axiom.yaml (4 integration(s), 4 model(s))
checked 1 file: 0 error(s), 1 warning(s)
```

Exit codes: `0` success (warnings allowed), `1` diagnostics, `2` usage error.

## Schema

The top level is a mapping with the keys `version`, `language`, `sources`,
`integrations`, `models`, `runtime`, `api_policies`, `compilation`, `browser`,
`frontend`, `filesystem`, and `commands`. Unknown keys are errors, never silently
ignored. A model's connection is named `integration`; there is no undocumented
aliasing.

### `version` and `language`

`version` must be `1` when present (defaults to `1`); any other value is
rejected. `language` must be `"1.0"` when present. `sources` is a sequence of
module globs (e.g. `src/**/*.ax`).

### `integrations`

A mapping of integration id → connection. An integration is a *connection*: it
names the protocol **adapter** and the endpoint. It is not an adapter, and not a
model alias.

| Key | Type | Notes |
| --- | --- | --- |
| `adapter` | string (required) | e.g. `anthropic_messages`, `openai_compatible` |
| `dialect` | string (optional) | adapter-specific flavor |
| `base_url` | string (required) | endpoint URL |
| `api_key` | string (optional) | usually a `${...}` reference to a secret |
| `api_key_optional` | bool (default `false`) | allow a missing/empty key (e.g. local endpoints) |
| `timeout_seconds` | integer (optional) | |
| `headers` | mapping string → string (optional) | values support `${...}` |
| `allowed_data_classes` | sequence (optional) | `public`, `internal`, `pii`; `secret` is refused |

### `models`

A mapping of model alias → profile. An alias is a *logical name* bound to an
integration and a concrete model id; it makes no claim about model intelligence.
Switching providers or models is a configuration change: source never names a
vendor.

| Key | Type | Notes |
| --- | --- | --- |
| `integration` | string (required) | must reference a declared integration |
| `model` | string (required) | a concrete, user-supplied model id |
| `capabilities` | sequence (optional) | `chat`, `structured_output`, `tool_calling`, `streaming`, `vision`, `embeddings`, `reasoning` |

Declared capabilities are expectations, not proof; unsupported operations must
fail explicitly.

### `runtime`

Environment-independent defaults and hard per-run budgets.

| Key | Type | Notes |
| --- | --- | --- |
| `default_model` | string (optional) | must reference a declared model alias |
| `max_parallel_agents` | integer ≥ 1 (optional) | |
| `max_retries` | integer (optional) | |
| `request_timeout_seconds` | integer (optional) | |
| `max_model_calls` | integer (optional) | hard ceiling per run |
| `max_wall_clock_seconds` | integer (optional) | hard ceiling per run |
| `max_agent_attempts` | integer ≥ 1 (optional) | |
| `max_run_cost` | number or `null` (optional) | `null` means "no cost ceiling" |

### `api_policies`

A mapping of API name → policy. The server enforces these on every call; they
are not an authorization grant from the client.

| Key | Type | Notes |
| --- | --- | --- |
| `version` | string (optional) | policy version |
| `authentication` | string (optional) | e.g. `required` |
| `requires` | sequence of string (optional) | capability identifiers checked against the compiled plan |
| `data_class` | string (optional) | `public`, `internal`, `pii` |
| `cache` | string (optional) | e.g. `none` |

### `compilation`

| Key | Type | Notes |
| --- | --- | --- |
| `targets` | sequence (optional) | subset of `server`, `wasm_browser` |
| `entrypoints` | mapping target → entry (optional) | see below |
| `wasm` | mapping (optional) | see below |

`entrypoints.<target>.module` is required. `server` accepts only `module`;
`wasm_browser` accepts `module`, `init`, `render`, and `handle_event`.

`compilation.wasm` keys: `target`, `host_api_version` (must match the supported
ABI, currently `1`), `max_module_bytes`, `reproducible`.

### `browser`

Browser-host quotas and state/cache defaults: `isolation`, `max_memory_pages`,
`max_compute_ms_per_event`, `max_ui_nodes`, `max_event_bytes`,
`max_storage_bytes`, `max_snapshot_bytes`,
`preserve_previous_release_on_failure`, plus nested `state`
(`default_scope`, `default_persistence`) and `data_cache` (`enabled`,
`max_entries`, `max_bytes`, `default_ttl_seconds`, `serve_stale_on_error`).

### `frontend`

`registry` (`content_addressed`, `retain_accepted_versions`,
`require_pinned_references`) and `build_cache` (`enabled`, `max_bytes`) govern
frontend artifact storage.

### `filesystem`

| Key | Type | Notes |
| --- | --- | --- |
| `project_root` | string (optional) | root the runtime is confined to |
| `default_agent_isolation` | string (optional) | e.g. `per_task`, `shared_read_only` |
| `max_read_bytes` | integer (optional) | per-read limit |
| `max_write_bytes` | integer (optional) | per-write limit |
| `max_file_bytes` | integer (optional) | per-file limit |
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

### `databases`

A mapping of database id → configuration. SQLite databases open locally with
separate least-privilege read/write/schema connections; PostgreSQL entries
declare the release target only (Phase 7 ships no remote driver).

| Key | Type | Notes |
| --- | --- | --- |
| `engine` | string (required) | `sqlite` or `postgres` |
| `path` | string (optional) | SQLite file, safe relative path (sqlite only) |
| `url` | string (optional) | PostgreSQL URL (postgres only, redacted in output) |
| `data_class` | string (optional) | data classification (default `internal`) |
| `allowed_tables` | sequence of string (optional) | tables application SQL may reference |
| `max_rows` | integer (optional) | read row cap |
| `timeout_seconds` | integer (optional) | statement timeout |
| `migrations` | string (optional) | reviewed migration directory (safe relative path) |
| `destructive_migrations` | bool (optional) | whether destructive migrations are configured as allowed |
| `allow_private_network` | bool (optional) | permit private/link-local PostgreSQL hosts |
| `require_write_approval` | bool (optional) | require a write approval (default true) |

`axiom db check` validates each entry, probes SQLite role isolation, and checks
the PostgreSQL target policy; `axiom db migrations plan --database <name>`
prints pending migrations and destructive changes.

## Providers (`axiom-providers`)

`resolve_spec(config, alias)` maps a model alias to a vendor-neutral
`ProviderSpec` (`adapter`, `model`, `base_url`, `has_secret`, `timeout_seconds`)
that **never** contains secret material, so it is safe in traces.
`ProviderRegistry` builds a `ModelProvider` from a registered adapter factory;
Phase 5 ships `MockFactory` (offline, deterministic) for `anthropic_messages` and
`openai_compatible`. Networked adapters arrive behind the same trait later.

## Tools (`axiom-tools`)

`ToolRegistry::with_builtins()` registers `fs.read`, `fs.search`, `fs.write`,
`compiler.check`, and `command.run`, each mapped to the effect an agent must
declare (`fs.read`, `fs.write`, `command.run`). `from_config` additionally
registers each `commands.<name>` profile as the tool `command.<name>`. Unknown
tools and unknown command profiles are refused. Executing these tools in a
sandbox is Phase 6.

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
`<redacted>`. Resolved `ProviderSpec`s record only whether a key was present.
Secrets are never written into diagnostics, IR dumps, traces, or error messages.
No credentials are needed to parse either file type.

## Diagnostics

- YAML **syntax** errors carry a precise line/column from the parser.
- Value-level checks (unknown key, missing field, unknown capability, unknown
  target, unsupported host ABI, unresolved reference, missing environment
  variable) carry an **approximate** span: YAML values have no spans, so Axiom
  points at the first occurrence of the offending key
  (`integrations.claude` → the `claude:` line), falling back to the start of the
  file. Precise value spans are a possible later improvement.

## Non-goals (Phase 5)

No integration calls, no capability negotiation, no vendor-specific option
validation, no model discovery, no tool execution. Vendor-namespaced options, the
adapter registry, and enforcement of the `filesystem`/`browser`/`commands` limits
are later-phase concerns.
