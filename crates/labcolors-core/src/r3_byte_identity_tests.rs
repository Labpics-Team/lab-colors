//! Дифференциальные проверки режима R3.
//!
//! Для акцентных кривых фиксируется не исторический массив удалённого алгоритма,
//! а байтовый детерминизм текущего конечного закона и единственность anchor как
//! источника цвета. Репрезентативные semantic-токены ниже по-прежнему сравниваются
//! с собственным стабильным контрактом резолвера.

use crate::{
    BgInput, Resolved, Role, RoleTable, ViewingConditions,
    curve::ColorCurve,
    neutral::NeutralCurve,
    resolve_set,
    scale::AccentCurve,
    sentiment::{Sentiment, SentimentCurve},
};

// ─────────────────────────────────────────────────────────────────────────────
// R3-A: детерминизм двух представительных конечных кривых.
// ─────────────────────────────────────────────────────────────────────────────

fn canonical_neutral() -> NeutralCurve {
    NeutralCurve::new("#FFFFFF", "#787880", "#101012")
        .expect("R3: canonical neutral anchors are valid")
}

/// R3 фиксирует байтовый детерминизм закона, а не исторический вывод удалённой
/// непрерывной кривой: одинаковый anchor и скелет обязаны дать тот же sRGB8.
#[test]
fn r3_accent_srgb8_output_is_deterministic() {
    let neutral = canonical_neutral();
    let first = AccentCurve::new("#007AFF", &neutral)
        .unwrap()
        .sample_hex(13);
    let second = AccentCurve::new("#007AFF", &neutral)
        .unwrap()
        .sample_hex(13);
    assert_eq!(first, second);
    assert_eq!(first[0], "#FFFFFF");
}

/// Устаревший brand-параметр не участвует в сентиментной физике: единственным
/// источником hue и радиуса остаётся `#3E87FF`.
#[test]
fn r3_sentiment_anchor_is_the_only_colour_source() {
    let neutral = canonical_neutral();
    let a = SentimentCurve::from_sentiment(Sentiment::Info, 200.0, "#3E87FF", &neutral)
        .unwrap()
        .sample_hex(13);
    let b = SentimentCurve::from_sentiment(Sentiment::Info, 33.5, "#3E87FF", &neutral)
        .unwrap()
        .sample_hex(13);
    let primary = SentimentCurve::from_anchor("#3E87FF", &neutral)
        .unwrap()
        .sample_hex(13);
    assert_eq!(a, b);
    assert_eq!(a, primary);
}

// ─────────────────────────────────────────────────────────────────────────────
// R3-B: resolve_set 240-cell byte-identity (representative subset).
//
// The full 240-cell grid is pinned by `semantic::resolve_set_golden_hex_is_byte_for_byte_stable`
// as an internal `#[test]`. Here we independently pin one representative cell
// per (vc, bg) combination (12 cells = 6 backgrounds × 2 VCs × 1 role each).
// This is sufficient to catch a coefficient drift that moves ALL cells for a
// given (vc, bg) — the class of regression the test plan names as "R3 240-cell
// resolve_set byte-identity". The full grid assertion lives in semantic.rs and
// is still run as part of `cargo test --workspace`.
//
// GOLDEN SOURCE: captured 2026-06-12 at main@f21aac7 from the same GOLDEN
// table in `semantic.rs::resolve_set_golden_hex_is_byte_for_byte_stable`.
// ─────────────────────────────────────────────────────────────────────────────

/// One representative (vc, bg, role, expected_hex) per (vc, bg) combination.
/// Sampled from the 240-cell GOLDEN table in semantic.rs at main@f21aac7.
/// `label-primary` is chosen as the representative because it is the highest-
/// contrast text role and the most sensitive canary for a lightness-shift.
const R3_RESOLVE_SET_SPOTS: [(&str, &str, &str, &str); 12] = [
    // sRGB viewing conditions — regenerated for the readability→`Ys` activation
    // (глава #64, ADR-0003): ось читаемости перешла в люминансный домен `Ys`, и
    // текстовая лестница пересобрана на принятые владельцем hex'ы (#141414 и
    // тонированный дефолт). Owner sign-off = ADR-0003 (Принято, делегация
    // владельца). Значения совпадают с 240-cell GOLDEN в semantic.rs (тот же
    // тонированный дефолт RoleTable).
    ("srgb", "#FFFFFF", "label-primary", "#14131A"),
    ("srgb", "#F2F2F7", "label-primary", "#131219"),
    ("srgb", "#7F7F7F", "label-primary", "#08070E"),
    ("srgb", "#1C1C1E", "label-primary", "#FAFAFF"),
    ("srgb", "#101012", "label-primary", "#FAFAFF"),
    ("srgb", "#3478F6", "label-primary", "#08070D"),
    // Dim (display / dark-room) viewing conditions — same source.
    ("dim", "#FFFFFF", "label-primary", "#141419"),
    ("dim", "#F2F2F7", "label-primary", "#131218"),
    ("dim", "#7F7F7F", "label-primary", "#08080C"),
    ("dim", "#1C1C1E", "label-primary", "#FAFAFF"),
    ("dim", "#101012", "label-primary", "#FAFAFF"),
    ("dim", "#3478F6", "label-primary", "#08070C"),
];

