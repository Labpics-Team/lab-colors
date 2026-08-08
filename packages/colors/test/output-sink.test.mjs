import { test } from "node:test";
import assert from "node:assert/strict";

import {
  OutputAtomicityViolationError,
  OutputBindingConflictError,
  OutputBindingError,
  OutputInlineBindingConflictError,
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

  removeProperty(name) {
    const previous = this.getPropertyValue(name);
    this.#values.delete(name);
    return previous;
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
  const match = /^(:root|:host) \{(?: (.*))?\}$/u.exec(text);
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
      this.actions = [];
      sheets.push(this);
    }

    replaceSync(text) {
      this.replaceCalls.push(text);
      const apply = () => {
        this.cssRules = parseSheet(text);
      };
      const action = this.actions.shift();
      if (action) return action({ apply, sheet: this, text });
      apply();
    }

    enqueue(action) {
      this.actions.push(action);
    }

    forceText(text) {
      this.cssRules = parseSheet(text);
    }
  }
  return { CSSStyleSheet: FakeCSSStyleSheet, sheets };
}

function installAdoptedStyleSheets(root) {
  let adopted = [];
  const control = {
    writes: 0,
    onSet: null,
    force(next) {
      adopted = [...next];
    },
    read() {
      return [...adopted];
    },
  };
  Object.defineProperty(root, "adoptedStyleSheets", {
    configurable: true,
    get() {
      return adopted;
    },
    set(next) {
      control.writes++;
      const candidate = Array.from(next);
      if (control.onSet) {
        return control.onSet(candidate, (value) => {
          adopted = [...value];
        });
      }
      adopted = candidate;
    },
  });
  root.adoptionControl = control;
  return root;
}

function makeDocument(realm = makeRealm()) {
  return installAdoptedStyleSheets({
    nodeType: 9,
    defaultView: realm,
    documentElement: null,
  });
}

function makeElement(document, treeRoot = document) {
  const inlineNames = [];
  let currentDocument = document;
  let currentTreeRoot = treeRoot;
  return {
    nodeType: 1,
    isConnected: true,
    shadowRoot: null,
    get ownerDocument() {
      return currentDocument;
    },
    getRootNode() {
      return currentTreeRoot;
    },
    moveToDocument(nextDocument) {
      if (currentDocument.documentElement === this) currentDocument.documentElement = null;
      currentDocument = nextDocument;
      currentTreeRoot = nextDocument;
      nextDocument.documentElement = this;
    },
    addInline(name) {
      if (!inlineNames.includes(name)) inlineNames.push(name);
    },
    removeInline(name) {
      const index = inlineNames.indexOf(name);
      if (index !== -1) inlineNames.splice(index, 1);
    },
    style: {
      get length() {
        return inlineNames.length;
      },
      item(index) {
        return inlineNames[index] ?? "";
      },
    },
  };
}

function documentTarget(realm = makeRealm()) {
  const root = makeDocument(realm);
  const target = makeElement(root);
  root.documentElement = target;
  return { target, root, realm };
}

function shadowTarget(realm = makeRealm()) {
  const document = makeDocument(realm);
  document.documentElement = makeElement(document);
  const target = makeElement(document);
  const root = installAdoptedStyleSheets({
    nodeType: 11,
    mode: "open",
    host: target,
    ownerDocument: document,
  });
  target.shadowRoot = root;
  return { target, root, document, realm };
}

function sheetText(sheet) {
  return [...sheet.cssRules].map((rule) => rule.cssText).join("\n");
}

function liveSheet(root) {
  return root.adoptionControl.read().at(-1) ?? null;
}

function sinkAuthority(target) {
  const symbol = Object.getOwnPropertySymbols(target).find((candidate) =>
    Symbol.keyFor(candidate)?.includes("output-sink/target-state"),
  );
  assert.ok(symbol, "the target must retain a private recovery authority");
  return { symbol, value: target[symbol] };
}

