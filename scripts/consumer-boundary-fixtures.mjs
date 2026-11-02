import assert from "node:assert/strict";

export const invalidU32 = [
  ["negative-subnormal", -Number.MIN_VALUE],
  ["positive-subnormal", Number.MIN_VALUE],
  ["one-plus-ulp", 1 + Number.EPSILON],
  ["maximum-minus-half", 0xffff_ffff - 0.5],
];
export const legalStreams = [0, -0, 1, 0x8000_0000, 0xffff_ffff];
export const criteria = [
  "sc-1.4.3-text-default", "sc-1.4.3-text-large-scale",
  "sc-1.4.11-ui-component-or-state", "sc-1.4.11-graphical-object",
];
export const manifestKeys = ["schemaVersion", "coverage", "sites", "checksum"];
export const siteKeys = [
  "siteId", "stableOutcomes", "compatibilityReleases", "evidenceClasses",
  "artifactIds", "boundIds", "proofIds", "runtimeAttestations",
];
export const assessmentKeys = [
  "kind", "profileId", "criterion", "foreground", "background",
  "foregroundLuminanceQ55", "backgroundLuminanceQ55", "q55Scale", "decision", "evidence",
];
export const evidenceKeys = [
  "kind", "artifactId", "artifactSha256", "boundId", "proofId", "proofSha256",
  "proofPayloadSha256", "generatorSha256", "verifierSha256", "profileChecksum", "profileSha256",
];

export function exactKeys(value, keys) {
  assert.deepEqual(Object.keys(value).sort(), [...keys].sort());
}

export function ordinaryData(value, path = "$") {
  if (value === null) return;
  if (typeof value !== "object") {
    assert.ok(["string", "number", "boolean"].includes(typeof value), path);
    return;
  }
  const array = Array.isArray(value);
  assert.equal(Object.getPrototypeOf(value), array ? Array.prototype : Object.prototype, path);
  assert.deepEqual(Object.getOwnPropertySymbols(value), [], path);
  for (const key of Object.getOwnPropertyNames(value)) {
    const property = Object.getOwnPropertyDescriptor(value, key);
    assert.ok(Object.hasOwn(property, "value"), path + "." + key + " must be data");
    assert.equal(property.writable, true, path + "." + key);
    assert.equal(property.enumerable, !(array && key === "length"), path + "." + key);
    assert.equal(property.configurable, !(array && key === "length"), path + "." + key);
    if (!(array && key === "length")) ordinaryData(property.value, path + "." + key);
  }
}

export function freshContainers(left, right) {
  if (left === null || typeof left !== "object") return;
  assert.notStrictEqual(left, right);
  for (const key of Object.keys(left)) freshContainers(left[key], right[key]);
}

export function mutateLeaves(value) {
  for (const key of Object.keys(value)) {
    if (value[key] !== null && typeof value[key] === "object") mutateLeaves(value[key]);
    else value[key] = typeof value[key] === "number" ? value[key] + 1 : "caller mutation";
  }
}

// Меняем prototypes только вокруг синхронного вызова; assertions выполняются после восстановления.
export function withProjectionSetters(invoke) {
  const keys = [...new Set([...manifestKeys, ...siteKeys, ...assessmentKeys, ...evidenceKeys, "lower", "upper"])];
  const traps = [
    ...keys.map((key) => ({ prototype: Object.prototype, key })),
    ...["0", "1"].map((key) => ({ prototype: Array.prototype, key })),
  ];
  for (const trap of traps) trap.previous = Object.getOwnPropertyDescriptor(trap.prototype, trap.key);
  let installed = 0, calls = 0;
  try {
    for (const trap of traps) {
      Object.defineProperty(trap.prototype, trap.key, {
        configurable: true,
        set() { calls += 1; throw new Error("inherited projection setter"); },
      });
      installed += 1;
    }
    return { result: invoke(), calls };
  } finally {
    for (let index = installed - 1; index >= 0; index -= 1) {
      const trap = traps[index];
      if (trap.previous) Object.defineProperty(trap.prototype, trap.key, trap.previous);
      else delete trap.prototype[trap.key];
    }
  }
}

export function fixedWire(Builder, version) {
  const builder = new Builder();
  if (version === 3) {
    builder.computedTarget(21, { kind: "box", lower: [20, 20, 20], upper: [20, 20, 20] },
      [20, 20, 20], 0).assignment([21]);
  } else builder.source(11, [20, 20, 20]).fixedTarget(21, 11);
  return builder.surfaceInputPort(31).solidPaint(41, 21).inputSurface(51, 31)
    .sourceOverOccurrence(61, 41, 51, 64, 0.2, 1)
    .presentationRoot(71, 61).presentationTarget(71, 61)
    .wcag22VisibleUnary(true, 81, 61, 3).output(91, 41).finish();
}

// revision/count передаются дословно: fixture не нормализует проверяемые scalar inputs.
export function observe(owner, revision, count, background) {
  return owner.updateObserved(revision, new Uint32Array([1]),
    new Uint8Array([background, background, background]), count);
}
export function outputs(snapshot) {
  return Array.from({ length: snapshot.outputCount() }, (_, index) => ({
    slot: snapshot.outputSlot(index), rgb: [...snapshot.outputRgb(index)],
    opacity: snapshot.outputOpacity(index),
  }));
}
export const authoredOutput = [{ slot: 91, rgb: [20, 20, 20], opacity: 1 }];
