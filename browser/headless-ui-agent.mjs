#!/usr/bin/env node
// End-to-end browser test for the UI-agent demo.
//
// Assumes the demo server is running:
//   cargo run -p axiom-server --example ui_agent_demo
//
// Then:
//   node browser/headless-ui-agent.mjs [url]
//
// It loads the control page, asks for the dark UI, waits for the iframe to
// reload the newly published Axiom frontend, and asserts the rendered heading
// and theme changed. Chrome/Edge is launched through the DevTools protocol.

import { execFile, spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const url = process.argv[2] || "http://127.0.0.1:8099/";
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
  console.error("no Chrome/Edge found; set AXIOM_CHROME");
  process.exit(2);
}

const sleep = (milliseconds) => new Promise((resolveSleep) => setTimeout(resolveSleep, milliseconds));

class Cdp {
  constructor(socketUrl) {
    this.nextId = 1;
    this.pending = new Map();
    this.socket = new WebSocket(socketUrl);
    this.socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      if (message.error) pending.reject(new Error(message.error.message));
      else pending.resolve(message.result);
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
    this.socket.send(JSON.stringify({ id, method, params, sessionId }));
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
    throw new Error(result.exceptionDetails.exception?.description ?? "evaluation failed");
  }
  return result.result?.value;
}

async function waitFor(cdp, sessionId, expression, predicate, timeout = 20000) {
  const deadline = Date.now() + timeout;
  let last;
  while (Date.now() < deadline) {
    last = await evaluate(cdp, sessionId, expression);
    if (predicate(last)) return last;
    await sleep(120);
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

const userDataDir = await mkdtemp(join(tmpdir(), "axiom-ui-agent-"));
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
      url
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

  await waitFor(
    cdp,
    sessionId,
    "document.querySelector('iframe')?.contentDocument?.body?.innerText ?? null",
    (text) => typeof text === "string" && text.includes("Axiom TeamBoard")
  );

  await evaluate(
    cdp,
    sessionId,
    "document.getElementById('theme').value = 'dark'; document.getElementById('update').click(); true"
  );

  const frameText = await waitFor(
    cdp,
    sessionId,
    "document.querySelector('iframe')?.contentDocument?.body?.innerText ?? null",
    (text) =>
      typeof text === "string" &&
      text.includes("Axiom TeamBoard · Dark") &&
      text.includes("Theme: dark")
  );
  const controlStatus = await evaluate(cdp, sessionId, "document.getElementById('status').innerText");
  if (!controlStatus.includes("Published")) {
    throw new Error(`unexpected control status: ${controlStatus}`);
  }
  if (!frameText.includes("Completed tasks")) {
    throw new Error(`missing metric in iframe: ${frameText}`);
  }

  console.log("ok: backend agent published a new frontend and the browser UI changed on demand");
} finally {
  if (cdp) cdp.close();
  await stopBrowser(child);
  for (let attempt = 0; attempt < 5; attempt += 1) {
    try {
      await rm(userDataDir, { recursive: true, force: true });
      break;
    } catch {
      await sleep(250);
    }
  }
}
