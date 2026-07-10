# @labpics/colors

Агностичный контраст-движок для дизайн-систем. Получает фоновый цвет и тему — возвращает полный набор цветовых ролей **вашей** системы. Словарь ролей не встроен в пакет: его задаёт конфиг дизайн-системы (`ThemeConfig`), загружаемый через `loadConfig`; имена вида `--lab-label-primary`, `--lab-border-base` в примерах ниже — из конфига дизайн-системы labui. CSS-переменные несут готовое значение `oklch(L% C H)` (для полупрозрачных ролей — `oklch(L% C H / A)`); сырой `#RRGGBB` остаётся данными роли (`roles.<ключ>.hex`). Ядро написано на Rust и скомпилировано в WebAssembly; пакет не имеет runtime-зависимостей.

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
import dsConfig from "./theme.config.json"; // паспорт вашей дизайн-системы (ThemeConfig)

await init();                                // загрузить WASM-модуль (один раз)
const engine = new LabColors();              // пустой движок: словаря ролей ещё нет
engine.loadConfig(JSON.stringify(dsConfig)); // без этого шага resolveTheme отклонится: config_required

const result = engine.resolveTheme("#FFFFFF", "light");
// result.vars  → готовые CSS-значения по ролям ВАШЕГО конфига:
//                { "--lab-label-primary": "oklch(<L>% <C> <H>)", … }
// result.roles → детали каждой роли (css, hex, контраст, флаги)

applyTheme(document.documentElement, result);   // записать все --lab-* в элемент
```

### Реактивное отслеживание

`watchTheme` синхронизирует переменные с фактическим фоном элемента автоматически.

```ts
import init, { LabColors, watchTheme } from "@labpics/colors";

await init();
const colors = new LabColors();
colors.loadConfig(JSON.stringify(dsConfig));   // конфиг дизайн-системы (см. квик-старт)

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
colors.loadConfig(JSON.stringify(dsConfig));   // конфиг дизайн-системы (см. квик-старт)

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

Создаёт движок **без** словаря ролей: до `loadConfig` любой вызов `resolveTheme` отклоняется ошибкой `config_required`. Повторные одинаковые вызовы `resolveTheme` обслуживаются из внутреннего кэша (кэш-пространство привязано к отпечатку конфига).

---

### `engine.resolveTheme(bgHex, theme): ResolvedTheme`

- `bgHex` — фон в формате `#RGB` или `#RRGGBB`.
- `theme` — `"light" | "dark" | "light-ic" | "dark-ic"`.

Возвращает объект `ResolvedTheme`:

```ts
interface ResolvedTheme {
  theme: ThemeName;
  background: string;                     // нормализованный #RRGGBB
  vars: Record<string, string>;           // достижимые роли: "--lab-<ключ>" → готовое CSS-значение oklch
  roles: Record<string, RoleResult>;      // все роли с деталями
}

type RoleResult =
  | SolvedColor
  | TranslucentRole
  | GlowRole
  | MaterialRole
  | NoneRole
  | UnreachableRole;
```

Каждая роль — одно из шести состояний:

- `SolvedColor` — цвет найден (`kind: "color"`, поля `css` — готовое `oklch(L% C H)`, `hex` — тот же цвет как данные, `lc`, `wcagRatio`, …).
- `TranslucentRole` — полупрозрачная роль лестницы или альфа-аналога (`kind: "translucent"`): `css` — готовое `oklch(L% C H / A)`, `tintHex` — тинт как данные, `alpha`, плюс `compositeHex` / `compositeLc` / `compositeWcag` — exact encoded-sRGB8 reference-композит на фоне резолва и его контраст. Конкретный renderer и color-management pipeline проверяются отдельно.
- `GlowRole` — два цвета для `mix-blend-mode: screen`: `cssVar` несёт halo, `${cssVar}-core` — core, `${cssVar}-alpha` — каноническую `alphaCss`. Цель относится только к изолированному halo; `targetStatus`, оба `*CompositeHex` и оба `*AchievedDj` не выдают point-расчёт за полный blur/overlap-эффект. `referenceProfile` фиксирует конечный расчётный домен, а не гарантирует любой браузер или дисплей.
- `MaterialRole` — тон двухслойного материала и выведенная alpha: primary — солид-канон, `-01` — тинт, `-02` — опаковая база; поля результата явно сообщают гарантию и её границы.
- `NoneRole` — роль намеренно пустая по дизайну (`kind: "none"`), не ошибка.
- `UnreachableRole` — ни один цвет не удовлетворяет требованиям для этого фона (`kind: "unreachable"`).

Недостижимость отдельных ролей — **часть успешного результата**. Отклоняет весь вызов (как `Error`) только при невалидном аргументе:

| Код ошибки | Причина |
|------------|---------|
| `config_required` | конфиг ещё не загружен (`loadConfig` не вызывался) |
| `invalid_background` | `bgHex` не является `#RGB` или `#RRGGBB` |
| `unknown_theme` | `theme` не входит в список допустимых |

---

### `engine.loadConfig(json): string`

