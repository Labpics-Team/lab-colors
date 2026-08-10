import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import "./fake-node-brand.mjs";
import { initSync } from "../pkg/labcolors.js";
import { observePointBackground } from "../background-observation.js";
import { watchTheme } from "../watch-theme.js";
import { adaptTheme } from "../adapt-theme.js";
import { outputElement } from "./output-host.mjs";

initSync({
  module: new WebAssembly.Module(readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url))),
});

const INITIALS = Object.freeze({
  "background-image": "none",
  "background-blend-mode": "normal",
  "background-clip": "border-box",
  "box-shadow": "none",
  "mix-blend-mode": "normal",
  filter: "none",
  "backdrop-filter": "none",
  "-webkit-backdrop-filter": "none",
  "mask-image": "none",
  "-webkit-mask-image": "none",
  opacity: "1",
});

function style(backgroundColor, overrides = {}) {
  const values = { ...INITIALS, "background-color": backgroundColor, ...overrides };
  return {
    getPropertyValue(property) {
      return values[property] ?? "";
    },
  };
}

function tree(entries) {
  const nodes = entries.map((entry) => ({
    style: style(entry.color, entry.effects),
    parent: null,
  }));
  for (let index = 0; index < nodes.length - 1; index++) nodes[index].parent = nodes[index + 1];
  return {
    leaf: nodes[0],
    nodes,
    getStyle: (node) => node.style,
    parentOf: (node) => node.parent,
  };
}

function target() {
  return outputElement();
}

function watchEngine() {
  const calls = [];
  return {
    calls,
    resolveTheme(background, theme) {
      calls.push({ background, theme });
      return {
        theme,
        background,
        outputBindings: ["--lab-x"],
        vars: { "--lab-x": background },
        roles: {},
      };
    },
  };
}

function adaptiveEngine() {
  const calls = [];
  let rechecks = 0;
  return {
    calls,
    rechecks: () => rechecks,
    resolveTheme(background, theme) {
      calls.push({ background, theme });
      return {
        outputBindings: ["--lab-label-primary"],
        vars: { "--lab-label-primary": "#123456" },
        roles: {
          "label-primary": {
            kind: "color",
            cssVar: "--lab-label-primary",
            hex: "#123456",
            lc: 60,
          },
        },
      };
    },
    recheckContrast() {
      rechecks++;
      return [60, 10];
    },
  };
}

test("C8c admits one supported opaque point without a canvas assumption", () => {
  const host = tree([
    { color: "rgba(255, 0, 0, 0.5)" },
    { color: "rgb(0, 0, 255)" },
  ]);
  assert.deepEqual(
    observePointBackground(host.leaf, host),
    { kind: "point", hex: "#800080" },
  );
});

test("C8c rounds every translucent occurrence through Core", () => {
  const host = tree([
    { color: "rgba(0, 0, 0, 0.5)" },
    { color: "rgba(1, 0, 0, 0.5)" },
    { color: "rgb(0, 0, 0)" },
  ]);
  assert.deepEqual(
    observePointBackground(host.leaf, host),
    { kind: "point", hex: "#010000" },
  );
});

test("C8c transparent root is Unknown unless an opaque canvas is declared", () => {
  const host = tree([
    { color: "transparent" },
    { color: "rgba(0, 0, 0, 0.5)" },
  ]);
  assert.deepEqual(observePointBackground(host.leaf, host), {
    kind: "unknown",
    reason: "transparent-root",
  });
  assert.deepEqual(
    observePointBackground(host.leaf, { ...host, canvas: "#FFFFFF" }),
    { kind: "point", hex: "#808080" },
  );
});

test("C8c invalid declared canvas fails before any host read", () => {
  let reads = 0;
  const host = tree([{ color: "transparent" }]);
  assert.throws(
    () =>
      observePointBackground(host.leaf, {
        ...host,
        canvas: "rgba(255, 255, 255, 0.5)",
        getStyle(node) {
          reads++;
          return node.style;
        },
      }),
    /canvas must be an opaque supported colour/u,
  );
  assert.equal(reads, 0);
  for (const canvas of ["#FZFFFF", "rebeccapurple", "oklch(50% 1e308 0)"]) {
    assert.throws(
      () => observePointBackground(host.leaf, { ...host, canvas }),
      /canvas must be an opaque supported colour/u,
      canvas,
    );
  }
});

test("C8c unsupported colour and effects are typed Unknown, never dropped layers", () => {
  for (const [property, value, reason] of [
    ["background-image", "linear-gradient(red, blue)", "background-image"],
    ["background-blend-mode", "multiply", "background-blend-mode"],
    ["background-clip", "padding-box", "background-clip"],
    ["box-shadow", "inset 0 0 2px black", "box-shadow"],
    ["mix-blend-mode", "screen", "mix-blend-mode"],
    ["filter", "blur(2px)", "filter"],
    ["backdrop-filter", "blur(2px)", "backdrop-filter"],
    ["mask-image", "linear-gradient(black, transparent)", "mask-image"],
    ["opacity", "0.5", "element-opacity"],
  ]) {
    const host = tree([
      { color: "rgba(255, 0, 0, 0.5)", effects: { [property]: value } },
      { color: "rgb(0, 0, 0)" },
    ]);
    assert.deepEqual(observePointBackground(host.leaf, host), {
      kind: "unknown",
      reason,
    });
  }

  const unsupportedColour = tree([
    { color: "color(display-p3 1 0 0)" },
    { color: "rgb(0, 0, 0)" },
  ]);
  assert.deepEqual(observePointBackground(unsupportedColour.leaf, unsupportedColour), {
    kind: "unknown",
    reason: "unsupported-background-color",
  });
});

