import test from "node:test";
import assert from "node:assert/strict";

import { brandFakeDocument, brandFakeElement } from "./fake-node-brand.mjs";
import { adaptTheme } from "../adapt-theme.js";
import {
  acquireOutputLease,
  OutputSinkBusyError,
  OutputTargetStaleError,
} from "../output-sink.js";
import { watchTheme } from "../watch-theme.js";

function parseSheet(text) {
  const source = String(text);
  if (source.trim() === "") return [];
  const match = /^\s*([^{}]+)\s*\{([\s\S]*)\}\s*$/u.exec(source);
  if (!match) throw new SyntaxError(`invalid stylesheet '${source}'`);
  const names = [];
  const values = new Map();
  for (const declaration of match[2].split(";")) {
    if (declaration.trim() === "") continue;
    const colon = declaration.indexOf(":");
    if (colon <= 0) throw new SyntaxError(`invalid declaration '${declaration}'`);
    const name = declaration.slice(0, colon).trim();
    names.push(name);
    values.set(name, declaration.slice(colon + 1).trim());
  }
  const style = {
    get length() {
      return names.length;
    },
    item(index) {
      return names[index] ?? "";
    },
    getPropertyValue(name) {
      return values.get(name) ?? "";
    },
    getPropertyPriority() {
      return "";
    },
    setProperty(name, value) {
      if (!/^--[-_a-zA-Z0-9]+$/u.test(name)) return;
      if (!values.has(name)) names.push(name);
      values.set(name, String(value).trim());
    },
    removeProperty(name) {
      const value = values.get(name) ?? "";
      values.delete(name);
      const index = names.indexOf(name);
      if (index >= 0) names.splice(index, 1);
      return value;
    },
    get cssText() {
      return names.map((name) => `${name}: ${values.get(name)};`).join(" ");
    },
  };
  const selectorText = match[1].trim();
  const rule = {
    selectorText,
    style,
    get cssText() {
      return style.length === 0 ? `${selectorText} {}` : `${selectorText} { ${style.cssText} }`;
    },
  };
  const rules = [rule];
  rules.item = (index) => rules[index] ?? null;
  return rules;
}

function outputHost({ forbidInlineWrites = false } = {}) {
  const sheetEvents = [];
  const attributes = new Map();
  const inlineMutations = [];
  let beforeLiveReplace = null;
  let liveFailure = null;

  class CSSStyleSheet {
    constructor() {
      this.text = "";
      this.cssRules = [];
      this.isAdopted = false;
    }

    replaceSync(text) {
      const source = String(text);
      const rules = parseSheet(source);
      if (this.isAdopted) {
        if (liveFailure) {
          const failure = liveFailure;
          liveFailure = null;
          throw failure;
        }
        if (beforeLiveReplace) beforeLiveReplace(this, source);
      }
      this.text = source;
      this.cssRules = rules;
      sheetEvents.push({ sheet: this, text: source, live: this.isAdopted });
    }
  }

  const win = { CSSStyleSheet };
  let element;
  const makeRoot = () => {
    let adopted = [];
    const root = brandFakeDocument({
      nodeType: 9,
      defaultView: win,
      querySelectorAll() {
        throw new Error("identity-native sink must not scan the root");
      },
      get adoptedStyleSheets() {
        return adopted;
      },
      set adoptedStyleSheets(next) {
        adopted = [...next];
        for (const sheet of adopted) sheet.isAdopted = true;
      },
    });
    root.ownerDocument = root;
    return root;
  };

  let root = makeRoot();
  element = brandFakeElement({
    nodeType: 1,
    isConnected: true,
    ownerDocument: root,
    getRootNode: () => root,
    hasAttribute() {
      throw new Error("identity-native sink must not read marker attributes");
    },
    getAttribute: (name) => attributes.get(name) ?? null,
    setAttribute() {
      throw new Error("identity-native sink must not write marker attributes");
    },
    removeAttribute() {
      throw new Error("identity-native sink must not remove marker attributes");
    },
    style: {
      get length() {
        return 0;
      },
      item: () => null,
      getPropertyValue: () => "",
      setProperty(name, value) {
        inlineMutations.push(["set", name, value]);
        if (forbidInlineWrites) throw new Error("sabotage: direct inline setProperty");
      },
      removeProperty(name) {
        inlineMutations.push(["remove", name]);
        if (forbidInlineWrites) throw new Error("sabotage: direct inline removeProperty");
      },
    },
  });
  root.documentElement = element;

  return {
    element,
    attributes,
    inlineMutations,
    sheetEvents,
    root: () => root,
    liveSheet() {
      assert.equal(root.adoptedStyleSheets.length, 1, "one target has one live sheet");
      return root.adoptedStyleSheets[0];
    },
    declarations(sheet = this.liveSheet()) {
      const rule = sheet.cssRules[0];
      if (!rule) return {};
      return Object.fromEntries(
        Array.from({ length: rule.style.length }, (_, index) => {
          const name = rule.style.item(index);
          return [name, rule.style.getPropertyValue(name)];
        }),
      );
    },
    liveReplaceCount(sheet = this.liveSheet()) {
      return sheetEvents.filter((event) => event.live && event.sheet === sheet).length;
    },
    armLiveFailure(message = "host atomic replace failed") {
      liveFailure = new Error(message);
    },
    setBeforeLiveReplace(callback) {
      beforeLiveReplace = callback;
    },
    detach() {
      element.isConnected = false;
    },
    moveToFreshDocument() {
      root = makeRoot();
      root.documentElement = element;
      element.ownerDocument = root;
      element.isConnected = true;
      return root;
    },
  };
}