Загружает конфиг дизайн-системы (JSON по типу `ThemeConfig`; схема — в репозитории: `docs/decisions/0001-config-boundary.md`, TS-типы — в поставляемом `labcolors.d.ts`). Это **единственный** источник словаря ролей — встроенной таблицы в движке нет: до загрузки конфига `resolveTheme` отклоняется ошибкой `config_required`. Полный preflight проверяет не только имена ролей, но и итоговый CSS-namespace: `-core`/`-alpha` Glow и `-01`/`-02` Material не могут быть затёрты другой ролью или алиасом. Невалидный конфиг отклоняется структурной ошибкой `invalid_config: …` и НЕ меняет состояние. Возвращает отпечаток конфига — 16 hex-символов; разные конфиги дают разные отпечатки и разные кэш-пространства.

---

### `engine.recheckContrast(bgHex, fgHexes, theme): Float64Array`

Дешёвая покадровая проверка: какие контрасты дают цвета `fgHexes` на фоне `bgHex` под темой `theme`, без полного резолва (один прямой ход модели на фон плюс по одному на каждый передний план). Возвращает `Float64Array` пар `[lc, wcagRatio]` в порядке `fgHexes`: индекс `2·i` — знаковый `Lc` цвета `i`, `2·i+1` — его WCAG-отношение. Это примитив, которым `adaptTheme` решает, пора ли пересчитывать.

---

### `engine.recheckContrastMulti(bgHexes, fgHexes, theme): Float64Array`

Батч-вариант `recheckContrast` для меняющегося фона (градиент / картинка / bg-blur / стекло): проверяет один набор `fgHexes` сразу против НЕСКОЛЬКИХ сэмплов фона `bgHexes` за один вызов, разделяя прямой ход модели каждого переднего плана между всеми сэмплами (он от фона не зависит). Байт-в-байт равен N отдельным вызовам `recheckContrast`, пара за парой — это закреплено parity-тестом границы, — только быстрее: ~2.5× на 3 сэмплах. Возвращает плоский background-major `Float64Array`: сэмпл `s`, цвет `i` лежит в `(s · fgHexes.length + i) · 2` (`lc`) и `+1` (`wcagRatio`). Именно этим вызовом `adaptTheme` схлопывает worst-case цикл по сэмплам в одно обращение к движку; на одном сэмпле выигрыша нет — контроллер остаётся на `recheckContrast`.

---

### `engine.muddiness(hex): number` · `engine.confidence(hex): number`

`muddiness` — оценка «грязи» цвета `hex` в диапазоне `[0, 1]` (Закон Грязи). `confidence` — надёжность этой оценки: `0` означает, что оценке нельзя доверять (у границы решения или серого фронтира), выше — увереннее. Верхний потолок — деталь калибровки, не контракт: не хардкодить.

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
  root?: Node;                           // корень MutationObserver (по умолчанию: documentElement)
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

Размер не закреплён в документации приблизительным числом: оно устаревает при
любом изменении солвера. SSOT — шаг CI `report bundle size (gzip)` в `ci.yml`,
который для каждого коммита печатает точные raw-байты и результат `gzip -9`
отдельно для `labcolors_bg.wasm` и wasm-bindgen-обёртки `labcolors.js`.

Это весь движок: CAM16, солверы контраста, лестницы и граница конфига. `.wasm`
поставляется отдельным ассетом. Будет ли его загрузка критическим путём первого
рендера, определяет интеграция: до первого `resolveTheme` инициализация обязана
завершиться; приложение может preload/кэшировать модуль. JS-хелперы
(`applyTheme`, `watchTheme`, `adaptTheme`, `effectiveBackground`) имеют
именованные экспорты и допускают tree-shaking, но их размер также следует мерить
сборкой, а не описывать приблизительно.

Требует современных браузеров (2023+): сборщики Vite / webpack 5 / Next штатно понимают `new URL('….wasm', import.meta.url)` (вывод wasm-pack). В node передавайте wasm-байты напрямую в `init({ module_or_path })`.

### Для аудиторов цепочки поставки

- **Network access (Socket и др.):** единственный `fetch` в пакете (`pkg/labcolors.js`) загружает СОБСТВЕННЫЙ `.wasm`-файл пакета при `init(url)` — стандартный лоадер wasm-bindgen. Ни внешних адресов, ни отправки данных, ни исполнения при импорте. В node-пути (передача байтов) `fetch` не вызывается.
- **Bundlephobia `BuildError`:** их webpack-конвейер не умеет `.wasm`-ассеты («loader customization needed») — так падает почти любой WASM-пакет. Реальный размер конкретного коммита показывает CI-шаг `report bundle size (gzip)`.
- **Zero runtime JS-dependencies:** npm-поле `dependencies` пусто — транзитивной JS/npm-цепочки поставки нет. Rust-крейты сборки (`serde`, `serde_json` и др.) компилируются ВНУТРЬ `.wasm` (учтены CI-замером); их цепочка аудируется на стороне сборки — `cargo audit` (RustSec) в CI lab-colors.
