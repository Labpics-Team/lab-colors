# Changelog

Все существенные изменения Lab Colors фиксируются в этом файле. Версии npm и
Rust различаются, потому что это разные delivery surfaces одного контракта.

## [Unreleased]

Атомарная numerical-decision граница (#292), exact WCAG 2.2 evaluator для
финальной sRGB8-пары (#284) и bounded complete-feasibility compiler (#295).
Существующая цветовая эмиссия, config fingerprint и adaptive runtime не
меняются, но Rust/npm/Swift transport API и conformance pack изменены;
следующий release обязан получить согласованный 0.x version bump.
Migration-note: [exact alpha / typed Glow](docs/migrations/exact-alpha-glow.md),
дополнение ADR-0004 от 2026-07-12.

### Added

- Versioned `evaluate_wcag22_srgb8` / `evaluate_wcag22_hex` и эквивалентные
  WASM/TypeScript/UniFFI границы. Criterion всегда объявляет клиент; verdict
  возвращает точное terminal evidence без epsilon или округлённого ratio.
- Canonical Q55 artifact и независимый verifier: Decimal directed-rounding +
  integer tightness для 768 строк, полный scan всех `16 777 216` sRGB8-цветов,
  source bindings и SHA-256 live typed registry admission-row.
- npm package несёт byte-exact profile/table/proof в `evidence/`; release
  verifier и clean-install gate повторно проверяют их хэши и содержимое.
- `labcolors-protocol` задаёт единственную versioned bytes→Core→wire границу
  complete-feasibility. `@labpics/colors/compiler` принимает только
  `Uint8Array`, Swift — `Data` или `[UInt8]`;
  обе поверхности сохраняют `Success(Feasible | Infeasible | NotEvaluated)`
  либо typed `Failure` и не воспроизводят математику Core.
- Rust Core принимает также непустой явный конечный набор пар «opaque ID +
  финальный sRGB8», канонизирует точные UTF-8-байты ID и использует тот же
  exhaustive kernel. Возможность включена в default Core, но не проецируется в
  Protocol/WASM/FFI/npm/Swift.
- Только полный `Feasible`-терминал минтит sealed capability выбора. Клиент
  передаёт непрозрачный ID политики и порядок кандидатов; Core полностью
  проверяет декларацию, выбирает первый feasible ID и повторно проверяет его
  строку тем же exact evaluator без heap-allocation после создания source/policy.
- Conformance pack 5.0.0 добавляет ровно одно семейство
  `wcag22-feasibility.json`: exact 7/2/0/92/59, mixed/all NotApplicable,
  typed conflict/resource failures и opaque-ID law. Шесть прежних family
  остаются byte-identical.

### Breaking (npm API)

- Complete-feasibility API и его request/outcome types перенесены из package
  root в `@labpics/colors/compiler`. Offline compiler загружает отдельный WASM
  через `@labpics/colors/compiler/wasm`; runtime dependency cone больше не
  содержит feasibility protocol.

### Breaking (Rust API)

- Удалены `classify_at_least_v1`, `AtLeastDecisionV1` и `DecisionGuaranteeV1`:
  сравнительная «сила гарантии» как данные допускала lossy-схлопывание
  взаимоисключающих исходов.
- `NumericalDecisionV1<T>` стал атомарным: `Determinate { evidence }` |
  `Compatibility { release_id, provenance }` | `Indeterminate { evidence }`.
  Legacy-исход — отдельный `Compatibility` с registered release
  `glow-cam16-ucs-jprime-target-or-max-v1`, а не determinate со слабой
  гарантией; stable outcome с legacy provenance непредставим в типе.
- `NumericalDecisionEvidenceV1::BitExact` запечатан (приватное поле-печать):
  внешний код матчит только с `..`, минт выполняет registry-owned конструктор
  (закреплено compile-fail тестом).
- Все terminal-варианты `NumericalDecisionV1<T>` и
  `GlowDecisionOutcomeV1` запечатаны variant-level `#[non_exhaustive]`:
  подлинное evidence одного site нельзя переупаковать как результат другого;
  внешний match обязан использовать `..`.
- Временный capability V1 заменён единственным public
  `NumericalCapabilityManifestV2`; WCAG site несёт artifact/bound/proof IDs.
  Published `@labpics/colors` 0.10.0 остаётся неизменным, а следующий npm
  release должен мигрировать на V2 явно.
- `RoleSpec::Glow` несёт typed execution mode
  (`NumericalExecutionModeV1::StableOnly` |
  `ExplicitCompatibility { release_id }`); строковый `GlowDecisionProfileV1`
  остался boundary-адаптером, прежние wire keys (`stable-v1`,
  `legacy-platform-dependent-v1`, `bit-exact`) сохранены byte-for-byte.
- Raw WCAG profile/proof JSON больше не поля runtime-профиля: используйте
  `Wcag22ProfileV1::source_json()` / `proof_json()`. Это позволяет linker-у не
  включать отдельно поставляемые evidence-документы в WASM.

### Changed

- WCAG proof envelope переведён на schema 2: SHA всего `lib.rs` заменён
  versioned binding canonical Cargo lib target и двух exact source
  capsules. Profile V1, proof ID `wcag22-srgb8-full-domain-q55-v1` и package path
  `evidence/wcag22-srgb8-q55-proof-v1.json` не меняются: это отдельные version
  domains, а доказанная математика и finite artifact прежние.
- WASM size history стала role-aware: V5 задаёт независимые exact Linux-x64
  size/SHA и рецепты `runtime`/`compiler` с нулевым headroom, не переписывая
  V1–V4. Whole-call V3 проверяет dedicated compiler entry, связывает его с V5
  и native admission V4 и сохраняет детерминированную request/outcome-проекцию.
  `initSync`, прогретая операция, process maxRSS и WASM pages остаются
  наблюдениями, не SLO.
- Native feasibility admission также append-only относительно принятого
  `main`: V1–V3 проверяются в исторических snapshots, а текущий V4 связывает
  artifact SHA
  `3c257c336bc403eee933990fd7188a3b0a6e89d0cbc983aff18846ef76206275`
  с одним точным dependency cone. Source-bound recorder проверяет Git objects,
  SHA-256 verifier/subject-файлов и точный `Cargo.lock` до сборки и после
  запуска; fresh target, пустая Cargo-config hierarchy и закрытая среда
  исключают ambient profile/flags/wrappers. Receipt фиксирует Rust/Cargo 1.96.0,
  SHA-256 обоих toolchain executables и реально запущенного benchmark binary,
  явный feature set и explicit-empty compiler overrides; V3/V4 сохраняют одну
  deterministic scenario/identity-проекцию, поэтому C1 меняет только workspace
  provenance и admission machinery, а не конечный алгоритм; 71 негативная мутация
  проверяет fail-closed границы.
  Сырые наблюдения сохранены без timing threshold, а промежуточные draft-
  артефакты не становятся публичной историей.
- Conformance pack 4.0.0 добавил `wcag22.json`; pack 5.0.0 добавляет только
  versioned complete-feasibility transport family, поэтому `packDigest`
  закономерно изменён. `manifest.numericalCapabilities` зеркалит single public
  V2 core manifest (coverage `migrated-sites-only-v1`, FNV-1a-32
  drift-checksum над canonical length-prefixed preimage).
- Release manifest schema v3 сохраняет введённую в V2 секцию
  `numericalCapabilities` и добавляет упорядоченные exact records обеих
  WASM-ролей и build metadata. Publish read-back перепроверяет их по байтам
  tarball; Swift conformance-тесты по-прежнему независимо пересчитывают
  capability checksum.
- Добавлен компилируемый numerical plan (`compile_numerical_plan_v1`) с
  канонической invocation identity и checksum — типизированная проекция того,
  какие site/mode заявляет сборка.

## [@labpics/colors 0.10.0 / Rust 0.2.0] - 2026-07-11

Breaking release относительно `@labpics/colors` 0.9.1 / Rust 0.1.0. Пошаговый
переход и rollback: [exact alpha / typed Glow](docs/migrations/exact-alpha-glow.md).

Release packer воспроизводимо закреплён на Node 24.14/npm 11.9; публичный
consumer contract отделён и проверяется с первого Node 22 LTS — `22.11.0`.

### Breaking

- Каждый client-owned Glow-рецепт требует explicit `decision_profile`; default
  и silent legacy fallback удалены. Профиль входит в config fingerprint.
- `GlowRole` стал union из determinate `kind: "glow"` и typed terminal
  `kind: "glow-indeterminate"`. Indeterminate не эмитит halo/core/alpha CSS vars.
- Единый `referenceProfile` из pre-release API заменён раздельными
  `compositeProfile` / `compositeGuarantee`, `layerRecipeProfile`,
  `appearanceDiagnosticProfile`, nullable `selectionDiagnosticProfile` и
  `decisionProfile` / `decisionGuarantee`.
- `solve_screen_alpha_for_dj` принимает `GlowDecisionProfileV1` и возвращает
  `NumericalDecisionV1<GlowSolve>`; поля `GlowSolve` доступны через getters.
- `resolve_alpha_analog_hex` возвращает `Result<(String, f64), String>` вместо
  вложенного `Option`. Для валидного домена sRGB8-ответ тотален; недоменная
  alpha возвращает `Err`, а не клампится.
- Публичные continuous/point compositor-границы возвращают `Result` и одинаково
  отвергают нечисловой или внедиапазонный ввод в debug/release.
- Поля `BackdropBox` закрыты; `try_new` и material-функции возвращают typed
  validation/solve errors. `MaterialAlpha` сообщает typed status и numerical
  guarantee вместо неявного `Option`/public fields.
- `labcolors-conformance::generate_solve` и `Pack::generate` теперь возвращают
  `Result<_, PackGenerationError>`: внутренний/неизвестный core failure нельзя
  сериализовать как обычную физическую недостижимость.

### Added

- Exact encoded-sRGB8 source-over и screen profiles с bit-exact composite
  certificate, binary64 identity alpha и канонической `alphaCss`.
- Machine-readable registry branch-sensitive numerical sites и typed
  `Determinate` / `Indeterminate` с причиной и sound bounds либо честным
  `bounds: unavailable`.
- Material alpha несёт `BisectionBracketCharacterizedV1`: выбранный после 60
  шагов upper candidate повторно проверен, lower candidate не держит floor, а
  numerical profile равен
  `encoded-srgb-byte-scale-affine-platform-binary64-powf-v1`.
  Transparent/opaque endpoints имеют отдельные typed variants; wire несёт
  primary `alphaStatus`.
- Determinate Glow сообщает отдельные halo/core point-композиты и diagnostic
  `|ΔJ′|`, provenance recipe/appearance/selection, target status, constraint
  layer и классы гарантий.
- Conformance pack 2.0.0 с alpha half-tie-вектором.
- Публикуемый `labcolors-core` теперь несёт package-local README; CI проверяет
  реальный `.crate`, распаковывает его и исполняет doctest вне workspace tree.
- `@labpics/colors/build-metadata.json` экспортирует machine-readable связь
  npm/core versions, source SHA, conformance hashes и точных WASM bytes/hash;
  release verifier повторно сверяет её после чистой установки tarball.

### Fixed

- Source-over half-tie считается в byte-reference порядке: `#C0B2FA @ 0.122`
  над `#000000` даёт `#17161F`, без потери соседнего LSB при нормализации.
- Glow перечисляет first-passing alpha фактического binary64 screen-композитора:
  алгебраически равные rational walls больше не склеивают достижимый ULP-seam
  state. Доказанная граница потока — ≤ 766 states; tight maximum не заявлен.
- Material-alpha использует тот же byte-scale affine порядок
  `B + α·(T−B)`, что официальный JS-потребитель. На seam fixtures
  `#020202/floor=3` и `#000000/floor=7` прежний normalized-expanded upper
  candidate не проходил повторную consumer-проверку на один шаг binary64.
- Material all-backdrop recheck больше не полагается на ложную двухугловую
  монотонность: conservative binary64 channel envelope включает обе стороны
  downward seam frozen WCAG 2.1 (2018) EOTF `0.03928`. Directed-search guard
  ограничивает область бисекции; первый passing state и точная минимальная alpha
  не заявляются.
- Glow alpha больше не округляется независимо от выбранного sRGB8-state:
  `alphaCss` round-trip восстанавливает ту же binary64 alpha и тот же композит.
- Нетривиальный stable CAM16 target/max-site без sound error bound больше не
  получает правдоподобный platform-selected verdict: результат typed
  `Indeterminate`.
- Exact stable no-op больше не маркируется как CAM16-selected verdict:
  `selectionDiagnosticProfile` равен `null`. Полный semantic result отдельно
  сохраняет non-null `appearanceDiagnosticProfile`, потому что сообщает
  CAM16-derived `coreAchievedDj`, и versioned `layerRecipeProfile`.
- `BackdropBox::try_new` различает reversed, non-finite и out-of-range bounds
  typed ошибками без swap, clamp или debug-паники.
- Core-generated Glow/Material postcondition failures отделены от
  `InvalidInput`: WASM возвращает whole-call `internal_error`, UniFFI —
  `IncompatibleCoreContract`, conformance generation останавливается до записи
  артефакта вместо generic `"unreachable"` fallback.
- CSS namespace preflight теперь защищает основное имя каждого client-owned
  токена, включая `Zero` и алиасы на него. Производные ключи Glow (`-core`,
  `-alpha`) и Material (`-01`, `-02`) больше не могут молча записать цвет в
  `cssVar` роли с `kind: "none"`; конфликтующий конфиг отклоняется до резолва.
- Устаревший wasm32-only muddiness snapshot (`olive > 0.80`) больше не является
  мёртвым `#[test]`: реальный headless-browser gate проверяет публичный WASM-
  метод по единственному committed conformance corpus без второго semantic
  threshold или hand-written score.
- Исторические API `muddiness` / `drab` / `n_pure` и их conformance corpus
  сохраняют численную совместимость, но явно помещены в quarantine как frozen
  experimental compatibility proxy / research coordinate. Это не
  observer-validated human cleanliness verdict и не сигнал базового compiler /
  adaptive runtime; отклонённый provenance констант зафиксирован в inventory
  под владельцами #231 / #242.

### Compatibility evidence

- Explicit `legacy-platform-dependent-v1` сохраняет прежний CAM16/libm path и
  CSS-эмиссию, но не маркируется stable numerical guarantee.
- В закреплённом Lab UI corpus изменились ровно 24 Glow alpha-листа
  `resolveVars` (4 ключа на 6 записях); 1376 пар `(lc, wcag)` recheck остались
  байт-идентичны. Это ограниченное свидетельство для pinned corpus, не
  универсальная гарантия произвольного клиента.
- После перехода с rational-wall midpoint на actual binary64 partition внутри
  тех же 24 листов скорректированы 17 alpha-строк; в pinned corpus их point-
  композиты и все остальные vars не изменились.

### Known limits

- Для нетривиального CAM16 target/max-выбора sound cross-runtime bound пока не
  установлен; `stable-v1` намеренно возвращает `Indeterminate`.
- Exact point-композит не является сертификатом browser color-management,
  дисплея/HDR или пространственного blur/overlap-эффекта.
- Material-alpha bisection характеризует повторно проверенный fail/pass bracket
  в указанном numerical profile; это не proof глобальной монотонности, первого
  passing state, точной минимальной alpha, predecessor или sound cross-runtime
  bound. Профиль фиксирует original WCAG 2.1 (2018) split `0.03928`; текущая
  формула W3C с `0.04045` требует отдельной версионированной миграции (#284).
