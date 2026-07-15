# Миграция exact alpha / typed Glow

> Роль: breaking-migration guide для `@labpics/colors` 0.10.0 и Rust workspace
> 0.2.0 с предыдущих `@labpics/colors` 0.9.1 / Rust 0.1.0.

Release artifact собирается закреплённым Node 24.14/npm 11.9, но это не
consumer requirement: `@labpics/colors` поддерживает `Node >=22.11.0` (первый
Node 22 LTS), и этот floor проверяется отдельным CI job.

Эта версия разделяет два контракта, которые раньше выглядели одним:

1. exact encoded-sRGB8 point-композит и его воспроизводимые байты;
2. semantic target/max-выбор по CAM16-UCS J′, для которого пока нет sound
   cross-runtime error bound.

Обновление breaking: сначала поменяйте конфиг и exhaustive result handling,
затем обновляйте runtime. Не включайте `stable-v1`, если потребитель ещё
предполагает, что каждая Glow-роль всегда создаёт CSS-переменные.

## Версии одного контракта

| Артефакт | Было | Стало |
| --- | --- | --- |
| npm | `@labpics/colors` 0.9.1 | `@labpics/colors` 0.10.0 |
| Rust workspace | 0.1.0 | 0.2.0 |
| conformance pack | 1.0.0 | 2.0.0 |

Версии npm, Rust и conformance pack проверяют разные границы, но для этого
breaking-перехода должны обновляться согласованно.

## 1. Сделайте `decision_profile` обязательным

Каждый client-owned Glow-рецепт теперь обязан явно выбрать numerical-decision
profile:

```json
{
  "name": "client-owned-glow-id",
  "recipe": {
    "kind": "glow",
    "source": { "kind": "brand" },
    "step": "base",
    "decision_profile": "stable-v1"
  }
}
```

Допустимы только две строки:

| Значение | Контракт |
| --- | --- |
| `stable-v1` | Не выбирает нетривиальный CAM16 target/max state без sound bound; возвращает typed `Indeterminate`. |
| `legacy-platform-dependent-v1` | Явно сохраняет прежний CAM16/libm-dependent target/max-выбор и CSS-эмиссию. Это compatibility profile, не stable numerical guarantee. |

Пропущенное или неизвестное значение отклоняется как `invalid_config`; default
и silent legacy fallback отсутствуют. `decision_profile` входит в canonical
fingerprint конфига, поэтому после изменения ожидаем новый fingerprint и новый
cache namespace.

Для закреплённого полного Lab UI паспорта добавление explicit legacy profile
меняет fingerprint с `f2a892a62f7bc91e` на `c51445fcd167781a`. Это ожидаемая
смена identity при сохранении прежней Glow-эмиссии, а не drift якорей. Для
другого клиентского конфига вычисляйте собственный fingerprint — эти две строки
не являются универсальными значениями.

Если главная задача первого rollout — сохранить прежнюю эмиссию, начните с
`legacy-platform-dependent-v1`. Переход на `stable-v1` делайте отдельным
изменением потребителя после добавления ветки `glow-indeterminate`.

## 2. Обрабатывайте Glow как discriminated union

```ts
const role = result.roles[roleId];

switch (role.kind) {
  case "glow":
    // Determinate: vars уже содержат halo/core/alpha.
    renderGlowDiagnostics(role);
    break;
  case "glow-indeterminate":
    // Terminal typed outcome: semantic state и CSS не выбраны.
    reportNumericalIndeterminacy(role);
    break;
  // ...остальные варианты RoleResult...
}
```

Не превращайте `glow-indeterminate` в `unreachable` и не подставляйте последний
цвет, alpha=1 или legacy-результат. Это отдельный terminal outcome о
недостаточности численного доказательства.

### `stable-v1`: нетривиальный запрос

Для текущего branch-sensitive site форма результата такова:

```ts
{
  kind: "glow-indeterminate",
  cssVar: "--lab-client-owned-glow-id",
  sourceHex: "#4A8FFF",
  targetDj: 2.3006,
  constraintLayer: "halo",
  decisionProfile: "stable-v1",
  numericalSiteId: "glow-target-or-maximum-v1",
  reason: "sound-bound-unavailable",
  bounds: { kind: "unavailable" }
}
```

