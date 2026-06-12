# @labpics/colors

Framework-agnostic contrast engine. Given a background colour and a theme, it
resolves a full set of perceptually-anchored, WCAG-aware colour roles
(`text-primary`, `icon`, `border`, …) as `#RRGGBB` values plus their measured
contrasts. The engine is the `labcolors-core` Rust crate compiled to WebAssembly;
the package has zero runtime dependencies.

It returns **data**, never DOM side effects. Writing the result to the page is
the caller's choice — a vanilla helper is provided.

## Install

Built from the monorepo with `wasm-pack`:

```sh
npm run build   # → pkg/ (wasm + JS glue + .d.ts)
```

## Usage

```ts
import init, { LabColors, applyTheme } from "@labpics/colors";

await init();                       // load the wasm module (once)
const engine = new LabColors();     // zero-config: default role table

const result = engine.resolveTheme("#FFFFFF", "light");
// result.vars  → { "--lab-text-primary": "#1a1a1a", "--lab-icon": "#5b5b5b", ... }
// result.roles → per-role detail (hex, lc, wcagRatio, compressed, floorOverride)

applyTheme(document.documentElement, result);   // sets every --lab-* property
```

## API

### `new LabColors()`

Constructs a caching engine on the default role table and the default per-theme
viewing conditions. Identical `resolveTheme` calls are served from an internal
contract cache.

### `engine.resolveTheme(bgHex, theme): ResolvedTheme`

- `bgHex` — background as `#RGB` or `#RRGGBB`.
- `theme` — `"light" | "dark" | "light-ic" | "dark-ic"`.

Returns:

```ts
interface ResolvedTheme {
  theme: ThemeName;
  background: string;                       // normalised #RRGGBB
  vars: Record<string, string>;             // reachable roles: "--lab-<key>" → hex
  roles: Record<string, RoleResult>;        // every role, keyed by role key
}

type RoleResult =
  | { kind: "color"; cssVar: string; hex: string; lc: number;
      wcagRatio: number; compressed: boolean; floorOverride: boolean }
  | { kind: "none"; cssVar: string }                       // the explicit zero token
  | { kind: "unreachable"; cssVar: string; code: string; message: string };
```

Per-role unreachability is part of a **successful** result — each role carries
its own `kind`. Only whole-call failures (invalid hex, unknown or uncalibrated
theme) reject, as an `Error` whose message is `"<code>: <reason>"`.

The `-ic` (increased-contrast) themes are reserved in the type but **not yet
calibrated**: requesting one rejects with code `theme_not_calibrated`.

Whole-call rejections carry one of these stable codes (in `Error.message`, as
`"<code>: <reason>"`):

| Code | Cause |
|------|-------|
| `invalid_background` | `bgHex` is not `#RGB` or `#RRGGBB`. |
| `unknown_theme` | `theme` is not one of the four accepted names. |
| `theme_not_calibrated` | An `-ic` theme was requested before calibration. |

### `applyTheme(element, result): void`

Writes every reachable role from `result.vars` onto `element.style` via
`setProperty`. Roles absent from `vars` (unreachable or the zero token) are not
written, leaving the caller's CSS fallbacks in effect — including across
theme re-application: stale `--lab-*` inline properties from a previous call
are cleared first, so a role that lost reachability does not linger.

## Bundle size

| Artifact | gzip | brotli |
|----------|------|--------|
| `labcolors_bg.wasm` | ~54 KB | ~47 KB |
| `labcolors.js` (glue) | ~3 KB | ~3 KB |

Targets modern browsers (2023+). Built with `panic = "abort"` and `wasm-opt -Oz`;
there are no panic pages in the release bundle — errors are returned as values.
