import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";
import init, { attachProgramWire, compileProgramWire, isProgramError } from "../index.js";
import { ProgramWireBuilderV1 } from "../program-wire/abi-v1.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

// Белая подложка даёт ровно [128,128,128]; [254,254,254] уже не проходит.
// Поэтому успешный output различает исходные байты и неявное оборачивание 511→255.
const wire = new ProgramWireBuilderV1().source(11, [0, 0, 0]).fixedTarget(21, 11)
  .surfaceInputPort(31).opacityInput(32, 0.5).solidPaint(41, 21).opacityPaint(42, 41, 32)
  .inputSurface(51, 31).sourceOverOccurrence(61, 42, 51, 64, 0.2, 1)
  .presentationRoot(71, 61).presentationTarget(71, 61)
  .exactVisibleUnary(true, 81, 61, [128, 128, 128]).output(91, 42).finish();
const ids = () => new Uint32Array([1]);
const surfaces = () => new Uint8Array([255, 255, 255]);
const programError = (operation, code) => (error) =>
  isProgramError(error) && error.operation === operation && error.code === code;

const invalidInputs = [
  ["ordinary array", (_Type, values) => [...values]],
  ["wrapped array", (Type, values) => values.map((value) => value + 2 ** (8 * Type.BYTES_PER_ELEMENT))],
  ["fractional array", (_Type, values) => values.map((value) => value + 0.5)],
  ["string array", (_Type, values) => values.map(String)],
  ["wrong view", (Type, values) => new (Type === Uint8Array ? Uint8ClampedArray : Int32Array)(values)],
  ["spoofed view brand", (Type, values) => {
    const view = new (Type === Uint8Array ? Uint8ClampedArray : Int32Array)(values);
    Object.setPrototypeOf(view, Type.prototype);
    Object.defineProperty(view, Symbol.toStringTag, { value: Type.name });
    return view;
  }],
  ["Proxy", (Type, values) => new Proxy(new Type(values), {})],
  ["revoked Proxy", (Type, values) => {
    const pair = Proxy.revocable(new Type(values), {});
    pair.revoke();
    return pair.proxy;
  }],
  ["own length", (Type, values) => Object.defineProperty(new Type(values), "length", { value: values.length })],
  ["own byteLength", (Type, values) => Object.defineProperty(new Type(values), "byteLength", {
    value: values.length * Type.BYTES_PER_ELEMENT,
  })],
  ["own length getter", (Type, values, onRead) => Object.defineProperty(new Type(values), "length", {
    get() { onRead(); throw new Error("caller length must not be read"); },
  })],
  ["own byteLength getter", (Type, values, onRead) => Object.defineProperty(new Type(values), "byteLength", {
    get() { onRead(); throw new Error("caller byteLength must not be read"); },
  })],
  ["detached storage", (Type, values) => {
    const view = new Type(values);
    structuredClone(view.buffer, { transfer: [view.buffer] });
    return view;
  }],
];

function rejectsOwned(invoke, expected) {
  let unexpected;
  try {
    assert.throws(() => { unexpected = invoke(); }, expected);
  } finally {
    unexpected?.free();
  }
}

function attached(bytes = wire) {
  const host = { point: null, calls: 0 };
  const attachment = attachProgramWire(bytes, 1, 91, 501, 71, 61, (intent) => {
    host.calls += 1;
    host.point = intent.point;
    return true;
  });
  return { attachment, host, release() {
    host.point = null;
    attachment.dispose(true);
    attachment.free();
  } };
}

for (const [name, make] of invalidInputs) {
  test(`wire admission refuses ${name} before compile or attach`, () => {
    let reads = 0;
    const value = make(Uint8Array, [...wire], () => { reads += 1; });
    rejectsOwned(() => compileProgramWire(value, 1), programError("compileProgramWire", "program_wire"));
    let unexpected;
    try {
      assert.throws(() => { unexpected = attached(value); }, programError("attachProgramWire", "program_wire"));
    } finally {
      unexpected?.release();
    }
    assert.equal(reads, 0);
    const healthy = compileProgramWire(wire, 1);
    healthy.free();
  });

  for (const field of ["scenario IDs", "surfaces"]) {
    test(`runtime ${field}: ${name} refuses without consuming the revision`, () => {
      const runtime = compileProgramWire(wire, 1);
      let first, retry, reads = 0;
      try {
        first = runtime.updateObserved(1n, ids(), surfaces(), 1);
        const value = field === "surfaces"
          ? make(Uint8Array, [255, 255, 255], () => { reads += 1; })
          : make(Uint32Array, [1], () => { reads += 1; });
        rejectsOwned(() => runtime.updateObserved(
          2n, field === "scenario IDs" ? value : ids(), field === "surfaces" ? value : surfaces(), 1,
        ), programError("updateObserved", "program_update"));
        assert.equal(reads, 0);
        assert.equal(first.state, "ready");
        assert.deepEqual([...first.outputRgb(0)], [0, 0, 0]);
        retry = runtime.updateObserved(2n, ids(), surfaces(), 1);
        assert.equal(retry.state, "ready");
      } finally {
        retry?.free(); first?.free(); runtime.free();
      }
    });

    test(`attachment ${field}: ${name} preserves the installed head`, () => {
      const owner = attached();
      const { attachment, host } = owner;
      let first, authority, retry, reads = 0;
      try {
        first = attachment.updateObserved(1n, ids(), surfaces(), 1);
        const point = host.point;
        const calls = host.calls;
        const value = field === "surfaces"
          ? make(Uint8Array, [255, 255, 255], () => { reads += 1; })
          : make(Uint32Array, [1], () => { reads += 1; });
        rejectsOwned(() => attachment.updateObserved(
          2n, field === "scenario IDs" ? value : ids(), field === "surfaces" ? value : surfaces(), 1,
        ), programError("attachmentUpdateObserved", "program_attachment_update"));
        assert.equal(reads, 0);
        assert.equal(host.point, point);
        assert.equal(host.calls, calls);
        authority = attachment.materializationAuthority();
        assert.equal(authority.revision(), 1n);
        assert.deepEqual([...authority.terminalCompositeRgb()], [128, 128, 128]);
        retry = attachment.updateObserved(2n, ids(), surfaces(), 1);
        assert.equal(retry.state, "ready");
      } finally {
        retry?.free(); authority?.free(); first?.free(); owner.release();
      }
    });
  }

  test(`materialization identity refuses ${name} without changing authority`, () => {
    const owner = attached();
    let first, authority, retry, reads = 0;
    try {
      first = owner.attachment.updateObserved(1n, ids(), surfaces(), 1);
      authority = owner.attachment.materializationAuthority();
      const identity = authority.contentIdentity();
      const value = make(Uint8Array, [...identity], () => { reads += 1; });
      rejectsOwned(() => owner.attachment.materializationAuthorityFor(1n, value, authority.bindingEpoch()),
        programError("materializationAuthority", "program_materialization_stale_identity"));
      assert.equal(reads, 0);
      retry = owner.attachment.materializationAuthorityFor(1n, identity, authority.bindingEpoch());
      assert.equal(retry.revision(), 1n);
    } finally {
      retry?.free(); authority?.free(); first?.free(); owner.release();
    }
  });
}

