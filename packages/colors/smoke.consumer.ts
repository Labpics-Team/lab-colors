// Type-level consumer smoke. Compiled with `tsc --noEmit` to prove the public
// types of @labpics/colors are usable from a strict TypeScript consumer. It is
// never executed — `tsc` checking it is the test.

import init, {
  LabColors,
  applyTheme,
  watchTheme,
  adaptTheme,
} from "./index.js";
import type {
  FailureCategory,
  FailureRole,
  OutputConflict,
  OutputConflictError,
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

declare const noAliasGlow: GlowDeterminateRole;
// @ts-expect-error ambiguous Glow measurement alias was removed before first client.
noAliasGlow.achievedDj;
// @ts-expect-error boolean duplicates targetStatus and was removed.
noAliasGlow.degraded;
declare const noAliasMaterial: MaterialRole;
// @ts-expect-error boolean duplicates alphaStatus and was removed.
noAliasMaterial.guaranteed;

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
  // @ts-expect-error Unreachable rejects the whole snapshot as OutputConflictError.
  category: "unreachable",
  code: "exceeds_range",
  message: "physical output range exhausted",
});
requireFailure({
  kind: "failure",
  cssVar: "--lab-example",
  // @ts-expect-error failure categories are a closed core-owned vocabulary.
  category: "internal",
  code: "x",
  message: "x",
});
requireFailure({
  kind: "failure",
  cssVar: "--lab-example",
  // @ts-expect-error rejected closes the whole resolve and cannot be role data.
  category: "rejected",
  code: "invalid_input",
  message: "x",
});
requireFailure({
  kind: "failure",
  cssVar: "--lab-example",
  // @ts-expect-error unsupported closes the whole resolve and cannot be role data.
  category: "unsupported",
  code: "gamut_unsupported",
  message: "x",
});

const admittedFailureCategory: FailureCategory = "unresolved";
void admittedFailureCategory;

declare const outputConflict: OutputConflictError;
const outputConflictName: "OutputConflictError" = outputConflict.name;
const outputConflictCode: "output_conflict" = outputConflict.code;
const firstOutputConflict: OutputConflict = outputConflict.conflicts[0];
const opaqueRole: string = firstOutputConflict.role;
void outputConflictName;
void outputConflictCode;
void opaqueRole;

declare const glowDeterminateCommon: GlowDeterminateRoleBase;
const requireGlowDeterminate = (_role: GlowDeterminateRole): void => {};

requireGlowDeterminate({
  ...glowDeterminateCommon,
  decisionProfile: "stable-v1",
  decisionGuarantee: { kind: "bit-exact" },
  selectionDiagnosticProfile: null,
  targetStatus: "exact-noop-unreachable",
});
requireGlowDeterminate({
  ...glowDeterminateCommon,
  decisionProfile: "legacy-platform-dependent-v1",
  decisionGuarantee: { kind: "legacy-platform-dependent-v1" },
  selectionDiagnosticProfile: "cam16-ucs-jprime-li2017-v1",
  targetStatus: "legacy-reached",
});
requireGlowDeterminate({
  ...glowDeterminateCommon,
  decisionProfile: "legacy-platform-dependent-v1",
  decisionGuarantee: { kind: "legacy-platform-dependent-v1" },
  selectionDiagnosticProfile: "cam16-ucs-jprime-li2017-v1",
  targetStatus: "legacy-unreachable",
});

// @ts-expect-error stable-профиль не может нести legacy-status.
requireGlowDeterminate({ ...glowDeterminateCommon, decisionProfile: "stable-v1", decisionGuarantee: { kind: "bit-exact" }, selectionDiagnosticProfile: null, targetStatus: "legacy-reached" });
// @ts-expect-error legacy-профиль не может рекламировать bit-exact decision.
requireGlowDeterminate({ ...glowDeterminateCommon, decisionProfile: "legacy-platform-dependent-v1", decisionGuarantee: { kind: "bit-exact" }, selectionDiagnosticProfile: "cam16-ucs-jprime-li2017-v1", targetStatus: "legacy-reached" });
// @ts-expect-error exact no-op не выполняет CAM16 selection diagnostic.
requireGlowDeterminate({ ...glowDeterminateCommon, decisionProfile: "stable-v1", decisionGuarantee: { kind: "bit-exact" }, selectionDiagnosticProfile: "cam16-ucs-jprime-li2017-v1", targetStatus: "exact-noop-unreachable" });
// @ts-expect-error legacy-профиль не может нести exact stable status.
requireGlowDeterminate({ ...glowDeterminateCommon, decisionProfile: "legacy-platform-dependent-v1", decisionGuarantee: { kind: "legacy-platform-dependent-v1" }, selectionDiagnosticProfile: "cam16-ucs-jprime-li2017-v1", targetStatus: "exact-noop-unreachable" });
// @ts-expect-error Glow API не поддерживает outward decision guarantee.
requireGlowDeterminate({ ...glowDeterminateCommon, decisionProfile: "stable-v1", decisionGuarantee: { kind: "outward-interval-v1", lower: 0, upper: 1 }, selectionDiagnosticProfile: null, targetStatus: "exact-noop-unreachable" });

