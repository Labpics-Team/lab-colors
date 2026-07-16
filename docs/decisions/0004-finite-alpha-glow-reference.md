# ADR-0004: конечный sRGB8-контракт alpha и point-glow

- Статус: принято
- Дата: 2026-07-10
- Дополнение numerical-decision boundary: 2026-07-11
- Дополнение атомарного результата и compatibility release: 2026-07-12
- Связанные задачи: #41, #218, #221, #223, #233, #241, #258, #259, #281,
  #282, #292

## Контекст

Прежний glow-солвер выполнял фиксированные 48 шагов бисекции. Это требовало
неподтверждённой монотонности CAM16-UCS J′ по alpha. Затем числовая alpha
округлялась до четырёх знаков независимо от уже выбранного sRGB8-композита.
На воспроизводимом входе `halo=#4A8FFF`, `bg=#101012`, `target=2.3006`
граница `1275/35372` превращалась в `0.0360`, возвращала предыдущий байт
`#12151B` и нарушала собственную цель.

У straight-alpha был отдельный дефект той же природы. Нормализованный путь
`(byte/255) · alpha · 255` менял точную половину из-за binary64-округления:
для `#C0B2FA @ 0.122` над чёрным канал `250 · 0.122 = 30.5` обязан дать 31 в
объявленном byte-reference, но старый путь давал 30. Отчётные
`compositeHex`, Lc и WCAG могли относиться к соседнему байту.

## Решение

