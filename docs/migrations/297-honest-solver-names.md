# №297: честные имена локального поиска (breaking, одна волна)

Старый публичный язык солвера переобещал: «AA»-имена претендовали на
WCAG-соответствие, «legal» — на закон, `nearest` — на глобальный минимум.
Юзеров у пакета нет, поэтому исправление — чистый breaking без deprecated-слоя
и без fallback-чтения старых имён. Эмитируемые цвета, решения и все численные
биты не изменились (гейт — платформенные характеризационные фикстуры
`solve-characterization-v1-*`, реплеящиеся бит-в-бит до и после переименования).

## Карта имён

| Было | Стало | Слой |
|---|---|---|
| `Floor::AaText` / `Floor::AaUi` | `Floor::TextRatio` / `Floor::UiRatio` | Rust |
| `"aa-text"` / `"aa-ui"` (значения `floor` в конфиге) | `"text-ratio"` / `"ui-ratio"` | конфиг-JSON / TS-типы |
| `Contract::conformance()` / `with_conformance` | `Contract::ratio_floor()` / `with_ratio_floor` | Rust |
| `TextAnchor::conformance()` | `TextAnchor::ratio_floor()` | Rust |
| `Unreachable::QuantizationGap { nearest }` | `… { closest_examined }` | Rust |
| `RoleTable/RoleSpec::legal_floor()` | `floor_ratio()` | Rust |
| `legalFloor` (роль в resolved-теме) | `floorRatio` (тип `number \| null` не менялся) | wire/JS |

Старые конфиг-строки не читаются: `"aa-text"`/`"aa-ui"` в поле `floor` —
структурная ошибка десериализации. Отпечаток конфиг-схемы легитимно сменился
(`c51445fcd167781a` → `866006bd94b8ce02`, пин в `config_dto.rs`); клиентский
пин `PASSPORT_FINGERPRINT` на стороне labui обновить при подъёме версии пакета.

## Что осталось как было (имя не врёт)

- `wcag_ratio` / `wcagRatio` — имя ФОРМУЛЫ WCAG-ratio (1–21), не claim о
  соответствии критерию; доки уточнены.
- `floor_override` / `floorOverride` — ratio-пол действительно вытесняет
  перцептуальную цель.
- `QuantizationGap` — феномен назван верно; ложь жила в доках («no on-grid
  colour», «every hex») и в поле `nearest` — доки переписаны, поле переименовано.
- `FloorUnreachable` — отказ внутри объявленной полярности контракта; док
  проговаривает границы утверждения.
- `degraded` / `achieved_dj` у dJ'-пути — доки теперь говорят «ближайший из
  ИЗУЧЕННЫХ локальным однонаправленным walk'ом», не «nearest achievable».
- Wire-коды `quantization_gap` / `floor_unreachable` и ключи conformance-пака —
  байты `conformance/vectors/*` не тронуты, pack остаётся 6.0.0.

## Чем доказана байт-нейтральность

- `solve-characterization-v1-{macos-aarch64,linux-x64}.json` (77 кейсов,
  f64-биты всего публичного payload) реплеятся без изменений;
- workspace-сьют зелёный без правок ожиданий (кроме самих имён);
- эмиссия adapt-theme использует то же число под новым ключом `floorRatio`.
