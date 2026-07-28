# @labpics/colors

Агностичный контраст-движок для дизайн-систем. Получает фоновый цвет и тему — возвращает полный набор цветовых ролей **вашей** системы. Словарь ролей не встроен в пакет: его задаёт конфиг дизайн-системы (`ThemeConfig`), загружаемый через `loadConfig`; имена вида `--lab-label-primary`, `--lab-border-base` в примерах ниже — из конфига дизайн-системы labui. CSS-переменные несут готовое значение `oklch(L% C H)` (для полупрозрачных ролей — `oklch(L% C H / A)`); сырой `#RRGGBB` остаётся данными роли (`roles.<ключ>.hex`). Пакет не имеет runtime-зависимостей; runtime resolver — единственная WASM-роль.

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
npm run build   # → pkg/ (runtime WASM)
```

Пакет экспортирует `@labpics/colors/build-metadata.json` — self-declared
machine-readable metadata конкретной сборки: npm/core versions, exact source
SHA, digest и SHA-256 conformance manifest/family set, а также размер и SHA-256
`runtime` WASM-артефакта. Release-gate сверяет schema 2
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

Параллельные `init()` разделяют одну загрузку; повторный вызов после успеха —
no-op. `initSync()` нельзя смешивать с ещё не завершившимся `init()`: фасад
отклонит гонку до создания второго WASM-инстанса. `init()` разрешается в
`void`, `initSync()` возвращает `void`; raw WebAssembly exports они не раскрывают.

### Реактивное отслеживание

`watchTheme` синхронизирует переменные с явно переданным фоном или опорной
оценкой поддерживаемой цепочки `background-color`. Каждый поддерживаемый слой
этой цепочки проходит тот же exact encoded-sRGB8 point-композитор Core, что и
occurrence-граф; отдельной JS-формулы и публичного compositor API нет.

```ts
import init, { LabColors, watchTheme } from "@labpics/colors";
import dsConfig from "./theme.config.json";

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
import init, { LabColors, adaptTheme } from "@labpics/colors";
import dsConfig from "./theme.config.json";

await init();
const colors = new LabColors();
colors.loadConfig(JSON.stringify(dsConfig));   // конфиг дизайн-системы (см. квик-старт)

const surface = document.querySelector(".hero") as HTMLElement;
let samples = ["#101012"];
const adaptive = adaptTheme(surface, {
  colors,
  theme: "light",
  background: () => samples,
});

adaptive.start();           // запустить внутренний requestAnimationFrame-цикл
samples = ["#101012", "#202024"]; // интеграция обновила конечные образцы подложки
adaptive.tick();            // явно обработать новое наблюдение
adaptive.setTheme("dark");  // смена темы применяется мгновенно
adaptive.stop();            // остановить цикл
```

Для градиента, изображения или видео интеграция может передать конечный набор образцов,
полученных самостоятельно. Контроллер проверяет только переданные точки: он не
наблюдает всё поле и не переносит результат на промежутки между образцами.

### Инициализация в Node

Node получает локальные WASM-байты явно; нулевая форма `init()` предназначена
для браузерного loader-а.

```ts
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import initRuntime from "@labpics/colors";

const require = createRequire(import.meta.url);
const runtimeWasm = await readFile(
  require.resolve("@labpics/colors/pkg/labcolors_bg.wasm"),
);

