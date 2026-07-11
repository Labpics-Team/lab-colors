# Документация Lab Colors

Документация разделена по назначению. Текущий SHA, активный PR и roadmap здесь не дублируются: живое состояние разработки хранится в GitHub Issue `#228`.

## Начать работу

- [Root README](../README.md) — назначение продукта и быстрый старт.
- [Browser/WASM package](../packages/colors/README.md) — установка и публичный JavaScript API.
- [Архитектура](architecture.md) — устойчивые границы compile, resolve и runtime.

## Руководства

- [Browser/WASM package](../packages/colors/README.md) — `loadConfig`, `resolveTheme`, `applyTheme`, `watchTheme`, `adaptTheme`.
- [Конфиг-граница](decisions/0001-config-boundary.md) — почему семантика принадлежит клиенту.
- [Правила именования](NAMING.md) — публичные и внутренние имена.

Практические руководства должны использовать public API и проверяться тестами. План будущих руководств хранится в Issues, а не в этом индексе.

## Справочник

- `crates/labcolors-core` — Rust API и rustdoc.
- `packages/colors` — browser/WASM API и TypeScript declarations.
- `crates/labcolors-conformance` — platform-neutral test vectors.
- `docs/decisions/` — принятые архитектурные решения.
- `docs/conformance/` — зафиксированные platform attestations и ограничения.

Если текст и сгенерированный API расходятся, source code, tests и принятые ADR имеют приоритет; документация исправляется в том же срезе.

## Объяснение

- [Архитектура](architecture.md) — client schema, dependency model, contextual resolve, runtime и adapters.
- [Whitepaper](whitepaper.md) — математические и исследовательские основания.

Whitepaper не является roadmap. Каждое утверждение должно указывать собственный статус: стандарт, опубликованная модель, математический вывод, измеренный client preset, product policy или открытая гипотеза.

## Исследования и provenance

- [Empirical inventory](empirical-inventory.md) — происхождение текущих коэффициентов и policies.
- [Empirical residue](empirical-residue.md) — результаты проверок и оставшиеся научные вопросы.
- `docs/psychophysics/` — протоколы и исследовательские материалы, если они применимы к актуальной версии.
- `docs/experiments/` — исторические или воспроизводимые эксперименты с явной версией кода.

Исторический артефакт не считается доказательством текущего release без привязки к commit, модели, данным и domain.

## Разработка

- [`AGENTS.md`](../AGENTS.md) — постоянный контракт автономной разработки.
- GitHub Issue `#276` — стартовая карта для агента без контекста.
- GitHub Issue `#228` — текущий SHA, активный PR и следующий correctness-root.
- GitHub Issue `#248` — полный Definition of Done продукта.

Документы не должны копировать динамический статус этих Issues.

## Правила текста

1. Начинать с задачи пользователя или границы системы, а не с истории формулы.
2. Отделять client semantics от generic core.
3. Отделять текущую implementation от гипотезы и roadmap.
4. Не называть математический correlate человеческим outcome без evidence.
5. Не обещать browser/display/HDR/spatial capability шире тестируемого профиля.
6. Не дублировать изменяемые counts, размеры и file inventory без автоматической проверки.
7. Использовать русский язык; англоязычный термин оставлять только когда он является именем API, стандарта или устоявшимся техническим понятием.
8. Комментарий объясняет причину, инвариант или границу; не пересказывает код.
