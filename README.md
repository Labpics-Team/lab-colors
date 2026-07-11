# Lab Colors

Контекстный компилятор цветовых токенов для дизайн-систем.

Lab Colors принимает конфиг клиента, компилирует его в `NamedRoleTable` и решает всю таблицу для одного локального фона и темы. Зависимые роли, которые уже представлены специальными рецептами, используют фактически полученные композиты. Браузерные помощники применяют результат, перепроверяют его при изменении окружения и при необходимости запускают новый resolve.

```text
ThemeConfig клиента
→ проверка и компиляция
→ NamedRoleTable

NamedRoleTable
+ локальный фон
+ тема
→ resolve всей таблицы
→ значения и статусы ролей

изменение окружения
→ recheck
→ сохранить текущий результат, если он допустим
→ выполнить новый resolve, если это требуется
```

## Что уже работает

- **Клиентский словарь.** Имена ролей, тем и алиасов приходят из `ThemeConfig`. Core не выводит смысл из слов `primary`, `danger`, `hover` или имени компонента.
- **Контекстный resolve.** Одна таблица решается заново для переданного локального фона и темы.
- **Специализированные зависимости.** Например, `PairLabel` решается против эмитированного композита своей `PairFill`, а не против фона страницы.
- **Несколько видов результата.** Роль может вернуть solid, translucent, material, point-glow, явное отсутствие значения или недостижимость.
- **Непрерывные семейства.** `ColorCurve` и реализации `NeutralCurve`/`AccentCurve` доступны как низкоуровневые вычислительные примитивы.
- **Браузерное применение.** `applyTheme`, `watchTheme`, `adaptTheme` и `effectiveBackground` связывают результат WASM с локальным DOM-scope.

## Что не следует приписывать текущей реализации

- Произвольный generic dependency graph и совместный SCC-solver ещё не являются публичным API. Сейчас есть whole-table resolve и отдельные специализированные пути зависимостей.
- Текущие CAM16, CAM16-UCS, Oklab и LPC-решения не доказаны как битово-точные на всех runtime и платформах.
- DOM traversal не является измерением фактически нарисованных пикселей браузера.
- Point-glow и point-material не являются сертификатом blur, spatial field, HDR или физического дисплея.
- Автоматический выбор человечески «лучшего», «чистого», «похожего на бренд» или культурно правильного цвета не является частью обязательного базового resolve.
- P3, HDR, individual-observer appearance и неизвестный user-agent override нельзя молча сводить к sRGB.

## Быстрый старт

Установите пакет:

```sh
npm install @labpics/colors
```

Загрузите конфиг своей дизайн-системы и примените результат:

```ts
import init, { LabColors, applyTheme } from "@labpics/colors";
import themeConfig from "./theme.config.json";

await init();

const colors = new LabColors();
colors.loadConfig(JSON.stringify(themeConfig));

const result = colors.resolveTheme("#FFFFFF", "light");
applyTheme(document.documentElement, result);
```

Имена тем должны поддерживаться загруженным конфигом и текущей platform-границей. Имена CSS-переменных определяются клиентской схемой.

Для локального элемента:

```ts
import init, { LabColors, watchTheme } from "@labpics/colors";
import themeConfig from "./theme.config.json";

await init();

const colors = new LabColors();
colors.loadConfig(JSON.stringify(themeConfig));

const panel = document.querySelector(".panel");
if (!(panel instanceof HTMLElement)) {
  throw new Error("Элемент .panel не найден");
}

const watcher = watchTheme(panel, {
  colors,
  theme: "light",
});

watcher.setTheme("dark");
watcher.refresh();
watcher.stop();
```

TypeScript-путь `loadConfig → resolveTheme → applyTheme/watchTheme` проверяется consumer-smoke тестом пакета. Подробный API: [`packages/colors/README.md`](packages/colors/README.md).

## Граница клиентской семантики

Клиент владеет:

- именами токенов;
- названиями уровней;
- semantic categories;
- component state names;
- aliases;
- темами и modes;
- тем, какие роли связаны между собой;
- non-color cues.

Core владеет:

- цветовыми пространствами и поддерживаемыми output domains;
- построением производных цветов;
- контрастом и композитингом;
- конечной эмиссией;
- специализированными физическими контрактами;
- результатами и диагностикой.

