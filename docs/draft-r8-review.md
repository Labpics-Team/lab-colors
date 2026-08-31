---
track: lab-colors
revision: 8
type: independent-review-plan
status: PASS
verified: 2026-08-31
verifier: autonomous-agent
evidence: AC-1..AC-8 all passed; see Graphiti episode r8-wave1-independent-review-pass-2026-08-31
created: 2026-08-31
main_head: fbab4a8
floor_baseline: 1410
scope: R8 wave 1 Goals 1-4
baseline_note: pre-restructure baseline; отражает состояние до реструктуризации draft-r8.md по SPEC §4
---

# R8 Wave 1 — Независимый план ревью (Goal 5)

## a. Область

Проверяется корректность и полнота четырёх смержённых целей R8 wave 1:

| Цель | PR | Содержание |
|------|-----|------------|
| G1 Expect Sweep | #675 | Удаление устаревших `expect()` / замена на типизированные ошибки |
| G2 Branch Hygiene | #678 | Аудит и очистка remote-веток (результат: чистое дерево, 0 stale) |
| G3 EmptyClass Removal | #676 | Удаление пустых классов из AUD-01 матрицы после верификации отсутствия артефактов |
| G4 WASM-01 Harness | #677 | Новый интеграционный тест `wasm01_contract.rs` для program_wire API |

**Не входит в область:**
- Pre-existing clippy warnings (dead_code) — документированы ниже как известные
- MSRV receipt metadata mismatch — pre-existing
- Python proof test failures — вне Rust-трека
- Любые изменения кода, тестов или CI в рамках этого ревью

## b. Оси ревью

### Ось 1: Корректность wasm01_contract.rs (G4)

**Что проверяется:**
- Тесты используют только публичное API (`program_wire::*`, `Srgb8`)
- CANONICAL_BYTES действительно проходят compile и check без panic
- Детерминизм: `compile_program_wire_v1` возвращает идентичный `content_identity()` для одинаковых входных данных
- Error classification: corrupted, empty, truncated байты отклоняются как `ProgramWireCheckErrorV1::Wire`
- Session lifecycle: instantiate → update_observed → snapshot с непустыми outputs
- Identity length: check возвращает ровно 32 байта, не все нули

**Метод верификации:**
- Чтение исходного кода теста (выполнено)
- Проверка, что все assert'ы имеют осмысленные сообщения об ошибке
- Проверка отсутствия unwrap()/panic!() вне expect("reason")
- Изолированный прогон `cargo test --test wasm01_contract` (read-only verification-runner)

**Критерии:**
- [ ] Все 6 тестов компилируются и проходят
- [ ] Нет unwrap()/panic!() без expect-message
- [ ] CANONICAL_BYTES не захардкожен из приватного модуля (использует только pub API)
- [ ] Error assertions используют matches! с конкретным вариантом, не просто is_err()

### Ось 2: Полнота expect sweep (G1)

**Что проверяется:**
- Все `expect()` вызовы в crates/ имеют осмысленные сообщения
- Не осталось bare `.unwrap()` в production-коде (test code exempt)
- Замены на типизированные ошибки не изменили наблюдаемое поведение (error type compatibility)
- FLOOR baseline >= 1410 после изменений

**Метод верификации:**
- `text_search pattern="\.unwrap\(\)" glob="crates/**/*.rs"` — должен вернуть только test-код
- `text_search pattern="\.expect\(" glob="crates/**/*.rs"` — каждое сообщение осмысленно
- Сравнение FLOOR: текущий count тестов >= 1410
- Diff review PR #675: каждая замена unwrap→expect/type-safe имеет rationale

**Критерии:**
- [ ] 0 bare unwrap() в non-test production коде
- [ ] Все expect() имеют контекстное сообщение (не "should not fail")
- [ ] FLOOR >= 1410 подтверждён прогоном
- [ ] Ни одна замена не сломала error-type контракт downstream

### Ось 3: Корректность EmptyClass removal (G3)

**Что проверяется:**
- Удалённые классы действительно пусты (exhaustive search evidence в PR description или linked issue)
- AUD-01 матрица обновлена консистентно: строки удалены или помечены EmptyClass с evidence
- Никакой код не ссылается на удалённые классы (grep по имени класса)
- WASM-01 contract spec не потребляет удалённые классы

