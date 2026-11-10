---
title: UI agent demo
description: Backend Axiom orchestrates a UI change, compiles a new frontend, publishes it, and the browser reloads it.
---

# UI agent demo

This is the target model: **backend Axiom + a frontend agent that changes the UI
on demand**.

The demo backend accepts a UI-change request, generates a new Axiom frontend from
declared templates, compiles it for `wasm_browser`, publishes the release, and
serves the new artifact. The trusted browser host reloads it, so the UI changes
without shipping handwritten frontend JavaScript.

## Run it

```sh
cargo run -p axiom-server --example ui_agent_demo
```

Open <http://127.0.0.1:8099/>. Choose `light`, `dark`, or `minimal`, then press
**Ask agent to update UI**.

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
control page
  POST /api/update_frontend { request: "dark" }
        |
        v
  ApiGuard: authentication + ui:write
        |
        v
  deterministic agent maps the request to a declared template
        |
        v
  parse -> check -> lower -> wasm_browser artifact
        |
        v
  publish in-memory release { release_id, content_hash, theme }
        |
        v
  host reloads /app/ with the new manifest + wasm
        |
        v
  browser verifies hash/ABI/IR, interprets UiTree, renders the new UI
```

The control page is a trusted demo shell. Application UI stays in `.ax`; the
agent only selects between declared templates. Arbitrary request text is never
interpolated into source.

## What it proves

- The backend can generate a new frontend and compile it to a browser artifact.
- Typed API stubs and the browser host work with a real reload.
- The UI changes on demand while application logic remains in Axiom.
- `ApiGuard` protects the update operation; the browser never receives server
  authority.
- The artifact is content-addressed and the host verifies it before rendering.

## Honest boundaries

- The "agent" is deterministic and offline. A real model/provider adapter is
  still needed; this demo uses declared templates to avoid untrusted source
  generation.
- The release pointer is in memory; there is no persistent registry, atomic
  cross-process switch, or rollback in the demo (the artifact registry library
  has tested rollback semantics).
- There is no database or identity provider in the demo; the backend and
  `ApiGuard` are exercised at the HTTP boundary.
- The browser host interprets embedded IR; function-level WASM codegen and worker
  isolation remain hardening work.
