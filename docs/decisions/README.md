# Архитектурные решения (ADR)

`docs/decisions/` — это **трекаемый источник истины** для необратимых и
несущих решений движка lab-colors. Каждое решение, на которое ссылаются другие
части системы, живёт здесь как ADR на ветке-tip (а не в stash или orphan-объекте,
который может стереть `git gc`). Не задекларировано здесь — не существует.

## Реестр решений

| Файл | Статус | Область применения | Дата |
|---|---|---|---|
| [`apca-license.md`](apca-license.md) | принято | константы APCA в LPC — лицензионная позиция (`crates/labcolors-core/src/lpc.rs`) | 2026-06-10 |
| [`theme-invariant.md`](theme-invariant.md) | принято | инвариант светлой/тёмной темы (`semantic.rs`, `lpc.rs`, `spaces/vc.rs`) | 2026-06-10 |
| [`surface-jnd.md`](surface-jnd.md) | принято (частично) — shadow ramp **HARD BLOCKED** | перцептивные магнитуды декоративного контракта (`semantic.rs`: `DECORATIVE_FLOOR_MIN`, `Role::Separator`, `SHADOW_*_JND`) | 2026-06-21 |

## `surface-jnd.md` — статус ратификации

**`DECORATIVE_FLOOR_MIN = 15.0`** и **`Role::Separator = decorative(DECORATIVE_FLOOR_MIN)`**
ратифицированы эпиком `separator-tracks-jnd-floor` (chapter
`raise-floor-and-pin-separator`). Ступени shadow-ramp (`SHADOW_*_JND`) —
порядковые-заглушки (15.5/17.5/19.5/21.5), не производные JND; они остаются
**`TBD` — заполняются скоупом `shadow-ramp-derivation`**. Shadow ramp остаётся
**HARD BLOCKED** на главе non-solid-backgrounds до тех пор, пока не определена
модель alpha→effective-luminance для составных фонов.
