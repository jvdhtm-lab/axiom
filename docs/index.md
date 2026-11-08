---
title: Axiom
---

# Axiom

Axiom is a small, strongly typed language and runtime for building full-stack
applications with AI agents operating inside explicit boundaries and isolated
workspaces. One source language and one shared type/effect system compile for
`server` and `wasm_browser`.

> Boundaries define authority. Workspaces define where work happens. Agents
> propose changes. Validation determines what is accepted.

## Start here

- [Language specification](language-spec.md) — types, declarations, control flow, effects.
- [Grammar](grammar.ebnf) — the normative EBNF (served as a plain file).
- [Configuration](configuration.md) — `axiom.yaml`, model aliases, databases, commands.
- [Architecture](architecture.md) — compiler pipeline, crates, and runtime hosts.
- [Security model](security-model.md) — threat model, boundaries, and residual risks.
- [Conformance](conformance.md) — requirement-to-test matrix.
- [Implementation ledger](implementation-ledger.md) — what is done, partial, or missing.
- [Release readiness](release-readiness.md) — audit evidence and open blockers.
- [Providers](providers.md) — model integrations and offline mocks.
- [Development](development.md) — toolchain, gate commands, Windows notes.
- [Publishing to GitHub Pages](github-pages.md) — docs site and Axiom frontend deployment.
- [Architecture decisions](decisions/) — ADRs.

## Project status

Axiom 1.0 is **not complete**. The implementation ledger is the source of truth;
a feature is not claimed unless a passing test is cited there. The browser
target currently emits a verifiable WASM artifact that embeds the checked IR,
but there is no DOM renderer or trusted browser host shell yet.

## Repository files outside this site

Some links in these pages point to files kept in the repository but outside
`docs/` (for example the reference application and the top-level specification).
Those links work on GitHub, not on the rendered Pages site:

- [Repository](https://github.com/jvdhtm-lab/axiom)
- [`agents.md`](https://github.com/jvdhtm-lab/axiom/blob/master/agents.md) — project specification.
- [`README.md`](https://github.com/jvdhtm-lab/axiom/blob/master/README.md)
- [`examples/`](https://github.com/jvdhtm-lab/axiom/tree/master/examples) — reference project.
- [`editors/vscode/`](https://github.com/jvdhtm-lab/axiom/tree/master/editors/vscode) — `.ax` syntax highlighting.
