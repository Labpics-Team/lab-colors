# Lab Colors

Контекстный компилятор цветовых токенов для дизайн-систем.

Lab Colors принимает конфиг клиента, компилирует его в `NamedRoleTable` и решает всю таблицу для одного локального фона и темы. Зависимые роли, уже представленные специальными рецептами, используют фактически полученные композиты. Браузерные помощники применяют результат, перепроверяют его при изменении окружения и при необходимости запускают новый resolve.

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
- **Вложенные зависимости через generic-компонент.** Например, `PairLabel` решается против объявленной derived-поверхности (точный source-over композит своего тинта над локальным фоном), которую собирает приватный appearance-граф; поверхность не является эмитированным `PairFill`, и роль не решается против фона страницы.
- **Несколько видов результата.** Роль может вернуть solid, translucent, material, точечный glow, явное отсутствие значения или недостижимость.
- **Версионированные численные свидетельства.** Exact encoded-sRGB8
  source-over/screen операции несут проверяемый профиль и `bit-exact`
  сертификат; Glow требует явный decision profile и может завершиться
  типизированным `Indeterminate` без CSS fallback.
- **Полная проверка конечного домена.** В опубликованном V1 клиент объявляет
  opaque occurrence relations, точные соседние sRGB8-цвета и критерии WCAG 2.2,
  а домен фиксирован зарегистрированной `srgb8-neutral-axis-v1`. Core полностью
  перечисляет его и возвращает packed feasible partition с доказательством. В
  npm это offline-операция `@labpics/colors/compiler` с отдельным WASM, а не
  часть browser runtime; явный клиентский домен пока остаётся Core-only. Это
  проверка выполнимости, а не скрытая политика выбора.
- **Непрерывные семейства.** `ColorCurve` и реализации `NeutralCurve`/`AccentCurve` доступны как низкоуровневые вычислительные примитивы.
- **Браузерное применение.** `applyTheme`, `watchTheme`, `adaptTheme` и `effectiveBackground` связывают результат WASM с локальной областью DOM.

## Что не следует приписывать текущей реализации

- Произвольный generic dependency graph и совместный SCC-solver ещё не являются публичным API. Сейчас есть resolve всей таблицы и отдельные специализированные пути зависимостей.
- Legacy-решения на основе CAM16, CAM16-UCS, Oklab и LPC не доказаны как
  битово-точные на всех средах выполнения и платформах. Exact-гарантия
  распространяется только на явно зарегистрированные конечные операции.
- Обход DOM не является измерением фактически нарисованных пикселей браузера.
- Точечные Glow и Material не являются сертификатом blur, пространственного поля, HDR или физического дисплея.
- Автоматический выбор человечески «лучшего», «чистого», «похожего на бренд» или культурно правильного цвета не является частью обязательного базового resolve.
- P3, HDR, индивидуальное восприятие и неизвестное вмешательство user agent нельзя молча сводить к sRGB.

## Быстрый старт в браузере

Установите пакет:

```sh
npm install @labpics/colors
```

Загрузите конфиг своей дизайн-системы и примените результат:

```ts
import init, { LabColors, applyTheme } from "@labpics/colors";

await init();

const response = await fetch("/theme.config.json");
if (!response.ok) {
  throw new Error(`Не удалось загрузить конфиг: ${response.status}`);
}

const colors = new LabColors();
colors.loadConfig(await response.text());

const result = colors.resolveTheme("#FFFFFF", "light");
applyTheme(document.documentElement, result);
```

Имена тем должны поддерживаться загруженным конфигом и текущей платформенной границей. Имена CSS-переменных определяются клиентской схемой.

Для локального элемента:

```ts
import init, { LabColors, watchTheme } from "@labpics/colors";

await init();

const response = await fetch("/theme.config.json");
if (!response.ok) {
  throw new Error(`Не удалось загрузить конфиг: ${response.status}`);
}

const colors = new LabColors();
colors.loadConfig(await response.text());

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
- семантическими категориями;
- названиями состояний компонентов;
- алиасами;
- темами и режимами;
- тем, какие роли связаны между собой;
- нецветовыми признаками смысла.

Core владеет:

- цветовыми пространствами и поддерживаемыми выходными доменами;
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
| `AlphaAnalog` | точечный композит заданной solid-цели | воспринимаемую глубину |
| `PairFill` | сдвинутую до победы лейбл-стороны солид-заливку | подложку `PairLabel` |
| `PairLabel` | foreground против derived тинт-поверхности (generic source-over компонент) | публичный произвольный graph API |
| `Material` | объявленную многослойную точечную композицию | blur, преломление или физическое стекло |
| `Glow` | рецепт точечного эффекта | пространственный glow после blur |
| `Zero` | явное отсутствие цветового значения | пропущенный ключ |

## Численные гарантии

Работающая функция и сила доказательства — разные вещи.

### Виды доказательства

| Вид | Что означает |
|---|---|
| **Reference exact** | результат точен только для зафиксированной арифметики и эталонного профиля Lab Colors |
| **Browser observed** | пиксель подтверждён снимком конкретного браузера и renderer-контекста |
| **Display measured** | результат подтверждён измерительной сессией на физическом дисплее |

CSS-строка, CSSOM и обход DOM сами по себе не повышают результат до `Browser observed` или `Display measured`.

### Матрица возможностей

| Класс | Текущий смысл |
|---|---|
| **Поддерживаемый точечный путь sRGB** | encoded sRGB input/output, клиентский конфиг, resolve всей таблицы и специализированные рецепты в заявленной версии пакета |
| **Exact encoded-sRGB8 операции** | конечные source-over/screen композиторы, выбранная binary64 alpha и её канонический CSS round-trip; сертификат относится к point-reference, не к renderer/display |
| **Stable Glow decision** | exact point-no-op даёт `Determinate` с sealed `bit-exact` evidence без CAM16-профиля; нетривиальный target/max без sound bound даёт typed `Indeterminate`, не platform-selected fallback |
| **Explicit compatibility (legacy)** | явный legacy execution mode даёт атомарный `Compatibility`-результат с registered release (`glow-cam16-ucs-jprime-target-or-max-v1`) и provenance-классом `legacy-platform-dependent-v1`; это НЕ determinate: результат идентифицирует воспроизводимый АЛГОРИТМ, а не cross-runtime bit-exact значение |
| **Унаследованное платформенно охарактеризованное поведение** | target-driven CAM16/CAM16-UCS/Oklab/LPC, neutral/accent/sentiment policies и связанные поиски по `f64`; они поддерживают совместимость, но не дают cross-runtime bit-exact guarantee |
| **Точность в отдельном эталонном профиле** | только операции, для которых конкретный release объявляет эталонный профиль и проверяемый конечный контракт |
| **Явно не поддержано как стабильная гарантия** | Display-P3 solving, HDR/PQ/HLG, пространственное поле Glow/Material, индивидуальное восприятие, неизвестный browser/display pipeline |

Машинно читаемый capability manifest численных sites — typed-проекция core
registry с независимо пересчитываемым drift-checksum — входит в conformance
manifest. Он описывает возможности сборки и не повышает незарегистрированный
или explicit `Compatibility`-результат до determinate только потому, что тесты
на одной платформе зелёные. До появления внешних клиентов capability-контракт
исправлен атомарно: единственный `numericalCapabilityManifest()` возвращает
proof-capable schema V2; промежуточный public V1 и второй V2-entrypoint удалены.

## Источники и производные значения

Точные anchors и literals являются входными данными клиента. Solver не должен незаметно использовать их как свободные переменные.

Производный цвет может зависеть от:

- текущего локального фона;
- темы;
- выходного профиля;
- специализированного зависимого рецепта;
- явно выбранного клиентского профиля или профиля совместимости.

Полная модель происхождения данных развивается отдельно. Отсутствие provenance metadata в старом результате нельзя компенсировать догадкой по имени роли.

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
- Непрерывный результат не доказывает оптимальность на конечном выходном домене.
- Текущие gamma, chroma и hue policies являются политиками совместимости или продукта, пока более сильный статус не доказан.
- Стабильный конечный runtime в целевой архитектуре использует заранее скомпилированный набор состояний, а не семантическое ветвление по произвольному `ColorCurve::at(f64)`.

## Контраст и доступность

Нормативный контраст и экспериментальные метрики разделены.

- Нормативный floor применяется только там, где его требует контракт клиента или компонента.
- Core не определяет размер текста, essentialness, disabled/decorative status по имени роли.
- Экспериментальный LPC/APCA-shaped или appearance-результат не меняет WCAG pass/fail.
- Для финальной пары sRGB8 новый `wcag22-srgb8-contrast-v1` принимает явно объявленный критерий и возвращает строгий `Pass | Fail`; профиль, Q55-артефакт и full-domain proof входят в релиз.
- Старое поле `wcagRatio` остаётся compatibility-диагностикой текущего resolver/runtime и не может автоматически рекламироваться как результат нового evaluator-а.
- Цвет не должен быть единственным носителем смысла; текст, иконка и форма принадлежат компоненту.

## Runtime и фон

### `effectiveBackground`

Текущая функция — вспомогательная эталонная оценка для поддерживаемой цепочки CSS-цветов, а не доказательство того, что пользователь видит именно этот пиксель.

Критические ограничения:

- image, gradient, video, blend mode, filter и backdrop-filter требуют явных образцов или снимка;
- неизвестный или неподдерживаемый CSS нельзя считать прозрачным слоем;
- отсутствие непрозрачной базы, предел обхода и ошибка style API должны рассматриваться как неизвестный контекст, а не как белый или чёрный fallback;
- эмитированные движком значения предпочтительно передавать байтами, а не повторно декодировать из CSS-строки.

Текущий helper совместимости ещё не реализует весь строгий типизированный контракт наблюдения. Его bare hex нельзя использовать как сертификат браузера или дисплея.

### `watchTheme`

Обслуживает изменения, которые видит текущий DOM-adapter. `MutationObserver` не гарантирует обнаружение любого изменения computed style, media environment или layout.

### `adaptTheme`

Текущий контроллер является охарактеризованным механизмом совместимости. Он
валидирует stable Glow evidence и селективно перерешивает роль при переходе
между exact-no-op и `Indeterminate`, не ломая уже идущую цветовую анимацию. Это
не универсальная гарантия нормативного floor на каждом промежуточном кадре и
на всех возможных образцах.

Нормативный целевой контракт строже:

```text
известный набор фонов
→ решить candidate
→ проверить конечные emitted values на каждом обязательном образце
→ только затем записать
```

Нормативное нарушение не должно ожидать эстетические `sustain`/`dwell`. Анимация нормативного перехода допустима только при независимой проверке каждого реально записываемого кадра. `painted state` и `target state` — разные понятия. При `prefers-reduced-motion: reduce` промежуточная необязательная анимация должна отсутствовать, а не заменяться недоказанно «лучшим» fade.

## Темы и platform overrides

- Точный anchor конкретной темы имеет приоритет.
- Текущий адаптер может поддерживать более узкий список theme IDs, чем клиент-агностичная архитектура core.
- Product increased-contrast theme, `prefers-contrast` и forced colors — разные сущности.
- Авторское значение токена не является сертификатом фактического used color после неизвестного UA override.
- Forced Colors может удалить shadow/background-image; точечный результат Glow/Material тогда может не быть нарисован вовсе.

## Optional Screen ColorQuality

`Preserve / Audit / Project` — зафиксированная архитектурная политика отдельного необязательного слоя, а не обещание, что observer-backed projector уже доступен в stable package.

- `Preserve` не выполняет дополнительную семантическую мутацию.
- `Audit` добавляет анализ, не меняя candidate.
- `Project` допустим только с допущенной моделью или профилем, `NoChange`, семантикой неопределённости и сертификатом конечного результата.

Базовый context-dependent resolve от этого слоя не зависит.

## Документация и разработка

- [Browser/WASM API](packages/colors/README.md)
- [ADR: конфиг-граница](docs/decisions/0001-config-boundary.md)
- [Научный whitepaper](docs/whitepaper.md)
- [Реестр коэффициентов и policies](docs/empirical-inventory.md)
- [Правила именования и конформанса](docs/NAMING.md)
- [Conformance и numerical registry](conformance/README.md)
- [Миграция exact alpha / typed Glow](docs/migrations/exact-alpha-glow.md)
- [Changelog](CHANGELOG.md)

Агент без контекста читает:

```text
AGENTS.md
→ Issue #276
→ Issue #228
→ Issue #248
→ актуальный main
→ активный PR
→ Owner Issue текущего correctness-root
```

Roadmap, текущий SHA и активный PR хранятся только в Issues.

## Структура репозитория

```text
crates/
├── labcolors-core          — математика, конфиг, resolve и результаты
├── labcolors-protocol      — единая versioned bytes→Core→wire граница
├── labcolors-wasm          — WASM-граница
├── labcolors-ffi           — нативная FFI-граница
└── labcolors-conformance   — общие тест-векторы

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
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

Изменения browser/WASM/public wire contract дополнительно требуют сборки пакета, consumer-smoke, JS/browser tests и platform conformance. Обязательные gates определяются workflow-файлами и acceptance конкретного Issue.

## Главный принцип

```text
Дизайн-система задаёт язык, источники и намерения.
Lab Colors компилирует их в проверяемые цветовые контракты.
Runtime поддерживает эти контракты в пределах явно заявленных возможностей.
```
