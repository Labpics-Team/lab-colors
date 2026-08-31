---
track: lab-colors
revision: 8
supersedes: r7
owns:
  - "Labpics-Team/lab-colors:**"
  - "Labpics-Team/agents-config:plans/lab-colors/reference/**"
created: 2026-08-31
status: DRAFT
---

# R8 Plan: WASM-01 Completion и Technical Debt Closure

**Evidence cutoff:** 2026-08-31T00:00:00Z (lab-colors main HEAD 70ac4e2, FLOOR=1410).

## R7 Completion Summary

r7 выполнен в полном объёме. Все goals wave 1 (Goals 1–4) и wave 2 (Goal 5 WASM-01 first slice) завершены и слиты в main.

| Goal | Статус | Результат |
|---|---|---|
| G1: Python proof test failures | ✅ COMPLETE | Классифицированы как out-of-scope для Rust track; заведены Issues с RCA |
| G2: Artifact class characterization (4/14) | ✅ COMPLETE | 14/14 классов охарактеризованы; ParallelSsot → FiniteManifest (floor test добавлен); ResourceDimension → NotApplicable; GraphArtifactTest → EmptyClass; расхождение 4→3 устранено |
| G3: WASM-01 contract preconditions | ✅ COMPLETE | Input spec определён для 12/14 классов; boundary types mapped |
| G4: #[non_exhaustive] gate | ✅ COMPLETE | Applied to restorative_auto.rs; TODO удалён; тесты зелёные |
| G5: WASM-01 first slice | ✅ COMPLETE | program_wire.rs + labcolors-wasm boundary реализованы |

**FLOOR baseline:** обновлён с 1360 до 1410 (+50 тестов из G2 ParallelSsot floor + G5 WASM boundary tests).

**REVIEW-01-r7:** PASS по обеим осям (plan-contract, future-axis).

## R8 Goals

Приоритезация основана на r7-tech-debt-audit.md и follow-up actions из r7-artifact-class-characterization.md.

### Goal 1: Dead Code Sweep — Staged Type Activations and Pruning

**Objective:** Оценить 30 `#[allow(dead_code)]` suppressions в crates/; активировать типы, чья ревизия наступила (R-07/R-08), удалить типы, чья ревизия была отменена или superseded. Уменьшить количество suppressions до ≤10 обоснованных.

**Acceptance criteria:**
- Каждый из 30 suppressions имеет disposition: ACTIVATED (тип включён в использование + тесты), PRUNED (удалён), или RETAINED (с обновлённым комментарием и обоснованием)
- Количество `#[allow(dead_code)]` в crates/ ≤ 10 после sweep
- CI зелёный после каждого атомарного PR
- FLOOR не регрессирует

**Effort:** 6–10h
**Dependencies:** None
**Source:** r7-tech-debt-audit.md §Dead Code Suppressions; r7-artifact-class-characterization.md §Follow-up Actions item 1 (dependencies_source.rs removal)

### Goal 2: Branch Hygiene — Audit and Clean 47 Unmerged Branches

**Objective:** Классифицировать 47 unmerged remote branches на ACTIVE / ABANDONED / MERGED-VIA-OTHER. Удалить abandoned и merged-via-other ветки. Для active веток подтвердить владельца и актуальность.

**Acceptance criteria:**
- Каждая из 47 веток имеет классификацию в docs/r8-branch-audit.md (или аналогичном артефакте)
- Abandoned ветки удалены из remote
- Merged-via-other ветки удалены из remote
- Active ветки имеют подтверждённого владельца и linked Issue/Plan node
- EXT series (9 веток) полностью разобраны: ext09 standalone claims extractor оценён на необходимость мержа или удаления

**Effort:** 4–6h
**Dependencies:** None
**Source:** r7-tech-debt-audit.md §Unmerged Branches; review-01-r6-validation.md Note 2 (EXT-06 standalone)

### Goal 3: Enum Cleanup — Remove GraphArtifactTest Variant

**Objective:** Удалить enum variant `ArtifactClass::GraphArtifactTest` (EmptyClass per r7 characterization) из types.rs, dispose.rs, enumerate.rs и всех match sites. Подтвердить отсутствие потребителей через компиляцию и тесты.