Для этой роли в `ResolvedTheme.vars` нет ключей `cssVar`, `${cssVar}-core` и
`${cssVar}-alpha`. `applyTheme` удаляет переменные, оставшиеся от предыдущего
determinate resolve; собственный DOM-adapter обязан обеспечить ту же очистку.

`stable-v1` не означает «всегда Indeterminate». Если point screen-композит
bit-exact не может изменить фон при любой alpha, target/max-branch не требует
CAM16: результат determinate, `targetStatus: "exact-noop-unreachable"` и
`decisionGuarantee: { kind: "bit-exact" }`.

### Determinate Glow

По сравнению с 0.9.1 determinate `kind: "glow"` сохраняет прежние поля и
добавляет:

| Поле | Смысл |
| --- | --- |
| `alphaCss` | Каноническая shortest-roundtrip строка той же binary64 alpha; CSS var копирует её буквально. |
| `constraintLayer` | Слой цели; сейчас `"halo"`. |
| `targetDj` | Запрошенный point `\|ΔJ′\|`. |
| `targetStatus` | `"exact-noop-unreachable"`, `"legacy-reached"` либо `"legacy-unreachable"`; provenance результата не теряется в общем boolean. |
| `haloCompositeHex` | Exact reference point-композит halo. |
| `haloAchievedDj` | Diagnostic `\|ΔJ′\|` этого halo-композита. |
| `coreCompositeHex` | Exact reference point-композит core при той же alpha. |
| `coreAchievedDj` | Diagnostic `\|ΔJ′\|` этого core-композита. |
| `compositeProfile` | `"encoded-srgb8-screen-v1"`. |
| `compositeGuarantee` | `"bit-exact"`. |
| `layerRecipeProfile` | `"cam16-jprime-oklab-cusp-v1"`; identity алгоритма построения core/halo, не numerical guarantee. |
| `appearanceDiagnosticProfile` | `"cam16-ucs-jprime-li2017-v1"`; non-null у полного resolved result, потому что `*AchievedDj` измеряются этой моделью. |
| `selectionDiagnosticProfile` | `"cam16-ucs-jprime-li2017-v1"` только когда модель участвовала в target/max selection; `null` у exact no-op. |
| `decisionProfile` | Профиль, явно выбранный конфигом. |
| `decisionGuarantee` | Tagged certificate semantic verdict: `{kind:"bit-exact"}` для exact no-op либо `{kind:"legacy-platform-dependent-v1"}` для compatibility path. Outward determinate Glow capability в этом релизе не поддерживается. |

`achievedDj` остаётся deprecated alias на `haloAchievedDj`, а `degraded` —
deprecated alias на status `exact-noop-unreachable` или `legacy-unreachable`.
Новый код должен читать типизированные поля.

В промежуточном pre-release контракте #269 девятым новым полем был единый
`referenceProfile`. В финальном 0.10.0 его нет: замените его семью отдельными
полями `compositeProfile`, `compositeGuarantee`, `layerRecipeProfile`,
`appearanceDiagnosticProfile`, `selectionDiagnosticProfile`, `decisionProfile`,
`decisionGuarantee`. Иначе exact-гарантия композитора ошибочно распространяется
на recipe, CAM16 diagnostics или semantic decision.

## 3. Обновите Rust alpha API

### Эмиссионный hex-resolver стал тотальным для валидного домена

Было:

```rust
let Some((tint, alpha)) = resolve_alpha_analog_hex(solid, requested, bg)? else {
    return handle_absence();
};
```

Стало:

```rust
let (tint, alpha) = resolve_alpha_analog_hex(solid, requested, bg)?;
```

Сигнатура теперь `Result<(String, f64), String>` вместо
`Result<Option<(String, f64)>, String>`. Для валидных hex и конечной
`requested_alpha ∈ [0,1]` exact sRGB8-пара существует всегда; в худшем случае
фактическая alpha равна 1. Неконечная или внедиапазонная alpha возвращает
`Err`, а не клампится.

Continuous `resolve_alpha_analog` сохраняет `Option<AlphaAnalog>`, потому что
его вход — произвольные числовые каналы; `None` означает недоменный ввод. Он
также больше не клампит недоменную requested alpha.

### Прямые compositor-границы возвращают `Result`

Обновите call sites для:

