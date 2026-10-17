# Axiom language specification (draft)

This is the Phase 0/1 draft. It is intentionally small and is the contract the
parser implements today. Grammar changes are recorded in `docs/decisions/`.

## Lexical structure

- **Encoding:** UTF-8. Identifiers are ASCII (`[A-Za-z_][A-Za-z0-9_]*`).
- **Line comments:** `// ...` to end of line.
- **Block comments:** `/* ... */` (not nested). A block comment containing a line
  break acts as a line break.
- **Whitespace:** spaces and tabs separate tokens. Line breaks are **significant**
  as entry separators inside bodies (see below); consecutive breaks collapse.
- **Strings:** double-quoted, with escapes `\" \\ \n \r \t`. An unknown escape is
  a diagnostic. Strings may contain any other UTF-8 character.
- **Numbers:** `123` (integer) or `0.2` (float).
- **Durations:** an integer immediately followed by `ms`, `s`, `m`, or `h`
  (e.g. `120s`, `250ms`). Stored internally as milliseconds.
- **Booleans:** the identifiers `true` and `false` in value position.

## Separators

Inside a declaration **body** (`{ ... }`), entries are separated by one or more
line breaks, commas, or semicolons. Inside **value** literals (arrays, objects,
call arguments) line breaks are insignificant and elements are separated by
commas.

## Grammar

```ebnf
source        = { declaration } EOF ;

declaration   = integration_decl | model_decl | agent_decl | instance_decl
              | type_decl | contract_decl | goal_decl | tool_decl
              | runtime_decl | workflow_decl ;

integration_decl = "integration" ident block ;
model_decl       = "model" ident block ;
goal_decl        = "goal" ident block ;
contract_decl    = "contract" ident block ;
tool_decl        = "tool" ident [ "(" params ")" ] block ;
runtime_decl     = "runtime" [ ident ] block ;
type_decl        = ( "type" | "interface" ) ident type_body ;

block         = "{" { entry } "}" ;
entry         = ident ":" value ;

value         = string | number | duration | boolean | path | call
              | array | object ;
call          = ident "(" [ arg { "," arg } ] ")" ;
arg           = [ ident ":" ] value ;
array         = "[" [ value { "," value } ] "]" ;
object        = "{" [ entry { "," entry } ] "}" ;

type_body     = "{" { field } "}" ;
field         = ident ":" type_ref ;
type_ref      = path [ "<" type_ref { "," type_ref } ">" ] ;

params        = param { "," param } ;
param         = ident [ ":" type_ref ] ;

agent_decl    = "agent" ident [ "implements" type_ref ] "{" { agent_item } "}" ;
agent_item    = entry | permissions_block | constraints_block ;
permissions_block = "permissions" block ;
constraints_block = "constraints" block ;

instance_decl = "let" ident "=" expr ;

workflow_decl = "workflow" ident [ "(" params ")" ] "{" { workflow_item } "}" ;
workflow_item = "let" ident "=" expr
              | "return" [ expr ]
              | "require" expr [ "," string ]
              | "parallel" "{" { workflow_item } "}"
              | if_item
              | expr ;
if_item       = "if" expr "{" { workflow_item } "}"
                [ "else" ( if_item | "{" { workflow_item } "}" ) ] ;

expr          = postfix ;
postfix       = primary { "." ident | "." "with" object } ;
primary       = path [ "(" [ expr { "," expr } ] ")" ]
              | string | number | duration | boolean
              | array_expr | object_expr | "(" expr ")" ;
array_expr    = "[" [ expr { "," expr } ] "]" ;
object_expr   = "{" [ entry { "," entry } ] "}" ;

path          = ident { "." ident } ;
```

`env(...)` and `secret(...)` are ordinary calls. The lexer/parser do **not**
special-case them; they are validated in Phase 2 and resolved in Phase 4. A
secret argument is a *name*, never a value: secrets are never written into
compiled plans, traces, or error messages.

## Configuration precedence

```
per-invocation override > agent-instance override > agent declaration
  > project default > runtime default
```

Example:

```ts
runtime { default_model: "reasoning" }

agent Reviewer { /* model omitted → project/runtime default */ }
agent Architect { model: "reasoning" }        // declaration override
let localArchitect = Architect.with { model: "local_coding" }  // instance override

// one-call override (Phase 4+):  run localArchitect(input, { model: "other" })
```

## Diagnostics

Parsing never panics on malformed input. Every diagnostic carries a byte span and
is rendered with a line/column pointer and a source excerpt. The parser performs
error recovery, so a file can yield multiple diagnostics in one pass.

## What is *not* here yet

- No semantic analysis, symbol resolution, or type checking (Phase 2).
- No IR, no `explain`/`plan`, no execution (Phase 3+).
- Natural-language constraints (`must`, `must_not`) are **model-evaluated**, not
  statically verified; the spec does not claim otherwise.