Непрерывная алгебра опирается на W3C Compositing and Blending Level 1:
[source-over §9.1.4](https://www.w3.org/TR/compositing-1/#porterduffcompositingoperators_srcover)
и [screen §10.1.3](https://www.w3.org/TR/compositing-1/#blending-screen).
Спецификация задаёт операторы, но не нашу политику финального sRGB8-round;
выбор encoded sRGB как blending space и байтовая сетка ниже явно являются
версионированным reference-профилем продукта, а не приписываются W3C или
неизвестному browser/output pipeline.

### 1. Два численных домена не смешиваются

Непрерывная алгебра encoded-sRGB `[0,1]` остаётся для обратного хода alpha и
материальных интервальных расчётов. Финальный эмитируемый композит имеет
отдельный домен `encoded-sRGB8`:

```text
source-over: round(bg_byte + alpha · (tint_byte − bg_byte))
screen:      round(bg_byte + alpha · tint_byte · (255 − bg_byte) / 255)
```

Source-over affine-форма алгебраически равна оператору W3C, но в binary64
монотонна по alpha. Expanded-запись с двумя произведениями на известных ULP-
швах давала PASS→FAIL→PASS и делала lower-bound ложным. Порядок операций
является частью reference-контракта. Он не выдаётся за
гарантию неизвестного renderer или color-management pipeline; такую
применимость проверяют #233/#241.

Эмиссионный alpha-аналог решается прямо на этой конечной сетке. Сначала
проверяется запрошенная alpha: если существует хотя бы один byte-тинт, она не
меняется. Иначе полный lower-bound по упорядоченным битам `f64` находит первый
`binary64`, который проходит тот же reference-композитор; непосредственный
предшественник обязан не проходить. Каналы независимы, допустимый интервал
тинтов находится монотонным поиском, а канонический тинт минимизирует ошибку до
непрерывной инверсии (при равенстве выбирается меньший байт). Поэтому
`alphaCoerced` означает реальную недостижимость эмитируемой пары, а не более
строгой вспомогательной continuous-модели.

Публичные прямые compositor/hex/CSS/glow-границы отвергают NaN, бесконечности и
выход из домена через `Result`. Legacy continuous API
`invert_composite_encoded` / `min_alpha_encoded` / `resolve_alpha_analog`
сохраняют прежний `Option`-контракт до отдельной миграции #41. Различная
семантика debug/release запрещена.

Эмиссионная hex-обёртка `resolve_alpha_analog_hex` сильнее continuous API: для
валидных hex и `requested_alpha ∈ [0,1]` ответ существует всегда, поэтому её
тип — `Result<(String, f64), String>` без внутреннего `Option`. Некорректная
alpha не клампится в другой запрос, а возвращает `Err`.

### 2. Glow перечисляет actual binary64 states, а semantic decision профилирован

Для каждого канала production-перечислитель находит первый representable
binary64-alpha, переводящий публичный reference-композитор в следующий байт.
Поиск — точный lower-bound по упорядоченным положительным `f64`-битам с вызовом
того же `bg + alpha · glow · (255−bg) / 255`, который выполняет официальный
потребитель. Аналитическая рациональная half-wall используется только как
ускоряющий seed: экспоненциальный поиск строит bracket между уже известными
failing/passing endpoints, затем bitwise lower-bound находит точную границу;
корректность от качества seed не зависит. Канальные переходы сливаются только
тогда, когда совпадают их фактические first-passing bits. Это даёт полное
множество достижимых в данном численном профиле RGB-состояний и bit-exact
certificate выбранного
point-композита. Однако порядок этих states по диагностическому CAM16-UCS J′ —
другая задача: она содержит `powf`/libm и влияет на semantic branch.

Ранее рассмотренная рациональная группировка стенок отвергнута для production:
алгебраически равные дроби при разных факторизациях выражения могут разойтись на
несколько ULP. Контрпример `glow=#010200`, `background=#018000` создаёт
достижимый промежуточный state `#018100`, который рациональная группировка
теряла. Поэтому точное для рациональной модели число 763 не является границей
фактического runtime-потока.

Доказанная безопасная верхняя граница — **не более 766 состояний**: каждый
переход увеличивает хотя бы один из трёх байтов, каждый байт может увеличиться
не более 255 раз, плюс существует начальное состояние. Tightness этой границы
не установлена и не требуется контракту. Рабочий перечислитель хранит текущий
state и по одной следующей границе на канал, поэтому его вспомогательная память
O(1); `Vec` остаётся только в тестовом оракуле под `#[cfg(test)]`.

Compatibility profile `legacy-platform-dependent-v1` проверяет states в
порядке alpha и выбирает:

1. первое состояние, где `|ΔJ′| >= target`;
2. если такого нет — глобальный максимум `|ΔJ′|`;
3. при равном максимуме — первое состояние по alpha.

Каждая граница уже определена зафиксированным binary64-выражением в том же
порядке, что официальный JS-потребитель. Production-alpha берётся из
representable внутренности фактического интервала (или из замкнутого singleton
`alpha=1`) и перед возвратом обязана воспроизвести выбранный state.

Этот legacy-выбор не зависит от предположения о монотонности J′, но остаётся
platform/libm-dependent: точного эталона или sound outward bound для CAM16-
ветвления пока нет. Поэтому `stable-v1` на нетривиальном site
`glow-target-or-maximum-v1` не выбирает state и возвращает
`NumericalDecisionV1::Indeterminate { site_id, evidence, .. }`, где evidence —
`SoundBoundUnavailable`. WASM-проекция выводит из этого варианта согласованную
пару `reason: sound-bound-unavailable` + `bounds: unavailable`. Legacy не
включается как fallback — его обязан явно выбрать клиентский контракт.

Единственное stable-исключение не является специальной цветовой эвристикой:
если point screen-композит не может изменить ни один байт при любой alpha,
`ΔJ′ = 0` следует из равенства byte-state. Такой no-op determinate имеет
`bit-exact` guarantee без вызова CAM16. Публичная проекция уже мигрированных
branch-sensitive sites и классов evidence —
`numerical_capability_manifest_v2()` (#281/#284); internal registry остаётся
Core-owned SSOT и не объявляет полный аудит исторических `f64`-ветвлений,
которым владеет #291.

### 3. Alpha канонизируется внутри выбранного интервала

Каждое квантованное состояние имеет интервал `[lower, upper)`, последнее —
`[lower, 1]`. Каноническая alpha — середина интервала: она максимизирует
численный запас до обеих стенок, но не объявляется эстетическим оптимумом.

`alphaCss` — кратчайшая десятичная запись, восстанавливающая тот же `f64`.
CSS-переменная alpha копирует её буквально. Повторное округление downstream
запрещено. Побитовый parse-round-trip проверяется в Rust/JS-тестах; production
не включает универсальный десятичный парсер только ради повторной проверки
контракта стандартного shortest-roundtrip сериализатора.

### 4. Point-метрики не подменяют spatial effect

Цель glow относится только к изолированному halo. Результат отдельно сообщает
композит и `|ΔJ′|` для halo и core, `targetStatus` и `constraintLayer`.
`compositeProfile=encoded-srgb8-screen-v1` вместе с
`compositeGuarantee=bit-exact` описывает только конечный point-композитор.
`layerRecipeProfile=cam16-jprime-oklab-cusp-v1` идентифицирует алгоритм
построения core/halo. `appearanceDiagnosticProfile=cam16-ucs-jprime-li2017-v1`
идентифицирует модель reported `haloAchievedDj`/`coreAchievedDj` и потому
присутствует у полного resolved result. `selectionDiagnosticProfile` называет
модель только если она участвовала именно в target/max выборе; у exact no-op он
равен `null`. `decisionProfile` / `decisionGuarantee` отдельно сообщают, на
каком основании принят semantic verdict. Один общий profile был бы ложным
повышением recipe или диагностической модели до exact-гарантии. Совместный
blur/overlap/backdrop/HDR-эффект без геометрии не сертифицируется; это отдельный
`SpatialField` из #221.

Midpoint J′ используется только как seed Oklab-светлоты, а gamut-boundary
chroma — как recipe core v1. Эмитированный core отдельно измеряется и не
объявляется точным midpoint после смены координат и sRGB8-квантования. Это не
проверенный на наблюдателях «закон красивого свечения».

### 5. Доказательства являются исполняемыми

- все 65 536 пар одного screen-канала исчерпывающе проверяют first-passing
  binary64-разбиение против публичного compositor; отдельный ULP-seam тест
  запрещает склейку алгебраически равных рациональных стенок;
- независимый finite-state oracle проверяет legacy-выбор reached/unreachable;
- source-over и screen при `alpha=0.122` сверяются с независимой точной
  рациональной формулой на всех 65 536 парах байтов каждый;
- JS chain требует нулевого расхождения engine ↔ официальный consumer;
- stable-путь на нетривиальном site возвращает typed `Indeterminate`, не
  обращается к CAM16 и не эмитит CSS-переменные; exact no-op остаётся
  determinate с `bit-exact` guarantee;
- OKLCH и Display P3 проходят решётку с шагом 5 по каждому каналу (около
  140 тысяч цветов на путь, включая края) и полный серый ramp;
- изменение precision имеет быстрые mutation-killer sentinels.

Полный куб из 16 777 216 входов не заявляется пройденным: выборочная решётка не
подменяет exhaustive-доказательство. Дальнейший conformance-gate отслеживает
`#258` и обязан фиксировать target/toolchain, если такой перебор будет добавлен.

## Последствия и миграция

Изменение намеренно breaking до 1.0 и не публикуется как patch (#259).

Rust:

- `composite_over_encoded(...)` теперь возвращает `Result`;
- для финального байтового reference используется `composite_over_srgb8(...)`;
- `screen_layer_over_encoded(...)` остаётся непрерывной алгеброй, а финальный
  point-пиксель вычисляет `screen_layer_over_srgb8(...)`;
- `resolve_alpha_analog_hex(...)` возвращает тотальный для валидного домена
  `Result<(String, f64), String>` вместо `Result<Option<...>, String>`;
- поля `GlowSolve` читаются через getters;
- `solve_screen_alpha_for_dj(...)` требует `GlowDecisionProfileV1` и возвращает
  `NumericalDecisionV1<GlowSolve>`;
- alpha берётся из `alpha_css()`, статус — из `status()`, exact composite — из
  `composite_certificate()`.

WASM/TypeScript:

- каждый Glow-рецепт обязан явно нести `decision_profile`; отсутствие и
  неизвестная строка отвергаются при `loadConfig`, а профиль входит в fingerprint;
- determinate mocks обязаны добавить `alphaCss`, `constraintLayer`, `targetDj`,
  `targetStatus`, оба composite, оба `achievedDj`, а также раздельные
  `compositeProfile`, `compositeGuarantee`, `layerRecipeProfile`,
  `appearanceDiagnosticProfile`, nullable `selectionDiagnosticProfile`,
  `decisionProfile`, `decisionGuarantee`;
- exhaustive union обязан обрабатывать `kind: "glow-indeterminate"`; этот
  terminal outcome несёт site/reason/bounds, но не создаёт halo/core/alpha vars;
- legacy `achievedDj` и `degraded` остаются точными aliases на период миграции;
- изменение строк alpha и соответствующих golden ожидаемо.

При выпуске breaking-версии по #259 общий conformance pack должен быть повышен
до `2.0.0`: alpha-family должна содержать half-tie
`#C0B2FA @ 0.122` над чёрным, поэтому Rust FFI и все нативные биндинги обязаны
вернуть `#17161F`, а старый нормализованный путь больше не считается conformant.

Пошаговый переход и rollback опубликованной версии 0.10.0 сохранены в
content-addressed commit `52d7895774c2fa3796e51f7133452cf83c09346b`.
Текущий ADR владеет только решением и его границами, а не состоянием
последующих работ.

## Дополнение 2026-07-12: атомарный результат и registered compatibility release (#292)

Первая typed-decision граница различала determinate/indeterminate, но
legacy-исход всё ещё выглядел как «determinate со слабой гарантией». Это
позволяло читателю типа повысить платформозависимый выбор до доказанного.
Контракт ужесточён до атомарного результата:

- `NumericalDecisionV1<T>` имеет ровно три взаимоисключающих варианта:
  `Determinate { evidence }`, `Compatibility { release_id, provenance }` и
  `Indeterminate { evidence }`. Legacy-исход — это `Compatibility`, отдельный
  вариант, а НЕ determinate: он идентифицирует зарегистрированный воспроизводимый
  АЛГОРИТМ (`NumericalCompatibilityReleaseIdV1::GlowCam16UcsJPrimeTargetOrMaxV1`,
  key `glow-cam16-ucs-jprime-target-or-max-v1`), а не cross-runtime bit-exact
  значение. Незаконная комбинация (stable outcome с legacy provenance)
  непредставима в типе.
- Determinate-evidence запечатан: у `NumericalDecisionEvidenceV1::BitExact`
  приватное поле-печать, внешний код может только матчить с `..`, а минт
  выполняет registry-owned конструктор, сверяющий capability site. Подделка
  evidence вне ядра не компилируется (compile-fail тест).
- Промежуточные «граничные» классификаторы (`classify_at_least_v1`,
  `AtLeastDecisionV1`, `DecisionGuaranteeV1`) удалены: они кодировали
  сравнительную силу гарантии как данные и тем самым допускали lossy-схлопывание
  атомарных вариантов.
- Клиентский выбор перенесён в typed execution mode
  (`NumericalExecutionModeV1::StableOnly | ExplicitCompatibility { release_id }`),
  который несёт `RoleSpec::Glow`. Строковый `GlowDecisionProfileV1` остался
  boundary-адаптером: прежние wire keys (`stable-v1`,
  `legacy-platform-dependent-v1`, `bit-exact`) сохранены byte-for-byte, wire/npm
  JSON и fingerprint не изменились.
- Conformance pack 3.0.0 публикует `numericalCapabilities` — typed capability
  manifest ядра (coverage `migrated-sites-only-v1`, FNV-1a-32 drift-checksum над
  canonical length-prefixed preimage) вместо прозаического `numericalSites`;
  release verifier и Swift-тесты пересчитывают checksum независимо.

## Дополнение 2026-07-13: Core-owned terminal outcomes и capability V2 (#284)

- Struct-like варианты `NumericalDecisionV1` и `GlowDecisionOutcomeV1`
  запечатаны variant-level `#[non_exhaustive]`. Теперь внешний код не может
  переупаковать подлинное evidence другого site как Glow/WCAG outcome; он
  получает предметный результат только из Core-owned resolver-а.
- Единственная public capability projection —
  `numerical_capability_manifest_v2()`. Internal registry остаётся SSOT, а
  WCAG admission дополнительно SHA-256-связан с десятью фактическими typed
  полями, разрешающими минт bounded evidence.
- Pack 4.0.0 добавляет отдельное `wcag22`-семейство. Это terminal standard
  certificate для явно объявленного criterion, а не новая Glow-эвристика и не
  замена LPC-перцептивной цели.
