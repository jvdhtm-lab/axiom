# ADR-0012: Semantic analysis scope for Phase 2

- Status: accepted
- Date: 2026-10-18

## Context

Axiom 1.0 Phase 2 is "module graph, symbols, type system, generics, Option/Result,
effects, source/config cross-validation". A full type-inference and
effect-propagation engine is a large body of work. The reference examples in
`agents.md` §11 intentionally reference names that are external to any one module
(standard-library namespaces such as `fs`/`commands`/`artifacts`, workspace
instances, built-in enums, and undeclared tool namespaces), so a naive
"every referenced name must resolve" rule would reject the shipped examples.

## Decision

`axiom-types` implements Phase 2 in increments and is explicit about what it does
**not** do yet:

1. **Module graph.** Relative imports resolve deterministically and lexically;
   imports that do not resolve, name an unknown declaration, or name a
   non-exported declaration are errors, as are import cycles and duplicate
   `module` paths. Module names need not match file paths.
2. **Symbols.** One namespace of top-level declarations per module, recording
   kind, export status, and duplicates.
3. **Declaration checks.** Duplicate declarations/fields/enum members/member
   keys, built-in generic arity (`Option`, `Result`, `Map`, `Agent`), and record/
   interface/aggregate well-formedness.
4. **Effects.** Effect names are checked against the language's set; a `pure`
   function may not declare effects or perform `await`/`delegate`/`parallel`; a
   non-`pure` function must declare effects.
5. **Source/config cross-validation.** Literal model aliases (`model: "..."`) and
   `commands.run("...")` names are checked against a supplied `axiom.yaml`.
6. **No unknown-name errors for value/type references yet.** Unresolved
   references to stdlib namespaces, workspace instances, and built-in enums are
   accepted until the standard library and effect model define them (Phase 4/8).

## Consequences

- `axiom check` loads the module graph and runs these checks, optionally with
  `--config`, without breaking the reference examples.
- Full type inference, expression typing, `match` exhaustiveness, and
  inter-procedural effect propagation remain future work and are tracked as
  `partial`/`not started` in `docs/implementation-ledger.md`.
