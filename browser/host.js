// Trusted Axiom browser host.
//
// This host verifies a content-addressed WASM artifact, extracts the checked IR
// embedded in linear memory, interprets the portable function subset, renders a
// typed UiTree with DOM APIs, and forwards button events back to
// `handle_event`. It contains no application-specific Axiom logic.
//
// It is deliberately not a general JavaScript or HTML runtime: unknown UiTree
// nodes fail closed and all text is applied through `textContent`.

const ABI_VERSION = 1;
const IR_VERSION = 1;
const MAX_UI_NODES = 10000;
const SUPPORTED_IMPORTS = new Set([
  "axiom.ui_render",
  "axiom.ui_event",
  "axiom.api_call",
  "axiom.storage_get",
  "axiom.storage_set",
  "axiom.navigate"
]);

const UNIT = Symbol("unit");
const appElement = document.getElementById("app");
const statusElement = document.getElementById("status");

class HostError extends Error {}

function fail(message) {
  throw new HostError(message);
}

function setStatus(kind, message) {
  document.body.dataset.axiomStatus = kind;
  if (statusElement) {
    statusElement.textContent = message;
    statusElement.className = `status status-${kind}`;
  }
}

async function sha256Hex(buffer) {
  const digest = await crypto.subtle.digest("SHA-256", buffer);
  return Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

async function loadArtifact() {
  const manifestResponse = await fetch("./app.manifest.json", { cache: "no-store" });
  if (!manifestResponse.ok) {
    fail(`cannot load app.manifest.json (${manifestResponse.status})`);
  }
  const manifest = await manifestResponse.json();

  const wasmResponse = await fetch("./app.wasm", { cache: "no-store" });
  if (!wasmResponse.ok) {
    fail(`cannot load app.wasm (${wasmResponse.status})`);
  }
  const bytes = await wasmResponse.arrayBuffer();

  if (manifest.abi_version !== ABI_VERSION) {
    fail(`host ABI is v${ABI_VERSION}, artifact needs v${manifest.abi_version}`);
  }
  if (manifest.ir_version !== IR_VERSION) {
    fail(`host IR is v${IR_VERSION}, artifact needs v${manifest.ir_version}`);
  }
  if (manifest.target !== "wasm_browser") {
    fail(`unexpected artifact target ${manifest.target}`);
  }
  if (bytes.byteLength !== manifest.byte_len) {
    fail(`artifact length ${bytes.byteLength} does not match manifest ${manifest.byte_len}`);
  }
  const hash = await sha256Hex(bytes);
  if (hash !== manifest.content_hash) {
    fail(`artifact hash ${hash} does not match manifest ${manifest.content_hash}`);
  }

  const module = new WebAssembly.Module(bytes);
  for (const imported of WebAssembly.Module.imports(module)) {
    const name = `${imported.module}.${imported.name}`;
    if (!SUPPORTED_IMPORTS.has(name)) {
      fail(`unsupported host import ${name}`);
    }
    fail(`host imports are not implemented yet: ${name}`);
  }

  const instance = new WebAssembly.Instance(module, {});
  const memory = instance.exports.memory;
  if (!(memory instanceof WebAssembly.Memory)) {
    fail("artifact does not export memory");
  }
  const irLength = instance.exports.ir_len?.value;
  if (typeof irLength !== "number" || irLength <= 0) {
    fail("artifact does not export a non-empty ir_len");
  }
  const irBytes = new Uint8Array(memory.buffer, 0, irLength);
  const ir = JSON.parse(new TextDecoder().decode(irBytes));
  if (ir.ir_version !== IR_VERSION || ir.target !== "wasm_browser") {
    fail("embedded IR version or target does not match the manifest");
  }
  return { manifest, ir };
}

// ---------------------------------------------------------------------------
// Value model and display
// ---------------------------------------------------------------------------

function isVariant(value, name) {
  return value && typeof value === "object" && value.$variant === name;
}

function displayValue(value) {
  if (value === UNIT) return "()";
  if (typeof value === "string") return JSON.stringify(value);
  if (typeof value === "number") return String(value);
  if (typeof value === "boolean") return value ? "true" : "false";
  if (Array.isArray(value)) return `[${value.map(displayValue).join(", ")}]`;
  if (value && typeof value === "object") {
    if (value.$variant) {
      const args = value.$args || [];
      return args.length === 0 ? value.$variant : `${value.$variant}(${args.map(displayValue).join(", ")})`;
    }
    if (value.$duration !== undefined) return `${value.$duration}ms`;
    if (value.$timestamp !== undefined) return String(value.$timestamp);
    if (value.$bytes !== undefined) return `<${value.$bytes} bytes>`;
    return `{ ${Object.entries(value)
      .map(([name, field]) => `${name}: ${displayValue(field)}`)
      .join(", ")} }`;
  }
  return String(value);
}

function deepEqual(a, b) {
  if (a === b) return true;
  if (typeof a !== typeof b) return false;
  if (Array.isArray(a) || Array.isArray(b)) {
    if (!Array.isArray(a) || !Array.isArray(b) || a.length !== b.length) return false;
    return a.every((item, index) => deepEqual(item, b[index]));
  }
  if (a && b && typeof a === "object") {
    const aKeys = Object.keys(a);
    const bKeys = Object.keys(b);
    if (aKeys.length !== bKeys.length) return false;
    return aKeys.every((key) => bKeys.includes(key) && deepEqual(a[key], b[key]));
  }
  return false;
}

// ---------------------------------------------------------------------------
// IR interpreter
// ---------------------------------------------------------------------------

class ReturnSignal {
  constructor(value) {
    this.value = value;
  }
}

class Interpreter {
  constructor(ir) {
    this.functions = new Map();
    for (const module of ir.modules) {
      for (const fn of module.functions) {
        this.functions.set(fn.name, fn);
      }
    }
  }

  call(name, args) {
    const fn = this.functions.get(name);
    if (!fn) fail(`IR has no function \`${name}\``);
    if (fn.params.length !== args.length) {
      fail(`\`${name}\` expects ${fn.params.length} argument(s), found ${args.length}`);
    }
    const frame = {};
    fn.params.forEach((param, index) => {
      frame[param.name] = args[index];
    });
    try {
      return this.evalBlock(fn.body, [frame]).value;
    } catch (error) {
      if (error instanceof ReturnSignal) return error.value;
      throw error;
    }
  }

  evalBlock(block, env) {
    let value = UNIT;
    for (const wrapper of block.stmts) {
      value = this.evalStmt(wrapper.stmt, env);
    }
    return { value };
  }

  evalStmt(stmt, env) {
    switch (stmt.kind) {
      case "let": {
        env[env.length - 1][stmt.name] = this.evalExpr(stmt.value, env);
        return UNIT;
      }
      case "return": {
        const value = stmt.value == null ? UNIT : this.evalExpr(stmt.value, env);
        throw new ReturnSignal(value);
      }
      case "expr":
        return this.evalExpr(stmt.value, env);
      case "for": {
        const iterable = this.evalExpr(stmt.iter, env);
        if (!Array.isArray(iterable)) fail("`for` expects a list");
        for (const item of iterable) {
          const frame = {};
          frame[stmt.var] = item;
          env.push(frame);
          try {
            this.evalBlock(stmt.body, env);
          } finally {
            env.pop();
          }
        }
        return UNIT;
      }
      default:
        fail(`unsupported statement kind \`${stmt.kind}\``);
    }
  }

  lookup(name, env) {
    for (let index = env.length - 1; index >= 0; index -= 1) {
      if (Object.prototype.hasOwnProperty.call(env[index], name)) {
        return env[index][name];
      }
    }
    fail(`unbound local \`${name}\``);
  }

  evalExpr(wrapper, env) {
    const expr = wrapper.expr ?? wrapper;
    switch (expr.kind) {
      case "unit":
        return UNIT;
      case "bool":
      case "int":
      case "float":
      case "string":
        return expr.value;
      case "bytes":
        return { $bytes: expr.value };
      case "duration":
        return { $duration: expr.millis };
      case "local":
        return this.lookup(expr.name, env);
      case "function":
        return { $function: expr.name };
      case "namespace":
        return { $namespace: expr.name };
      case "list":
        return expr.items.map((item) => this.evalExpr(item, env));
      case "record": {
        const record = {};
        for (const field of expr.fields) {
          record[field.name] = this.evalExpr(field.value, env);
        }
        return record;
      }
      case "variant":
        return {
          $variant: expr.name,
          $args: expr.args.map((arg) => this.evalExpr(arg, env))
        };
      case "field": {
        const base = this.evalExpr(expr.base, env);
        if (base && typeof base === "object" && base.$namespace) {
          return { $namespace: base.$namespace, $method: expr.name };
        }
        if (!base || typeof base !== "object" || !(expr.name in base)) {
          fail(`no field \`${expr.name}\` on ${displayValue(base)}`);
        }
        return base[expr.name];
      }
      case "index": {
        const base = this.evalExpr(expr.base, env);
        const index = this.evalExpr(expr.index, env);
        if (Array.isArray(base) && Number.isInteger(index)) return base[index];
        if (base && typeof base === "object" && typeof index === "string") return base[index];
        fail(`cannot index ${displayValue(base)}`);
        break;
      }
      case "call": {
        const args = expr.args.map((arg) => this.evalExpr(arg.value, env));
        const callee = expr.callee.expr ?? expr.callee;
        if (callee.kind === "function") {
          return this.callNamed(callee.name, args);
        }
        if (callee.kind === "field") {
          const baseExpr = callee.base.expr ?? callee.base;
          if (baseExpr.kind === "namespace") {
            return this.callStdlib(baseExpr.name, callee.name, args);
          }
          return this.callMethod(this.evalExpr(callee.base, env), callee.name, args);
        }
        fail("call target is not callable");
        break;
      }
      case "unary": {
        const operand = this.evalExpr(expr.operand, env);
        if (expr.op === "not" && typeof operand === "boolean") return !operand;
        if (expr.op === "neg" && typeof operand === "number") return -operand;
        fail(`cannot apply \`${expr.op}\``);
        break;
      }
      case "binary":
        return this.binary(
          expr.op,
          this.evalExpr(expr.lhs, env),
          this.evalExpr(expr.rhs, env)
        );
      case "await":
        return this.evalExpr(expr.value, env);
      case "if": {
        const condition = this.evalExpr(expr.cond, env);
        if (typeof condition !== "boolean") fail("`if` condition must be bool");
        if (condition) return this.evalBlock(expr.then, env).value;
        return expr.else == null ? UNIT : this.evalExpr(expr.else, env);
      }
      case "match":
        return this.evalMatch(expr, env);
      case "block":
        return this.evalBlock(expr.value, env).value;
      default:
        fail(`unsupported expression kind \`${expr.kind}\``);
    }
  }

  callNamed(name, args) {
    if (name === "Some" || name === "Ok" || name === "Err") {
      return { $variant: name, $args: args };
    }
    if (name === "capability") return args[0] ?? UNIT;
    return this.call(name, args);
  }

  callStdlib(namespace, method, args) {
    if (namespace === "ui") {
      return { $node: method, args };
    }
    if (namespace === "checks") {
      return { $variant: "Validator", $args: [`checks.${method}`] };
    }
    fail(`\`${namespace}.${method}\` needs a host ABI that is not implemented`);
  }

  callMethod(receiver, method, args) {
    switch (method) {
      case "to_string":
        return displayValue(receiver);
      case "len":
        if (Array.isArray(receiver)) return receiver.length;
        if (typeof receiver === "string") return receiver.length;
        if (receiver && typeof receiver === "object") return Object.keys(receiver).length;
        fail("`len` is not defined for this value");
        break;
      case "unwrap_or":
        if (isVariant(receiver, "Some")) return receiver.$args[0];
        if (isVariant(receiver, "None")) return args[0] ?? UNIT;
        fail("`unwrap_or` needs an Option");
        break;
      case "unwrap":
        if (isVariant(receiver, "Some")) return receiver.$args[0];
        fail("called `unwrap` on None");
        break;
      case "is_some":
        return isVariant(receiver, "Some");
      case "is_none":
        return isVariant(receiver, "None");
      case "is_ok":
        return isVariant(receiver, "Ok");
      case "is_err":
        return isVariant(receiver, "Err");
      default:
        fail(`unknown method \`${method}\``);
    }
  }

  binary(op, lhs, rhs) {
    switch (op) {
      case "+":
        if (typeof lhs === "number" && typeof rhs === "number") return lhs + rhs;
        if (typeof lhs === "string" && typeof rhs === "string") return lhs + rhs;
        if (Array.isArray(lhs) && Array.isArray(rhs)) return lhs.concat(rhs);
        fail("cannot add these values");
        break;
      case "-":
        if (typeof lhs === "number" && typeof rhs === "number") return lhs - rhs;
        fail("cannot subtract these values");
        break;
      case "*":
        if (typeof lhs === "number" && typeof rhs === "number") return lhs * rhs;
        fail("cannot multiply these values");
        break;
      case "/":
        if (typeof lhs === "number" && typeof rhs === "number") {
          if (rhs === 0) fail("division by zero");
          return Number.isInteger(lhs) && Number.isInteger(rhs) ? Math.trunc(lhs / rhs) : lhs / rhs;
        }
        fail("cannot divide these values");
        break;
      case "%":
        if (typeof lhs === "number" && typeof rhs === "number") {
          if (rhs === 0) fail("remainder by zero");
          return lhs % rhs;
        }
        fail("cannot take a remainder of these values");
        break;
      case "<":
        return lhs < rhs;
      case "<=":
        return lhs <= rhs;
      case ">":
        return lhs > rhs;
      case ">=":
        return lhs >= rhs;
      case "==":
        return deepEqual(lhs, rhs);
      case "!=":
        return !deepEqual(lhs, rhs);
      case "and":
        if (typeof lhs === "boolean" && typeof rhs === "boolean") return lhs && rhs;
        fail("`and` needs bool operands");
        break;
      case "or":
        if (typeof lhs === "boolean" && typeof rhs === "boolean") return lhs || rhs;
        fail("`or` needs bool operands");
        break;
      default:
        fail(`unsupported binary operator \`${op}\``);
    }
  }

  evalMatch(expr, env) {
    const value = this.evalExpr(expr.scrutinee, env);
    for (const arm of expr.arms) {
      const frame = this.bindPattern(arm.pattern, value);
      if (!frame) continue;
      env.push(frame);
      try {
        return this.evalBlock(arm.body, env).value;
      } finally {
        env.pop();
      }
    }
    fail(`non-exhaustive match for ${displayValue(value)}`);
  }

  bindPattern(pattern, value) {
    switch (pattern.kind) {
      case "wildcard":
        return {};
      case "binding":
        return { [pattern.name]: value };
      case "int":
      case "string":
      case "bool":
        return pattern.value === value ? {} : null;
      case "variant": {
        if (!isVariant(value, pattern.name)) return null;
        const frame = {};
        if (pattern.inner) {
          const args = value.$args || [];
          for (let index = 0; index < pattern.inner.length; index += 1) {
            const bound = this.bindPattern(pattern.inner[index], args[index]);
            if (!bound) return null;
            Object.assign(frame, bound);
          }
        }
        return frame;
      }
      default:
        fail(`unsupported pattern kind \`${pattern.kind}\``);
    }
  }
}

// ---------------------------------------------------------------------------
// UiTree renderer
// ---------------------------------------------------------------------------

function scalar(value) {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return displayValue(value);
}

function renderUiTree(value, context) {
  context.nodes += 1;
  if (context.nodes > MAX_UI_NODES) fail(`UI node limit ${MAX_UI_NODES} exceeded`);
  if (!value || typeof value !== "object" || typeof value.$node !== "string") {
    const text = document.createElement("span");
    text.textContent = displayValue(value);
    return text;
  }
  const args = Array.isArray(value.args) ? value.args : [];
  switch (value.$node) {
    case "column":
    case "row": {
      const container = document.createElement("div");
      container.className = value.$node === "column" ? "ax-column" : "ax-row";
      const children = args[0];
      if (!Array.isArray(children)) fail(`ui.${value.$node} expects a list of children`);
      for (const child of children) container.appendChild(renderUiTree(child, context));
      return container;
    }
    case "heading": {
      const heading = document.createElement("h1");
      heading.textContent = scalar(args[0]);
      return heading;
    }
    case "text": {
      const paragraph = document.createElement("p");
      paragraph.textContent = scalar(args[0]);
      return paragraph;
    }
    case "metric": {
      const card = document.createElement("div");
      card.className = "ax-metric";
      const label = document.createElement("span");
      label.className = "ax-metric-label";
      label.textContent = scalar(args[0]);
      const valueElement = document.createElement("strong");
      valueElement.className = "ax-metric-value";
      valueElement.textContent = scalar(args[1]);
      card.append(label, valueElement);
      return card;
    }
    case "button": {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = scalar(args[0]);
      button.dataset.axiomEvent = JSON.stringify(args[1] ?? null);
      button.addEventListener("click", () => context.handleEvent(args[1] ?? UNIT));
      return button;
    }
    default:
      fail(`unsupported UiTree node \`${value.$node}\``);
  }
}

// ---------------------------------------------------------------------------
// Host lifecycle
// ---------------------------------------------------------------------------

async function main() {
  setStatus("loading", "Loading Axiom artifact…");
  const { manifest, ir } = await loadArtifact();
  const interpreter = new Interpreter(ir);
  for (const entry of ["init", "render", "handle_event"]) {
    if (!interpreter.functions.has(entry)) {
      fail(`frontend entry point \`${entry}\` is missing from the IR`);
    }
  }

  let state = interpreter.call("init", [0]);

  const context = {
    nodes: 0,
    handleEvent(eventValue) {
      try {
        const result = interpreter.call("handle_event", [state, eventValue]);
        if (!isVariant(result, "Ok")) {
          fail(`handle_event returned ${displayValue(result)}; expected Ok(state)`);
        }
        state = result.$args[0];
        render();
      } catch (error) {
        showError(error);
      }
    }
  };

  function render() {
    context.nodes = 0;
    const tree = interpreter.call("render", [state]);
    appElement.replaceChildren(renderUiTree(tree, context));
    window.axiomHost.state = state;
  }

  function showError(error) {
    const message = error instanceof Error ? error.message : String(error);
    setStatus("error", `Axiom host error: ${message}`);
    const paragraph = document.createElement("p");
    paragraph.className = "ax-error";
    paragraph.textContent = message;
    appElement.replaceChildren(paragraph);
    console.error(error);
  }

  window.axiomHost = {
    manifest,
    state,
    render,
    getState: () => state,
    clickFirstButton() {
      appElement.querySelector("button")?.click();
    }
  };

  render();
  setStatus("ok", `Axiom host ready · abi v${manifest.abi_version} · ir v${manifest.ir_version}`);
}

main().catch((error) => {
  const message = error instanceof Error ? error.message : String(error);
  setStatus("error", message);
  if (appElement) {
    const paragraph = document.createElement("p");
    paragraph.className = "ax-error";
    paragraph.textContent = message;
    appElement.replaceChildren(paragraph);
  }
  console.error(error);
});
