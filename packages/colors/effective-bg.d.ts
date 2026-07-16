// Public types for the effective-background resolver.

/** `[r, g, b, a]` — r,g,b in 0..255, a in 0..1. */
export type Rgba = [number, number, number, number];

/** A computed-style-like accessor: only `getPropertyValue` is used. */
export interface StyleLike {
  getPropertyValue(property: string): string;
}

export interface EffectiveBackgroundOptions {
  /** Base colour when the ancestor chain never reaches an opaque layer. Default `"#FFFFFF"`. */
  fallback?: string;
  /** Injection seam for the computed style of an element. Defaults to `getComputedStyle`. */
  getStyle?: (element: unknown) => StyleLike;
  /** Injection seam for an element's parent. Defaults to `el.parentElement`. */
  parentOf?: (element: unknown) => unknown;
  /** Guard against detached/cyclic chains. Default `64`. */
  maxDepth?: number;
}

/** Parse a CSS colour string into `[r,g,b,a]`, or `null` if unrecognised. */
export declare function parseCssColor(css: string): Rgba | null;

/** Porter-Duff source-over composite of `top` onto `bottom`. */
export declare function compositeOver(top: Rgba, bottom: Rgba): Rgba;

/** `[r,g,b]` (0..255) → `#RRGGBB`. */
export declare function toHex(rgb: Rgba | [number, number, number]): string;

/**
 * Linearly interpolate the Oklab coordinates of two colours at `t ∈ [0,1]` →
 * `#RRGGBB`. `from`/`to` may be any string `parseCssColor` accepts (`#hex`,
 * `rgb()`/`rgba()`, `oklch()`, `transparent`), not only `#RRGGBB`. Endpoints are
 * exact and out-of-gamut intermediate channels are clamped.
 */
export declare function oklabLerp(from: string, to: string, t: number): string;

/** Composite an ordered front-to-back layer stack over an opaque base → `#RRGGBB`. */
export declare function compositeStackToHex(layersFrontToBack: Rgba[], opaqueBase: Rgba): string;

/**
 * Opaque reference estimate for the supported solid/translucent ancestor
 * `background-color` chain, composited over the declared fallback. This is not
 * a browser pixel observation and does not account for images, gradients,
 * filters, video, or other unsupported layers.
 */
export declare function effectiveBackground(
  element: unknown,
  opts?: EffectiveBackgroundOptions,
): string;
