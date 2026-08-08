import test from "node:test";
import assert from "node:assert/strict";

import { adaptTheme } from "../adapt-theme.js";
import {
  acquireOutputLease,
  OutputBindingConflictError,
  OutputInlineBindingConflictError,
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

function outputHost({ inline = [], consumerSheetText = null } = {}) {
  const sheetEvents = [];
  let beforeLiveReplace = null;

  class CSSStyleSheet {
    constructor() {
      this.text = "";
      this.cssRules = [];
      this.isAdopted = false;
    }

    replaceSync(text) {
      const rules = parseSheet(text);
      if (this.isAdopted && beforeLiveReplace) beforeLiveReplace(this, String(text));
      this.text = String(text);
      this.cssRules = rules;
      sheetEvents.push({ sheet: this, text: this.text, live: this.isAdopted });
    }
  }

  const attributes = new Map();
  const inlineProps = new Map(inline);
  const inlineMutations = [];
  const win = { CSSStyleSheet };
  let element;

  const makeRoot = () => {
    let adopted = [];
    const root = {
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
    };
    root.ownerDocument = root;
    return root;
  };

  let root = makeRoot();
  element = {
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
        return inlineProps.size;
      },
      item: (index) => [...inlineProps.keys()][index] ?? null,
      getPropertyValue: (name) => inlineProps.get(name) ?? "",
      setProperty(name, value) {
        inlineMutations.push(["set", name, value]);
        inlineProps.set(name, value);
      },
      removeProperty(name) {
        inlineMutations.push(["remove", name]);
        inlineProps.delete(name);
      },
    },
  };
  root.documentElement = element;

  let consumerSheet = null;
  if (consumerSheetText !== null) {
    consumerSheet = new CSSStyleSheet();
    consumerSheet.replaceSync(consumerSheetText);
    root.adoptedStyleSheets = [consumerSheet];
    sheetEvents.length = 0;
  }

  return {
    element,
    attributes,
    inlineProps,
    inlineMutations,
    sheetEvents,
    consumerSheet,
    root: () => root,
    liveSheet() {
      const owned = root.adoptedStyleSheets.filter((sheet) => sheet !== consumerSheet);
      assert.equal(owned.length, 1, "one target has exactly one Lab Colors sheet");
      return owned[0];
    },
    liveDeclarations() {
      const rule = this.liveSheet().cssRules[0];
      if (!rule) return {};
      return Object.fromEntries(
        Array.from({ length: rule.style.length }, (_, index) => {
          const name = rule.style.item(index);
          return [name, rule.style.getPropertyValue(name)];
        }),
      );
    },
    liveReplaceCount() {
      const sheet = this.liveSheet();
      return sheetEvents.filter((event) => event.live && event.sheet === sheet).length;
    },
    setBeforeLiveReplace(callback) {
      beforeLiveReplace = callback;
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

function adaptEngine(snapshot) {
  return {
    resolveTheme: () => snapshot,
    recheckContrast(_background, foregrounds) {
      return Array.from({ length: foregrounds.length }, () => [100, 10]).flat();
    },
  };
}

test("F-02: snapshots carry an exact outputBindings manifest before a sink is acquired", () => {
  const malformed = [
    { vars: { "--lab-a": "#111111" }, roles: {} },
    colorSnapshot({ "--lab-a": "#111111" }, ["--lab-a", "--lab-a"]),
    colorSnapshot({ "--lab-a": "#111111" }, ["--lab-b"]),
  ];

  for (const result of malformed) {
    const host = outputHost();
    assert.throws(
      () =>
        watchTheme(host.element, {
          colors: { resolveTheme: () => result },
          theme: "light",
          background: "#FFFFFF",
          observe: false,
          target: host.element,
        }),
      /outputBindings|undeclared|subset/u,
    );
    assert.deepEqual(host.root().adoptedStyleSheets, []);
    assert.equal(host.attributes.size, 0);
    assert.deepEqual(host.inlineMutations, []);
  }
});

test("F-02: disjoint watch/adapt attachments coexist and dispose revokes only its lease", () => {
  const consumerText = "[data-consumer] { --lab-consumer-sheet: #ABCDEF; }";
  const host = outputHost({
    inline: [["--lab-consumer-inline", "#FEDCBA"]],
    consumerSheetText: consumerText,
  });
  const watch = watchTheme(host.element, {
    colors: { resolveTheme: () => colorSnapshot({ "--lab-watch": "#111111" }) },
    theme: "light",
    background: "#FFFFFF",
    observe: false,
    target: host.element,
  });
  const adapt = adaptTheme(host.element, {
    colors: adaptEngine(colorSnapshot({ "--lab-adapt": "#222222" })),
    theme: "light",
    background: "#FFFFFF",
    target: host.element,
    now: () => 1,
    win: {},
  });

  try {
    assert.deepEqual(host.liveDeclarations(), {
      "--lab-watch": "#111111",
      "--lab-adapt": "#222222",
    });
    assert.equal(host.inlineProps.get("--lab-consumer-inline"), "#FEDCBA");
    assert.deepEqual(host.inlineMutations, []);
    assert.equal(host.consumerSheet.text, consumerText);
    assert.ok(host.root().adoptedStyleSheets.includes(host.consumerSheet));

    watch.dispose();
    assert.deepEqual(host.liveDeclarations(), { "--lab-adapt": "#222222" });
    assert.equal(host.inlineProps.get("--lab-consumer-inline"), "#FEDCBA");
    assert.equal(host.consumerSheet.text, consumerText);
  } finally {
    watch.dispose();
    adapt.dispose();
  }
});

test("F-02: an overlapping lease fails before target or live stylesheet mutation", () => {
  const host = outputHost();
  const first = acquireOutputLease(host.element, ["--lab-shared"], "first attachment");
  first.publish({ "--lab-shared": "#111111" });
  const before = {
    text: host.liveSheet().text,
    stamp: first.stamp,
    replaceCount: host.liveReplaceCount(),
    adopted: [...host.root().adoptedStyleSheets],
    attributes: [...host.attributes],
  };

  assert.throws(
    () => acquireOutputLease(host.element, ["--lab-shared", "--lab-other"], "second attachment"),
    (error) => {
      assert.ok(error instanceof OutputBindingConflictError);
      assert.equal(error.code, "OUTPUT_BINDING_CONFLICT");
      assert.deepEqual(error.bindings, ["--lab-shared"]);
      return true;
    },
  );
  assert.deepEqual(
    {
      text: host.liveSheet().text,
      stamp: first.stamp,
      replaceCount: host.liveReplaceCount(),
      adopted: [...host.root().adoptedStyleSheets],
      attributes: [...host.attributes],
    },
    before,
  );
  first.dispose();
});

test("F-02: an owned inline declaration is a typed conflict before sink acquisition", () => {
  const host = outputHost({
    inline: [
      ["--lab-owned", "#111111"],
      ["--lab-consumer", "#222222"],
    ],
  });

  assert.throws(
    () => acquireOutputLease(host.element, ["--lab-owned"], "inline ownership"),
    (error) => {
      assert.ok(error instanceof OutputInlineBindingConflictError);
      assert.equal(error.code, "OUTPUT_INLINE_BINDING_CONFLICT");
      assert.deepEqual(error.bindings, ["--lab-owned"]);
      return true;
    },
  );
  assert.deepEqual(host.root().adoptedStyleSheets, []);
  assert.equal(host.attributes.size, 0);
  assert.deepEqual(host.inlineMutations, []);
  assert.deepEqual(Object.fromEntries(host.inlineProps), {
    "--lab-owned": "#111111",
    "--lab-consumer": "#222222",
  });
});

test("F-02: aliases are one exact lease and one whole-snapshot writer", () => {
  const host = outputHost();
  // Primary precedes its alias by the Core manifest even when lexical order is
  // the opposite. The sink may sort a private merge index, never this SSOT.
  const bindings = ["--lab-z-accent", "--lab-a-accent-alias"];
  const lease = acquireOutputLease(host.element, bindings, "aliased output");
  const before = host.liveReplaceCount();

  assert.equal(
    lease.publish({
      "--lab-z-accent": "oklch(60% 0.1 250)",
      "--lab-a-accent-alias": "oklch(60% 0.1 250)",
    }),
    true,
  );

  assert.deepEqual(lease.outputBindings, bindings);
  assert.equal(Object.isFrozen(lease.outputBindings), true);
  assert.equal(host.liveReplaceCount() - before, 1, "aliases share one live publication");
  assert.deepEqual(host.liveDeclarations(), {
    "--lab-a-accent-alias": "oklch(60% 0.1 250)",
    "--lab-z-accent": "oklch(60% 0.1 250)",
  });
  lease.dispose();
});

test("F-02: a disposed generation cannot overwrite a reattached generation", () => {
  const host = outputHost();
  const retired = acquireOutputLease(host.element, ["--lab-a"], "generation A");
  retired.publish({ "--lab-a": "#111111" });
  assert.equal(retired.dispose(), true);
  assert.equal(retired.state, "disposed");

  const current = acquireOutputLease(host.element, ["--lab-a"], "generation B");
  current.publish({ "--lab-a": "#BBBBBB" });
  const before = {
    text: host.liveSheet().text,
    stamp: current.stamp,
    count: host.liveReplaceCount(),
  };

  assert.equal(retired.publish({ "--lab-a": "#AAAAAA" }), false);
  assert.deepEqual(
    { text: host.liveSheet().text, stamp: current.stamp, count: host.liveReplaceCount() },
    before,
  );
  assert.deepEqual(host.liveDeclarations(), { "--lab-a": "#BBBBBB" });
  current.dispose();
});

test("F-02: a cancelled first adaptive manifest neither pins bindings nor retains a blank lease", () => {
  const host = outputHost();
  let pointAvailable = false;
  let controller;
  let reentered = false;
  const glowVar = "--lab-glow";
  const glowBindings = [glowVar, `${glowVar}-core`, `${glowVar}-alpha`];
  const glow = {
    outputBindings: glowBindings,
    vars: {
      [glowVar]: "oklch(70% 0.1 280)",
      [`${glowVar}-core`]: "oklch(80% 0.1 280)",
      [`${glowVar}-alpha`]: "0.5",
    },
    roles: {
      glow: {
        kind: "glow",
        cssVar: glowVar,
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
  const next = colorSnapshot({ "--lab-b-label": "#123456" });
  const calls = [];
  const colors = {
    resolveTheme(_background, theme) {
      calls.push(theme);
      return theme === "next" ? next : glow;
    },
    recheckContrast(_background, foregrounds) {
      return Array.from({ length: foregrounds.length }, () => [100, 10]).flat();
    },
    isStableGlowPointNoop() {
      if (!reentered) {
        reentered = true;
        controller.setTheme("next");
      }
      return true;
    },
  };
  const getStyle = () => ({
    getPropertyValue(property) {
      if (property === "background-color") {
        return pointAvailable ? "rgb(255, 255, 255)" : "transparent";
      }
      return "";
    },
  });

  controller = adaptTheme(host.element, {
    colors,
    theme: "initial",
    target: host.element,
    getStyle,
    parentOf: () => null,
    now: () => 0,
    win: {},
  });
  assert.deepEqual(controller.current(), {});
  assert.deepEqual(host.root().adoptedStyleSheets, []);
  assert.equal(host.attributes.size, 0);

  pointAvailable = true;
  controller.tick(1);

  assert.equal(reentered, true, "Glow seam must revoke the admitted first candidate");
  assert.deepEqual(calls, ["initial", "next"]);
  assert.deepEqual(controller.current(), { "--lab-b-label": "#123456" });
  assert.deepEqual(host.liveDeclarations(), { "--lab-b-label": "#123456" });
  assert.equal(Object.keys(host.liveDeclarations()).some((name) => glowBindings.includes(name)), false);
  assert.equal(host.root().adoptedStyleSheets.length, 1, "only the winning generation owns a sheet");

  controller.dispose();
  assert.deepEqual(host.root().adoptedStyleSheets, []);
  assert.equal(host.attributes.size, 0);
});
