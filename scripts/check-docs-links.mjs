#!/usr/bin/env node
// Check relative Markdown links and Liquid relative_url targets in the
// GitHub Pages entry pages.
import { readFileSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const root = resolve("docs");
const files = [
  "index.md",
  "quickstart.md",
  "language-tour.md",
  "examples.md",
  "cli.md",
  "server.md",
  "remaining-phases.md",
  "github-pages.md",
];
let missing = 0;
let checked = 0;

const sourceForUrl = (url) => {
  if (url === "/" || url === "") {
    return "index.md";
  }
  if (url.endsWith(".html")) {
    return url.slice(1, -".html".length) + ".md";
  }
  return url.slice(1);
};

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
      console.error(`missing markdown link: ${file} -> ${target}`);
      missing += 1;
    }
  }

  for (const match of text.matchAll(/\{\{\s*'([^']+)'\s*\|\s*relative_url\s*\}\}/g)) {
    const target = match[1];
    if (/^(https?:|mailto:)/.test(target)) {
      continue;
    }
    checked += 1;
    const source = sourceForUrl(target);
    if (!existsSync(join(root, source))) {
      console.error(`missing relative_url target: ${file} -> ${target} (${source})`);
      missing += 1;
    }
  }
}

if (missing > 0) {
  process.exit(1);
}
console.log(`ok: ${checked} links in ${files.length} pages`);
