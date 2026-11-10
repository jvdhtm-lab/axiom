---
title: UI agent demo
description: Backend Axiom orchestrates a UI change, compiles a new frontend, publishes it, and the browser reloads it.
---

# UI agent demo

This is the target model: **backend Axiom + a frontend agent that changes the UI
on demand**.

The demo backend accepts a UI-change request, asks an agent for a new Axiom
frontend, validates and compiles it for `wasm_browser`, publishes the release, and
serves the new artifact. The trusted browser host reloads it, so the UI changes
without shipping handwritten frontend JavaScript.

Two agent modes are available:

- `template`: deterministic, offline, safe template selection (default).
- `opencode`: a real free model through OpenCode, restricted to a tool-less
  agent. The generated source is untrusted: it is parsed, type-checked, repaired
  once with structured findings, and compiled before publication.

## Run it

```sh
cargo run -p axiom-server --example ui_agent_demo
```

Open <http://127.0.0.1:8099/>. Choose `light`, `dark`, or `minimal`, then press
**Ask agent to update UI**.

### One-command launcher and recording

On Windows, the launcher builds, signs, starts, and opens the demo:

```powershell
powershell -File scripts/run-ui-demo.ps1
```

Use the real free OpenCode agent:

```powershell
powershell -File scripts/install-opencode.ps1
powershell -File scripts/run-ui-demo.ps1 -Agent opencode
```

For a clean app window that is easy to screen-record:

```powershell
powershell -File scripts/run-ui-demo.ps1 -Agent opencode -Record
```

`-Record` opens Chrome/Edge with `--app=` at 1280?900, so there is no browser
chrome in the capture. Use OBS, Xbox Game Bar, or any screen recorder. Other
options:

```powershell
powershell -File scripts/run-ui-demo.ps1 -Agent opencode -Detach
powershell -File scripts/run-ui-demo.ps1 -Stop
powershell -File scripts/run-ui-demo.ps1 -Port 8100 -Release
```

The OpenCode mode defaults to the free `opencode-go/beam-free` model and the
tool-less `axiom-ui` agent in `.opencode/agents/axiom-ui.md`. Override with
`OPENCODE_MODEL` and `OPENCODE_AGENT`; set `OPENCODE_BIN` if the CLI is not in
`.tools/opencode/`.

The browser test runs the whole loop:

```sh
cargo build -p axiom-server --example ui_agent_demo
# start the server in another terminal
cargo run -p axiom-server --example ui_agent_demo

node browser/headless-ui-agent.mjs
```

Expected:

```text
ok: backend agent published a new frontend and the browser UI changed on demand
```

## Flow

```text
```text
control page
  POST /api/update_frontend { request: "dark analytics dashboard..." }
        |
        v
  ApiGuard: authentication + ui:write
        |
        v
  template mode: map request to a declared template
  OpenCode mode: `opencode run --standalone --model opencode-go/beam-free
                 --agent axiom-ui` (tool-less agent)
        |
        v
  extract the ax code block
        |
        v
  parse -> type/effect check -> repair once on failure -> lower -> wasm
        |
        v
  publish in-memory release { release_id, content_hash, agent, theme }
        |
        v
  host reloads /app/ with the new manifest + wasm
        |
        v
  browser verifies hash/ABI/IR, interprets UiTree, renders the new UI
```

The control page is a trusted demo shell. Application UI stays in `.ax`. In
template mode the agent selects declared templates; in OpenCode mode the model
output is untrusted source that must pass validation before publication.

## What it proves

- The backend can generate a new frontend and compile it to a browser artifact.
- Typed API stubs and the browser host work with a real reload.
- The UI changes on demand while application logic remains in Axiom.
- `ApiGuard` protects the update operation; the browser never receives server
  authority.
- The artifact is content-addressed and the host verifies it before rendering.

## Test both modes

Template mode is deterministic and runs in CI:

```sh
node browser/headless-ui-agent.mjs
```

OpenCode mode needs the local CLI and network/model access:

```sh
node browser/headless-ui-agent.mjs --request "Create a dark analytics dashboard with three metrics: tasks, reviews, incidents. Keep the increment button."
```

Expected:

```text
ok: a free OpenCode model generated and published a new browser UI on demand
```

## Honest boundaries

- OpenCode mode uses an external CLI, the user's OpenCode auth, and a free model
  whose availability can change. It is not an in-tree networked provider adapter.
- The model is restricted to a tool-less agent, but its source output is still
  untrusted and must pass the Axiom checker/compiler.
- The release pointer is in memory; there is no persistent registry, atomic
  cross-process switch, or rollback in the demo (the artifact registry library
  has tested rollback semantics).
- There is no database or identity provider in the demo; the backend and
  `ApiGuard` are exercised at the HTTP boundary.
- The browser host interprets embedded IR; function-level WASM codegen and worker
  isolation remain hardening work.
