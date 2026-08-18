import init, {
  ProgramRuntime,
  ProgramSnapshot,
  compileProgramWire,
  evaluateWcag22,
  numericalCapabilityManifest,
} from "./index.js";

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

void boot;
void evaluateWcag22("#000000", "#FFFFFF", "sc-1.4.3-text-default");
void numericalCapabilityManifest();

// Legacy recipe/browser roots are intentionally absent after atomic C7c.
// @ts-expect-error removed: RoleRecipe
import type { RoleRecipe } from "./index.js";
// @ts-expect-error removed: ThemeConfig
import type { ThemeConfig } from "./index.js";
// @ts-expect-error removed: LabColors recipe engine
import { LabColors } from "./index.js";
// @ts-expect-error removed: applyTheme browser root
import { applyTheme } from "./index.js";
void (null as unknown as RoleRecipe);
void (null as unknown as ThemeConfig);
void LabColors;
void applyTheme;