- `composite_over_encoded(...) -> Result<[f64; 3], String>`;
- `screen_layer_over_encoded(...) -> Result<[f64; 3], String>`;
- `composite_over_srgb8(...) -> Result<[u8; 3], String>`;
- `screen_layer_over_srgb8(...) -> Result<[u8; 3], String>`.

Публичный ввод с NaN, infinity или значением вне домена отклоняется одинаково
в debug и release. Не восстанавливайте прежнее поведение через локальный
`clamp`.

### Material alpha сообщает typed outcome и численную границу честно

`BackdropBox` больше нельзя создать struct literal: поля закрыты, а
`BackdropBox::try_new(min, max)` возвращает `BackdropBoxErrorV1` с отдельными
вариантами для reversed, non-finite и out-of-range channel. Границы не
переставляются и не клампятся. `worst_contrast_encoded`,
`solve_material_alpha_encoded` и `solve_material_alpha_hex` возвращают
`Result<_, MaterialSolveErrorV1>`; недостижимый floor остаётся typed domain
outcome `MaterialAlphaStatusV1::Degraded`, а не ошибкой или fallback.

У `MaterialAlpha` используйте getters `alpha()`, `worst_contrast()`, `pole()`,
`status()` и `guarantee()`. Satisfied-результат несёт
`MaterialAlphaGuaranteeV1::BisectionBracketCharacterizedV1`: после 60 шагов
бисекции `lower_alpha` повторно измерен ниже floor, `upper_alpha` повторно
проходит floor, а `alpha()` побитно равен `upper_alpha`. Перед поиском действует
directed-search guard; неподдерживаемое отношение возвращает
`UnsupportedDirectedSearchRelation`. Numerical profile явно равен
`encoded-srgb-byte-scale-affine-platform-binary64-powf-v1`. Это
platform-characterization, а не заявление глобальной монотонности, первого
passing state, точной минимальной alpha, предшественника или sound cross-runtime
bound. Degraded-результат аналогично
несёт `OpaqueEndpointCharacterizedV1` для повторно проверенного `alpha = 1`.
Если floor уже держится при `alpha = 0`, возвращается отдельный
`TransparentEndpointCharacterizedV1`, а не фиктивный threshold bracket. На wire
primary outcome — `alphaStatus`; прежний `guaranteed` остаётся только derived
compatibility alias. `alphaGuarantee` сохраняет tagged evidence и profile.

Если consumer независимо перепроверяет material, он обязан повторить именно
byte-scale affine order `(B_byte + alpha·(T_byte−B_byte))/255`. Прежняя
normalized expanded запись `alpha·T + (1−alpha)·B` алгебраически равна, но не
принадлежит этому binary64 profile и расходится на известных WCAG-швах.
Профиль фиксирует original WCAG 2.1 (2018) split `0.03928` (не текущий W3C
`0.04045`). Для all-backdrop результата ядро расширяет фактические endpoint-
композиты conservative binary64 channel envelope и включает обе стороны
пересечённого EOTF seam; перепроверка только двух углов недостаточна.

## 4. Обновите Rust Glow API

`solve_screen_alpha_for_dj` принимает обязательный typed execution mode и
возвращает `NumericalDecisionV1<GlowSolve>`:

```rust
let decision = solve_screen_alpha_for_dj(
    tint,
    background,
    target_dj,
    GlowDecisionProfileV1::StableV1.execution_mode(),
    viewing_conditions,
)?;

match decision {
    NumericalDecisionV1::Determinate { value, evidence, .. } => {
        emit_stable(value.alpha_css(), evidence.class_key());
    }
    NumericalDecisionV1::Compatibility {
        value,
        release_id,
        provenance,
        ..
    } => {
        emit_compatibility(
            value.alpha_css(),
            release_id.key(),
            provenance.key(),
        );
    }
    NumericalDecisionV1::Indeterminate { site_id, evidence, .. } => {
        match evidence {
            NumericalIndeterminacyV1::SoundBoundUnavailable => {
                record_unbounded(site_id);
            }
            NumericalIndeterminacyV1::IntervalOverlap(interval) => {
                record_overlap(site_id, interval.lower(), interval.upper());
            }
            // `NumericalIndeterminacyV1` is `#[non_exhaustive]`.
            _ => return handle_unknown_numerical_evidence(),
        }
    }
    // `NumericalDecisionV1` is also `#[non_exhaustive]`.
    _ => return handle_unknown_numerical_decision(),
}
```

`evidence` не позволяет собрать логически невозможную пару «reason отдельно,
bounds отдельно»: `SoundBoundUnavailable` не несёт интервал, а
`IntervalOverlap(interval)` обязан владеть валидированным outward interval.
WASM-проекция сохраняет discriminated wire-форму `reason` + `bounds`, но выводит
оба поля из одного enum-варианта. Внешний Rust-код обязан сохранять wildcard-arm
для `#[non_exhaustive]` enum и трактовать неизвестный вариант как явную
несовместимость версии, а не как legacy fallback.

