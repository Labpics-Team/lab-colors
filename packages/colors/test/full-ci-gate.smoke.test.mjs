// TDD RED — the engine gate smoke from the GROUND condition.
//
// The full chapter gate is: cargo fmt --check + clippy -D warnings + cargo test
// + wasm-pack build + index.d.ts boundary check + headless-Chrome consumer
// smoke, all green. The behavioural END of that pipeline — the thing the gate
// exists to prove — is that the built engine carries the new `surface-shadow-tint`
// role all the way to the consumer's CSS variables through `applyTheme`.
//
// This smoke pins that end-to-end outcome cheaply (node --test, no browser): the
// chapter must produce a built role contract whose `vars` include
// `--lab-surface-shadow-tint`, and `applyTheme` must write it onto an element.
//
// HOW IT BITES NOW (RED): the chapter has not produced the built role contract
// fixture (`test/role-contract.json`, generated from the WASM engine's emitted
// `resolveTheme(...).vars` keys), so the smoke fails. When the chapter lands the
// role and the build regenerates the fixture, this turns green — proving the gate
// carried the new role through the real boundary, not a hand-typed string.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import { applyTheme } from "../apply-theme.js";

const here = dirname(fileURLToPath(import.meta.url));

const SURFACE_SHADOW_TINT_VAR = "--lab-surface-shadow-tint";

// A minimal element stub with a live inline-style list — enough to drive
// `applyTheme` headlessly, mirroring the consumer-smoke leg of the gate.
function elementStub() {
  const props = new Map();
  return {
    style: {
      get length() {
        return props.size;
      },
      item(i) {
        return [...props.keys()][i] ?? null;
      },
      setProperty(name, value) {
        props.set(name, value);
      },
      removeProperty(name) {
        props.delete(name);
      },
    },
    get(name) {
      return props.get(name);
    },
  };
}

// The built engine's role contract — the keys `resolveTheme(...)` emits as
// `--lab-*` vars, generated FROM the WASM engine by the build step (never typed
// by hand). Its absence means the gate has not produced a build carrying the new
// role.
test("built role contract includes surface-shadow-tint", () => {
  const fixture = join(here, "role-contract.json");
  assert.ok(
    existsSync(fixture),
    "gate must generate test/role-contract.json from the built engine's resolveTheme vars",
  );
  const contract = JSON.parse(readFileSync(fixture, "utf8"));
  assert.ok(
    Array.isArray(contract.varNames),
    "role contract must list the emitted --lab-* var names",
  );
  assert.ok(
    contract.varNames.includes(SURFACE_SHADOW_TINT_VAR),
    `built engine must emit ${SURFACE_SHADOW_TINT_VAR} (the gate carries the role to CSS)`,
  );
});

// The consumer-smoke leg: a resolved theme carrying the new role must write
// `--lab-surface-shadow-tint` onto the element via `applyTheme`. The `vars` are
// taken from the built contract so the value is engine-derived, not invented.
test("applyTheme writes the surface-shadow-tint var end-to-end", () => {
  const fixture = join(here, "role-contract.json");
  assert.ok(
    existsSync(fixture),
    "gate must generate test/role-contract.json from the built engine",
  );
  const contract = JSON.parse(readFileSync(fixture, "utf8"));
  // `sampleVars` is the engine's emitted vars for the canonical white/light case,
  // captured by the build — including the law-derived shadow tint.
  const vars = contract.sampleVars;
  assert.ok(
    vars && typeof vars[SURFACE_SHADOW_TINT_VAR] === "string",
    `built sample must carry a value for ${SURFACE_SHADOW_TINT_VAR}`,
  );

  const el = elementStub();
  applyTheme(el, { vars });
  assert.equal(
    el.get(SURFACE_SHADOW_TINT_VAR),
    vars[SURFACE_SHADOW_TINT_VAR],
    "applyTheme must write the law-derived shadow tint onto the element",
  );
});
