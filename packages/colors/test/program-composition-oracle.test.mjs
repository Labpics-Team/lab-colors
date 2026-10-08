import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { attachProgramWire, compileProgramWire, isProgramError } from "../index.js";
import { ProgramWireBuilderV1 } from "../program-wire/abi-v1.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

// Integer oracle for the dyadic source-over cases below. Each declared
// occurrence rounds once; it does not reuse the runtime's floating formula.
function over(source, quarters, backdrop) {
  return source.map((channel, index) => Number(
    (2n * (BigInt(channel) * BigInt(quarters) + BigInt(backdrop[index]) * BigInt(4 - quarters)) + 4n) / 8n,
  ));
}

function graph(first, qa, second, qb, expected, rename = 0) {
  const id = (value) => value + rename;
  return new ProgramWireBuilderV1()
    .source(id(11), first).source(id(12), second)
    .fixedTarget(id(21), id(11)).fixedTarget(id(22), id(12))
    .surfaceInputPort(id(31)).opacityInput(id(32), qa / 4).opacityInput(id(33), qb / 4)
    .solidPaint(id(41), id(21)).opacityPaint(id(42), id(41), id(32))
    .solidPaint(id(43), id(22)).opacityPaint(id(44), id(43), id(33))
    .inputSurface(id(51), id(31)).occurrenceSurface(id(52), id(61))
    .sourceOverOccurrence(id(61), id(42), id(51), 64, 0.2, 1)
    .sourceOverOccurrence(id(62), id(44), id(52), 64, 0.2, 1)
    .presentationRoot(id(71), id(62)).presentationTarget(id(71), id(62))
    .exactVisibleUnary(true, id(81), id(62), expected).output(id(91), id(44)).finish();
}

function plain(snapshot) {
  return { state: snapshot.state, outputs: Array.from({ length: snapshot.outputCount() }, (_, i) => ({
    rgb: [...snapshot.outputRgb(i)], opacity: snapshot.outputOpacity(i),
  })) };
}

test("whole Program output obeys per-occurrence rounding and every observed scenario", () => {
  const colors = [[0, 1, 255], [1, 127, 254], [127, 128, 129], [255, 254, 0]];
  let accepted = 0, rejected = 0, rows = 0;
  for (let seed = 0; seed < colors.length; seed += 1) {
    const first = colors[seed], second = colors[(seed + 1) % colors.length];
    for (let qa = 0; qa <= 4; qa += 1) {
      for (let qb = 0; qb <= 4; qb += 1) {
        const backdrop = colors[(seed + 2) % colors.length];
        const alternate = colors[(seed + 3) % colors.length];
        const expected = over(second, qb, over(first, qa, backdrop));
        const other = over(second, qb, over(first, qa, alternate));
        const universal = expected.every((value, i) => value === other[i]);
        const runtime = compileProgramWire(graph(first, qa, second, qb, expected), 1);
        const held = [];
        try {
          const initial = runtime.updateObserved(1n, new Uint32Array([101]), new Uint8Array(backdrop), 1);
          held.push(initial);
          assert.deepEqual(plain(initial), { state: "ready", outputs: [{ rgb: second, opacity: qb / 4 }] });
          const unchanged = plain(initial);
          let accidental;
          try {
            assert.throws(() => { accidental = runtime.updateObserved(2n, new Uint32Array([101, 102]), new Uint8Array(backdrop), 1); },
              (error) => isProgramError(error) && error.code === "program_update");
          } finally { accidental?.free(); }
          const both = runtime.updateObserved(2n, new Uint32Array([101, 102]), new Uint8Array([...backdrop, ...alternate]), 1);
          held.push(both);
          // Core keeps the last verified values for inspection. Only ready
          // certifies them for the current observation.
          assert.deepEqual(plain(both), {
            state: universal ? "ready" : "failed", outputs: [{ rgb: second, opacity: qb / 4 }],
          });
          assert.deepEqual(plain(initial), unchanged);
          const reordered = runtime.updateObserved(3n, new Uint32Array([102, 101]), new Uint8Array([...alternate, ...backdrop]), 1);
          held.push(reordered);
          assert.deepEqual(plain(reordered), plain(both));
          const unknown = runtime.updateUnknown(4n, 1);
          held.push(unknown);
          assert.equal(unknown.state, "stale");
          assert.deepEqual(plain(unknown).outputs, unchanged.outputs);
          const recovered = runtime.updateObserved(5n, new Uint32Array([101]), new Uint8Array(backdrop), 1);
          held.push(recovered);
          assert.deepEqual(plain(recovered), unchanged);
          universal ? accepted += 1 : rejected += 1;
          rows += 1;
        } finally {
          for (const snapshot of held.reverse()) snapshot.free();
          runtime.free();
        }
      }
    }
  }
  assert.equal(rows, 100);
  assert.ok(accepted > 0 && rejected > 0);
});

