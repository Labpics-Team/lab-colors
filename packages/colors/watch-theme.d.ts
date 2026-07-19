// Public types for the reactive theme runtime.

import type { LabColors, ResolvedTheme, ThemeName } from "./index.js";

export interface WatchThemeOptions {
  /** An initialised `LabColors` engine (after `await init()`). */
  colors: Pick<LabColors, "resolveTheme">;
  /** Theme name. */
  theme: ThemeName;
  /**
   * Explicit reference background, overriding the ancestor estimate. A hex
   * sampled from image/gradient/blur content remains one declared point, not an
   * observation of the whole field. Явное значение обязано быть непустой
   * строкой; невалидное значение не подменяется fallback-оценкой.
   */
  background?: string | (() => string);
  /** Element to write the `--lab-*` variables onto. Defaults to the watched element. */
  target?: HTMLElement;
  /** Непрозрачная поддерживаемая база полностью прозрачной цепочки. По умолчанию `"#FFFFFF"`. */
  fallback?: string;
  /** Auto-refresh on `style`/`class` attribute changes in the observed subtree. Default `true`. */
  observe?: boolean;
  /**
   * Получает отказы observer-обновлений и startup-отказы после захвата
   * observer. Явные `refresh()` и `setTheme()` синхронны и бросают вызывающему.
   * Без обработчика host сообщает об исключении через `reportError`/своё
   * error-событие.
   */
  onError?: (error: unknown) => void;
  /** Mutation-observer root. Defaults to the document element. */
  root?: Node;
  /** Window-like host (for `MutationObserver`). Defaults to `globalThis`. */
  win?: Window;
  /** Injection seam for the computed style of an element (testing). */
  getStyle?: (element: unknown) => { getPropertyValue(property: string): string };
  /** Injection seam for an element's parent (testing). */
  parentOf?: (element: unknown) => unknown;
}

export interface WatchController {
  /**
   * Re-resolve and re-apply if the background/reference input (or theme) changed;
   * `force` re-applies unconditionally. Returns the now-applied result, or the
   * cached one when nothing changed. Returns `null` only when observer
   * acquisition preceded a failed startup, so no snapshot exists yet.
   */
  refresh(force?: boolean): ResolvedTheme | null;
  /** Switch theme and re-apply; a rejected candidate keeps the committed theme. */
  setTheme(theme: ThemeName): void;
  /** The background/reference hex last committed, or `null` before any commit. */
  background(): string | null;
  /** Disconnect observers and stop watching. */
  stop(): void;
}

/**
 * Согласует `--lab-*` элемента с явной подложкой или поддерживаемой оценкой по
 * цепочке предков.
 *
 * Изменения атрибутов `style`/`class` в наблюдаемом поддереве планируют refresh;
 * непрерывные входы обновляются вызовом `refresh()` из цикла
 * `requestAnimationFrame`. Конфликт отклоняется до изменения DOM или состояния
 * контроллера, поэтому то же наблюдение можно повторить. Изменения пикселей и
 * раскладки не отслеживаются. До захвата observer startup-ошибка синхронна;
 * после захвата функция сначала возвращает владельца ресурса, затем сообщает
 * ошибку через `onError`/host error channel.
 */
export declare function watchTheme(
  element: HTMLElement,
  options: WatchThemeOptions,
): WatchController;