function colorSnapshot(vars, outputBindings = Object.keys(vars)) {
  return {
    outputBindings,
    vars,
    roles: Object.fromEntries(
      Object.entries(vars).map(([cssVar, hex]) => [
        cssVar.slice(2),
        { kind: "color", cssVar, hex, lc: 100 },
      ]),
    ),
  };
}

function captureSinkState(host, lease, controllerState = undefined) {
  const sheet = host.liveSheet();
  return {
    bytes: sheet.text,
    declarations: host.declarations(sheet),
    stamp: lease?.stamp,
    leaseState: lease?.state,
    controllerState,
    liveEffects: host.liveReplaceCount(sheet),
  };
}

function assertSinkStateUnchanged(before, after) {
  assert.equal(after.bytes, before.bytes, "live stylesheet bytes changed");
  assert.deepEqual(after.declarations, before.declarations, "live declarations changed");
  assert.equal(after.stamp, before.stamp, "sink stamp changed");
  assert.equal(after.leaseState, before.leaseState, "lease state changed");
  assert.deepEqual(after.controllerState, before.controllerState, "controller state changed");
  assert.equal(after.liveEffects, before.liveEffects, "an extra live publication occurred");
}

test("F-03: one successful snapshot causes one live effect and one stamp transition", () => {
  const host = outputHost();
  const lease = acquireOutputLease(host.element, ["--lab-a", "--lab-b"], "atomic effect");
  const beforeStamp = lease.stamp;
  const beforeEffects = host.liveReplaceCount();

  assert.equal(lease.publish({ "--lab-a": "#AAAAAA", "--lab-b": "#BBBBBB" }), true);

  assert.equal(host.liveReplaceCount() - beforeEffects, 1);
  assert.equal(lease.stamp, beforeStamp + 1);
  assert.deepEqual(host.declarations(), { "--lab-a": "#AAAAAA", "--lab-b": "#BBBBBB" });
  lease.dispose();
});

test("F-03: a failed live replace preserves bytes, stamp and lease state", () => {
  const host = outputHost();
  const lease = acquireOutputLease(host.element, ["--lab-a", "--lab-b"], "atomic failure");
  lease.publish({ "--lab-a": "#111111", "--lab-b": "#222222" });
  const before = captureSinkState(host, lease);

  host.armLiveFailure();
  assert.throws(
    () => lease.publish({ "--lab-a": "#AAAAAA", "--lab-b": "#BBBBBB" }),
    /host atomic replace failed/u,
  );

  assertSinkStateUnchanged(before, captureSinkState(host, lease));
  lease.dispose();
});