function narrowGlowDeterminate(role: GlowDeterminateRole): void {
  if (role.decisionProfile === "stable-v1") {
    const status: "exact-noop-unreachable" = role.targetStatus;
    const guarantee: "bit-exact" = role.decisionGuarantee.kind;
    const diagnostic: null = role.selectionDiagnosticProfile;
    void status;
    void guarantee;
    void diagnostic;
  } else if (role.targetStatus === "legacy-reached") {
    const status: "legacy-reached" = role.targetStatus;
    void status;
  } else {
    const status: "legacy-unreachable" = role.targetStatus;
    void status;
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
});
requireMaterial({
  ...materialCommon,
  alpha: 1,
  alphaGuarantee: {
    kind: "opaque-endpoint-characterized-v1",
    numericalProfile: "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1",
  },
  alphaStatus: "degraded",
});

// @ts-expect-error degraded несовместим с transparent endpoint.
requireMaterial({ ...materialCommon, alpha: 0, alphaGuarantee: { kind: "transparent-endpoint-characterized-v1", numericalProfile: "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1" }, alphaStatus: "degraded" });
// @ts-expect-error satisfied несовместим с opaque endpoint.
requireMaterial({ ...materialCommon, alpha: 1, alphaGuarantee: { kind: "opaque-endpoint-characterized-v1", numericalProfile: "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1" }, alphaStatus: "satisfied" });
// @ts-expect-error transparent endpoint конструктивно имеет alpha 0.
requireMaterial({ ...materialCommon, alpha: 0.25, alphaGuarantee: { kind: "transparent-endpoint-characterized-v1", numericalProfile: "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1" }, alphaStatus: "satisfied" });
// @ts-expect-error degraded opaque endpoint конструктивно имеет alpha 1.
requireMaterial({ ...materialCommon, alpha: 0.9, alphaGuarantee: { kind: "opaque-endpoint-characterized-v1", numericalProfile: "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1" }, alphaStatus: "degraded" });

function narrowMaterial(role: MaterialRole): void {
  if (role.alphaStatus === "degraded") {
    const alpha: 1 = role.alpha;
    const guarantee: "opaque-endpoint-characterized-v1" = role.alphaGuarantee.kind;
    void alpha;
    void guarantee;
    return;
  }

  const status: "satisfied" = role.alphaStatus;
  void status;
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
  // @ts-expect-error resolver snapshots are immutable values.
  result.vars["--lab-injected"] = "#000000";
  // @ts-expect-error role dictionaries are immutable values.
  result.roles.injected = { kind: "none", cssVar: "--lab-injected" };

  // The discriminated union narrows on `kind`.
  const primary: RoleResult = result.roles["label-primary"];
  if (primary.kind === "color") {
    const hex: string = primary.hex;
    const lc: number = primary.lc;
    const wcag: number = primary.wcagRatio;
    const legalFloor: number | null = primary.legalFloor;
    // @ts-expect-error exact bytes do not expose a hue-visibility verdict.
    primary.hueVanished;
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

  // The reactive runtime keeps an element in sync; the controller is typed.
  const surface = document.querySelector(".surface") as HTMLElement;
  const controller = watchTheme(surface, {
    colors: engine,
    theme,
    background: "#101012",
    onError(error: unknown) {
      void error;
    },
  });
  const applied: ResolvedTheme = controller.refresh();
  // @ts-expect-error the controller returns its immutable admitted snapshot.
  applied.vars["--lab-injected"] = "#000000";
  void applied;
  controller.setTheme("dark");
  const bgHex: string = controller.background();
  void bgHex;
  controller.stop();

  watchTheme(surface, {
    colors: engine,
    theme,
    // @ts-expect-error asynchronous observer errors require a callback.
    onError: "invalid",
  });

  // Current adaptive API remains a legacy characterised controller. Its types
  // are smoke-tested here without promoting it to a universal safety proof.
  const adaptive = adaptTheme(surface, {
    colors: engine,
    theme,
    background: "#101012",
    easeMs: 280,
    dropFraction: 0.2,
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
    background: (): string[] => ["#101012", "#202024"],
  });
  adaptiveBackdrop.stop();
}

void consume;