test("target identity is native: only documentElement/:root or an own open ShadowRoot host/:host", () => {
  const documentHost = documentTarget();
  const documentLease = acquireOutputLease(
    documentHost.target,
    ["--lab-a"],
    "test/document-identity",
  );
  assert.equal(documentLease.publish({ "--lab-a": "#111111" }), true);
  assert.equal(sheetText(liveSheet(documentHost.root)), ":root { --lab-a: #111111; }");

  const arbitrary = makeElement(documentHost.root);
  assert.throws(
    () => acquireOutputLease(arbitrary, ["--lab-a"], "test/light-dom"),
    (error) =>
      error instanceof OutputTargetCapabilityError && error.code === "OUTPUT_TARGET_CAPABILITY",
  );
  assert.equal(Object.getOwnPropertySymbols(arbitrary).length, 0);

  const shadowHost = shadowTarget();
  const shadowLease = acquireOutputLease(
    shadowHost.target,
    ["--lab-b"],
    "test/shadow-identity",
  );
  assert.equal(shadowLease.publish({ "--lab-b": "#222222" }), true);
  assert.equal(sheetText(liveSheet(shadowHost.root)), ":host { --lab-b: #222222; }");

  const cloneWithoutShadow = makeElement(shadowHost.document);
  assert.throws(
    () => acquireOutputLease(cloneWithoutShadow, ["--lab-b"], "test/shadow-clone"),
    (error) => error?.code === "OUTPUT_TARGET_CAPABILITY",
  );

  const closedHost = shadowTarget();
  closedHost.root.mode = "closed";
  closedHost.target.shadowRoot = null;
  assert.throws(
    () => acquireOutputLease(closedHost.target, ["--lab-c"], "test/closed-shadow"),
    (error) => error?.code === "OUTPUT_TARGET_CAPABILITY",
  );
});

test("one sheet merges in active-lease then manifest order; identical snapshots do zero work", () => {
  const { target, root, realm } = documentTarget();
  const first = acquireOutputLease(target, ["--lab-z", "--lab-a"], "test/order-a");
  const second = acquireOutputLease(target, ["--lab-c", "--lab-b"], "test/order-b");
  assert.equal(root.adoptedStyleSheets.length, 1);

  assert.equal(first.publish({ "--lab-a": "a", "--lab-z": "z" }), true);
  assert.equal(second.publish({ "--lab-b": "b", "--lab-c": "c" }), true);
  const live = liveSheet(root);
  assert.equal(
    sheetText(live),
    ":root { --lab-z: z; --lab-a: a; --lab-c: c; --lab-b: b; }",
  );

  const scratch = realm.sheets.find((sheet) => sheet !== live);
  const before = {
    liveCalls: live.replaceCalls.length,
    scratchCalls: scratch.replaceCalls.length,
    stamp: first.stamp,
  };
  let ownsChecks = 0;
  assert.equal(
    second.publish({ "--lab-c": "c", "--lab-b": "b" }, () => {
      ownsChecks++;
      return true;
    }),
    true,
  );
  assert.equal(ownsChecks, 2, "no-op still checks ownership before and after freshness");
  assert.equal(live.replaceCalls.length, before.liveCalls);
  assert.equal(scratch.replaceCalls.length, before.scratchCalls);
  assert.equal(first.stamp, before.stamp);

  live.forceText(":root { --lab-z: drift; }");
  assert.throws(
    () => second.publish({ "--lab-b": "b", "--lab-c": "c" }),
    (error) => error instanceof OutputTargetStaleError,
    "a semantic no-op must not skip stale-host validation",
  );
});

test("foreign adopted stylesheets retain identity, order, and bytes across the sink lifecycle", () => {
  const { target, root, realm } = documentTarget();
  const before = new realm.CSSStyleSheet();
  const after = new realm.CSSStyleSheet();
  before.replaceSync(":root { --foreign-before: one; }");
  after.replaceSync(":root { --foreign-after: two; }");
  root.adoptionControl.force([before]);

  const lease = acquireOutputLease(target, ["--lab-a"], "test/foreign");
  const live = liveSheet(root);
  root.adoptionControl.force([before, live, after]);
  assert.equal(lease.publish({ "--lab-a": "owned" }), true);
  assert.deepEqual(root.adoptedStyleSheets, [before, live, after]);
  assert.equal(sheetText(before), ":root { --foreign-before: one; }");
  assert.equal(sheetText(after), ":root { --foreign-after: two; }");

  assert.equal(lease.dispose(), true);
  assert.deepEqual(root.adoptedStyleSheets, [before, after]);
});

