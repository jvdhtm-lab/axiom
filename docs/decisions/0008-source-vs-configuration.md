# ADR-0008: `.ax` source versus `axiom.yaml` configuration

**Status:** accepted (Phase 0/1)

## Context

Axiom must keep the portable language separate from deployment-specific
configuration. Bundling endpoints, credentials, and environment references into
`.ax` source would make agent definitions non-portable and leak secrets into
artifacts. Earlier drafts placed `integration` and `model` declarations inside
`.ax` files; the canonical scope now assigns connection/endpoint/profile
configuration to `axiom.yaml`.

## Decision

- **`.ax`** holds the language: types, agents, goals, contracts, tools, policies,
  and workflows. It must not contain credentials or environment-specific
  endpoints; it refers to model profiles by logical alias only.
- **`axiom.yaml`** holds deployment/environment configuration: provider
  connections (adapter, base URL, secret references, timeouts, headers), model
  profiles (`provider` + concrete `model` id + declared capabilities), and
  runtime defaults/limits.
- Environment references in configuration use explicit `${NAME}` /
  `${NAME:-default}` expansion implemented by Axiom, never by the YAML parser.
  Secrets are resolved at runtime and redacted from logs, diagnostics, IR, and
  traces.
- The `.ax` grammar still accepts `integration` and `model` declarations
  (implemented in Phase 1) for portability and self-contained examples. They are
  valid syntax and parse cleanly; how embedded declarations interact with
  `axiom.yaml` (merging, precedence, or rejection) is deferred to Phase 2 and
  will be documented there.

## Consequences

- Configuration is validated independently of source (`axiom config check`), and
  the source front end stays free of credentials.
- Two canonical file types, with clear ownership of each concern.
- A small amount of duplication is possible while embedded `.ax` declarations
  remain supported; Phase 2 must resolve that explicitly rather than silently
  choosing a winner.