test("F-03: controller state commits only after a successful sink commit", () => {
  const host = outputHost();
  let background = "#FFFFFF";
  const colors = {
    resolveTheme: (bg) =>
      colorSnapshot({ "--lab-label": bg === "#FFFFFF" ? "#111111" : "#EEEEEE" }),
  };
  const controller = watchTheme(host.element, {
    colors,
    theme: "light",
    background: () => background,
    observe: false,
    target: host.element,
  });
  const sheet = host.liveSheet();
  const before = {
    bytes: sheet.text,
    declarations: host.declarations(sheet),
    background: controller.background(),
    liveEffects: host.liveReplaceCount(sheet),
  };

  background = "#000000";
  host.armLiveFailure("watch publication failed");
  assert.throws(() => controller.refresh(), /watch publication failed/u);

  assert.deepEqual(
    {
      bytes: sheet.text,
      declarations: host.declarations(sheet),
      background: controller.background(),
      liveEffects: host.liveReplaceCount(sheet),
    },
    before,
  );
  controller.dispose();
});

test("F-03: a successful adaptive replace linearizes before a newer queued intent fails", () => {
  const host = outputHost();
  const values = {
    initial: "#111111",
    admitted: "#AAAAAA",
    newer: "#EEEEEE",
  };
  const calls = [];
  const controller = adaptTheme(host.element, {
    colors: {
      resolveTheme(_background, theme) {
        calls.push(theme);
        return colorSnapshot({ "--lab-label": values[theme] });
      },
      recheckContrast(_background, foregrounds) {
        return Array.from({ length: foregrounds.length }, () => [100, 10]).flat();
      },
    },
    theme: "initial",
    background: "#FFFFFF",
    target: host.element,
    now: () => 1,
    win: {},
  });
  const before = host.declarations();
  const beforeEffects = host.liveReplaceCount();
  let phase = "queue-newer";
  host.setBeforeLiveReplace(() => {
    if (phase === "queue-newer") {
      phase = "fail-newer";
      controller.setTheme("newer");
      return;
    }
    if (phase === "fail-newer") {
      phase = "failed";
      throw new Error("queued newer publication failed");
    }
  });

  assert.throws(
    () => controller.setTheme("admitted"),
    /queued newer publication failed/u,
  );

  assert.equal(phase, "failed", "both the successful and failing live effects must execute");
  assert.deepEqual(calls, ["initial", "admitted", "newer"]);
  assert.notDeepEqual(host.declarations(), before, "the successful first replace must not roll back");
  assert.deepEqual(host.declarations(), { "--lab-label": values.admitted });
  assert.deepEqual(controller.current(), { "--lab-label": values.admitted });
  assert.equal(
    host.liveReplaceCount() - beforeEffects,
    1,
    "the rejected newer intent must not add a live publication",
  );

  host.setBeforeLiveReplace(null);
  controller.dispose();
});

test("F-03: reentrant publish is typed busy while the admitted whole snapshot commits", () => {
  const host = outputHost();
  const lease = acquireOutputLease(host.element, ["--lab-a"], "reentrant publish");
  let nestedError = null;
  let reentered = false;
  host.setBeforeLiveReplace(() => {
    if (reentered) return;
    reentered = true;
    try {
      lease.publish({ "--lab-a": "#BBBBBB" });
    } catch (error) {
      nestedError = error;
    }
  });
  const beforeEffects = host.liveReplaceCount();

  assert.equal(lease.publish({ "--lab-a": "#AAAAAA" }), true);

  assert.equal(reentered, true, "sabotage hook must run inside the live effect");
  assert.ok(nestedError instanceof OutputSinkBusyError);
  assert.equal(nestedError.code, "OUTPUT_SINK_BUSY");
  assert.equal(host.liveReplaceCount() - beforeEffects, 1);
  assert.deepEqual(host.declarations(), { "--lab-a": "#AAAAAA" });
  host.setBeforeLiveReplace(null);
  lease.dispose();
});

test("F-03: authority cancelled during scratch preparation cannot reach the live sheet", () => {
  const host = outputHost();
  const lease = acquireOutputLease(host.element, ["--lab-a", "--lab-b"], "cancel prepare");
  lease.publish({ "--lab-a": "#111111", "--lab-b": "#222222" });
  const before = captureSinkState(host, lease);
  let ownershipChecks = 0;

  assert.equal(
    lease.publish(
      { "--lab-a": "#AAAAAA", "--lab-b": "#BBBBBB" },
      () => ++ownershipChecks === 1,
    ),
    false,
  );
  assert.equal(
    ownershipChecks,
    2,
    "authority must be checked before and after scratch preparation",
  );
  assertSinkStateUnchanged(before, captureSinkState(host, lease));
  lease.dispose();
});