`DecisionGuaranteeV1` удалён: generic Core больше не ранжирует
взаимоисключающие исходы по «силе гарантии». `Determinate` несёт sealed
`NumericalDecisionEvidenceV1`, а explicit legacy-путь — отдельный
`Compatibility { release_id, provenance }`.

На WASM-границе прежний client-facing `decisionGuarantee` остаётся
tagged object и выводится адаптером из атомарного outcome:

```ts
type GlowDecisionGuaranteeV1 =
  | { readonly kind: "bit-exact" }
  | { readonly kind: "legacy-platform-dependent-v1" };
```

Это wire-type, не generic Rust evidence enum. Не принимайте неизвестный `kind`
как `bit-exact` или legacy.

Low-level UniFFI/Swift поверхность не переносит flattened provenance-поля
generic wire-формы. Её `GlowPointDecision` — algebraic sum ровно четырёх
допустимых outcome: `stableExactNoop(value)`, `legacyReached(value)`,
`legacyUnreachable(value)` и `indeterminate(siteId, evidence)`. Общий
`GlowPointValue` хранит только alpha, target/achieved diagnostic, exact
composite hex и composite profile/guarantee. Отдельные native-типы
`GlowDecisionGuarantee`, `GlowTargetStatus`, `GlowDiagnosticProfile` и
одноимённые независимые output-поля удалены: невозможный cross-product нельзя
сконструировать. `GlowDecisionProfile` остаётся обязательным input функции, но
не дублируется в output.

Native adapter заранее валидирует tint, background и конечный `targetDj > 0`;
только такой public input возвращает `ColorError.InvalidGlowRequest`. Если после
успешной проверки core всё же возвращает `Err`, неизвестный forward variant
либо illegal site/release/evidence tuple, граница возвращает
`ColorError.IncompatibleCoreContract`. Тот же закон действует для нового
неизвестного `Unreachable`: adapter не подменяет его строкой
`"unreachable"`. `NumericalIndeterminacy.intervalOverlap` остаётся в Swift как
законное outward evidence для `indeterminate`.

В Swift замените прежний `case .determinate(...)` на exhaustive switch по этим
четырём вариантам; `value` одинаков только по форме composite payload, а смысл
ветки берётся из enum-case. Удалите mocks и helper-типы, которые продолжают
принимать независимые native decision/status/diagnostic поля.

Поля `GlowSolve` закрыты; используйте getters `alpha()`, `alpha_css()`,
`target_dj()`, `achieved_dj()`, `composite_hex()`, `status()` и
`selection_diagnostic_profile()`, `composite_certificate()`. Exact certificate хранит tint/background bytes,
binary64 identity alpha, каноническую CSS-строку и composite bytes. Он не
сертифицирует браузер, дисплей, blur или пространственное перекрытие.

## 5. Обновите fixtures, mocks и golden

Минимальный список:

1. Добавьте `decision_profile` во все Glow-рецепты JSON и typed builders.
2. Обновите exhaustive `RoleResult` switches для `glow-indeterminate`.
3. Добавьте все 15 determinate-полей из таблицы выше; удалите
   `referenceProfile` из pre-release mocks.
4. Для `stable-v1` проверяйте отсутствие трёх CSS vars, а не snapshot legacy-
   цвета.
5. Сравнивайте alpha через `alphaCss` или побитный parse round-trip; не
   округляйте её до фиксированного числа знаков.