**Acceptance criteria:**
- `ArtifactClass::GraphArtifactTest` удалён из enum definition
- Все match arms обновлены (компиляция без ошибок благодаря #[non_exhaustive] из r7 G4)
- Тесты зелёные; FLOOR не регрессирует
- dispose.rs::as_str() и enumerate.rs::class_discriminant() не содержат упоминаний

**Effort:** 2–3h
**Dependencies:** Goal 1 (dead code sweep может выявить дополнительные references)
**Source:** r7-artifact-class-characterization.md §GraphArtifactTest → EmptyClass; Follow-up Actions item 2

### Goal 4: WASM-01 Contract Finalization и Integration Test Harness

**Objective:** Завершить WASM-01 контракт на основе r7 G3 preconditions + r7 G5 first slice. Создать integration test harness, проверяющий round-trip: native audit → WASM boundary serialization → deserialization → projection equality.

**Acceptance criteria:**
- WASM-01 input spec document финализирован в docs/wasm01-contract.md (или plans/lab-colors/reference/)
- Integration test harness покрывает все 12 consumed artifact classes
- Round-trip property test: serialize(deserialize(x)) == x для каждого класса
- Sabotage controls: corrupted boundary data вызывает детерминированную ошибку, не silent corruption
- FLOOR обновлён если добавлены тесты

**Effort:** 8–12h
**Dependencies:** Goal 3 (enum cleanup упрощает serialization matrix)
**Source:** r7-artifact-class-characterization.md §Impact on WASM-01 Contract; wasm01-preconditions.md (не существует на main — контент интегрирован в r7 G3 output)

### Goal 5: Independent Review of r8 Plan

**Objective:** Пройти plan-contract и future-axis review на этом draft перед ACTIVE switch.

**Acceptance criteria:**
- Обе оси ревью возвращают PASS
- Все findings resolved или explicitly accepted с rationale
- REVIEW-01-r8 документ создан

**Effort:** 2–3h
**Dependencies:** Goals 1–4 scoped

## Dependencies

| Входной артефакт | Источник | Использование в r8 |
|---|---|---|
| r7-tech-debt-audit.md | main docs/ | Приоритезация G1 (dead code), G2 (branches) |
| r7-artifact-class-characterization.md | main docs/ | G3 (enum cleanup), G4 (WASM scope = 12/14 classes), Follow-up Actions |
| review-01-r6-validation.md | main docs/ | G2 (EXT-06 standalone оценка) |
| draft-r7.md | main docs/ | Контекст delta, DAG структура, rollback protocol |
| wasm01-preconditions.md | main docs/ (HEAD 70ac4e2) | G4: input spec, gap analysis (§3), recommended scope (§4), acceptance targets (§4.3) |
| r7 G5 WASM-01 first slice (program_wire.rs + labcolors-wasm) | main HEAD 70ac4e2 | G4 базовая реализация |
| FLOOR baseline = 1410 | CI gate | Regression floor для всех goals |

## Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Dead code sweep активирует тип с скрытыми зависимостями, ломающий downstream | Medium | Medium | Атомарные PR per type group; CI gate на каждый PR; rollback = revert single commit |
| Branch hygiene удаляет ветку, которая оказалась active без linked Issue | Low | High | Классификация требует подтверждения владельца перед удалением; 7-day grace period для contested branches |
| GraphArtifactTest removal ломает external consumer (если есть) | Low | Medium | #[non_exhaustive] из r7 G4 гарантирует compile-time catch; grep по Labpics-Team org перед удалением |
| WASM-01 round-trip test выявляет lossy serialization в существующем first slice | Medium | High | Это expected discovery; fix входит в G4 scope, не блокирует plan |
| EXT-06 standalone extractor на ext09 содержит coverage, отсутствующую в bundled версии | Low | Medium | G2 включает diff analysis ext09 vs main; если gap найден — отдельный PR merge, не scope creep |

## Rollback Protocol

- **r8 plan before ACTIVE switch:** Закрыть plan PR; ACTIVE.md остаётся на r7.
- **Individual goal:** Standard git revert per merged PR; FLOOR updated if test count affected.
- **Full r8 rollback:** Revert in reverse merge order; verify FLOOR gate passes at each step.
- **Branch hygiene rollback:** Deleted branches recoverable via reflog for 30 days; audit log сохраняется в docs/r8-branch-audit.md.

## Unknowns

- Сколько из 30 dead_code suppressions относятся к R-08+ staged types (не подлежащим активации в r8).
- Содержит ли ext09 standalone claims extractor тесты, отсутствующие в bundled EXT-09 на main.
- Есть ли external consumers ArtifactClass enum вне Labpics-Team/lab-colors (влияет на безопасность удаления GraphArtifactTest).
- Требует ли WASM-01 round-trip harness дополнительных boundary types beyond WasmBoundary/NativeBoundary из EXT-07.