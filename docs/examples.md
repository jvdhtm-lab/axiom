---
title: Examples
description: Complete Axiom examples for pure programs, server APIs, databases, frontend state, agents, workspaces, and configuration.
---

# Examples

The repository ships a small reference project under `examples/`. This page
shows the important pieces and the commands that check them. Every `.ax` snippet
below comes from a file that the test suite parses and checks.

Start with the [quickstart](quickstart.md) if you have not installed the CLI.

## Run the checks for every example

From the repository root:

```sh
axiom check examples/src/*.ax --config examples/axiom.yaml
axiom run examples/src/deterministic.ax
axiom check examples/src/frontend.ax --config examples/axiom.yaml
axiom workspace check examples/src/agents.ax
axiom db check --config examples/axiom.yaml
```

`examples/axiom.yaml` references model API keys through environment variables.
Set `ANTHROPIC_API_KEY`, `DEEPSEEK_API_KEY`, and `KIMI_API_KEY` (or give defaults
in the config) for `config check`, `sandbox plan`, and commands that load
integrations. The generated project from `axiom createapp` has no integrations,
so it runs without environment variables.

| File | What it demonstrates | Status |
| --- | --- | --- |
| `examples/src/deterministic.ax` | Types, enums, functions, `match`, `for`, lists, records | parse, check, `run` |
| `examples/src/shared.ax` | Shared records, enums, `Option`/`Result` error types | parse, check |
| `examples/src/server.ax` | `api` function, `db.read`, parameterized SQL, `Result` | parse, check; runtime test with mock DB |
| `examples/src/frontend.ax` | `init`, `render`, `handle_event`, typed state, `api.call` | parse, check; browser host not implemented |
| `examples/src/agents.ax` | `workspace`, `agent`, `agents.iterate`, validators | parse, check; runtime library with mock provider |
| `examples/src/agent_designer.ax` | Self-contained workspace + agent + bounded orchestration | parse, check, `workspace check` |

## 1. A pure program

`examples/src/deterministic.ax` is the smallest complete module:

```axiom
module examples.deterministic;

export enum Sign { Negative; Zero; Positive }

export type Point = { x: int; y: int; };

export fn add(a: int, b: int) -> int {
  return a + b;
}

export fn classify(n: int) -> Sign {
  if n > 0 {
    return Sign.Positive;
  }
  if n < 0 {
    return Sign.Negative;
  }
  return Sign.Zero;
}

export fn describe(n: int) -> string {
  match classify(n) {
    Sign.Positive => {
      return "positive";
    }
    Sign.Negative => {
      return "negative";
    }
    Sign.Zero => {
      return "zero";
    }
  }
}

export fn main() -> string {
  let p = { x: add(2, 3), y: 0 };
  return describe(p.x) + ": " + p.x.to_string();
}
```

```sh
axiom run examples/src/deterministic.ax
```

```text
"positive: 5"
```

The full breakdown is in the [language tour](language-tour.md).

## 2. Shared types and a server API

Shared types live in `examples/src/shared.ax`:

```axiom
module examples.shared;

export type DashboardInput = { team_id: string; };

export type DashboardSummary = { team_id: string; completed: int; };

export enum AppError { NotFound; DatabaseUnavailable; ValidationFailed; Unauthorized; }
```

The server handler is an ordinary exported function with `api` and `async`
modifiers:

```axiom
module examples.server;

import { DashboardInput, DashboardSummary, AppError } from "./shared.ax";

export api async fn get_dashboard(
  input: DashboardInput
) -> Result<DashboardSummary, AppError>
  effects [db.read] {
  match await db.query_one<DashboardSummary>(
    database: "teamboard",
    sql: "SELECT team_id, completed FROM v_team_performance
          WHERE team_id = :team_id LIMIT 1",
    params: { team_id: input.team_id },
    timeout: 5s
  ) {
    Ok(summary) => return Ok(summary);
    Err(_) => return Err(AppError.DatabaseUnavailable);
  }
}
```

The API policy is configuration, not a client grant:

```yaml
api_policies:
  get_dashboard:
    version: "1.0.0"
    authentication: required
    requires: ["dashboard:read"]
    data_class: internal
    cache: none
```

Checking the two modules:

```sh
axiom check examples/src/shared.ax examples/src/server.ax --config examples/axiom.yaml
```

The runtime test runs `get_dashboard` against a mock database only after
authentication, capability, and tenant checks pass. There is no HTTP server or
identity-provider integration yet, so this is a library boundary today.

## 3. Database access

Axiom code never names a vendor. It selects a named connection, and each
connection has its own policy:

