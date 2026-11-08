# Axiom for VS Code

Syntax highlighting and editor configuration for Axiom (`.ax`) source files.

## Features

- TextMate highlighting for the small Axiom language surface: `module`,
  `import`, `export`, `type`, `enum`, `fn`, `agent`, `workspace`, `api`,
  `async`, `effects`, and `let`/`if`/`else`/`match`/`for`/`return`/`await`.
- Highlighting for primitives, `Option`/`Result`/`Map`/`UiTree`, prelude
  constructors (`Some`, `None`, `Ok`, `Err`), enum variants, effects, named
  arguments, strings, durations, sizes, and comments.
- Line (`//`) and block (`/* ... */`) comments, bracket matching, auto-closing,
  and indentation rules.

This extension is syntax highlighting only. Diagnostics come from `axiom check`
and `axiom lint`; there is no language server yet.

## Try it

Open this folder (`editors/vscode`) in VS Code and press **F5**. In the Extension
Development Host window, open any `.ax` file, for example
`examples/src/frontend.ax`.

## Install a packaged build

```sh
npx @vscode/vsce package
code --install-extension axiom-language-0.1.0.vsix
```

The extension activates on `.ax` files and does not run any code.

## Validate the grammar

```sh
node test/validate.mjs
```

This parses the extension JSON and compiles every TextMate regex.
