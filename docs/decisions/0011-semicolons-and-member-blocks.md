# ADR-0011: Semicolon-terminated statements and member blocks

- Status: accepted
- Date: 2026-10-18
- Supersedes: ADR-0006

## Context

ADR-0006 made a line break an entry separator inside declaration bodies. The
rewritten Axiom 1.0 master prompt (`agents.md` §5.1) instead states that
**statements and member entries end with `;` and newlines are insignificant
whitespace**, and that declaration bodies are *member blocks* of the form
`name { key: value; ... }` (for example `constraints { ... }` and
`limits { ... }`). The parser had to be reworked to the 1.0 grammar, which made
ADR-0006 contradicted.

## Decision

1. Statements and member entries are terminated by `;`. A closing `}` also acts
   as the separator of the final entry in a block, so the last entry needs no
   trailing `;`.
2. The lexer still emits `Newline` tokens (kept in `ParseResult::tokens` for
   tooling), but the parser discards them, so a newline never terminates a
   statement.
3. Declaration bodies (`agent`, `team`, `tool`, `contract`, `goal`, `policy`,
   `workspace`) are member blocks: `name { entry; ... }`. An entry is
   `key: value`, `key { ... }` (nested block), or `Type -> Member` (routing).
   Contract clauses (`postcondition`, `criterion`, …) carry a label.
4. `delegate`, `parallel for`, and `await` are expressions; `await`, durations,
   and size literals are recognised by the lexer.

## Consequences

- `docs/grammar.ebnf` is the normative grammar and documents the member-block and
  `;` rules; `docs/language-spec.md` states that newlines are insignificant.
- Example programs in `examples/src/` are written in the 1.0 form and all parse
  cleanly.
- ADR-0006 is superseded and must not be relied upon.