```yaml
databases:
  app:
    driver: sqlite
    path: "var/app.sqlite"
    data_class: internal
    allowed_tables: [teams, tasks, v_team_performance]
    max_rows: 1000
    timeout_seconds: 5
    migrations: "db/migrations"
    destructive_migrations: false
    require_write_approval: true

  analytics:
    driver: sqlite
    path: "var/analytics.sqlite"
    data_class: confidential
    allowed_tables: [events]
    max_rows: 500
    timeout_seconds: 10
    require_write_approval: true
```

Reads use `db.query` or `db.query_one`; writes use `db.execute`:

```axiom
let rows = await db.query<DashboardSummary>(
  database: "analytics",
  sql: "SELECT team_id, completed FROM v_team_performance WHERE team_id = :team_id",
  params: { team_id: input.team_id },
  max_rows: 500,
  timeout: 10s
);
```

Rules that the runtime enforces:

- SQL must be one literal statement with named placeholders. Anonymous `?`
  placeholders, multiple statements, `pragma`, `attach`, `load_extension`, and
  transaction control are refused.
- Tables must be inside `allowed_tables`.
- Reads are bounded by `max_rows` and a timeout.
- Writes are gated by approval, bound to the SQL fingerprint, parameter hash,
  row cap, action hash, and expiry. Replays use idempotency keys.
- Audit records contain fingerprints, counts, data class, and outcome, never SQL
  text, bound values, credentials, or raw driver errors.
- `migrations` are reviewed files with checksums; destructive changes require an
  explicit gate.

Validate the configured connections and SQLite role isolation:

```sh
axiom db check --config examples/axiom.yaml
axiom db migrations plan --config examples/axiom.yaml --database app
```

`sqlite` is the bundled reference driver. Connections are driver-independent
([ADR-0015](decisions/0015-database-independent-connections.md)); other
drivers are optional host adapters and none ship in this repository.

## 4. Frontend state and events

A frontend module follows a small convention: `init`, `render`, and
`handle_event`.

```axiom
module examples.frontend;

import { DashboardInput, DashboardState, DashboardEvent,
         DashboardEventKind, TimeRange, AppError } from "./shared.ax";
import { get_dashboard } from "./server.ax";

export fn init(input: DashboardInput) -> DashboardState {
  return {
    team_id: input.team_id,
    completed: 0,
    range: TimeRange.Month,
    loading: false
  };
}

export fn render(state: DashboardState) -> UiTree {
  return ui.column([
    ui.heading("TeamBoard"),
    ui.text("Team: " + state.team_id),
    ui.metric("Completed tasks", state.completed),
    ui.text("Range: " + state.range.to_string()),
    ui.button("Refresh", { kind: DashboardEventKind.Refresh, range: None })
  ]);
}

export async fn handle_event(
  state: DashboardState,
  event: DashboardEvent
) -> Result<DashboardState, AppError>
  effects [api.call] {
  match event.kind {
    DashboardEventKind.Refresh => {
      match await get_dashboard({ team_id: state.team_id }) {
        Ok(summary) => return Ok({
          team_id: state.team_id,
          completed: summary.completed,
          range: state.range,
          loading: false
        });
        Err(error) => return Err(error);
      }
    }
    DashboardEventKind.SetRange => {
      return Ok({
        team_id: state.team_id,
        completed: state.completed,
        range: event.range.unwrap_or(state.range),
        loading: state.loading
      });
    }
  }
}
```

The entry points and state metadata are configuration:

```yaml
compilation:
  targets: [server, wasm_browser]
  entrypoints:
    server:
      module: examples.server
    wasm_browser:
      module: examples.frontend
      init: init
      render: render
      handle_event: handle_event

frontend_state:
  id: teamboard.dashboard
  type: examples.shared.DashboardState
  schema_version: 1
  scope: route
  persistence: session
```

`UiTree` is a typed value built with `ui.*` functions, not HTML. Rendering is a
pure projection from state. `handle_event` returns a new state and may declare
`api.call`.

Important limitations:

- `axiom check` validates the source, but the browser host shell, DOM renderer,
  and typed API stubs are not implemented. The reference frontend cannot build
  for `wasm_browser` while it imports `get_dashboard`, because the server body
  uses `db.read`.
- A pure browser-safe frontend can build:
  ```sh
  axiom build --target wasm_browser --out dist hello/frontend/main.ax
  ```
- This produces a verifiable artifact, not a rendered UI. See
  [GitHub Pages](github-pages.md)
  for the deployment contract.

## 5. Agents, workspaces, and validation

A workspace is the maximum filesystem authority; an agent's workspace cannot
widen it:

