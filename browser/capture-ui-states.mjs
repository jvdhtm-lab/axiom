#!/usr/bin/env node
// Capture the UI demo control page and each rendered state with Chrome.
//
// Assumes a demo server is running:
//   powershell -File scripts/run-ui-demo.ps1 -Agent template -Port 8100 -Detach
//   node browser/capture-ui-states.mjs --url http://127.0.0.1:8100/ --out docs/assets/screenshots
//
// For an OpenCode-agent state:
//   node browser/capture-ui-states.mjs --url http://127.0.0.1:8099/ \
//     --out docs/assets/screenshots --request "Create a dark analytics dashboard..." \
//     --name agent

import { execFile, spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const args = process.argv.slice(2);
let url = "http://127.0.0.1:8100/";
let outDir = "docs/assets/screenshots";
let request = null;
let name = "template";
for (let index = 0; index < args.length; index += 1) {
  if (args[index] === "--url") url = args[++index];
  else if (args[index] === "--out") outDir = args[++index];
  else if (args[index] === "--request") request = args[++index];
  else if (args[index] === "--name") name = args[++index];
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

async function waitFor(cdp, sessionId, expression, predicate, timeout = 120000) {
  const deadline = Date.now() + timeout;
  let last;
  while (Date.now() < deadline) {
    last = await evaluate(cdp, sessionId, expression);
    if (predicate(last)) return last;
    await sleep(150);
  }
  throw new Error(`timed out waiting for ${expression}; last value: ${JSON.stringify(last)}`);
}

async function capture(cdp, sessionId, path, clip) {
  const params = { format: "png", captureBeyondViewport: true };
  if (clip) params.clip = { ...clip, scale: 1 };
  const result = await cdp.send("Page.captureScreenshot", params, sessionId);
  await writeFile(path, Buffer.from(result.data, "base64"));
  console.log(`wrote ${path}`);
}

async function frameClip(cdp, sessionId) {
  return evaluate(
    cdp,
    sessionId,
    `(() => {
      const frame = document.querySelector("iframe");
      if (!frame) return null;
      frame.scrollIntoView({ block: "center", inline: "nearest" });
      const rect = frame.getBoundingClientRect();
      return {
        x: rect.x + window.scrollX,
        y: rect.y + window.scrollY,
        width: rect.width,
        height: rect.height
      };
    })()`
  );
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

async function captureState(cdp, sessionId, prefix) {
  await evaluate(cdp, sessionId, "window.scrollTo(0, 0); true");
  await sleep(250);
  await capture(cdp, sessionId, join(outDir, `${prefix}.png`));
  const clip = await frameClip(cdp, sessionId);
  await sleep(150);
  await capture(cdp, sessionId, join(outDir, `${prefix}-app.png`), clip);
}

await mkdir(outDir, { recursive: true });
const userDataDir = await mkdtemp(join(tmpdir(), "axiom-capture-"));
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
      "--window-size=1280,900",
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
  await cdp.send("Page.enable", {}, sessionId);
  await cdp.send("Runtime.enable", {}, sessionId);
  await cdp.send(
    "Emulation.setDeviceMetricsOverride",
    { width: 1280, height: 900, deviceScaleFactor: 1, mobile: false },
    sessionId
  );

  await waitFor(
    cdp,
    sessionId,
    "document.querySelector('iframe')?.contentDocument?.body?.innerText ?? null",
    (text) => typeof text === "string" && text.includes("Axiom")
  );

  if (request) {
    await evaluate(
      cdp,
      sessionId,
      `document.getElementById('request').value = ${JSON.stringify(request)}; document.getElementById('update').click(); true`
    );
    await waitFor(
      cdp,
      sessionId,
      "document.getElementById('status').innerText",
      (text) => typeof text === "string" && (text.includes("Published") || text.includes("Failed")),
      300000
    );
    await waitFor(
      cdp,
      sessionId,
      "document.querySelector('iframe')?.contentDocument?.body?.innerText ?? null",
      (text) => typeof text === "string" && text.includes("Axiom")
    );
    await sleep(500);
    await captureState(cdp, sessionId, name);
  } else {
    await captureState(cdp, sessionId, `${name}-light`);
    for (const theme of ["dark", "minimal"]) {
      await evaluate(
        cdp,
        sessionId,
        `document.getElementById('theme').value = ${JSON.stringify(theme)}; document.getElementById('update').click(); true`
      );
      await waitFor(
        cdp,
        sessionId,
        "document.getElementById('status').innerText",
        (text) => typeof text === "string" && text.includes("Published")
      );
      await waitFor(
        cdp,
        sessionId,
        "document.querySelector('iframe')?.contentDocument?.body?.innerText ?? null",
        (text) => typeof text === "string" && text.includes(`Theme: ${theme}`)
      );
      await sleep(500);
      await captureState(cdp, sessionId, `${name}-${theme}`);
    }
  }

  console.log(`ok: captured UI states in ${outDir}`);
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
