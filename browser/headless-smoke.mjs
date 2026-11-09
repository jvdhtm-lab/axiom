#!/usr/bin/env node
// Real headless-browser smoke test for the Axiom host.
//
// Usage:
//   node browser/headless-smoke.mjs <app.wasm> <app.manifest.json>
//
// Serves `browser/index.html` + `host.js` and the artifact on localhost, launches
// Chrome/Edge headless with the DevTools protocol, waits for the host to render,
// clicks the first button through the DOM, and asserts the state/DOM changed.
// Set AXIOM_CHROME to override the browser path.

import { execFile, spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const browserDir = dirname(fileURLToPath(import.meta.url));
const [wasmArg, manifestArg] = process.argv.slice(2);
if (!wasmArg || !manifestArg) {
  console.error("usage: headless-smoke.mjs <app.wasm> <app.manifest.json>");
  process.exit(2);
}

const wasmPath = resolve(wasmArg);
const manifestPath = resolve(manifestArg);
if (!existsSync(wasmPath) || !existsSync(manifestPath)) {
  console.error("artifact or manifest does not exist");
  process.exit(2);
}

const candidates = [
  process.env.AXIOM_CHROME,
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
  "/usr/bin/google-chrome",
  "/usr/bin/google-chrome-stable",
  "/opt/google/chrome/chrome",
  "/usr/bin/chromium",
  "/usr/bin/chromium-browser"
].filter(Boolean);
const browser = candidates.find((path) => existsSync(path));
if (!browser) {
  console.error("no Chrome/Edge found; set AXIOM_CHROME to a headless-capable browser");
  process.exit(2);
}

const sleep = (milliseconds) => new Promise((resolveSleep) => setTimeout(resolveSleep, milliseconds));

class Cdp {
  constructor(url) {
    this.nextId = 1;
    this.pending = new Map();
    this.socket = new WebSocket(url);
    this.socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      if (message.error) {
        pending.reject(new Error(`${message.error.code}: ${message.error.message}`));
      } else {
        pending.resolve(message.result);
      }
    });
  }

  async ready() {
    await new Promise((resolveOpen, rejectOpen) => {
      this.socket.addEventListener("open", resolveOpen, { once: true });
      this.socket.addEventListener("error", rejectOpen, { once: true });
    });
  }

  send(method, params = {}, sessionId) {
    const id = this.nextId;
    this.nextId += 1;
    const message = { id, method, params };
    if (sessionId) message.sessionId = sessionId;
    this.socket.send(JSON.stringify(message));
    return new Promise((resolveSend, rejectSend) => {
      this.pending.set(id, { resolve: resolveSend, reject: rejectSend });
    });
  }

  close() {
    this.socket.close();
  }
}

async function evaluate(cdp, sessionId, expression) {
  const result = await cdp.send(
    "Runtime.evaluate",
    { expression, returnByValue: true, awaitPromise: true },
    sessionId
  );
  if (result.exceptionDetails) {
    const description = result.exceptionDetails.exception?.description ?? "evaluation failed";
    throw new Error(description);
  }
  return result.result?.value;
}

async function waitFor(cdp, sessionId, expression, predicate, timeout = 15000) {
  const deadline = Date.now() + timeout;
  let last;
  while (Date.now() < deadline) {
    last = await evaluate(cdp, sessionId, expression);
    if (predicate(last)) return last;
    await sleep(100);
  }
  throw new Error(`timed out waiting for ${expression}; last value: ${JSON.stringify(last)}`);
}

async function stopBrowser(child) {
  if (!child || child.exitCode !== null) return;
  const closed = new Promise((resolveClosed) => {
    child.once("close", resolveClosed);
    setTimeout(resolveClosed, 5000);
  });
  if (process.platform === "win32") {
    await new Promise((resolveKill) => {
      execFile("taskkill", ["/pid", String(child.pid), "/T", "/F"], () => resolveKill());
    });
  } else {
    child.kill("SIGKILL");
  }
  await closed;
}

const indexHtml = await readFile(join(browserDir, "index.html"));
const hostJs = await readFile(join(browserDir, "host.js"));
const wasm = await readFile(wasmPath);
const manifest = await readFile(manifestPath);

