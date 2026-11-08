import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => JSON.parse(readFileSync(join(root, path), "utf8"));
const assert = (condition, message) => {
  if (!condition) {
    throw new Error(message);
  }
};

const pkg = read("package.json");
const language = read("language-configuration.json");
const grammar = read("syntaxes/axiom.tmLanguage.json");

const languageContribution = pkg.contributes.languages[0];
assert(languageContribution.id === "axiom", "language id must be axiom");
assert(
  languageContribution.extensions.includes(".ax"),
  "language must register the .ax extension",
);
assert(
  languageContribution.configuration === "./language-configuration.json",
  "language must reference its configuration",
);

const grammarContribution = pkg.contributes.grammars[0];
assert(grammarContribution.language === "axiom", "grammar language must be axiom");
assert(grammarContribution.scopeName === "source.axiom", "grammar scope must be source.axiom");
assert(grammar.scopeName === grammarContribution.scopeName, "scope names must agree");

assert(language.comments.lineComment === "//", "line comments must be //");
assert(
  language.comments.blockComment?.[0] === "/*" && language.comments.blockComment?.[1] === "*/",
  "block comments must be /* */",
);

const regexKeys = new Set(["match", "begin", "end"]);
let regexCount = 0;
const scopes = new Set();
const walk = (node) => {
  if (!node || typeof node !== "object") {
    return;
  }
  for (const [key, value] of Object.entries(node)) {
    if (regexKeys.has(key) && typeof value === "string") {
      new RegExp(value);
      regexCount += 1;
    }
    if (key === "name" && typeof value === "string") {
      scopes.add(value);
    }
    walk(value);
  }
};
walk(grammar);

assert(regexCount > 20, "grammar must contain the expected rule set");
for (const scope of [
  "keyword.control.axiom",
  "storage.type.axiom",
  "entity.name.function.axiom",
  "entity.name.agent.axiom",
  "entity.name.workspace.axiom",
  "support.type.effect.axiom",
  "constant.numeric.duration.axiom",
  "comment.block.axiom",
]) {
  assert(scopes.has(scope), `missing scope ${scope}`);
}

console.log(`ok: ${regexCount} regexes, ${scopes.size} scopes, .ax registered`);