test("failed attachment restores baseline foreign order and preserves setter-time additions", () => {
  const { target, root, realm } = documentTarget();
  const first = new realm.CSSStyleSheet();
  const second = new realm.CSSStyleSheet();
  const added = new realm.CSSStyleSheet();
  root.adoptionControl.force([first, second]);
  let write = 0;
  root.adoptionControl.onSet = (next, store) => {
    write++;
    if (write === 1) {
      const owned = next.at(-1);
      store([added, second, owned]);
      return;
    }
    store(next);
  };

  assert.throws(
    () => acquireOutputLease(target, ["--lab-a"], "test/foreign-rollback"),
    (error) => error instanceof OutputTargetCapabilityError,
  );
  assert.deepEqual(
    root.adoptedStyleSheets,
    [first, second, added],
    "rollback restores the baseline first, then keeps new identities in observed order",
  );

  root.adoptionControl.onSet = null;
  const recovered = acquireOutputLease(target, ["--lab-a"], "test/foreign-rollback-retry");
  const live = liveSheet(root);
  assert.deepEqual(root.adoptedStyleSheets, [first, second, added, live]);
  assert.equal(recovered.publish({ "--lab-a": "owned" }), true);
});

test("binding and inline ownership conflicts fail before live mutation", () => {
  const { target, root } = documentTarget();
  assert.throws(
    () => acquireOutputLease(target, ["color"], "test/non-custom"),
    (error) => error instanceof OutputBindingError && error.code === "OUTPUT_BINDING_INVALID",
  );
  assert.equal(root.adoptedStyleSheets.length, 0);

  target.addInline("--lab-inline");
  assert.throws(
    () => acquireOutputLease(target, ["--lab-inline"], "test/inline"),
    (error) => error instanceof OutputInlineBindingConflictError,
  );
  target.removeInline("--lab-inline");

  const first = acquireOutputLease(target, ["--lab-a"], "test/owner-a");
  const live = liveSheet(root);
  assert.throws(
    () => acquireOutputLease(target, ["--lab-a"], "test/owner-b"),
    (error) => error instanceof OutputBindingConflictError,
  );
  assert.equal(root.adoptedStyleSheets.length, 1);
  assert.equal(live.replaceCalls.length, 0);

  assert.throws(
    () => first.publish({ "--lab-a": "a", "--lab-extra": "x" }),
    (error) => error instanceof OutputBindingError && error.code === "OUTPUT_VALUE_OUTSIDE_BINDINGS",
  );
  assert.equal(live.replaceCalls.length, 0);
});

test("a replacement rejected before mutation leaves bytes, logical values, and stamp unchanged", () => {
  const { target, root } = documentTarget();
  const lease = acquireOutputLease(target, ["--lab-a"], "test/raw-replace-failure");
  assert.equal(lease.publish({ "--lab-a": "old" }), true);
  const live = liveSheet(root);
  const before = sheetText(live);
  const stamp = lease.stamp;
  live.enqueue(() => {
    throw new Error("host rejected replacement");
  });

  assert.throws(() => lease.publish({ "--lab-a": "new" }), /host rejected replacement/u);
  assert.equal(sheetText(live), before);
  assert.equal(lease.stamp, stamp);
  assert.equal(lease.state, "active");
});

test("post-replace drift rolls bytes back exactly and does not commit logical state", () => {
  const { target, root } = documentTarget();
  const lease = acquireOutputLease(target, ["--lab-a"], "test/post-replace-rollback");
  assert.equal(lease.publish({ "--lab-a": "old" }), true);
  const live = liveSheet(root);
  const before = sheetText(live);
  const stamp = lease.stamp;
  live.enqueue(({ apply, sheet }) => {
    apply();
    sheet.forceText(":root { --lab-a: drift; }");
  });

  assert.throws(
    () => lease.publish({ "--lab-a": "new" }),
    (error) =>
      error instanceof OutputAtomicityViolationError &&
      error.code === "OUTPUT_ATOMICITY_VIOLATION",
  );
  assert.equal(sheetText(live), before);
  assert.equal(lease.stamp, stamp);
  assert.equal(lease.state, "active");

  assert.equal(lease.publish({ "--lab-a": "retry" }), true);
  assert.equal(sheetText(live), ":root { --lab-a: retry; }");
});

