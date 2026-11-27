import assert from "node:assert/strict";
import test from "node:test";
import { ProgramWireBuilderV3, ProgramWireError } from "../program-wire/abi-v3.js";
import { lowerColorSystem } from "../authoring-graph.js";

test("compose snapshots input values and rejects malformed declarations transactionally", () => {
  const builder = new ProgramWireBuilderV3().source(1, [10, 20, 30]);
  const before = builder.finish();
  for (const run of [
    () => builder.compose(null),
    () => builder.compose([NaN]),
    () => builder.compose([0x1_0000_0000]),
    () => builder.compose(Array(4097).fill(1)),
    () => builder.compose([], { reference: null }),
    () => builder.compose([], { reference: [[1, [0, 0, 0]], [1, [1, 1, 1]]] }),
    () => builder.compose([], { reference: [[1, [256, 0, 0]]] }),
    () => builder.compose([], { reference: [[1, [0, 0]]] }),
    () => builder.compose([], { reference: [[1, [0, 0, 0], 2]] }),
    () => builder.compose([], { reference: Array(4097).fill([1, [0, 0, 0]]) }),
    () => builder.compose([], { get reference() { throw new Error("getter"); } }),
  ]) {
    assert.throws(run, ProgramWireError);
    assert.deepEqual(builder.finish(), before);
  }
  const targets = [3, 2], rgb = [255, 128, 0], reference = [[4, rgb]];
  for (const input of [targets, reference, reference[0], rgb]) {
    input[Symbol.iterator] = () => { throw new Error("caller iterator must not run"); };
  }
  builder.compose(targets, { reference });
  const accepted = builder.finish();
  targets[0] = 99; rgb[0] = 0; reference.length = 0;
  assert.deepEqual(builder.finish(), accepted);
});

test("assignment and compose cannot overwrite either admitted selection", () => {
  for (const first of ["assignment", "compose"]) {
    for (const second of ["assignment", "compose"]) {
      const builder = new ProgramWireBuilderV3()[first]([1]);
      const accepted = builder.finish();
      assert.throws(() => builder[second]([2], { reference: [] }), ProgramWireError);
      assert.deepEqual(builder.finish(), accepted);
    }
  }
});

test("reentrant compose and assignment inputs retain only the first admitted selection", () => {
  for (const outer of ["assignment", "compose"]) {
    for (const inner of ["assignment", "compose"]) {
      const builder = new ProgramWireBuilderV3();
      let accepted;
      assert.throws(() => builder[outer]([1], {
        get reference() {
          builder[inner]([2], { reference: [[4, [0, 0, 0]]] });
          accepted = builder.finish();
          return [[4, [255, 255, 255]]];
        },
      }), ProgramWireError);
      assert.deepEqual(builder.finish(), accepted);
    }
  }
});

test("compose rejects malformed list lengths before reading entries", () => {
  for (const length of [NaN, Infinity, -1, 1.5, "2", 2n, Symbol("length")]) {
    let entryReads = 0;
    const input = new Proxy([], {
      get(_target, property) {
        if (property === "length") return length;
        entryReads++;
        return undefined;
      },
    });
    for (const declare of [
      (builder) => builder.compose(input),
      (builder) => builder.compose([], { reference: input }),
    ]) {
      const builder = new ProgramWireBuilderV3();
      const before = builder.finish();
      assert.throws(() => declare(builder), ProgramWireError);
      assert.equal(entryReads, 0);
      assert.deepEqual(builder.finish(), before);
    }
  }
});

test("ordinary lowering always composes the complete declaration order", (t) => {
  const compose = t.mock.method(ProgramWireBuilderV3.prototype, "compose");
  const composeReference = t.mock.method(ProgramWireBuilderV3.prototype, "composeReference");
  t.mock.method(ProgramWireBuilderV3.prototype, "assignment", () => assert.fail("ordinary lowering must compose"));
  const input = { kind: "input", id: "canvas", ordinal: 0 };
  const makeUse = (ordinal, adaptive, against) => ({
    kind: "use", ordinal, against, policy: { opacity: 1 },
    role: { adaptive, anchor: [64, 64, 64], options: {} },
  });
  for (const adaptive of [false, true]) {
    for (const authored of [false, true]) {
      const base = makeUse(1, adaptive, input), foreground = makeUse(2, adaptive, base);
      const unrelated = makeUse(3, adaptive, input);
      const entries = [{ name: "last", node: unrelated }, { name: "first", node: foreground }];
      const graph = lowerColorSystem(entries, {
        reference: authored ? new Map([["canvas", [255, 255, 255]]]) : undefined,
      });
      const call = (authored ? composeReference : compose).mock.calls.at(-1);
      const expectedTargets = [...graph.targets].filter(([, node]) => node.role.adaptive).map(([target]) => target);
      assert.deepEqual(call.arguments[0], expectedTargets);
      assert.equal(expectedTargets.length, adaptive ? 3 : 0);
      assert.deepEqual(call.arguments[1], authored ? [[graph.inputs[0].port, [255, 255, 255]]] : undefined);
    }
  }
  assert.equal(compose.mock.callCount(), 2);
  assert.equal(composeReference.mock.callCount(), 2);
});
