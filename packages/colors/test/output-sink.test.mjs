import { test } from "node:test";
import assert from "node:assert/strict";

import {
  OutputBindingConflictError,
  OutputBindingError,
  OutputInlineBindingConflictError,
  OutputAtomicityViolationError,
  OutputSinkError,
  OutputSinkBusyError,
  OutputTargetCapabilityError,
  OutputTargetStaleError,
  acquireOutputLease,
} from "../output-sink.js";

class FakeStyleDeclaration {
  #values = new Map();

  get length() {
    return this.#values.size;
  }

  item(index) {
    return [...this.#values.keys()][index] ?? "";
  }

  getPropertyValue(name) {
    return this.#values.get(name) ?? "";
  }

  setProperty(name, value) {
    if (!/^--[a-z0-9-]+$/u.test(name) || typeof value !== "string" || value.length === 0) {
      return;
    }
    this.#values.set(name, value.trim());
  }

  get cssText() {
    return [...this.#values].map(([name, value]) => `${name}: ${value};`).join(" ");
  }

  entries() {
    return [...this.#values];
  }
}

class FakeRule {
  constructor(selectorText, declarations = []) {
    this.selectorText = selectorText;
    this.style = new FakeStyleDeclaration();
    for (const [name, value] of declarations) this.style.setProperty(name, value);
  }

  get cssText() {
    const declarations = this.style.cssText;
    return declarations === ""
      ? `${this.selectorText} {}`
      : `${this.selectorText} { ${declarations} }`;
  }
}

function parseSheet(text) {
  if (text === "") return [];
  const match = /^(\[[a-z0-9-]+\]) \{(?: (.*))?\}$/u.exec(text);
  if (!match) return [];
  const declarations = [];
  if (match[2]) {
    for (const part of match[2].split("; ")) {
      const declaration = part.endsWith(";") ? part.slice(0, -1) : part;
      if (declaration === "") continue;
      const separator = declaration.indexOf(": ");
      if (separator < 0) continue;
      declarations.push([declaration.slice(0, separator), declaration.slice(separator + 2)]);
    }
  }
  return [new FakeRule(match[1], declarations)];
}

function makeRealm() {
  const sheets = [];
  class FakeCSSStyleSheet {
    constructor() {
      this.cssRules = [];
      this.replaceCalls = [];
      this.failNext = false;
      this.beforeReplace = null;
      sheets.push(this);
    }

    replaceSync(text) {
      this.replaceCalls.push(text);
      const callback = this.beforeReplace;
      this.beforeReplace = null;
      callback?.();
      if (this.failNext) {
        this.failNext = false;
        throw new Error("atomic host rejected replacement");
      }
      this.cssRules = parseSheet(text);
    }
  }
  return { CSSStyleSheet: FakeCSSStyleSheet, sheets };
}

function makeRoot(realm, kind, ownerDocument = null) {
  const elements = [];
  const root = kind === "document"
    ? { nodeType: 9, defaultView: realm, adoptedStyleSheets: [] }
    : { nodeType: 11, host: {}, ownerDocument, adoptedStyleSheets: [] };
  root.querySelectorAll = (selector) => {
    const match = /^\[([a-z0-9-]+)\]$/u.exec(selector);
    if (!match) return [];
    return elements.filter(
      (element) => element.getRootNode() === root && element.hasAttribute(match[1]),
    );
  };
  root.addElement = (element) => {
    if (!elements.includes(element)) elements.push(element);
  };
  return root;
}

function makeElement(document, root) {
  const attributes = new Map();
  const inlineNames = [];
  let currentDocument = document;
  let currentRoot = root;
  const target = {
    nodeType: 1,
    isConnected: true,
    get ownerDocument() {
      return currentDocument;
    },
    set ownerDocument(next) {
      currentDocument = next;
    },
    getRootNode: () => currentRoot,
    hasAttribute: (name) => attributes.has(name),
    setAttribute: (name, value) => attributes.set(name, String(value)),
    removeAttribute: (name) => attributes.delete(name),
    attributeNames: () => [...attributes.keys()],
    addInline: (name) => inlineNames.push(name),
    removeInline: (name) => {
      const index = inlineNames.indexOf(name);
      if (index !== -1) inlineNames.splice(index, 1);
    },
    moveTo: (nextDocument, nextRoot) => {
      currentDocument = nextDocument;
      currentRoot = nextRoot;
      nextRoot.addElement(target);
    },
    style: {
      get length() {
        return inlineNames.length;
      },
      item: (index) => inlineNames[index] ?? "",
    },
  };
  root.addElement(target);
  return target;
}

function documentTarget() {
  const realm = makeRealm();
  const root = makeRoot(realm, "document");
  const target = makeElement(root, root);
  return { target, root, realm };
}

function shadowTarget() {
  const realm = makeRealm();
  const document = makeRoot(realm, "document");
  const root = makeRoot(realm, "shadow", document);
  const target = makeElement(document, root);
  return { target, root, document, realm };
}

function selectorFor(target) {
  const marker = target.attributeNames().find((name) => name.startsWith("data-lab-colors-output-sink-"));
  assert.ok(marker, "sink must install its private marker");
  return `[${marker}]`;
}

function sheetText(sheet) {
  return [...sheet.cssRules].map((rule) => rule.cssText).join("\n");
}

test("one target has one sheet; active disjoint leases merge while absent bindings stay absent", () => {
  const { target, root } = documentTarget();
  const supplied = ["--lab-z", "--lab-a"];
  const a = acquireOutputLease(target, supplied, "test/a");
  supplied.push("--lab-mutated-after-acquire");
  const b = acquireOutputLease(target, ["--lab-b"], "test/b");

  assert.equal(root.adoptedStyleSheets.length, 1);
  assert.deepEqual(a.outputBindings, ["--lab-z", "--lab-a"]);
  assert.ok(Object.isFrozen(a.outputBindings));

  assert.equal(a.publish({ "--lab-a": "#111111" }), true);
  assert.equal(a.stamp, 1);
  assert.equal(
    sheetText(root.adoptedStyleSheets[0]),
    `${selectorFor(target)} { --lab-a: #111111; }`,
  );

  assert.equal(b.publish({ "--lab-b": "#222222" }), true);
  assert.equal(b.stamp, 2);
  assert.equal(
    sheetText(root.adoptedStyleSheets[0]),
    `${selectorFor(target)} { --lab-a: #111111; --lab-b: #222222; }`,
  );
});

test("steady publication has two full guards and no ownership churn", () => {
  const { target, root, realm } = documentTarget();
  const lease = acquireOutputLease(target, ["--lab-a"], "test/steady-budget");
  assert.equal(lease.publish({ "--lab-a": "#111111" }), true);
  const live = root.adoptedStyleSheets[0];
  const scratchCalls = () =>
    realm.sheets
      .filter((sheet) => sheet !== live)
      .reduce((total, sheet) => total + sheet.replaceCalls.length, 0);

  let markerQueries = 0;
  const query = root.querySelectorAll;
  root.querySelectorAll = (selector) => {
    markerQueries++;
    return query(selector);
  };
  let adopted = root.adoptedStyleSheets;
  let adoptedWrites = 0;
  Object.defineProperty(root, "adoptedStyleSheets", {
    configurable: true,
    get: () => adopted,
    set(next) {
      adoptedWrites++;
      adopted = next;
    },
  });
  let markerWrites = 0;
  const setAttribute = target.setAttribute;
  const removeAttribute = target.removeAttribute;
  target.setAttribute = (...args) => {
    markerWrites++;
    return setAttribute(...args);
  };
  target.removeAttribute = (...args) => {
    markerWrites++;
    return removeAttribute(...args);
  };
  const beforeScratchCalls = scratchCalls();
  const liveCalls = live.replaceCalls.length;

  assert.equal(lease.publish({ "--lab-a": "#222222" }), true);
  assert.equal(markerQueries, 2, "pre-live and post-live guards each scan marker uniqueness once");
  assert.equal(scratchCalls(), beforeScratchCalls + 1);
  assert.equal(live.replaceCalls.length, liveCalls + 1);
  assert.equal(markerWrites, 0);
  assert.equal(adoptedWrites, 0);
});

test("overlap is typed and rejected before any target or live-sheet mutation", () => {
  const { target, root } = documentTarget();
  const first = acquireOutputLease(target, ["--lab-a", "--lab-b"], "test/first");
  first.publish({ "--lab-a": "#111111", "--lab-b": "#222222" });
  const live = root.adoptedStyleSheets[0];
  const before = { text: sheetText(live), calls: live.replaceCalls.length, stamp: first.stamp };

  assert.throws(
    () => acquireOutputLease(target, ["--lab-c", "--lab-b"], "test/conflict"),
    (error) => {
      assert.ok(error instanceof OutputBindingConflictError);
      assert.equal(error.code, "OUTPUT_BINDING_CONFLICT");
      assert.deepEqual(error.bindings, ["--lab-b"]);
      return true;
    },
  );
  assert.equal(root.adoptedStyleSheets.length, 1);
  assert.equal(sheetText(live), before.text);
  assert.equal(live.replaceCalls.length, before.calls);
  assert.equal(first.stamp, before.stamp);
});

test("a rejected live replacement leaves bytes, lease state, and stamp unchanged", () => {
  const { target, root } = documentTarget();
  const lease = acquireOutputLease(target, ["--lab-a", "--lab-b"], "test/failure");
  lease.publish({ "--lab-a": "#111111", "--lab-b": "#222222" });
  const live = root.adoptedStyleSheets[0];
  const before = sheetText(live);
  const stamp = lease.stamp;
  const calls = live.replaceCalls.length;

  live.failNext = true;
  assert.throws(
    () => lease.publish({ "--lab-a": "#aaaaaa", "--lab-b": "#bbbbbb" }),
    /atomic host rejected replacement/u,
  );

  assert.equal(sheetText(live), before);
  assert.equal(lease.stamp, stamp);
  assert.equal(lease.state, "active");
  assert.equal(live.replaceCalls.length, calls + 1, "one live replace was attempted");
});

test("host drift during replace poisons publication but leaves a revocable lease", () => {
  const { target, root } = documentTarget();
  const lease = acquireOutputLease(target, ["--lab-a"], "test/post-replace-drift");
  const live = root.adoptedStyleSheets[0];
  live.beforeReplace = () => target.addInline("--lab-a");

  assert.throws(
    () => lease.publish({ "--lab-a": "#111111" }),
    (error) =>
      error instanceof OutputAtomicityViolationError &&
      error.code === "OUTPUT_ATOMICITY_VIOLATION" &&
      error.cause instanceof OutputInlineBindingConflictError,
  );
  assert.equal(lease.state, "active", "a poisoned publication must not hide its lease");
  assert.equal(lease.stamp, 0, "a rejected publication must not commit logical state");

  assert.equal(lease.dispose(), true, "poison must not block explicit revocation");
  assert.equal(lease.state, "disposed");
  assert.equal(sheetText(live), "");
  assert.deepEqual(root.adoptedStyleSheets, []);
  assert.deepEqual(target.style.item(0), "--lab-a", "revocation must preserve foreign inline state");

  target.removeInline("--lab-a");
  const recovered = acquireOutputLease(target, ["--lab-a"], "test/post-replace-recovery");
  assert.equal(recovered.publish({ "--lab-a": "#222222" }), true);
});

test("every structural seam is checked again after the live replacement", () => {
  const cases = [
    {
      name: "connectivity",
      mutate({ target }) {
        target.isConnected = false;
      },
    },
    {
      name: "root and realm",
      mutate({ target }) {
        const nextRealm = makeRealm();
        const nextRoot = makeRoot(nextRealm, "document");
        target.moveTo(nextRoot, nextRoot);
      },
    },
    {
      name: "target marker",
      mutate({ target }) {
        target.removeAttribute(target.attributeNames()[0]);
      },
    },
    {
      name: "marker uniqueness",
      mutate({ target, root }) {
        const clone = makeElement(root, root);
        clone.setAttribute(target.attributeNames()[0], "");
      },
    },
    {
      name: "adopted stylesheet",
      mutate({ root }) {
        root.adoptedStyleSheets = [];
      },
    },
  ];

  for (const seam of cases) {
    const host = documentTarget();
    const lease = acquireOutputLease(host.target, ["--lab-a"], `test/post-${seam.name}`);
    const live = host.root.adoptedStyleSheets[0];
    live.beforeReplace = () => seam.mutate(host);

    assert.throws(
      () => lease.publish({ "--lab-a": "#111111" }),
      (error) =>
        error instanceof OutputAtomicityViolationError &&
        error.code === "OUTPUT_ATOMICITY_VIOLATION" &&
        error.cause instanceof OutputSinkError,
      `${seam.name} drift must not return publication success`,
    );
    assert.equal(lease.state, "active", `${seam.name} drift must leave a reachable lease`);
    assert.equal(lease.dispose(), true, `${seam.name} drift must remain revocable`);
    assert.equal(lease.state, "disposed");
    assert.equal(sheetText(live), "");
    assert.deepEqual(host.root.adoptedStyleSheets, []);
  }
});

test("dispose atomically removes only its binding set and makes the lease stale", () => {
  const { target, root } = documentTarget();
  const a = acquireOutputLease(target, ["--lab-a"], "test/a");
  const b = acquireOutputLease(target, ["--lab-b"], "test/b");
  a.publish({ "--lab-a": "#111111" });
  b.publish({ "--lab-b": "#222222" });
  const live = root.adoptedStyleSheets[0];
  const calls = live.replaceCalls.length;

  assert.equal(a.dispose(), true);
  assert.equal(a.state, "disposed");
  assert.equal(a.stamp, 3);
  assert.equal(live.replaceCalls.length, calls + 1);
  assert.equal(sheetText(live), `${selectorFor(target)} { --lab-b: #222222; }`);

  assert.equal(a.publish({ "--lab-a": "#aaaaaa" }), false);
  assert.equal(a.dispose(), false);
  assert.equal(live.replaceCalls.length, calls + 1);
  assert.equal(sheetText(live), `${selectorFor(target)} { --lab-b: #222222; }`);

  const callsBeforeLastDispose = live.replaceCalls.length;
  assert.equal(b.dispose(), true);
  assert.equal(live.replaceCalls.length, callsBeforeLastDispose + 1);
  assert.equal(sheetText(live), "");
  assert.deepEqual(root.adoptedStyleSheets, [], "the last lease must detach its dormant sheet");
  assert.deepEqual(target.attributeNames(), [], "the last lease must remove its marker");

  const reacquired = acquireOutputLease(target, ["--lab-a"], "test/reacquire");
  assert.equal(root.adoptedStyleSheets.length, 1);
  assert.equal(root.adoptedStyleSheets[0], live, "reacquisition reuses the per-target sheet");
  assert.equal(reacquired.publish({ "--lab-a": "#333333" }), true);
});

test("disposing an uncommitted empty lease performs no fallible live replacement", () => {
  const { target, root } = documentTarget();
  const lease = acquireOutputLease(target, ["--lab-a"], "test/uncommitted-revoke");
  const live = root.adoptedStyleSheets[0];
  let callbacks = 0;
  live.beforeReplace = () => callbacks++;

  assert.equal(lease.dispose(), true);
  assert.equal(callbacks, 0);
  assert.deepEqual(live.replaceCalls, []);
  assert.deepEqual(root.adoptedStyleSheets, []);
});

test("dispose uses stored authority after a cross-realm move and dormant reacquire rehomes", () => {
  const first = documentTarget();
  const lease = acquireOutputLease(first.target, ["--lab-a"], "test/move");
  assert.equal(lease.publish({ "--lab-a": "#111111" }), true);
  const oldLive = first.root.adoptedStyleSheets[0];

  const nextRealm = makeRealm();
  const nextRoot = makeRoot(nextRealm, "document");
  first.target.moveTo(nextRoot, nextRoot);

  assert.throws(
    () => lease.publish({ "--lab-a": "#222222" }),
    (error) => error instanceof OutputTargetStaleError,
  );
  assert.equal(lease.dispose(), true);
  assert.equal(lease.state, "disposed");
  assert.equal(sheetText(oldLive), "");
  assert.deepEqual(first.root.adoptedStyleSheets, []);
  assert.deepEqual(first.target.attributeNames(), []);

  const reacquired = acquireOutputLease(first.target, ["--lab-a"], "test/rehome");
  assert.equal(nextRoot.adoptedStyleSheets.length, 1);
  assert.notEqual(
    nextRoot.adoptedStyleSheets[0],
    oldLive,
    "a new realm requires a newly constructed stylesheet",
  );
  assert.equal(reacquired.publish({ "--lab-a": "#333333" }), true);
  assert.equal(
    sheetText(nextRoot.adoptedStyleSheets[0]),
    `${selectorFor(first.target)} { --lab-a: #333333; }`,
  );
});

test("failed cross-realm rehome is target-atomic and remains retryable", () => {
  const first = documentTarget();
  const lease = acquireOutputLease(first.target, ["--lab-a"], "test/rehome-source");
  assert.equal(lease.publish({ "--lab-a": "#111111" }), true);
  assert.equal(lease.dispose(), true);

  const nextRealm = makeRealm();
  const nextRoot = makeRoot(nextRealm, "document");
  let adopted = nextRoot.adoptedStyleSheets;
  let reject = true;
  let failNextReadback = false;
  Object.defineProperty(nextRoot, "adoptedStyleSheets", {
    configurable: true,
    get() {
      if (failNextReadback) {
        failNextReadback = false;
        throw new Error("new root rejected dormant sheet readback");
      }
      return adopted;
    },
    set(next) {
      adopted = next;
      if (reject && next.length > 0) failNextReadback = true;
    },
  });
  first.target.moveTo(nextRoot, nextRoot);

  assert.throws(
    () => acquireOutputLease(first.target, ["--lab-a"], "test/rehome-failure"),
    (error) => error instanceof OutputTargetCapabilityError,
  );
  assert.deepEqual(first.target.attributeNames(), []);
  assert.deepEqual(nextRoot.adoptedStyleSheets, []);

  reject = false;
  const recovered = acquireOutputLease(first.target, ["--lab-a"], "test/rehome-retry");
  assert.equal(recovered.publish({ "--lab-a": "#222222" }), true);
  assert.equal(nextRoot.adoptedStyleSheets.length, 1);
});

test("reentrant acquire during dormant rehome is typed busy and creates no second sink", () => {
  const first = documentTarget();
  const original = acquireOutputLease(first.target, ["--lab-a"], "test/rehome-reentrant-source");
  assert.equal(original.dispose(), true);

  const nextRealm = makeRealm();
  const nextRoot = makeRoot(nextRealm, "document");
  let adopted = nextRoot.adoptedStyleSheets;
  let nestedError = null;
  let reenter = true;
  Object.defineProperty(nextRoot, "adoptedStyleSheets", {
    configurable: true,
    get: () => adopted,
    set(next) {
      adopted = next;
      if (!reenter || next.length === 0) return;
      reenter = false;
      try {
        acquireOutputLease(first.target, ["--lab-b"], "test/rehome-reentrant-nested");
      } catch (error) {
        nestedError = error;
      }
    },
  });
  first.target.moveTo(nextRoot, nextRoot);

  const outer = acquireOutputLease(first.target, ["--lab-a"], "test/rehome-reentrant-outer");
  assert.ok(nestedError instanceof OutputSinkBusyError);
  assert.equal(nestedError.code, "OUTPUT_SINK_BUSY");
  assert.equal(nextRoot.adoptedStyleSheets.length, 1);
  assert.equal(first.target.attributeNames().length, 1);
  assert.equal(outer.publish({ "--lab-a": "#111111" }), true);
  assert.equal(outer.dispose(), true);
  assert.deepEqual(nextRoot.adoptedStyleSheets, []);
  assert.deepEqual(first.target.attributeNames(), []);
});

test("cross-realm rehome rolls back an adoption setter that mutates then throws", () => {
  const first = documentTarget();
  const lease = acquireOutputLease(first.target, ["--lab-a"], "test/rehome-throw-source");
  assert.equal(lease.dispose(), true);

  const nextRealm = makeRealm();
  const nextRoot = makeRoot(nextRealm, "document");
  let adopted = nextRoot.adoptedStyleSheets;
  let reject = true;
  Object.defineProperty(nextRoot, "adoptedStyleSheets", {
    configurable: true,
    get: () => adopted,
    set(next) {
      adopted = next;
      if (reject && next.length > 0) throw new Error("rehome adoption mutated before throwing");
    },
  });
  first.target.moveTo(nextRoot, nextRoot);

  assert.throws(
    () => acquireOutputLease(first.target, ["--lab-a"], "test/rehome-throw"),
    (error) => error instanceof OutputTargetCapabilityError,
  );
  assert.deepEqual(first.target.attributeNames(), []);
  assert.deepEqual(nextRoot.adoptedStyleSheets, []);

  reject = false;
  const recovered = acquireOutputLease(first.target, ["--lab-a"], "test/rehome-throw-retry");
  assert.equal(recovered.publish({ "--lab-a": "#222222" }), true);
});

test("dispose ignores owned-inline drift, preserves it, and permits clean reacquire", () => {
  const { target, root } = documentTarget();
  const lease = acquireOutputLease(target, ["--lab-a"], "test/inline-revoke");
  assert.equal(lease.publish({ "--lab-a": "#111111" }), true);
  const live = root.adoptedStyleSheets[0];
  target.addInline("--lab-a");

  assert.equal(lease.dispose(), true);
  assert.equal(lease.state, "disposed");
  assert.equal(sheetText(live), "");
  assert.deepEqual(root.adoptedStyleSheets, []);
  assert.equal(target.style.item(0), "--lab-a");

  target.removeInline("--lab-a");
  const reacquired = acquireOutputLease(target, ["--lab-a"], "test/inline-reacquire");
  assert.equal(reacquired.publish({ "--lab-a": "#222222" }), true);
});

test("dispose repairs stored-sheet drift for remaining owners without consulting the target host", () => {
  const { target, root } = documentTarget();
  const first = acquireOutputLease(target, ["--lab-a"], "test/drift-a");
  const second = acquireOutputLease(target, ["--lab-b"], "test/drift-b");
  assert.equal(first.publish({ "--lab-a": "#111111" }), true);
  assert.equal(second.publish({ "--lab-b": "#222222" }), true);
  const live = root.adoptedStyleSheets[0];
  live.replaceSync(
    `${selectorFor(target)} { --lab-a: #aaaaaa; --lab-b: #bbbbbb; }`,
  );

  assert.equal(first.dispose(), true);
  assert.equal(first.state, "disposed");
  assert.equal(
    sheetText(live),
    `${selectorFor(target)} { --lab-b: #222222; }`,
    "revocation must reconstruct the exact state of the remaining owner",
  );
  assert.equal(second.publish({ "--lab-b": "#333333" }), true);
});

test("dispose releases a disconnected target and allows a later clean reacquire", () => {
  const { target, root } = documentTarget();
  const lease = acquireOutputLease(target, ["--lab-a"], "test/disconnected-revoke");
  assert.equal(lease.publish({ "--lab-a": "#111111" }), true);
  const live = root.adoptedStyleSheets[0];
  target.isConnected = false;

  assert.equal(lease.dispose(), true);
  assert.equal(lease.state, "disposed");
  assert.equal(sheetText(live), "");
  assert.deepEqual(root.adoptedStyleSheets, []);
  assert.deepEqual(target.attributeNames(), []);

  target.isConnected = true;
  const reacquired = acquireOutputLease(target, ["--lab-a"], "test/reconnected");
  assert.equal(reacquired.publish({ "--lab-a": "#222222" }), true);
});

test("a foreign clone of a dormant marker stays inert and cannot block ownership release", () => {
  const { target, root } = documentTarget();
  const lease = acquireOutputLease(target, ["--lab-a"], "test/marker-clone");
  assert.equal(lease.publish({ "--lab-a": "#111111" }), true);
  const marker = target.attributeNames()[0];
  const clone = makeElement(root, root);
  clone.setAttribute(marker, "");

  assert.equal(lease.dispose(), true);
  assert.equal(lease.state, "disposed");
  assert.deepEqual(root.adoptedStyleSheets, []);
  assert.deepEqual(target.attributeNames(), []);
  assert.deepEqual(clone.attributeNames(), [marker], "the sink must not mutate a foreign clone");

  const reacquired = acquireOutputLease(target, ["--lab-a"], "test/after-marker-clone");
  assert.notEqual(target.attributeNames()[0], marker);
  assert.equal(reacquired.publish({ "--lab-a": "#222222" }), true);
});

test("replace failure during dispose is typed after logical ownership is released", () => {
  const { target, root } = documentTarget();
  const lease = acquireOutputLease(target, ["--lab-a"], "test/revoke-replace-failure");
  assert.equal(lease.publish({ "--lab-a": "#111111" }), true);
  const live = root.adoptedStyleSheets[0];
  live.failNext = true;

  assert.throws(
    () => lease.dispose(),
    (error) =>
      error instanceof OutputAtomicityViolationError &&
      error.code === "OUTPUT_ATOMICITY_VIOLATION",
  );
  assert.equal(lease.state, "disposed", "host failure must not retain hidden ownership");
  assert.equal(lease.dispose(), false);
  assert.deepEqual(root.adoptedStyleSheets, [], "the failed sheet must still be detached");
  assert.deepEqual(target.attributeNames(), []);
  assert.throws(
    () => acquireOutputLease(target, ["--lab-a"], "test/revoke-poisoned"),
    (error) => error instanceof OutputTargetStaleError && error.code === "OUTPUT_TARGET_STALE",
    "uncertain dormant bytes poison reuse instead of masquerading as an active-owner conflict",
  );
});

test("scratch failure during dispose releases ownership before reporting uncertainty", () => {
  const { target, root, realm } = documentTarget();
  const first = acquireOutputLease(target, ["--lab-a"], "test/revoke-scratch-a");
  const second = acquireOutputLease(target, ["--lab-b"], "test/revoke-scratch-b");
  assert.equal(first.publish({ "--lab-a": "#111111" }), true);
  assert.equal(second.publish({ "--lab-b": "#222222" }), true);
  const scratch = realm.sheets[2];
  scratch.failNext = true;

  assert.throws(
    () => first.dispose(),
    (error) =>
      error instanceof OutputAtomicityViolationError &&
      error.code === "OUTPUT_ATOMICITY_VIOLATION",
  );
  assert.equal(first.state, "disposed", "scratch failure must not retain hidden ownership");
  assert.equal(first.dispose(), false);
  assert.equal(second.dispose(), true, "the remaining owner must retain a cleanup path");
  assert.deepEqual(root.adoptedStyleSheets, []);
  assert.deepEqual(target.attributeNames(), []);
});

test("detach failure is typed only after the lease and target marker are released", () => {
  const { target, root } = documentTarget();
  const lease = acquireOutputLease(target, ["--lab-a"], "test/revoke-detach-failure");
  assert.equal(lease.publish({ "--lab-a": "#111111" }), true);
  const live = root.adoptedStyleSheets[0];
  let adopted = root.adoptedStyleSheets;
  Object.defineProperty(root, "adoptedStyleSheets", {
    configurable: true,
    get: () => adopted,
    set(next) {
      if (next.length === 0) throw new Error("root rejected sheet detachment");
      adopted = next;
    },
  });

  assert.throws(
    () => lease.dispose(),
    (error) =>
      error instanceof OutputAtomicityViolationError &&
      error.code === "OUTPUT_ATOMICITY_VIOLATION",
  );
  assert.equal(lease.state, "disposed");
  assert.equal(lease.dispose(), false);
  assert.equal(sheetText(live), "", "failed detachment must leave only inert empty bytes");
  assert.deepEqual(target.attributeNames(), [], "marker cleanup must run after detach failure");
});

test("ownership cancellation and live-host reentrancy cannot publish a stale operation", () => {
  const { target, root } = documentTarget();
  const lease = acquireOutputLease(target, ["--lab-a"], "test/reentrant");
  const live = root.adoptedStyleSheets[0];

  assert.equal(lease.publish({ "--lab-a": "#111111" }, () => false), false);
  assert.equal(live.replaceCalls.length, 0);
  assert.equal(lease.stamp, 0);

  let ownsChecks = 0;
  assert.equal(
    lease.publish({ "--lab-a": "#111111" }, () => ++ownsChecks === 1),
    false,
    "authority revoked after scratch preparation must not reach the live sheet",
  );
  assert.equal(ownsChecks, 2);
  assert.equal(live.replaceCalls.length, 0);

  let nested;
  live.beforeReplace = () => {
    try {
      lease.publish({ "--lab-a": "#222222" });
    } catch (error) {
      nested = error;
    }
  };
  assert.equal(lease.publish({ "--lab-a": "#111111" }), true);
  assert.ok(nested instanceof OutputSinkBusyError);
  assert.equal(nested.code, "OUTPUT_SINK_BUSY");
  assert.equal(live.replaceCalls.length, 1);
  assert.equal(sheetText(live), `${selectorFor(target)} { --lab-a: #111111; }`);
  assert.equal(lease.stamp, 1);
});

test("partial last-lease cleanup cannot leave a logically active lease", () => {
  const { target, root } = documentTarget();
  const lease = acquireOutputLease(target, ["--lab-a"], "test/cleanup-failure");
  lease.publish({ "--lab-a": "#111111" });
  const live = root.adoptedStyleSheets[0];
  const stamp = lease.stamp;
  target.removeAttribute = () => {
    throw new Error("marker removal rejected");
  };

  assert.throws(
    () => lease.dispose(),
    (error) =>
      error instanceof OutputAtomicityViolationError &&
      error.code === "OUTPUT_ATOMICITY_VIOLATION",
  );
  assert.equal(sheetText(live), "");
  assert.deepEqual(root.adoptedStyleSheets, []);
  assert.equal(lease.state, "disposed", "semantic revoke commits before best-effort cleanup");
  assert.equal(lease.stamp, stamp + 1);
  assert.equal(lease.dispose(), false);
  assert.throws(
    () => acquireOutputLease(target, ["--lab-a"], "test/poisoned-reacquire"),
    (error) => error instanceof OutputTargetStaleError,
  );
});

test("failed sheet construction or marker verification leaves no target effect", () => {
  const construction = documentTarget();
  construction.realm.CSSStyleSheet = class {
    constructor() {
      throw new Error("construction rejected");
    }
  };
  assert.throws(
    () => acquireOutputLease(construction.target, ["--lab-a"], "test/construction"),
    (error) => error instanceof OutputTargetCapabilityError,
  );
  assert.deepEqual(construction.target.attributeNames(), []);
  assert.deepEqual(construction.root.adoptedStyleSheets, []);

  const verification = documentTarget();
  const query = verification.root.querySelectorAll;
  let markerQueries = 0;
  verification.root.querySelectorAll = (selector) => {
    markerQueries++;
    if (markerQueries === 2) throw new Error("post-set query rejected");
    return query(selector);
  };
  assert.throws(
    () => acquireOutputLease(verification.target, ["--lab-a"], "test/marker"),
    (error) => error instanceof OutputTargetCapabilityError,
  );
  assert.deepEqual(verification.target.attributeNames(), []);
  assert.deepEqual(verification.root.adoptedStyleSheets, []);

  const adoption = documentTarget();
  let adopted = adoption.root.adoptedStyleSheets;
  let failNextReadback = false;
  Object.defineProperty(adoption.root, "adoptedStyleSheets", {
    configurable: true,
    get() {
      if (failNextReadback) {
        failNextReadback = false;
        throw new Error("post-adoption readback rejected");
      }
      return adopted;
    },
    set(next) {
      adopted = next;
      if (next.length > 0) failNextReadback = true;
    },
  });
  assert.throws(
    () => acquireOutputLease(adoption.target, ["--lab-a"], "test/adoption-readback"),
    (error) => error instanceof OutputTargetCapabilityError,
  );
  assert.deepEqual(adoption.target.attributeNames(), []);
  assert.deepEqual(adoption.root.adoptedStyleSheets, []);

  const throwingAdoption = documentTarget();
  let partiallyAdopted = throwingAdoption.root.adoptedStyleSheets;
  Object.defineProperty(throwingAdoption.root, "adoptedStyleSheets", {
    configurable: true,
    get: () => partiallyAdopted,
    set(next) {
      partiallyAdopted = next;
      if (next.length > 0) throw new Error("adoption mutated before throwing");
    },
  });
  assert.throws(
    () => acquireOutputLease(throwingAdoption.target, ["--lab-a"], "test/adoption-throw"),
    (error) => error instanceof OutputTargetCapabilityError,
  );
  assert.deepEqual(throwingAdoption.target.attributeNames(), []);
  assert.deepEqual(throwingAdoption.root.adoptedStyleSheets, []);

  const throwingMarker = documentTarget();
  const setAttribute = throwingMarker.target.setAttribute;
  throwingMarker.target.setAttribute = (...args) => {
    setAttribute(...args);
    throw new Error("marker mutated before throwing");
  };
  assert.throws(
    () => acquireOutputLease(throwingMarker.target, ["--lab-a"], "test/marker-throw"),
    (error) => error instanceof OutputTargetCapabilityError,
  );
  assert.deepEqual(throwingMarker.target.attributeNames(), []);
  assert.deepEqual(throwingMarker.root.adoptedStyleSheets, []);
});

test("extra vars and non-custom bindings are rejected before live mutation", () => {
  const { target, root } = documentTarget();
  for (const ordinaryProperty of ["color", "background"]) {
    assert.throws(
      () => acquireOutputLease(target, [ordinaryProperty], "test/ordinary-property"),
      (error) => error instanceof OutputBindingError && error.code === "OUTPUT_BINDING_INVALID",
    );
  }
  assert.equal(root.adoptedStyleSheets.length, 0);

  const lease = acquireOutputLease(target, ["--lab-ok"], "test/good");
  const live = root.adoptedStyleSheets[0];
  assert.throws(
    () => lease.publish({ "--lab-ok": "#111111", "--lab-extra": "#222222" }),
    (error) => error instanceof OutputBindingError && error.code === "OUTPUT_VALUE_OUTSIDE_BINDINGS",
  );
  assert.equal(live.replaceCalls.length, 0);
  assert.equal(lease.stamp, 0);
});

test("owned inline declarations fail closed while nonbinding inline declarations are untouched", () => {
  const first = documentTarget();
  first.target.addInline("--lab-owned");
  assert.throws(
    () => acquireOutputLease(first.target, ["--lab-owned"], "test/inline-at-acquire"),
    (error) =>
      error instanceof OutputInlineBindingConflictError &&
      error.code === "OUTPUT_INLINE_BINDING_CONFLICT" &&
      error.bindings[0] === "--lab-owned",
  );
  assert.equal(first.root.adoptedStyleSheets.length, 0);
  assert.deepEqual(first.target.attributeNames(), []);

  const second = documentTarget();
  second.target.addInline("--lab-foreign");
  const lease = acquireOutputLease(second.target, ["--lab-owned"], "test/inline-after");
  assert.equal(lease.publish({ "--lab-owned": "#111111" }), true);
  const live = second.root.adoptedStyleSheets[0];
  const before = sheetText(live);
  const calls = live.replaceCalls.length;
  const stamp = lease.stamp;
  second.target.addInline("--lab-owned");

  assert.throws(
    () => lease.publish({ "--lab-owned": "#222222" }),
    (error) => error instanceof OutputInlineBindingConflictError,
  );
  assert.equal(sheetText(live), before);
  assert.equal(live.replaceCalls.length, calls);
  assert.equal(lease.stamp, stamp);
  assert.deepEqual(
    second.target.style.item(0),
    "--lab-foreign",
    "the sink must never mutate a nonbinding inline declaration",
  );
});

test("a ShadowRoot move/adoption and sink detachment are typed stale failures", () => {
  const { target, root, realm } = shadowTarget();
  const lease = acquireOutputLease(target, ["--lab-a"], "test/shadow");
  lease.publish({ "--lab-a": "#111111" });
  const live = root.adoptedStyleSheets[0];
  assert.equal(sheetText(live), `${selectorFor(target)} { --lab-a: #111111; }`);
  const stamp = lease.stamp;
  const calls = live.replaceCalls.length;

  target.ownerDocument = makeRoot(realm, "document");
  assert.throws(
    () => lease.publish({ "--lab-a": "#222222" }),
    (error) => error instanceof OutputTargetStaleError && error.code === "OUTPUT_TARGET_STALE",
  );
  assert.equal(lease.stamp, stamp);
  assert.equal(live.replaceCalls.length, calls);
});

test("removing the live sheet from adoptedStyleSheets is a typed stale failure", () => {
  const { target, root } = documentTarget();
  const lease = acquireOutputLease(target, ["--lab-a"], "test/detach");
  lease.publish({ "--lab-a": "#111111" });
  const live = root.adoptedStyleSheets[0];
  const calls = live.replaceCalls.length;
  const stamp = lease.stamp;
  root.adoptedStyleSheets = [];

  assert.throws(
    () => lease.publish({ "--lab-a": "#222222" }),
    (error) => error instanceof OutputTargetStaleError && error.code === "OUTPUT_TARGET_STALE",
  );
  assert.equal(live.replaceCalls.length, calls);
  assert.equal(lease.stamp, stamp);
});

test("a disconnected target is typed stale before a live replacement", () => {
  const { target, root } = documentTarget();
  const lease = acquireOutputLease(target, ["--lab-a"], "test/disconnect");
  lease.publish({ "--lab-a": "#111111" });
  const live = root.adoptedStyleSheets[0];
  const calls = live.replaceCalls.length;
  const stamp = lease.stamp;
  target.isConnected = false;

  assert.throws(
    () => lease.publish({ "--lab-a": "#222222" }),
    (error) => error instanceof OutputTargetStaleError && error.code === "OUTPUT_TARGET_STALE",
  );
  assert.equal(live.replaceCalls.length, calls);
  assert.equal(lease.stamp, stamp);
});

test("independent module copies share the target registry and cannot overlap", async () => {
  const { target, root } = documentTarget();
  const a = acquireOutputLease(target, ["--lab-a"], "test/copy-a");
  const copy = await import(`../output-sink.js?copy=${Date.now()}`);

  assert.throws(
    () => copy.acquireOutputLease(target, ["--lab-a"], "test/copy-b"),
    (error) => error?.code === "OUTPUT_BINDING_CONFLICT",
  );
  assert.equal(root.adoptedStyleSheets.length, 1);
  assert.equal(a.stamp, 0);
});

test("independent module copies share the target acquisition gate during initial attachment", async () => {
  const { target, root } = documentTarget();
  const copy = await import(`../output-sink.js?acquire-copy=${Date.now()}`);
  let adopted = root.adoptedStyleSheets;
  let nestedError = null;
  let reenter = true;
  Object.defineProperty(root, "adoptedStyleSheets", {
    configurable: true,
    get: () => adopted,
    set(next) {
      adopted = next;
      if (!reenter || next.length === 0) return;
      reenter = false;
      try {
        copy.acquireOutputLease(target, ["--lab-b"], "test/attach-reentrant-nested");
      } catch (error) {
        nestedError = error;
      }
    },
  });

  const outer = acquireOutputLease(target, ["--lab-a"], "test/attach-reentrant-outer");
  assert.equal(nestedError?.code, "OUTPUT_SINK_BUSY");
  assert.equal(root.adoptedStyleSheets.length, 1);
  assert.equal(target.attributeNames().length, 1);
  assert.equal(outer.publish({ "--lab-a": "#111111" }), true);
  assert.equal(outer.dispose(), true);
  assert.deepEqual(root.adoptedStyleSheets, []);
  assert.deepEqual(target.attributeNames(), []);
});
