#!/usr/bin/env node
// Trusted-host-shaped smoke harness for an Axiom browser artifact.
//
// This is an integration test, not a real browser test: it reproduces the
// checks the browser host must perform (header, ABI/IR versions, content hash,
// imports, embedded IR) and instantiates the module in Node's WebAssembly.
// The true headless-browser smoke test (render a UiTree, forward an event) is
// the outstanding Phase 4 item tracked in docs/implementation-ledger.md.

import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";

const ABI_VERSION = 1;
const SUPPORTED_IMPORTS = new Set([
  "axiom.ui_render",
  "axiom.ui_event",
  "axiom.api_call",
  "axiom.storage_get",
  "axiom.storage_set",
  "axiom.navigate",
]);

function fail(message) {
  console.error(`FAIL: ${message}`);
  process.exit(1);
}

const [wasmPath, manifestPath] = process.argv.slice(2);
if (!wasmPath || !manifestPath) {
  console.error("usage: host-smoke.mjs <artifact.wasm> <manifest.json>");
  process.exit(2);
}

const wasm = readFileSync(wasmPath);
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));

if (wasm.length < 8 || wasm.toString("latin1", 0, 4) !== "\0asm") {
  fail("bad WASM magic number");
}
if (wasm.readUInt32LE(4) !== 1) {
  fail("unsupported WASM binary version");
}

const hash = createHash("sha256").update(wasm).digest("hex");
if (hash !== manifest.content_hash) {
  fail(`content hash mismatch: ${hash} != ${manifest.content_hash}`);
}
if (wasm.length !== manifest.byte_len) {
  fail(`byte_len mismatch: ${wasm.length} != ${manifest.byte_len}`);
}
if (manifest.abi_version !== ABI_VERSION) {
  fail(`host ABI is v${ABI_VERSION}, artifact needs v${manifest.abi_version}`);
}
if (manifest.target !== "wasm_browser") {
  fail(`unexpected target ${manifest.target}`);
}

const module = new WebAssembly.Module(wasm);
for (const imp of WebAssembly.Module.imports(module)) {
  if (!SUPPORTED_IMPORTS.has(`${imp.module}.${imp.name}`)) {
    fail(`unsupported import ${imp.module}.${imp.name}`);
  }
}

const instance = await WebAssembly.instantiate(module, {});
const memory = instance.exports.memory;
if (!(memory instanceof WebAssembly.Memory)) {
  fail("missing exported memory");
}
const irLen = instance.exports.ir_len?.value;
if (typeof irLen !== "number" || irLen <= 0) {
  fail("missing or empty ir_len export");
}

const bytes = new Uint8Array(memory.buffer, 0, irLen);
let ir;
try {
  ir = JSON.parse(new TextDecoder().decode(bytes));
} catch (error) {
  fail(`embedded IR is not valid JSON: ${error.message}`);
}
if (ir.ir_version !== manifest.ir_version) {
  fail(`IR version mismatch: ${ir.ir_version} != ${manifest.ir_version}`);
}
if (ir.target !== "wasm_browser") {
  fail(`IR target mismatch: ${ir.target}`);
}
if (!Array.isArray(ir.modules)) {
  fail("IR has no modules array");
}

console.log(
  `ok: sha256 ${manifest.content_hash.slice(0, 12)}… abi v${manifest.abi_version} ` +
    `ir v${manifest.ir_version} ${irLen} bytes, ${ir.modules.length} module(s)`,
);