await initRuntime({ module_or_path: runtimeWasm });
```

---

## Темы

Словарь тем принадлежит конфигу: `themes` объявляет пары «клиентское имя →
физический VC-пресет», и `resolveTheme`/`recheckContrast` принимают ИМЕННО эти
имена (ключ вне словаря — ошибка `unknown_theme`; встроенных имён у движка
нет). Физических пресетов четыре:

| VC-пресет | Условия просмотра |
|-----------|-------------------|
| `"srgb"` | светлое окружение (average surround) |
| `"dim"` | тёмное окружение (dim surround) |
| `"srgb-ic"` | светлое + повышенный контраст |
| `"dim-ic"` | тёмное + повышенный контраст |

Словарь labui-паспорта: `light → srgb`, `dark → dim`, `light-ic → srgb-ic`,
`dark-ic → dim-ic` — поэтому примеры ниже используют `"light"`/`"dark"`.
Несколько имён могут разделять один пресет: физика одинакова, имя в
результате сохраняется клиентское.

---

## Справочник публичного API

### `new LabColors()`

Создаёт движок **без** словаря ролей: до `loadConfig` любой вызов `resolveTheme` отклоняется ошибкой `config_required`. Повторные одинаковые вызовы `resolveTheme` обслуживаются из внутреннего кэша (кэш-пространство привязано к отпечатку конфига).

---

### `engine.resolveTheme(bgHex, theme): ResolvedTheme`

- `bgHex` — фон в формате `#RGB` или `#RRGGBB`.
- `theme` — клиентский ключ из словаря `themes` загруженного конфига.

Возвращает объект `ResolvedTheme`:

```ts
interface ResolvedTheme {
  readonly theme: ThemeName;
  readonly background: string;                                  // нормализованный #RRGGBB
  readonly vars: Readonly<Record<string, string>>;               // "--lab-<ключ>" → oklch
  readonly roles: Readonly<Record<string, RoleResult>>;          // все роли с деталями
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
- `TranslucentRole` — полупрозрачная роль лестницы или альфа-аналога (`kind: "translucent"`): `css` — готовое `oklch(L% C H / A)`, `tintHex` — тинт как данные, `alpha`, плюс `compositeHex` / `compositeLc` / `compositeWcag` — exact encoded-sRGB8 reference-композит, знаковая кандидатная оценка по `Ys` и WCAG ratio на фоне резолва. `compositeLc` не является LPC/readability verdict; конкретный renderer и color-management pipeline проверяются отдельно.
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
  согласованных ветвей: смешать stable/legacy профиль, гарантию, диагностику и
  статус на уровне TypeScript невозможно. Legacy-ветви
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
  `upperAlpha`) и degraded opaque endpoint (`alpha = 1`). `alphaStatus` и
  `alphaGuarantee` согласованы типом. `floor`
  — запрошенный пол; он держится только при `alphaStatus: "satisfied"`. Primary
  остаётся солид-каноном, `-01` — тинтом, `-02` — опаковой базой. Численный
  профиль явно фиксирует operation order:
  `encoded-srgb-byte-scale-affine-platform-binary64-powf-v1`: original WCAG 2.1
  (2018) split `0.03928`, conservative binary64 channel envelope и обе стороны
  пересечённого EOTF seam. Bracket доступен только после directed-search guard;
  это platform-characterization, а не двухугловая теорема, глобальная
  монотонность, первый passing state или точная минимальная alpha.
- `NoneRole` — роль намеренно пустая по дизайну (`kind: "none"`), не ошибка.
- `FailureRole` — типизированный `Unresolved` без выбранного цвета
  (`kind: "failure"`, `category: "unresolved"`): ограниченный поиск завершился,
  но не доказал ни решение, ни недостижимость. `code` уточняет причину.

`Unresolved` остаётся частью успешного результата и не попадает в `vars`.
Доказанный ordinary `Unreachable` означает, что полный снимок не существует:
`resolveTheme` отклоняется структурным `OutputConflictError` до aliases,
проекции и кэша. Его `conflicts` — непустой список
`{ role, code, message }` в порядке объявления ролей; client-owned ID остаются
непрозрачными. Проверяйте `error.name === "OutputConflictError"` и
`error.code === "output_conflict"`: отдельный runtime-конструктор для
`instanceof` не экспортируется. `rejected`, `unsupported` и `internal` также не
являются `RoleResult` и отклоняют весь вызов без частичного `ResolvedTheme`,
`vars` или CSS. После успешного preflight дрейф Core выходит как `internal_error` с
исходной причиной в сообщении. Основные ошибки resolve-пути:

| Код ошибки | Причина |
|------------|---------|
| `config_required` | конфиг ещё не загружен (`loadConfig` не вызывался) |
| `invalid_background` | `bgHex` не является `#RGB` или `#RRGGBB` |
| `unknown_theme` | `theme` не входит в список допустимых |
| `output_conflict` | ordinary-роль доказанно недостижима; полный снимок не создан |
| `internal_error` | Core нарушил собственный инвариант; частичный CSS не возвращается |

