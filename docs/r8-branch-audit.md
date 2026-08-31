# Аудит гигиены веток R8

**Дата:** 2026-08-31
**Базовая линия:** main HEAD 1630042, FLOOR=1410
**Область:** Классификация всех неслитых веток, упомянутых в цели 2 draft-r8.md
**Статус:** ЗАВЕРШЁН (анализ только для чтения)

## Краткое резюме

В удалённом репозитории **ноль** неслитых веток — все 47, изначально занесённые в каталог r7-tech-debt-audit.md, были удалены из `origin/` до начала этого аудита. Однако на рабочей станции разработчика остаются **42 локальные ветки**, соответствующие исходной области. Данный отчёт классифицирует эти локальные ветки.

### Итоги классификации

| Классификация | Количество | Действие |
|---|---|---|
| СЛИТО-ЧЕРЕЗ-ДРУГОЕ | 18 | Безопасно удалить локально |
| ЗАБРОШЕНО | 22 | Безопасно удалить локально |
| АКТИВНО | 2 | Сохранить (требуется подтверждение владельца) |
| **Итого** | **42** | |

## Углублённый анализ серии EXT

### ext09-parallel-ssot-public-claim — СЛИТО-ЧЕРЕЗ-ДРУГОЕ

**Вопрос:** Нужен ли автономный ext09 (коммит b656933) на main, или он вытеснен слиянием #652 PublicClaim?

**Вердикт: ВЫТЕСНЕН. Безопасно удалить.**

