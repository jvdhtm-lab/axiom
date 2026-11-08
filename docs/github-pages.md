# Using Axiom with GitHub Pages

GitHub Pages is a static hosting service. It can serve two different things for
this project:

1. **The Axiom documentation site** (this `docs/` directory), published as HTML
   by Jekyll or GitHub Actions.
2. **An Axiom `wasm_browser` frontend** (a compiled `.wasm` artifact plus a
   trusted host shell), published as static files.

Part 1 works today. Part 2 is **not a working interactive application yet**:
Axiom emits a verifiable browser artifact, but the trusted browser host shell,
DOM renderer, event loop, and typed API stubs are still missing. Part 2 documents
the intended layout and CI so the deployment can be enabled when that host
lands. Do not advertise an interactive Axiom app on Pages before then.

---

## Part 1 — Publish the Axiom documentation site

### What gets published

The `docs/` directory is a Jekyll-compatible site:

| File | Purpose |
| --- | --- |
| `docs/index.md` | Landing page and navigation |
| `docs/_config.yml` | Jekyll title, defaults, plugins, `baseurl` |
| `docs/_layouts/` | Custom site and documentation page layouts |
| `docs/assets/` | Site CSS and favicon |
| `docs/quickstart.md` | Install, scaffold, check, run, and build |
| `docs/language-tour.md` | Construct-by-construct language guide |
| `docs/examples.md` | Complete server, database, frontend, and agent examples |
| `docs/cli.md` | Complete CLI reference |
| `docs/*.md` | Language spec, configuration, architecture, security, ledger, etc. |
| `docs/decisions/*.md` | ADRs; `decisions/README.md` becomes the section index |
| `docs/grammar.ebnf` | Served as a downloadable plain file |

The current `_config.yml` targets the project-site URL
`https://jvdhtm-lab.github.io/axiom/` with `baseurl: "/axiom"`. Change `url` and
`baseurl` if you fork or rename the repository.

### Option A — deploy from the `docs/` folder (no workflow)

This is the simplest option. GitHub runs Jekyll for you.

1. Commit and push `docs/index.md` and `docs/_config.yml`.
2. In the repository, open **Settings → Pages**.
3. Under **Build and deployment**, set **Source** to **Deploy from a branch**.
4. Choose your default branch (`master` for this repository; `main` in many
   forks) and the folder **`/docs`**.
5. Press **Save**. GitHub queues a Pages build.
6. Open `https://<owner>.github.io/<repo>/` after the build finishes. For this
   repository that is <https://jvdhtm-lab.github.io/axiom/>.
7. In **Settings → Pages**, use **Visit site** to confirm the deploy.

Changes pushed to the selected branch/folder are published automatically. The
branch build has a soft limit of about 10 builds per hour; batch documentation
changes if you hit it.

### Option B — deploy with GitHub Actions

Use this when you want an explicit build, a custom Jekyll version, or one
workflow that also runs checks. Set **Settings → Pages → Source** to **GitHub
Actions** first; a repository cannot use branch source and Actions source at the
same time.

Create `.github/workflows/pages-docs.yml`:

```yaml
name: Deploy docs to GitHub Pages

on:
  push:
    branches: [master]   # use [main] if that is your default branch
  workflow_dispatch:

permissions:
  contents: read
  pages: write
  id-token: write

concurrency:
  group: pages
  cancel-in-progress: true

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/configure-pages@v5
      - name: Build the Jekyll site
        uses: actions/jekyll-build-pages@v1
        with:
          source: ./docs
          destination: ./_site
      - uses: actions/upload-pages-artifact@v3

  deploy:
    needs: build
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - id: deployment
        uses: actions/deploy-pages@v4
```

Notes:

- `id-token: write` is required for the OIDC handshake used by
  `actions/deploy-pages`.
- The `github-pages` environment protects the deployed URL and can require
  reviewers.
- `source: ./docs` means the repository root does **not** need a root
  `_config.yml`; the configuration lives in `docs/_config.yml`.
- Markdown files do not need YAML front matter because the site enables
  `jekyll-optional-front-matter`.
