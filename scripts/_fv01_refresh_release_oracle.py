#!/usr/bin/env python3
from pathlib import Path

path = Path("scripts/verify-package-release.mjs")
text = path.read_text(encoding="utf-8")

old_exports = '''assert.deepEqual(Object.keys(colors).sort(), [
  "ProgramRuntime",
  "ProgramSnapshot",
  "compileProgramWire",
  "default",
  "evaluateWcag22",
  "init",
  "initSync",
  "isProgramError",
  "numericalCapabilityManifest",
]);'''
new_exports = '''assert.deepEqual(Object.keys(colors).sort(), [
  "AttachedMaterializationAuthority",
  "AttachedProgramRuntime",
  "AttachedProgramUpdate",
  "CompiledAttachedProgram",
  "ProgramRuntime",
  "ProgramSnapshot",
  "compileAttachedProgramWire",
  "compileProgramWire",
  "default",
  "evaluateWcag22",
  "init",
  "initSync",
  "isProgramError",
  "numericalCapabilityManifest",
]);'''
if text.count(old_exports) != 1:
    raise SystemExit("release export oracle anchor changed")
text = text.replace(old_exports, new_exports, 1)

init_anchor = '''await colors.init({ module_or_path: wasm });

const capability = colors.numericalCapabilityManifest();'''
init_replacement = '''await colors.init({ module_or_path: wasm });
assert.throws(
  () => colors.compileAttachedProgramWire(new Uint8Array()),
  (error) => colors.isProgramError(error)
    && error.code === "attached_program_wire"
    && error.operation === "compileAttachedProgramWire",
);

const capability = colors.numericalCapabilityManifest();'''
if text.count(init_anchor) != 1:
    raise SystemExit("runtime attached smoke anchor changed")
text = text.replace(init_anchor, init_replacement, 1)

start = text.find("export function typeSmokeSource() {")
end_marker = "\n}\n\n// Execute the same packed-package runtime smoke"
end = text.find(end_marker, start)
if start < 0 or end < 0:
    raise SystemExit("type smoke function anchors changed")

replacement = r'''export function typeSmokeSource() {
  return String.raw`
import init, {
  AttachedMaterializationAuthority,
  AttachedProgramRuntime,
  AttachedProgramUpdate,
  CompiledAttachedProgram,
  ProgramRuntime,
  ProgramSnapshot,
  compileAttachedProgramWire,
  compileProgramWire,
  evaluateWcag22,
  isProgramError,
  numericalCapabilityManifest,
  type AttachedPointSinkHostIntentV1,
  type AttachedPointSinkHostV1,
  type AttachedProgramBindingsV1,
  type NumericalCapabilityManifestV2,
  type ProgramErrorCode,
  type ProgramOperation,
  type Wcag22AssessmentV1,
  type Wcag22CriterionV1,
} from "@labpics/colors";

async function boot(module: WebAssembly.Module, wire: Uint8Array): Promise<ProgramRuntime> {
  await init({ module_or_path: module });
  const runtime = compileProgramWire(wire, 1);
  const snapshot: ProgramSnapshot = runtime.updateObserved(
    1n,
    new Uint32Array([1]),
    new Uint8Array([255, 255, 255]),
    1,
  );
  snapshot.state;
  snapshot.outputCount();
  return runtime;
}

function attach(
  wire: Uint8Array,
  host: AttachedPointSinkHostV1,
): readonly [
  CompiledAttachedProgram,
  AttachedProgramRuntime,
  AttachedProgramUpdate,
  AttachedMaterializationAuthority | undefined,
] {
  const compiled: CompiledAttachedProgram = compileAttachedProgramWire(wire);
  const bindings: AttachedProgramBindingsV1 = {
    emissionOutputs: new Uint32Array([91]),
    emissionSinkOutputs: new Uint32Array([3]),
    presentationOutputs: new Uint32Array([91]),
    presentationRoots: new Uint32Array([71]),
    presentationOccurrences: new Uint32Array([61]),
  };
  const runtime: AttachedProgramRuntime = compiled.attach(1, bindings, host);
  const update: AttachedProgramUpdate = runtime.updateObserved(
    1n,
    new Uint32Array([1]),
    new Uint8Array([255, 255, 255]),
  );
  const authority = update.authorityCount() > 0 ? update.takeAuthority(0) : undefined;
  if (authority !== undefined) {
    runtime.validateAuthority(authority);
    const revision: bigint = authority.publishedRevision;
    const sequence: bigint = authority.sinkSequence;
    const epoch: bigint = authority.sinkBindingEpoch;
    const identity: Uint8Array = authority.contentIdentity();
    const source: Uint8Array = authority.sourceRgb();
    void revision;
    void sequence;
    void epoch;
    void identity;
    void source;
  }
  return [compiled, runtime, update, authority];
}

const host: AttachedPointSinkHostV1 = {
  tryInstall(intent: AttachedPointSinkHostIntentV1): void {
    void intent.kind;
  },
};
const criterion: Wcag22CriterionV1 = "sc-1.4.3-text-default";
const assessment: Wcag22AssessmentV1 = evaluateWcag22("#000000", "#FFFFFF", criterion);
const capability: NumericalCapabilityManifestV2 = numericalCapabilityManifest();
const programFailure = (error: unknown): readonly [ProgramErrorCode, ProgramOperation] | undefined =>
  isProgramError(error) ? [error.code, error.operation] : undefined;
void boot;
void attach;
void host;
void programFailure;
void assessment;
void capability;
// @ts-expect-error C7c removed the recipe engine.
import { LabColors } from "@labpics/colors";
// @ts-expect-error C7c removed recipe DTOs.
import type { RoleRecipe } from "@labpics/colors";
void LabColors;
void (null as unknown as RoleRecipe);
`;
}'''

text = text[:start] + replacement + text[end + 2:]
path.write_text(text, encoding="utf-8")