---

### `engine.loadConfig(json): string`

Загружает конфиг дизайн-системы (JSON по типу `ThemeConfig`; схема — в репозитории: `docs/decisions/0001-config-boundary.md`, TS-типы — в поставляемом `labcolors.d.ts`). Это **единственный** источник словаря ролей — встроенной таблицы в движке нет: до загрузки конфига `resolveTheme` отклоняется ошибкой `config_required`. Полный preflight проверяет не только имена ролей, но и итоговый CSS-namespace: `-core`/`-alpha` Glow и `-01`/`-02` Material не могут быть затёрты другой ролью или алиасом. Невалидный конфиг отклоняется структурной ошибкой `invalid_config: …` и НЕ меняет состояние. Возвращает детерминированный 64-битный отпечаток как 16 hex-символов; это вероятностный идентификатор, а корректность reload обеспечивается полным очищением кэша.

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
Ys candidate score `lc` и диагностический `wcagRatio` не могут изменить этот
вердикт.

### `numericalCapabilityManifest(): NumericalCapabilityManifestV2`

Возвращает статический манифест численных возможностей установленной сборки:
какие solver-sites зарегистрированы и какие artifact/bound/proof IDs они могут
выдать. Это диагностическая поверхность для tooling, CI и ИИ-агентов; она не
выбирает режим и не превращает compatibility-результат в доказанный. Публичная
функция одна: отдельного V1 или `numericalCapabilityManifestV2()` нет.

---

### `engine.themeHandle(theme): number`

Минтит числовой хэндл темы для горячего update-loop-а. `theme` ищется в словаре
загруженного конфига (`config_required` без конфига, `unknown_theme` для
необъявленного ключа), как и в `resolveTheme`. Строку темы разрешают ОДИН раз на
холодном крае, затем в каждом кадре передают числовой хэндл в `recheckContrast`/
`recheckContrastMulti` — так словарь темы не пересканируется по строке на каждом
тике.

---

### `engine.recheckContrast(bg, fgs, themeHandle): Float64Array`

Дешёвая покадровая проверка: какие Ys candidate score и WCAG ratio дают цвета
`fgs` на фоне `bg` под темой `themeHandle`, без полного резолва. `bg` — упакованное
слово `0x00RRGGBB` (u32, старший байт зарезервирован и обязан быть 0); `fgs` —
`Uint32Array` таких же упакованных слов; `themeHandle` — число из
`engine.themeHandle(theme)`. Один contiguous-копи в линейную память, ноль
hex-parse и строковых аллокаций на update-пути. Возвращает `Float64Array` пар
`[lc, wcagRatio]` в порядке `fgs`: индекс `2·i` — знаковая кандидатная оценка по
`Ys` из frozen SAPC-shaped curve, `2·i+1` — WCAG-отношение. `lc` не является
LPC/readability verdict; runtime использует его только как координату текущего
transitional solver-а.

---

### `engine.recheckContrastMulti(bgs, fgs, themeHandle): Float64Array`

