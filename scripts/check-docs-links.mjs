#!/usr/bin/env node
// Check relative Markdown links in the GitHub Pages entry pages.
import { readFileSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const root = resolve("docs");
const files = ["index.md", "github-pages.md"];
let missing = 0;
let checked = 0;

for (const file of files) {
  const path = join(root, file);
  const text = readFileSync(path, "utf8");
  for (const match of text.matchAll(/\]\(([^)]+)\)/g)) {
    const target = match[1].split("#")[0].trim();
    if (!target || /^(https?:|mailto:|#)/.test(target)) {
      continue;
    }
    checked += 1;
    if (!existsSync(resolve(dirname(path), target))) {
      console.error(`missing: ${file} -> ${target}`);
      missing += 1;
    }
  }
}

if (missing > 0) {
  process.exit(1);
}
console.log(`ok: ${checked} relative links in docs/index.md and docs/github-pages.md`);