test("F-03: dispose requested reentrantly cannot split an in-flight publication", () => {
  const host = outputHost();
  const lease = acquireOutputLease(host.element, ["--lab-a"], "reentrant dispose");
  lease.publish({ "--lab-a": "#111111" });
  let nestedError = null;
  let reentered = false;
  host.setBeforeLiveReplace(() => {
    if (reentered) return;
    reentered = true;
    try {
      lease.dispose();
    } catch (error) {
      nestedError = error;
    }
  });
  const beforeEffects = host.liveReplaceCount();

  assert.equal(lease.publish({ "--lab-a": "#AAAAAA" }), true);

  assert.ok(nestedError instanceof OutputSinkBusyError);
  assert.equal(nestedError.code, "OUTPUT_SINK_BUSY");
  assert.equal(lease.state, "active");
  assert.equal(host.liveReplaceCount() - beforeEffects, 1);
  assert.deepEqual(host.declarations(), { "--lab-a": "#AAAAAA" });
  host.setBeforeLiveReplace(null);
  assert.equal(lease.dispose(), true);
  assert.equal(lease.state, "disposed");
});

test("F-03: a queued refresh cannot publish after dispose", async () => {
  const host = outputHost();
  let background = "#FFFFFF";
  let observerCallback = null;
  let resolveCount = 0;
  const controller = watchTheme(host.element, {
    colors: {
      resolveTheme(bg) {
        resolveCount++;
        return colorSnapshot({ "--lab-a": bg === "#FFFFFF" ? "#111111" : "#EEEEEE" });
      },
    },
    theme: "light",
    background: () => background,
    target: host.element,
    win: {
      MutationObserver: function (callback) {
        observerCallback = callback;
        return { observe() {}, disconnect() {} };
      },
      document: { documentElement: {} },
      queueMicrotask,
    },
  });
  assert.equal(resolveCount, 1);
  const live = host.liveSheet();

  background = "#000000";
  observerCallback();
  controller.dispose();
  const afterDispose = {
    bytes: live.text,
    effects: host.liveReplaceCount(live),
    resolveCount,
    adopted: host.root().adoptedStyleSheets.length,
  };
  await Promise.resolve();
  await Promise.resolve();

  assert.deepEqual(
    {
      bytes: live.text,
      effects: host.liveReplaceCount(live),
      resolveCount,
      adopted: host.root().adoptedStyleSheets.length,
    },
    afterDispose,
  );
});

test("F-03: dispose from queued-stop cleanup revokes the lease exactly once", () => {
  const host = outputHost();
  let background = "#FFFFFF";
  let controller;
  let disconnectCalls = 0;
  const win = {
    MutationObserver: function () {
      return {
        observe() {},
        disconnect() {
          disconnectCalls++;
          controller.dispose();
        },
      };
    },
    document: { documentElement: {} },
    queueMicrotask,
  };
  controller = watchTheme(host.element, {
    colors: {
      resolveTheme(bg) {
        return colorSnapshot({ "--lab-a": bg === "#FFFFFF" ? "#111111" : "#EEEEEE" });
      },
    },
    theme: "light",
    background: () => background,
    target: host.element,
    win,
  });
  const live = host.liveSheet();
  const beforeEffects = host.liveReplaceCount(live);
  let stopQueued = false;
  host.setBeforeLiveReplace(() => {
    if (stopQueued) return;
    stopQueued = true;
    controller.stop();
  });

  background = "#000000";
  controller.refresh();

  assert.equal(stopQueued, true, "stop must be queued from inside the live publication");
  assert.equal(disconnectCalls, 1, "observer ownership is released exactly once");
  assert.equal(
    host.liveReplaceCount(live) - beforeEffects,
    2,
    "one refresh publication and one lease revocation must occur",
  );
  assert.equal(live.text, "", "dispose revokes the complete owned snapshot");
  assert.deepEqual(live.cssRules, []);
  assert.deepEqual(host.root().adoptedStyleSheets, []);
  assert.equal(host.attributes.size, 0);

  const terminal = {
    bytes: live.text,
    effects: host.liveReplaceCount(live),
    adopted: [...host.root().adoptedStyleSheets],
    attributes: [...host.attributes],
  };
  controller.dispose();
  controller.refresh(true);
  controller.setTheme("dark");
  assert.deepEqual(
    {
      bytes: live.text,
      effects: host.liveReplaceCount(live),
      adopted: [...host.root().adoptedStyleSheets],
      attributes: [...host.attributes],
    },
    terminal,
    "terminal controller operations are idempotent and cannot recreate output",
  );
});

