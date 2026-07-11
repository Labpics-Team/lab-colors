# Архитектура Lab Colors

## Назначение

Документ разделяет:

1. фактически работающий продуктовый путь;
2. устойчивые архитектурные инварианты;
3. границы, которые нельзя выдавать за реализованную capability.

Текущий SHA, активный PR и roadmap здесь не хранятся. Живое состояние разработки находится в GitHub Issue `#228`; полный Definition of Done — в `#248`.

## Краткая модель

```text
client ThemeConfig
→ validate and compile
→ NamedRoleTable

NamedRoleTable
+ current local background
+ theme/profile supported by adapter
→ resolve whole role set
→ emitted values and per-role statuses

runtime context change
→ recheck current values
→ keep while valid
→ resolve again when required
```

Lab Colors не присваивает клиентским словам математический смысл. Дизайн-система задаёт имена и отношения, core выполняет объявленные цветовые контракты.

## Что работает в текущем продукте

### Compile boundary

`ThemeConfig`:

- содержит источники, themes, roles, recipes и aliases;
- валидируется до публикации;
- компилируется в `NamedRoleTable`;
- получает fingerprint для cache separation;
- не заменяет активный корректный config при ошибке загрузки.

Имена ролей принадлежат клиенту. Core не выбирает алгоритм по словам `primary`, `danger`, `hover` или имени компонента.

### Contextual resolve

`resolve_named_set` / `resolveTheme` решает полный набор ролей для одного локального background и theme/profile.

В одном результате могут находиться:

- решённые solid/translucent/material/glow values;
- явное отсутствие значения;
- физически недостижимая роль;
- диагностические флаги конкретного recipe.

Невалидный request и недостижимость одной валидной роли — разные события.

### Специализированные recipes

Текущая реализация содержит специализированные способы решения:

| Recipe / примитив | Контракт |
|---|---|
| `TextAnchor` | foreground относительно текущего background |
| `DjAnchor` | appearance-delta относительно background в объявленной модели |
| `DecorativeLc` | декоративная контрастная величина без автоматического текстового floor |
| `Ladder` | клиентский preset тинта/alpha |
| `AlphaAnalog` | alpha-представление заданной solid-цели в объявленном compositing profile |
| `PairFill` / `PairLabel` | вложенная зависимость «fill composite → dependent label» |
| `Material` | объявленная многослойная композиция |
| `Glow` | point-effect contract |
| `Zero` | явное значение «нет цвета» |

Эти имена описывают технический способ решения. Они не являются обязательным языком дизайн-системы.

### Browser runtime

- `applyTheme` применяет уже решённые CSS custom properties.
- `effectiveBackground` композитит поддерживаемые `background-color` предков.
- `watchTheme` обслуживает дискретные изменения, видимые текущему adapter.
- `adaptTheme` перепроверяет текущие значения и запускает resolve после устойчивого нарушения заданного runtime contract.
- Неоднородный фон передаётся вызывающей стороной как набор образцов.

`watchTheme` и `adaptTheme` — stateful adapters. Математический resolve должен оставаться отдельной детерминированной операцией.

## Граница ответственности

### Клиент владеет

- token IDs;
- semantic roles;
- названиями уровней;
- component state names;
- aliases;
- theme/mode IDs;
- declared relations и composition topology;
- non-color cues.

### Core владеет

- color spaces и output domains;
- source/family construction;
- contrast and compositing;
- gamut/output mapping в поддерживаемом scope;
- finite emission;
- generic contracts;
- resolve results и diagnostics.

### Adapter владеет

- platform DTO;
- наблюдением runtime state;
- применением результата;
- capability reporting.

Adapter не должен повторно реализовывать color math или угадывать client semantics.

## Источники и производные значения

### Exact source

Exact anchor или literal:

- приходит от клиента;
- сохраняет source provenance;
- не меняется solver-ом без отдельной явной client operation.

### Derived value

Derived value:

- связан с source/family;
- строится по объявленному recipe/profile;
- может зависеть от background, theme и output context;
- проверяется на фактически emitted state.

Alias — второе имя того же результата, а не новая независимо изменяемая копия.

## Один стимул, несколько координат

Oklab, CAM16, CAM16-UCS и другие координаты — представления одного цветового состояния.

Архитектурный инвариант:

- нельзя независимо задавать несколько hue-представлений одного цвета;
- операция должна материализовать один физически представимый working/emitted state;
- остальные координаты вычисляются из него;
- у ахромата hue отсутствует, а числовой placeholder не считается измеренным направлением.

Это не требует выбрать одну «универсально правильную» hue-ось.

## Клиентский уровень не равен одной координате

Слово клиента:

```text
secondary
```

не означает автоматически фиксированные `t`, `J′`, `Lc`, alpha или hex.

Пример отображения:

```text
text.secondary
→ readability contract
→ normative floor
→ ordering relation

border.secondary
→ edge-separation contract
→ ordering relation
```

Оба токена сохраняют одно клиентское намерение, но решают разные физические задачи.

## Непрерывные семейства и конечный output

```text
immutable anchors
→ construction policy
→ continuous family
→ selection by current contract/context
→ output mapping and quantization
→ final emitted state
→ postcondition measurement
```

Инварианты:

