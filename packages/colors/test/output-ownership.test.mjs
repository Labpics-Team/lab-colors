// F-02 — output ownership of the inline `--lab-*` namespace.
//
// CLAIM UNDER TEST: the DOM writer revokes inline custom properties by PREFIX,
// not by the output set it actually owns. `snapshot.js::writeVars` scans the
// element's whole inline style and removes EVERY `--lab-*` entry before writing
// its own vars (snapshot.js:207-217), so a second controller — or the consuming
// application itself — loses variables it declared and the writer never owned.
//
// Every case below states the exact style state BEFORE and AFTER the write and
// asserts on the surviving entries, so a failure names the erased variable.
//
// STATUS: the four cases marked `todo: RED_F02` fail on origin/main today. They
// are the reproduction of F-02, not a regression — `todo` keeps the suite
// honest (the failure is reported, the gate is not falsely green) until the
// ownership decision is made. Cases 3, 4 and 5 hold on main and are plain
// asserts: they also prove this fixture is not vacuous, since the same element
// double and the same assertion style go green where the invariant does hold.

const RED_F02 =
  "F-02: writeVars (snapshot.js:207-217) revokes inline custom properties by " +
  "`--lab-` PREFIX, not by the output set the writer owns.";

import { test } from "node:test";
import assert from "node:assert/strict";

import { applyTheme } from "../apply-theme.js";
import { watchTheme } from "../watch-theme.js";
import { adaptTheme } from "../adapt-theme.js";

// Inline-style double. Records every mutation AND the stack of each removal, so
// the erasing call site is evidence, not inference.
function spyElement(initial = []) {
  const props = new Map(initial);
  const mutations = [];
  const removalStacks = [];
  return {
    props,
    mutations,
    removalStacks,
    snapshot: () => Object.fromEntries(props),
    style: {
      get length() {
        return props.size;
      },
      item: (index) => [...props.keys()][index] ?? null,
      setProperty(key, value) {
        mutations.push(["set", key, value]);
        props.set(key, value);
      },
      removeProperty(key) {
        mutations.push(["remove", key]);
        removalStacks.push([key, new Error(`removeProperty(${key})`).stack]);
        props.delete(key);
      },
    },
  };
}

const engineEmitting = (vars) => ({ resolveTheme: () => ({ vars, roles: {} }) });

// A `resolveTheme` result adaptTheme accepts: one `kind: "color"` role plus its
// canonical var. Mirrors the fixture shape used by test/adapt-theme.test.mjs.
const colorResult = (cssVar, key, hex) => ({
  vars: { [cssVar]: hex },
  roles: { [key]: { kind: "color", cssVar, hex, lc: 100 } },
});

const adaptEngine = (result) => ({
  resolveTheme: () => result,
  recheckContrast(_bg, foregrounds) {
    const out = [];
    for (let index = 0; index < foregrounds.length; index++) out.push(100, 10);
    return out;
  },
});

const attributedTo = (element, key) =>
  element.removalStacks.find(([removed]) => removed === key)?.[1] ?? "(not removed)";

// ---------------------------------------------------------------------------
// CASE 1 — two controllers, disjoint output sets, one element.
// ---------------------------------------------------------------------------
test("F-02/1: a second controller's write must not revoke the first controller's disjoint outputs", { todo: RED_F02 }, () => {
  const element = spyElement();

  const a = watchTheme(element, {
    colors: engineEmitting({
      "--lab-a-label": "oklch(20.000% 0 0)",
      "--lab-a-border": "oklch(40.000% 0 0)",
    }),
    theme: "light",
    background: "#FFFFFF",
    observe: false,
  });
  assert.deepEqual(
    element.snapshot(),
    { "--lab-a-label": "oklch(20.000% 0 0)", "--lab-a-border": "oklch(40.000% 0 0)" },
    "precondition: controller A owns exactly its two outputs",
  );

  // BEFORE: { --lab-a-label, --lab-a-border }
  const b = watchTheme(element, {
    colors: engineEmitting({ "--lab-b-surface": "oklch(96.000% 0 0)" }),
    theme: "light",
    background: "#FFFFFF",
    observe: false,
  });
  // AFTER (observed on main): { --lab-b-surface } — A's two outputs are gone.

  try {
    assert.equal(
      element.props.get("--lab-a-label"),
      "oklch(20.000% 0 0)",
      `controller B revoked '--lab-a-label', an output it does not own.\n` +
        `AFTER = ${JSON.stringify(element.snapshot())}\n` +
        `removed by: ${attributedTo(element, "--lab-a-label")}`,
    );
    assert.equal(
      element.props.get("--lab-a-border"),
      "oklch(40.000% 0 0)",
      "controller B revoked '--lab-a-border', an output it does not own",
    );
    assert.equal(element.props.get("--lab-b-surface"), "oklch(96.000% 0 0)");
  } finally {
    a.stop();
    b.stop();
  }
});