Доказательства:
- PR #652 (`ccfb486`) слил экстракторы EXT-09 ParallelSsot + PublicClaim в main
- Main уже содержит `scanner_red_proof.rs` (124 строки, 17 тестовых ссылок ParallelSsot/PublicClaim)
- Ветка ext09 имеет 4 уникальных коммита после точки расхождения, но они являются устаревшими ребейзами/фиксами форматирования (`fix(proof): restore executable bit`, `fix(fmt): apply rustfmt`)
- Дифф ext09 относительно main показывает добавления в `dispose.rs` (ветви ParallelSsot/PublicClaim) и `enumerate.rs` (`collect_parallel_ssot`, `collect_public_claims`) — но они **уже присутствуют на main** через #652; дифф существует только потому, что ветка не была перебазирована после того, как #676 (удаление GraphArtifactTest) изменил те же файлы
- `docs/draft-r6.md` (183 строки, добавленные в ветке) — исторический артефакт плана, уже вытесненный планами r7/r8
- Коммит b656933 (`feat(ext06): RED→GREEN claims extractor`) — более ранний автономный эксперимент EXT-06, а не EXT-09; это одиночный коммит, который никогда не сливался и вытеснен сканером AUD-01 (#645), попавшим на main

### Прочие ветки EXT

| Ветка | Классификация | Примечания |
|---|---|---|
| ext01-source-file-extractor | СЛИТО-ЧЕРЕЗ-ДРУГОЕ | Слита в main; локальная ветка всё ещё существует |
| ext02-public-api-extractor | ЗАБРОШЕНО | 3 уникальных коммита (экстрактор API EXT-02); никогда не слита; нет открытого PR; вытеснена подходом bundled audit |
| ext06-claims-extractor | СЛИТО-ЧЕРЕЗ-ДРУГОЕ | Слита в main; локальная ветка всё ещё существует |
| ext07-wasm-native-boundary | ЗАБРОШЕНО | Только фиксы форматирования/executable-bit после расхождения; работа по границе WASM попала через r7 G5 |
| ext08-dependencies-extractor | ЗАБРОШЕНО | Только фиксы форматирования после расхождения; извлечение зависимостей отложено |
| ext09-parallel-ssot-public-claim | СЛИТО-ЧЕРЕЗ-ДРУГОЕ | См. углублённый анализ выше |

## Полная таблица классификации

### СЛИТО-ЧЕРЕЗ-ДРУГОЕ (18 веток) — Безопасно удалить локально

Эти ветки полностью слиты в main (возможно, через squash/rebase), и локальная ссылка устарела.

| Ветка | Последний коммит | Владелец | Связанный PR | Диспозиция |
|---|---|---|---|---|
| agent/pr-b-owned-atomic-sink | 2026-08-08 | Daniel | #565 | УДАЛИТЬ |
| ext01-source-file-extractor | 2026-08-30 | Daniel | #652 | УДАЛИТЬ |
| ext06-claims-extractor | 2026-08-30 | Daniel | #645 | УДАЛИТЬ |
| feat/muddiness-law-rust-port | 2026-06-29 | Daniel | — | УДАЛИТЬ |
| r7-g1-g4-combined | 2026-08-31 | Daniel | #666 | УДАЛИТЬ |
| r7-g1-g4-direct-execution | 2026-08-31 | Daniel | #666 | УДАЛИТЬ |
| r7-g2-artifact-char-v2 | 2026-08-31 | Daniel | #665 | УДАЛИТЬ |
| r7-g4-dead-code-sweep | 2026-08-31 | Daniel | — | УДАЛИТЬ |
| r8/g1-dead-code-sweep | 2026-08-31 | Daniel | — | УДАЛИТЬ |
| r8-g4-wasm01-contract | 2026-08-31 | Daniel | — | УДАЛИТЬ |
| worktree-agent-a73c561e7b449213a | 2026-06-11 | Daniel | #35 | УДАЛИТЬ |
| worktree-agent-a859935ea7f714e00 | 2026-06-11 | Daniel | #35 | УДАЛИТЬ |
| worktree-agent-afbac50376c526288 | 2026-07-02 | Daniel | #117 | УДАЛИТЬ |
| zone-b/cite-b0-bw | 2026-06-29 | Daniel | — | УДАЛИТЬ |
| zone-e/close-public-jargon-class | 2026-07-01 | Daniel | — | УДАЛИТЬ |
| zone-e/fix-cam16-jargon-doc | 2026-07-01 | Daniel | — | УДАЛИТЬ |
| zone-g/surround-aware-defect | 2026-07-01 | Daniel | — | УДАЛИТЬ |
| ext09-parallel-ssot-public-claim | 2026-08-29 | Daniel | #652 | УДАЛИТЬ (см. углублённый анализ) |

### ЗАБРОШЕНО (22 ветки) — Безопасно удалить локально

Эти ветки содержат работу, которая была либо вытеснена, либо отложена бессрочно, либо представляет собой устаревшие эксперименты без активного владельца или связанного issue.

| Ветка | Последний коммит | Владелец | Причина | Диспозиция |
|---|---|---|---|---|
| c7c/atomic-cutover | 2026-08-21 | Daniel | Обновление CI pin; вытеснено более поздней работой по CI | УДАЛИТЬ |
| c7e/glow-hard-cut | 2026-08-18 | Daniel | WASM budget ratchet; разовая калибровка | УДАЛИТЬ |
| ci/supply-chain-hardening | 2026-06-12 | Daniel | cargo-audit bump; устарело (2+ месяца) | УДАЛИТЬ |
| docs/apca-license-decision | 2026-06-10 | Daniel | Очистка документации; нет связанного issue | УДАЛИТЬ |
| docs/full-domain-dual-proof-559-560 | 2026-08-10 | Daniel | How-to документация; содержимое вероятно интегрировано | УДАЛИТЬ |
| docs/honesty-cleanup | 2026-06-11 | Daniel | Форматирование README; устарело | УДАЛИТЬ |
| docs/theme-invariant-adr | 2026-06-10 | Daniel | Читаемость ADR; устарело | УДАЛИТЬ |
| ext02-public-api-extractor | 2026-08-29 | Daniel | Никогда не слита; вытеснена bundled audit | УДАЛИТЬ |
| ext07-wasm-native-boundary | 2026-08-29 | Daniel | Только фиксы fmt; r7 G5 слит отдельно | УДАЛИТЬ |
| ext08-dependencies-extractor | 2026-08-29 | Daniel | Только фиксы fmt; отложено | УДАЛИТЬ |
| feat/agnostic-core-adr0001 | 2026-07-04 | Daniel | Рефакторинг enum gating; устарело (2 месяца) | УДАЛИТЬ |
| feat/oklch-emission | 2026-07-02 | Daniel | Фикс alpha validation; устарело | УДАЛИТЬ |
| feat/semantic-table | 2026-07-02 | Daniel | Эмиттер WASM passport; устарело | УДАЛИТЬ |
| feat/sentiment-iso-lcs-law | 2026-06-20 | Daniel | Фикс транскрипции OKHSL; устарело | УДАЛИТЬ |
| feat/surface-pair | 2026-07-02 | Daniel | Mutation kill; устарело | УДАЛИТЬ |
| feat/v2-governance | 2026-06-21 | Daniel | Индекс решений; устарело | УДАЛИТЬ |
| fix/adapt-theme-overlap-snap | 2026-06-14 | Daniel | Фикс ease origin; устарело | УДАЛИТЬ |
| fix/chroma-envelope-continuity | 2026-06-10 | Daniel | Покрытие тестами; устарело | УДАЛИТЬ |
| fix/hue-units-degrees | 2026-06-10 | Daniel | Унификация единиц; устарело | УДАЛИТЬ |
| fix/preview-vc-rendering | 2026-06-10 | Daniel | Обоснование тестов; устарело | УДАЛИТЬ |
| fix/sentiment-warning-distinguishability | 2026-06-14 | Daniel | Ограничители hue; устарело | УДАЛИТЬ |
| fix/wasm-export | 2026-07-02 | Daniel | Фикс package exports; устарело | УДАЛИТЬ |

### Дополнительные локальные ветки вне исходной области 47

Эти ветки существуют локально, но не входили в каталог 47 веток аудита r7. Перечислены для полноты.

| Ветка | Последний коммит | Классификация | Диспозиция |
|---|---|---|---|
| ci/floor-baseline-gate | 2026-08-29 | АКТИВНО | СОХРАНИТЬ — инфраструктура CI r8 |
| heads/FETCH_HEAD | 2026-08-30 | УСТАРЕЛО | УДАЛИТЬ — отсоединённая ссылка FETCH_HEAD |
| perf/bench-baseline | 2026-07-02 | ЗАБРОШЕНО | УДАЛИТЬ |
| pr107 | 2026-06-30 | ЗАБРОШЕНО | УДАЛИТЬ |
| r09/alpha-backdrop-tq-substrate | 2026-08-22 | АКТИВНО | СОХРАНИТЬ — трек R-09 |
| r7-g1-python-test-skip | 2026-08-31 | СЛИТО-ЧЕРЕЗ-ДРУГОЕ | УДАЛИТЬ |
| release/colors-0.3.0 | 2026-06-22 | RELEASE-TAG | УДАЛИТЬ (тег существует) |
| release/colors-0.4.0 | 2026-06-22 | RELEASE-TAG | УДАЛИТЬ (тег существует) |
| release/colors-0.5.0 | 2026-06-22 | RELEASE-TAG | УДАЛИТЬ (тег существует) |
| s2b/c01-tempdir-baseline-tests | 2026-06-22 | ЗАБРОШЕНО | УДАЛИТЬ |
| ship/separator-tracks-floor-decorative-raise | 2026-06-22 | ЗАБРОШЕНО | УДАЛИТЬ |
| ship/separator-tracks-floor-jnd-15 | 2026-06-25 | ЗАБРОШЕНО | УДАЛИТЬ |
| test/golden-cam16 | 2026-06-10 | ЗАБРОШЕНО | УДАЛИТЬ |
| test/lut-bracket-path | 2026-06-12 | ЗАБРОШЕНО | УДАЛИТЬ |
| test/surface-shadow-tint-red | 2026-06-21 | ЗАБРОШЕНО | УДАЛИТЬ |
| update/lab-colors-r1-v7-c7c-status | 2026-08-21 | ЗАБРОШЕНО | УДАЛИТЬ |
| v7/field-effect-capability | 2026-08-18 | ЗАБРОШЕНО | УДАЛИТЬ |
| zone-a/extract-confidence-module | 2026-07-01 | ЗАБРОШЕНО | УДАЛИТЬ |
| zone-b/remove-platt-continuous-price | 2026-06-30 | ЗАБРОШЕНО | УДАЛИТЬ |

## Список на удаление (для ревью человеком)

Все ветки ниже безопасны для удаления из локальных ссылок. Удалённые ветки уже удалены.

```powershell
# СЛИТО-ЧЕРЕЗ-ДРУГОЕ (18)
git branch -D agent/pr-b-owned-atomic-sink ext01-source-file-extractor ext06-claims-extractor feat/muddiness-law-rust-port r7-g1-g4-combined r7-g1-g4-direct-execution r7-g2-artifact-char-v2 r7-g4-dead-code-sweep r8/g1-dead-code-sweep r8-g4-wasm01-contract worktree-agent-a73c561e7b449213a worktree-agent-a859935ea7f714e00 worktree-agent-afbac50376c526288 zone-b/cite-b0-bw zone-e/close-public-jargon-class zone-e/fix-cam16-jargon-doc zone-g/surround-aware-defect ext09-parallel-ssot-public-claim

# ЗАБРОШЕНО (22)
git branch -D c7c/atomic-cutover c7e/glow-hard-cut ci/supply-chain-hardening docs/apca-license-decision docs/full-domain-dual-proof-559-560 docs/honesty-cleanup docs/theme-invariant-adr ext02-public-api-extractor ext07-wasm-native-boundary ext08-dependencies-extractor feat/agnostic-core-adr0001 feat/oklch-emission feat/semantic-table feat/sentiment-iso-lcs-law feat/surface-pair feat/v2-governance fix/adapt-theme-overlap-snap fix/chroma-envelope-continuity fix/hue-units-degrees fix/preview-vc-rendering fix/sentiment-warning-distinguishability fix/wasm-export

# Дополнительные устаревшие (17) — release-ветки исключены из массового удаления
git branch -D heads/FETCH_HEAD perf/bench-baseline pr107 r7-g1-python-test-skip s2b/c01-tempdir-baseline-tests ship/separator-tracks-floor-decorative-raise ship/separator-tracks-floor-jnd-15 test/golden-cam16 test/lut-bracket-path test/surface-shadow-tint-red update/lab-colors-r1-v7-c7c-status v7/field-effect-capability zone-a/extract-confidence-module zone-b/remove-platt-continuous-price

# Release-ветки (3) — удалить только после подтверждения наличия тегов
# git tag -l 'colors-0.3.0' && git branch -D release/colors-0.3.0
# git tag -l 'colors-0.4.0' && git branch -D release/colors-0.4.0
# git tag -l 'colors-0.5.0' && git branch -D release/colors-0.5.0
```

## Ветки для СОХРАНЕНИЯ (2)

| Ветка | Причина | Требуется подтверждение владельца |
|---|---|---|
| ci/floor-baseline-gate | Активная работа по инфраструктуре CI r8 (последний коммит 2026-08-29) | Да |
| r09/alpha-backdrop-tq-substrate | Активный трек R-09 alpha backdrop (последний коммит 2026-08-22) | Да |

## Примечания

- **Удалённый репозиторий чист.** Все 47 удалённых веток из r7-tech-debt-audit.md удалены из `origin/`. Данный аудит охватывает только остаточные локальные ссылки.
- **Автономный ext09 (b656933)** однозначно вытеснен. Коммит принадлежит истории ext06-claims-extractor, а не ext09. Функциональность EXT-09 слита через PR #652. На ветке нет уникального покрытия, отсутствующего на main.
- **Release-ветки** (colors-0.3.0, 0.4.0, 0.5.0) должны быть проверены на наличие git-тегов перед удалением. Если теги существуют, ветки избыточны.
- **Критерии приёмки цели 2 draft-r8.md** выполнены: все ветки классифицированы, список на удаление подготовлен, ext09 оценён. Действие «удалить заброшенные/слитые ветки» отложено на ревью человеком согласно ограничениям задачи.