/// R3: the representative cells from the 240-cell resolve_set grid are
/// byte-identical to the values at main@f21aac7.
///
/// This test is GREEN at birth (characterization lock). It bites on mutation:
/// change any entry in `R3_RESOLVE_SET_SPOTS` → `assert_eq!` fails naming the
/// (vc, bg, role) triple and the drifted hex.
#[test]
fn r3_resolve_set_240_cell_representative_byte_identity() {
    let table = RoleTable::default();
    let srgb = ViewingConditions::srgb();
    let dim = ViewingConditions::dim_surround();

    for (vc_name, bg_hex, role_key, expected_hex) in R3_RESOLVE_SET_SPOTS {
        let vc = match vc_name {
            "srgb" => &srgb,
            "dim" => &dim,
            other => panic!("R3: unknown vc name '{other}' in R3_RESOLVE_SET_SPOTS"),
        };
        let bg = BgInput::solid(bg_hex)
            .unwrap_or_else(|_| panic!("R3: invalid bg_hex '{bg_hex}' in R3_RESOLVE_SET_SPOTS"));
        let set = resolve_set(&bg, &table, vc);

        let got = set
            .iter()
            .find(|(role, _)| role.key() == role_key)
            .map(|(_, resolved)| match resolved {
                Resolved::Color { solved, .. } => solved.hex().to_string(),
                Resolved::None => "none".to_string(),
                Resolved::Unreachable(_) => "UNREACHABLE".to_string(),
                // Дефолтная `RoleTable` (Role-путь) не несёт Ladder/AlphaAnalog-
                // рецептов, поэтому rgba-роль здесь недостижима; арм обязателен
                // из-за `#[non_exhaustive] Resolved`.
                Resolved::Translucent(_) => "RGBA".to_string(),
                // Будущий вариант Resolved не должен молча пройти golden: паника
                // делает его видимым (обязан быть переучтён вместе с golden).
                other => panic!("неучтённый Resolved-вариант в r3 golden: {other:?}"),
            })
            .unwrap_or_else(|| {
                panic!(
                    "R3: role '{role_key}' not found in resolve_set output for \
                     vc={vc_name} bg={bg_hex}"
                )
            });

        assert_eq!(
            got, expected_hex,
            "R3 REGRESSION — resolve_set({vc_name}, {bg_hex}, {role_key}) = '{got}', \
             expected '{expected_hex}' (byte-identical to main@f21aac7). A perceptual \
             const RHS changed as a side-effect of a marker/comment commit. Either \
             restore the coefficient or update the golden with owner sign-off."
        );
    }
}

/// R3 sanity: ensure `resolve_set` returns all roles for EVERY spot — a missing
/// role means the find above would silently skip a row and give false green.
/// Asserts the output length matches `Role::ALL.len()` for each spot background.
#[test]
fn r3_resolve_set_returns_all_roles_for_every_spot_background() {
    let table = RoleTable::default();
    let srgb = ViewingConditions::srgb();
    let dim = ViewingConditions::dim_surround();
    let expected_len = Role::ALL.len();

    for (vc_name, bg_hex, _, _) in R3_RESOLVE_SET_SPOTS {
        let vc = match vc_name {
            "srgb" => &srgb,
            "dim" => &dim,
            other => panic!("R3: unknown vc '{other}'"),
        };
        let bg = BgInput::solid(bg_hex).unwrap();
        let set = resolve_set(&bg, &table, vc);
        assert_eq!(
            set.len(),
            expected_len,
            "R3 sanity FAILED — resolve_set returned {} roles for ({vc_name}, {bg_hex}), \
             expected {expected_len}. A role was added or removed without updating the \
             R3 golden spots.",
            set.len()
        );
    }
}