- `jekyll-readme-index` makes `docs/decisions/README.md` the index for
  `/decisions/`.
- `jekyll-relative-links` rewrites links such as `language-spec.md` to their
  generated `.html` URLs.

### Preview the site locally

With Docker (recommended on Windows). The site enables GitHub Pages plugins, so
install them in the container before serving:

```powershell
docker run --rm -it -p 4000:4000 `
  -v "${PWD}/docs:/srv/jekyll" `
  -w /srv/jekyll `
  jekyll/jekyll:latest `
  bash -lc "gem install jekyll-optional-front-matter jekyll-default-layout jekyll-readme-index jekyll-relative-links jekyll-titles-from-headings jekyll-sitemap jekyll-seo-tag jekyll-feed --no-document && jekyll serve --host 0.0.0.0 --baseurl ''"
```

Open <http://localhost:4000/>. The empty `--baseurl` overrides the production
`/axiom` prefix so local links resolve.

With Ruby and Bundler:

```sh
gem install bundler
cd docs
bundle init
bundle add github-pages --group jekyll_plugins
bundle exec jekyll serve --baseurl ""
```

The `github-pages` gem matches the plugin set and versions GitHub uses. A plain
`jekyll` gem may support plugins that GitHub Pages does not, and it must be
installed with the plugins listed in `docs/_config.yml`.

### Custom domain

1. Add the domain in **Settings → Pages → Custom domain**.
2. Create a DNS `CNAME` record for a subdomain (`docs.example.com` →
   `<owner>.github.io`) or the required `A`/`AAAA` records for an apex domain.
3. If you deploy from `/docs`, put the domain in `docs/CNAME` so it survives
   deploys. If you deploy from Actions, add `CNAME` to the artifact or set the
   custom domain in Settings.
4. Enable **Enforce HTTPS** after DNS verification.
5. For a custom domain served at the root, set `baseurl: ""` in
   `docs/_config.yml`.

### Troubleshooting the docs site

