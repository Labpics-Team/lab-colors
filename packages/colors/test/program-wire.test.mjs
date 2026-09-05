// Кросс-язычный parity канонической wire-грамматики Program v1: одни
// декларации -> одни байты на JS и Rust. Reference hex эмитирован Rust-тестом
// wire::tests::_emit_reference_wire_hex из того же графа; дрейф любой стороны
// формата ломает сравнение.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const PACKAGE_ROOT = dirname(dirname(fileURLToPath(import.meta.url)));

import {
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

function npmInvocation({
  platform = process.platform,
  npmExecPath = process.env.npm_execpath,
  node = process.execPath,
} = {}) {
  if (npmExecPath) return { command: node, argsPrefix: [npmExecPath], shell: false };
  return {
    command: platform === "win32" ? "npm.cmd" : "npm",
    argsPrefix: [],
    shell: platform === "win32",
  };
}

function npm(args, cwd) {
  const { command, argsPrefix, shell } = npmInvocation();
  return execFileSync(command, [...argsPrefix, ...args], {
    cwd,
    encoding: "utf8",
    shell,
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

test("npm command selection honors the lifecycle entrypoint", () => {
  assert.deepEqual(
    npmInvocation({ platform: "linux", npmExecPath: "/npm/cli.js", node: "/node" }),
    { command: "/node", argsPrefix: ["/npm/cli.js"], shell: false },
  );
});

test("npm command selection falls back to the platform PATH shim", () => {
  assert.deepEqual(npmInvocation({ platform: "win32", npmExecPath: undefined }), {
    command: "npm.cmd",
    argsPrefix: [],
    shell: true,
  });
  assert.deepEqual(npmInvocation({ platform: "linux", npmExecPath: undefined }), {
    command: "npm",
    argsPrefix: [],
    shell: false,
  });
});

test("packed ProgramWire subpath resolves with runtime and declarations", () => {
  const fixture = mkdtempSync(join(tmpdir(), "labcolors-program-wire-pack-"));
  try {
    const [{ filename }] = JSON.parse(
      npm(["pack", "--ignore-scripts", "--json", `--pack-destination=${fixture}`, PACKAGE_ROOT], fixture),
    );
    writeFileSync(
      join(fixture, "package.json"),
      `${JSON.stringify({ private: true, type: "module" })}\n`,
    );
    npm(
      ["install", "--ignore-scripts", "--no-audit", "--no-fund", join(fixture, filename)],
      fixture,
    );
    const output = execFileSync(
      process.execPath,
      [
        "--input-type=module",
        "--eval",
        'import { ProgramWireBuilderV1 } from "@labpics/colors/program-wire/abi-v1.js"; console.log(new ProgramWireBuilderV1().finish().length)',
      ],
      { cwd: fixture, encoding: "utf8" },
    ).trim();
    assert.equal(output, "66");

    writeFileSync(
      join(fixture, "smoke.ts"),
      'import { ProgramWireBuilderV1 } from "@labpics/colors/program-wire/abi-v1.js";\nnew ProgramWireBuilderV1().source(1, [0, 0, 0]).finish();\n',
    );
    execFileSync(
      process.execPath,
      [
        join(PACKAGE_ROOT, "node_modules", "typescript", "lib", "tsc.js"),
        "--noEmit",
        "--strict",
        "--module",
        "NodeNext",
        "--moduleResolution",
        "NodeNext",
        "--target",
        "ES2022",
        join(fixture, "smoke.ts"),
      ],
      { cwd: fixture, stdio: ["ignore", "pipe", "pipe"] },
    );
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});

test("JS builder bytes are byte-identical to the Rust reference wire", () => {
  assert.equal(toHex(referenceBuilder().finish()), REFERENCE_WIRE_HEX);
});

test("canonical bytes are deterministic across rebuilds", () => {
  assert.deepEqual(referenceBuilder().finish(), referenceBuilder().finish());
});

test("oversized sections are refused with a typed code before emission", () => {
  const builder = new ProgramWireBuilderV1();
  for (let id = 0; id <= 4096; id += 1) {
    builder.surfaceInputPort(id);
  }
  assert.throws(
    () => builder.finish(),
    (error) =>
      error instanceof ProgramWireError && error.code === PROGRAM_WIRE_TOO_MANY_ENTRIES,
  );
});

test("invalid declarations are typed refusals, not coerced bytes", () => {
  const isTyped = (error) => error instanceof ProgramWireError;
  const fresh = () => new ProgramWireBuilderV1();
  assert.throws(() => fresh().source(-1, [0, 0, 0]), isTyped);
  assert.throws(() => fresh().source(1, [0, 0]), isTyped);
  assert.throws(() => fresh().source(1, [0, 0, 256]), isTyped);
  // silent f64 coercion paths: NaN/undefined/string must refuse, not encode.
  assert.throws(() => fresh().opacityInput(1, Number.NaN), isTyped);
  assert.throws(() => fresh().opacityInput(1, undefined), isTyped);
  assert.throws(
    () => fresh().sourceOverOccurrence(1, 2, 3, "bright", 0.2, 1),
    isTyped,
  );
  // enum domains: byte-range values outside the registered sets must refuse.
  assert.throws(
    () => fresh().sourceOverOccurrence(1, 2, 3, 64.0, 0.2, 7),
    isTyped,
  );
  assert.throws(() => fresh().wcag22VisibleUnary(true, 1, 2, 9), isTyped);
  // structured candidates: null/primitives are typed refusals, never TypeError.
  assert.throws(() => fresh().finiteTarget(1, [null]), isTyped);
  assert.throws(() => fresh().finiteTarget(1, [7]), isTyped);
  // relation candidates: null/empty are typed refusals, never TypeError.
  assert.throws(() => fresh().exactIntrinsicRelationHard(1, 2, null), isTyped);
  assert.throws(() => fresh().exactIntrinsicRelationHard(1, 2, []), isTyped);
});

test("every refused declaration leaves the builder byte-stream untouched", () => {
  const invalidDeclarations = [
    (builder) => builder.source(-1, [0, 0, 0]),
    (builder) => builder.source(1, [0, 0]),
    (builder) => builder.fixedTarget(1, -1),
    (builder) => builder.finiteTarget(1, [{ id: 1, rgb: [0, 0, 0], opacity: Number.NaN }]),
    (builder) => builder.finiteTarget(1, [null]),
    (builder) => builder.finiteTarget(1, [7]),
    (builder) => builder.family(1, Array(31).fill(0)),
    (builder) => builder.family(1, [...Array(31).fill(0), 256]),
    (builder) => builder.surfaceInputPort(-1),
    (builder) => builder.opacityInput(1, Number.NaN),
    (builder) => builder.solidPaint(1, -1),
    (builder) => builder.opacityPaint(1, 2, -1),
    (builder) => builder.inputSurface(1, -1),
    (builder) => builder.occurrenceSurface(1, -1),
    (builder) => builder.sourceOverOccurrence(1, 2, 3, Number.NaN, 0.2, 1),
    (builder) => builder.presentationRoot(1, -1),
    (builder) => builder.presentationTarget(1, -1),
    (builder) => builder.exactVisibleUnary(true, 1, 2, [0, 0]),
    (builder) => builder.wcag22VisibleUnary(true, 1, 2, 9),
    (builder) => builder.exactIntrinsicRelationHard(1, 2, []),
    (builder) => builder.output(1, -1),
  ];
  const expected = new ProgramWireBuilderV1().source(11, [0x14, 0x14, 0x14]).finish();

  for (const declare of invalidDeclarations) {
    const builder = new ProgramWireBuilderV1().source(11, [0x14, 0x14, 0x14]);
    assert.throws(() => declare(builder), (error) => error instanceof ProgramWireError);
    assert.deepEqual(builder.finish(), expected);
  }
});
