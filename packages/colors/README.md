# @labpics/colors

Агностичный контраст-движок для дизайн-систем. Получает фоновый цвет и тему — возвращает полный набор цветовых ролей **вашей** системы. Словарь ролей не встроен в пакет: его задаёт конфиг дизайн-системы (`ThemeConfig`), загружаемый через `loadConfig`; имена вида `--lab-label-primary`, `--lab-border-base` в примерах ниже — из конфига дизайн-системы labui. CSS-переменные несут готовое значение `oklch(L% C H)` (для полупрозрачных ролей — `oklch(L% C H / A)`); сырой `#RRGGBB` остаётся данными роли (`roles.<ключ>.hex`). Пакет не имеет runtime-зависимостей и разделён на две WASM-роли: runtime resolver в корне и offline compiler в `@labpics/colors/compiler`.

Ядро возвращает **данные**, не затрагивает DOM. Три вспомогательные функции переводят эти данные в живые CSS-переменные: `applyTheme` (разовое применение), `watchTheme` (реактивное — обновляется при изменении фона) и `adaptTheme` (плавная адаптация для фона, меняющегося каждый кадр).

---

## Установка

Пакет публикуется в npm-реестр Labpics:

```sh
npm install @labpics/colors
```

Минимально поддерживается TypeScript `>= 5.2.2`: ветка 5.2 впервые добавила
стандартный `esnext.disposable` ([официальное описание TypeScript
5.2](https://www.typescriptlang.org/docs/handbook/release-notes/typescript-5-2.html)).
Публичный `.d.ts` подключает эту библиотеку сам, поэтому потребителю не требуется
расширять свой `lib`. Release-gate компилирует чисто установленный пакет с
`skipLibCheck: false` и точным floor `5.2.2`, и текущим compiler из lockfile.

При сборке из монорепо:

```sh
npm run build   # → pkg/ (runtime) + compiler/ (offline compiler)
```

Пакет экспортирует `@labpics/colors/build-metadata.json` — self-declared
machine-readable metadata конкретной сборки: npm/core versions, exact source
SHA, digest и SHA-256 conformance manifest/family set, а также размер и SHA-256
упорядоченных `runtime`/`compiler` WASM-артефактов. Release-gate сверяет schema 2
целиком с исходными файлами и повторяет
проверку после чистой установки tarball. Это integrity metadata внутри
артефакта, не криптографическая provenance/Sigstore-аттестация, не runtime
telemetry и не сетевой запрос.

---

## Как использовать в браузере

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

`watchTheme` синхронизирует переменные с явно переданным фоном или опорной
оценкой поддерживаемой цепочки `background-color`.

```ts
import init, { LabColors, watchTheme } from "@labpics/colors";

await init();
const colors = new LabColors();
colors.loadConfig(JSON.stringify(dsConfig));   // конфиг дизайн-системы (см. квик-старт)

const panel = document.querySelector(".panel") as HTMLElement;
const watcher = watchTheme(panel, { colors, theme: "light" });
// panel теперь несёт --lab-* для текущей опорной оценки фона

watcher.setTheme("dark");   // переключить тему
watcher.refresh();          // принудительная пересинхронизация
watcher.stop();             // отключить наблюдателя
```

Поддерживает два режима изменений:

- **Дискретные DOM-входы** — изменения атрибутов `style`/`class` в наблюдаемом поддереве автоматически планируют `refresh()` через `MutationObserver`; layout, canvas, пиксельные изменения и другие атрибуты он не наблюдает.
- **Тема** меняется явно через `watcher.setTheme(...)`; это не событие `MutationObserver`.
- **Непрерывные** (CSS-анимация, кадровый фон, который никогда не мутирует `inline style`) — управляются вручную через `refresh()` из собственного цикла `requestAnimationFrame`. `refresh()` дёшев: пересчитывает только при реальном изменении строки фона.

### Плавная адаптация к анимированному фону

`watchTheme` пересчитывает весь набор при каждом изменении доступной оценки
фона. Для поддерживаемого `background-color`, меняющегося **каждый кадр**,
`adaptTheme` читает образцы на каждом `tick`, но повторно вычисляет метрики только
когда набор образцов изменился либо продолжается pending breach/transition.
Новый resolve с переходом запускается после устойчивого относительного снижения
отслеживаемых метрик.

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

Для градиента, изображения или видео интеграция может передать конечный набор
самостоятельно полученных образцов. Контроллер проверяет только переданные точки:
он не наблюдает всё поле и не переносит результат на промежутки между образцами.

```ts
adaptTheme(hero, {
  colors,
  theme: "light",
  background: () => sampleBackdrop(hero),  // например ["#0B0B0E", "#3A3A40"]
});
```

### Инициализация в Node

Node получает локальные WASM-байты явно; нулевая форма `init()` предназначена
для браузерного loader-а. Runtime и compiler инициализируются независимо:

```ts
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import initRuntime from "@labpics/colors";
import initCompiler from "@labpics/colors/compiler";

const require = createRequire(import.meta.url);
const runtimeWasm = await readFile(
  require.resolve("@labpics/colors/pkg/labcolors_bg.wasm"),
);
const compilerWasm = await readFile(
  require.resolve("@labpics/colors/compiler/wasm"),
);

await initRuntime({ module_or_path: runtimeWasm });
await initCompiler({ module_or_path: compilerWasm });
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
  vars: Record<string, string>;           // роли с выбранным CSS-значением: "--lab-<ключ>" → oklch
  roles: Record<string, RoleResult>;      // все роли с деталями
}

type RoleResult =
  | SolvedColor
  | TranslucentRole
  | GlowRole
  | MaterialRole
  | NoneRole
  | FailureRole;
```

Каждая роль — одна из шести форм результата; у Glow есть два терминальных
состояния:

- `SolvedColor` — цвет найден (`kind: "color"`, поля `css` — готовое `oklch(L% C H)`, `hex` — тот же цвет как данные, `lc`, `wcagRatio`, …).
- `TranslucentRole` — полупрозрачная роль лестницы или альфа-аналога (`kind: "translucent"`): `css` — готовое `oklch(L% C H / A)`, `tintHex` — тинт как данные, `alpha`, плюс `compositeHex` / `compositeLc` / `compositeWcag` — exact encoded-sRGB8 reference-композит на фоне резолва и его контраст. Конкретный renderer и color-management pipeline проверяются отдельно.
- `GlowRole` — discriminated union. `kind: "glow"` несёт два цвета для
  `mix-blend-mode: screen`: `cssVar` — halo, `${cssVar}-core` — core,
  `${cssVar}-alpha` — каноническая `alphaCss`. `compositeProfile` /
  `compositeGuarantee` описывают exact point-композит отдельно от
  `decisionGuarantee`. `layerRecipeProfile` фиксирует общий рецепт слоёв
  `"cam16-jprime-oklab-cusp-v1"`, а обязательный
  `appearanceDiagnosticProfile` — CAM16 appearance-модель результата. Отдельный
  `selectionDiagnosticProfile` сообщает, применялась ли CAM16-диагностика именно
  при выборе состояния: для exact no-op он равен `null`, для legacy solve —
  `"cam16-ucs-jprime-li2017-v1"`. `targetStatus` различает эти ветви явно:
  `"exact-noop-unreachable"`, `"legacy-reached"` или
  `"legacy-unreachable"`. Форма `kind: "glow"` — объединение этих трёх
  согласованных ветвей: смешать stable/legacy профиль, гарантию, диагностику
  выбора, статус и `degraded` на уровне TypeScript невозможно. Legacy-ветви
  (`decisionGuarantee: { kind: "legacy-platform-dependent-v1" }`) — это
  результат совместимости зарегистрированного алгоритма, а не доказанная
  численная гарантия. Интервал с внешним округлением не
  входит в Glow-возможности этого релиза. `kind: "glow-indeterminate"`
  означает, что профиль `stable-v1` не выбрал target/max state без sound bound;
  для такой роли CSS-переменные не эмитятся и legacy fallback не применяется.
  Цель и все `*CompositeHex` / `*AchievedDj` относятся только к изолированным
  point-слоям, а не к полному blur/overlap-эффекту, браузеру или дисплею.
- `MaterialRole` — объединение трёх терминальных исходов: satisfied transparent
  endpoint (`alpha = 0`), satisfied bisection bracket (`alpha` побитно равна
  `upperAlpha`) и degraded opaque endpoint (`alpha = 1`). `alphaStatus`,
  `alphaGuarantee` и compatibility-поле `guaranteed` согласованы типом. `floor`
  — запрошенный пол; он держится только при `alphaStatus: "satisfied"`. Primary
  остаётся солид-каноном, `-01` — тинтом, `-02` — опаковой базой. Численный
  профиль явно фиксирует operation order:
  `encoded-srgb-byte-scale-affine-platform-binary64-powf-v1`: original WCAG 2.1
  (2018) split `0.03928`, conservative binary64 channel envelope и обе стороны
  пересечённого EOTF seam. Bracket доступен только после directed-search guard;
  это platform-characterization, а не двухугловая теорема, глобальная
  монотонность, первый passing state или точная минимальная alpha.
- `NoneRole` — роль намеренно пустая по дизайну (`kind: "none"`), не ошибка.
- `FailureRole` — типизированный терминальный исход без выбранного
  цвета (`kind: "failure"`). `category` отделяет доказанную
  недостижимость (`"unreachable"`) от исхода ограниченного поиска без
  доказательства (`"unresolved"`). `code` уточняет причину.

Ожидаемый failure отдельной роли — **часть успешного результата** и не
попадает в `vars`. `rejected`, `unsupported` и `internal` не являются
`RoleResult`: они атомарно отклоняют весь `resolveTheme`, поэтому частичного
`ResolvedTheme`, `vars` или CSS не существует. После успешного preflight такой
дрейф Core выходит через Engine как `internal_error` с исходной причиной в
сообщении. Публичные ошибки вызова:

| Код ошибки | Причина |
|------------|---------|
| `config_required` | конфиг ещё не загружен (`loadConfig` не вызывался) |
| `invalid_background` | `bgHex` не является `#RGB` или `#RRGGBB` |
| `unknown_theme` | `theme` не входит в список допустимых |
| `internal_error` | Core нарушил собственный инвариант; частичный CSS не возвращается |

---

### `engine.loadConfig(json): string`

Загружает конфиг дизайн-системы (JSON по типу `ThemeConfig`; схема — в репозитории: `docs/decisions/0001-config-boundary.md`, TS-типы — в поставляемом `labcolors.d.ts`). Это **единственный** источник словаря ролей — встроенной таблицы в движке нет: до загрузки конфига `resolveTheme` отклоняется ошибкой `config_required`. Полный preflight проверяет не только имена ролей, но и итоговый CSS-namespace: `-core`/`-alpha` Glow и `-01`/`-02` Material не могут быть затёрты другой ролью или алиасом. Невалидный конфиг отклоняется структурной ошибкой `invalid_config: …` и НЕ меняет состояние. Возвращает отпечаток конфига — 16 hex-символов; разные конфиги дают разные отпечатки и разные кэш-пространства.

Каждый Glow-рецепт обязан явно выбрать numerical-decision profile; default и
неявного legacy-пути нет:

```json
{
  "kind": "glow",
  "source": { "kind": "brand" },
  "step": "base",
  "decision_profile": "stable-v1"
}
```

`stable-v1` возвращает typed `glow-indeterminate`, когда sound bound для
нетривиального CAM16 target/max-решения недоступен. Если существующая интеграция
обязана пока сохранить прежнюю CSS-эмиссию, укажите
`legacy-platform-dependent-v1` явно и обрабатывайте его как compatibility
profile, а не как численную гарантию.

---

### `evaluateWcag22(foreground, background, criterion): Wcag22AssessmentV1`

Проверяет одну **финальную sRGB8-пару** по датированному профилю WCAG 2.2. Core
не угадывает назначение токена или размер текста: клиент явно выбирает критерий
для конкретного использования, а evaluator возвращает строгий `pass | fail`.

```js
import init, { evaluateWcag22 } from "@labpics/colors";

await init();
const assessment = evaluateWcag22(
  "#898CB8",
  "#3E2217",
  "sc-1.4.3-text-default",
);
// assessment.decision === "fail" — значение строго ниже 4.5:1
```

| `criterion` | Порог | Когда выбирать |
|---|---:|---|
| `sc-1.4.3-text-default` | 4.5:1 | обычный текст |
| `sc-1.4.3-text-large-scale` | 3:1 | только текст, который клиент уже классифицировал как large-scale по WCAG |
| `sc-1.4.11-ui-component-or-state` | 3:1 | необходимая визуальная информация компонента или состояния |
| `sc-1.4.11-graphical-object` | 3:1 | необходимая визуальная информация графического объекта |

Core не выводит критерий из имени токена, CSS-класса, размера шрифта или
компонента: applicability принадлежит клиентскому контексту. Неверный ключ и
нестрогий цветовой transport отклоняются ошибкой, без fallback.

Решение не использует epsilon, округлённый display-ratio или отдельную JS-
формулу. Оно приходит из Rust core вместе с identity профиля, Q55-таблицы,
bound-law и воспроизводимого full-domain proof. Файлы доказательства входят в
npm-тарбол в `evidence/`; proof также SHA-256-связан с фактической typed
registry-строкой, разрешающей Core минтить terminal evidence. APCA-shaped
diagnostic-компонент текущего LPC и legacy `wcagRatio` не могут изменить этот
вердикт.

---

### Offline compiler: `evaluateWcag22Feasibility(request)`

Полностью перебирает зарегистрированную конечную ось sRGB8 против всех явно
объявленных клиентом связей. Операция отвечает только на вопрос «какие
кандидаты проходят все эти ограничения?». Она не выбирает лучший цвет, не
угадывает применимость и не понимает семантику ID.

Операция экспортируется только из `@labpics/colors/compiler` и загружает
отдельный compiler WASM; package root остаётся runtime API. Операция принимает
только зарегистрированную нейтральную ось V1.

В браузере compiler принадлежит offline/Worker execution class: main thread
импортирует только runtime, а dedicated module Worker владеет compiler WASM и
полным вызовом. Node build-tooling может вызывать тот же entry напрямую.

```ts
// color-compiler.worker.ts
import initCompiler, {
  evaluateWcag22Feasibility,
  type Wcag22FeasibilityRequestV1,
} from "@labpics/colors/compiler";

await initCompiler();

self.addEventListener(
  "message",
  ({ data }: MessageEvent<Wcag22FeasibilityRequestV1>) => {
    const bytes = new TextEncoder().encode(JSON.stringify(data));
    self.postMessage(evaluateWcag22Feasibility(bytes));
  },
);
self.postMessage({ type: "ready" } as const);
```

```ts
// build-colors.ts — main thread не импортирует runtime-код compiler-а
import type {
  Wcag22FeasibilityOutcomeV1,
  Wcag22FeasibilityRequestV1,
} from "@labpics/colors/compiler";

const request: Wcag22FeasibilityRequestV1 = {
  schemaVersion: 1,
  domainId: "srgb8-neutral-axis-v1",
  resourceProfileId: "compile-v1",
  relations: [{
    relationId: "opaque-relation-7f3a",
    occurrenceId: "opaque-occurrence-17",
    kind: "applicable",
    criterion: "sc-1.4.3-text-default",
    adjacent: [[0, 0, 0], [255, 255, 255]],
  }],
};

const outcome = await new Promise<Wcag22FeasibilityOutcomeV1>((resolve, reject) => {
  const worker = new Worker(new URL("./color-compiler.worker.ts", import.meta.url), {
    type: "module",
  });
  worker.addEventListener("message", ({ data }) => {
    if (data?.type === "ready") {
      worker.postMessage(request);
      return;
    }
    worker.terminate();
    resolve(data);
  });
  worker.addEventListener("error", (event) => {
    worker.terminate();
    reject(event.error ?? new Error(event.message));
  }, { once: true });
});

if (outcome.outcome === "success") {
  // feasible | infeasible | notEvaluated
  console.log(outcome.feasibility.status);
} else {
  // strict transport/core failure as typed data; no fallback colour
  console.error(outcome.error);
}
```

Вход — только настоящий `Uint8Array` со strict JSON V1; иной JavaScript-тип
детерминированно отклоняется `TypeError` до чтения WASM-owned ceiling и копии.
Граница размера выведена из грамматики и resource profile; после `initCompiler()` её
возвращает `wcag22FeasibilityMaxBytes()`. Package wrapper проверяет `byteLength`
до избежимой копии в WASM, а Rust повторяет авторитетную проверку. Выход хранит
домен и канонические связи по одному разу, а решения — в candidate-major LSB0
bitset; объектного графа `256 × E` в public result нет.

---

### `numericalCapabilityManifest(): NumericalCapabilityManifestV2`

Возвращает статический манифест численных возможностей установленной сборки:
какие solver-sites зарегистрированы и какие artifact/bound/proof IDs они могут
выдать. Это диагностическая поверхность для tooling, CI и ИИ-агентов; она не
выбирает режим и не превращает compatibility-результат в доказанный. Публичная
функция одна: отдельного V1 или `numericalCapabilityManifestV2()` нет.

---

### `engine.recheckContrast(bgHex, fgHexes, theme): Float64Array`

Дешёвая покадровая проверка: какие контрасты дают цвета `fgHexes` на фоне `bgHex` под темой `theme`, без полного резолва (один прямой ход модели на фон плюс по одному на каждый передний план). Возвращает `Float64Array` пар `[lc, wcagRatio]` в порядке `fgHexes`: индекс `2·i` — знаковый `Lc` цвета `i`, `2·i+1` — его WCAG-отношение. Это примитив, которым `adaptTheme` решает, пора ли пересчитывать.

---

### `engine.recheckContrastMulti(bgHexes, fgHexes, theme): Float64Array`

Батч-вариант `recheckContrast` для конечного набора образцов меняющегося фона
(градиент / картинка / bg-blur / стекло): проверяет один набор `fgHexes` сразу
против нескольких `bgHexes`, разделяя прямой ход модели каждого переднего плана
между всеми образцами (он от фона не зависит). Результат байт-в-байт равен N
отдельным вызовам `recheckContrast`, пара за парой — это закреплено parity-тестом
границы. Возвращает плоский background-major `Float64Array`: образец `s`, цвет
`i` лежит в `(s · fgHexes.length + i) · 2` (`lc`) и `+1` (`wcagRatio`).
`adaptTheme` использует один батч-вызов вместо N отдельных пересечений границы;
эффект на производительность зависит от host и интеграции и без отдельного
воспроизводимого гейта не заявляется. Для одного образца контроллер остаётся на
`recheckContrast`.

---

### `engine.muddiness(hex): number`

`muddiness` возвращает замороженный числовой выход исторической формулы. Это
`experimental compatibility proxy`: legacy-имя сохранено для совместимости, но
значение не является валидированным на наблюдателях человеческим вердиктом
clean/dirty и не должно использоваться как production decision. Сам диапазон
`[0, 1]` — свойство формулы, а не шкала человеческого восприятия.

---

### `applyTheme(element, result): void`

Записывает все выбранные CSS-значения из `result.vars` в `element.style` через
`setProperty`. Устаревшие `--lab-*` от предыдущего вызова сбрасываются перед
записью: роль, перешедшая в `failure`, `none` или `glow-indeterminate`, не оставляет
устаревшее значение. Передавайте сюда полный успешный снимок `resolveTheme`:
функция не знает клиентскую схему и сама не может доказать полноту вручную
собранных `result.vars`. Если `resolveTheme` отклонён, нового снимка нет и вызывать
`applyTheme` не с чем; DOM остаётся прежним. В успешном снимке `failure` может
быть только `unreachable` или `unresolved`.

---

### `watchTheme(element, options): WatchController`

Синхронизирует `--lab-*` переменные с явно переданным фоном или опорной оценкой
поддерживаемой `background-color`-цепочки. Применяет первый результат немедленно
и возвращает контроллер.

```ts
interface WatchThemeOptions {
  colors: LabColors;
  theme: ThemeName;
  background?: string | (() => string);  // явный фон (если автоматический невозможен)
  target?: HTMLElement;                  // куда писать переменные (по умолчанию: element)
  fallback?: string;                     // фон при полностью прозрачной цепочке (по умолчанию "#FFFFFF")
  observe?: boolean;                     // авто-обновление при style/class в поддереве (по умолчанию true)
  onError?: (error: unknown) => void;     // ошибки автоматического observer-refresh
  root?: Node;                           // корень MutationObserver (по умолчанию: documentElement)
}

interface WatchController {
  refresh(force?: boolean): ResolvedTheme;         // применить или вернуть последний снимок
  setTheme(theme: ThemeName): void;                // переключить тему и применить
  background(): string;                            // последний входной reference hex
  stop(): void;                                    // отключить наблюдателей
}
```

Для поверхности над изображением, градиентом или размытым фоном — где helper не
видит поле — передайте явный reference-образец `background` (hex-строку или
функцию, возвращающую hex). Один образец не является доказательством всего поля.
Явные `refresh()` и `setTheme()` бросают ошибку синхронно. Ошибка автоматического
обновления один раз передаётся в `onError`; без обработчика она отправляется в
стандартный канал ошибок среды через `reportError` или исключение микрозадачи.
При ошибке resolve последний успешный снимок остаётся применённым. Сам CSSOM не
поддерживает атомарный откат: исключение во время записи может оставить inline-style
частичным. Контроллер помечает его незавершённым, а следующий `refresh()` или
наблюдаемая мутация повторно применяет полный канонический снимок, не вызывая
resolver при неизменных входах.

---

### `adaptTheme(element, options): AdaptController`

Режим с dwell-фильтром: каждый `tick` читает конечный объявленный набор образцов,
но пропускает metric recheck, когда набор и pending state не изменились. Новый
resolve и переход запускаются только при устойчивом относительном снижении. Это
один порог с выдержкой, не
двухпороговый гистерезис. Результат относится только к этим образцам и не
является универсальной гарантией читаемости. Применяет немедленно и возвращает
контроллер.

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
  strict?: boolean;                  // legacy characterized clamp; не universal floor certificate (по умолчанию false)
  reducedMotion?: boolean;           // переопределить системную настройку
}

interface AdaptController {
  tick(now?: number): void;          // один шаг (или использовать start())
  setTheme(theme: ThemeName): void;  // переключить тему мгновенно
  start(): void;                     // запустить внутренний requestAnimationFrame-цикл
  stop(): void;                      // остановить цикл
  current(): Record<string, string>; // logical targets; во время ease не painted DOM
}
```

`strict` сохраняет прежнее runtime-поведение, но не является доказательством
минимального или проходящего состояния на каждом кадре: путь
Oklab→gamut clip→sRGB8 немонотонен. Включайте его только явно для воспроизведения
этого legacy clamp, а не как режим корректности или читаемости.

Управляйте через `start()` (внутренний rAF-цикл) или вызывайте `tick()` из собственного цикла. Смена темы применяется мгновенно — это осознанное намерение, а не дрейф.
Перед применением результата контроллер проверяет provenance как stable-, так и
legacy-ветвей Glow. Нетипизированный внешний mock с невозможным сочетанием
profile/guarantee/diagnostic/status отклоняется `TypeError`; fallback-цвет из
такого объекта не применяется. Resolve по нескольким образцам, проверка
Glow-свидетельств и подготовка перехода образуют одну транзакцию: ошибка до
фазы записи в DOM сохраняет
предыдущие CSS-переменные и `current()` без промежуточного кандидата.
Это обещание относится к ошибкам resolve, recheck и проверки свидетельств до
фазы записи. Исключение самой CSSOM-среды во время последовательных `setProperty`
нельзя откатить атомарно через inline-style API. В таком случае контроллер
забывает базу дифференциальной записи, а следующий явный `tick()` повторяет полный
канонический снимок. Это восстановление, а не атомарный откат уже записанных свойств.

---

### `effectiveBackground(element, options?): string`

Возвращает непрозрачную опорную оценку `#RRGGBB` для поддерживаемой цепочки
сплошных и полупрозрачных DOM `background-color`. Это не browser pixel capture
и не сертификат цвета, который реально видит наблюдатель. Helper обходит цепочку
предков и композитит распознанные слои поверх `fallback` (по умолчанию белый).

```ts
const bg = effectiveBackground(panel);                        // например "#0F1014"
const bg2 = effectiveBackground(panel, { fallback: "#101012" });
```

**Честное ограничение:** работает только с поддерживаемыми сплошными и
полупрозрачными `background-color`; неподдерживаемый CSS, неполная прозрачная
цепочка, `background-image`, градиент, blur и video не дают полного наблюдения.
Текущий compatibility helper ещё может отбросить неподдерживаемый слой или
использовать fallback. Если такой контент влияет на решение, интеграция должна
передать собственный конечный набор образцов в `adaptTheme`; он расширяет только
набор проверенных точек и не превращается в наблюдение всего поля.

Дополнительно экспортируются вспомогательные функции для работы со слоями:
`parseCssColor`, `compositeOver`, `compositeStackToHex`, `toHex` и `oklabLerp`
(линейная интерполяция координат Oklab между двумя цветами с последующим
преобразованием в непрозрачный `#RRGGBB`). Alpha входов при этом отбрасывается,
а точность endpoints относится только к непрозрачным RGB-байтам.

---

## Размер бандла

Raw-размер WASM — hard gate с append-only историей. Текущий
`bench/wasm-size-budget-v9.json` содержит exact Linux-x64 size-бюджеты с нулевым
headroom для `runtime` и `compiler`; checker выбирает текущую версию, а все
предыдущие versioned-файлы остаются неизменяемой историей. Size policy не
притворяется идентификатором артефакта: фактический SHA каждой роли вместе с
source SHA записывается в `build-metadata.json` и повторно сверяется с точными
байтами tarball при публикации. Release-equivalent CI требует точного размера
обеих ролей и неизменных рецептов сборки. CI собирает роли, выполняет
`cargo clean`, повторяет сборку и сравнивает результаты внутри одного Linux job
с закреплённым toolchain; это проверка повторяемости в данном job, а не
утверждение о cross-run или cross-host reproducibility. На других host-платформах
checker сообщает только raw/gzip/SHA-диагностику и не выдаёт локальные байты за
канонический release artifact.

Runtime и offline compiler поставляются двумя независимо загружаемыми
`.wasm`-ассетами. Будет ли runtime-загрузка критическим путём первого рендера,
определяет интеграция: до первого `resolveTheme` инициализация обязана
завершиться; compiler можно не загружать в пользовательской сессии. JS-хелперы
(`applyTheme`, `watchTheme`, `adaptTheme`, `effectiveBackground`) имеют
именованные экспорты и допускают tree-shaking, но их размер также следует мерить
сборкой, а не описывать приблизительно.

Release packer закреплён на Node 24.14/npm 11.9; это не consumer floor.
Публичный контракт `Node >=22.11.0` независимо прогоняется отдельным CI job. Browser-
матрица использует прямую передачу wasm-байтов в `init({ module_or_path })` и
headless Chrome из CI. ESM/URL-обёртка генерируется wasm-pack, но совместимость
с конкретной версией Vite, webpack, Next или другим браузером не заявляется без
отдельного smoke-гейта.

### Для аудиторов цепочки поставки

- **Build metadata:** экспорт `@labpics/colors/build-metadata.json` декларирует
  source SHA, conformance pack и обе WASM-роли; verifier отклоняет любое лишнее,
  отсутствующее или несовпадающее поле и перечитывает metadata из tarball.
  Объект не подписан и не заменяет отключённую npm/Sigstore provenance-аттестацию.
- **Network access (Socket и др.):** оба generated loader-а умеют получить свой
  `.wasm` через `fetch`, только когда интеграция передала URL или использовала
  browser default. В пакете нет внешнего endpoint, отправки данных или исполнения
  при импорте; Node-smoke передаёт локальные байты.
- **Bundlephobia `BuildError`:** их webpack-конвейер не умеет `.wasm`-ассеты
  («loader customization needed»). Канонические размеры проверяет CI-gate
  `enforce measured WASM role budgets`; gzip публикуется только как диагностика.
- **Zero runtime JS-dependencies:** npm-поле `dependencies` пусто — транзитивной JS/npm-цепочки поставки нет. Rust-крейты сборки (`serde`, `serde_json` и др.) компилируются ВНУТРЬ `.wasm` (учтены CI-замером); их цепочка аудируется на стороне сборки — `cargo audit` (RustSec) в CI lab-colors.
