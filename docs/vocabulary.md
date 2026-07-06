# Словарь ролей Lab UI — канон (v1)

Источник: решения владельца (Daniel), июль 2026. Этот файл — человекочитаемая
половина канона; машинная половина — `tools/figma-vocab/semantic.snapshot.txt`
и guard-тест `crates/labcolors-wasm/tests/figma_vocab_guard.rs`. Расхождение =
красный CI. Словарь меняется только вместе с решением владельца.

## Пять семей (закрытый список)

| Семья | Что это | Оси |
|---|---|---|
| `Backgrounds` | фоны и материалы поверхностей | Neutral/акценты, Grouped, Inverted, Static, Overlay, Materials |
| `Fills` | заливки интерактивов и плашек | сентимент × иерархия, Static, None |
| `Labels` | текст и глифы | сентимент × иерархия, Inverted, Static |
| `Border` | обводки и разделители (быв. Separator) | сентимент × вес, Static |
| `FX` | эффекты | цвет (Glow, Focus-ring, Skeleton, Shadow) + геометрия (Blur, Shift, Spread — коллекция 1.1 Dimension) |

Шестой семьи нет. Упразднены: `Misc` (растворение — см. таск «Misc-маппинг»),
`Icon-Labels` → `Labels`, `Separator` как семья → `Border`.

## Материалы = Backgrounds

`Material = Background` с флагом состояния. Лексикон флага: **`solid | glass | blur`**.
Слова `ambient`, `glassy` — запрещённые историзмы (код `core/theme.ts` уже мигрировал;
Figma-коллекция «⚪️ 5.0 Materials» переименована 2026-07-06).
Дерево: `Backgrounds/Materials/{Base|Subtle|Soft|Muted|Elevated}/{01|02|mix-blend-mode|backdrop-filter}`.
Отдельная коллекция «⚪️ 5.0 Materials» (3 булевых флага × моды Solid/Glass/Blur)
подлежит слиянию в `Backgrounds` — структурная операция, см. таск «Materials-merge».

## Оси (закрытые лексиконы)

- Сентимент: `Neutral | Brand | Danger | Warning | Success | Info`
- Иерархия: `Primary | Secondary | Tertiary | Quaternary`
- Вес (Border, Overlay): `Ghost | Soft | Base | Strong`
- Статика (инварианты вне темы): `Static/Light | Static/Dark`
- Прочее: `Inverted`; `Grouped` (только Backgrounds/Neutral); `None` (только Fills/Neutral); `Overlay` (только Backgrounds)

## Грамматика имени (Figma)

`Имя ::= Семья ("/" Сегмент)+`, сегмент — `[A-Za-z0-9-]+`.
Листья-CSS-свойства пишутся как свойство (`mix-blend-mode`, `backdrop-filter`) — без двоеточий.
Семья эффектов пишется `FX` (обиходное «Fx» — то же слово).

## Код-словарь (kebab-case) ↔ Figma

`label-*`, `fill-*`, `border-*` ↔ `Labels`/`Fills`/`Border`. Легаси-ключи кода
(`separator`, `surface`, `text-*`, `dim`, `dim-ic`) — deprecated; миграция с
deprecation-путём в паспорте — таск #65 (код-половина).

## Открытые вопросы (решает только владелец)

1. Асимметрия Static: `Labels/Static/*`, но `Fills|Border/Neutral/Static/*` — унифицировать?
2. `FX/Focus-ring` и `FX/Glow` без `Success`/`Info` — умышленно?
3. `Backgrounds/Materials/{Light-mode,Dark-mode}` — листья дублируют ось мод коллекции?
4. `Adaptives/*` живёт в 1.1 Dimension вне пяти семей — коллекция несемантическая, допустимо; канонизировать отдельно.
5. Маппинг Misc: `Misc/Control/Control-bg` → Backgrounds или Fills? `Misc/Badge/*` → `Labels/Badge/*`?

## Щит: как обновлять снапшот

1. Изменил переменные в Figma → экспортируй имена коллекции «🔵 4.2 Semantic»
   (figma-console MCP) и перезапиши `tools/figma-vocab/semantic.snapshot.txt`.
2. `cargo test -p labcolors-wasm --test figma_vocab_guard`.
3. Красный тест = имя вне канона: сначала решение владельца, потом имя.
   Список `GRANDFATHERED` в тесте только уменьшается.
