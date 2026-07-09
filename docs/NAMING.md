# NAMING — нейминг-канон lab-colors

> Роль: канон именования (закон + справка). Сверяется гейтом
> `scripts/check-docs-drift.mjs` с фактами ФС из `scripts/naming-inventory.mjs`:
> числа и списки ниже обязаны совпадать с реальностью, иначе прогон красный.

Канон согласован с эталоном экосистемы Labpics-Team — labui/lab-icons/lab-motion
(docs/NAMING.md + гейт docs-drift): те же общие принципы, репо-специфика ниже.
Отличие от Node-монорепо эталона: lab-colors — Rust-workspace, поэтому закон
имён по-доменный (snake_case для Rust/Python — закон языка, не отступление).

## Инвентарь (сверяется гейтом)

| Метрика | N |
| --- | --- |
| членов workspace (Cargo.toml `members`, глоб развёрнут по ФС) | 6 |
| крейтов семейства в crates/ | 5 |
| экспорт-субпутей package.json @labpics/colors | 7 |
| python-скриптов scripts/*.py | 2 |
| маркдаун-доков docs/**/*.md (включая этот канон) | 11 |
| векторов conformance/vectors/*.json (включая manifest) | 6 |
| файлов вне закона имён | 4 |

## Общие принципы (эталон lab-icons)

1. **Закон языка — закон имён** — файл именуется по конвенции своего домена:
   snake_case для `.rs`/`.py` (rustc/PEP 8), PascalCase для `.swift`,
   kebab-case для JS/TS/JSON/SVG и экспорт-субпутей, kebab- или КАПС-стем
   (каноны вроде `NAMING.md`, README) для маркдауна.
2. **Домен-префикс** — всё публичное несёт префикс домена: крейты
   `labcolors-<роль>`, npm-пакет @labpics/colors.
3. **Суффиксы вариантов** — вариант отделяется суффиксом от основы
   (`jhk_golden_ref.py` от `golden_ref.py`), не переизобретает основу.
4. **Дока ↔ реальность** — каждое имя в доке существует в коде, инвентарь
   покрыт докой; расхождение ловит гейт, а не ревьюер.

## Законы lab-colors

### Крейты workspace

- Имя крейта = `labcolors-<роль>`, kebab-case; роль — одно слово:
  `labcolors-core`, `labcolors-conformance`, `labcolors-ffi`,
  `labcolors-preview`, `labcolors-wasm`. Директория `crates/<имя крейта>`.
- Члены workspace объявлены в корневом Cargo.toml (`members`); глоб `crates/*`
  разворачивается по ФС, harness-члены вне crates/ перечислены поимённо
  (experiments/psychophysics — имя пакета без префикса: не публикуется,
  зависимость-нулевой калибровочный харнесс). Каждый член обязан разрешаться
  в ФС — фантомные члены ловит гейт.
- Файлы `.rs` — snake_case стема (закон rustc); это относится и к тестам,
  бенчам и примерам (`accent_balance.rs`, `y_hk.rs`, `bg_ladder_anchors.rs`).

### npm-пакет @labpics/colors

- Субпуть = домен, kebab-case: `./apply-theme`, `./watch-theme`,
  `./adapt-theme`, `./effective-bg`. Каждый субпуть разрешается в исходник
  packages/colors (exports без кода запрещены — сверяет typecheck пакета).
- Служебный субпуть `./package.json` — стандарт npm, разрешён законом.
- Артефактный субпуть `./pkg/labcolors_bg.wasm` — см. «Известные отступления».
- Файлы пакета — kebab-case (`apply-theme.js` + `apply-theme.d.ts`).

### Python-скрипты (golden-эталоны)

- snake_case по PEP 8: `golden_ref.py` — эталон colour-science для CIECAM16;
  `jhk_golden_ref.py` — вариант с суффиксом-основой для J'a'b' (JHK).
- Скрипты живут только в scripts/; каждый упомянут в этом каноне — появление
  нового скрипта требует строчки здесь (иначе гейт красный).

### Доки

- docs/*.md — kebab-case стем (`empirical-inventory.md`,
  `verification-map.md`); канонам разрешён КАПС-стем (`NAMING.md`).
- ADR в docs/decisions/ — `NNNN-kebab-тема.md` (`0001-config-boundary.md`);
  номер сквозной, тема — kebab.
- Векторы конформанса — conformance/vectors/`<домен>.json`, домен — одно
  kebab-слово (alpha, contrasts, ladders, muddiness, solve) + manifest.json.

### Swift-биндинг

- Файлы `.swift` — PascalCase (конвенция Swift); `Package.swift`, `Cargo.toml`
  и прочие имена, продиктованные тулингом, — вне юрисдикции закона.

## Известные отступления

Зафиксированные сканом отклонения от законов выше; каждая запись обязана
соответствовать живому факту ФС — исчезло из кода, убирается и отсюда (гейт).

- `./pkg/labcolors_bg.wasm` — субпуть-артефакт wasm-pack: имя `labcolors_bg.wasm`
  генерирует wasm-bindgen из `--out-name labcolors`, snake_case продиктован
  тулингом и руками не переименовывается.
- `crates/labcolors-core/tests/data/labui_emission_golden.txt` — golden-фикстура
  Rust-теста, snake_case в тон соседним `.rs` (данные теста наследуют закон
  своего потребителя).
- `crates/labcolors-wasm/tests/data/labui.config.prod.json` — фикстура
  байт-в-байт воспроизводит имя реального конфига labui; точечные сегменты —
  как у источника, переименование сломало бы прослеживаемость.
- `packages/colors/bench/AFTER.txt` и `packages/colors/bench/BASELINE.txt` —
  bench-слепки; КАПС-стем — маркер «снапшот, не код», по аналогии с
  КАПС-канонами маркдауна.

## Как чинить дрейф

1. `node scripts/naming-inventory.mjs` — посмотреть факты ФС (JSON).
2. `node scripts/check-docs-drift.mjs` — список расхождений с этой докой.
3. Правь то, что врёт: либо код (имя вне закона), либо этот канон (число в
   инвентаре, пропавшее/новое имя, запись в «Известных отступлениях»).
4. Юниты гейта: `node --test scripts/docs-drift.test.mjs`.