test("F-02/1b: adaptTheme's first commit must not revoke a foreign controller's outputs", { todo: RED_F02 }, () => {
  const element = spyElement();

  const b = watchTheme(element, {
    colors: engineEmitting({ "--lab-b-surface": "oklch(96.000% 0 0)" }),
    theme: "light",
    background: "#FFFFFF",
    observe: false,
  });
  assert.deepEqual(element.snapshot(), { "--lab-b-surface": "oklch(96.000% 0 0)" });

  // BEFORE: { --lab-b-surface }
  const a = adaptTheme(element, {
    colors: adaptEngine(colorResult("--lab-a-label", "a-label", "#1A1A1A")),
    theme: "light",
    background: "#FFFFFF",
    target: element,
    now: () => 1000,
    win: {},
  });
  // AFTER (observed on main): { --lab-a-label } — B's output is gone.

  try {
    assert.equal(
      element.props.get("--lab-b-surface"),
      "oklch(96.000% 0 0)",
      `adaptTheme revoked '--lab-b-surface', an output it does not own.\n` +
        `AFTER = ${JSON.stringify(element.snapshot())}\n` +
        `removed by: ${attributedTo(element, "--lab-b-surface")}`,
    );
  } finally {
    a.stop();
    b.stop();
  }
});

// ---------------------------------------------------------------------------
// CASE 2 — an application-owned `--lab-*` variable no controller ever declared.
// ---------------------------------------------------------------------------
test("F-02/2: a consumer-owned --lab-* variable no controller declared must survive a write", { todo: RED_F02 }, () => {
  const element = spyElement();
  // The consuming design system owns this name. It is not in any `result.vars`,
  // so no writer can claim it as its own output.
  element.style.setProperty("--lab-brand-ring", "#FF0000");
  assert.deepEqual(element.snapshot(), { "--lab-brand-ring": "#FF0000" });

  // BEFORE: { --lab-brand-ring }
  applyTheme(element, {
    vars: { "--lab-label-primary": "oklch(20.000% 0 0)" },
    roles: {},
  });
  // AFTER (observed on main): { --lab-label-primary } — the consumer var is gone.

  assert.equal(
    element.props.get("--lab-brand-ring"),
    "#FF0000",
    `applyTheme revoked '--lab-brand-ring', a consumer variable absent from result.vars.\n` +
      `AFTER = ${JSON.stringify(element.snapshot())}\n` +
      `removed by: ${attributedTo(element, "--lab-brand-ring")}`,
  );
});

// ---------------------------------------------------------------------------
// CASE 3 — teardown scope. Expected to hold on main: `stop()` is not a revoke.
// ---------------------------------------------------------------------------
test("F-02/3: stop() revokes no variable at all — neither its own outputs nor a foreign one", () => {
  const element = spyElement();
  const a = watchTheme(element, {
    colors: engineEmitting({ "--lab-a-label": "oklch(20.000% 0 0)" }),
    theme: "light",
    background: "#FFFFFF",
    observe: false,
  });
  element.style.setProperty("--lab-brand-ring", "#FF0000");
  const mutationsBeforeStop = element.mutations.length;

  a.stop();

  assert.deepEqual(
    element.mutations.slice(mutationsBeforeStop),
    [],
    "stop() must not touch the inline style",
  );
  assert.deepEqual(element.snapshot(), {
    "--lab-a-label": "oklch(20.000% 0 0)",
    "--lab-brand-ring": "#FF0000",
  });
});