test("F-03: direct stop keeps reentrant dispose cleanup retryable after disconnect fails", async (t) => {
  for (const retryWith of ["dispose", "stop"]) {
    await t.test(`retry with ${retryWith}`, () => {
      const host = outputHost();
      const cleanupFailure = new Error("reentrant observer disconnect failed");
      let controller;
      let disconnectCalls = 0;
      let disconnectDepth = 0;
      let maxDisconnectDepth = 0;
      let disconnected = false;
      const win = {
        MutationObserver: function () {
          return {
            observe() {},
            disconnect() {
              disconnectCalls++;
              disconnectDepth++;
              maxDisconnectDepth = Math.max(maxDisconnectDepth, disconnectDepth);
              try {
                if (disconnectCalls === 1) {
                  controller.dispose();
                  throw cleanupFailure;
                }
                disconnected = true;
              } finally {
                disconnectDepth--;
              }
            },
          };
        },
        document: { documentElement: {} },
        queueMicrotask,
      };
      controller = watchTheme(host.element, {
        colors: {
          resolveTheme() {
            return colorSnapshot({ "--lab-a": "#111111" });
          },
        },
        theme: "light",
        background: "#FFFFFF",
        target: host.element,
        win,
      });
      const live = host.liveSheet();

      assert.throws(() => controller.stop(), (error) => error === cleanupFailure);
      assert.equal(disconnectCalls, 1);
      assert.equal(maxDisconnectDepth, 1, "reentrant dispose must not double-disconnect");
      assert.equal(disconnected, false);
      assert.equal(live.text, "", "reentrant dispose must still revoke the output lease");
      assert.deepEqual(host.root().adoptedStyleSheets, []);
      assert.equal(host.attributes.size, 0);
      const revokeEffects = host.liveReplaceCount(live);

      assert.doesNotThrow(() => controller[retryWith]());
      assert.equal(disconnectCalls, 2, `${retryWith} must retry the retained observer handle`);
      assert.equal(maxDisconnectDepth, 1);
      assert.equal(disconnected, true);
      assert.equal(
        host.liveReplaceCount(live),
        revokeEffects,
        "observer retry must not revoke the already-released output twice",
      );

      controller.dispose();
      controller.stop();
      assert.equal(disconnectCalls, 2, "terminal cleanup must become idempotent");
    });
  }
});

test("F-03: detach or cross-root move is typed stale before live mutation", () => {
  for (const mutation of ["detach", "move"]) {
    const host = outputHost();
    const lease = acquireOutputLease(host.element, ["--lab-a"], mutation);
    lease.publish({ "--lab-a": "#111111" });
    const originalRoot = host.root();
    const live = host.liveSheet();
    const before = {
      bytes: live.text,
      stamp: lease.stamp,
      effects: host.liveReplaceCount(live),
    };
    if (mutation === "detach") host.detach();
    else host.moveToFreshDocument();

    assert.throws(
      () => lease.publish({ "--lab-a": "#AAAAAA" }),
      (error) => {
        assert.ok(error instanceof OutputTargetStaleError);
        assert.ok(typeof error.code === "string" && error.code.length > 0);
        return true;
      },
      mutation,
    );
    assert.deepEqual(
      { bytes: live.text, stamp: lease.stamp, effects: host.liveReplaceCount(live) },
      before,
    );
    if (mutation === "move") {
      assert.deepEqual(host.root().adoptedStyleSheets, []);
      assert.ok(originalRoot.adoptedStyleSheets.includes(live));
    }
  }
});

