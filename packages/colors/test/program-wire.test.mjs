// Кросс-язычный parity канонической wire-грамматики Program v1: одни
// декларации -> одни байты на JS и Rust. Reference hex эмитирован Rust-тестом
// wire::tests::_emit_reference_wire_hex из того же графа; дрейф любой стороны
// формата ломает сравнение.
import assert from "node:assert/strict";
import test from "node:test";

import {
  MAX_SECTION_ENTRIES_V1,
  PROGRAM_WIRE_TOO_MANY_ENTRIES,
  ProgramWireBuilderV1,
  ProgramWireError,
  SURROUND_AVERAGE_V1,
  WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1,
} from "../program-wire/abi-v1.js";

// Тот же граф, что reference_wire() в crates/labcolors-core/src/program/wire.rs.
const REFERENCE_WIRE_HEX =
  "4c4350570100b3000000010000000b0000001414140100000015000000010b0000000000" +
  "000000000000010000001f00000000000000010000002900000001150000000100000033" +
  "000000011f000000010000003d000000290000003300000000000000000050409a999999" +
  "9999c93f0101000000470000003d00000001000000470000003d00000001000000510000" +
  "00093d000000030100000052000000013d000000141414010000005b00000029000000";

function referenceBuilder() {
  const builder = new ProgramWireBuilderV1();
  builder
    .source(11, [0x14, 0x14, 0x14])
    .fixedTarget(21, 11)
    .surfaceInputPort(31)
    .solidPaint(41, 21)
    .inputSurface(51, 31)
    .sourceOverOccurrence(61, 41, 51, 64.0, 0.2, SURROUND_AVERAGE_V1)
    .presentationRoot(71, 61)
    .presentationTarget(71, 61)
    .wcag22VisibleUnary(true, 81, 61, WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1)
    .exactVisibleUnary(false, 82, 61, [0x14, 0x14, 0x14])
    .output(91, 41);
  return builder;
}

function toHex(bytes) {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

test("JS builder bytes are byte-identical to the Rust reference wire", () => {
  assert.equal(toHex(referenceBuilder().finish()), REFERENCE_WIRE_HEX);
});

test("canonical bytes are deterministic across rebuilds", () => {
  assert.deepEqual(referenceBuilder().finish(), referenceBuilder().finish());
});

test("oversized sections are refused with a typed code before emission", () => {
  const builder = new ProgramWireBuilderV1();
  for (let id = 0; id <= MAX_SECTION_ENTRIES_V1; id += 1) {
    builder.surfaceInputPort(id);
  }
  assert.throws(
    () => builder.finish(),
    (error) =>
      error instanceof ProgramWireError && error.code === PROGRAM_WIRE_TOO_MANY_ENTRIES,
  );
});

test("invalid declarations are typed refusals, not coerced bytes", () => {
  const builder = new ProgramWireBuilderV1();
  assert.throws(
    () => builder.source(-1, [0, 0, 0]),
    (error) => error instanceof ProgramWireError,
  );
  assert.throws(
    () => builder.source(1, [0, 0]),
    (error) => error instanceof ProgramWireError,
  );
  assert.throws(
    () => builder.source(1, [0, 0, 256]),
    (error) => error instanceof ProgramWireError,
  );
});
