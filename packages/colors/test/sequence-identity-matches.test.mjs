import test from "node:test";
import assert from "node:assert/strict";

import { sequenceIdentityMatches } from "../sequence-identity-matches.js";

const A = Object.freeze({ id: "A" });
const B = Object.freeze({ id: "B" });
const C = Object.freeze({ id: "C" });
const X = Object.freeze({ id: "X" });

function denseLcsLength(left, right) {
  // The exhaustive vectors are at most seven entries, so Uint16 cannot wrap.
  const rows = Array.from(
    { length: left.length + 1 },
    () => new Uint16Array(right.length + 1),
  );
  for (let leftIndex = left.length - 1; leftIndex >= 0; leftIndex--) {
    for (let rightIndex = right.length - 1; rightIndex >= 0; rightIndex--) {
      rows[leftIndex][rightIndex] =
        left[leftIndex] === right[rightIndex]
          ? rows[leftIndex + 1][rightIndex + 1] + 1
          : Math.max(rows[leftIndex + 1][rightIndex], rows[leftIndex][rightIndex + 1]);
    }
  }
  return rows[0][0];
}

function sequences(alphabet, length) {
  const result = [];
  const cardinality = alphabet.length ** length;
  for (let ordinal = 0; ordinal < cardinality; ordinal++) {
    const sequence = [];
    let encoded = ordinal;
    for (let index = 0; index < length; index++) {
      sequence.push(alphabet[encoded % alphabet.length]);
      encoded = Math.floor(encoded / alphabet.length);
    }
    result.push(sequence);
  }
  return result;
}

test("suffix-canonical identity alignment pins every observable tie", () => {
  assert.deepEqual(sequenceIdentityMatches([], []), []);
  assert.deepEqual(sequenceIdentityMatches([A, B], [X]), []);
  assert.deepEqual(sequenceIdentityMatches([A, A], [A]), [[0, 0]]);
  assert.deepEqual(sequenceIdentityMatches([A], [A, A]), [[0, 0]]);
  assert.deepEqual(sequenceIdentityMatches([A, B], [B, A]), [[1, 0]]);
  assert.deepEqual(sequenceIdentityMatches([A, A], [X, A]), [[1, 1]]);
  assert.deepEqual(sequenceIdentityMatches([A, B, A], [A, A]), [
    [0, 0],
    [2, 1],
  ]);
  assert.deepEqual(sequenceIdentityMatches([A, B, A], [C, C, A, A, C]), [
    [0, 2],
    [2, 3],
  ]);
  assert.deepEqual(sequenceIdentityMatches([A, A, B, B], [B, A, A, A, A, A]), [
    [0, 1],
    [1, 2],
  ]);
  assert.deepEqual(
    sequenceIdentityMatches(
      [Object.freeze({ id: 1 })],
      [Object.freeze({ id: 1 })],
    ),
    [],
    "equal payloads do not substitute for stylesheet identity",
  );
});

test("linear-space Myers is deterministic and maximum for every binary sequence through length 7", () => {
  const all = [];
  for (let length = 0; length <= 7; length++) {
    all.push(...sequences([A, B], length));
  }
  assert.equal(all.length, 255, "anti-vacuity: exhaustive sequence inventory");

  let checked = 0;
  for (const left of all) {
    for (const right of all) {
      const actual = sequenceIdentityMatches(left, right);
      assert.deepEqual(sequenceIdentityMatches(left, right), actual, "alignment is deterministic");
      assert.equal(actual.length, denseLcsLength(left, right), "alignment is maximum");
      let priorLeft = -1;
      let priorRight = -1;
      for (const [leftIndex, rightIndex] of actual) {
        assert.ok(
          Number.isInteger(leftIndex) && leftIndex >= 0 && leftIndex < left.length,
          "left index is an in-range integer",
        );
        assert.ok(
          Number.isInteger(rightIndex) && rightIndex >= 0 && rightIndex < right.length,
          "right index is an in-range integer",
        );
        assert.ok(leftIndex > priorLeft, "left indices increase strictly");
        assert.ok(rightIndex > priorRight, "right indices increase strictly");
        assert.equal(left[leftIndex], right[rightIndex], "a match preserves strict identity");
        priorLeft = leftIndex;
        priorRight = rightIndex;
      }
      checked++;
    }
  }
  assert.equal(checked, 65_025, "anti-vacuity: every ordered sequence pair was checked");
});