```axiom
export workspace frontend_edit {
  root: ".";
  read: ["examples/src/frontend.ax", "examples/src/shared.ax"];
  write: ["examples/src/frontend.ax"];
  deny: ["examples/src/server.ax", "axiom.yaml", "db/migrations/**"];
  isolation: copy_on_write;
  limits: { max_files: 200, max_file_bytes: 1000000, max_changed_files: 30 };
}
```

An agent declares typed input/output, a model alias, tools, and limits:

```axiom
export agent FrontendDesigner(input: FrontendChange) -> FrontendPatch {
  model: "coding";
  workspace: frontend_edit;
  tools: [fs.read, fs.search, fs.write, compiler.check];
  limits: { max_calls: 6, timeout: 120s, max_output_bytes: 1000000 };
}
```

The workflow is an ordinary async function that calls `agents.iterate`:

```axiom
export async fn update_frontend(
  input: FrontendChange
) -> Result<FrontendRelease, AppError>
  effects [model, fs.read, fs.write, workspace.create, command.run, artifact.publish] {
  let candidate = await agents.iterate(
    agent: FrontendDesigner,
    input: input,
    max_attempts: 3,
    validate: [
      checks.ax_compile,
      checks.wasm_build,
      checks.api_compatibility,
      checks.workspace_scope,
      checks.browser_tests
    ]
  );
  match candidate.accepted_artifact() {
    Ok(artifact) => return await frontend.publish(artifact);
    Err(_) => return Err(AppError.ValidationFailed);
  }
}
```

Check the workspace policy:

```sh
axiom workspace check examples/src/agents.ax
```

A self-contained version is also checked in as
[`examples/src/agent_designer.ax`](https://github.com/jvdhtm-lab/axiom/blob/master/examples/src/agent_designer.ax):

```sh
axiom check examples/src/agent_designer.ax
axiom workspace check examples/src/agent_designer.ax
```

The runtime library runs `agents.iterate` with runtime-owned acceptance, a
fail-closed validator registry, content-addressed candidates, and declared
budgets. The repository ships an offline mock provider only. There are no
networked adapters, and the CLI does not run agent workflows yet; the behavior is
covered by runtime tests.

## 6. Configuration for models, tools, and commands

Model aliases point at integrations; `.ax` files reference the alias, never
credentials:

```yaml
integrations:
  default_cloud:
    adapter: openai_compatible
    base_url: "${AXIOM_MODEL_BASE_URL:-https://api.example.com/v1}"
    api_key: "${AXIOM_MODEL_API_KEY}"
    timeout_seconds: 120

models:
  coding:
    integration: default_cloud
    model: "${AXIOM_CODING_MODEL:-example-coder}"
```

Command profiles are allowlisted executable plus fixed arguments — never a shell
string assembled from model output:

```yaml
commands:
  unit_tests:
    executable: cargo
    args: ["test", "--offline", "--workspace"]
    working_directory: "."
    timeout_seconds: 600
    network: false
```

Inspect a command plan without running it:

```sh
axiom sandbox plan --config examples/axiom.yaml --command cargo_check
```

Secrets are expanded explicitly by Axiom (`${VAR}` and `${VAR:-default}`) and
redacted from diagnostics. The repository ships no networked provider adapter;
the mock provider is deterministic for tests.

## 7. CLI cookbook

```sh
# Diagnostics
axiom check src/**/*.ax --config axiom.yaml
axiom lint src/**/*.ax
axiom fmt src/**/*.ax --check
axiom fmt src/**/*.ax

# Typed IR and execution
axiom explain src/main.ax
axiom run src/main.ax
axiom run src/main.ax --fn other_entry

# Targets
axiom build src/main.ax
axiom build --target wasm_browser --out dist src/frontend.ax

# Configuration and isolation
axiom config check axiom.yaml
axiom workspace check src/agents.ax
axiom sandbox plan --config axiom.yaml --command unit_tests

# Artifacts and databases
axiom artifacts hash axiom.yaml
axiom db check --config axiom.yaml
axiom db migrations plan --config axiom.yaml --database app
```

The complete command and flag reference is in the [CLI guide](cli.md).

## Next steps

- [Quickstart](quickstart.md) — install and scaffold a project.
- [Language tour](language-tour.md) — the full construct-by-construct tour.
- [Language specification](language-spec.md) — normative semantics.
- [Configuration](configuration.md) — the strict `axiom.yaml` schema.
- [Security model](security-model.md) — the authority boundaries behind these
  examples.
- [Implementation ledger](implementation-ledger.md) — exact test evidence.
