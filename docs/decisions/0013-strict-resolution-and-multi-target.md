# ADR-0013: Strict name resolution and multi-target/frontend scope

- Status: accepted
- Date: 2026-10-18

## Context

ADR-0012 scoped Phase 2 to declaration/effect checks and explicitly deferred
unknown-name errors ("No unknown-name errors for value/type references yet").
The project owner has since directed that name resolution be **strict**: an
unresolved value or type reference is an error, not a silent pass. Separately,
the Axiom 1.0 specification grew a **multi-target** shape — one compiler
front end feeding a server target and a `wasm_browser` target — with first-class
frontend/UI concepts: `operation` declarations, `iterate` expressions, the
`ui`/`api`/`browser` standard namespaces, and expanded effects
(`api.call`, `ui.render`/`ui.event`/`ui.state`,
`browser.storage`/`navigation`/`clipboard`/`download`).

## Decision

1. **Strict resolution.** `axiom-types::resolve` walks each module body and
   reports every reference that does not resolve to a local declaration, an
   import, a built-in value/type, or a language namespace. This supersedes
   ADR-0012 decision item 6. `axiom check` therefore errors on unresolved
   value/type names.
2. **Built-in surface.** To keep strict resolution practical, the resolver
   recognises the language namespaces (`fs`, `commands`, `artifacts`, `db`,
   `ui`, `api`, `browser`, ...), the built-in types named in the specification
   (including `UiTree`, `UiError`, `Capability`, `AuthenticationRequirement`,
   `Validator`/`Validation*`, `InvokeOptions`, `IterationOutcome`/`Attempt`),
   and the built-in constructors `Ok`/`Err`/`Some`/`None` plus the bindings
   `iteration`, `capability`.
3. **Operation and iterate.** `operation` declarations are parsed and checked
   (`version`, `authentication`, optional `requires`/`data_classes`, `handler`);
   the `handler` target is resolved cross-module but not required to be a
   value in the current module. `iterate <call> { max_iterations; validate;
   until; }` is parsed, resolved, and checked; `validate:` entries name
   registered validators (validator registry is Phase 8) and are not module
   values.
4. **Multi-target/frontend scope.** The reference program adds a frontend slice
   (`frontend_contracts.ax`, `performance_dashboard.ax`, `server_dashboard.ax`)
   that exercises operations, UI/state effects, and dotted enum patterns. The
   dashboard `initialize` entry point is written as `export pure fn` so it
   satisfies the pure/effect rule; making an entry point that both reads state
   and is pure is a deliberate, revisitable deviation recorded here.
5. **Configuration.** `axiom.yaml` accepts `language` and `sources`
   top-level keys and per-integration `dialect` and `allowed_data_classes`
   (`secret` is rejected).

## Consequences

- The shipped reference examples must resolve strictly; they were adjusted to
  do so (`hello_agent.ax` removed; `support_db.ax` and the frontend slice added).
- New crates (`axiom-ir`, `axiom-codegen-server`, `axiom-codegen-wasm`,
  `axiom-wasm-artifacts`, `axiom-ui`, `axiom-browser-abi`) are specified but not
  yet built; the front end only parses and checks the frontend surface today.
- Full type inference, expression typing, `match` exhaustiveness, and
  inter-procedural effect propagation remain future work.