// ---------------------------------------------------------------------------
// CASE 4 — two names for one output. Expected to hold on main: one writer.
// ---------------------------------------------------------------------------
test("F-02/4: two aliases of one output are written by ONE writer, in one clear-then-write pass", () => {
  const element = spyElement();
  const aliased = {
    vars: {
      "--lab-accent": "oklch(60.000% 0.1 250)",
      "--lab-accent-compat": "oklch(60.000% 0.1 250)",
    },
    roles: {},
  };

  applyTheme(element, aliased);
  element.mutations.length = 0;
  applyTheme(element, aliased);

  const removals = element.mutations.filter(([kind]) => kind === "remove");
  const sets = element.mutations.filter(([kind]) => kind === "set");
  const lastRemovalAt = element.mutations.findLastIndex(([kind]) => kind === "remove");
  const firstSetAt = element.mutations.findIndex(([kind]) => kind === "set");

  assert.deepEqual(
    removals.map(([, key]) => key).sort(),
    ["--lab-accent", "--lab-accent-compat"],
    "each alias is revoked exactly once — not once per alias-owner",
  );
  assert.equal(sets.length, 2, "each alias is written exactly once");
  assert.ok(lastRemovalAt < firstSetAt, "a single clear-then-write pass, not two interleaved writers");
});

// ---------------------------------------------------------------------------
// CASE 5 — reattach with a new generation.
// ---------------------------------------------------------------------------
test("F-02/5: a stopped controller's later refresh must not revoke the new generation's output", () => {
  const element = spyElement();
  const oldGeneration = watchTheme(element, {
    colors: engineEmitting({ "--lab-a-label": "oklch(20.000% 0 0)" }),
    theme: "light",
    background: "#FFFFFF",
    observe: false,
  });
  oldGeneration.stop();

  const newGeneration = watchTheme(element, {
    colors: engineEmitting({ "--lab-b-surface": "oklch(96.000% 0 0)" }),
    theme: "light",
    background: "#FFFFFF",
    observe: false,
  });
  assert.equal(element.props.get("--lab-b-surface"), "oklch(96.000% 0 0)");

  oldGeneration.refresh(true); // the retired generation tries to write again

  try {
    assert.equal(
      element.props.get("--lab-b-surface"),
      "oklch(96.000% 0 0)",
      "a stopped generation must not revoke the live generation's output",
    );
  } finally {
    newGeneration.stop();
  }
});

test("F-02/5b: a still-live retired controller DOES revoke the new generation's output", { todo: RED_F02 }, () => {
  const element = spyElement();
  // Same reattach, but the old controller was never stopped — the only thing
  // that kept case 5 green. Ownership is per-instance state, not per-variable.
  const oldGeneration = watchTheme(element, {
    colors: engineEmitting({ "--lab-a-label": "oklch(20.000% 0 0)" }),
    theme: "light",
    background: "#FFFFFF",
    observe: false,
  });
  const newGeneration = watchTheme(element, {
    colors: engineEmitting({ "--lab-b-surface": "oklch(96.000% 0 0)" }),
    theme: "light",
    background: "#FFFFFF",
    observe: false,
  });
  assert.deepEqual(element.snapshot(), { "--lab-b-surface": "oklch(96.000% 0 0)" });

  oldGeneration.refresh(true);

  try {
    assert.equal(
      element.props.get("--lab-b-surface"),
      "oklch(96.000% 0 0)",
      `the retired controller revoked '--lab-b-surface', an output it does not own.\n` +
        `AFTER = ${JSON.stringify(element.snapshot())}\n` +
        `removed by: ${attributedTo(element, "--lab-b-surface")}`,
    );
  } finally {
    oldGeneration.stop();
    newGeneration.stop();
  }
});
