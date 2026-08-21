# Lab Colors

Компилятор канонических Program wire байтов в сертифицированные цветовые выходы. Без рецептов, без темных конфигов, без браузерных хелперов. Единственный корень рантайма: `compileProgramWire` → `ProgramRuntime` → `ProgramSnapshot`.

- Program wire — единственный источник авторства и рантайма.
- Recipe DTO и устаревшие темные хелперы не экспортируются.
- Численные возможности объявлены в proof-capable манифесте, а не выводятся из зелёных тестов.

## Быстрый старт

```ts
import init, { compileProgramWire } from "@labpics/colors";

// 1. Инициализация WASM-модуля
await init();

// 2. Компиляция канонических Program wire байтов
const wireBytes = new Uint8Array([/* ... канонические LCPW v1 байты ... */]);
const runtime = compileProgramWire(wireBytes, 1);

// 3. Передача наблюдаемого сценария и получение сертифицированного снимка
const snapshot = runtime.updateObserved(
  1n,                              // ревизия (bigint)
  new Uint32Array([1]),            // ID сценариев
  new Uint8Array([255, 255, 255]), // значения поверхностей (row-major RGB)
  1                                // количество поверхностей на сценарий
);

// 4. Чтение сертифицированных выходов
if (snapshot.state === "ready" && snapshot.outputCount() > 0) {
  const slot = snapshot.outputSlot(0);
  const rgb = snapshot.outputRgb(0);      // Uint8Array [R, G, B]
  const opacity = snapshot.outputOpacity(0); // number 0..1
}

// 5. Явное управление жизненным циклом
runtime.free(); // или Symbol.dispose через `using`
```

Этот пример проходит проверку типов против `packages/colors/index.d.ts`. Исходный код теста: `packages/colors/smoke.consumer.ts`.

## Граница зрелости

### Стабильно

- Компиляция Program wire и идентичность содержимого.
- Атомарные переходы `ProgramRuntime.updateObserved` / `updateUnknown`.
- Аксессоры выходов и состояние жизненного цикла `ProgramSnapshot`.
- Автономная оценка `evaluateWcag22`.
- Схема V2 `numericalCapabilityManifest`.
- Машина состояний инициализации WASM (`init` / `initSync`).

### В разработке / ещё не стабильно

- Параметр доверия семейных артефактов (закрыт по умолчанию).
- Браузерные/DOM-хелперы прикрепления (удалены в C7c; повторное введение TBD).
- Решение Display-P3, HDR/PQ/HLG.
- Сертификация пространственного поля Glow/Material.

## Миграция с устаревшего API (до C7c)

| Удалено | Замена |
|---|---|
| `new LabColors()` + `loadConfig(json)` | `compileProgramWire(canonicalBytes, streamId)` |
| `resolveTheme(bg, theme)` | `runtime.updateObserved(revision, scenarioIds, surfaces, count)` |
| `applyTheme(el, result)` | Ручное применение CSS-переменных из `snapshot.outputRgb/Opacity` |
| `watchTheme(el, opts)` | Внешний MutationObserver + `runtime.updateObserved` при изменении |
| `adaptTheme(...)` | Полный пересчёт через `updateObserved` с новым сценарием |
| `ThemeConfig` / `RoleRecipe` JSON | Канонические Program wire байты (формат LCPW v1) |
| `NamedRoleTable` | Непрозрачный скомпилированный граф внутри `ProgramRuntime` |

Автоматического инструмента миграции нет. Устаревший конфиг необходимо переавторизовать как Program wire.

## Документация

- [Туториал: первый запуск](docs/tutorials/start.md)
- [How-to: работа с рантаймом](docs/how-to/runtime.md)
- [Справочник: API](docs/reference/api.md)
- [Справочник: профили возможностей](docs/reference/profiles.md)
- [Объяснение: модель](docs/explanation/model.md)
- [Объяснение: архитектура](docs/explanation/architecture.md)
- [Объяснение: научная база](docs/explanation/science.md)

## Структура репозитория

```text
crates/
├── labcolors-core          — математика, компиляция wire, решение
├── labcolors-wasm          — WASM-граница
├── labcolors-ffi           — нативная FFI-граница
└── labcolors-conformance   — общие тест-векторы

packages/
└── colors                  — npm-пакет и фасад инициализации

docs/
├── tutorials/
├── how-to/
├── reference/
└── explanation/
```

## Зависимости

`labcolors-core` имеет ноль рантайм-зависимостей. Проверяемый контракт:

```sh
cargo tree -p labcolors-core --edges=no-dev
```

Команда должна вывести только `labcolors-core` без дочерних рантайм-пакетов. Dev-зависимости тестов и бенчмарков в этот контракт не входят.

## Проверки

Основной локальный набор:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

Изменения browser/WASM/public wire contract дополнительно требуют сборки пакета, consumer-smoke, JS/browser tests и platform conformance. Обязательные gates определяются workflow-файлами и затронутым публичным контрактом.