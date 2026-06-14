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

/** Composite an ordered front-to-back layer stack over an opaque base → `#RRGGBB`. */
export declare function compositeStackToHex(layersFrontToBack: Rgba[], opaqueBase: Rgba): string;

/**
 * The opaque effective background `#RRGGBB` visible behind `element`'s content,
 * by walking ancestors and alpha-compositing their `background-color`s.
 */
export declare function effectiveBackground(
  element: unknown,
  opts?: EffectiveBackgroundOptions,
): string;
