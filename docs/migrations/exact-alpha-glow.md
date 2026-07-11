# Миграция exact alpha / typed Glow

> Роль: breaking-migration guide для `@labpics/colors` 0.10.0 и Rust workspace
> 0.2.0 с предыдущих `@labpics/colors` 0.9.1 / Rust 0.1.0.

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
CAM16: результат determinate, `targetStatus: "unreachable"` и
`decisionGuarantee: { kind: "bit-exact" }`.

### Determinate Glow

По сравнению с 0.9.1 determinate `kind: "glow"` сохраняет прежние поля и
добавляет:

| Поле | Смысл |
| --- | --- |
| `alphaCss` | Каноническая shortest-roundtrip строка той же binary64 alpha; CSS var копирует её буквально. |
| `constraintLayer` | Слой цели; сейчас `"halo"`. |
| `targetDj` | Запрошенный point `|ΔJ′|`. |
| `targetStatus` | `"reached"` либо `"unreachable"`. |
| `haloCompositeHex` | Exact reference point-композит halo. |
| `haloAchievedDj` | Diagnostic `|ΔJ′|` этого halo-композита. |
| `coreCompositeHex` | Exact reference point-композит core при той же alpha. |
| `coreAchievedDj` | Diagnostic `|ΔJ′|` этого core-композита. |
| `compositeProfile` | `"encoded-srgb8-screen-v1"`. |
| `compositeGuarantee` | `"bit-exact"`. |
| `diagnosticProfile` | `"cam16-ucs-jprime-li2017-v1"` либо `null`; `null` у exact no-op, где appearance model не выполнялась. |
| `decisionProfile` | Профиль, явно выбранный конфигом. |
| `decisionGuarantee` | Tagged certificate semantic verdict: `{kind:"bit-exact"}` для exact no-op, `{kind:"legacy-platform-dependent-v1"}` для compatibility path либо `{kind:"outward-interval-v1", lower, upper}` с самим доказанным outward interval. |

`achievedDj` остаётся deprecated alias на `haloAchievedDj`, а `degraded` —
deprecated alias на `targetStatus === "unreachable"`. Новый код должен читать
типизированные поля.

В промежуточном pre-release контракте #269 девятым новым полем был единый
`referenceProfile`. В финальном 0.10.0 его нет: замените его пятью отдельными
полями `compositeProfile`, `compositeGuarantee`, `diagnosticProfile`,
`decisionProfile`, `decisionGuarantee`. Иначе exact-гарантия композитора
ошибочно распространяется на CAM16 decision.

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

## 4. Обновите Rust Glow API

`solve_screen_alpha_for_dj` теперь принимает обязательный
`GlowDecisionProfileV1` и возвращает `NumericalDecisionV1<GlowSolve>`:

```rust
let decision = solve_screen_alpha_for_dj(
    tint,
    background,
    target_dj,
    GlowDecisionProfileV1::StableV1,
    viewing_conditions,
)?;

match decision {
    NumericalDecisionV1::Determinate { value, guarantee } => {
        emit(value.alpha_css(), guarantee);
    }
    NumericalDecisionV1::Indeterminate { site_id, evidence } => {
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

`DecisionGuaranteeV1` тоже `#[non_exhaustive]`. На WASM-границе он сериализуется
tagged object, а не строкой:

```ts
type GlowDecisionGuaranteeV1 =
  | { readonly kind: "bit-exact" }
  | {
      readonly kind: "outward-interval-v1";
      readonly lower: number;
      readonly upper: number;
    }
  | { readonly kind: "legacy-platform-dependent-v1" };
```

Нельзя отделять `lower`/`upper` от outward-варианта или принимать неизвестный
`kind` как `bit-exact`.

Поля `GlowSolve` закрыты; используйте getters `alpha()`, `alpha_css()`,
`target_dj()`, `achieved_dj()`, `composite_hex()`, `status()` и
`composite_certificate()`. Exact certificate хранит tint/background bytes,
binary64 identity alpha, каноническую CSS-строку и composite bytes. Он не
сертифицирует браузер, дисплей, blur или пространственное перекрытие.

## 5. Обновите fixtures, mocks и golden

Минимальный список:

1. Добавьте `decision_profile` во все Glow-рецепты JSON и typed builders.
2. Обновите exhaustive `RoleResult` switches для `glow-indeterminate`.
3. Добавьте все 13 determinate-полей из таблицы выше; удалите
   `referenceProfile` из pre-release mocks.
4. Для `stable-v1` проверяйте отсутствие трёх CSS vars, а не snapshot legacy-
   цвета.
5. Сравнивайте alpha через `alphaCss` или побитный parse round-trip; не
   округляйте её до фиксированного числа знаков.
6. Используйте conformance pack 2.0.0. Half-tie
   `#C0B2FA @ 0.122` над `#000000` обязан дать `#17161F`.

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
- `cam16-ucs-jprime-li2017-v1` — identity диагностической модели, не error bound.
- `legacy-platform-dependent-v1` явно допускает зависимость semantic branch от
  platform/libm.
- `stable-v1` сохраняет неопределённость вместо выдуманного epsilon или
  fallback.
- Ни один из этих профилей не сертифицирует реальный browser color-management,
  HDR/display pipeline, blur, overlap или spatial glow field.
