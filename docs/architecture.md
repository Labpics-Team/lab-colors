# Архитектура Lab Colors

## Назначение

Этот документ объясняет устойчивые границы системы. Он не хранит roadmap, текущий SHA, активный PR или статус исследований.

Живое состояние разработки находится в GitHub Issue `#228`. Полный Definition of Done — в `#248`.

## Система в одном предложении

Lab Colors компилирует клиентскую схему цветовых токенов в набор формальных контрактов, решает их для текущего локального контекста и поддерживает результат при изменении этого контекста.

```text
client schema
→ compile
→ compiled table / dependency model
→ resolve(current context)
→ emitted token values
→ recheck
→ selective re-resolve
```

## Слои

### 1. Клиентская дизайн-система

Клиент владеет:

- именами токенов;
- semantic roles;
- названиями уровней;
- component state names;
- aliases;
- темами и mode IDs;
- тем, какие элементы связаны и встречаются вместе;
- non-color cues и component semantics.

Эти понятия не являются частью generic color core.

Пример:

```text
text.primary
button.default.fill
button.default.label
button.pressed.fill
status.critical.label
```

Для core это непрозрачные идентификаторы. Слова не запускают скрытую ветку алгоритма.

### 2. Compile boundary

`ThemeConfig` валидируется и компилируется в `NamedRoleTable`.

Compile stage отвечает за:

- корректность IDs и ссылок;
- aliases;
- допустимость рецептов и параметров;
- theme/profile references;
- canonical fingerprint;
- атомарность: ошибочный конфиг не заменяет уже загруженный корректный.

Compile stage не имеет права выдавать context-free значения токенов за универсальные цвета.

### 3. Pure resolve

Resolve получает:

- compiled table;
- текущий фон;
- theme/profile;
- поддерживаемый output context.

Он возвращает полный набор результатов для одного context scope.

Pure resolve:

- детерминирован;
- не хранит скрытую runtime history;
- не зависит от порядка словаря;
- не изменяет exact sources;
- возвращает недостижимость явно;
- повторно измеряет конечный emitted state там, где контракт относится к output.

### 4. Runtime controller

Browser runtime управляет изменяющимся окружением:

```text
DOM / caller samples
→ effective context
→ recheck current values
→ preserve while valid
→ resolve when required
→ update scoped CSS variables
```

`watchTheme` и `adaptTheme` являются stateful adapters над pure resolver, а не частью математического source of truth.

### 5. Platform adapters

WASM, JavaScript, Swift/FFI, CLI, CSS, DTCG, Tailwind и Figma integrations должны использовать один canonical core contract.

Adapter может:

- преобразовать DTO;
- наблюдать platform state;
- применить результат;
- сообщить platform capability.

Adapter не может:

- повторно реализовать color math;
- угадывать client semantics;
- подставлять platform-specific fallback без статуса;
- расширять scientific claim core.

## Источники истины внутри цветовой модели

### Exact sources

Exact source:

- задан клиентом;
- имеет provenance;
- сохраняется побайтно в объявленном source contract;
- не становится solver variable.

К exact sources относятся anchors и explicit literals.

### Derived values

Derived value:

- связан с source/family provenance;
- строится по versioned recipe/profile;
- зависит от контекста только через явно объявленный контракт;
- может меняться при смене background/theme/output;
- проверяется после final emission.

### Coordinate views

Oklab, CAM16, CAM16-UCS и другие координаты — представления одного стимула, а не независимые источники истины.

Нельзя независимо задавать или интерполировать несколько hue-представлений одного цвета без материализации единого физически представимого состояния.

У ахроматического состояния hue отсутствует. Числовой placeholder не считается измеренным направлением.

## Client semantics и физические контракты

Клиентское слово само по себе не несёт математики.

```text
secondary
```

становится вычислимым только после отображения в конкретный contract bundle.

Например:

```text
client token: text.secondary
→ foreground readability contract
→ floor
→ ordering relation

client token: border.secondary
→ edge separation contract
→ ordering relation
```