6. Используйте актуальный conformance pack (5.0.0; half-tie введён в 2.0.0
   и обязателен с тех пор). Half-tie
   `#C0B2FA @ 0.122` над `#000000` обязан дать `#17161F`. Обрабатывайте
   `generate_solve()` / `Pack::generate()` как `Result`: internal core failure
   теперь возвращает `PackGenerationError`, а не fake `unreachable` vector.
7. Проверьте имена `Zero`-ролей и алиасов на них: их `cssVar` остаётся
   client-owned namespace даже без эмитированного значения и не может совпадать
   с производным ключом Glow (`-core`/`-alpha`) или Material (`-01`/`-02`).
   Такой конфиг теперь честно отклоняется на preflight вместо зависимости от
   порядка JSON-писателей.

В закреплённом Lab UI regression corpus exact alpha меняет ровно 24 листа
`resolveVars`: четыре `--lab-fx-glow-*-alpha` на шести записях. Все 1376 пар
`(lc, wcag)` в секции recheck остаются байт-идентичны. Это характеристика
конкретного зафиксированного corpus, не обещание для произвольного клиентского
конфига.

## Порядок rollout

1. Подготовьте новый конфиг с explicit profile и сохраните snapshot старого.
2. Выпустите потребителя, который понимает оба Glow outcome и очищает stale
   vars.
3. Обновите npm/Rust runtime и conformance pack согласованно.
4. Сбросьте кэш, привязанный к старому config fingerprint. Для полного Lab UI
   паспорта ожидаемый переход — `f2a892a62f7bc91e` → `c51445fcd167781a`;
   отклонение требует проверки фактического JSON, а не ручной подмены пина.
5. Сначала разверните `legacy-platform-dependent-v1`, если требуется
   byte-compatible CSS-поведение; отдельно включите `stable-v1` и наблюдайте
   typed indeterminacy по `numericalSiteId`/`reason`.

## Rollback

Rollback выполняется парой runtime + config:

1. Зафиксируйте до rollout проверенные артефакты npm 0.9.1 / Rust 0.1.0,
   lockfiles и конфиг, который принимала старая schema.
2. Верните старый runtime и старый config snapshot атомарно. Не рассчитывайте,
   что старая версия обязана понимать новый `decision_profile`.
3. Очистите cache namespace нового fingerprint и выполните полный resolve/apply
   для каждого активного root; не переиспользуйте `ResolvedTheme` другой
   версии.
4. Проверьте восстановление halo/core/alpha vars и старого conformance pack до
   открытия трафика.

Данных пользователя эта миграция не преобразует. Опасность rollback — только
смешение несовместимых schema/result contracts и оставшиеся CSS vars.

## Границы гарантии

- `bit-exact` относится к объявленному encoded-sRGB8 point-композитору.
- `cam16-jprime-oklab-cusp-v1` — identity layer recipe, не numerical guarantee.
- `cam16-ucs-jprime-li2017-v1` — identity appearance/selection diagnostic,
  не error bound; наличие в appearance measurement не доказывает selection.
- `legacy-platform-dependent-v1` явно допускает зависимость semantic branch от
  platform/libm.
- `stable-v1` сохраняет неопределённость вместо выдуманного epsilon или
  fallback.
- Ни один из этих профилей не сертифицирует реальный browser color-management,
  HDR/display pipeline, blur, overlap или spatial glow field.

## Историческая migration-note: атомарный `NumericalDecisionV1` и pack 3.0.0 (#292)

> Этот подраздел фиксирует переход #292 до добавления WCAG-семейства. Для
> текущего unreleased-контракта используйте pack 6.0.0 и дополнения ниже.

Последующий rework численной границы (см. дополнение ADR-0004 от 2026-07-12)
намеренно НЕ меняет wire: прежние ключи сохранены byte-for-byte как
boundary-адаптер, поэтому для JS/TS-потребителей и golden-снапшотов миграция
не требуется.

- `decision_profile` в конфиге по-прежнему принимает ровно `stable-v1` |
  `legacy-platform-dependent-v1`; строка парсится адаптером
  `GlowDecisionProfileV1` в typed execution mode
  (`StableOnly` | `ExplicitCompatibility { release_id }`). Fingerprint конфига
  не меняется.
