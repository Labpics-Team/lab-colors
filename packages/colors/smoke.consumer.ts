// Type-level consumer smoke. Compiled with `tsc --noEmit` to prove the public
// types of @labpics/colors are usable from a strict TypeScript consumer. It is
// never executed — `tsc` checking it is the test.

import init, {
  LabColors,
  applyTheme,
  watchTheme,
  adaptTheme,
  effectiveBackground,
  oklabLerp,
} from "./index.js";
import type { ResolvedTheme, RoleResult, ThemeName } from "./index.js";

async function consume(clientConfigJson: string): Promise<void> {
  await init();
  const engine = new LabColors();

  // A client config must be loaded before resolving client-owned role names.
  const configFingerprint: string = engine.loadConfig(clientConfigJson);
  void configFingerprint;

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

  const bg: string = result.background;
  void bg;

  const glow: RoleResult = result.roles["fx-glow-brand"];
  if (glow.kind === "glow") {
    const diagnosticProfile: "cam16-ucs-jprime-li2017-v1" | null =
      glow.diagnosticProfile;
    void diagnosticProfile;
  }

  applyTheme(document.documentElement, result);

  // Current effectiveBackground returns the legacy solid reference estimate,
  // not evidence of the browser's actually rendered pixel.
  const effBg: string = effectiveBackground(document.documentElement);
  void effBg;

  // The interpolation helper is an explicit Oklab construction primitive.
  const blended: string = oklabLerp("#101012", effBg, 0.5);
  void blended;

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

  // Current adaptive API remains a legacy characterised controller. Its types
  // are smoke-tested here without promoting it to a universal safety proof.
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

  // A varying backdrop can be supplied as an explicit sample set.
  const adaptiveBackdrop = adaptTheme(surface, {
    colors: engine,
    theme,
    background: (): string[] => ["#101012", effectiveBackground(surface), "#202024"],
    strict: true,
  });
  adaptiveBackdrop.stop();
}

void consume;
