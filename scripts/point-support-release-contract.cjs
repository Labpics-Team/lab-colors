"use strict";

const POINT_SUPPORT_CERTIFIED_CLAIM =
  "for every successfully evaluated enabled stability cell, decision is Retained iff current_lower_surplus >= (10000-drop_bps)/10000 * max(baseline_lower_surplus,0); the declared anchor remains a separate hard floor";
const POINT_SUPPORT_EXCLUDED_CLAIM =
  "does not certify retention against the unknown exact baseline surplus, renderer equivalence outside encoded-sRGB8 source-over, or a successful result when evaluation fails";
const POINT_SUPPORT_SOURCE_BINDING_SCOPE =
  "exact bytes of the private point-support Rust semantic cone and its two WCAG include_str inputs; comments and cfg(test) text are intentionally significant";
const POINT_SUPPORT_SOURCE_BINDING_EXCLUSIONS = Object.freeze([
  "whole-crate compilation or compiler/toolchain attestation",
  "binary, package, FFI, renderer, or browser transport attestation",
  "unrelated Lab Colors modules outside the declared point-support semantic cone",
]);
const POINT_SUPPORT_SOURCE_PATHS = Object.freeze(
  [
    "crates/labcolors-core/contracts/wcag22-srgb8-q55-proof-v1.json",
    "crates/labcolors-core/contracts/wcag22-srgb8-v1.json",
    "crates/labcolors-core/src/appearance.rs",
    "crates/labcolors-core/src/composition.rs",
    "crates/labcolors-core/src/constraints/exact.rs",
    "crates/labcolors-core/src/constraints/mod.rs",
    "crates/labcolors-core/src/constraints/wcag22.rs",
    "crates/labcolors-core/src/hash.rs",
    "crates/labcolors-core/src/lcs_occurrence.rs",
    "crates/labcolors-core/src/lib.rs",
    "crates/labcolors-core/src/numerics.rs",
    "crates/labcolors-core/src/observation.rs",
    "crates/labcolors-core/src/point_support.rs",
    "crates/labcolors-core/src/session.rs",
    "crates/labcolors-core/src/srgb8.rs",
    "crates/labcolors-core/src/wcag22.rs",
    "crates/labcolors-core/src/wcag22/kernel.rs",
    "crates/labcolors-core/src/wcag22/q55_data.rs",
    "crates/labcolors-core/src/wcag22_evidence.rs",
  ].sort(),
);

// The proof carries exact Q55/u128 integer lexemes beyond Number.MAX_SAFE_INTEGER.
// Remove its self-digest from canonical raw bytes without a lossy JSON round-trip.
function exactJsonPayloadWithoutTopLevelField(bytes, field, label, reject) {
  if (typeof reject !== "function") {
    throw new TypeError("exact JSON rejection callback must be a function");
  }
  function rejectInvalid(message) {
    reject(message);
    throw new Error(`exact JSON rejection callback returned: ${message}`);
  }
  if (
    !Buffer.isBuffer(bytes) ||
    bytes.length < 3 ||
    bytes[bytes.length - 1] !== 0x0a ||
    bytes[bytes.length - 2] !== 0x7d
  ) {
    rejectInvalid(`${label} must be one canonical JSON object followed by one LF`);
  }

  const body = bytes.subarray(0, -1);
  if (body[0] !== 0x7b) {
    rejectInvalid(`${label} must have a top-level JSON object`);
  }

  const members = [];
  const containers = [0x7b];
  let memberStart = 1;
  let inString = false;
  let escaped = false;

  function addMember(end) {
    if (end === memberStart) {
      rejectInvalid(`${label} has an empty or trailing top-level member`);
    }
    let keyEnd = memberStart + 1;
    if (body[memberStart] !== 0x22) {
      rejectInvalid(`${label} has a non-string top-level key`);
    }
    let keyEscaped = false;
    for (; keyEnd < end; keyEnd += 1) {
      const byte = body[keyEnd];
      if (keyEscaped) {
        keyEscaped = false;
      } else if (byte === 0x5c) {
        keyEscaped = true;
      } else if (byte === 0x22) {
        break;
      }
    }
    if (keyEnd >= end || body[keyEnd + 1] !== 0x3a) {
      rejectInvalid(`${label} has a malformed top-level member`);
    }
    const rawKey = body.subarray(memberStart, keyEnd + 1);
    const key = JSON.parse(rawKey.toString("utf8"));
    if (!rawKey.equals(Buffer.from(JSON.stringify(key), "utf8"))) {
      rejectInvalid(`${label} has a non-canonical top-level key`);
    }
    members.push({ start: memberStart, end, key, rawKey });
  }

  for (let index = 1; index < body.length; index += 1) {
    const byte = body[index];
    if (inString) {
      if (escaped) {
        escaped = false;
      } else if (byte === 0x5c) {
        escaped = true;
      } else if (byte === 0x22) {
        inString = false;
      }
      continue;
    }
    if (byte === 0x22) {
      inString = true;
      continue;
    }
    if (byte === 0x20 || byte === 0x09 || byte === 0x0a || byte === 0x0d) {
      rejectInvalid(`${label} contains non-canonical insignificant whitespace`);
    }
    if (byte === 0x7b || byte === 0x5b) {
      containers.push(byte);
      continue;
    }
    if (byte === 0x7d || byte === 0x5d) {
      const expectedOpen = byte === 0x7d ? 0x7b : 0x5b;
      if (containers.at(-1) !== expectedOpen) {
        rejectInvalid(`${label} has mismatched JSON containers`);
      }
      if (containers.length === 1) {
        if (byte !== 0x7d || index !== body.length - 1) {
          rejectInvalid(`${label} has bytes after its top-level object`);
        }
        if (index !== memberStart || members.length > 0) addMember(index);
      }
      containers.pop();
      continue;
    }
    if (byte === 0x2c && containers.length === 1) {
      addMember(index);
      memberStart = index + 1;
    }
  }

  if (inString || containers.length !== 0) {
    rejectInvalid(`${label} is not a complete JSON object`);
  }
  for (let index = 1; index < members.length; index += 1) {
    if (Buffer.compare(members[index - 1].rawKey, members[index].rawKey) >= 0) {
      rejectInvalid(`${label} top-level keys are duplicate or unsorted`);
    }
  }

  const targetIndex = members.findIndex(({ key }) => key === field);
  if (
    targetIndex < 0 ||
    members.some(({ key }, index) => key === field && index !== targetIndex)
  ) {
    rejectInvalid(`${label} must contain exactly one top-level ${field}`);
  }
  const target = members[targetIndex];
  if (members.length === 1) return Buffer.from("{}", "utf8");
  if (targetIndex === 0) {
    return Buffer.concat([body.subarray(0, target.start), body.subarray(target.end + 1)]);
  }
  return Buffer.concat([body.subarray(0, target.start - 1), body.subarray(target.end)]);
}

module.exports = Object.freeze({
  POINT_SUPPORT_CERTIFIED_CLAIM,
  POINT_SUPPORT_EXCLUDED_CLAIM,
  POINT_SUPPORT_SOURCE_BINDING_SCOPE,
  POINT_SUPPORT_SOURCE_BINDING_EXCLUSIONS,
  POINT_SUPPORT_SOURCE_PATHS,
  exactJsonPayloadWithoutTopLevelField,
});
