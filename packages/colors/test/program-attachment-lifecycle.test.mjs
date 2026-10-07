import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { attachProgramWire, isProgramError } from "../index.js";
import { ProgramWireBuilderV1 } from "../program-wire/abi-v1.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

const wire = new ProgramWireBuilderV1()
  .source(11, [20, 20, 20]).fixedTarget(21, 11).surfaceInputPort(31)
  .solidPaint(41, 21).inputSurface(51, 31).sourceOverOccurrence(61, 41, 51, 64, 0.2, 1)
  .presentationRoot(71, 61).presentationTarget(71, 61)
  .wcag22VisibleUnary(true, 81, 61, 3).output(91, 41).finish();
const observe = (attachment, revision) => attachment.updateObserved(
  revision, new Uint32Array([1]), new Uint8Array([255, 255, 255]), 1,
);
const releaseError = (code) => (error) =>
  isProgramError(error) && error.operation === "attachmentFree" && error.code === code;

// Node >=22.11 — поддерживаемая среда пакета с explicit resource management.
assert.equal(typeof Symbol.dispose, "symbol");

for (const [name, release] of [
  ["free", (attachment) => attachment.free()],
  ["symbol disposal", (attachment) => attachment[Symbol.dispose]()],
]) {
  test(`${name} requires external revoke confirmation and preserves the current head`, () => {
    const host = { point: null, calls: 0 };
    const attachment = attachProgramWire(wire, 1, 91, 501, 71, 61, (intent) => {
      host.calls += 1;
      host.point = intent.point;
      return true;
    });
    let unexpectedlyReleased = false;
    let first, retry, authority;
    try {
      first = observe(attachment, 1n);
      assert.equal(first.state, "ready");
      const calls = host.calls;
      const point = host.point;
      assert.throws(() => {
        release(attachment);
        unexpectedlyReleased = true;
      }, releaseError("program_attachment_revoke_unconfirmed"));
      assert.equal(host.calls, calls);
      assert.equal(host.point, point);
      assert.throws(() => attachment.dispose(false), (error) =>
        isProgramError(error) && error.code === "program_attachment_revoke_unconfirmed");
      assert.throws(() => release(attachment), releaseError("program_attachment_revoke_unconfirmed"));
      authority = attachment.materializationAuthority();
      assert.equal(authority.revision(), 1n);
      assert.deepEqual([...authority.terminalCompositeRgb()], [20, 20, 20]);
      retry = observe(attachment, 2n);
      assert.equal(retry.state, "ready");
    } finally {
      authority?.free();
      retry?.free();
      first?.free();
      host.point = null;
      // RED на старом alias уже освободил объект: повтор не должен скрывать
      // исходную ошибку теста вторичным обращением к потреблённому указателю.
      if (!unexpectedlyReleased) {
        attachment.dispose(true);
        release(attachment);
      }
    }
    assert.equal(host.point, null);
  });

  test(`${name} refuses host reentry before consuming the attachment`, () => {
    let attachment, nestedError, attempted = false;
    const host = { point: null, reenter: false };
    attachment = attachProgramWire(wire, 1, 91, 501, 71, 61, (intent) => {
      if (host.reenter) {
        attempted = true;
        try { release(attachment); } catch (error) { nestedError = error; }
      }
      host.point = intent.point;
      return true;
    });
    let snapshot, retry, authority;
    try {
      host.reenter = true;
      snapshot = observe(attachment, 1n);
      assert.equal(attempted, true);
      assert.equal(releaseError("program_attachment_busy")(nestedError), true);
      assert.equal(snapshot.state, "ready");
      authority = attachment.materializationAuthority();
      assert.equal(authority.revision(), 1n);
      host.reenter = false;
      retry = observe(attachment, 2n);
      assert.equal(retry.state, "ready");
    } finally {
      authority?.free();
      retry?.free();
      snapshot?.free();
      host.point = null;
      attachment.dispose(true);
      release(attachment);
    }
  });
}