Оба token могут называться `secondary`, но не обязаны иметь одинаковый `Lc`, `J′`, alpha, `t` или hex.

## Специализированные примитивы текущей реализации

### `TextAnchor`

Решает foreground относительно текущего background.

Контракт может включать:

- долю доступного контраста;
- нормативный floor;
- polarity;
- hierarchy ordering.

Не является общей мерой силы любого объекта.

### `DjAnchor`

Решает appearance-delta относительно background в объявленной модели.

Не является автоматически:

- читаемостью;
- WCAG;
- prominence;
- семантической различимостью.

### `Ladder`

Воспроизводит клиентский preset тинта/alpha.

Текущие Figma alpha positions — измеренные данные конкретного клиента, а не generic уровни core.

### `PairFill` / `PairLabel`

Моделируют вложенную зависимость:

```text
parent background
→ resolve fill
→ compute emitted composite
→ resolve label against that composite
```

Это ключевой пример того, почему токены образуют граф, а не независимый словарь.

### `Material`

Описывает объявленную многослойную композицию.

Точная модель зависит от:

- layer order;
- source;
- alpha association;
- backdrop set;
- output profile.

Point material recipe не является полной моделью blur, refraction или physical glass.

### `Glow`

Текущий контракт относится к точечным цветовым слоям и их композитам.

Spatial glow требует дополнительных данных:

- kernel;
- extent;
- overlap;
- backdrop field;
- temporal context.

Point result не сертифицирует spatial field.

### `Zero`

Явное значение «нет цветового результата». Это не пропущенная роль и не ошибка сериализации.

## Dependency model

Концептуально токены образуют ориентированный граф:

```text
source
→ derived fill
→ emitted composite
→ dependent label
```

Другие связи:

- exact alias;
- immutable source;
- ordering;
- must-distinguish;
- correspondence between contexts;
- client-defined relation.

Текущая implementation использует `NamedRoleTable` и специализированные recipes. Любое обобщение graph path обязано сначала воспроизвести существующее поведение дифференциальными тестами.

### Циклы

Односторонняя зависимость решается топологически.

Цикл допустим только при явно определённом joint contract. Нельзя разрешать цикл случайным порядком итерации.

### Incremental resolve

Runtime может пересчитывать только затронутую dependency closure, если доказано равенство полному resolve.

Иначе применяется полный resolve scope.

## Continuous family и finite output

Continuous family отвечает на вопрос:

> Какие состояния принадлежат одному вычислительному семейству?

Finite emission отвечает на вопрос:

> Какое состояние реально можно отдать потребителю?

```text
anchors
→ versioned continuous construction
→ candidate selection by contract
→ output mapping / quantization
→ final emitted state
→ postcondition measurement
```

Правила:

- anchors и construction — разные сущности;
- параметр `t` не содержит client semantics;
- одинаковый client level не означает одинаковый `t`;
- непрерывная оптимальность не доказывает оптимальность на конечной output grid;
- current gamma/hue/chroma parameters остаются compatibility policies, пока более сильный статус не доказан.

## Контраст и другие outcomes

Система не использует один universal quality score.

Отдельные contracts:

- normative WCAG;
- foreground legibility;
- appearance delta;
- rendered composite separation;
- family identity;
- pairwise distinction;
- optional observer estimate;
- migration/recourse.

Разные единицы нельзя складывать в один scalar без принятой модели.

Hard contract проверяется отдельно и не компенсируется улучшением другого outcome.

## Ошибки и результаты

Нужно различать:

### Compile failure

Схема или конфиг невалидны. Новый graph/table не публикуется.

Примеры:

- неизвестная ссылка;
- недопустимый параметр;
- конфликт aliases;
- противоречивый graph.

### Resolve outcome

Вызов корректен, но конкретная роль может быть физически недостижима в текущем context.

Это результат роли, а не обязательно ошибка всего вызова.

### Runtime status

Контроллер может:

- сохранить текущий certified state;
- выполнить recheck без resolve;
- начать новый resolve;
- отменить stale generation;
- остановиться;
- сообщить недостаточный runtime context.

### Internal invariant failure

Ошибка реализации. Её нельзя представлять как клиентский `Unreachable` или правдоподобный цвет.

## Runtime context

### `effectiveBackground`

Поддерживает вычисление эффективного background из совместимых `background-color` предков.

Не моделирует автоматически:

- background image;
- gradient;
- mix-blend-mode;
- filter/backdrop-filter;
- blur;
- video;
- HDR/WCG platform transform.

Для неоднородного фона caller передаёт образцы или другой explicit context adapter.

### `watchTheme`

Подходит для дискретных изменений, которые наблюдает текущий adapter.

Не следует считать, что `MutationObserver` видит любое изменение computed style, media environment или layout.

### `adaptTheme`

Используется для частого изменения background.

Правильная последовательность:

```text
sample context
→ recheck current result
→ require sustained breach according to controller profile
→ resolve
→ apply safely
```

Sustain, dwell и ease — разные механизмы. Один threshold с задержкой не следует называть Schmitt hysteresis.

Pure resolver не зависит от controller history.

## Themes

Theme ID принадлежит клиенту и adapter schema.

Exact theme-specific anchor имеет приоритет и не меняется.

Если derived state отсутствует, он может быть решён заново в target context по тому же client contract. Он не обязан быть инверсией или численной функцией already-emitted source-theme state.

Product increased-contrast theme, `prefers-contrast` и forced colors — разные понятия.

## Optional Screen ColorQuality

Base resolver не зависит от optional human-quality layer.

```text
Preserve
Audit
Project
```

управляет только дополнительной semantic mutation.

- `Preserve`: не менять semantic candidate.
- `Audit`: добавить факты/оценки без изменения candidate.
- `Project`: изменить candidate только с admitted profile, uncertainty semantics, `NoChange` и final-state certificate.

Нет универсального `cleanColors: boolean`.

Research proxy не становится human verdict.

## Миграция архитектуры

Любой специальный production path заменяется так:

```text
characterization
→ RED test for desired generic contract
→ parallel legacy/new paths
→ differential comparison
→ switch one consumer
→ remove replaced orchestration
```

Запрещён mega-refactor, который одновременно меняет:

- client schema;
- family construction;
- graph semantics;
- solver;
- runtime controller;
- wire format.

## Кэш и идентичность результата

Cache/certificate identity включает все данные, которые могут изменить решение:

- compiled config/graph ID;
- source/family profile IDs;
- context/theme ID;
- output profile;
- model/numerical release;
- runtime generation where applicable.

Изменение любого semantic input не должно получать результат старого cache key.

## Проверяемые инварианты

1. Перестановка client declarations не меняет результат.
2. Exact sources и aliases сохраняются.
3. Dependent label решается против final emitted composite dependency.
4. Final relation проверяется после quantization/output mapping.
5. Invalid compile не меняет active valid graph.
6. Incremental resolve совпадает с full resolve.
7. Stale runtime generation не пишет значения.
8. Stop/cancel запрещает дальнейшие writes.
9. Unsupported domain возвращает статус, а не fallback.
10. Second-client fixture проходит без Lab UI vocabulary.
11. Platform adapters дают эквивалентные decision classes и declared bytes.
12. Documentation claim не сильнее теста, evidence или capability manifest.

## Антипаттерны

- Парсить семантику из имени токена.
- Встраивать слова конкретной дизайн-системы в core.
- Считать один цвет значением токена для всех фонов.
- Использовать фиксированный `t` как semantic level.
- Смешивать WCAG, preference и identity в weighted sum.
- Исправлять связанные роли независимо.
- Изменять source anchor ради оптимизации.
- Выдавать current client preset за universal law.
- Прятать ошибку под neutral/black/white fallback.
- Дублировать цветовую математику в adapter.
- Считать screenshot единственным correctness oracle.
- Документировать roadmap как долговечную архитектуру.