Одинаковое клиентское слово не обязано означать одинаковую числовую координату. Например, `text.secondary` и `border.secondary` могут принадлежать одному клиентскому уровню, но решать разные физические задачи и иметь разные `Lc`, `J′`, alpha и hex.

## Текущие рецепты

| Рецепт | Для чего используется | Чего не доказывает |
|---|---|---|
| `TextAnchor` | foreground относительно текущего фона | универсальную силу любого стимула |
| `DjAnchor` | заданный шаг appearance-коррелята | читаемость или WCAG |
| `DecorativeLc` | декоративную контрастную величину | нормативную доступность текста |
| `Ladder` | клиентский preset тинта и alpha | общий закон уровней core |
| `AlphaAnalog` | point-композит заданной solid-цели | воспринимаемую глубину |
| `PairFill` / `PairLabel` | вложенную пару fill → composite → label | произвольный generic graph |
| `Material` | объявленную многослойную point-композицию | blur, refraction или physical glass |
| `Glow` | point-effect recipe | spatial glow после blur |
| `Zero` | явное отсутствие цветового значения | пропущенный ключ |

## Численные гарантии

Работающая функция и сила доказательства — разные вещи.

### Виды доказательства

| Вид | Что означает |
|---|---|
| **Reference exact** | результат точен только для зафиксированной арифметики и reference-profile Lab Colors |
| **Browser observed** | пиксель подтверждён конкретным browser/renderer capture |
| **Display measured** | результат подтверждён измерительной сессией на физическом дисплее |

CSS-строка, CSSOM и DOM traversal сами по себе не повышают результат до `Browser observed` или `Display measured`.

### Матрица capability

| Класс | Текущий смысл |
|---|---|
| **Supported point sRGB path** | encoded sRGB input/output, client config, whole-table resolve и специализированные recipes в заявленной версии пакета |
| **Legacy platform-characterized** | target-driven CAM16/CAM16-UCS/Oklab/LPC, neutral/accent/sentiment policies и связанные float-search решения; они работают как compatibility behavior, но не являются cross-runtime bit-exact guarantee |
| **Capability-specific reference exact** | только операции, для которых конкретный release возвращает/документирует reference profile и проверяемый конечный контракт |
| **Explicitly unsupported as stable guarantee** | Display-P3 solving, HDR/PQ/HLG, spatial glow/material field, individual-observer appearance, неизвестный browser/display pipeline |

До появления machine-readable numerical profile нельзя повышать legacy float-result до `BitExact`, `ProvenOptimal` или `ProvenInfeasible` только потому, что тесты на одной платформе зелёные.

## Источники и производные значения

Exact anchors и literals являются входными данными клиента. Solver не должен незаметно использовать их как свободные переменные.

Производный цвет может зависеть от:

- текущего локального фона;
- темы;
- output profile;
- специализированного зависимого recipe;
- явно выбранного client/compatibility profile.

Полная provenance-модель развивается отдельно. Отсутствие provenance metadata в старом результате нельзя компенсировать догадкой по имени роли.

## Непрерывные семейства и конечный output

```text
anchors
→ versioned construction
→ continuous family
→ выбор состояния по контракту
→ output mapping / quantization
→ final emitted state
→ повторная проверка
```

- `t` не является встроенным словарём `Primary / Secondary / ...`.
- Одинаковый клиентский уровень разных семейств не обязан иметь одинаковый `t`.
- Непрерывный результат не доказывает конечную output-оптимальность.
- Текущие gamma, chroma и hue policies являются compatibility/product policies, пока более сильный статус не доказан.
- Stable finite runtime в целевой архитектуре использует заранее скомпилированный конечный набор состояний, а не семантическое ветвление по произвольному `ColorCurve::at(f64)`.

## Контраст и доступность

Нормативный контраст и экспериментальные метрики разделены.

- Нормативный floor применяется только там, где его требует клиентский/component contract.
- Core не определяет размер текста, essentialness, disabled/decorative status по имени роли.
- Экспериментальный LPC/APCA-shaped или appearance-результат не меняет WCAG pass/fail.
- До миграции на единый versioned WCAG 2.2 profile старое поле `wcagRatio` нельзя автоматически рекламировать как `Wcag22`.
- Цвет не должен быть единственным носителем смысла; текст, иконка и форма принадлежат компоненту.

## Runtime и фон

### `effectiveBackground`

Текущая функция — convenience/reference estimate для поддерживаемой цепочки CSS-цветов, а не доказательство того, что пользователь видит именно этот пиксель.

