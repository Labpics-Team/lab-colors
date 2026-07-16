//! Сквозные инварианты ПАСПОРТА/КОНФИГА: идемпотентность экспорта и детерминизм
//! отпечатка. Нативный интеграционный тест (не `wasm32`-гейт) — гоняется под
//! `cargo test --workspace` в CI-джобе `test`, без браузера.
//!
//! Что уже закрыто (НЕ дублируем):
//! - `config_dto.rs::labui_passport_round_trips_through_json` — равенство
//!   `ThemeConfig` (PartialEq ЯДРА) на круг-трипе паспорт→DTO→JSON→DTO→ядро;
//! - `config_dto.rs::fingerprint_is_deterministic_and_discriminating` —
//!   отпечаток стабилен к pretty-репарсу и различает конфиги;
//! - `config_dto.rs::thin_and_full_labui_share_one_fingerprint` — тонкий==полный.
//!
//! Дыры, которые закрывает этот файл:
//! 1. КРУГ-ТРИП ЧЕРЕЗ ДВИЖОК ПО ОТПЕЧАТКУ И БАЙТАМ. Существующий тест сверяет
//!    `cfg == restored` (PartialEq ядра) — это САМОРЕФЕРЕНТНО: обе стороны прошли
//!    ОДНУ конверсию DTO→ядро, поэтому поле, которое ядро роняет, не всплывёт
//!    (оно уже потеряно в `cfg`). Идентичность-наружу — это ОТПЕЧАТОК и БАЙТЫ
//!    DTO. Здесь: `fingerprint(DTO_вход) == fingerprint(экспорт(движок(DTO_вход)))`
//!    и каноническая сериализация обоих побайтово равна — «замороженные поля»
//!    паспорта переживают экспортный путь.
//! 2. LF/CRLF- и порядок-ключей-инвариантность отпечатка ЯВНЫМ гардом (класс
//!    ловили на Windows-хосте). Структурно отпечаток берётся по РАСПАРСЕННОМУ
//!    DTO, поэтому инвариантность гарантирована конструкцией — но регрессионного
//!    замка на неё не было.

use labcolors_core::config::ThemeConfig;
use labcolors_wasm::config_dto::{ConfigDto, fingerprint};

const LABUI_JSON: &str = include_str!("data/labui.config.json");
const LABUI_PROD_JSON: &str = include_str!("data/labui.config.prod.json");

