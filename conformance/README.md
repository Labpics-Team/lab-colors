# Conformance-пак labcolors

Платформо-нейтральный контракт проверки биндингов движка. Пак задаёт набор
векторов «вход → канонический выход» и правила сравнения; конкретная поверхность
называется **conformant** только после того, как действительно воспроизвела
каждый вектор заявленной версии в зафиксированной runtime-матрице. Наличие пака
само по себе не доказывает, что все существующие поверхности уже прошли его
целиком.

Это опора динамической архитектуры: рантайм-ядро на каждой заявленной платформе
обязано пройти правила сравнения пака; запечённые build-time токены — лишь
оптимизация и проверяются против того же контракта.

## Версионирование

- **Версия пака** (`manifest.packVersion`, сейчас `10.0.0`) — семантическая
  версия СХЕМЫ и состава векторов: пять семейств (`contrasts`, `ladders`,
  `alpha`, `solve`, `wcag22`), их байты запинены SHA-256 в
  `crates/labcolors-conformance/tests/pack_v10_contract.rs`.
  Удаление/изменение семейства = major-bump с
  absence-законом на снятые файлы; история составов — в git.
- **Версия ядра** (`manifest.coreVersion`, для этого пака `0.2.0`) — версия
  `labcolors-core`, из канона которой сгенерированы значения. Пак действителен
  ровно для этой версии ядра; при легитимной смене канона (значения
  якорей/ручек, формулы) генератор перегенерирует векторы и `coreVersion`
  сдвигается.
- **Дайджест** (`manifest.packDigest`) — FNV-1a-32 над сырыми байтами пяти
  семейств (в порядке `contrasts, ladders, alpha, solve, wcag22`).
  Отпечаток КОНКРЕТНОГО закоммиченного артефакта. Зависит от платформы
  генерации (последний ULP f64 в сериализации) — не кросс-платформенный
  инвариант, а якорь целостности файлов.

## Семейства векторов (`vectors/*.json`)

| Файл | Что фиксирует | Схема элемента |
|------|---------------|----------------|
| `contrasts.json` | контраст (fg,bg,тема) | `{fg, bg, theme, lc, wcagRatio}` |
| `ladders.json` | альфы позиции лестницы | `{position, alphaLight, alphaDark}` |
| `alpha.json` | подложка→α | `{tint, alpha, bg, composite, minAlpha}` |
| `solve.json` | резолв контракта | `{bg, contract, theme, outcome}` |
| `wcag22.json` | финальная sRGB8-пара и явно выбранный критерий WCAG 2.2 | `{foreground, background, criterion, decision, *Q55, evidence*}` |
| `manifest.json` | метаданные и capability manifest численных решений | `{packVersion, coreVersion, packDigest, counts, numericalCapabilities}` |

Точные решения нейтральной оси — не параметры solver-а, а вычисленные
мощности допустимых подмножеств полной 256-точечной оси `#000000…#FFFFFF`:

- для normal text 4.5:1 против `#767676` проходят 7 кандидатов:
  `#000000…#040404` и `#FEFEFE…#FFFFFF`; против black+white проходят только
  `#757575…#767676` (2), а добавление `#767676` к black+white даёт пустое
  пересечение (0);
- для каждого из трёх критериев с отношением 3:1 против `#767676` проходят
  `#000000…#2D2D2D` и `#D2D2D2…#FFFFFF` (92); против black+white —
  `#5A5A5A…#949494` (59).

Их независимый exact-rational oracle — `scripts/verify_wcag22_neutral_axis.py`;
его content-bound результат —
`crates/labcolors-core/contracts/wcag22-neutral-axis-oracle-v1.json`, запиненный
по SHA-256 и replay-имый через публичный exact-вычислитель
(`crates/labcolors-core/tests/wcag22_neutral_axis_replay.rs`). Любое изменение
adjacent bytes или нормативного отношения пересчитывает набор, а не сохраняет
эти числа как константы.


- `theme` — kebab-ключ ЛОКАЛЬНОГО fixture-словаря пака (совпадает со словарём
  labui-паспорта): `light` \| `dark` \| `light-ic` \| `dark-ic`. Канонический
  словарь тем принадлежит клиентскому конфигу (C5.1); ядро встроенных имён
  не несёт.
- `contract` (в `solve`): `{kind:"text", lc}` \| `{kind:"ui", lc}` \|
  `{kind:"range", floor, ceiling}`.
- `outcome` (в `solve`): успех `{kind:"solved", hex, lc, wcagRatio, floorOverride}`
  или типизированный терминальный исход `{kind:"failure", category, code}`.
- `(category, code)` — атомарная core-owned классификация, общая для всех
  биндингов: `unreachable/exceeds_range`, `unreachable/floor_unreachable`,
  `unresolved/bounded_search_exhausted`, `rejected/invalid_input`,
  `unreachable/below_contrast_floor` и `unsupported/gamut_unsupported`. Только
  `unreachable` доказывает отсутствие
  решения в объявленном полном domain; `unresolved` не делает утверждения о
  непроверенных кандидатах.
- `alpha.json` начиная с pack `2.0.0` обязательно содержит точный byte-reference
  half-tie `#C0B2FA @ 0.122` над `#000000` → `#17161F`. Это mutation-killer
  старого пути `(byte/255) · alpha · 255`, который выбирал соседний LSB.
