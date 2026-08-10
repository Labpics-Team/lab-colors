import assert from "node:assert/strict";
import test from "node:test";

import { outputElement } from "./output-host.mjs";

const { createPrivateProgramConsumer } = await import("../private-program/consumer.js");

const OUTPUT_BINDING = "--lab-private-program-output";
const REQUEST_LENGTH = 296;
const RESULT_LENGTH = 95;
const REQUEST_POINTER = 0;
const RESULT_POINTER = 1_024;
const CSS_POINTER = 2_048;
const REQUEST_SINK_OUTPUT_OFFSET = 292;
const HOST_MODULE = "labcolors_private_fixture_host_v1";
const HOST_INSTALL = "labcolors_private_fixture_host_install_v1";
const HOST_CONFIRM = "labcolors_private_fixture_host_confirm_disposed_v1";
const HOST_INSTALL_SUCCESS = 0x4c43_0001;
const HOST_DISPOSE_CONFIRMED = 0x4c43_0002;
const DISPOSE_BEGIN_BUSY = -1;
// The fake mirrors the Core wire contract: live dispose tokens are encoded in
// [DISPOSE_TOKEN_BASE, 2 * DISPOSE_TOKEN_BASE - 1], disjoint from error statuses.
const DISPOSE_TOKEN_BASE = 0x1000_0000;
const DISPOSE_TOKEN_ENCODED_END = 2 * DISPOSE_TOKEN_BASE - 1;
const OPERATION_SET_ALL = 1;
const OPERATION_REVOKE_ALL = 2;
const OPERATION_CONFIRM_EXACT = 3;
const CSS = "rgba(64,64,64,0.5)";
const SMALLEST_POSITIVE_ALPHA_CSS = `0.${"0".repeat(323)}5`;
const HALF_OPACITY_BITS = 0x3fe0_0000_0000_0000n;
const ENCODER = new TextEncoder();

function requestBytes(sinkOutput = 501) {
  const bytes = new Uint8Array(REQUEST_LENGTH);
  bytes.set([0x4c, 0x43, 0x46, 0x51]);
  const view = new DataView(bytes.buffer);
  view.setUint16(4, 1, true);
  view.setUint16(6, REQUEST_LENGTH, true);
  view.setUint32(REQUEST_SINK_OUTPUT_OFFSET, sinkOutput, true);
  return bytes;
}

