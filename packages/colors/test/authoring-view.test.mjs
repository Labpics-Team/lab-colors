import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { colors } from "../index.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

// A deterministic DOM boundary: real Core sessions, with observable stylesheet commits.
// Browser rendering and CSS consumption are exercised by the installed example separately.
function host() {
  const state = { writes: [], failure: null };
  class Node {
    constructor() { this.parentNode = null; this.children = []; this.text = ""; }
    get textContent() { return this.text; }
    set textContent(value) {
      if (state.failure !== null) throw state.failure;
      this.text = value;
      state.writes.push(value);
    }
    appendChild(child) { this.children.push(child); child.parentNode = this; return child; }
    removeChild(child) {
      this.children.splice(this.children.indexOf(child), 1);
      child.parentNode = null;
      return child;
    }
  }
  class Element extends Node {
    constructor() { super(); this.attributes = new Map(); this.inline = new Map(); }
    setAttribute(name, value) { this.attributes.set(name, value); }
    hasAttribute(name) { return this.attributes.has(name); }
    getAttribute(name) { return this.attributes.get(name) ?? null; }
    removeAttribute(name) { this.attributes.delete(name); }
    style = { getPropertyValue: (name) => this.inline.get(name) ?? "" };
  }
  class HTMLElement extends Element {
    getRootNode() { return this.ownerDocument; }
    isConnected = true;
  }
  const document = {
    head: new Node(),
    defaultView: { Node, Element, HTMLElement },
    createElement() { const element = new HTMLElement(); element.ownerDocument = document; return element; },
  };
  const root = document.createElement("main");
  return { ...state, root, document, state,
    get sheet() { return document.head.children[0]?.textContent ?? ""; } };
}

async function fixedProgram(name, source = "#808080", options = { separation: [0.1, 2] }) {
  const c = colors(), canvas = c.surface("canvas");
  return c.compile({ [name]: c.fixed(name, source).on(canvas, options) });
}

test("controlled context and all values commit once; conflict and unknown keep the accepted frame", async () => {
  const h = host(), program = await fixedProgram("ink");
  const view = program.attach(h.root, { canvas: "#e6ddd0" });
  try {
    assert.equal(view.lastUpdate.state, "committed");
    assert.equal(h.state.writes.length, 1);
    assert.match(h.sheet, /--canvas:rgb\(230 221 208 \/ 1\);--ink:rgb\(128 128 128 \/ 1\);/u);
    const current = view.current, sheet = h.sheet;
    const conflict = view.set({ canvas: "#808080" });
    assert.equal(conflict.state, "rejected");
    assert.equal(conflict.proposed.state, "conflict");
    assert.equal(view.current, current);
    assert.equal(h.sheet, sheet);
    assert.equal(h.state.writes.length, 1);
    assert.equal(view.lastUpdate, conflict);
    const unknown = view.set({ canvas: null });
    assert.equal(unknown.state, "rejected");
    assert.equal(unknown.proposed.state, "unknown");
    assert.equal(view.current, current);
    assert.equal(h.sheet, sheet);
    assert.equal(view.set({ canvas: "#283548" }).state, "committed");
    assert.equal(h.state.writes.length, 2);
    assert.notEqual(view.current, current);
    assert.match(h.sheet, /--canvas:rgb\(40 53 72 \/ 1\)/u);
    assert.deepEqual(current.context.canvas, [230, 221, 208]);
  } finally { view.dispose(); program.dispose(); }
});

test("a host exception and reentrant input access cannot partially commit or poison later updates", async () => {
  const h = host(), program = await fixedProgram("ink");
  const view = program.attach(h.root, { canvas: "#e6ddd0" });
  try {
    const current = view.current, sheet = h.sheet, failure = new Error("host refused stylesheet");
    h.state.failure = failure;
    assert.throws(() => view.set({ canvas: "#283548" }), (error) => error === failure);
    assert.equal(view.current, current);
    assert.equal(h.sheet, sheet);
    h.state.failure = null;
    let reentrantError;
    assert.throws(() => view.set({ get canvas() {
      try { view.set({ canvas: "#283548" }); }
      catch (error) { reentrantError = error; throw error; }
      return "#283548";
    } }), { code: "color_invalid_context" });
    assert.equal(reentrantError.code, "color_busy");
    assert.equal(view.current, current);
    assert.equal(h.sheet, sheet);
    assert.equal(view.set({ canvas: "#283548" }).state, "committed");
    assert.equal(h.state.writes.length, 2);
  } finally { view.dispose(); program.dispose(); }
});