- Wire-ключи гарантий (`bit-exact`, `legacy-platform-dependent-v1`) и форма
  resolved-ролей не изменились. Legacy-результат внутри ядра теперь атомарный
  `Compatibility` с registered release
  `glow-cam16-ucs-jprime-target-or-max-v1` — он никогда не был и не становится
  determinate; адаптер лишь проецирует его в прежний wire-ключ.
- Breaking только Rust API: удалены `classify_at_least_v1`,
  `AtLeastDecisionV1`, `DecisionGuaranteeV1`; сопоставление по
  `NumericalDecisionV1` обязано обрабатывать три варианта
  (`Determinate`/`Compatibility`/`Indeterminate`), а `BitExact`-evidence
  матчится только с `..` (sealed).
- `conformance/vectors/manifest.json` — pack 3.0.0: `numericalSites` заменён
  typed `numericalCapabilities` (coverage `migrated-sites-only-v1`,
  FNV-1a-32 drift-checksum). Векторные семейства и `packDigest` не изменились;
  потребители манифеста должны читать новую секцию.

## Исторический контракт: WCAG 2.2 и pack 4.0.0 (#284)

- Единственный public capability contract — V2; он добавляет proof-capable
  `wcag22-srgb8-contrast-v1` с artifact/bound/proof IDs. Временный V1 не
  сохраняется compatibility alias-ом до появления клиентов.
- Pack 4.0.0 добавляет `wcag22.json`, поэтому `packDigest` меняется. npm API
  добавляет `evaluateWcag22`; profile/table/proof поставляются byte-exact в
  `evidence/` и перепроверяются release gate-ом.
- Все struct-like terminal variants `NumericalDecisionV1` и
  `GlowDecisionOutcomeV1` sealed variant-level `#[non_exhaustive]`: внешний
  Rust-код матчится с `..` и не может переупаковать genuine evidence другого
  site.
- Raw WCAG JSON читается через `Wcag22ProfileV1::source_json()` и
  `proof_json()`. Runtime-профиль хранит только IDs/хэши, поэтому отдельно
  поставляемые документы не дублируются в WASM.
- Proof schema 2 заменяет SHA всего crate-root на versioned semantic route
  binding. Kernel, Q55 source/bin, profile, generator, normalized facade и
  terminal evidence остаются byte-identical; production-only parser-capsule в
  исходном `srgb8.rs` связан exact SHA без роста optimized WASM size или
  code-body lengths, а добавление несвязанного API больше не выглядит сменой
  математики.

## Текущий transport-контракт: complete feasibility и pack 6.0.0 (#295)

- Pack 5.0.0 добавляет ровно `wcag22-feasibility.json`; байты шести family из
  pack 4 сохранены. Новый corpus фиксирует versioned request/outcome bytes,
  packed LSB0 evidence, все три feasibility-терминала и типизированные
  conflict/resource error paths.
- `evaluateWcag22Feasibility(Uint8Array)`, `wcag22FeasibilityMaxBytes()` и
  request/outcome types перенесены из package root в
  `@labpics/colors/compiler`. Compiler загружает собственный WASM через
  `@labpics/colors/compiler/wasm`; runtime WASM больше не содержит feasibility
  protocol. Raw wasm-bindgen ABI обеих ролей остаётся приватным.
- Инициализация теперь также разделена по execution-role: прежний root
  `await init()` инициализирует только runtime. Перед первым compiler-вызовом
  отдельно импортируйте `init` (или `initSync`) из `@labpics/colors/compiler`
  и инициализируйте его собственный WASM; иначе compiler API fail-fast, а не
  использует скрытый runtime fallback. В браузере перенесите compiler import,
  init и evaluate в dedicated module Worker; UI thread загружает только
  runtime.

  ```ts
  const compiler = new Worker(
    new URL("./color-compiler.worker.ts", import.meta.url),
    { type: "module" },
  );
  ```

  Worker-модуль инициализирует `@labpics/colors/compiler`, регистрирует handler
  и только затем отправляет `ready`; main thread передаёт запрос после этого
  сигнала, как показано в package README. Для Node offline tooling можно
  вызывать entry напрямую, передав ему байты compiler WASM. Один и тот же
  модуль для двух ролей не подходит.
- Feasibility полностью перечисляет зарегистрированный домен и не выбирает
  цвет. Selection policy, брендовая близость, polarity и appearance-scoring не
  являются скрытой частью этого migration-контракта.
