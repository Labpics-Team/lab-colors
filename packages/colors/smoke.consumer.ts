// Type-level consumer smoke. Compiled with `tsc --noEmit` to prove the public
// types of @labpics/colors are usable from a strict TypeScript consumer. It is
// never executed — `tsc` checking it is the test.

import init, { LabColors, applyTheme } from "./index.js";
import type { ResolvedTheme, RoleResult, ThemeName } from "./index.js";

async function consume(): Promise<void> {
  await init();
  const engine = new LabColors();

  const theme: ThemeName = "light";
  const result: ResolvedTheme = engine.resolveTheme("#FFFFFF", theme);

  // The discriminated union narrows on `kind`.
  const primary: RoleResult = result.roles["text-primary"];
  if (primary.kind === "color") {
    const hex: string = primary.hex;
    const lc: number = primary.lc;
    const wcag: number = primary.wcagRatio;
    void hex;
    void lc;
    void wcag;
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
}

void consume;
