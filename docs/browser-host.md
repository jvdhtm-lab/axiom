---
title: Browser host
description: The trusted Axiom browser host, IR interpreter, UiTree renderer, event loop, and headless-browser test.
---

# Browser host

The trusted browser host lives in `browser/`:

| File | Role |
| --- | --- |
| `browser/index.html` | Host page, accessible shell, status region |
| `browser/host.js` | Artifact verification, IR interpreter, UiTree renderer, event loop |
| `browser/headless-smoke.mjs` | Real Chrome/Edge headless smoke test |
| `browser/host-smoke.mjs` | Node-side artifact/ABI verifier |

The host is not a second frontend framework. It contains no application-specific
business logic: application code stays in `.ax`, the checked IR describes the
functions, and the host only verifies, interprets the portable subset, and
renders the typed `UiTree` with DOM APIs.

## What it does

1. Fetches `app.manifest.json` and `app.wasm` with relative paths.
2. Recomputes SHA-256 and verifies `content_hash`, `byte_len`, `abi_version`,
   `ir_version`, target, and imports before running anything.
3. Instantiates the WASM module, reads the embedded checked IR from linear
   memory, and parses it.
4. Interprets the portable function subset (`init`, `render`, `handle_event`) —
   no application JavaScript.
5. Renders `UiTree` nodes (`column`, `row`, `heading`, `text`, `metric`,
   `button`) with `textContent`, never `innerHTML`.
6. Forwards button events to `handle_event`, accepts only an `Ok(new_state)`
   result, re-renders, and fails closed on `Err` or unknown nodes.
7. Implements caller-side `api.call`: imported API bodies are replaced by typed
   stubs, and the host POSTs the typed input to `window.AXIOM_API_BASE` or the
   `<meta name="axiom-api-base">` value (default `/api`).
8. Caps UI nodes at 10,000 and reports structured host status in
   `document.body.dataset.axiomStatus` and the status region.

Unknown UiTree nodes, unsupported host imports, ABI/IR mismatches, hash
mismatches, and non-`Ok` events are all errors.

## Try the demo

The repository ships a pure frontend module at `examples/src/ui_demo.ax`:

```sh
axiom build --target wasm_browser --out dist examples/src/ui_demo.ax
node browser/headless-smoke.mjs dist/app.wasm dist/app.manifest.json
```

Expected:

```text
ok: headless browser rendered UiTree and forwarded a button event
```

The test launches Chrome or Edge headless through the DevTools protocol, serves
the host and artifact over localhost, waits for the host status, clicks the
button, and asserts the count changed from `0` to `1`. Set `AXIOM_CHROME` to
override the browser path.

The reference frontend also builds for `wasm_browser` now: its server API body is
replaced by a typed stub, so no database code enters the artifact. The full
browser path is exercised with a mock API response:

```sh
axiom build --target wasm_browser --out dist_ref \
  examples/src/shared.ax examples/src/frontend.ax examples/src/server.ax

node browser/headless-smoke.mjs dist_ref/app.wasm dist_ref/app.manifest.json \
  --input-file browser/fixtures/reference_input.json \
  --api-file browser/fixtures/reference_api.json \
  --expect "TeamBoard" --expect "Completed tasks" --expect "7"
```

The host posts to `/api/get_dashboard`; the test server returns the mock
`DashboardSummary`. The real backend is `axiom-server` (tested with the same
handler and a mock DB).

To preview manually, serve the repository and artifact with any static server
and open `browser/index.html` with `app.wasm` and `app.manifest.json` beside the
host files.

## Current limits

- The artifact still embeds IR (`codegen: "ir-embedding"`). There is no
  function-level WASM code generation yet; the host interprets the IR.
- Supported `UiTree` nodes are the small demo subset; forms, tables, and richer
  inputs are not implemented.
- The host runs on the main thread; dedicated-worker isolation is not
  implemented. Storage and navigation host imports are not implemented.
- The reference frontend builds and passes the mock-API browser test, but a
  production backend (identity provider, TLS/CORS, live DB) is not wired yet.
- Typed API stubs are caller-side only; the server enforces authentication and
  authorization on every request.
- There is no production release registry or atomic active-release switch in the
  browser yet.

See [Publishing to GitHub Pages](github-pages.md) for deploying the demo host.