test("renaming Program IDs preserves the physical result and its failure boundary", () => {
  const first = [1, 127, 255], second = [254, 128, 0], backdrop = [64, 64, 64];
  const expected = over(second, 1, over(first, 3, backdrop));
  const outcomes = [];
  for (const rename of [0, 5000]) {
    const runtime = compileProgramWire(graph(first, 3, second, 1, expected, rename), 7);
    let snapshot, failed;
    try {
      snapshot = runtime.updateObserved(1n, new Uint32Array([9]), new Uint8Array(backdrop), 1);
      failed = runtime.updateObserved(2n, new Uint32Array([9]), new Uint8Array([0, 0, 0]), 1);
      outcomes.push({ ready: plain(snapshot), failed: plain(failed) });
    } finally { failed?.free(); snapshot?.free(); runtime.free(); }
  }
  assert.deepEqual(outcomes[0], outcomes[1]);
  assert.equal(outcomes[0].ready.state, "ready");
  assert.equal(outcomes[0].failed.state, "failed");
});


test("the attached consumer revokes failed composites and can retry an uncommitted install", () => {
  const first = [1, 127, 255], second = [254, 128, 0], backdrop = [64, 64, 64];
  const expected = over(second, 1, over(first, 3, backdrop));
  const host = { point: null, reject: false, operations: [] };
  const attachment = attachProgramWire(graph(first, 3, second, 1, expected), 11, 91, 501, 71, 62, (intent) => {
    host.operations.push(intent.operation);
    if (host.reject) return false;
    host.point = intent.point;
    return true;
  });
  const held = [];
  try {
    const initial = attachment.updateObserved(1n, new Uint32Array([1]), new Uint8Array(backdrop), 1);
    held.push(initial);
    assert.equal(initial.state, "ready");
    const authority = attachment.materializationAuthority();
    held.push(authority);
    assert.deepEqual([...authority.terminalCompositeRgb()], expected);
    assert.equal(authority.revision(), 1n);
    const failed = attachment.updateObserved(2n, new Uint32Array([1]), new Uint8Array([0, 0, 0]), 1);
    held.push(failed);
    assert.equal(failed.state, "failed");
    assert.equal(host.point, null);
    assert.equal(host.operations.at(-1), "revokeAll");
    assert.throws(() => attachment.materializationAuthority(), (error) =>
      isProgramError(error) && error.code === "program_materialization_not_ready");
    host.reject = true;
    let accidental;
    try {
      assert.throws(() => { accidental = attachment.updateObserved(3n, new Uint32Array([1]), new Uint8Array(backdrop), 1); },
        (error) => isProgramError(error) && error.code === "program_attachment_host_rejected");
    } finally { accidental?.free(); }
    assert.equal(host.point, null);
    host.reject = false;
    const recovered = attachment.updateObserved(3n, new Uint32Array([1]), new Uint8Array(backdrop), 1);
    held.push(recovered);
    assert.equal(recovered.state, "ready");
    const current = attachment.materializationAuthority();
    held.push(current);
    assert.equal(current.revision(), 3n);
    assert.deepEqual([...current.terminalCompositeRgb()], expected);
    assert.equal(authority.revision(), 1n);
    assert.deepEqual([...authority.terminalCompositeRgb()], expected);
  } finally {
    for (const snapshot of held.reverse()) snapshot.free();
    host.point = null;
    attachment.dispose(true);
    attachment.free();
  }
});