Критические ограничения:

- image, gradient, video, blend mode, filter и backdrop-filter требуют явных samples или capture;
- неизвестный/неподдерживаемый CSS нельзя считать прозрачным слоем;
- отсутствие непрозрачной базы, предел обхода и ошибка style API должны рассматриваться как неизвестный контекст, а не как белый/чёрный fallback;
- engine-owned output предпочтительно передавать байтами, а не повторно декодировать из CSS-строки.

Текущий legacy helper ещё не реализует весь строгий typed-observation контракт. Его bare hex нельзя использовать как browser/display certificate.

### `watchTheme`

Обслуживает изменения, которые видит текущий DOM adapter. `MutationObserver` не гарантирует обнаружение любого изменения computed style, media environment или layout.

### `adaptTheme`

Текущий контроллер является совместимым legacy runtime-механизмом. Он не должен описываться как уже доказанная гарантия нормативного floor на каждом промежуточном кадре и на всех samples.

Нормативный целевой контракт строже:

```text
известный набор фонов
→ решить candidate
→ проверить конечные emitted values на каждом обязательном sample
→ только затем записать
```

Нормативное нарушение не должно ожидать эстетические `sustain`/`dwell`. Анимация нормативного перехода допустима только при независимой проверке каждого реально записываемого кадра. `painted state` и `target state` — разные понятия. При `prefers-reduced-motion: reduce` промежуточная необязательная анимация должна отсутствовать, а не заменяться недоказанно «лучшим» fade.

## Темы и platform overrides

- Точный theme-specific anchor имеет приоритет.
- Current adapter может поддерживать более узкий список theme IDs, чем client-agnostic core architecture.
- Product increased-contrast theme, `prefers-contrast` и forced colors — разные сущности.
- Authored token value не является сертификатом фактического used color после неизвестного UA override.
- Forced Colors может удалить shadow/background-image; point Glow/Material result тогда может не быть нарисован вовсе.

## Optional Screen ColorQuality

`Preserve / Audit / Project` — зафиксированная архитектурная политика отдельного optional layer, а не обещание, что observer-backed projector уже доступен в stable package.

- `Preserve` не выполняет optional semantic mutation.
- `Audit` добавляет анализ, не меняя candidate.
- `Project` допустим только с admitted model/profile, `NoChange`, uncertainty и final-state certificate.

Базовый context-dependent resolve от этого слоя не зависит.

## Документация и разработка

- [Browser/WASM API](packages/colors/README.md)
- [ADR: конфиг-граница](docs/decisions/0001-config-boundary.md)
- [Научный whitepaper](docs/whitepaper.md)
- [Реестр коэффициентов и policies](docs/empirical-inventory.md)
- [Правила именования и конформанса](docs/NAMING.md)

Агент без контекста читает:

```text
AGENTS.md
→ Issue #276
→ Issue #228
→ Issue #248
→ Issue #281
→ активный Issue / PR
→ актуальный main
```

Roadmap, текущий SHA и активный PR хранятся только в Issues.

## Структура репозитория

```text
crates/
├── labcolors-core          — математика, конфиг, resolve и результаты
├── labcolors-wasm          — WASM-граница
├── labcolors-ffi           — нативная FFI-граница
├── labcolors-conformance   — общие тест-векторы
└── labcolors-preview       — вспомогательный рендер

packages/
└── colors                  — browser package и runtime helpers

docs/
├── decisions/
├── empirical-inventory.md
├── empirical-residue.md
└── whitepaper.md
```

## Зависимости

`labcolors-core` имеет ноль рантайм-зависимостей. Проверяемый контракт:

```sh
cargo tree -p labcolors-core --edges=no-dev
```

Команда должна вывести только `labcolors-core` без дочерних runtime-пакетов. Dev-зависимости тестов и benchmark-ов в этот контракт не входят.

## Проверки

Основной локальный набор:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Изменения browser/WASM/public wire contract дополнительно требуют сборки пакета, consumer-smoke, JS/browser tests и platform conformance. Обязательные gates определяются workflow-файлами и acceptance конкретного Issue.

## Главный принцип

```text
Дизайн-система задаёт язык, источники и намерения.
Lab Colors компилирует их в проверяемые цветовые контракты.
Runtime поддерживает эти контракты в пределах явно заявленных capabilities.
```
