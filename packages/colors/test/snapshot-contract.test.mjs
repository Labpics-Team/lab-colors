import { test } from "node:test";
import assert from "node:assert/strict";

import {
  isCanonicalOutputBindingName,
  outputBindingsEqual,
} from "../output-bindings.js";
import { brandFakeElement } from "./fake-node-brand.mjs";
import { applyTheme } from "../apply-theme.js";
import { admitSnapshot } from "../snapshot.js";

test("one canonical output-binding grammar rejects every non-canonical spelling", () => {
  for (const name of ["--lab-a", "--lab-0", "--a-b-c"]) {
    assert.equal(isCanonicalOutputBindingName(name), true, name);
  }
  for (const name of ["--Lab-a", "--lab_a", " --lab-a", "--lab-a ", "--", "lab-a", null]) {
    assert.equal(isCanonicalOutputBindingName(name), false, String(name));
  }

  assert.throws(
    () =>
      admitSnapshot(
        {
          outputBindings: ["--Lab-a"],
          vars: { "--Lab-a": "#123456" },
          roles: {},
        },
        "snapshot contract",
      ),
    /canonical lower-case ASCII CSS custom-property names/u,
  );
});

test("controller rejects a non-canonical manifest before observing its output target", () => {
  let targetReads = 0;
  const target = brandFakeElement({
    get isConnected() {
      targetReads++;
      throw new Error("output target was observed before snapshot admission");
    },
  });

  assert.throws(
    () =>
      applyTheme(target, {
        outputBindings: ["--Lab-a"],
        vars: { "--Lab-a": "#123456" },
        roles: {},
      }),
    /canonical lower-case ASCII CSS custom-property names/u,
  );
  assert.equal(targetReads, 0);
});

test("output-binding equality is exact, positional, and length-sensitive", () => {
  assert.equal(outputBindingsEqual(["--lab-a", "--lab-b"], ["--lab-a", "--lab-b"]), true);
  assert.equal(outputBindingsEqual(["--lab-a", "--lab-b"], ["--lab-b", "--lab-a"]), false);
  assert.equal(outputBindingsEqual(["--lab-a"], ["--lab-a", "--lab-b"]), false);
});