test("F-03: adaptive ease frames use one atomic sink publication and no inline writer", () => {
  const host = outputHost({ forbidInlineWrites: true });
  const keys = ["--lab-p", "--lab-q"];
  let now = 1000;
  let background = "#FFFFFF";
  let metrics = [100, 100];
  let resolved = colorSnapshot({ "--lab-p": "#000000", "--lab-q": "#000000" }, keys);
  let resolveCount = 0;
  const colors = {
    resolveTheme() {
      resolveCount++;
      return resolved;
    },
    recheckContrast() {
      return metrics.flatMap((value) => [value, 10]);
    },
  };
  const controller = adaptTheme(host.element, {
    colors,
    theme: "light",
    background: () => background,
    target: host.element,
    now: () => now,
    win: {},
    sustainMs: 100,
    dwellMs: 100,
    easeMs: 1000,
  });

  metrics = [10, 10];
  background = "#202020";
  controller.tick();
  resolved = colorSnapshot({ "--lab-p": "#F0F0F0", "--lab-q": "#F0F0F0" }, keys);
  now = 1300;
  background = "#202021";
  controller.tick();
  metrics = [100, 100];
  now = 1400;
  background = "#202022";
  const beforeEffects = host.liveReplaceCount();
  controller.tick();

  assert.equal(resolveCount, 2, "the fixture must enter a real ease after a stable breach");
  assert.equal(host.liveReplaceCount() - beforeEffects, 1);
  assert.deepEqual(Object.keys(host.declarations()), keys);
  assert.deepEqual(host.inlineMutations, []);
  controller.dispose();
});

test("F-03: stable Glow class changes use the same one-effect atomic sink", () => {
  const host = outputHost({ forbidInlineWrites: true });
  const cssVar = "--lab-fx";
  const outputBindings = [cssVar, `${cssVar}-core`, `${cssVar}-alpha`];
  const determinate = {
    outputBindings,
    vars: {
      [cssVar]: "oklch(70% 0.1 280)",
      [`${cssVar}-core`]: "oklch(80% 0.1 280)",
      [`${cssVar}-alpha`]: "0.5",
    },
    roles: {
      fx: {
        kind: "glow",
        cssVar,
        coreHex: "#D8CEFF",
        haloHex: "#C0B2FA",
        decisionProfile: "stable-v1",
        decisionGuarantee: { kind: "bit-exact" },
        compositeProfile: "encoded-srgb8-screen-v1",
        compositeGuarantee: "bit-exact",
        layerRecipeProfile: "cam16-jprime-oklab-cusp-v1",
        appearanceDiagnosticProfile: "cam16-ucs-jprime-li2017-v1",
        selectionDiagnosticProfile: null,
        constraintLayer: "halo",
        targetStatus: "exact-noop-unreachable",
      },
    },
  };
  const indeterminate = {
    outputBindings,
    vars: {},
    roles: {
      fx: {
        kind: "glow-indeterminate",
        cssVar,
        sourceHex: "#C0B2FA",
        decisionProfile: "stable-v1",
        numericalSiteId: "glow-target-or-maximum-v1",
        constraintLayer: "halo",
        reason: "sound-bound-unavailable",
        bounds: { kind: "unavailable" },
      },
    },
  };
  let background = "#FFFFFF";
  let resolveCount = 0;
  const controller = adaptTheme(host.element, {
    colors: {
      resolveTheme(bg) {
        resolveCount++;
        return bg === "#FFFFFF" ? determinate : indeterminate;
      },
      recheckContrast: () => [],
      isStableGlowPointNoop(_source, bg) {
        return bg === "#FFFFFF";
      },
    },
    theme: "light",
    background: () => background,
    target: host.element,
    now: () => 1000,
    win: {},
    sustainMs: 10_000,
    dwellMs: 10_000,
  });

  background = "#FEFEFE";
  const beforeEffects = host.liveReplaceCount();
  controller.tick();

  assert.equal(resolveCount, 2, "the fixture must execute stable Glow reconciliation");
  assert.equal(host.liveReplaceCount() - beforeEffects, 1);
  assert.deepEqual(host.declarations(), {});
  assert.deepEqual(host.inlineMutations, []);
  controller.dispose();
});

test("F-03 anti-vacuity: the unchanged-state oracle rejects a one-declaration hybrid", () => {
  const before = {
    bytes: "[x] { --lab-a: #111111; --lab-b: #222222; }",
    declarations: { "--lab-a": "#111111", "--lab-b": "#222222" },
    stamp: 7,
    leaseState: "active",
    controllerState: { background: "#FFFFFF" },
    liveEffects: 1,
  };
  const sabotaged = {
    ...before,
    bytes: "[x] { --lab-a: #AAAAAA; --lab-b: #222222; }",
    declarations: { "--lab-a": "#AAAAAA", "--lab-b": "#222222" },
  };

  assert.throws(
    () => assertSinkStateUnchanged(before, sabotaged),
    /live stylesheet bytes changed/u,
  );
});