test("an unresolved byte restore remains reachable and is reconciled before retry", () => {
  const { target, root } = documentTarget();
  const lease = acquireOutputLease(target, ["--lab-a"], "test/restore-journal");
  assert.equal(lease.publish({ "--lab-a": "old" }), true);
  const live = liveSheet(root);
  const stamp = lease.stamp;
  live.enqueue(({ apply }) => {
    apply();
    throw new Error("mutated then threw");
  });
  live.enqueue(() => {
    throw new Error("rollback rejected");
  });

  assert.throws(
    () => lease.publish({ "--lab-a": "candidate" }),
    (error) => error instanceof OutputAtomicityViolationError,
  );
  assert.equal(sheetText(live), ":root { --lab-a: candidate; }");
  assert.equal(lease.stamp, stamp);
  assert.equal(lease.state, "active");

  assert.equal(lease.publish({ "--lab-a": "recovered" }), true);
  assert.equal(sheetText(live), ":root { --lab-a: recovered; }");
  assert.equal(root.adoptedStyleSheets.filter((sheet) => sheet === live).length, 1);
});

test("ownership cancellation is checked after pre-commit and post-replace freshness", () => {
  const { target, root } = documentTarget();
  const lease = acquireOutputLease(target, ["--lab-a"], "test/cancellation");
  const live = liveSheet(root);

  assert.equal(lease.publish({ "--lab-a": "first" }, () => false), false);
  assert.equal(live.replaceCalls.length, 0);
  assert.equal(lease.stamp, 0);

  let checks = 0;
  assert.equal(
    lease.publish({ "--lab-a": "first" }, () => ++checks === 1),
    false,
    "ownership lost after assertFresh must stop before live replace",
  );
  assert.equal(checks, 2);
  assert.equal(live.replaceCalls.length, 0);

  checks = 0;
  assert.equal(
    lease.publish({ "--lab-a": "first" }, () => ++checks < 3),
    false,
    "ownership lost after the post-replace guard must roll back",
  );
  assert.equal(checks, 3);
  assert.equal(sheetText(live), "");
  assert.equal(live.replaceCalls.length, 2, "candidate plus exact rollback");
  assert.equal(lease.stamp, 0);
});

test("scratch and live failures during revoke leave the lease active and retryable", () => {
  const { target, root, realm } = documentTarget();
  const first = acquireOutputLease(target, ["--lab-a"], "test/revoke-a");
  const second = acquireOutputLease(target, ["--lab-b"], "test/revoke-b");
  assert.equal(first.publish({ "--lab-a": "a" }), true);
  assert.equal(second.publish({ "--lab-b": "b" }), true);
  const live = liveSheet(root);
  const scratch = realm.sheets.find((sheet) => sheet !== live);
  const before = sheetText(live);
  const stamp = first.stamp;

  scratch.enqueue(() => {
    throw new Error("scratch rejected revoke");
  });
  assert.throws(
    () => first.dispose(),
    (error) =>
      error?.code === "OUTPUT_STYLESHEET_INVALID" &&
      error.cause?.message === "scratch rejected revoke",
  );
  assert.equal(first.state, "active");
  assert.equal(first.stamp, stamp);
  assert.equal(sheetText(live), before);

  live.enqueue(() => {
    throw new Error("live rejected revoke");
  });
  assert.throws(() => first.dispose(), /live rejected revoke/u);
  assert.equal(first.state, "active");
  assert.equal(first.stamp, stamp);
  assert.equal(sheetText(live), before);

  assert.equal(first.dispose(), true);
  assert.equal(first.state, "disposed");
  assert.equal(sheetText(live), ":root { --lab-b: b; }");
  assert.equal(second.dispose(), true);
  assert.deepEqual(root.adoptedStyleSheets, []);
});

