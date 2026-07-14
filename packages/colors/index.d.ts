// Public types for @labpics/colors.
//
// Re-exports the wasm-bindgen-generated types (the rich `ResolvedTheme` /
// `RoleResult` union and the `LabColors` engine) and the vanilla `applyTheme`
// helper, so a consumer gets full typing from the package root.

import type {
  Wcag22AssessmentV1,
  Wcag22CriterionV1,
  Wcag22FeasibilityOutcomeV1,
} from "./pkg/labcolors.js";

export {
  default,
  default as init,
  initSync,
  LabColors,
  numericalCapabilityManifest,
} from "./pkg/labcolors.js";

/** Exact WCAG 2.2 assessment for one canonical final-sRGB8 occurrence. */
export declare function evaluateWcag22(
  foreground: string,
  background: string,
  criterion: Wcag22CriterionV1,
): Wcag22AssessmentV1;

/** Exact derived V1 request ceiling, available after WASM initialization. */
export declare function wcag22FeasibilityMaxBytes(): number;

/** Evaluate one strict V1 UTF-8 JSON byte envelope; protocol failures are data. */
export declare function evaluateWcag22Feasibility(
  request: Uint8Array,
): Wcag22FeasibilityOutcomeV1;

// Curated public schema/result surface. wasm-bindgen's InitOutput and raw
// __wbg_* ABI helpers remain implementation details.
export type {
  ThemeName,
  SolvedColor,
  NoneRole,
  UnreachableRole,
  TranslucentRole,
  GlowDecisionProfileV1,
  GlowLayerRecipeProfileV1,
  GlowDiagnosticProfileV1,
  GlowTargetStatusV1,
  NumericalIndeterminacyV1,
  GlowBitExactDecisionGuaranteeV1,
  GlowLegacyDecisionGuaranteeV1,
  GlowDecisionGuaranteeV1,
  GlowDeterminateRoleBase,
  GlowStableExactNoopRole,
  GlowLegacyReachedRole,
  GlowLegacyUnreachableRole,
  GlowDeterminateRole,
  GlowIndeterminateRoleBase,
  GlowIndeterminateRole,
  GlowRole,
  MaterialAlphaGuaranteeBaseV1,
  MaterialBisectionBracketGuaranteeV1,
  MaterialTransparentEndpointGuaranteeV1,
  MaterialOpaqueEndpointGuaranteeV1,
  MaterialAlphaGuaranteeV1,
  MaterialRoleBase,
  MaterialSatisfiedTransparentRole,
  MaterialSatisfiedBracketRole,
  MaterialDegradedOpaqueRole,
  MaterialRole,
  RoleResult,
  ThemeAnchors,
  LadderSource,
  LadderPositionV1,
  RoleRecipe,
  ThemeConfig,
  ResolvedTheme,
  NumericalCapabilitySiteV2,
  NumericalCapabilityManifestV2,
  Wcag22CriterionV1,
  Wcag22DecisionV1,
  Wcag22Q55BoundsV1,
  Wcag22AssessmentV1,
  Wcag22FeasibilityRequestV1,
  Wcag22FeasibilityOutcomeV1,
} from "./pkg/labcolors.js";

export { applyTheme } from "./apply-theme.js";
export { watchTheme } from "./watch-theme.js";
export type { WatchThemeOptions, WatchController } from "./watch-theme.js";
export { adaptTheme } from "./adapt-theme.js";
export type { AdaptThemeOptions, AdaptController } from "./adapt-theme.js";
export {
  effectiveBackground,
  parseCssColor,
  compositeOver,
  compositeStackToHex,
  toHex,
  oklabLerp,
} from "./effective-bg.js";
export type { Rgba, EffectiveBackgroundOptions, StyleLike } from "./effective-bg.js";
