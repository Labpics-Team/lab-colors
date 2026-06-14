// Public types for @labpics/colors.
//
// Re-exports the wasm-bindgen-generated types (the rich `ResolvedTheme` /
// `RoleResult` union and the `LabColors` engine) and the vanilla `applyTheme`
// helper, so a consumer gets full typing from the package root.

export {
  default,
  default as init,
  initSync,
  LabColors,
} from "./pkg/labcolors.js";

export type {
  ThemeName,
  SolvedColor,
  NoneRole,
  UnreachableRole,
  RoleResult,
  ResolvedTheme,
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
