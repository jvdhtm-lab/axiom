#!/usr/bin/env node
// Validate the vendored Prism core and the custom Axiom grammar.
import { readFileSync } from "node:fs";
import vm from "node:vm";

const core = readFileSync("docs/assets/vendor/prism-core.min.js", "utf8");
const custom = readFileSync("docs/assets/prism-axiom.js", "utf8");

const sandbox = {
  module: { exports: {} },
  exports: {},
  console,
  document: {
    readyState: "loading",
    currentScript: null,
    addEventListener() {}
  }
};
sandbox.window = sandbox;
sandbox.globalThis = sandbox;
vm.createContext(sandbox);
vm.runInContext(core, sandbox);

const Prism = sandbox.Prism || sandbox.module.exports;
if (!Prism || !Prism.languages) {
  throw new Error("Prism core did not initialize");
}
sandbox.Prism = Prism;
vm.runInContext(custom, sandbox);

const grammar = Prism.languages.axiom;
if (!grammar) {
  throw new Error("axiom grammar was not registered");
}

const sample = `module demo.agents;

// A bounded frontend agent.
export type FrontendChange = { request: string; };

export workspace frontend_edit {
  isolation: copy_on_write;
  limits: { max_files: 200 };
}

export agent FrontendDesigner(input: FrontendChange) -> FrontendPatch {
  model: "coding";
  workspace: frontend_edit;
}

export async fn update(
  input: FrontendChange
) -> Result<FrontendRelease, AppError>
  effects [model, db.read, api.call] {
  let candidate = await agents.iterate(
    agent: FrontendDesigner,
    max_attempts: 3,
    timeout: 120s
  );
  return Ok(candidate);
}`;

const highlighted = Prism.highlight(sample, grammar, "axiom");
for (const token of [
  "token comment",
  "token keyword",
  "token builtin",
  "token class-name",
  "token namespace",
  "token string",
  "token number",
  "token property",
  "token punctuation"
]) {
  if (!highlighted.includes(token)) {
    throw new Error(`missing highlighted token: ${token}`);
  }
}

console.log(`ok: axiom grammar (${Object.keys(grammar).length} token rules, ${highlighted.length} bytes)`);
