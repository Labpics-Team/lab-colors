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
import type {
  FailureCategory,
  FailureRole,
  GlowDiagnosticProfileV1,
  GlowDeterminateRole,
  GlowDeterminateRoleBase,
  GlowLayerRecipeProfileV1,
  GlowTargetStatusV1,
  MaterialRole,
  MaterialRoleBase,
  ResolvedTheme,
  RoleResult,
  ThemeName,
} from "./index.js";

const requireFailure = (_role: FailureRole): void => {};
requireFailure({
  kind: "failure",
  cssVar: "--lab-example",
  category: "unresolved",
  code: "bounded_search_exhausted",
  message: "bounded search exhausted",
});
requireFailure({
  kind: "failure",
  cssVar: "--lab-example",
  // @ts-expect-error failure categories are a closed core-owned vocabulary.
  category: "internal",
  code: "x",
  message: "x",
});

declare const glowDeterminateCommon: GlowDeterminateRoleBase;
const requireGlowDeterminate = (_role: GlowDeterminateRole): void => {};

requireGlowDeterminate({
  ...glowDeterminateCommon,
  decisionProfile: "stable-v1",
  decisionGuarantee: { kind: "bit-exact" },
  selectionDiagnosticProfile: null,
  targetStatus: "exact-noop-unreachable",
  degraded: true,
});
requireGlowDeterminate({
  ...glowDeterminateCommon,
  decisionProfile: "legacy-platform-dependent-v1",
  decisionGuarantee: { kind: "legacy-platform-dependent-v1" },
  selectionDiagnosticProfile: "cam16-ucs-jprime-li2017-v1",
  targetStatus: "legacy-reached",
  degraded: false,
});
requireGlowDeterminate({
  ...glowDeterminateCommon,
  decisionProfile: "legacy-platform-dependent-v1",
  decisionGuarantee: { kind: "legacy-platform-dependent-v1" },
  selectionDiagnosticProfile: "cam16-ucs-jprime-li2017-v1",
  targetStatus: "legacy-unreachable",
  degraded: true,
});

// @ts-expect-error stable-профиль не может нести legacy-status.
requireGlowDeterminate({ ...glowDeterminateCommon, decisionProfile: "stable-v1", decisionGuarantee: { kind: "bit-exact" }, selectionDiagnosticProfile: null, targetStatus: "legacy-reached", degraded: false });
// @ts-expect-error legacy-профиль не может рекламировать bit-exact decision.
requireGlowDeterminate({ ...glowDeterminateCommon, decisionProfile: "legacy-platform-dependent-v1", decisionGuarantee: { kind: "bit-exact" }, selectionDiagnosticProfile: "cam16-ucs-jprime-li2017-v1", targetStatus: "legacy-reached", degraded: false });
// @ts-expect-error exact no-op не выполняет CAM16 selection diagnostic.
requireGlowDeterminate({ ...glowDeterminateCommon, decisionProfile: "stable-v1", decisionGuarantee: { kind: "bit-exact" }, selectionDiagnosticProfile: "cam16-ucs-jprime-li2017-v1", targetStatus: "exact-noop-unreachable", degraded: true });
// @ts-expect-error reached-ветвь не является degraded.
requireGlowDeterminate({ ...glowDeterminateCommon, decisionProfile: "legacy-platform-dependent-v1", decisionGuarantee: { kind: "legacy-platform-dependent-v1" }, selectionDiagnosticProfile: "cam16-ucs-jprime-li2017-v1", targetStatus: "legacy-reached", degraded: true });
// @ts-expect-error Glow API не поддерживает outward decision guarantee.
requireGlowDeterminate({ ...glowDeterminateCommon, decisionProfile: "stable-v1", decisionGuarantee: { kind: "outward-interval-v1", lower: 0, upper: 1 }, selectionDiagnosticProfile: null, targetStatus: "exact-noop-unreachable", degraded: true });

function narrowGlowDeterminate(role: GlowDeterminateRole): void {
  if (role.decisionProfile === "stable-v1") {
    const status: "exact-noop-unreachable" = role.targetStatus;
    const guarantee: "bit-exact" = role.decisionGuarantee.kind;
    const diagnostic: null = role.selectionDiagnosticProfile;
    const degraded: true = role.degraded;
    void status;
    void guarantee;
    void diagnostic;
    void degraded;
  } else if (role.targetStatus === "legacy-reached") {
    const degraded: false = role.degraded;
    void degraded;
  } else {
    const status: "legacy-unreachable" = role.targetStatus;
    const degraded: true = role.degraded;
    void status;
    void degraded;
  }
}

