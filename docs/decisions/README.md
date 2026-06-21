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

## Внимание: `surface-jnd.md` — это скелет

Выведенные **финальные магнитуды** (значение `DECORATIVE_FLOOR_MIN`, пин
`Role::Separator`, ступени shadow-ramp) в `surface-jnd.md` помечены **`TBD`** —
их ратифицируют downstream-скоупы `jnd-floor-and-separator-pin` и
`shadow-ramp-derivation`, **не** этот эпик. Сохранены только факты движка
(аналитический клип 7.30 Lc, описание сетки 7.6) и цитаты-первоисточники
(порог невидимости тонкой линии Lc 15). Читатель не должен принимать `TBD`-числа
за окончательные: пока соответствующий скоуп не закрыт, это плейсхолдеры, а
shadow-ramp остаётся **HARD BLOCKED** на главе non-solid-backgrounds.
