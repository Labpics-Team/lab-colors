// Public entry for @labpics/colors.
//
// Re-exports the wasm-bindgen surface (the default `init` loader, `initSync`,
// and the `LabColors` engine class) plus the vanilla `applyTheme` DOM helper.
// The wasm glue is the generated `pkg/` artifact (built by `npm run build`).

export { default, default as init, initSync, LabColors } from "./pkg/labcolors.js";

export { applyTheme } from "./apply-theme.js";
