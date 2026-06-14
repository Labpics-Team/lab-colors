# @labpics/colors

Framework-agnostic contrast engine. Given a background colour and a theme, it
resolves a full set of perceptually-anchored, WCAG-aware colour roles
(`label-primary`, `icon`, `border-base`, `fill-primary`, …) as `#RRGGBB` values
plus their measured contrasts. The engine is the `labcolors-core` Rust crate
compiled to WebAssembly; the package has zero runtime dependencies.

The WASM core returns **data**, never DOM side effects. Three vanilla helpers turn
that data into live CSS custom properties: `applyTheme` (one-shot), `watchTheme`
(reactive — re-resolves whenever the background changes), and `adaptTheme` (the
calm runtime for *continuously* moving backgrounds — it re-checks cheaply each
frame and only re-solves, then smoothly eases, when contrast actually degrades).

## Install

Built from the monorepo with `wasm-pack`:

```sh
npm run build   # → pkg/ (wasm + JS glue + .d.ts)
```

## Usage

### One-shot

```ts
import init, { LabColors, applyTheme } from "@labpics/colors";

await init();                       // load the wasm module (once)
const engine = new LabColors();     // zero-config: default role table

const result = engine.resolveTheme("#FFFFFF", "light");
// result.vars  → { "--lab-label-primary": "#1a1a1a", "--lab-icon": "#5b5b5b", ... }
// result.roles → per-role detail (hex, lc, wcagRatio, compressed, floorOverride, legalFloor)

applyTheme(document.documentElement, result);   // sets every --lab-* property
```

### Reactive — keep colours in sync with the background

`watchTheme` closes the loop a design system needs: point it at a surface and its
`--lab-*` variables follow the surface's (effective) background automatically.

```ts
import init, { LabColors, watchTheme } from "@labpics/colors";

await init();
const colors = new LabColors();

const panel = document.querySelector(".panel") as HTMLElement;
const watcher = watchTheme(panel, { colors, theme: "light" });
// `panel` now carries the right --lab-* for its background, and re-resolves
// whenever that background changes.

watcher.setTheme("dark");   // switch theme, re-applies
watcher.refresh();          // force a re-check (e.g. after a manual mutation)
watcher.stop();             // disconnect and stop watching
```

It serves both change regimes:

- **Discrete** changes (a theme/class/style toggle, a DOM reflow) are caught
  automatically by a `MutationObserver` — set it once and forget.
- **Continuous** changes (a CSS-animated or per-frame-scripted background that
  never mutates inline style) are driven by calling `refresh()` from your own
  `requestAnimationFrame` loop. `refresh()` is cheap: it re-resolves only when the
  effective background string actually changed, so a steady background costs one
  comparison per frame, not a WASM solve.

### Adaptive — smooth, lazy adaptation for live backgrounds

`watchTheme` re-resolves the whole set every time the background changes. For a
background that changes *every frame* (an animation, parallax scroll, blurred
backdrop) that means a full WASM solve per frame, and colours that twitch as they
chase the background. `adaptTheme` is the elegant alternative — the way real
systems (e.g. iOS) behave: a colour adapts *smoothly and with a beat*, not on
every frame.

```ts
import init, { LabColors, adaptTheme, effectiveBackground } from "@labpics/colors";

await init();
const colors = new LabColors();

const surface = document.querySelector(".hero") as HTMLElement;
const adaptive = adaptTheme(surface, {
  colors,
  theme: "light",
  background: () => effectiveBackground(surface, { fallback: "#101012" }),
});

adaptive.start();          // drive an internal requestAnimationFrame loop
adaptive.setTheme("dark"); // a theme switch is intent → applied instantly
adaptive.stop();           // stop the loop
```

Each frame it cheaply **re-checks** whether the current colours still pass their
contrast against the new background (one CAM16 forward for the background plus one
per role, *no solve*).
While they pass it does nothing — no churn, no jitter. Only when a role's contrast
stays below target for a sustained moment does it **re-solve** and **ease** to the
fresh colours over a short crossfade. The control law is principled and tunable:

- **Schmitt trigger** (`dropFraction`) — re-solve only when contrast drops past a
  margin, so a background hovering on the line cannot make it chatter.
- **Debounce** (`sustainMs`) + **dwell** (`dwellMs`) — the breach must persist,
  and re-solves are rate-capped well under the flash threshold.
- **Perceptual ease** (`easeMs`) — a non-overshooting Oklab crossfade (even
  timing, no muddy midpoint); under `prefers-reduced-motion` a gentle short fade,
  never a jarring snap.

Two refinements for demanding surfaces:

- **`strict: true`** — hold each role's WCAG legal floor *every frame* of the ease
  (for text directly on animated content, or under `prefers-contrast`). An
  intermediate colour is shown only while it still clears the role's `legalFloor`;
  the colour advances monotonically to the destination and never dips below the
  line. By default the ease is free (a brief surplus dip during the transition is
  imperceptible while the destination is always legal).
