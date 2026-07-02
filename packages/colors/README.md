# @labpics/colors

Адаптивные цветовые роли для дизайн-системы. Получает фоновый цвет и тему — возвращает полный набор ролей (`--lab-label-primary`, `--lab-icon`, `--lab-border-base`, …) как `#RRGGBB`-значения CSS-переменных. Ядро написано на Rust и скомпилировано в WebAssembly; пакет не имеет runtime-зависимостей.

Ядро возвращает **данные**, не затрагивает DOM. Три вспомогательные функции переводят эти данные в живые CSS-переменные: `applyTheme` (разовое применение), `watchTheme` (реактивное — обновляется при изменении фона) и `adaptTheme` (плавная адаптация для фона, меняющегося каждый кадр).

---

## Установка

Пакет публикуется в npm-реестр Labpics:

```sh
npm install @labpics/colors
```

При сборке из монорепо:

```sh
npm run build   # → pkg/ (wasm + JS-обёртка + .d.ts)
```

---

## Как использовать

### Разовое применение

```ts
import init, { LabColors, applyTheme } from "@labpics/colors";

await init();                       // загрузить WASM-модуль (один раз)
const engine = new LabColors();     // движок по умолчанию

const result = engine.resolveTheme("#FFFFFF", "light");
// result.vars  → { "--lab-label-primary": "#1a1a1a", "--lab-icon": "#5b5b5b", ... }
// result.roles → детали каждой роли (hex, контраст, флаги)

applyTheme(document.documentElement, result);   // записать все --lab-* в элемент
```

### Реактивное отслеживание

`watchTheme` синхронизирует переменные с фактическим фоном элемента автоматически.

```ts
import init, { LabColors, watchTheme } from "@labpics/colors";

await init();
const colors = new LabColors();

const panel = document.querySelector(".panel") as HTMLElement;
const watcher = watchTheme(panel, { colors, theme: "light" });
// panel теперь несёт правильные --lab-* для своего фона

watcher.setTheme("dark");   // переключить тему
watcher.refresh();          // принудительная пересинхронизация
watcher.stop();             // отключить наблюдателя
```

Поддерживает два режима изменений:

- **Дискретные** (переключение темы, обновление класса, DOM-мутация) — отслеживаются `MutationObserver` автоматически.
- **Непрерывные** (CSS-анимация, кадровый фон, который никогда не мутирует `inline style`) — управляются вручную через `refresh()` из собственного цикла `requestAnimationFrame`. `refresh()` дёшев: пересчитывает только при реальном изменении строки фона.

### Плавная адаптация к анимированному фону

`watchTheme` пересчитывает весь набор при каждом изменении фона. Для фона, меняющегося **каждый кадр** (анимация, параллакс, размытый фон), `adaptTheme` — более элегантный вариант: проверяет дёшево каждый кадр, пересчитывает и плавно переходит к новым цветам только когда контраст реально ухудшается.

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

adaptive.start();           // запустить внутренний requestAnimationFrame-цикл
adaptive.setTheme("dark");  // смена темы применяется мгновенно
adaptive.stop();            // остановить цикл
```

Для фона с градиентом или изображением можно передать несколько образцов — цвета будут корректны для наиболее сложного из них:

```ts
adaptTheme(hero, {
  colors,
  theme: "light",
  strict: true,
  background: () => sampleBackdrop(hero),  // например ["#0B0B0E", "#3A3A40"]
});
```

---

## Темы

| Имя темы | Назначение |
|----------|------------|
| `"light"` | Светлая тема |
| `"dark"` | Тёмная тема |
| `"light-ic"` | Светлая с повышенным контрастом |
| `"dark-ic"` | Тёмная с повышенным контрастом |

---

## Справочник публичного API

### `new LabColors()`

Создаёт движок с таблицей ролей и условиями наблюдения по умолчанию. Повторные одинаковые вызовы `resolveTheme` обслуживаются из внутреннего кэша.

---

### `engine.resolveTheme(bgHex, theme): ResolvedTheme`

- `bgHex` — фон в формате `#RGB` или `#RRGGBB`.
- `theme` — `"light" | "dark" | "light-ic" | "dark-ic"`.

Возвращает объект `ResolvedTheme`:

```ts
interface ResolvedTheme {
  theme: ThemeName;
  background: string;                     // нормализованный #RRGGBB
  vars: Record<string, string>;           // достижимые роли: "--lab-<ключ>" → hex
  roles: Record<string, RoleResult>;      // все роли с деталями
}

type RoleResult = SolvedColor | NoneRole | UnreachableRole;
```

Каждая роль — одно из трёх состояний:

- `SolvedColor` — цвет найден (`kind: "color"`, поля `hex`, `lc`, `wcagRatio`, …).
- `NoneRole` — роль намеренно пустая по дизайну (`kind: "none"`), не ошибка.
- `UnreachableRole` — ни один цвет не удовлетворяет требованиям для этого фона (`kind: "unreachable"`).

Недостижимость отдельных ролей — **часть успешного результата**. Отклоняет весь вызов (как `Error`) только при невалидном аргументе:

| Код ошибки | Причина |
|------------|---------|
| `invalid_background` | `bgHex` не является `#RGB` или `#RRGGBB` |
| `unknown_theme` | `theme` не входит в список допустимых |

---

### `applyTheme(element, result): void`

Записывает все достижимые роли из `result.vars` в `element.style` через `setProperty`. Устаревшие `--lab-*` от предыдущего вызова сбрасываются перед записью — роль, потерявшая достижимость, не остаётся висеть.

---

### `watchTheme(element, options): WatchController`

Синхронизирует `--lab-*` переменные с фактическим фоном элемента. Применяет немедленно при создании и возвращает контроллер.

