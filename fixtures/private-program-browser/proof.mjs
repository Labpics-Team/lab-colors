const OUTPUT_BINDING = "--lab-private-program-output";
const EXPECTED_REQUEST_LENGTH = 70;
const EXPECTED_UPDATE_LENGTH = 40;
const EXPECTED_OUTPUT = 17;
const EXPECTED_SINK_OUTPUT = 501;
const EXPECTED_PAINT_SOURCE = Object.freeze([64, 64, 64]);
const EXPECTED_PAINT_OPACITY_BITS_HEX = "3fe0000000000000";
const EXPECTED_COMPUTED_CSS = "rgba(64, 64, 64, 0.5)";

const REQUEST_HEX = "4c43465102004600404040000000000000e03f00000000000050409a9999999999c93f01606060f50100001f0000000100000000000000010100000080808000000000000000";
const UPDATE_HEX = "4c434655020028001f00000002000000000000000101020000008080800000000000000000000000";
const CHANGED_UPDATE_HEX =
  "4c434655020028001f0000000300000000000000010103000000ffffff0000000000000000000000";
const EXPECTED_CONTENT_IDENTITY =
  "4daf4731d823ba228f4189d08c76183dd8a81e8b593ff709828b7329e075a914";

const EXPECTED_CHECKS = Object.freeze([
  "installed-physical-private-program",
  "caller-owned-request-literal",
  "pre-run-dispose-idempotence",
  "exact-computed-css",
  "exact-certified-receipt",
  "explicit-observation-update",
  "changed-observation-invalidates-certified-result",
  "dispose",
  "post-run-dispose-idempotence",
]);

function fail(message) {
  throw new Error(`private Program browser fixture: ${message}`);
}

function assert(condition, message) {
  if (!condition) fail(message);
}