/// Паспорт нормализован к LF: `include_str!` вшивает БАЙТЫ с диска, а на
/// Windows-checkout это может быть CRLF. Инварианты про перевод строк должны
/// строиться от известной базы, а не от того, как git развернул рабочее дерево.
fn lf(s: &str) -> String {
    s.replace("\r\n", "\n")
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. ИДЕМПОТЕНТНОСТЬ ЭКСПОРТА: конфиг → движок → экспорт → снова движок
// ─────────────────────────────────────────────────────────────────────────────

/// Ядро экспортного круг-трипа: `DTO_вход → ThemeConfig → DTO_выход`. И
/// отпечаток, и каноническая сериализация обязаны совпасть побайтово — иначе
/// экспортный путь теряет/переставляет поле (тихий дрейф идентичности кэша и
/// контракта наружу). Сравниваем СЕРИАЛИЗАЦИИ двух `ConfigDto` (обе через serde,
/// один формат чисел/порядок полей), а не сырой файл — так тест не ловит
/// косметику исходного JSON, только реальную потерю на пути через ядро.
fn assert_export_roundtrip_is_byte_identical(passport: &str, label: &str) {
    let dto_in: ConfigDto = serde_json::from_str(passport).expect("паспорт парсится");
    let cfg = ThemeConfig::try_from(dto_in.clone()).expect("DTO → ThemeConfig");
    let dto_out = ConfigDto::try_from(&cfg).expect("ThemeConfig → DTO");

    assert_eq!(
        fingerprint(&dto_in),
        fingerprint(&dto_out),
        "{label}: экспортный путь сдвинул отпечаток — движок теряет/переставляет поле паспорта"
    );

    let bytes_in = serde_json::to_string(&dto_in).expect("сериализация входа");
    let bytes_out = serde_json::to_string(&dto_out).expect("сериализация экспорта");
    assert_eq!(
        bytes_in, bytes_out,
        "{label}: каноническая форма разошлась на экспортном пути (не байт-в-байт)"
    );
}

/// Канонический паспорт (target M1 text-anchor стиль).
#[test]
fn canonical_passport_export_roundtrip_is_byte_identical() {
    assert_export_roundtrip_is_byte_identical(LABUI_JSON, "canonical");
}

/// Прод-снапшот (ladder-стиль hued-лейблов) — другой стиль рецептов идёт тем же
/// путём без потерь. Закрывает «идемпотентность держится лишь на одном стиле».
#[test]
fn prod_passport_export_roundtrip_is_byte_identical() {
    assert_export_roundtrip_is_byte_identical(LABUI_PROD_JSON, "prod");
}

/// Неподвижная точка: второй прогон экспорта ничего не меняет
/// (`движок→экспорт→движок→экспорт == движок→экспорт`). Ловит осциллирующие
/// потери, которые байт-тест одного круга мог бы не поймать, будь конверсия
/// не-идемпотентной со второго шага.
#[test]
fn export_is_a_fixed_point_second_pass_is_identical() {
    let dto_in: ConfigDto = serde_json::from_str(LABUI_JSON).expect("паспорт парсится");
    let cfg1 = ThemeConfig::try_from(dto_in).expect("DTO → ThemeConfig (1)");
    let dto1 = ConfigDto::try_from(&cfg1).expect("экспорт (1)");
    let cfg2 = ThemeConfig::try_from(dto1.clone()).expect("DTO → ThemeConfig (2)");
    let dto2 = ConfigDto::try_from(&cfg2).expect("экспорт (2)");
    assert_eq!(
        serde_json::to_string(&dto1).unwrap(),
        serde_json::to_string(&dto2).unwrap(),
        "второй проход экспорта не идемпотентен"
    );
    assert_eq!(
        fingerprint(&dto1),
        fingerprint(&dto2),
        "отпечаток дрейфует между проходами"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. ДЕТЕРМИНИЗМ ОТПЕЧАТКА: LF/CRLF и порядок ключей
// ─────────────────────────────────────────────────────────────────────────────

/// Отпечаток инвариантен к переводам строк (LF vs CRLF) и к отступам: он берётся
/// по РАСПАРСЕННОМУ DTO, где пробелы вне строк уже съедены serde. Явный
/// регрессионный замок класса, что ловили на Windows-хосте.
#[test]
fn fingerprint_is_invariant_to_lf_crlf_and_indentation() {
    let base = lf(LABUI_JSON);
    let crlf = base.replace('\n', "\r\n");
    // Отступ вокруг структуры (pretty) — тот же контент, другие пробелы.
    let pretty = {
        let v: serde_json::Value = serde_json::from_str(&base).expect("паспорт → Value");
        serde_json::to_string_pretty(&v).expect("pretty")
    };
    let pretty_crlf = pretty.replace('\n', "\r\n");

    let fp = |s: &str| {
        let dto: ConfigDto = serde_json::from_str(s).expect("парсится");
        fingerprint(&dto)
    };
    let want = fp(&base);
    assert_eq!(fp(&crlf), want, "CRLF сдвинул отпечаток");
    assert_eq!(fp(&pretty), want, "pretty-отступы сдвинули отпечаток");
    assert_eq!(fp(&pretty_crlf), want, "pretty+CRLF сдвинул отпечаток");
}

/// Отпечаток инвариантен к порядку ключей JSON: две записи ОДНОГО конфига с
/// по-разному упорядоченными top-level ключами дают один отпечаток (serde
/// нормализует порядок при десериализации в структуру). Малый инлайн-конфиг —
/// прямой контроль порядка без борьбы с сортировкой `serde_json::Value`.
#[test]
fn fingerprint_is_invariant_to_json_key_order() {
    // Порядок A: brand, neutral, palette, sentiments, themes, roles.
    let order_a = r##"{
      "brand": {"light": "#7C3AED", "dark": "#8B5CF6", "light_ic": "#5B21B6", "dark_ic": "#A78BFA"},
      "neutral": {
        "anchors": {"light": "#FFFFFF", "mid": "#7A7A82", "dark": "#17171A"},
        "tint": {"ratio": 0.1, "target_mp": 6.1, "hue_stiffness": 9.0}
      },
      "palette": [],
      "sentiments": {"categories": [], "hardness": 5.0, "chroma_fraction": 0.88},
      "themes": [{"name": "light", "preset": "srgb"}],
      "roles": [
        {"name": "body-text", "recipe": {"kind": "text-anchor", "fraction": 0.62, "floor": "text-ratio"}}
      ]
    }"##;
    // Порядок B: те же ключи/значения, обратный top-level порядок.
    let order_b = r##"{
      "roles": [
        {"name": "body-text", "recipe": {"kind": "text-anchor", "fraction": 0.62, "floor": "text-ratio"}}
      ],
      "themes": [{"name": "light", "preset": "srgb"}],
      "sentiments": {"categories": [], "hardness": 5.0, "chroma_fraction": 0.88},
      "palette": [],
      "neutral": {
        "tint": {"ratio": 0.1, "target_mp": 6.1, "hue_stiffness": 9.0},
        "anchors": {"light": "#FFFFFF", "mid": "#7A7A82", "dark": "#17171A"}
      },
      "brand": {"dark_ic": "#A78BFA", "light_ic": "#5B21B6", "dark": "#8B5CF6", "light": "#7C3AED"}
    }"##;
    let fp = |s: &str| {
        let dto: ConfigDto = serde_json::from_str(s).expect("парсится");
        fingerprint(&dto)
    };
    assert_eq!(
        fp(order_a),
        fp(order_b),
        "порядок ключей JSON не должен влиять на отпечаток"
    );
}
