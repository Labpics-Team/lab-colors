// Public entry for @labpics/colors.
//
// Re-exports the wasm-bindgen surface (the default `init` loader, `initSync`,
// and the `LabColors` engine class) plus the vanilla DOM runtime helpers:
// `applyTheme` (one-shot apply), `watchTheme` (reactive sync), and the
// effective-background resolver. The wasm glue is the generated `pkg/` artifact
// (built by `npm run build`).

export { default, default as init, initSync, LabColors } from "./pkg/labcolors.js";

export { applyTheme } from "./apply-theme.js";
export { watchTheme } from "./watch-theme.js";
export { adaptTheme } from "./adapt-theme.js";
export {
  effectiveBackground,
  parseCssColor,
  compositeOver,
  compositeStackToHex,
  toHex,
  oklabLerp,
} from "./effective-bg.js";