test("successful publish and revoke each use one live replace; dormant reacquire reuses the sheet", () => {
  const { target, root } = documentTarget();
  const first = acquireOutputLease(target, ["--lab-a"], "test/live-budget-a");
  const second = acquireOutputLease(target, ["--lab-b"], "test/live-budget-b");
  const live = liveSheet(root);

  assert.equal(first.publish({ "--lab-a": "a" }), true);
  assert.equal(live.replaceCalls.length, 1);
  assert.equal(second.publish({ "--lab-b": "b" }), true);
  assert.equal(live.replaceCalls.length, 2);
  assert.equal(first.dispose(), true);
  assert.equal(live.replaceCalls.length, 3);
  assert.equal(first.publish({ "--lab-a": "x" }), false);
  assert.equal(first.dispose(), false);

  assert.equal(second.dispose(), true);
  assert.equal(live.replaceCalls.length, 4);
  assert.deepEqual(root.adoptedStyleSheets, []);

  const reacquired = acquireOutputLease(target, ["--lab-a"], "test/live-budget-reacquire");
  assert.equal(liveSheet(root), live);
  assert.equal(reacquired.dispose(), true, "an uncommitted last lease needs no live replace");
  assert.equal(live.replaceCalls.length, 4);
});

test("initial attach double failure retains an authority journal and retry never accumulates sheets", () => {
  const { target, root } = documentTarget();
  let write = 0;
  root.adoptionControl.onSet = (next, store) => {
    write++;
    if (write === 1) {
      store(next);
      throw new Error("attachment mutated then threw");
    }
    if (write === 2) throw new Error("attachment rollback rejected");
    store(next);
  };

  assert.throws(
    () => acquireOutputLease(target, ["--lab-a"], "test/attach-double-failure"),
    (error) => error instanceof OutputAtomicityViolationError,
  );
  assert.equal(root.adoptedStyleSheets.length, 1, "the residual effect is reachable, not hidden");
  const authority = sinkAuthority(target).value;
  assert.ok(Object.isFrozen(authority));

  root.adoptionControl.onSet = null;
  const recovered = acquireOutputLease(target, ["--lab-a"], "test/attach-retry");
  assert.equal(root.adoptedStyleSheets.length, 1);
  assert.equal(new Set(root.adoptedStyleSheets).size, 1);
  assert.equal(recovered.publish({ "--lab-a": "recovered" }), true);
});

test("cross-realm rehome double failure reconciles the residual candidate before retry", () => {
  const firstHost = documentTarget();
  const first = acquireOutputLease(firstHost.target, ["--lab-a"], "test/rehome-source");
  assert.equal(first.publish({ "--lab-a": "old" }), true);
  assert.equal(first.dispose(), true);
  assert.deepEqual(firstHost.root.adoptedStyleSheets, []);

  const nextRealm = makeRealm();
  const nextRoot = makeDocument(nextRealm);
  firstHost.target.moveToDocument(nextRoot);
  let write = 0;
  nextRoot.adoptionControl.onSet = (next, store) => {
    write++;
    if (write === 1) {
      store(next);
      throw new Error("rehome mutated then threw");
    }
    if (write === 2) throw new Error("rehome rollback rejected");
    store(next);
  };

  assert.throws(
    () => acquireOutputLease(firstHost.target, ["--lab-a"], "test/rehome-double-failure"),
    (error) => error instanceof OutputAtomicityViolationError,
  );
  assert.equal(nextRoot.adoptedStyleSheets.length, 1);

  nextRoot.adoptionControl.onSet = null;
  const recovered = acquireOutputLease(firstHost.target, ["--lab-a"], "test/rehome-retry");
  assert.equal(nextRoot.adoptedStyleSheets.length, 1);
  assert.equal(new Set(nextRoot.adoptedStyleSheets).size, 1);
  assert.equal(recovered.publish({ "--lab-a": "new" }), true);
  assert.equal(sheetText(liveSheet(nextRoot)), ":root { --lab-a: new; }");
});

