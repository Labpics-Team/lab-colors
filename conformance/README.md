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

- **Версия пака** (`manifest.packVersion`, сейчас `3.0.0`) — семантическая
  версия СХЕМЫ и состава векторов. Bump 2.0.0 → 3.0.0 менял только схему
  манифеста (`numericalSites` → `numericalCapabilities`); векторные семейства
  и `packDigest` не изменились.
- **Версия ядра** (`manifest.coreVersion`, для этого пака `0.2.0`) — версия
  `labcolors-core`, из канона которой сгенерированы значения. Пак действителен
  ровно для этой версии ядра; при легитимной смене канона (значения
  якорей/ручек, формулы) генератор перегенерирует векторы и `coreVersion`
  сдвигается.
- **Дайджест** (`manifest.packDigest`) — FNV-1a-32 над сырыми байтами семейств
  (в порядке `contrasts, ladders, alpha, solve, muddiness`). Отпечаток
  КОНКРЕТНОГО закоммиченного артефакта. Зависит от платформы генерации (последний
  ULP f64 в сериализации) — не кросс-платформенный инвариант, а якорь
  целостности файлов.

## Семейства векторов (`vectors/*.json`)

| Файл | Что фиксирует | Схема элемента |
|------|---------------|----------------|
| `contrasts.json` | контраст (fg,bg,тема) | `{fg, bg, theme, lc, wcagRatio}` |
| `ladders.json` | альфы позиции лестницы | `{position, alphaLight, alphaDark}` |
| `alpha.json` | подложка→α | `{tint, alpha, bg, composite, minAlpha}` |
| `solve.json` | резолв контракта | `{bg, contract, theme, outcome}` |
| `muddiness.json` | замороженная legacy-координата `muddiness` | `{hex, score}` |
| `manifest.json` | метаданные и capability manifest численных решений | `{packVersion, coreVersion, packDigest, counts, numericalCapabilities}` |

`muddiness.json` — это `experimental compatibility proxy`: corpus доказывает
воспроизводимость исторического числового API, но не валидированный на
наблюдателях человеческий вердикт clean/dirty и не пригодность для production
decision. Legacy-идентификаторы сохранены только для совместимости.

- `theme` — kebab-ключ: `light` \| `dark` \| `light-ic` \| `dark-ic`.
- `contract` (в `solve`): `{kind:"text", lc}` \| `{kind:"ui", lc}` \|
  `{kind:"range", floor, ceiling}`.
- `outcome` (в `solve`): успех `{kind:"solved", hex, lc, wcagRatio, floorOverride}`
  или честный отказ `{kind:"unreachable", code}`.
- `code` недостижимости — стабильный словарь, общий для всех биндингов:
  `below_contrast_floor`, `exceeds_range`, `quantization_gap`,
  `floor_unreachable`, `polarity_mismatch`, `gamut_unsupported`, `invalid_input`.
- `alpha.json` начиная с pack `2.0.0` обязательно содержит точный byte-reference
  half-tie `#C0B2FA @ 0.122` над `#000000` → `#17161F`. Это mutation-killer
  старого пути `(byte/255) · alpha · 255`, который выбирал соседний LSB.
- `manifest.numericalCapabilities` (схема пака 3.0.0) генерируется из
  core-owned `numerical_capability_manifest_v1()` и заменяет прозаический
  `numericalSites` пака 2.x. Форма:
  `{schemaVersion, coverage, sites[], checksum}`, где `schemaVersion` —
  независимый version domain capability-схемы (сейчас `1`); `coverage` —
  `migrated-sites-only-v1` (перечислены только **уже мигрированные**
  branch-sensitive sites, не утверждение полного аудита исторических
  `f64`-ветвлений — он остаётся в scope #291); каждая строка `sites[]` несёт
  `siteId` и шесть списков стабильных ключей (`stableOutcomes`,
  `compatibilityReleases`, `evidenceClasses`, `artifactIds`, `boundIds`,
  `runtimeAttestations`; пустой список — явное «evidence отсутствует», не
  пропуск); `checksum` — FNV-1a-32 (8 lowercase hex) над canonical
  length-prefixed preimage с домен-сепаратором
  `labcolors.numerical-capability.v1`. Release verifier и Swift-тесты
  пересчитывают checksum НЕЗАВИСИМО от Rust-кода. Сейчас в manifest только
  `glow-target-or-maximum-v1`.

Словарь **позиций лестницы** (не ролей): `label-*`, `fill-*`, `border-*`,
`focus-ring`, `glow`, `skeleton-*`, `neutral-fill-*`, `neutral-border-*`,
`shadow-*`. Пак НЕ вводит роль `icon` — иконки и текст всегда красятся
labels (канон labui): роли `icon` в словаре нет.

## Критерий conformance

Биндинг conformant по версии пака `X`, если на КАЖДОМ векторе его выход
совпадает с каноном по этим правилам:

- **Числовые поля** (`lc`, `wcagRatio`, `score`, `alpha`, `minAlpha`, `alpha*`) —
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

Внутренняя ошибка core и неизвестный forward-вариант `Unreachable` не являются
solve-векторами: `Pack::generate()` возвращает `PackGenerationError` и не пишет
правдоподобный `{kind:"unreachable"}` fallback в сертификационный артефакт.
- **Строки/enum/bool** (`theme`, `position`, `code`, `floorOverride`, `kind`) —
  ТОЧНО.

## Референс: ядро само себя проходит

`crates/labcolors-conformance` несёт генератор (`--bin gen`) и раннер-референс
(`tests/reference_runner.rs`). Раннер — CI-гейт: ядро воспроизводит каждый
вектор в пределах толерантности, дайджест сходится с сырыми байтами, а
опубликованные WCAG-якоря (21:1, граница `#767676`) держатся. Раннер входит в
`cargo test --workspace` на Linux x86_64. Активный Swift/UniFFI gate также
прогоняет все пять семейств пака в pinned Linux x86_64 container.

Активный browser-gate теперь воспроизводит все 82 закоммиченных вектора внутри
фактического wasm32 core runtime и отдельно держит targeted parity-тесты
публичной JS-границы. Это доказывает wasm32-исполнение ядра против независимых
байтов пака, но ещё не прогоняет каждый вектор всех пяти семейств непосредственно
через публичный JS API. Поэтому полная conformance именно JS-поверхности текущего
пака пока не заявляется. В
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
падёт, если закоммиченные векторы разошлись с ядром за пределами толерантности —
тогда перегенерируй и обнови `coreVersion`.
