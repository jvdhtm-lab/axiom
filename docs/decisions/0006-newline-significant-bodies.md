# ADR-0006: Line breaks are entry separators in bodies

**Status:** accepted (Phase 0)

## Context

The target syntax reads like TypeScript but omits commas between declaration
body entries:

```ts
integration local_llm {
  adapter: openai_compatible
  base_url: env("AXIOM_LOCAL_BASE_URL", "http://localhost:11434/v1")
}
```

while value literals use commas:

```ts
capabilities: [chat, structured_output]
defaults: { temperature: 0.2 }
```

## Decision

The lexer emits explicit `Newline` tokens (consecutive breaks collapse; block
comments containing a line break count as one). The parser uses them
contextually:

- In a **declaration body**, entries are separated by one or more line breaks,
  commas, or semicolons (lenient).
- In **value literals** (arrays, objects, call arguments), line breaks are
  insignificant and elements are separated by commas.

## Consequences

Expressions can span lines, and bodies do not require punctuation. Comments do
not merge two entries (a block comment spanning lines emits a break). The tradeoff
is that newline handling is deliberate parser behavior with dedicated tests
(both separators, mixed, and trailing separators).