Батч-вариант `recheckContrast` для конечного объявленного набора образцов
меняющегося фона: проверяет один набор `fgs` сразу против нескольких `bgs`,
разделяя прямой ход модели каждого переднего плана между всеми образцами (он от
фона не зависит). Это finite-sample API, не доказательство всего поля градиента,
картинки, blur или стекла. `bgs` и `fgs` — `Uint32Array`
упакованных `0x00RRGGBB` слов, `themeHandle` — число из `engine.themeHandle(theme)`.
Результат байт-в-байт равен N отдельным вызовам `recheckContrast`, пара за парой —
это закреплено parity-тестом границы. Возвращает плоский background-major
`Float64Array`: образец `s`, цвет `i` лежит в `(s · fgs.length + i) · 2` (`lc`) и
`+1` (`wcagRatio`).
`adaptTheme` использует один батч-вызов, только когда все foreground occurrences
не зависят от фона (`opacity = 1`). При `opacity < 1` контроллер сначала заново
композитит source на каждом текущем sample, затем вызывает `recheckContrast` для
этого sample: общий foreground-ряд здесь физически неверен. Эффект на
производительность зависит от host и интеграции и без отдельного
воспроизводимого гейта не заявляется. Для одного образца контроллер также
остаётся на `recheckContrast`.

---


### `applyTheme(element, result): void`

Принимает полный снимок `ResolvedTheme` и до первой CSSOM-операции проверяет его
структуру и отсутствие ordinary `Unreachable`. Затем удаляет прежние inline
`--lab-*` и записывает выбранные значения из `result.vars` через `setProperty`.
В штатном результате `resolveTheme` исходы `none`, `glow-indeterminate` и
`Unresolved` не имеют CSS-значения, поэтому устаревший var не сохраняется.
Ordinary-конфликт либо невалидный контейнер отклоняется до изменения DOM;
передача одного вручную собранного `vars` не поддерживается. Проверка provenance
и соответствия каждого var сертификату принадлежит полному output-контракту,
а не этому DOM helper.

---

### `watchTheme(element, options): WatchController`

Синхронизирует `--lab-*` переменные с явно переданным фоном или опорной оценкой
поддерживаемой `background-color`-цепочки. В штатном пути применяет первый
результат немедленно и возвращает контроллер.

```ts
interface WatchThemeOptions {
  colors: LabColors;
  theme: ThemeName;
  background?: string | (() => string);  // явный фон (если автоматический невозможен)
  target?: HTMLElement;                  // куда писать переменные (по умолчанию: element)
  canvas?: string;                       // явный непрозрачный canvas для прозрачного корня
  observe?: boolean;                     // авто-обновление при style/class в поддереве (по умолчанию true)
  onError?: (error: unknown) => void;     // ошибки автоматического observer-refresh
  root?: Node;                           // корень MutationObserver (по умолчанию: documentElement)
}

interface WatchController {
  refresh(force?: boolean): ResolvedTheme | null;  // null, если первый commit не состоялся
  setTheme(theme: ThemeName): void;                // переключить тему и применить
  background(): string | null;                     // последний закоммиченный reference hex
  stop(): void;                                    // отключить наблюдателей
}
```

Для поверхности над изображением, градиентом или размытым фоном — где helper не
видит поле — передайте явный reference-образец `background` (hex-строку или
функцию, возвращающую hex). Один образец не является доказательством всего поля.
Явный вход обязан быть непустой строкой: невалидное значение отклоняется и не
трактуется как отсутствие `background`. Если поддерживаемая цепочка полностью
прозрачна и `canvas` не объявлен, наблюдение имеет тип `Unknown`; белый цвет не
подставляется.
До захвата `MutationObserver` невалидный вход или initial resolve бросает
синхронно и не создаёт долгоживущий ресурс. После захвата observer функция
сначала возвращает контроллер-владелец. Если затем упал `observe`, первый CSS
commit или его cleanup, отказ асинхронно передаётся в `onError`; без обработчика
он отправляется в стандартный канал ошибок среды через `reportError` или
исключение микрозадачи. Такой контроллер может вернуть `null`, пока не было ни
одного успешного commit, а повторный `stop()` освобождает retained handle после
временного отказа `disconnect()`.
Явные `refresh()` и `setTheme()` после успешного старта бросают синхронно.
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
  canvas?: string;                   // явный непрозрачный canvas для прозрачного корня
  dropFraction?: number;             // запас контраста до пересчёта (по умолчанию 0.2)
  sustainMs?: number;                // минимальное время удержания нарушения (по умолчанию 120)
  dwellMs?: number;                  // минимальный интервал между пересчётами (по умолчанию 250)
  easeMs?: number;                   // длительность перехода (по умолчанию 280; уменьшается при reduced-motion)
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

