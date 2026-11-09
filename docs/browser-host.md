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
7. Caps UI nodes at 10,000 and reports structured host status in
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

To preview manually, serve the repository and artifact with any static server
and open `browser/index.html` with `app.wasm` and `app.manifest.json` beside the
host files.

## Current limits

- The artifact still embeds IR (`codegen: "ir-embedding"`). There is no
  function-level WASM code generation yet; the host interprets the IR.
- Only the portable subset used by `ui_demo.ax` is implemented (no typed API
  stubs, storage, navigation, or forms yet).
- The reference `examples/src/frontend.ax` still cannot build for
  `wasm_browser` because it imports `get_dashboard`, whose server body uses
  `db.read`.
- The host ABI imports are verified but not implemented; any imported call fails
  closed.
- There is no production release registry or atomic active-release switch in the
  browser yet.

See [Publishing to GitHub Pages](github-pages.md) for deploying the demo host.