test("C8c cycle and depth exhaustion are distinct Unknown outcomes", () => {
  const cycle = tree([
    { color: "transparent" },
    { color: "transparent" },
  ]);
  cycle.nodes[1].parent = cycle.nodes[0];
  assert.deepEqual(observePointBackground(cycle.leaf, cycle), {
    kind: "unknown",
    reason: "ancestor-cycle",
  });

  const deep = tree([
    { color: "transparent" },
    { color: "transparent" },
    { color: "rgb(0, 0, 0)" },
  ]);
  assert.deepEqual(observePointBackground(deep.leaf, { ...deep, maxDepth: 2 }), {
    kind: "unknown",
    reason: "depth-exhausted",
  });
  assert.throws(
    () => observePointBackground(deep.leaf, { ...deep, maxDepth: 0 }),
    /maxDepth must be a positive safe integer/u,
  );
});

test("C8c output algebra is closed to Point or Unknown", () => {
  const supported = tree([{ color: "rgb(1, 2, 3)" }]);
  const cases = [
    observePointBackground(supported.leaf, supported),
    observePointBackground(null),
  ];
  for (const result of cases) {
    assert.ok(result.kind === "point" || result.kind === "unknown");
    assert.equal(Object.hasOwn(result, "raster"), false);
    assert.equal(Object.hasOwn(result, "field"), false);
  }
});

test("watchTheme performs no resolver or DOM work on Unknown and recovers on Point", () => {
  const host = tree([{ color: "transparent" }]);
  const output = target();
  const colors = watchEngine();
  const controller = watchTheme(host.leaf, {
    colors,
    theme: "light",
    target: output,
    observe: false,
    getStyle: host.getStyle,
    parentOf: host.parentOf,
  });

  assert.equal(colors.calls.length, 0);
  assert.equal(output.mutations.length, 0);
  assert.equal(controller.background(), null);

  host.nodes[0].style = style("rgb(255, 255, 255)");
  controller.refresh();
  assert.deepEqual(colors.calls, [{ background: "#FFFFFF", theme: "light" }]);
  assert.equal(output.props.get("--lab-x"), "#FFFFFF");
  assert.equal(controller.background(), "#FFFFFF");

  host.nodes[0].style = style("rgb(255, 255, 255)", {
    "background-image": "linear-gradient(red, blue)",
  });
  const mutations = output.mutations.length;
  controller.refresh();
  assert.equal(colors.calls.length, 1);
  assert.equal(output.mutations.length, mutations);
  assert.equal(controller.background(), "#FFFFFF");
});

test("watchTheme preserves theme intent across Unknown", () => {
  const host = tree([{ color: "transparent" }]);
  const colors = watchEngine();
  const controller = watchTheme(host.leaf, {
    colors,
    theme: "light",
    target: target(),
    observe: false,
    getStyle: host.getStyle,
    parentOf: host.parentOf,
  });
  controller.setTheme("dark");
  assert.equal(colors.calls.length, 0);

  host.nodes[0].style = style("rgb(0, 0, 0)");
  controller.refresh();
  assert.deepEqual(colors.calls, [{ background: "#000000", theme: "dark" }]);
});

test("adaptTheme bootstraps only when Unknown becomes Point", () => {
  const host = tree([{ color: "transparent" }]);
  const output = target();
  const colors = adaptiveEngine();
  const controller = adaptTheme(host.leaf, {
    colors,
    theme: "light",
    target: output,
    getStyle: host.getStyle,
    parentOf: host.parentOf,
    now: () => 0,
    reducedMotion: true,
  });

  assert.equal(colors.calls.length, 0);
  assert.equal(colors.rechecks(), 0);
  assert.deepEqual(controller.current(), {});
  assert.equal(output.mutations.length, 0);

  host.nodes[0].style = style("rgb(255, 255, 255)");
  controller.tick(1);
  assert.deepEqual(colors.calls, [{ background: "#FFFFFF", theme: "light" }]);
  assert.equal(output.props.get("--lab-label-primary"), "#123456");
});

test("adaptTheme preserves setTheme intent while observation is Unknown", () => {
  const host = tree([{ color: "rgb(255, 255, 255)" }]);
  const colors = adaptiveEngine();
  const controller = adaptTheme(host.leaf, {
    colors,
    theme: "light",
    target: target(),
    getStyle: host.getStyle,
    parentOf: host.parentOf,
    now: () => 0,
    reducedMotion: true,
  });
  assert.equal(colors.calls.length, 1);

  host.nodes[0].style = style("transparent");
  controller.setTheme("dark");
  assert.equal(colors.calls.length, 1);

  host.nodes[0].style = style("rgb(255, 255, 255)");
  controller.tick(1);
  assert.deepEqual(colors.calls.at(-1), { background: "#FFFFFF", theme: "dark" });
});