const server = createServer((request, response) => {
  const path = new URL(request.url, "http://localhost").pathname;
  const send = (status, type, body) => {
    response.writeHead(status, { "content-type": type, "cache-control": "no-store" });
    response.end(body);
  };
  switch (path) {
    case "/":
    case "/index.html":
      send(200, "text/html; charset=utf-8", indexHtml);
      break;
    case "/host.js":
      send(200, "text/javascript; charset=utf-8", hostJs);
      break;
    case "/app.wasm":
      send(200, "application/wasm", wasm);
      break;
    case "/app.manifest.json":
      send(200, "application/json", manifest);
      break;
    case "/favicon.ico":
      response.writeHead(204);
      response.end();
      break;
    default:
      send(404, "text/plain; charset=utf-8", "not found");
  }
});

await new Promise((resolveListen) => server.listen(0, "127.0.0.1", resolveListen));
const address = server.address();
const port = typeof address === "object" && address ? address.port : 0;
const userDataDir = await mkdtemp(join(tmpdir(), "axiom-headless-"));

let child;
let cdp;
try {
  child = spawn(
    browser,
    [
      "--headless=new",
      "--disable-gpu",
      "--no-sandbox",
      "--disable-dev-shm-usage",
      "--no-first-run",
      "--no-default-browser-check",
      "--remote-debugging-port=0",
      `--user-data-dir=${userDataDir}`,
      `http://127.0.0.1:${port}/`
    ],
    { stdio: ["ignore", "ignore", "pipe"] }
  );

  const devtools = await new Promise((resolveDevtools, rejectDevtools) => {
    let stderr = "";
    const timer = setTimeout(() => rejectDevtools(new Error("DevTools endpoint was not announced")), 15000);
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
      const match = /DevTools listening on (ws:\/\/\S+)/.exec(stderr);
      if (match) {
        clearTimeout(timer);
        resolveDevtools(match[1]);
      }
    });
    child.on("exit", (code) => {
      clearTimeout(timer);
      rejectDevtools(new Error(`browser exited before DevTools was ready (${code})`));
    });
  });

  cdp = new Cdp(devtools);
  await cdp.ready();

  const targets = await cdp.send("Target.getTargets");
  const page = targets.targetInfos.find((target) => target.type === "page");
  if (!page) throw new Error("no page target");
  const attached = await cdp.send("Target.attachToTarget", {
    targetId: page.targetId,
    flatten: true
  });
  const sessionId = attached.sessionId;
  await cdp.send("Runtime.enable", {}, sessionId);

  const status = await waitFor(
    cdp,
    sessionId,
    "document.body?.dataset?.axiomStatus ?? null",
    (value) => value === "ok" || value === "error"
  );
  if (status !== "ok") {
    const detail = await evaluate(cdp, sessionId, "document.body.innerText");
    throw new Error(`host reported an error: ${detail}`);
  }

  const initial = await waitFor(
    cdp,
    sessionId,
    "window.axiomHost?.getState?.().count ?? null",
    (value) => value === 0
  );
  if (initial !== 0) throw new Error(`unexpected initial count ${initial}`);

  await evaluate(cdp, sessionId, "document.querySelector('button')?.click(); true");
  await waitFor(
    cdp,
    sessionId,
    "window.axiomHost?.getState?.().count ?? null",
    (value) => value === 1
  );

  const dom = await evaluate(cdp, sessionId, "document.documentElement.outerHTML");
  const text = await evaluate(cdp, sessionId, "document.body.innerText");
  for (const [label, expected] of [
    ["rendered heading", "Axiom browser smoke"],
    ["rendered count", "Count: 1"],
    ["event button", "Increment"]
  ]) {
    if (!text.includes(expected)) {
      throw new Error(`missing ${label}: ${expected}\n${text}`);
    }
  }
  if (!dom.includes('data-axiom-status="ok"')) {
    throw new Error("host did not report a successful status");
  }

  console.log("ok: headless browser rendered UiTree and forwarded a button event");
} finally {
  if (cdp) cdp.close();
  await stopBrowser(child);
  await new Promise((resolveClose) => server.close(resolveClose));
  for (let attempt = 0; attempt < 5; attempt += 1) {
    try {
      await rm(userDataDir, { recursive: true, force: true });
      break;
    } catch (error) {
      if (attempt === 4) {
        console.warn(`warning: could not remove ${userDataDir}: ${error.message}`);
      } else {
        await sleep(250);
      }
    }
  }
}