- anchors и construction — разные сущности;
- `t` не содержит client semantics;
- continuous family остаётся first-class low-level mechanism;
- непрерывный optimum не доказывает optimum на конечной output grid;
- current gamma/hue/chroma constants не называются универсальными законами восприятия;
- фактический output проверяется после emission.

## Dependency model

Текущая implementation использует `NamedRoleTable` и специализированные recipes. Уже существующая вложенная связь:

```text
parent background
→ PairFill
→ emitted fill composite
→ PairLabel resolved against that composite
```

показывает, что токены концептуально образуют dependency graph.

Обобщение этого графа обязано:

- сначала зафиксировать legacy behavior characterization tests;
- вводить opaque node IDs;
- не добавлять client taxonomy в core;
- сравнивать legacy и generic path дифференциально;
- переносить по одному production mechanism;
- проверять relations после final emission.

Пока generic graph не заменил специализированный путь, документация не должна выдавать его за полностью реализованный public API.

## Контраст, appearance и другие outcomes

Нет одного universal quality score.

Отдельно существуют:

- нормативный WCAG contract;
- foreground readability;
- appearance delta;
- rendered-composite separation;
- family identity;
- pairwise distinction;
- optional observer estimate;
- migration/recourse.

Разные outcomes и единицы не компенсируют друг друга без принятой модели. Hard contract проверяется отдельно.

## Ошибки и статусы

Архитектурно различаются:

### Compile failure

Config/schema невалидны; новый compiled artifact не публикуется.

### Per-role resolve outcome

Request валиден, но отдельная роль может быть недостижима в текущем context.

### Runtime status

Controller может:

- сохранить текущий result;
- выполнить recheck без resolve;
- выполнить новый resolve;
- остановиться или отменить stale work;
- сообщить недостаточный runtime context.

### Internal invariant failure

Ошибка реализации. Она не должна сериализоваться как `Unreachable` или правдоподобный цвет.

Точные типы публичных ошибок эволюционируют отдельно; текст не должен обещать более богатую wire schema, чем фактически предоставляет текущий adapter.

## Runtime context и ограничения

### `effectiveBackground`

Автоматически учитывает поддерживаемую цепочку `background-color`.

Не следует считать, что он понимает:

- image/gradient/video;
- blend modes;
- filters и backdrop blur;
- произвольный HDR/WCG transform;
- весь фактически увиденный пользователем spatial field.

Для таких случаев caller передаёт explicit samples/context.

### `watchTheme`

Наблюдает изменения, поддерживаемые текущим DOM adapter. `MutationObserver` не гарантирует обнаружение любого изменения computed style, media environment или layout.

### `adaptTheme`

Разделяет:

- recheck;
- sustained breach;
- resolve;
- применение результата.

Sustain, dwell, ease и Schmitt hysteresis — разные механизмы. Документация использует точное имя фактически реализованной policy.

## Themes и platform overrides

Theme ID принадлежит клиентской схеме, но конкретный adapter может поддерживать более узкий набор IDs.

Exact theme-specific anchor имеет приоритет.

Product increased-contrast theme, `prefers-contrast`, forced colors и UA used-value replacement — разные сущности. Authored token result не является сертификатом фактического used color после неизвестного UA override.

## Optional Screen ColorQuality

`Preserve / Audit / Project` относятся к отдельному optional layer и не управляют обязательным base resolve.

- `Preserve` — без optional semantic mutation.
- `Audit` — анализ без изменения candidate.
- `Project` — изменение только в admitted scope с `NoChange`, uncertainty и final-state certificate.

Эти понятия не означают, что observer-backed projector уже реализован или допущен к default. Research proxy не является human verdict.

## Безопасная миграция

Специальный production path заменяется так:

```text
characterization
→ RED test
→ parallel legacy/new paths
→ differential comparison
→ switch one consumer
→ remove replaced orchestration
```

Не следует одновременно менять client schema, family construction, graph semantics, solver, runtime controller и wire format одним refactor.

## Требуемые инварианты

Некоторые пункты ниже являются целевыми архитектурными gates, а не утверждением, что каждый уже полностью доказан в `main`:

1. Перестановка client declarations не меняет результат.
2. Exact sources и aliases сохраняются.
3. Dependent role решается против final emitted dependency.
4. Relation проверяется после output mapping.
5. Invalid compile не меняет active valid config.
6. Incremental resolve, если он используется, совпадает с full resolve.
7. Stale runtime work не пишет значения в новый generation.
8. Stop/cancel запрещает дальнейшие writes.
9. Unsupported domain возвращает статус, а не fallback.
10. Second-client fixture не требует Lab UI vocabulary.
11. Platform adapters сохраняют один contract.
12. Документационный claim не сильнее кода, теста, evidence или capability manifest.

## Антипаттерны

- Парсить семантику из имени токена.
- Встраивать слова конкретной дизайн-системы в core.
- Считать один hex значением токена для всех фонов.
- Использовать фиксированный `t` как semantic level.
- Смешивать WCAG, preference и identity в weighted sum.
- Исправлять связанные роли независимо.
- Изменять source anchor ради оптимизации.
- Выдавать client preset за universal law.
- Прятать ошибку под neutral/black/white fallback.
- Дублировать color math в adapter.
- Считать screenshot единственным correctness oracle.
- Описывать roadmap как уже реализованную архитектуру.
