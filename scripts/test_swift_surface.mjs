import assert from "node:assert/strict";
import test from "node:test";
import { authoringSurface, verifySwiftSurface } from "./verify-swift-surface.mjs";

const declarations = {
  "v1.d.ts": "export class ProgramWireBuilderV1 { source(id: number, rgb: number[]): this; finish(): Uint8Array; }",
  "v2.d.ts": "export class ProgramWireBuilderV2 extends ProgramWireBuilderV1 { selectionRelease(revision: bigint, groups: unknown[]): this; finish(): Uint8Array; }",
};
const inventory = { schemaVersion: 1, operations: {
  "fn::compile_program_wire": 2, "ProgramRuntime::update_observed": 4,
} };
function operation(owner, name, arity) {
  return { kind: { identifier: owner === "fn" ? "swift.func" : "swift.method" },
    accessLevel: "public", pathComponents: [...(owner === "fn" ? [] : [owner]), name + "(" + "_:".repeat(arity) + ")"],
    functionSignature: { parameters: Array.from({ length: arity }, () => ({ name: "argument" })) } };
}
function graph() {
  return { module: { name: "LabColors" }, symbols: [
    ...["ProgramWireBuilderV1", "ProgramWireBuilderV2"].map((name) => ({ kind: { identifier: "swift.class" },
      accessLevel: "public", pathComponents: [name], identifier: { precise: name } })),
    operation("fn", "compileProgramWire", 2), operation("ProgramRuntime", "updateObserved", 4),
    operation("ProgramWireBuilderV1", "source", 2), operation("ProgramWireBuilderV1", "finish", 0),
    operation("ProgramWireBuilderV2", "selectionRelease", 2),
  ], relationships: [{ kind: "inheritsFrom", source: "ProgramWireBuilderV2", target: "ProgramWireBuilderV1" }] };
}

test("compiled inheritance and additional lawful SDK operations are admitted", () => {
  const compiled = graph(); compiled.symbols.push(operation("fn", "nativeUtility", 1));
  assert.equal(verifySwiftSurface(inventory, declarations, compiled), 6);
});

test("new native or authoring operations require a real public compiled symbol", () => {
  const next = structuredClone(inventory); next.operations["fn::next_operation"] = 1;
  assert.throws(() => verifySwiftSurface(next, declarations, graph()), /nextOperation/u);
  const compiled = graph(); compiled.symbols.push(operation("fn", "nextOperation", 1));
  assert.equal(verifySwiftSurface(next, declarations, compiled), 7);
  const updated = { ...declarations, "v2.d.ts": declarations["v2.d.ts"].replace("finish():", "choose(id: number): this; finish():") };
  assert.throws(() => verifySwiftSurface(inventory, updated, graph()), /choose/u);
});

test("a hidden or wrong-arity method does not count as consumer coverage", () => {
  for (const mutate of [
    (symbol) => { symbol.accessLevel = "internal"; },
    (symbol) => { symbol.functionSignature.parameters.pop(); },
    (symbol) => { symbol.pathComponents[0] = "PrivateRuntime"; },
  ]) {
    const compiled = graph(); mutate(compiled.symbols.find((symbol) => symbol.pathComponents[0] === "ProgramRuntime"));
    assert.throws(() => verifySwiftSurface(inventory, declarations, compiled), /updateObserved/u);
  }
});

test("missing compiler output or discovery roots cannot produce success", () => {
  assert.throws(() => verifySwiftSurface({ schemaVersion: 1, operations: {} }, declarations, graph()), /incomplete/u);
  assert.throws(() => verifySwiftSurface(inventory, declarations, { ...graph(), symbols: [] }), /missing operations/u);
  assert.throws(() => verifySwiftSurface(inventory, declarations, { ...graph(), module: { name: "Other" } }), /compiled LabColors/u);
  assert.throws(() => authoringSurface({}), /roots/u);
});


test("SymbolKit may omit empty parameters, but not an entire signature or required arguments", () => {
  const valid = graph();
  const finish = valid.symbols.find((symbol) => symbol.pathComponents.at(-1) === "finish()");
  delete finish.functionSignature.parameters;
  assert.equal(verifySwiftSurface(inventory, declarations, valid), 6);
  delete finish.functionSignature;
  assert.throws(() => verifySwiftSurface(inventory, declarations, valid), /no signature/u);
  const malformed = graph();
  delete malformed.symbols.find((symbol) => symbol.pathComponents[0] === "ProgramRuntime").functionSignature.parameters;
  assert.throws(() => verifySwiftSurface(inventory, declarations, malformed), /parameter list/u);
});

test("a static replacement cannot satisfy an instance operation", () => {
  const compiled = graph();
  compiled.symbols.find((symbol) => symbol.pathComponents[0] === "ProgramRuntime").kind.identifier = "swift.type.method";
  assert.throws(() => verifySwiftSurface(inventory, declarations, compiled), /updateObserved/u);
});


test("new publicly exported authoring versions join native coverage without a second version list", async () => {
  const { authoringExportPaths } = await import("./authoring-surface.mjs");
  const pkg = { exports: {
    "./program-wire/abi-v1.js": { types: "./program-wire/abi-v1.d.ts" },
    "./program-wire/abi-v2.js": { types: "./program-wire/abi-v2.d.ts" },
    "./program-wire/abi-v3.js": { types: "./program-wire/abi-v3.d.ts" },
    ".": { types: "./index.d.ts" },
  } };
  assert.deepEqual(authoringExportPaths(pkg), ["./program-wire/abi-v1.d.ts", "./program-wire/abi-v2.d.ts", "./program-wire/abi-v3.d.ts"]);
  const documents = { ...declarations, "v3.d.ts": "export class ProgramWireBuilderV3 { futureVersion(value: number): this; }" };
  assert.throws(() => verifySwiftSurface(inventory, documents, graph()), /futureVersion/u);
  for (const path of ["../foreign.d.ts", "./program-wire/../foreign.d.ts", "./program-wire/code.js", null]) {
    const malformed = structuredClone(pkg);
    malformed.exports["./program-wire/abi-v3.js"].types = path;
    assert.throws(() => authoringExportPaths(malformed), /canonical type/u);
  }
  assert.throws(() => authoringExportPaths({ exports: {} }), /exports are missing/u);
});