test("valid Buffer, subclasses and offset views keep their exact values", () => {
  const paddedIds = new Uint32Array([99, 1, 99]);
  const paddedSurfaces = Buffer.from([0, 255, 255, 255, 0]);
  const runtime = compileProgramWire(Buffer.from(wire), 1);
  const owner = attached(new (class extends Uint8Array {})(wire));
  const held = [];
  try {
    held.push(runtime.updateObserved(1n, paddedIds.subarray(1, 2), paddedSurfaces.subarray(1, 4), 1));
    held.push(owner.attachment.updateObserved(1n,
      new (class extends Uint32Array {})([1]), new (class extends Uint8Array {})([255, 255, 255]), 1));
    assert.equal(held[0].state, "ready");
    assert.equal(held[1].state, "ready");
    const authority = owner.attachment.materializationAuthority();
    try {
      const copy = owner.attachment.materializationAuthorityFor(
        1n, Buffer.from(authority.contentIdentity()), authority.bindingEpoch(),
      );
      held.push(copy);
      assert.deepEqual([...copy.terminalCompositeRgb()], [128, 128, 128]);
    } finally { authority.free(); }
    held.push(runtime.updateObserved(2n, ids(), new Uint8Array([254, 254, 254]), 1));
    assert.equal(held.at(-1).state, "failed");
  } finally {
    for (const value of held.reverse()) value.free();
    owner.release(); runtime.free();
  }
});

for (const [name, make] of [
  ["cross-realm", (Type, values) => runInNewContext(`new ${Type.name}(${JSON.stringify(values)})`)],
  ["inherited storage getters", (Type, values, onRead) => new (class extends Type {
    get length() { onRead(); throw new Error("caller length must not execute"); }
    get byteLength() { onRead(); throw new Error("caller byteLength must not execute"); }
  })(values)],
  ["Proxy prototype", (Type, values, onRead) => {
    const value = new Type(values);
    Object.setPrototypeOf(value, new Proxy(Type.prototype, {
      get() { onRead(); throw new Error("prototype get must not execute"); },
      getPrototypeOf() { onRead(); throw new Error("prototype walk must not execute"); },
      getOwnPropertyDescriptor() { onRead(); throw new Error("prototype descriptor must not execute"); },
    }));
    return value;
  }],
]) {
  test(`${name} uses intrinsic storage without running caller code`, () => {
    let reads = 0;
    const onRead = () => { reads += 1; };
    const runtime = compileProgramWire(make(Uint8Array, [...wire], onRead), 1);
    const owner = attached(make(Uint8Array, [...wire], onRead));
    const held = [];
    try {
      held.push(runtime.updateObserved(1n, make(Uint32Array, [1], onRead), make(Uint8Array, [255, 255, 255], onRead), 1));
      held.push(owner.attachment.updateObserved(1n, make(Uint32Array, [1], onRead), make(Uint8Array, [255, 255, 255], onRead), 1));
      assert.equal(held[0].state, "ready");
      assert.equal(held[1].state, "ready");
      const authority = owner.attachment.materializationAuthority();
      held.push(authority);
      held.push(owner.attachment.materializationAuthorityFor(1n,
        make(Uint8Array, [...authority.contentIdentity()], onRead), authority.bindingEpoch()));
      assert.equal(reads, 0);
    } finally {
      for (const value of held.reverse()) value.free();
      owner.release(); runtime.free();
    }
  });
}

test("attachment busy refusal precedes input inspection during host reentry", () => {
  let attachment, nestedError;
  const nestedInput = Proxy.revocable(new Uint32Array([1]), {});
  nestedInput.revoke();
  attachment = attachProgramWire(wire, 1, 91, 501, 71, 61, () => {
    try { attachment.updateObserved(2n, nestedInput.proxy, surfaces(), 1); }
    catch (error) { nestedError = error; }
    return true;
  });
  let first;
  try {
    first = attachment.updateObserved(1n, ids(), surfaces(), 1);
    assert.equal(programError("attachmentUpdateObserved", "program_attachment_busy")(nestedError), true);
    assert.equal(first.state, "ready");
  } finally {
    first?.free(); attachment.dispose(true); attachment.free();
  }
});
