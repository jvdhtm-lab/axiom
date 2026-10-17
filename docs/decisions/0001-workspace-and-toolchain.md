# ADR-0001: Cargo workspace layout and pinned GNU toolchain

**Status:** accepted (Phase 0)

## Context

Axiom is a Rust project with a compiler front end and a planned runtime. It must
build on the author's Windows machine, which has no MSVC linker and an
Application Control policy that can block unsigned artifacts, as well as on CI
(Linux).

## Decision

- Use a **Cargo workspace** with small, purpose-built crates. Start with
  `axiom-syntax` (front end, dependency-free) and `axiom-cli` (the `axiom`
  binary). Add `axiom-types`, `axiom-ir`, `axiom-providers`, `axiom-runtime` in
  later phases — not up front.
- Keep **dependency direction acyclic**: the syntax crate never depends on the
  runtime; only the CLI (and later runtime) depend downward.
- Pin the toolchain in `rust-toolchain.toml` to **1.99.0**, which resolves to the
  GNU host (`x86_64-pc-windows-gnu`) on Windows and the default host on Linux.
- Keep Cargo caches off the small `C:` drive (`CARGO_HOME`, `TEMP`, `TMP`).

## Consequences

A single `cargo test --workspace` exercises the front end with no external
services. The front end builds with an installed GNU toolchain and no third-party
dependencies, which also sidesteps the Application Control content-block risk in
early phases. Splitting into more crates is deferred until a real boundary needs
it.