function equal(actual, expected, message) {
  if (!Object.is(actual, expected)) {
    fail(`${message}; expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}

function assertStableLiterals() {
  assert(
    /^(?:[0-9a-f]{2})+$/u.test(REQUEST_HEX),
    "authored request is awaiting ABI_STABLE exact bytes",
  );
  assert(
    /^[0-9a-f]{64}$/u.test(EXPECTED_CONTENT_IDENTITY),
    "content identity is awaiting ABI_STABLE exact bytes",
  );
}

function exactWireBytes(hex) {
  const bytes = new Uint8Array(hex.length / 2);
  for (let index = 0; index < bytes.length; index += 1) {
    bytes[index] = Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16);
  }
  return bytes;
}

async function runProof() {
  assertStableLiterals();
  const checks = [];
  const module = await import("/installed/private-program/consumer.js");
  assert(
    typeof module.createPrivateProgramConsumer === "function",
    "installed physical consumer exports createPrivateProgramConsumer",
  );
  checks.push("installed-physical-private-program");

  const requestBytes = exactWireBytes(REQUEST_HEX);
  equal(
    requestBytes.length,
    EXPECTED_REQUEST_LENGTH,
    "caller-owned request has the exact ABI length",
  );
  checks.push("caller-owned-request-literal");

  const target = document.documentElement;
  const probe = document.querySelector("#private-program-probe");
  assert(probe instanceof HTMLElement, "computed-style probe exists");
  probe.style.backgroundColor = `var(${OUTPUT_BINDING})`;

  const consumer = await module.createPrivateProgramConsumer({
    target,
    outputBinding: OUTPUT_BINDING,
  });
  assert(consumer && typeof consumer.run === "function", "consumer exposes run");
  assert(typeof consumer.update === "function", "consumer exposes update");
  assert(typeof consumer.dispose === "function", "consumer exposes dispose");

  equal(await consumer.dispose(), true, "pre-run dispose returns the exact success literal");
  probe.getBoundingClientRect();
  equal(
    getComputedStyle(probe).backgroundColor,
    "rgba(0, 0, 0, 0)",
    "pre-run dispose leaves no output binding",
  );
  checks.push("pre-run-dispose-idempotence");

  let disposeResult;
  try {
    const receipt = await consumer.run(requestBytes);
    assert(receipt && typeof receipt === "object", "run returns a receipt object");
    assert(Object.isFrozen(receipt), "run receipt is frozen");
    assert(Array.isArray(receipt.paintSource), "paintSource is an array");
    assert(Object.isFrozen(receipt.paintSource), "paintSource is frozen");

    probe.getBoundingClientRect();
    equal(
      getComputedStyle(probe).backgroundColor,
      EXPECTED_COMPUTED_CSS,
      "computed background is the exact expected CSS literal",
    );
    checks.push("exact-computed-css");

    equal(receipt.output, EXPECTED_OUTPUT, "certified output identity");
    equal(receipt.sinkOutput, EXPECTED_SINK_OUTPUT, "certified sink-output identity");
    equal(
      JSON.stringify(receipt.paintSource),
      JSON.stringify(EXPECTED_PAINT_SOURCE),
      "certified paint source",
    );
    assert(typeof receipt.paintOpacityBits === "bigint", "paint opacity is exact bigint bits");
    equal(
      receipt.paintOpacityBits.toString(16).padStart(16, "0"),
      EXPECTED_PAINT_OPACITY_BITS_HEX,
      "certified paint opacity bits",
    );
    equal(receipt.contentIdentity, EXPECTED_CONTENT_IDENTITY, "certified content identity");
    checks.push("exact-certified-receipt");

    const updateBytes = exactWireBytes(UPDATE_HEX);
    equal(updateBytes.length, EXPECTED_UPDATE_LENGTH, "update has the exact ABI length");
    const updated = await consumer.update(updateBytes);
    equal(updated.state, 2, "updated state is Ready");
    equal(updated.stream, 31, "updated state retains stream identity");
    equal(updated.revision, 2n, "updated state advances revision");
    equal(updated.contentIdentity, EXPECTED_CONTENT_IDENTITY, "update reuses compiled identity");
    checks.push("explicit-observation-update");

    const changedUpdateBytes = exactWireBytes(CHANGED_UPDATE_HEX);
    equal(changedUpdateBytes.length, EXPECTED_UPDATE_LENGTH, "changed update has the exact ABI length");
    const changed = await consumer.update(changedUpdateBytes);
    equal(changed.state, 4, "changed observation state is Failed");
    equal(changed.stream, 31, "failed state retains stream identity");
    equal(changed.revision, 3n, "failed state advances revision");
    equal(changed.output, 0, "failed state has no certified output");
    equal(changed.contentIdentity, "0".repeat(64), "failed state has no certified identity");
    probe.getBoundingClientRect();
    equal(
      getComputedStyle(probe).backgroundColor,
      "rgba(0, 0, 0, 0)",
      "changed observation revokes the previous certified output",
    );
    checks.push("changed-observation-invalidates-certified-result");
  } finally {
    disposeResult = await consumer.dispose();
  }

  equal(disposeResult, true, "consumer disposal returns the exact success literal");
  probe.getBoundingClientRect();
  equal(
    getComputedStyle(probe).backgroundColor,
    "rgba(0, 0, 0, 0)",
    "dispose removes the installed output binding",
  );
  checks.push("dispose");
  equal(
    await consumer.dispose(),
    true,
    "post-run repeated dispose returns the exact success literal",
  );
  probe.getBoundingClientRect();
  equal(
    getComputedStyle(probe).backgroundColor,
    "rgba(0, 0, 0, 0)",
    "post-run repeated dispose cannot resurrect output CSS",
  );
  checks.push("post-run-dispose-idempotence");
  equal(JSON.stringify(checks), JSON.stringify(EXPECTED_CHECKS), "ordered proof checks");
  return Object.freeze({ checks: Object.freeze([...checks]) });
}

globalThis.__LAB_COLORS_PRIVATE_PROGRAM_PROOF__ = runProof();
