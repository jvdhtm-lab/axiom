# ADR-0009: YAML parsing dependency (`yaml_serde`)

**Status:** accepted (Phase 1)

## Context

`axiom.yaml` must be parsed with real YAML semantics (comments, nested mappings,
sequences, flow style) and clear error locations. Writing a YAML parser by hand
is disproportionate to the task. The obvious crate, `serde_yaml`, is
**archived/deprecated** (`0.9.34+deprecated`); `serde_yml` is also archived. The
prompt requires checking current stable options before choosing a dependency.

Maintained alternatives (checked 2026, via crates.io + crate docs):

- `yaml_serde` 0.10 — the official YAML-organization fork of `serde_yaml`; full
  compatibility, `serde`-based, exposes `Value`/`Mapping` and error locations.
- `serde-saphyr` 1.x/2.x — typed-only deserializer (no `Value` DOM), Rust ≥ 1.89.
- `saphyr` / `yaml-rust2` — lower-level YAML 1.2 parser primitives, no serde.

## Decision

Use **`yaml_serde` 0.10.7** in the new `axiom-config` crate. It is a drop-in,
maintained successor to `serde_yaml`, gives a `serde` value tree we can walk for
precise, contextual diagnostics, and reports syntax errors with line/column
locations. `axiom-syntax` keeps **zero** third-party dependencies (ADR-0002);
only `axiom-config` gains `yaml_serde` (and its transitive `serde`).

## Consequences

- `axiom-config` depends on `yaml_serde`; `axiom-syntax` and the `.ax` front end
  remain dependency-free.
- We walk the `Value` tree ourselves instead of deriving `Deserialize`, which
  keeps control over messages and cross-field checks, at the cost of a little
  more code.
- Value-level diagnostics carry approximate spans (see `docs/configuration.md`);
  syntax errors carry exact locations.
- If `yaml_serde` is later abandoned, migration to `serde-saphyr`/`saphyr` is a
  contained change behind the `axiom-config` boundary and would be a new ADR.