declare const materialCommon: MaterialRoleBase;
const requireMaterial = (_role: MaterialRole): void => {};

requireMaterial({
  ...materialCommon,
  alpha: 0,
  alphaGuarantee: {
    kind: "transparent-endpoint-characterized-v1",
    numericalProfile: "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1",
  },
  alphaStatus: "satisfied",
  guaranteed: true,
});
requireMaterial({
  ...materialCommon,
  alpha: 0.5,
  alphaGuarantee: {
    kind: "bisection-bracket-characterized-v1",
    iterations: 60,
    lowerAlpha: 0.49,
    upperAlpha: 0.5,
    numericalProfile: "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1",
  },
  alphaStatus: "satisfied",
  guaranteed: true,
});
requireMaterial({
  ...materialCommon,
  alpha: 1,
  alphaGuarantee: {
    kind: "opaque-endpoint-characterized-v1",
    numericalProfile: "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1",
  },
  alphaStatus: "degraded",
  guaranteed: false,
});

// @ts-expect-error degraded несовместим с transparent endpoint.
requireMaterial({ ...materialCommon, alpha: 0, alphaGuarantee: { kind: "transparent-endpoint-characterized-v1", numericalProfile: "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1" }, alphaStatus: "degraded", guaranteed: false });
// @ts-expect-error satisfied несовместим с opaque endpoint.
requireMaterial({ ...materialCommon, alpha: 1, alphaGuarantee: { kind: "opaque-endpoint-characterized-v1", numericalProfile: "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1" }, alphaStatus: "satisfied", guaranteed: true });
// @ts-expect-error compatibility boolean выводится из typed status.
requireMaterial({ ...materialCommon, alpha: 0.5, alphaGuarantee: { kind: "bisection-bracket-characterized-v1", iterations: 60, lowerAlpha: 0.49, upperAlpha: 0.5, numericalProfile: "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1" }, alphaStatus: "satisfied", guaranteed: false });
// @ts-expect-error transparent endpoint конструктивно имеет alpha 0.
requireMaterial({ ...materialCommon, alpha: 0.25, alphaGuarantee: { kind: "transparent-endpoint-characterized-v1", numericalProfile: "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1" }, alphaStatus: "satisfied", guaranteed: true });
// @ts-expect-error degraded opaque endpoint конструктивно имеет alpha 1.
requireMaterial({ ...materialCommon, alpha: 0.9, alphaGuarantee: { kind: "opaque-endpoint-characterized-v1", numericalProfile: "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1" }, alphaStatus: "degraded", guaranteed: false });

function narrowMaterial(role: MaterialRole): void {
  if (role.alphaStatus === "degraded") {
    const alpha: 1 = role.alpha;
    const guarantee: "opaque-endpoint-characterized-v1" = role.alphaGuarantee.kind;
    const guaranteed: false = role.guaranteed;
    void alpha;
    void guarantee;
    void guaranteed;
    return;
  }

  const status: "satisfied" = role.alphaStatus;
  const guaranteed: true = role.guaranteed;
  void status;
  void guaranteed;
  if (role.alphaGuarantee.kind === "transparent-endpoint-characterized-v1") {
    const guarantee: "transparent-endpoint-characterized-v1" = role.alphaGuarantee.kind;
    void guarantee;
  } else {
    const guarantee: "bisection-bracket-characterized-v1" = role.alphaGuarantee.kind;
    const upper: number = role.alphaGuarantee.upperAlpha;
    void guarantee;
    void upper;
  }
}

void narrowGlowDeterminate;
void narrowMaterial;

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
  } else if (primary.kind === "failure") {
    const category: FailureCategory = primary.category;
    const code: string = primary.code;
    void category;
    void code;
  } else {
    const cssVar: string = primary.cssVar;
    void cssVar;
  }

  const bg: string = result.background;
  void bg;

  const glow: RoleResult = result.roles["fx-glow-brand"];
  if (glow.kind === "glow") {
    const layerRecipeProfile: GlowLayerRecipeProfileV1 = glow.layerRecipeProfile;
    const appearanceDiagnosticProfile: GlowDiagnosticProfileV1 =
      glow.appearanceDiagnosticProfile;
    const selectionDiagnosticProfile: GlowDiagnosticProfileV1 | null =
      glow.selectionDiagnosticProfile;
    const targetStatus: GlowTargetStatusV1 = glow.targetStatus;
    void layerRecipeProfile;
    void appearanceDiagnosticProfile;
    void selectionDiagnosticProfile;
    void targetStatus;
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