Объявленный набор `background` обязан быть непустым и содержать только непустые
строки. Невалидный явный образец отклоняется до resolver без coercion и без
подмены выдуманным цветом. Без объявленного `canvas` полностью прозрачная
поддерживаемая цепочка даёт `Unknown` и не запускает resolver.

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

## Размер бандла

Raw-размер WASM — hard gate. SSOT текущего exact Linux-x64 size-бюджета
`runtime` — self-contained `packages/colors/bench/wasm.json`: он закрепляет
полный toolchain/recipe, источник измерения и потолок с нулевым headroom.
Предыдущие состояния восстанавливаются из Git и не дублируются в live tree.
`scripts/check-wasm-size-budget.mjs` закрепляет канонические байты этого
контракта и отклоняет numbered snapshots. Size policy не притворяется
идентификатором артефакта: фактический SHA артефакта вместе с source SHA
записывается в `build-metadata.json` и повторно сверяется с точными байтами
tarball при публикации. Release-equivalent CI требует точного размера и
неизменного рецепта сборки. CI собирает артефакт, выполняет `cargo clean`,
повторяет сборку и сравнивает результаты внутри одного Linux job с закреплённым
toolchain; это проверка повторяемости в данном job, а не утверждение о cross-run
или cross-host reproducibility. На других host-платформах checker сообщает
только raw/gzip/SHA-диагностику и не выдаёт локальные байты за канонический
release artifact.

Будет ли runtime-загрузка критическим путём первого рендера, определяет
интеграция: до первого `resolveTheme` инициализация обязана завершиться.
JS-хелперы (`applyTheme`, `watchTheme`, `adaptTheme`)
имеют именованные экспорты и допускают tree-shaking, но их размер также следует
мерить сборкой, а не описывать приблизительно.

Release packer закреплён на Node 24.14/npm 11.9; это не consumer floor.
Публичный контракт `Node >=22.11.0` независимо прогоняется отдельным CI job. Browser-
матрица использует прямую передачу wasm-байтов в `init({ module_or_path })` и
headless Chrome из CI. ESM/URL-обёртка генерируется wasm-pack, но совместимость
с конкретной версией Vite, webpack, Next или другим браузером не заявляется без
отдельного smoke-гейта.

### Для аудиторов цепочки поставки

- **Build metadata:** экспорт `@labpics/colors/build-metadata.json` декларирует
  source SHA, conformance pack и runtime WASM-артефакт; verifier отклоняет любое
  лишнее, отсутствующее или несовпадающее поле и перечитывает metadata из tarball.
  Объект не подписан и не заменяет отключённую npm/Sigstore provenance-аттестацию.
- **Network access (Socket и др.):** generated loader умеет получить свой
  `.wasm` через `fetch`, только когда интеграция передала URL или использовала
  browser default. В пакете нет внешнего endpoint, отправки данных или исполнения
  при импорте; Node-smoke передаёт локальные байты.
- **Bundlephobia `BuildError`:** их webpack-конвейер не умеет `.wasm`-ассеты
  («loader customization needed»). Канонические размеры проверяет CI-gate
  `enforce measured WASM role budgets`; gzip публикуется только как диагностика.
- **Zero runtime JS-dependencies:** npm-поле `dependencies` пусто — транзитивной JS/npm-цепочки поставки нет. Rust-крейты сборки (`serde`, `serde_json` и др.) компилируются ВНУТРЬ `.wasm` (учтены CI-замером); их цепочка аудируется на стороне сборки — `cargo audit` (RustSec) в CI lab-colors.