test("each attached owner survives program disposal and has an independent runtime", async () => {
  const a = host(), b = host(), program = await fixedProgram("ink");
  const first = program.attach(a.root, { canvas: "#e6ddd0" });
  const second = program.attach(b.root, { canvas: "#283548" });
  program.dispose();
  try {
    assert.throws(() => program.resolve({ canvas: "#283548" }), { code: "color_disposed" });
    assert.equal(first.set({ canvas: "#283548" }).state, "committed");
    assert.equal(second.set({ canvas: "#e6ddd0" }).state, "committed");
    assert.deepEqual(first.current.context.canvas, [40, 53, 72]);
    assert.deepEqual(second.current.context.canvas, [230, 221, 208]);
    first.dispose();
    assert.equal(a.document.head.children.length, 0);
    assert.equal(second.set({ canvas: "#283548" }).state, "committed");
  } finally { first.dispose(); second.dispose(); program.dispose(); }
});

test("replacement swaps the whole alias frame and transfers ownership only after acceptance", async () => {
  const h = host(), original = await fixedProgram("oldInk");
  const next = await fixedProgram("newInk", "#0067f4", {});
  const rejected = await fixedProgram("impossible", "#283548", { separation: [0.1, 2] });
  const old = original.attach(h.root, { canvas: "#283548" });
  let active = old;
  try {
    const frame = old.current, sheet = h.sheet;
    const refused = old.replace(rejected, { canvas: "#283548" });
    assert.equal(refused.state, "rejected");
    assert.equal(refused.proposed.state, "conflict");
    assert.equal(old.current, frame);
    assert.equal(h.sheet, sheet);
    assert.equal(h.state.writes.length, 1);

    const outcome = old.replace(next, { canvas: "#6b7f72" });
    assert.equal(outcome.state, "committed");
    active = outcome.view;
    assert.equal(active.current, outcome.frame);
    assert.equal(old.current, null);
    assert.throws(() => old.set({ canvas: "#e6ddd0" }), { code: "color_disposed" });
    assert.equal(h.state.writes.length, 2);
    assert.doesNotMatch(h.sheet, /--oldInk:|--impossible:/u);
    assert.match(h.sheet, /--canvas:rgb\(107 127 114 \/ 1\);--newInk:rgb\(0 103 244 \/ 1\);/u);
    old.dispose();
    next.dispose();
    assert.equal(h.document.head.children.length, 1);
    assert.equal(active.set({ canvas: "#e6ddd0" }).state, "committed");
  } finally { active.dispose(); original.dispose(); next.dispose(); rejected.dispose(); }
  assert.equal(h.document.head.children.length, 0);
  assert.equal(h.root.hasAttribute("data-lab-colors-scope"), false);
});

test("foreign changes to the owned CSS are detected before any runtime or style commit", async () => {
  const h = host(), program = await fixedProgram("ink");
  const view = program.attach(h.root, { canvas: "#e6ddd0" });
  try {
    const current = view.current;
    h.root.inline.set("--ink", "red");
    assert.throws(() => view.set({ canvas: "#283548" }), { code: "color_host_changed" });
    assert.equal(view.current, current);
    assert.equal(h.state.writes.length, 1);
    h.root.inline.clear();
    assert.equal(view.set({ canvas: "#283548" }).state, "committed");
    assert.throws(() => program.attach(h.root), { code: "color_host_changed" });
    assert.equal(h.document.head.children.length, 1);
  } finally { view.dispose(); program.dispose(); }
});