**Метод верификации:**
- Grep имён удалённых классов по всему crates/ — 0 hits
- Чтение AUD-01 matrix файла — consistency check
- Cross-reference с wasm01_contract.rs — нет зависимостей от удалённых классов
- Diff review PR #676: каждый удалённый класс имеет exit evidence

**Критерии:**
- [ ] 0 ссылок на удалённые классы в коде
- [ ] AUD-01 матрица консистентна с удалением
- [ ] Exit evidence присутствует для каждого удалённого класса
- [ ] WASM-01 harness не зависит от удалённых классов

## c. Pre-existing Issues (НЕ в области R8)

Следующие проблемы существуют на main HEAD fbab4a8 и НЕ являются регрессиями R8 wave 1:

| Проблема | Расположение | Серьёзность | Примечания |
|-------|----------|----------|-------|
| Clippy dead_code: варианты LadderPosition | ladder.rs | Низкая | Staged types, явные #[allow] комментарии |
| Clippy dead_code: ThemeAnchors::for_vc | theme_anchors.rs | Низкая | Forward-staged для будущей ревизии |
| Clippy dead_code: BackdropBoundV1, BackdropBoxErrorV1 | backdrop_bound.rs | Низкая | R-09 staged types |
| Clippy dead_code: варианты RestorativeAutoErrorV1 | restorative_auto.rs | Низкая | R-07 staged, одиночный TODO отслеживается |
| Clippy dead_code: PropagationRuleV1, BatchScopeViolationV1 | propagation.rs | Низкая | Staged infrastructure |
| MSRV receipt metadata mismatch | CI config | Средняя | Pre-existing с r6; требует отдельного расследования |
| Всего clippy warnings | workspace | Инфо | 23 warnings в labcolors-core lib; все dead_code на staged types |

**Важно:** эти warnings блокируют clean CI signal, но не блокируют приёмку R8 wave 1. Фикс требует отдельной цели (R8 wave 2 или R9).

## d. Критерии приёмки

Бинарный вердикт: ВСЕ ДА = PASS, ЛЮБОЙ НЕТ = FAIL.

| № | Критерий | Гейт |
|---|-----------|------|
| AC-1 | wasm01_contract.rs: все 6 тестов проходят изолированно | `cargo test --test wasm01_contract` = 0 failures |
| AC-2 | wasm01_contract.rs: нет unwrap/panic без message | grep + manual review = 0 violations |
| AC-3 | Expect sweep: 0 bare unwrap в production коде | text_search = 0 non-test hits |
| AC-4 | Expect sweep: FLOOR >= 1410 | test count verification |
| AC-5 | EmptyClass: 0 dangling references to removed classes | grep = 0 hits |
| AC-6 | EmptyClass: AUD-01 matrix consistent with removals | document cross-check |
| AC-7 | Branch hygiene: tree clean per G2 audit | git branch -r = only main + active PR branches |
| AC-8 | No new regressions introduced by R8 wave 1 | diff against fbab4a8 shows only G1-G4 changes |

## e. Рекомендуемые следующие шаги

### При PASS (все AC = ДА):
1. Зафиксировать REVIEW-01-r8 с вердиктом PASS
2. Переключить ACTIVE.md на R8
3. Перейти к планированию R8 wave 2 или R9:
   - **Приоритет A:** Fix pre-existing clippy dead_code warnings (отдельная цель, не смешивать с функциональными изменениями)
   - **Приоритет B:** MSRV receipt metadata mismatch investigation
   - **Приоритет C:** Продолжение WASM-01 contract implementation на основе validated harness

### При FAIL (любой AC = НЕТ):
1. Задокументировать конкретный failed criterion с evidence
2. Создать targeted fix PR (минимальная область, один AC за раз)
3. Повторить verification-runner на исправленном состоянии
4. Не переходить к следующей волне до закрытия всех AC

### Независимо от вердикта:
- Pre-existing issues требуют отдельного трекинга (Issue или R8 wave 2 goal)
- FLOOR baseline обновляется только при изменении количества тестов
- Graphiti запись о результате ревью обязательна (DECISION_WITH_REJECTED_ALTERNATIVES или DEAD_END_WITH_REASON)