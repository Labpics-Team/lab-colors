// Public types for the adaptive hysteresis controller.

import type { LabColors, ThemeName } from "./index.js";

export interface AdaptThemeOptions {
  /**
   * An initialised engine — needs resolve + contrast recheck. The exact
   * `isStableGlowPointNoop` capability is conditionally required when a result
   * contains a stable Glow role; its absence then fails explicitly.
   * `recheckContrastMulti` is optional: when metric evaluation is performed,
   * it rechecks a multi-sample backdrop in ONE batched call (byte-identical to
   * the per-sample loop, locked by the wasm boundary parity test); when absent,
   * the controller falls back to N `recheckContrast` calls. Unchanged idle
   * ticks skip metric evaluation entirely.
   */
  colors: Pick<LabColors, "resolveTheme" | "recheckContrast"> &
    Partial<Pick<LabColors, "recheckContrastMulti" | "isStableGlowPointNoop">>;
  theme: ThemeName;
  /**
   * Explicit background evidence, overriding the ancestor reference estimate.
   * A single hex is one solid surface; an array (or a function returning one)
   * is a finite, caller-supplied sample set for a varying backdrop (gradient /
   * image / video). The controller compares every supplied point and bases its
   * decision on the worst returned metric; it does not infer between samples
   * or observe the whole field. With one sample this is identical to plain
   * single-background mode.
   */
  background?: string | string[] | (() => string | string[]);
  /** Element to write the `--lab-*` variables onto. Defaults to the watched element. */
  target?: HTMLElement;
  /** Base colour when the ancestor chain is fully translucent. Default `"#FFFFFF"`. */
  fallback?: string;
  /** Fraction of a role's contrast surplus that may be lost before a re-solve. Default `0.2`. */
  dropFraction?: number;
  /** A breach must persist this many ms before re-solving (debounce). Default `120`. */
  sustainMs?: number;
  /** Minimum ms between re-solves (dwell / rate cap). Default `250`. */
  dwellMs?: number;
  /** Crossfade duration in ms. Default `280` (capped to a short fade under reduced motion). */
  easeMs?: number;
  /**
   * Enable the legacy characterized per-frame clamp. The current
   * Oklab→clip→sRGB8 path is not globally monotone, so this option is not a
   * universal floor/least-blend or legibility certificate. Use it only when an
   * integration explicitly needs the characterized legacy clamp. Default
   * `false`.
   */
  strict?: boolean;
  /** Override reduced-motion detection (default reads `matchMedia`). */
  reducedMotion?: boolean;
  /** Clock injection (default `performance.now`/`Date.now`). */
  now?: () => number;
  /** Window-like host (rAF, matchMedia). Defaults to `globalThis`. */
  win?: Window;
  /** Injection seam for the computed style of an element (testing). */
  getStyle?: (element: unknown) => { getPropertyValue(property: string): string };
  /** Injection seam for an element's parent (testing). */
  parentOf?: (element: unknown) => unknown;
}

export interface AdaptController {
  /**
   * Один шаг чтения образцов; неизменное idle-состояние пропускает метрики.
   * Отказ resolver/recheck/evidence до фазы записи сохраняет
   * закоммиченные логические цели и DOM-переменные.
   */
  tick(now?: number): void;
  /**
   * Switch theme INSTANTLY (intent, not drift) — bypasses the hysteresis. A
   * отклонённый кандидат оставляет прежние тему, цели и DOM. Если подготовка
   * reentrant-но запускает более новый `setTheme`/`tick`, новый вызов владеет
   * commit, а устаревший кандидат становится инертным.
   */
  setTheme(theme: ThemeName): void;
  /** Begin an internal `requestAnimationFrame` loop. */
  start(): void;
  /**
   * Остановить внутренний цикл, не выбрасывая незавершённый ease; поздние
   * `start()`/`tick()` продолжат его по текущим часам.
   */
  stop(): void;
  /** Canonical logical targets; during an ease these differ from painted DOM values. */
  current(): Record<string, string>;
}

/**
 * Keep an element's `--lab-*` variables adapting to its (changing) background
 * without re-solving every frame. Each tick reads the declared sample set;
 * metric evaluation is skipped while that set and the pending state are
 * unchanged. A changed/pending set is compared with the last resolved baseline,
 * and re-solve + ease starts only after a sustained relative drop. This does not
 * establish legibility outside the supplied samples or between them. Output
 * conflicts are rejected before DOM/controller mutation and remain retryable.
 */
export declare function adaptTheme(element: HTMLElement, options: AdaptThemeOptions): AdaptController;