function expectedHex(bytes) {
  return bytes.map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

class FakePrivateProgramWasm {
  constructor(imports, plans = ["success"], memory = new WebAssembly.Memory({ initial: 1 })) {
    this.imports = imports;
    this.plans = [...plans];
    this.memory = memory;
    this.resultPointer = RESULT_POINTER;
    this.nextGeneration = 1;
    this.activeGeneration = null;
    this.disposing = false;
    this.runCount = 0;
    this.beginCalls = 0;
    this.beginTokens = [];
    this.abortTokens = [];
    this.commitTokens = [];
    this.installStatuses = [];
    this.lengthCalls = 0;
    this.failNextCommit = false;
    this.nextBeginOutcome = null;
    this.nextAbortOutcome = null;
    this.nextCommitOutcome = null;
    this.exports = {
      memory: this.memory,
      labcolors_private_fixture_request_v1_ptr: () => REQUEST_POINTER,
      labcolors_private_fixture_request_v1_len: () => {
        this.lengthCalls++;
        return REQUEST_LENGTH;
      },
      labcolors_private_fixture_result_v1_ptr: () => this.resultPointer,
      labcolors_private_fixture_result_v1_len: () => {
        this.lengthCalls++;
        return RESULT_LENGTH;
      },
      labcolors_private_fixture_run_v1: () => this.run(),
      labcolors_private_fixture_begin_dispose_v1: () => this.beginDispose(),
      labcolors_private_fixture_abort_dispose_v1: (token) => this.abortDispose(token),
      labcolors_private_fixture_commit_dispose_v1: (token) => this.commitDispose(token),
    };
  }

  get install() {
    return this.imports[HOST_MODULE][HOST_INSTALL];
  }

  get confirmDisposed() {
    return this.imports[HOST_MODULE][HOST_CONFIRM];
  }

  requestSinkOutput() {
    return new DataView(this.memory.buffer).getUint32(REQUEST_SINK_OUTPUT_OFFSET, true);
  }

  putCss(value) {
    const bytes = ENCODER.encode(value);
    new Uint8Array(this.memory.buffer, CSS_POINTER, bytes.length).set(bytes);
    return bytes.length;
  }

  callInstall({
    generation,
    operation = OPERATION_SET_ALL,
    revision = 9n,
    expected = 0n,
    desired = 1n,
    output = 17,
    sinkOutput = this.requestSinkOutput(),
    css = CSS,
  }) {
    const cssLength = css === "" ? 0 : this.putCss(css);
    const words = (value) => [Number(value & 0xffff_ffffn), Number(value >> 32n)];
    const [revisionLow, revisionHigh] = words(revision);
    const [expectedLow, expectedHigh] = words(expected);
    const [desiredLow, desiredHigh] = words(desired);
    const status = this.install(
      generation,
      operation,
      revisionLow,
      revisionHigh,
      expectedLow,
      expectedHigh,
      desiredLow,
      desiredHigh,
      output,
      sinkOutput,
      CSS_POINTER,
      cssLength,
    );
    this.installStatuses.push(status);
    return status;
  }

  writeResult({ sinkOutput = this.requestSinkOutput(), output = 17 } = {}) {
    const bytes = new Uint8Array(this.memory.buffer, RESULT_POINTER, RESULT_LENGTH);
    bytes.fill(0);
    bytes.set([0x4c, 0x43, 0x46, 0x52]);
    const view = new DataView(this.memory.buffer, RESULT_POINTER, RESULT_LENGTH);
    view.setUint16(4, 1, true);
    view.setUint16(6, RESULT_LENGTH, true);
    view.setUint32(8, output, true);
    view.setUint32(12, sinkOutput, true);
    view.setUint32(16, 3, true);
    bytes.set([64, 64, 64], 20);
    view.setBigUint64(23, HALF_OPACITY_BITS, true);
    bytes.set(Array.from({ length: 32 }, (_, index) => index), 31);
    bytes.set(Array.from({ length: 32 }, (_, index) => 255 - index), 63);
  }

  run() {
    this.runCount++;
    if (this.activeGeneration !== null) return 14;
    const plan = this.plans.shift() ?? "success";
    if (plan === "run-trap-vacant") {
      throw new WebAssembly.RuntimeError("vacant run trap");
    }
    if (plan === "pre-attach-failure") return 1;

    const generation = this.nextGeneration++;
    if (
      plan === "run-trap-active" ||
      plan === "run-trap-probe-busy" ||
      plan === "run-trap-probe-throw" ||
      plan === "run-trap-published-probe-busy"
    ) {
      if (plan === "run-trap-published-probe-busy") {
        const status = this.callInstall({ generation });
        if (status !== HOST_INSTALL_SUCCESS) return 8;
      }
      this.activeGeneration = generation;
      if (plan === "run-trap-probe-busy" || plan === "run-trap-published-probe-busy") {
        this.nextBeginOutcome = DISPOSE_BEGIN_BUSY;
      }
      if (plan === "run-trap-probe-throw") {
        this.nextBeginOutcome = new WebAssembly.RuntimeError("dispose probe trap");
      }
      throw new WebAssembly.RuntimeError("active run trap");
    }
    if (plan === "active-no-callback" || plan === "active-no-install-success") {
      this.activeGeneration = generation;
      if (plan === "active-no-install-success") this.writeResult();
      return plan === "active-no-callback" ? 8 : 0;
    }
    let status;
    if (
      plan === "repeat-set" ||
      plan === "set-then-revoke" ||
      plan === "set-then-confirm"
    ) {
      status = this.callInstall({ generation, revision: 9n, expected: 0n, desired: 1n });
      if (status === HOST_INSTALL_SUCCESS) {
        if (plan === "repeat-set") {
          status = this.callInstall({
            generation,
            revision: 10n,
            expected: 1n,
            desired: 2n,
            css: "rgba(1,2,3,0.5)",
          });
        } else if (plan === "set-then-revoke") {
          status = this.callInstall({
            generation,
            operation: OPERATION_REVOKE_ALL,
            revision: 10n,
            expected: 1n,
            desired: 2n,
            output: 0,
            sinkOutput: 0,
            css: "",
          });
        } else {
          status = this.callInstall({
            generation,
            operation: OPERATION_CONFIRM_EXACT,
            revision: 9n,
            expected: 1n,
            desired: 1n,
          });
        }
      }
    } else if (plan === "generation-mismatch") {
      status = this.callInstall({ generation: generation + 1 });
      if (status === HOST_INSTALL_SUCCESS) {
        this.activeGeneration = generation;
        return 8;
      }
    } else if (plan === "ignored-host-failure") {
      status = this.callInstall({ generation });
      if (status === HOST_INSTALL_SUCCESS) {
        this.callInstall({
          generation,
          operation: 99,
          revision: 10n,
          expected: 1n,
          desired: 2n,
        });
      }
      this.activeGeneration = generation;
      this.writeResult();
      return 0;
    } else {
      const plannedCss =
        {
          "css-token": "url(https://attacker.invalid/x)",
          "css-utf8": "rgba(64,64,64,🔥)",
          "css-oversize": "x".repeat(345),
          "css-noncanonical-alpha": "rgba(64,64,64,0.50)",
          "css-scientific-alpha": "rgba(64,64,64,5e-1)",
          "css-rgb-out-of-range": "rgba(256,64,64,0.5)",
          "css-alpha-zero": "rgba(0,0,0,0)",
          "css-alpha-one": "rgba(255,255,255,1)",
          "css-alpha-roundtrip": "rgba(1,2,3,0.03604545968562789)",
          "css-alpha-smallest": `rgba(1,2,3,${SMALLEST_POSITIVE_ALPHA_CSS})`,
        }[plan] ?? CSS;
      status = this.callInstall({
        generation:
          plan === "positive-u32-max-host-generation"
            ? 0xffff_ffff
            : plan === "above-u32-host-generation"
              ? 0x1_0000_0001
              : generation,
        sinkOutput: plan === "install-sink-mismatch" ? 502 : this.requestSinkOutput(),
        css: plannedCss,
      });
    }

    if (status !== HOST_INSTALL_SUCCESS) {
      this.activeGeneration = generation;
      return 8;
    }
    this.activeGeneration = generation;
    this.writeResult({
      sinkOutput: plan === "result-sink-mismatch" ? 502 : this.requestSinkOutput(),
    });
    if (plan === "result-pointer-oob") {
      this.resultPointer = this.memory.buffer.byteLength - RESULT_LENGTH + 1;
    }
    return 0;
  }

  beginDispose() {
    this.beginCalls++;
    if (this.nextBeginOutcome !== null) {
      const outcome = this.nextBeginOutcome;
      this.nextBeginOutcome = null;
      if (outcome instanceof Error) throw outcome;
      return outcome;
    }
    if (this.activeGeneration === null || this.disposing) return 0;
    this.disposing = true;
    this.beginTokens.push(this.activeGeneration);
    return DISPOSE_TOKEN_BASE + this.activeGeneration;
  }

  abortDispose(token) {
    this.abortTokens.push(token - DISPOSE_TOKEN_BASE);
    if (this.nextAbortOutcome !== null) {
      const outcome = this.nextAbortOutcome;
      this.nextAbortOutcome = null;
      if (outcome instanceof Error) throw outcome;
      return outcome;
    }
    if (!this.disposing || token - DISPOSE_TOKEN_BASE !== this.activeGeneration) return 16;
    this.disposing = false;
    return 0;
  }

  commitDispose(token) {
    this.commitTokens.push(token - DISPOSE_TOKEN_BASE);
    if (this.nextCommitOutcome !== null) {
      const outcome = this.nextCommitOutcome;
      this.nextCommitOutcome = null;
      if (outcome instanceof Error) throw outcome;
      return outcome;
    }
    if (!this.disposing || token - DISPOSE_TOKEN_BASE !== this.activeGeneration) return 16;
    if (this.confirmDisposed(this.activeGeneration, token - DISPOSE_TOKEN_BASE) !== HOST_DISPOSE_CONFIRMED) return 17;
    if (this.failNextCommit) {
      this.failNextCommit = false;
      return 17;
    }
    this.disposing = false;
    this.activeGeneration = null;
    return 0;
  }
}

async function withFakeProgram(
  {
    plans = ["success"],
    target = outputElement(),
    outputBinding = OUTPUT_BINDING,
    memory,
  } = {},
  body,
) {
  const originalFetch = globalThis.fetch;
  const originalInstantiate = WebAssembly.instantiate;
  let fetched = null;
  let wasm = null;
  globalThis.fetch = async (url) => {
    fetched = String(url);
    return { ok: true, arrayBuffer: async () => new ArrayBuffer(8) };
  };
  WebAssembly.instantiate = async (_source, imports) => {
    wasm = new FakePrivateProgramWasm(imports, plans, memory);
    return { instance: { exports: wasm.exports } };
  };
  try {
    const consumer = await createPrivateProgramConsumer({ target, outputBinding });
    return await body({ consumer, fetched, target, wasm });
  } finally {
    globalThis.fetch = originalFetch;
    WebAssembly.instantiate = originalInstantiate;
  }
}

function installDelegatedLease(target, leaseOrAcquire) {
  const acquire = typeof leaseOrAcquire === "function" ? leaseOrAcquire : () => leaseOrAcquire;
  const authority = Object.create(null);
  Object.defineProperties(authority, {
    protocol: { value: "@labpics/colors/output-sink/v2" },
    acquire: { value: acquire },
  });
  Object.freeze(authority);
  Object.defineProperty(target, Symbol.for("@labpics/colors/output-sink/target-state/v2"), {
    configurable: false,
    value: authority,
    writable: false,
  });
}

test("shared WASM memory is rejected before any ABI call or lease effect", async () => {
  const sharedMemory = new WebAssembly.Memory({ initial: 1, maximum: 1, shared: true });
  let wasm = null;
  const originalFetch = globalThis.fetch;
  const originalInstantiate = WebAssembly.instantiate;
  globalThis.fetch = async () => ({ ok: true, arrayBuffer: async () => new ArrayBuffer(8) });
  WebAssembly.instantiate = async (_source, imports) => {
    wasm = new FakePrivateProgramWasm(imports, ["success"], sharedMemory);
    return { instance: { exports: wasm.exports } };
  };
  try {
    await assert.rejects(
      () => createPrivateProgramConsumer({ target: outputElement(), outputBinding: OUTPUT_BINDING }),
      /unshared ArrayBuffer/u,
    );
    assert.equal(wasm.runCount, 0);
    assert.equal(wasm.beginCalls, 0);
    assert.equal(wasm.lengthCalls, 0);
    assert.deepEqual(wasm.installStatuses, []);
  } finally {
    globalThis.fetch = originalFetch;
    WebAssembly.instantiate = originalInstantiate;
  }
});

test("private consumer returns only the frozen certified receipt and remains reusable", async () => {
  await withFakeProgram({ plans: ["success", "success"] }, ({ consumer, fetched, target, wasm }) => {
    assert.equal(Object.isFrozen(consumer), true);
    assert.match(fetched, /\/private-program\/labcolors_private_program\.wasm$/u);
    assert.equal(consumer.dispose(), true);
    assert.equal(consumer.dispose(), true);
    assert.deepEqual(wasm.beginTokens, []);
    assert.equal(target.outputHost.root.adoptedStyleSheets.length, 0);

    const receipt = consumer.run(requestBytes());
    assert.deepEqual(Object.keys(receipt), [
      "output",
      "sinkOutput",
      "selectedStateIndex",
      "paintSource",
      "paintOpacityBits",
      "contentIdentity",
      "selectionReleaseIdentity",
    ]);
    assert.equal(Object.isFrozen(receipt), true);
    assert.equal(Object.isFrozen(receipt.paintSource), true);
    assert.equal(receipt.output, 17);
    assert.equal(receipt.sinkOutput, 501);
    assert.equal(receipt.selectedStateIndex, 3);
    assert.deepEqual(receipt.paintSource, [64, 64, 64]);
    assert.equal(receipt.paintOpacityBits, HALF_OPACITY_BITS);
    assert.equal(receipt.contentIdentity, expectedHex(Array.from({ length: 32 }, (_, i) => i)));
    assert.equal(
      receipt.selectionReleaseIdentity,
      expectedHex(Array.from({ length: 32 }, (_, i) => 255 - i)),
    );
    assert.throws(() => receipt.paintSource.push(0), TypeError);
    assert.equal(target.props.get(OUTPUT_BINDING), CSS);

    assert.equal(consumer.dispose(), true);
    assert.equal(target.props.has(OUTPUT_BINDING), false);
    const committed = wasm.commitTokens.length;
    assert.equal(consumer.dispose(), true);
    assert.equal(wasm.commitTokens.length, committed, "repeated dispose must have no host effect");

    const nextReceipt = consumer.run(requestBytes());
    assert.equal(nextReceipt.selectedStateIndex, 3);
    assert.equal(target.props.get(OUTPUT_BINDING), CSS);
    assert.deepEqual(wasm.installStatuses.slice(-2), [HOST_INSTALL_SUCCESS, HOST_INSTALL_SUCCESS]);
    assert.equal(consumer.dispose(), true);
  });
});

test("run admits only the exact request carrier without acquiring a host lease", async () => {
  await withFakeProgram({}, ({ consumer, target, wasm }) => {
    assert.throws(() => consumer.run(new Uint8Array(REQUEST_LENGTH - 1)), TypeError);
    assert.throws(() => consumer.run(Buffer.alloc(REQUEST_LENGTH)), TypeError);
    assert.equal(wasm.runCount, 0);
    assert.equal(target.outputHost.root.adoptedStyleSheets.length, 0);
    assert.equal(consumer.dispose(), true);
  });
  await withFakeProgram(
    { outputBinding: "not-a-custom-property" },
    ({ consumer, target, wasm }) => {
      assert.throws(
        () => consumer.run(requestBytes()),
        (error) => error?.code === "OUTPUT_BINDING_INVALID",
      );
      assert.equal(wasm.runCount, 0);
      assert.equal(target.outputHost.root.adoptedStyleSheets.length, 0);
      assert.equal(consumer.dispose(), true);
    },
  );
});

test("a pre-attach run failure directly cleans its provisional lease and can retry", async () => {
  await withFakeProgram(
    { plans: ["pre-attach-failure", "success"] },
    ({ consumer, target, wasm }) => {
      assert.throws(() => consumer.run(requestBytes()), /run failed with status 1/u);
      assert.equal(target.outputHost.root.adoptedStyleSheets.length, 0);
      assert.equal(wasm.beginCalls, 1, "every failed run must probe Core lifecycle");
      assert.deepEqual(wasm.beginTokens, []);
      assert.equal(consumer.run(requestBytes()).sinkOutput, 501);
      assert.equal(consumer.dispose(), true);
    },
  );
});

test("WASM run traps probe and close both Vacant and Active Core lifecycles", async () => {
  await withFakeProgram(
    { plans: ["run-trap-vacant", "run-trap-active", "success"] },
    ({ consumer, target, wasm }) => {
      assert.throws(() => consumer.run(requestBytes()), /vacant run trap/u);
      assert.equal(wasm.beginCalls, 1);
      assert.deepEqual(wasm.beginTokens, []);
      assert.equal(target.outputHost.root.adoptedStyleSheets.length, 0);

      assert.throws(() => consumer.run(requestBytes()), /active run trap/u);
      assert.deepEqual(wasm.beginTokens, [1]);
      assert.deepEqual(wasm.commitTokens, [1]);
      assert.equal(target.outputHost.root.adoptedStyleSheets.length, 0);

      assert.equal(consumer.run(requestBytes()).sinkOutput, 501);
      assert.equal(consumer.dispose(), true);
    },
  );
});

test("an unknown Core lifecycle cleans the host lease and poisons reuse", async () => {
  for (const plan of ["run-trap-probe-busy", "run-trap-probe-throw"]) {
    const target = outputElement();
    let disposes = 0;
    installDelegatedLease(target, {
      publish: () => true,
      dispose() {
        disposes++;
        return true;
      },
    });
    await withFakeProgram({ target, plans: [plan] }, ({ consumer }) => {
      assert.throws(
        () => consumer.run(requestBytes()),
        (error) =>
          error instanceof AggregateError &&
          error.errors.some((cause) => /active run trap/u.test(cause.message)) &&
          error.errors.some((cause) => /probe/u.test(cause.message)),
      );
      assert.equal(disposes, 1, `${plan} must clean the host lease exactly once`);
      assert.equal(consumer.dispose(), true);
      assert.equal(consumer.dispose(), true);
      assert.equal(disposes, 1, "poisoned disposal must have no further host effect");
      assert.throws(
        () => consumer.run(requestBytes()),
        (error) => error?.code === "PRIVATE_PROGRAM_LIFECYCLE" && /poisoned/u.test(error.message),
      );
    });
  }

  await withFakeProgram(
    { plans: ["run-trap-published-probe-busy"] },
    ({ consumer, target }) => {
      assert.throws(() => consumer.run(requestBytes()), AggregateError);
      assert.equal(
        target.props.has(OUTPUT_BINDING),
        false,
        "poisoning must remove a publication installed before the WASM trap",
      );
      assert.equal(consumer.dispose(), true);
      assert.throws(() => consumer.run(requestBytes()), /poisoned/u);
    },
  );
});

test("poisoned host cleanup remains retryable without claiming Core recovery", async () => {
  const target = outputElement();
  let disposes = 0;
  installDelegatedLease(target, {
    publish: () => true,
    dispose() {
      disposes++;
      return disposes === 1 ? Promise.resolve(true) : true;
    },
  });
  await withFakeProgram(
    { target, plans: ["run-trap-probe-busy"] },
    ({ consumer }) => {
      assert.throws(
        () => consumer.run(requestBytes()),
        (error) =>
          error instanceof AggregateError &&
          error.errors.length === 3 &&
          error.errors.some((cause) => /literal true/u.test(cause.message)),
      );
      assert.throws(
        () => consumer.run(requestBytes()),
        (error) =>
          error?.code === "PRIVATE_PROGRAM_LIFECYCLE" && /poisoned-cleanup-required/u.test(error.message),
      );
      assert.equal(consumer.dispose(), true);
      assert.equal(disposes, 2);
      assert.equal(consumer.dispose(), true);
      assert.equal(disposes, 2);
      assert.throws(
        () => consumer.run(requestBytes()),
        (error) => error?.code === "PRIVATE_PROGRAM_LIFECYCLE" && /poisoned/u.test(error.message),
      );
    },
  );
});

test("Vacant provisional cleanup aggregates failure and stays retryable", async () => {
  const target = outputElement();
  let disposes = 0;
  installDelegatedLease(target, {
    publish: () => true,
    dispose() {
      disposes++;
      return disposes === 1 ? Promise.resolve(true) : true;
    },
  });
  await withFakeProgram(
    { target, plans: ["pre-attach-failure", "success"] },
    ({ consumer }) => {
      assert.throws(
        () => consumer.run(requestBytes()),
        (error) =>
          error instanceof AggregateError &&
          error.errors.length === 2 &&
          /run failed with status 1/u.test(error.errors[0].message) &&
          /literal true/u.test(error.errors[1].message),
      );
      assert.equal(consumer.dispose(), true);
      assert.equal(disposes, 2);
      assert.equal(consumer.run(requestBytes()).sinkOutput, 501);
      assert.equal(consumer.dispose(), true);
    },
  );
});

test("host cleanup is exclusive against nested dispose and run", async () => {
  const target = outputElement();
  let consumer = null;
  let disposes = 0;
  let nestedOnce = false;
  const nestedErrors = [];
  installDelegatedLease(target, {
    publish: () => true,
    dispose() {
      disposes++;
      if (!nestedOnce) {
        nestedOnce = true;
        for (const operation of [
          () => consumer.dispose(),
          () => consumer.run(requestBytes()),
        ]) {
          try {
            operation();
          } catch (error) {
            nestedErrors.push(error);
          }
        }
      }
      return true;
    },
  });
  await withFakeProgram(
    { target, plans: ["pre-attach-failure", "success"] },
    ({ consumer: created }) => {
      consumer = created;
      assert.throws(() => consumer.run(requestBytes()), /run failed with status 1/u);
      assert.equal(disposes, 1, "one cleanup attempt must invoke the host exactly once");
      assert.equal(nestedErrors.length, 2);
      assert.ok(nestedErrors.every((error) => error?.code === "PRIVATE_PROGRAM_LIFECYCLE"));
      assert.ok(nestedErrors.every((error) => /cleanup-running/u.test(error.message)));
      assert.equal(consumer.run(requestBytes()).sinkOutput, 501);
      assert.equal(consumer.dispose(), true);
    },
  );
});

test("retryable poisoned cleanup is exclusive and never permits Core reuse", async () => {
  const target = outputElement();
  let consumer = null;
  let disposes = 0;
  const nestedErrors = [];
  installDelegatedLease(target, {
    publish: () => true,
    dispose() {
      disposes++;
      if (disposes === 1) return Promise.resolve(true);
      if (disposes === 2) {
        for (const operation of [
          () => consumer.dispose(),
          () => consumer.run(requestBytes()),
        ]) {
          try {
            operation();
          } catch (error) {
            nestedErrors.push(error);
          }
        }
      }
      return true;
    },
  });
  await withFakeProgram(
    { target, plans: ["run-trap-probe-busy"] },
    ({ consumer: created }) => {
      consumer = created;
      assert.throws(() => consumer.run(requestBytes()), AggregateError);
      assert.equal(disposes, 1);
      assert.equal(consumer.dispose(), true);
      assert.equal(disposes, 2, "the retry must invoke the host exactly once");
      assert.equal(nestedErrors.length, 2);
      assert.ok(nestedErrors.every((error) => error?.code === "PRIVATE_PROGRAM_LIFECYCLE"));
      assert.ok(nestedErrors.every((error) => /poison-cleanup-running/u.test(error.message)));
      assert.equal(consumer.dispose(), true);
      assert.equal(disposes, 2);
      assert.throws(() => consumer.run(requestBytes()), /poisoned/u);
    },
  );
});

test("the sole SetAll publication requires synchronous literal true", async () => {
  const rejected = [
    ["false", false],
    ["throw", new Error("SetAll publication failed")],
    ["Promise", Promise.resolve(true)],
    ["thenable", Object.freeze({ then() {} })],
  ];
  for (const [label, outcome] of rejected) {
    const target = outputElement();
    let publishes = 0;
    let disposes = 0;
    installDelegatedLease(target, {
      publish() {
        publishes++;
        if (outcome instanceof Error) throw outcome;
        return outcome;
      },
      dispose() {
        disposes++;
        return true;
      },
    });
    await withFakeProgram({ target }, ({ consumer, wasm }) => {
      assert.throws(
        () => consumer.run(requestBytes()),
        label === "throw" ? (error) => error === outcome : /literal true/u,
      );
      assert.equal(publishes, 1, label);
      assert.deepEqual(wasm.beginTokens, [1], label);
      assert.deepEqual(wasm.commitTokens, [1], label);
      assert.equal(disposes, 1, label);
      assert.equal(consumer.dispose(), true);
      assert.equal(disposes, 1, `${label} post-cleanup dispose must have no host effect`);
    });
  }
});

test("shipping trace rejects repeat SetAll, RevokeAll, and ConfirmExact before extra publish", async () => {
  for (const plan of ["repeat-set", "set-then-revoke", "set-then-confirm"]) {
    const target = outputElement();
    let publishes = 0;
    let disposes = 0;
    installDelegatedLease(target, {
      publish() {
        publishes++;
        return true;
      },
      dispose() {
        disposes++;
        return true;
      },
    });
    await withFakeProgram({ target, plans: [plan] }, ({ consumer }) => {
      assert.throws(() => consumer.run(requestBytes()), /exactly one SetAll/u, plan);
      assert.equal(publishes, 1, `${plan} must reject before an extra publication`);
      assert.equal(disposes, 1, `${plan} must clean the first publication`);
      assert.equal(consumer.dispose(), true);
    });
  }
});

test("SetAll rejects non-canonical, non-ASCII, and oversized CSS before publish", async () => {
  for (const plan of [
    "css-token",
    "css-utf8",
    "css-oversize",
    "css-noncanonical-alpha",
    "css-scientific-alpha",
    "css-rgb-out-of-range",
  ]) {
    const target = outputElement();
    let publishes = 0;
    let disposes = 0;
    installDelegatedLease(target, {
      publish() {
        publishes++;
        return true;
      },
      dispose() {
        disposes++;
        return true;
      },
    });
    await withFakeProgram({ target, plans: [plan] }, ({ consumer }) => {
      assert.throws(() => consumer.run(requestBytes()), /canonical rgba|CSS length|ASCII/u, plan);
      assert.equal(publishes, 0, `${plan} must fail before publication`);
      assert.equal(disposes, 1, `${plan} must clean the provisional lease`);
      assert.equal(consumer.dispose(), true);
    });
  }
});

test("SetAll admits Rust shortest-roundtrip alpha boundary vectors", async () => {
  const plans = [
    "css-alpha-zero",
    "css-alpha-one",
    "css-alpha-roundtrip",
    "css-alpha-smallest",
  ];
  const target = outputElement();
  let publishes = 0;
  let disposes = 0;
  installDelegatedLease(target, {
    publish() {
      publishes++;
      return true;
    },
    dispose() {
      disposes++;
      return true;
    },
  });
  await withFakeProgram({ target, plans }, ({ consumer }) => {
    for (const plan of plans) {
      assert.equal(consumer.run(requestBytes()).sinkOutput, 501, plan);
      assert.equal(consumer.dispose(), true, plan);
    }
    assert.equal(publishes, plans.length);
    assert.equal(disposes, plans.length);
  });
});

test("a rejected first install probes and closes Active Core before a fresh lease", async () => {
  const target = outputElement();
  let acquisitions = 0;
  let cleanups = 0;
  installDelegatedLease(target, () => {
    acquisitions++;
    const accepted = acquisitions > 1;
    return {
      publish() {
        return accepted;
      },
      dispose() {
        cleanups++;
        return true;
      },
    };
  });
  await withFakeProgram({ target, plans: ["success", "success"] }, ({ consumer, wasm }) => {
    assert.throws(
      () => consumer.run(requestBytes()),
      /did not synchronously return literal true/u,
    );
    assert.deepEqual(wasm.beginTokens, [1], "any entered host install leaves Core Active");
    assert.deepEqual(wasm.commitTokens, [1]);
    assert.equal(wasm.beginCalls, 1);
    assert.equal(acquisitions, 1);
    assert.equal(cleanups, 1);
    assert.equal(consumer.run(requestBytes()).sinkOutput, 501);
    assert.equal(acquisitions, 2);
    assert.equal(consumer.dispose(), true);
    assert.equal(cleanups, 2);
  });
});

test("failed runs probe Core rather than inferring lifecycle from host callbacks", async () => {
  await withFakeProgram(
    { plans: ["active-no-callback", "active-no-install-success", "success"] },
    ({ consumer, target, wasm }) => {
      assert.throws(() => consumer.run(requestBytes()), /run failed with status 8/u);
      assert.deepEqual(wasm.beginTokens, [1]);
      assert.deepEqual(wasm.commitTokens, [1]);
      assert.equal(target.outputHost.root.adoptedStyleSheets.length, 0);

      assert.throws(
        () => consumer.run(requestBytes()),
        /successful run did not install one certified output/u,
      );
      assert.deepEqual(wasm.beginTokens, [1, 2]);
      assert.deepEqual(wasm.commitTokens, [1, 2]);
      assert.equal(target.outputHost.root.adoptedStyleSheets.length, 0);

      assert.equal(consumer.run(requestBytes()).sinkOutput, 501);
      assert.equal(consumer.dispose(), true);
    },
  );
});

test("Core probe token repairs a mismatched callback generation before cleanup", async () => {
  await withFakeProgram(
    { plans: ["generation-mismatch", "success"] },
    ({ consumer, target, wasm }) => {
      assert.throws(
        () => consumer.run(requestBytes()),
        (error) =>
          error instanceof AggregateError &&
          error.errors.some((cause) => /generation disagrees/u.test(cause.message)),
      );
      assert.deepEqual(wasm.beginTokens, [1]);
      assert.deepEqual(wasm.commitTokens, [1]);
      assert.equal(target.outputHost.root.adoptedStyleSheets.length, 0);
      assert.equal(consumer.run(requestBytes()).sinkOutput, 501);
      assert.equal(consumer.dispose(), true);
    },
  );
});

test("a stored host failure dominates a later zero run status and matching result", async () => {
  await withFakeProgram(
    { plans: ["ignored-host-failure", "success"] },
    ({ consumer, target, wasm }) => {
      assert.throws(() => consumer.run(requestBytes()), /exactly one SetAll/u);
      assert.deepEqual(wasm.beginTokens, [1]);
      assert.deepEqual(wasm.commitTokens, [1]);
      assert.equal(target.props.has(OUTPUT_BINDING), false);
      assert.equal(consumer.run(requestBytes()).sinkOutput, 501);
      assert.equal(consumer.dispose(), true);
    },
  );
});

test("host import reentry during publish is rejected and the outer publication is cleaned", async () => {
  const target = outputElement();
  let wasm = null;
  let publishes = 0;
  let disposes = 0;
  const nestedStatuses = [];
  installDelegatedLease(target, {
    publish() {
      publishes++;
      if (publishes === 1) {
        nestedStatuses.push(wasm.callInstall({ generation: wasm.nextGeneration - 1 }));
      }
      return true;
    },
    dispose() {
      disposes++;
      return true;
    },
  });
  await withFakeProgram({ target }, ({ consumer, wasm: instance }) => {
    wasm = instance;
    assert.throws(() => consumer.run(requestBytes()), /reentrant host install/u);
    assert.deepEqual(nestedStatuses, [0]);
    assert.equal(publishes, 1, "the nested callback must not reach the sink");
    assert.equal(disposes, 1, "the outer live publication must be cleaned exactly once");
    assert.deepEqual(wasm.beginTokens, [1]);
    assert.deepEqual(wasm.commitTokens, [1]);
    assert.equal(consumer.dispose(), true);
    assert.equal(disposes, 1);
  });
});

test("WASM i32 carriers reject positive u32 spellings instead of wrapping", async () => {
  for (const plan of ["positive-u32-max-host-generation", "above-u32-host-generation"]) {
    await withFakeProgram({ plans: [plan] }, ({ consumer, target, wasm }) => {
      assert.throws(() => consumer.run(requestBytes()), /signed i32 carrier/u, plan);
      assert.deepEqual(wasm.beginTokens, [1], plan);
      assert.deepEqual(wasm.commitTokens, [1], plan);
      assert.equal(target.props.has(OUTPUT_BINDING), false, plan);
      assert.equal(consumer.dispose(), true);
    });
  }
});

test("commit failure aborts Core but retains the tombstone for a no-revoke retry", async () => {
  await withFakeProgram({}, ({ consumer, target, wasm }) => {
    consumer.run(requestBytes());
    wasm.failNextCommit = true;
    assert.throws(() => consumer.dispose(), /commit dispose failed with status 17/u);
    assert.deepEqual(wasm.beginTokens, [1]);
    assert.deepEqual(wasm.commitTokens, [1]);
    assert.deepEqual(wasm.abortTokens, [1]);
    assert.equal(target.props.has(OUTPUT_BINDING), false);

    assert.equal(consumer.dispose(), true);
    assert.deepEqual(wasm.beginTokens, [1, 1]);
    assert.deepEqual(wasm.commitTokens, [1, 1]);
    assert.deepEqual(wasm.abortTokens, [1]);
    assert.equal(consumer.dispose(), true);
  });
});

test("an unknown abort result cleans a still-live lease and poisons Core reuse", async () => {
  await withFakeProgram({}, ({ consumer, target, wasm }) => {
    consumer.run(requestBytes());
    const revokeFailure = new Error("transient revoke failure before unknown abort");
    target.outputHost.failNextLiveReplace(revokeFailure);
    wasm.nextAbortOutcome = 99;
    assert.throws(
      () => consumer.dispose(),
      (error) =>
        error instanceof AggregateError &&
        error.errors.some((cause) => cause === revokeFailure) &&
        error.errors.some((cause) => /abort dispose failed with status 99/u.test(cause.message)),
    );
    assert.equal(target.props.has(OUTPUT_BINDING), false);
    assert.equal(consumer.dispose(), true);
    assert.throws(() => consumer.run(requestBytes()), /poisoned/u);
  });
});

test("throwing and asynchronous abort outcomes clean the live host and poison Core", async () => {
  const cases = [
    ["throw", new WebAssembly.RuntimeError("abort dispose trap")],
    ["Promise", Promise.resolve(0)],
    ["thenable", Object.freeze({ then() {} })],
  ];
  for (const [label, outcome] of cases) {
    await withFakeProgram({}, ({ consumer, target, wasm }) => {
      consumer.run(requestBytes());
      const revokeFailure = new Error(`transient revoke failure before ${label} abort`);
      target.outputHost.failNextLiveReplace(revokeFailure);
      wasm.nextAbortOutcome = outcome;
      assert.throws(
        () => consumer.dispose(),
        (error) =>
          error instanceof AggregateError &&
          error.errors.some((cause) => cause === revokeFailure) &&
          error.errors.some((cause) => /abort dispose|abort dispose trap/u.test(cause.message)),
        label,
      );
      assert.equal(target.props.has(OUTPUT_BINDING), false, label);
      assert.equal(consumer.dispose(), true);
      assert.throws(() => consumer.run(requestBytes()), /poisoned/u);
    });
  }
});

test("an unknown abort after commit failure preserves the existing host tombstone", async () => {
  await withFakeProgram({}, ({ consumer, target, wasm }) => {
    consumer.run(requestBytes());
    wasm.failNextCommit = true;
    wasm.nextAbortOutcome = 99;
    assert.throws(
      () => consumer.dispose(),
      (error) =>
        error instanceof AggregateError &&
        error.errors.some((cause) => /commit dispose failed with status 17/u.test(cause.message)) &&
        error.errors.some((cause) => /abort dispose failed with status 99/u.test(cause.message)),
    );
    assert.equal(target.props.has(OUTPUT_BINDING), false);
    assert.equal(consumer.dispose(), true);
    assert.throws(() => consumer.run(requestBytes()), /poisoned/u);
  });
});

test("throwing and asynchronous commit outcomes abort without republishing a tombstone", async () => {
  const cases = [
    ["throw", new WebAssembly.RuntimeError("commit dispose trap")],
    ["Promise", Promise.resolve(0)],
    ["thenable", Object.freeze({ then() {} })],
  ];
  for (const [label, outcome] of cases) {
    const target = outputElement();
    let disposes = 0;
    installDelegatedLease(target, {
      publish: () => true,
      dispose() {
        disposes++;
        return true;
      },
    });
    await withFakeProgram({ target }, ({ consumer, wasm }) => {
      consumer.run(requestBytes());
      wasm.nextCommitOutcome = outcome;
      assert.throws(
        () => consumer.dispose(),
        (error) => /commit dispose|commit dispose trap/u.test(error.message),
        label,
      );
      assert.deepEqual(wasm.abortTokens, [1], label);
      assert.equal(disposes, 1, `${label} must install one tombstone`);
      assert.equal(consumer.dispose(), true);
      assert.equal(disposes, 1, `${label} retry must not revoke twice`);
      assert.equal(consumer.dispose(), true);
    });
  }
});

test("divergent or unknown active begin-dispose outcomes clean the host and poison reuse", async () => {
  const cases = [
    ["zero", 0, /returned zero/u],
    ["Busy", DISPOSE_BEGIN_BUSY, /returned Busy/u],
    ["typed status", 12, /failed with status 12/u],
    ["positive u32 max", 0xffff_ffff, /signed i32 carrier/u],
    ["above u32", 0x1_0000_0001, /signed i32 carrier/u],
    ["throw", new WebAssembly.RuntimeError("begin dispose trap"), /begin dispose trap/u],
    ["Promise", Promise.resolve(1), /signed i32 carrier/u],
    ["thenable", Object.freeze({ then() {} }), /signed i32 carrier/u],
  ];
  for (const [label, outcome, expectedFailure] of cases) {
    const target = outputElement();
    let disposes = 0;
    installDelegatedLease(target, {
      publish: () => true,
      dispose() {
        disposes++;
        return true;
      },
    });
    await withFakeProgram({ target }, ({ consumer, wasm }) => {
      assert.equal(consumer.run(requestBytes()).sinkOutput, 501);
      wasm.nextBeginOutcome = outcome;
      assert.throws(
        () => consumer.dispose(),
        (error) =>
          error instanceof AggregateError &&
          error.errors.some((cause) => expectedFailure.test(cause.message)),
        label,
      );
      assert.equal(wasm.beginCalls, 1, label);
      assert.equal(disposes, 1, `${label} must directly clean the host lease`);
      assert.equal(consumer.dispose(), true);
      assert.equal(disposes, 1, `${label} must leave terminal cleanup idempotent`);
      assert.throws(() => consumer.run(requestBytes()), /poisoned/u);
    });
  }
});

test("failed cleanup after divergent active begin remains host-only retryable", async () => {
  const target = outputElement();
  let disposes = 0;
  installDelegatedLease(target, {
    publish: () => true,
    dispose() {
      disposes++;
      return disposes === 1 ? Promise.resolve(true) : true;
    },
  });
  await withFakeProgram({ target }, ({ consumer, wasm }) => {
    assert.equal(consumer.run(requestBytes()).sinkOutput, 501);
    wasm.nextBeginOutcome = 0;
    assert.throws(
      () => consumer.dispose(),
      (error) =>
        error instanceof AggregateError &&
        error.errors.length === 3 &&
        error.errors.some((cause) => /literal true/u.test(cause.message)),
    );
    assert.equal(disposes, 1);
    assert.throws(() => consumer.run(requestBytes()), /poisoned-cleanup-required/u);
    assert.equal(consumer.dispose(), true);
    assert.equal(disposes, 2);
    assert.equal(consumer.dispose(), true);
    assert.equal(disposes, 2);
    assert.throws(() => consumer.run(requestBytes()), /poisoned/u);
  });
});

test("divergent begin after a committed tombstone never disposes the host twice", async () => {
  const cases = [
    ["zero", 0],
    ["Busy", DISPOSE_BEGIN_BUSY],
    ["throw", new WebAssembly.RuntimeError("retry begin trap")],
    ["non-i32", Promise.resolve(1)],
  ];
  for (const [label, outcome] of cases) {
    const target = outputElement();
    let disposes = 0;
    installDelegatedLease(target, {
      publish: () => true,
      dispose() {
        disposes++;
        return true;
      },
    });
    await withFakeProgram({ target }, ({ consumer, wasm }) => {
      consumer.run(requestBytes());
      wasm.failNextCommit = true;
      assert.throws(() => consumer.dispose(), /commit dispose failed with status 17/u);
      assert.equal(disposes, 1, `${label} setup must install one tombstone`);
      assert.deepEqual(wasm.abortTokens, [1], label);

      wasm.nextBeginOutcome = outcome;
      assert.throws(() => consumer.dispose(), AggregateError, label);
      assert.equal(disposes, 1, `${label} must preserve the existing tombstone`);
      assert.equal(consumer.dispose(), true);
      assert.equal(disposes, 1, `${label} poison must be idempotent`);
      assert.throws(() => consumer.run(requestBytes()), /poisoned/u);
    });
  }
});

test("lease acquisition is inside the consumer reentry guard", async () => {
  const target = outputElement();
  let consumer = null;
  const reentryErrors = [];
  installDelegatedLease(target, () => {
    try {
      consumer.dispose();
    } catch (error) {
      reentryErrors.push(error);
    }
    try {
      consumer.run(requestBytes());
    } catch (error) {
      reentryErrors.push(error);
    }
    return { publish: () => true, dispose: () => true };
  });
  await withFakeProgram({ target }, ({ consumer: created }) => {
    consumer = created;
    assert.equal(consumer.run(requestBytes()).sinkOutput, 501);
    assert.equal(reentryErrors.length, 2);
    assert.ok(reentryErrors.every((error) => error?.code === "PRIVATE_PROGRAM_LIFECYCLE"));
    assert.equal(consumer.dispose(), true);
  });
});

test("a caller-owned canonical binding passes unchanged to the output sink", async () => {
  const binding = "--caller-owned-private-output";
  await withFakeProgram({ outputBinding: binding }, ({ consumer, target }) => {
    assert.equal(consumer.run(requestBytes()).sinkOutput, 501);
    assert.equal(target.props.get(binding), CSS);
    assert.equal(consumer.dispose(), true);
  });
});

test("failed host disposal aborts Core disposal and retries the same generation", async () => {
  await withFakeProgram({}, ({ consumer, target, wasm }) => {
    consumer.run(requestBytes());
    const failure = new Error("transient revoke failure");
    target.outputHost.failNextLiveReplace(failure);
    assert.throws(() => consumer.dispose(), (error) => error === failure);
    assert.deepEqual(wasm.beginTokens, [1]);
    assert.deepEqual(wasm.abortTokens, [1]);
    assert.deepEqual(wasm.commitTokens, []);
    assert.equal(target.props.get(OUTPUT_BINDING), CSS);

    assert.equal(consumer.dispose(), true);
    assert.deepEqual(wasm.beginTokens, [1, 1]);
    assert.deepEqual(wasm.commitTokens, [1]);
    assert.equal(target.props.has(OUTPUT_BINDING), false);
    assert.equal(consumer.dispose(), true);
    assert.deepEqual(wasm.commitTokens, [1]);
  });
});

test("authored, imported, and certified sink identities must agree", async () => {
  await withFakeProgram({ plans: ["install-sink-mismatch"] }, ({ consumer, target }) => {
    assert.throws(() => consumer.run(requestBytes()), /authored sink identity/u);
    assert.equal(target.props.has(OUTPUT_BINDING), false);
    assert.equal(consumer.dispose(), true);
  });

  await withFakeProgram(
    { plans: ["result-sink-mismatch", "success"] },
    ({ consumer, target, wasm }) => {
    assert.throws(() => consumer.run(requestBytes()), /installed output identity/u);
      assert.equal(target.props.has(OUTPUT_BINDING), false);
      assert.deepEqual(wasm.beginTokens, [1]);
      assert.deepEqual(wasm.commitTokens, [1]);
      assert.equal(consumer.dispose(), true);
      assert.equal(consumer.run(requestBytes()).sinkOutput, 501);
    assert.equal(consumer.dispose(), true);
    },
  );
});

test("an out-of-bounds result buffer auto-closes the successful Core attachment", async () => {
  await withFakeProgram(
    { plans: ["result-pointer-oob", "success"] },
    ({ consumer, target, wasm }) => {
      assert.throws(() => consumer.run(requestBytes()), /result buffer range/u);
      assert.equal(target.props.has(OUTPUT_BINDING), false);
      assert.deepEqual(wasm.beginTokens, [1]);
      assert.deepEqual(wasm.commitTokens, [1]);
      assert.equal(consumer.dispose(), true);

      wasm.resultPointer = RESULT_POINTER;
      assert.equal(consumer.run(requestBytes()).sinkOutput, 501);
      assert.equal(consumer.dispose(), true);
    },
  );
});
