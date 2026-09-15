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
} from "@labpics/colors";
import type {
  AttachedPointSinkHostIntentV1,
  AttachedPointSinkHostV1,
  AttachedProgramBindingsV1,
  ProgramErrorCode,
  ProgramOperation,
} from "@labpics/colors";
import {
  PROGRAM_WIRE_INVALID_DECLARATION,
  ProgramWireBuilderV1,
  ProgramWireError,
} from "@labpics/colors/program-wire/abi-v1.js";

async function boot(module: WebAssembly.Module, wire: Uint8Array): Promise<ProgramRuntime> {
  await init({ module_or_path: module });
  const runtime: ProgramRuntime = compileProgramWire(wire, 1);
  const snapshot: ProgramSnapshot = runtime.updateObserved(
    1n,
    new Uint32Array([1]),
    new Uint8Array([255, 255, 255]),
    1,
  );
  snapshot.state;
  snapshot.outputCount();
  if (snapshot.outputCount() > 0) {
    snapshot.outputSlot(0);
    snapshot.outputRgb(0);
    snapshot.outputOpacity(0);
  }
  return runtime;
}

const wire = new ProgramWireBuilderV1()
  .source(1, [0, 0, 0])
  .fixedTarget(2, 1)
  .finish();
const invalidDeclaration = new ProgramWireError(
  PROGRAM_WIRE_INVALID_DECLARATION,
  "invalid declaration",
);
const invalidCode: ProgramWireError["code"] = invalidDeclaration.code;
const readProgramFailure = (error: unknown): readonly [ProgramErrorCode, ProgramOperation] | undefined =>
  isProgramError(error) ? [error.code, error.operation] : undefined;

void boot;
void wire;
void invalidCode;
void readProgramFailure;
void evaluateWcag22("#000000", "#FFFFFF", "sc-1.4.3-text-default");
void numericalCapabilityManifest();

// Legacy recipe/browser roots are intentionally absent after atomic C7c.
// @ts-expect-error removed: RoleRecipe
import type { RoleRecipe } from "@labpics/colors";
// @ts-expect-error removed: ThemeConfig
import type { ThemeConfig } from "@labpics/colors";
// @ts-expect-error removed: LabColors recipe engine
import { LabColors } from "@labpics/colors";
// @ts-expect-error removed: applyTheme browser root
import { applyTheme } from "@labpics/colors";
void (null as unknown as RoleRecipe);
void (null as unknown as ThemeConfig);
void LabColors;
void applyTheme;

// Проверяется только компилятором: JsValue на Rust-границе не расширяет
// generated API до any/unknown и не делает обязательные аргументы optional.
function scalarInputTypes(
  bytes: Uint8Array,
  runtime: ProgramRuntime,
  snapshot: ProgramSnapshot,
): void {
  const scenarios = new Uint32Array([1]);
  const surfaces = new Uint8Array([255, 255, 255]);
  const revision: bigint = 1n;
  const count: number = 1;

  compileProgramWire(bytes, 1);
  runtime.updateObserved(revision, scenarios, surfaces, count);
  runtime.updateUnknown(revision, 1);
  snapshot.outputSlot(0);
  snapshot.outputRgb(0);
  snapshot.outputOpacity(0);

  // @ts-expect-error stream ID остаётся number, не bigint.
  compileProgramWire(bytes, 1n);
  // @ts-expect-error observed revision остаётся bigint, не number.
  runtime.updateObserved(1, scenarios, surfaces, count);
  // @ts-expect-error surface count остаётся number, не bigint.
  runtime.updateObserved(revision, scenarios, surfaces, 1n);
  // @ts-expect-error unknown revision остаётся bigint, не number.
  runtime.updateUnknown(1, 1);
  // @ts-expect-error reason ID остаётся number, не bigint.
  runtime.updateUnknown(revision, 1n);
  // @ts-expect-error slot index остаётся number, не bigint.
  snapshot.outputSlot(0n);
  // @ts-expect-error RGB index остаётся number, не bigint.
  snapshot.outputRgb(0n);
  // @ts-expect-error opacity index остаётся number, не bigint.
  snapshot.outputOpacity(0n);

  // @ts-expect-error stream ID обязателен.
  compileProgramWire(bytes);
  // @ts-expect-error surface count обязателен.
  runtime.updateObserved(revision, scenarios, surfaces);
  // @ts-expect-error reason ID обязателен.
  runtime.updateUnknown(revision);
  // @ts-expect-error slot index обязателен.
  snapshot.outputSlot();
  // @ts-expect-error RGB index обязателен.
  snapshot.outputRgb();
  // @ts-expect-error opacity index обязателен.
  snapshot.outputOpacity();
}

const attachedHost: AttachedPointSinkHostV1 = {
  tryInstall(intent: AttachedPointSinkHostIntentV1): void {
    void intent.kind;
  },
};

function attachedInputTypes(bytes: Uint8Array): AttachedMaterializationAuthority | undefined {
  const compiled: CompiledAttachedProgram = compileAttachedProgramWire(bytes);
  const bindings: AttachedProgramBindingsV1 = {
    emissionOutputs: new Uint32Array([91]),
    emissionSinkOutputs: new Uint32Array([3]),
    presentationOutputs: new Uint32Array([91]),
    presentationRoots: new Uint32Array([71]),
    presentationOccurrences: new Uint32Array([61]),
  };
  const runtime: AttachedProgramRuntime = compiled.attach(1, bindings, attachedHost);
  const scenarios = new Uint32Array([1]);
  const surfaces = new Uint8Array([255, 255, 255]);
  const revision: bigint = 1n;
  const update: AttachedProgramUpdate = runtime.updateObserved(revision, scenarios, surfaces);
  runtime.updateUnknown(2n, 1);
  const authority = update.authorityCount() > 0 ? update.takeAuthority(0) : undefined;
  if (authority !== undefined) {
    runtime.validateAuthority(authority);
    const publishedRevision: bigint = authority.publishedRevision;
    const sinkSequence: bigint = authority.sinkSequence;
    const sinkBindingEpoch: bigint = authority.sinkBindingEpoch;
    const identity: Uint8Array = authority.contentIdentity();
    const source: Uint8Array = authority.sourceRgb();
    void publishedRevision;
    void sinkSequence;
    void sinkBindingEpoch;
    void identity;
    void source;
  }

  // @ts-expect-error attached stream ID остаётся number, не bigint.
  compiled.attach(1n, bindings, attachedHost);
  // @ts-expect-error host обязателен.
  compiled.attach(1, bindings);
  // @ts-expect-error bindings используют Uint32Array, не number[].
  compiled.attach(1, { ...bindings, emissionOutputs: [91] }, attachedHost);
  // @ts-expect-error attached observed revision остаётся bigint, не number.
  runtime.updateObserved(1, scenarios, surfaces);
  // @ts-expect-error surfaces обязательны.
  runtime.updateObserved(revision, scenarios);
  // @ts-expect-error attached unknown reason остаётся number, не bigint.
  runtime.updateUnknown(2n, 1n);
  // @ts-expect-error authority index остаётся number, не bigint.
  update.takeAuthority(0n);
  // @ts-expect-error authority обязателен.
  runtime.validateAuthority();
  return authority;
}

void scalarInputTypes;
void attachedInputTypes;
void attachedHost;