```ts
interface WatchThemeOptions {
  colors: LabColors;
  theme: ThemeName;
  background?: string | (() => string);  // явный фон (если автоматический невозможен)
  target?: HTMLElement;                  // куда писать переменные (по умолчанию: element)
  fallback?: string;                     // фон при полностью прозрачной цепочке (по умолчанию "#FFFFFF")
  observe?: boolean;                     // авто-обновление при DOM-мутациях (по умолчанию true)
}

interface WatchController {
  refresh(force?: boolean): ResolvedTheme | null;  // пересчитать и применить, если фон изменился
  setTheme(theme: ThemeName): void;                // переключить тему и применить
  background(): string;                            // последний вычисленный фон (hex)
  stop(): void;                                    // отключить наблюдателей
}
```

Для поверхности над изображением, градиентом или размытым фоном — где автоматическое вычисление невозможно — передайте явный `background` (hex-строку или функцию, возвращающую hex).

---

### `adaptTheme(element, options): AdaptController`

Режим с гистерезисом: дешёвая проверка каждый кадр, пересчёт и плавный переход только при устойчивом ухудшении контраста. Применяет немедленно и возвращает контроллер.

```ts
interface AdaptThemeOptions {
  colors: LabColors;
  theme: ThemeName;
  background?: string | string[] | (() => string | string[]);
                                     // один hex или несколько образцов фона (наихудший учитывается)
  target?: HTMLElement;              // куда писать переменные (по умолчанию: element)
  fallback?: string;                 // фон при прозрачной цепочке (по умолчанию "#FFFFFF")
  dropFraction?: number;             // запас контраста до пересчёта (по умолчанию 0.2)
  sustainMs?: number;                // минимальное время удержания нарушения (по умолчанию 120)
  dwellMs?: number;                  // минимальный интервал между пересчётами (по умолчанию 250)
  easeMs?: number;                   // длительность перехода (по умолчанию 280; уменьшается при reduced-motion)
  strict?: boolean;                  // держать минимальный контраст на каждом кадре перехода (по умолчанию false)
  reducedMotion?: boolean;           // переопределить системную настройку
}

interface AdaptController {
  tick(now?: number): void;          // один шаг (или использовать start())
  setTheme(theme: ThemeName): void;  // переключить тему мгновенно
  start(): void;                     // запустить внутренний requestAnimationFrame-цикл
  stop(): void;                      // остановить цикл
  current(): Record<string, string>; // текущие применённые --lab-* переменные
}
```

Управляйте через `start()` (внутренний rAF-цикл) или вызывайте `tick()` из собственного цикла. Смена темы применяется мгновенно — это осознанное намерение, а не дрейф.

---

### `effectiveBackground(element, options?): string`

Возвращает непрозрачный `#RRGGBB` — цвет, который наблюдатель реально видит за содержимым `element`. Обходит цепочку предков и выполняет альфа-композитинг каждого `background-color` до получения непрозрачного результата, поверх `fallback` (по умолчанию белый).

```ts
const bg = effectiveBackground(panel);                        // например "#0F1014"
const bg2 = effectiveBackground(panel, { fallback: "#101012" });
```

**Честное ограничение:** работает только с сплошными и полупрозрачными `background-color` — не с `background-image`, градиентами, размытым фоном или видео. Для таких поверхностей передайте фон явно.

Дополнительно экспортируются вспомогательные функции для работы со слоями: `parseCssColor`, `compositeOver`, `compositeStackToHex`, `toHex` и `oklabLerp` (перцептуально равномерная интерполяция между двумя hex-значениями — траектория цвета при переходе).

---

## Размер бандла

| Артефакт | raw | gzip | brotli |
|----------|-----|------|--------|
| `labcolors_bg.wasm` | ~313 КБ | ~138 КБ | ~116 КБ |
| `labcolors.js` (JS-обёртка) | ~17 КБ | ~4 КБ | ~4 КБ |

Это ВЕСЬ движок: перцептивная модель CAM16, солверы контраста, лестницы, граница конфига. `.wasm` — не JS-байты бандла, а ассет: он **не на критическом пути рендера** — грузится параллельно, компилируется потоково вне главного треда, кэшируется браузером после первой загрузки. Вспомогательные функции (`applyTheme`, `watchTheme`, `adaptTheme`, `effectiveBackground`) — несколько сотен байт чистого JavaScript с tree-shaking через именованные экспорты.

Требует современных браузеров (2023+): сборщики Vite / webpack 5 / Next штатно понимают `new URL('….wasm', import.meta.url)` (вывод wasm-pack). В node передавайте wasm-байты напрямую в `init({ module_or_path })`.

### Для аудиторов цепочки поставки

- **Network access (Socket и др.):** единственный `fetch` в пакете (`pkg/labcolors.js`) загружает СОБСТВЕННЫЙ `.wasm`-файл пакета при `init(url)` — стандартный лоадер wasm-bindgen. Ни внешних адресов, ни отправки данных, ни исполнения при импорте. В node-пути (передача байтов) `fetch` не вызывается.
- **Bundlephobia `BuildError`:** их webpack-конвейер не умеет `.wasm`-ассеты («loader customization needed») — так падает почти любой WASM-пакет. Реальные размеры — в таблице выше (замер CI-шага `report bundle size`).
- **Zero runtime dependencies:** npm-поле `dependencies` пусто — нет транзитивной JS-цепочки поставки, нечего аудировать за пределами этого пакета. (`serde`/`serde_json` — Rust-крейты сборки: они компилируются ВНУТРЬ `.wasm` и учитываются в его размере выше, но JS-зависимостью не являются.)