- **Worst-case sampling** — pass `background` as an **array** of samples (or a
  function returning one) for a varying backdrop you sampled yourself; the colours
  are held legible against the *hardest* sample. A single hex is one sample and
  behaves exactly like the solid-background case.

```ts
// A gradient/image backdrop sampled at a few points → legible against the worst.
adaptTheme(hero, {
  colors,
  theme: "light",
  strict: true,
  background: () => sampleBackdrop(hero),   // returns e.g. ["#0B0B0E", "#3A3A40"]
});
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
      wcagRatio: number; compressed: boolean; floorOverride: boolean;
      legalFloor: number | null }   // WCAG clamp (4.5 / 3.0) or null (decorative)
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

### `watchTheme(element, options): WatchController`

Keeps `element`'s `--lab-*` variables in sync with its (effective) background.
Applies immediately on creation and returns a controller.

```ts
interface WatchThemeOptions {
  colors: LabColors;               // an initialised engine (after await init())
  theme: ThemeName;
  background?: string | (() => string);  // explicit effective bg (see below)
  target?: HTMLElement;            // where to write vars (default: element)
  fallback?: string;               // base when the chain is fully translucent (default "#FFFFFF")
  observe?: boolean;               // auto-refresh on DOM mutations (default true)
}

interface WatchController {
  refresh(force?: boolean): ResolvedTheme | null;  // re-resolve+apply if changed
  setTheme(theme: ThemeName): void;                // switch theme, re-apply
  background(): string;                            // last effective background hex
  stop(): void;                                    // disconnect observers
}
```

By default the **effective background** is computed from the DOM (see below). For
a surface over an image, gradient, or blurred backdrop — which has no single
readable colour — pass an explicit `background` (a hex string, or a `() => hex`
you sample yourself).

### `adaptTheme(element, options): AdaptController`

The lazy/hysteresis runtime: re-check each frame, hold while colours still pass,
re-solve + ease only when contrast stably degrades. Applies the resolved set
immediately and returns a controller.

```ts
interface AdaptThemeOptions {
  colors: LabColors;               // an initialised engine (after await init())
  theme: ThemeName;
  background?: string | string[] | (() => string | string[]);
                                   // one solid hex, or worst-case samples of a backdrop
  target?: HTMLElement;            // where to write vars (default: element)
  fallback?: string;               // base when the chain is fully translucent (default "#FFFFFF")
  dropFraction?: number;           // contrast surplus lost before re-solving (default 0.2)
  sustainMs?: number;              // a breach must persist this long (default 120)
  dwellMs?: number;                // minimum between re-solves (default 250)
  easeMs?: number;                 // crossfade duration (default 280; capped under reduced motion)
  strict?: boolean;                // hold the WCAG floor every frame of the ease (default false)
  reducedMotion?: boolean;         // override; defaults to matchMedia
}

interface AdaptController {
  tick(now?: number): void;        // drive one step (or let start() do it)
  setTheme(theme: ThemeName): void;// switch theme INSTANTLY (intent, not drift)
  start(): void;                   // begin an internal requestAnimationFrame loop
  stop(): void;                    // stop the loop
  current(): Record<string, string>; // the currently-applied --lab-* variables
}
```

Drive it with `start()` (internal rAF loop) or call `tick()` yourself from an
existing loop. A theme switch is a deliberate intent, so it applies instantly
(never run through the hysteresis). With a single background sample this behaves
identically to a solid background; an array opts into worst-case sampling.

### `effectiveBackground(element, options?): string`

Resolves the opaque `#RRGGBB` a viewer actually sees behind `element`'s content,
by walking the ancestor chain and **alpha-compositing** each element's
`background-color` (true Porter-Duff source-over) until the stack is opaque, over
a `fallback` (default white). This is what lets a translucent panel
(`rgba(…, .8)`) resolve against the real colour it sits on, not a guess.

```ts
const bg = effectiveBackground(panel);                  // e.g. "#0F1014"
const bg2 = effectiveBackground(panel, { fallback: "#101012" });
```

**Honest limit:** it composites solid/translucent `background-color` layers only —
not `background-image`s, gradients, blurred backdrops, or video. For those,
supply the effective background yourself (sample it, or pass a known value).

Lower-level helpers are exported for callers that sample their own layers:
`parseCssColor`, `compositeOver`, `compositeStackToHex`, `toHex`, and `oklabLerp`
(perceptually-uniform interpolation between two hexes, the ease's colour path).

## Bundle size

| Artifact | gzip | brotli |
|----------|------|--------|
| `labcolors_bg.wasm` | ~54 KB | ~47 KB |
| `labcolors.js` (glue) | ~3 KB | ~3 KB |

The runtime helpers (`applyTheme`, `watchTheme`, `adaptTheme`, `effective-bg`) are
a few hundred bytes of dependency-free JavaScript, tree-shakeable via the
`./watch-theme`, `./adapt-theme`, and `./effective-bg` subpath exports.

Targets modern browsers (2023+). Built with `panic = "abort"` and `wasm-opt -Oz`;
there are no panic pages in the release bundle — errors are returned as values.
