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