test("the frozen target authority hides mutable state and independent copies delegate to it", async () => {
  const { target, root } = documentTarget();
  const first = acquireOutputLease(target, ["--lab-a"], "test/authority-a");
  const { symbol, value } = sinkAuthority(target);
  const descriptor = Object.getOwnPropertyDescriptor(target, symbol);
  assert.equal(descriptor.configurable, false);
  assert.equal(descriptor.writable, false);
  assert.ok(Object.isFrozen(value));
  assert.equal(Object.getPrototypeOf(value), null);
  assert.deepEqual(Reflect.ownKeys(value).sort(), ["acquire", "protocol"]);
  assert.equal("record" in value, false);
  assert.equal("targets" in value, false);
  assert.throws(() => {
    value.busy = false;
  }, TypeError);

  const copy = await import(`../output-sink.js?authority-copy=${Date.now()}`);
  assert.throws(
    () => copy.acquireOutputLease(target, ["--lab-a"], "test/authority-copy"),
    (error) => error?.code === "OUTPUT_BINDING_CONFLICT",
  );
  assert.equal(root.adoptedStyleSheets.length, 1);
  assert.equal(first.stamp, 0);
});

test("independent module reentrancy during attachment is busy and cannot create a second sheet", async () => {
  const { target, root } = documentTarget();
  const copy = await import(`../output-sink.js?reentrant-copy=${Date.now()}`);
  let nestedError;
  let once = true;
  root.adoptionControl.onSet = (next, store) => {
    store(next);
    if (!once || next.length === 0) return;
    once = false;
    try {
      copy.acquireOutputLease(target, ["--lab-b"], "test/nested-acquire");
    } catch (error) {
      nestedError = error;
    }
  };

  const lease = acquireOutputLease(target, ["--lab-a"], "test/outer-acquire");
  assert.ok(nestedError instanceof OutputSinkBusyError || nestedError?.code === "OUTPUT_SINK_BUSY");
  assert.equal(root.adoptedStyleSheets.length, 1);
  assert.equal(lease.publish({ "--lab-a": "a" }), true);
});

test("active publication fails stale before live mutation, while dispose retains stored cleanup authority", () => {
  const firstHost = documentTarget();
  const lease = acquireOutputLease(firstHost.target, ["--lab-a"], "test/stale-source");
  assert.equal(lease.publish({ "--lab-a": "old" }), true);
  const oldLive = liveSheet(firstHost.root);
  const calls = oldLive.replaceCalls.length;
  const stamp = lease.stamp;

  const nextRoot = makeDocument(makeRealm());
  firstHost.target.moveToDocument(nextRoot);
  assert.throws(
    () => lease.publish({ "--lab-a": "new" }),
    (error) => error instanceof OutputTargetStaleError,
  );
  assert.equal(oldLive.replaceCalls.length, calls);
  assert.equal(lease.stamp, stamp);

  assert.equal(lease.dispose(), true);
  assert.deepEqual(firstHost.root.adoptedStyleSheets, []);
  const reacquired = acquireOutputLease(firstHost.target, ["--lab-a"], "test/stale-rehome");
  assert.equal(reacquired.publish({ "--lab-a": "new" }), true);
});

test("an externally detached sheet remains explicitly revocable and reacquirable", () => {
  const { target, root } = documentTarget();
  const lease = acquireOutputLease(target, ["--lab-a"], "test/external-detach");
  assert.equal(lease.publish({ "--lab-a": "old" }), true);
  const live = liveSheet(root);
  root.adoptionControl.force([]);

  assert.throws(
    () => lease.publish({ "--lab-a": "new" }),
    (error) => error instanceof OutputTargetStaleError,
  );
  assert.equal(lease.dispose(), true);
  assert.equal(lease.state, "disposed");
  assert.equal(sheetText(live), "");
  assert.deepEqual(root.adoptedStyleSheets, []);

  const recovered = acquireOutputLease(target, ["--lab-a"], "test/external-detach-retry");
  assert.equal(liveSheet(root), live);
  assert.equal(recovered.publish({ "--lab-a": "recovered" }), true);
});