| Symptom | Cause and fix |
| --- | --- |
| “There isn’t a GitHub Pages site here” | Pages is not enabled, or the branch/folder does not match. Check **Settings → Pages**. |
| Markdown downloads instead of rendering | Jekyll is not processing the file. For branch source the folder must be `/docs` and `docs/_config.yml` must exist. For Actions source, the workflow must run `actions/jekyll-build-pages` or another site builder. |
| Theme CSS 404s under `/repo/` | `baseurl` is wrong. Project sites need `baseurl: "/<repo>"`; custom/user sites at the domain root need `baseurl: ""`. |
| Links to `../agents.md` or `../examples/` 404 | Those files are outside the published `docs/` folder. Link to GitHub instead (see `docs/index.md`) or copy the file into `docs/`. |
| A plugin is rejected by the Pages build | Only [GitHub Pages allowlisted plugins](https://pages.github.com/versions/) are supported by branch builds. Use one of them or build the site in Actions with a pinned toolchain. |
| Builds are queued or skipped | Branch-source builds are rate-limited (roughly 10 per hour). Push fewer, larger changes or switch to Actions. |
| Stale content after a deploy | Wait for the Pages deployment to finish, then hard-refresh. GitHub’s CDN may serve a previous build briefly. |
| Custom domain loops or shows 404 | Check DNS, remove conflicting `CNAME` files, and re-check **Enforce HTTPS**. |
| `_config.yml` seems ignored | For branch source, Pages reads `_config.yml` from the **publishing folder** (`docs/`), not always the repository root. |

### Documentation limitations

- The Pages site is static: no search service, comments, or authenticated pages.
- GitHub-rendered relative links to repository files outside `docs/` do not
  survive publication; use absolute GitHub URLs for those.
- `grammar.ebnf` is served as a file, not rendered as HTML.

---

## Part 2 — Deploy an Axiom frontend to GitHub Pages

### Current status (Axiom 0.1.0)

| Capability | Status |
| --- | --- |
| `axiom build --target wasm_browser --out <dir> <browser-safe .ax>` | works; writes `app.wasm` + `app.manifest.json` |
| Artifact verification (`node browser/host-smoke.mjs`) | works; checks magic, version, SHA-256, ABI, IR, imports |
| Trusted browser host shell (`index.html`, host JS, worker) | **not implemented**; `browser/` contains only the Node smoke harness |
| DOM renderer, `UiTree` rendering, event forwarding | **not implemented** |
| Typed browser API stubs for `api` functions | **not implemented** |
| Reference frontend (`examples/src/frontend.ax`) build for `wasm_browser` | **refused**: it imports `get_dashboard`, whose server body uses `db.read` |
| Interactive Axiom app on GitHub Pages | **not possible yet** |

What you *can* publish today is the artifact plus a hand-written static status
page. That is useful for smoke-testing deployment plumbing, not for shipping a
UI. The rest of this section defines the target deployment and the constraints
the host must satisfy.

### What GitHub Pages can and cannot do

GitHub Pages:

- serves static files over HTTPS;
- publishes a project site under `https://<owner>.github.io/<repo>/`;
- supports custom domains and HTTPS;
- does **not** run server code, databases, model providers, or migrations;
- does **not** let you set custom response headers (CSP, COOP, COEP,
  `Content-Type` overrides);
- does not protect files: anything in the artifact is public.

Consequences for Axiom:

- Never embed secrets, API keys, database URLs, or server source in `app.wasm`,
  `app.manifest.json`, the host JS, or the HTML.
- An `api` call from the browser needs a real backend hosted elsewhere (or a
  separate service). Pages cannot be that backend.
- Authorization must be enforced by the backend on every request; a browser stub
  or a shared type is not a grant.
- Browser state and storage are local and disposable; durable business records
  belong on the server.

### Build the artifact

```powershell
cargo build -p axiom-cli --release

# A browser-safe module (no server-only effects such as db.read).
.\target\release\axiom.exe build `
  --target wasm_browser `
  --out dist `
  examples\src\deterministic.ax

node browser\host-smoke.mjs dist\app.wasm dist\app.manifest.json
```

`build` writes:

```text
dist/
  app.wasm
  app.manifest.json
```

The manifest pins `content_hash`, `byte_len`, `abi_version`, `ir_version`,
`target`, `codegen`, `exports`, and `entry_points`. The artifact currently
embeds the checked IR (`codegen: "ir-embedding"`); it does not yet contain
per-function WASM functions or a UI renderer.

To build a frontend module, pass every `.ax` file in its module graph:

```powershell
.\target\release\axiom.exe build `
  --target wasm_browser `
  --out dist `
  my_app\frontend\main.ax
```

The build fails closed when the module graph uses an effect the browser target
does not support. That is intentional.

### Required site layout

When the host shell exists, a Pages artifact should look like this:

```text
dist/
  index.html                 # trusted host page
  host.js                    # loads and verifies the artifact
  host.worker.js             # optional: dedicated worker isolation
  app.wasm                   # compiled browser artifact
  app.manifest.json          # hashes, ABI, exports, IR version
  .nojekyll                  # for branch-based Pages deploys
```

`index.html` and the host JS are **trusted runtime code**, not generated
application logic. Application UI, state, and business logic stay in `.ax`.
Never hand-write application-specific React/TypeScript/JSX into the host.

### Host loading rules

The host must:

1. Fetch `app.manifest.json` and `app.wasm` with relative paths so project-site
   prefixes work:
   ```js
   const wasmUrl = new URL("./app.wasm", document.baseURI);
   ```
2. Verify WASM magic/version, `content_hash`, `byte_len`, `abi_version`,
   `ir_version`, `target`, exported entry points, and the import allowlist
   before running anything.
3. Fail closed on an unknown ABI version or an import outside the host ABI
   (`axiom.ui_render`, `axiom.ui_event`, `axiom.api_call`,
   `axiom.storage_get`, `axiom.storage_set`, `axiom.navigate`).
4. Instantiate with a non-streaming fallback because Pages does not guarantee
   `Content-Type: application/wasm`:
   ```js
   const response = await fetch(wasmUrl);
   const bytes = await response.arrayBuffer();
   const { instance } = await WebAssembly.instantiate(bytes, imports);
   ```
5. Keep execution under the configured limits: memory pages, per-event compute
   time, event bytes, UI node count, and storage bytes.
6. Render `UiTree` into accessible DOM. Do not interpret strings as HTML.
7. Forward typed events to the exported `handle_event` function and commit only
   validated state.
8. Route API calls through the server bridge; never let a fetched manifest,
   cache entry, or UI event widen authority.
9. Report structured errors and preserve the previous healthy release on
   failure.

`browser/host-smoke.mjs` implements the Node-side subset of steps 1-3 and is the
reference for the verification logic the browser host must repeat.

### Target GitHub Actions workflow

**Do not enable this until `browser/host-shell/` exists and the host passes a
real headless-browser test.** It shows the intended shape.

```yaml
name: Deploy Axiom frontend to GitHub Pages

on:
  push:
    branches: [master]   # use [main] if that is your default branch
  workflow_dispatch:

permissions:
  contents: read
  pages: write
  id-token: write

concurrency:
  group: pages
  cancel-in-progress: true

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
        with:
          components: rustfmt, clippy
      - name: Build the Axiom CLI
        run: cargo build -p axiom-cli --release
      - name: Compile the browser target
        run: |
          mkdir -p dist
          ./target/release/axiom build \
            --target wasm_browser \
            --out dist \
            my_app/frontend/main.ax
      - name: Copy the trusted host shell
        run: |
          cp browser/host-shell/index.html dist/
          cp browser/host-shell/host.js dist/
          cp browser/host-shell/host.worker.js dist/  # if used
          touch dist/.nojekyll
      - name: Verify the artifact
        run: node browser/host-smoke.mjs dist/app.wasm dist/app.manifest.json
      - uses: actions/upload-pages-artifact@v3
        with:
          path: dist

  deploy:
    needs: build
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - id: deployment
        uses: actions/deploy-pages@v4
```

Set **Settings → Pages → Source** to **GitHub Actions** for this workflow.
Unlike the docs site, do not add a Jekyll build step: the artifact is already
static.

### Alternative: publish a `gh-pages` branch

If you prefer branch-based hosting:

1. Build `dist/` in CI or locally.
2. Publish only `dist/` to a `gh-pages` branch, for example with
   `peaceiris/actions-gh-pages` or a dedicated worktree:
   ```sh
   git subtree push --prefix dist origin gh-pages
   ```
3. Set **Settings → Pages → Source** to **Deploy from a branch**, branch
   `gh-pages`, folder `/ (root)`.
4. Keep `.nojekyll` in `dist/` so filenames that start with `_` are served and
   Jekyll does not rewrite the artifact.

### Routing on Pages

GitHub Pages has no server-side rewrite. Choose one:

- **Hash routing** (`/#/team/42`): works with no extra files.
- **History routing** (`/team/42`): add a `404.html` that loads the same shell
  (copy `index.html` to `404.html`). Pages returns it for unknown paths; the
  client router then handles the URL. This is a static-host workaround, not
  server routing.
- **Custom 404 page**: a plain `404.html` that links back to the app root.

Keep asset URLs relative (`./app.wasm`, `./host.js`) so the app works under the
project-site prefix. If the host uses an explicit base path, derive it from
`document.baseURI`, not from a hard-coded `/`.

### Caching and releases

- `app.wasm` and `app.manifest.json` have stable names. Put the content hash in
  a query string or a versioned directory to avoid stale CDN copies:
  `./app.wasm?v=<content_hash>`.
- Keep the manifest and WASM matched. A manifest from one build with a WASM from
  another must fail the hash check.
- Publish immutable release directories (`releases/<content_hash>/...`) and
  point `index.html` at one. Roll back by switching the pointer, not by
  overwriting bytes.
- GitHub Pages cannot send custom cache headers; query strings and unique paths
  are the practical controls.

### Security headers and isolation

GitHub Pages cannot set response headers. You can add a CSP meta tag in
`index.html`, but it is weaker than a header:

```html
<meta
  http-equiv="Content-Security-Policy"
  content="default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self' https://api.example.com"
>
```

- `wasm-unsafe-eval` (or `unsafe-eval` on older browsers) is required for
  WebAssembly under strict CSP.
- `worker-src 'self'` is needed if the host uses a dedicated worker.
- If the host needs `SharedArrayBuffer`, it needs COOP/COEP headers, which
  GitHub Pages cannot provide. Use a custom host that can set headers, or avoid
  `SharedArrayBuffer`.
- A dedicated worker without `SharedArrayBuffer` still works; the spec’s
  isolation mode does not by itself require cross-origin isolation.

### Backend API calls

Pages cannot host the server side of Axiom. Until an HTTP adapter and typed
browser stubs exist:

- Browser code cannot call `get_dashboard`-style `api` functions for real.
- The reference `examples/src/frontend.ax` imports `get_dashboard`; the
  `wasm_browser` build is refused because the server body uses `db.read`. That
  refusal is correct: the compiler must not pull server handlers or database
  dependencies into the WASM module.
- A future build will compile the imported `api` function to a typed stub whose
  only caller-side effect is `api.call`. The stub calls a backend over HTTPS;
  the backend enforces authentication and authorization on every request.
- Host the backend somewhere that runs Rust (a container, a serverless function,
  or a dedicated service), configure CORS for the Pages origin, and never put
  credentials in the Pages artifact.

### Smoke-test the deployment plumbing today

You can validate the static artifact path without a UI:

```powershell
.\target\release\axiom.exe build `
  --target wasm_browser `
  --out dist `
  examples\src\deterministic.ax

node browser\host-smoke.mjs dist\app.wasm dist\app.manifest.json
```

Expected shape:

```text
ok: sha256 <prefix>… abi v1 ir v1 <bytes> bytes, 1 module(s)
```

If that passes locally, the same two files can be uploaded by a Pages workflow.
They will not render an application until the host shell exists.

### Troubleshooting an Axiom Pages deployment

| Symptom | Cause and fix |
| --- | --- |
| Build fails with “effect not available on `wasm_browser`” | The module graph imports server-only code (database, filesystem, commands, secrets). Split shared types from server functions; wait for typed API stubs for real calls. |
| `WebAssembly.instantiateStreaming` fails | Pages may serve `application/octet-stream`. Use `fetch(...).arrayBuffer()` + `WebAssembly.instantiate`. |
| `app.wasm` 404s under `/<repo>/` | Use `new URL("./app.wasm", document.baseURI)` or relative `./` paths, not `/app.wasm`. |
| Hash mismatch after deploy | Manifest and WASM came from different builds, or a stale CDN copy. Rebuild both together and add a cache-busting query/path. |
| Unknown import / ABI error | The host and compiler disagree on `abi_version` or the import allowlist. Rebuild the host and artifact from the same source revision. |
| API calls fail with CORS | The backend must allow the Pages origin and the request headers. Pages cannot proxy the request. |
| `SharedArrayBuffer is not defined` | COOP/COEP headers are missing and Pages cannot add them. Avoid `SharedArrayBuffer` or move to a host that sets headers. |
| Everything is public | Pages serves the artifact to anyone. Remove secrets; enforce authorization on the server. |

### Definition of “Axiom on GitHub Pages is working”

Do not call the Pages deployment complete until all of these are true:

1. `axiom build --target wasm_browser` succeeds for the frontend module graph.
2. The artifact passes `browser/host-smoke.mjs`.
3. A real headless-browser test renders a `UiTree`, forwards an event, and
   observes a state change.
4. The browser cannot reach server-only imports, database code, credentials, or
   the filesystem.
5. A typed `api` stub reaches a backend that enforces authorization on every
   request.
6. The deployed Pages URL loads under `/<repo>/` with relative asset paths.
7. A failed release preserves the previous healthy artifact.
8. State survives a layout-only redeploy or is migrated/rejected by policy.

Until then, publish the docs site (Part 1) and treat Part 2 as the deployment
design and security contract.
