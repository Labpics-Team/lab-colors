// Type-level consumer smoke. Compiled with `tsc --noEmit` to prove the public
// types of @labpics/colors are usable from a strict TypeScript consumer. It is
// never executed — `tsc` checking it is the test.

import init, {
  LabColors,
  applyTheme,
  watchTheme,
  adaptTheme,
  effectiveBackground,
} from "./index.js";
import type { ResolvedTheme, RoleResult, ThemeName } from "./index.js";

async function consume(): Promise<void> {
  await init();
  const engine = new LabColors();

  const theme: ThemeName = "light";
  const result: ResolvedTheme = engine.resolveTheme("#FFFFFF", theme);

  // The discriminated union narrows on `kind`.
  const primary: RoleResult = result.roles["label-primary"];
  if (primary.kind === "color") {
    const hex: string = primary.hex;
    const lc: number = primary.lc;
    const wcag: number = primary.wcagRatio;
    const legalFloor: number | null = primary.legalFloor;
    void hex;
    void lc;
    void wcag;
    void legalFloor;
  } else if (primary.kind === "unreachable") {
    const code: string = primary.code;
    void code;
  } else {
    // kind === "none"
    const cssVar: string = primary.cssVar;
    void cssVar;
  }

  // vars is a string→string map of reachable roles.
  const bg: string = result.background;
  void bg;

  // The vanilla helper accepts the result directly.
  applyTheme(document.documentElement, result);

  // The effective-background resolver returns a solid hex.
  const effBg: string = effectiveBackground(document.documentElement);
  void effBg;

  // The reactive runtime keeps an element in sync; the controller is typed.
  const surface = document.querySelector(".surface") as HTMLElement;
  const controller = watchTheme(surface, {
    colors: engine,
    theme,
    background: () => effectiveBackground(surface, { fallback: "#101012" }),
  });
  const applied: ResolvedTheme | null = controller.refresh();
  void applied;
  controller.setTheme("dark");
  const bgHex: string = controller.background();
  void bgHex;
  controller.stop();

  // The adaptive (hysteresis) controller: lazy re-check + eased re-solve.
  const adaptive = adaptTheme(surface, {
    colors: engine,
    theme,
    background: () => effectiveBackground(surface, { fallback: "#101012" }),
    easeMs: 280,
    dropFraction: 0.2,
    strict: true,
  });
  adaptive.start();
  adaptive.tick();
  adaptive.setTheme("dark");
  const appliedVars: Record<string, string> = adaptive.current();
  void appliedVars;
  adaptive.stop();
}

void consume;