- `manifest.numericalCapabilities`, введённый в pack `3.0.0`, переведённый на
  proof-capable schema V2 в pack `4.0.0` и сохранённый в `5.0.0`, генерируется из
  proof-capable core-owned `numerical_capability_manifest_v2()`. До появления
  внешних клиентов промежуточная Glow-only capability-схема V1 удалена из
  public API: один `numericalCapabilityManifest()` сразу возвращает V2, без
  второго version-suffixed entrypoint. Это намеренная pre-client breaking
  коррекция ложной схемы, а не поддержка двух конкурирующих контрактов. Форма V2:
  `{schemaVersion, coverage, sites[], checksum}`, где `schemaVersion` —
  независимый version domain capability-схемы (сейчас `2`); `coverage` —
  `migrated-sites-only-v1` (перечислены только **уже мигрированные**
  branch-sensitive sites, не утверждение полного аудита исторических
  `f64`-ветвлений — он остаётся в scope #291); каждая строка `sites[]` несёт
  `siteId` и семь списков стабильных ключей (`stableOutcomes`,
  `compatibilityReleases`, `evidenceClasses`, `artifactIds`, `boundIds`,
  `proofIds`, `runtimeAttestations`; пустой список — явное «evidence отсутствует»,
  не пропуск); `checksum` — FNV-1a-32 (8 lowercase hex) над canonical
  length-prefixed preimage с домен-сепаратором
  `labcolors.numerical-capability.v2`. Release verifier и Swift-тесты
  пересчитывают checksum НЕЗАВИСИМО от Rust-кода. Manifest содержит
  `glow-target-or-maximum-v1` и proof-bound `wcag22-srgb8-contrast-v1`.
  Отдельно full-domain WCAG proof несёт SHA-256 private admission-row: ровно
  десять live typed полей, которые разрешают mint terminal evidence, включая
  `boundStatus` и `fallbackStatus`. Это site-local proof binding, а не новые
  public capability-поля и не FNV checksum всего manifest.

Словарь **позиций лестницы** (не ролей): `label-*`, `fill-*`, `border-*`,
`focus-ring`, `glow`, `skeleton-*`, `neutral-fill-*`, `neutral-border-*`,
`shadow-*`. Пак НЕ вводит роль `icon` — иконки и текст всегда красятся
labels (канон labui): роли `icon` в словаре нет.

## Критерий conformance

Биндинг conformant по версии пака `X`, если на КАЖДОМ векторе его выход
совпадает с каноном по этим правилам:

- **Числовые поля** (`lc`, `wcagRatio`, `alpha`, `minAlpha`, `alpha*`) —
  в пределах `DRIFT_TOL = 1e-6` (абсолютная). Для зависимых от libm путей
  (`powf`/`atan2`/`ln`) битовая идентичность f64 между платформами не
  гарантируется: реализации разных ОС/архитектур могут расходиться на несколько
  ULP (~1e-13); реальный дрейф (не тот surround, опечатка в матрице, путаница
  единиц) сдвигает значения на целые единицы — на порядки выше толерантности.
  Единственный источник этого значения для пака —
  `crates/labcolors-conformance/src/lib.rs` (`DRIFT_TOL`).
- **`composite` (hex)** — ТОЧНО относительно объявленного encoded-sRGB8
  operation profile: фиксированный порядок IEEE binary64
  умножения/сложения и квантования задаёт byte-reference. Это контракт профиля,
  а не заявление о доказанной идентичности произвольной платформы; исполняемое
  evidence ограничено фактически аттестованной runtime-матрицей ниже.
- **`solve.outcome.hex`** — в пределах **±1 LSB на канал**. Это квантование
  трансцендентного резолва: у границы 8-бит-ячейки libm-шум может качнуть
  результат на один шаг.

Внутренняя ошибка core и неизвестный forward-вариант без public boundary
descriptor не являются solve-векторами: `Pack::generate()` возвращает
`PackGenerationError` и не пишет правдоподобный failure fallback в
сертификационный артефакт.
- **Строки/enum/bool** (`theme`, `position`, `category`, `code`, `floorOverride`, `kind`) —
  ТОЧНО.

## Референс: ядро само себя проходит

`crates/labcolors-conformance` несёт генератор (`--bin gen`) и раннер-референс
(`tests/reference_runner.rs`). Раннер — CI-гейт: ядро воспроизводит каждый
вектор в пределах толерантности, дайджест сходится с сырыми байтами, а
опубликованные WCAG-якоря (21:1, граница `#767676`) держатся. Раннер входит в
`cargo test --workspace` на Linux x86_64. Активный Swift/UniFFI gate также
прогоняет все перечисленные manifest-ом семейства в pinned Linux x86_64
container.

Активный browser-gate теперь воспроизводит каждый вектор всех перечисленных
manifest-ом семейств внутри фактического wasm32 core runtime; anti-vacuum total
вычисляется из длин самих replayed family files, а не поддерживается отдельным
числом. Targeted parity-тесты отдельно проверяют публичную JS-границу. Это
доказывает wasm32-исполнение ядра против независимых байтов пака, но ещё не
прогоняет каждый вектор непосредственно через публичный JS API. Поэтому полная
conformance именно JS-поверхности текущего пака пока не заявляется. В
`native-conformance.yml` сохранён ручной macOS/arm64 reference path, но он не
запускается на PR/push и не считается достигнутой аттестацией текущего пака.
Полная runtime-матрица остаётся scope #258; допуск `DRIFT_TOL` задаёт правило
сравнения и не заменяет отсутствующий прогон.

## Регенерация

При легитимной смене канона ядра:

```sh
cargo run -p labcolors-conformance --bin gen
```

Пишет `vectors/*.json` + `manifest.json` детерминированно. Раннер-референс
падёт, если старые численные векторы разошлись с ядром за пределами
толерантности. Перегенерация допустима только при принятом изменении соответствующего
контракта; `coreVersion` обновляется лишь вместе с версией ядра.
