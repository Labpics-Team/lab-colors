//! Жёсткий контраст-констрейнт ТИНТ-бейджа (`RoleSpec::PairLabel`, task #29).
//!
//! Класс, который закрывают эти тесты: контраст `label ↔ tinted-fill` у бейджа
//! на тинт-фоне был ЭМЕРДЖЕНТНЫМ, не гарантированным. Обычные `label-*` роли
//! решаются против ФОНА СТРАНИЦЫ и достигают своего WCAG-пола там; тинт-бейдж
//! (`labui/lab-badge.ts`, `type=tinted`) кладёт их на `fill-*-primary` (@12
//! семейный тинт) — на этой подложке контраст ниже, и для warning/success
//! «цветной» лейбл (доля 0.4757, как `label-*-tertiary`) оседает к ~2.9:1 < 3:1.
//! `PairLabel` решает оттеночный лейбл ПРОТИВ тинт-поверхности, поэтому пол
//! гарантирован по построению; при недостижимости тон клампится (флаг
//! `compressed`, ADR-0002), а не молча остаётся нечитаемым.
//!
//! Три группы:
//!  1. `shipped_*` — реальный контракт labui (`label-<fam>-primary` на
//!     `fill-<fam>-primary`) держит UI-пол на тинте во всех темах (гвардит то,
//!     что уже отгружено, — near-black лейбл сейчас даёт 13–18:1).
//!  2. `pair_label_*` — новая роль держит пол против тинт-поверхности во всех
//!     сентимент-категориях × light/dark (± IC).
//!  3. `pair_label_beats_page_resolved_label` — дифференциальный RED-proof:
//!     ТОТ ЖЕ контракт (доля 0.4757, `AaUi`), решённый против страницы
//!     (`label-<fam>-tertiary`), проваливает 3:1 на тинте у warning/success,
//!     а `PairLabel` (против поверхности) — держит. Разница ТОЛЬКО в подложке
//!     резолва: если бы `resolve_pair_label` целил фон страницы, тест бы упал.

use crate::config::fixture::labui_reference;
use crate::solve::Floor;
use crate::{BgInput, LadderSource, Resolved, RoleRecipe, ViewingConditions, resolve_named_set};

/// (имя семьи, источник лестницы) — 5 цветных семей тинт-бейджа labui
/// (нейтраль/статики используют `label-primary`/семейный примитив, покрыты
/// отдельно) .
fn families() -> [(&'static str, LadderSource); 5] {
    [
        ("brand", LadderSource::Brand),
        ("danger", LadderSource::Sentiment("danger".to_string())),
        ("warning", LadderSource::Sentiment("warning".to_string())),
        ("success", LadderSource::Sentiment("success".to_string())),
        ("info", LadderSource::Sentiment("info".to_string())),
    ]
}

/// Четыре режима labui: тема (фон) × VC-пресет.
fn themes() -> [(&'static str, &'static str, ViewingConditions); 4] {
    [
        ("light", "#FFFFFF", ViewingConditions::srgb()),
        (
            "light-ic",
            "#FFFFFF",
            ViewingConditions::srgb_high_contrast(),
        ),
        ("dark", "#101012", ViewingConditions::dim_surround()),
        (
            "dark-ic",
            "#101012",
            ViewingConditions::dim_surround_high_contrast(),
        ),
    ]
}

/// Юр. пол UI (WCAG 1.4.11, 3:1) — консервативный дефолт порога тинт-бейджа
/// (короткая пилюля-индикатор — UI-объект, не длинный текст; 4.5:1 на светлом
/// тинте вынудил бы near-black и убил бы «цветной» вид). Порог 3:1 vs 4.5:1 —
/// открытый вопрос владельцу (см. отчёт task #29).
const UI_FLOOR: f64 = 3.0;

fn enc(hex: &str) -> [f64; 3] {
    crate::spaces::srgb::srgb_encoded_from_hex(hex).expect("тестовый hex валиден")
}

/// Тинт-поверхность семьи = композит `fill-<fam>-primary` (то, во что складывается
/// `fill-*-tinted`).
fn surface_hex(set: &[(String, Resolved)], fam: &str) -> String {
    set.iter()
        .find(|(n, _)| n == &format!("fill-{fam}-primary"))
        .and_then(|(_, r)| r.translucent())
        .map(|t| t.composite_hex().to_string())
        .unwrap_or_else(|| panic!("fill-{fam}-primary — тинт-поверхность"))
}

/// Solved-hex цветной роли (лейбл).
fn solid_hex(set: &[(String, Resolved)], role: &str) -> String {
    set.iter()
        .find(|(n, _)| n == role)
        .and_then(|(_, r)| r.solved())
        .map(|s| s.hex().to_string())
        .unwrap_or_else(|| panic!("роль `{role}` обязана решиться цветом"))
}

/// WCAG-контраст роли против тинт-поверхности своей семьи.
fn ratio_on_tint(set: &[(String, Resolved)], role: &str, fam: &str) -> f64 {
    crate::wcag::contrast_ratio(enc(&solid_hex(set, role)), enc(&surface_hex(set, fam)))
}

/// Конфиг labui + добавленные роли `badge-label-<fam>` (`PairLabel`, доля
/// 0.47572199 «цветного» уровня = `label-*-tertiary`, пол `AaUi`) — форма,
/// которую тинт-бейдж должен потреблять.
fn labui_with_badge_labels() -> crate::NamedRoleTable {
    let mut cfg = labui_reference();
    for (fam, source) in families() {
        cfg.roles.push((
            format!("badge-label-{fam}"),
            RoleRecipe::PairLabel {
                source,
                fraction: 0.47572199,
                floor: Floor::AaUi,
            },
        ));
    }
    cfg.compile_named_role_table()
        .expect("labui + badge-label компилируется")
}

// ── 1. Отгруженный контракт: label-<fam>-primary на fill-<fam>-primary ────────

#[test]
fn shipped_tinted_badge_label_clears_ui_floor_on_tint_all_families_and_themes() {
    let table = labui_reference().compile_named_role_table().unwrap();
    for (tname, bg_hex, vc) in themes() {
        let bg = BgInput::solid(bg_hex).unwrap();
        let set = resolve_named_set(&bg, &table, &vc);
        for (fam, _) in families() {
            let r = ratio_on_tint(&set, &format!("label-{fam}-primary"), fam);
            assert!(
                r >= UI_FLOOR - 1e-9,
                "[{tname}] отгруженный тинт-бейдж `label-{fam}-primary` на \
                 `fill-{fam}-primary` обязан держать {UI_FLOOR}:1, получено {r:.2}:1"
            );
        }
    }
}

// ── 2. PairLabel: жёсткий пол против тинт-поверхности, все семьи × темы ────────

#[test]
fn pair_label_clears_ui_floor_against_tinted_surface_all_families_and_themes() {
    let table = labui_with_badge_labels();
    for (tname, bg_hex, vc) in themes() {
        let bg = BgInput::solid(bg_hex).unwrap();
        let set = resolve_named_set(&bg, &table, &vc);
        for (fam, _) in families() {
            let role = format!("badge-label-{fam}");
            // Решается цветом (не Unreachable/None) и держит пол на тинте.
            let r = ratio_on_tint(&set, &role, fam);
            assert!(
                r >= UI_FLOOR - 1e-9,
                "[{tname}] `{role}` обязан держать {UI_FLOOR}:1 против тинт-поверхности \
                 `fill-{fam}-primary`, получено {r:.2}:1"
            );
        }
    }
}

/// Лейбл тинт-бейджа остаётся ЦВЕТНЫМ (оттенок не испарился) — иначе гарантия
/// контраста была бы куплена ценой near-black, а не выведенного цвета.
#[test]
fn pair_label_stays_hued_not_near_black() {
    let table = labui_with_badge_labels();
    let bg = BgInput::solid("#FFFFFF").unwrap();
    let set = resolve_named_set(&bg, &table, &ViewingConditions::srgb());
    for (fam, _) in families() {
        let role = format!("badge-label-{fam}");
        let res = &set.iter().find(|(n, _)| n == &role).unwrap().1;
        let Resolved::Color {
            solved,
            hue_vanished,
            ..
        } = res
        else {
            panic!("`{role}` обязан решиться цветом");
        };
        assert!(
            !hue_vanished,
            "`{role}`: оттенок семьи обязан выжить (не near-black кламп)"
        );
        // И отличается от отгруженного near-black `label-<fam>-primary` (17:1):
        // цветной лейбл у пола — это НЕ чёрный.
        let primary = solid_hex(&set, &format!("label-{fam}-primary"));
        assert_ne!(
            solved.hex(),
            primary,
            "`{role}` не должен схлопнуться в near-black `label-{fam}-primary`"
        );
    }
}

// ── 3. Дифференциальный RED-proof: подложка резолва — и есть констрейнт ────────

/// ТОТ ЖЕ контракт (доля 0.4757, `AaUi`), решённый против СТРАНИЦЫ
/// (`label-<fam>-tertiary`), проваливает 3:1 на тинте у warning/success в light;
/// `PairLabel` (против ПОВЕРХНОСТИ) — держит. Единственная разница — подложка
/// резолва: если `resolve_pair_label` целил бы фон страницы, «after» совпал бы с
/// «before» и упал. Кусающийся тест, не green-from-birth.
#[test]
fn pair_label_beats_page_resolved_label_on_failing_families() {
    let table = labui_with_badge_labels();
    let bg = BgInput::solid("#FFFFFF").unwrap();
    let set = resolve_named_set(&bg, &table, &ViewingConditions::srgb());

    // warning/success — семьи, где страничный `label-*-tertiary` проваливает 3:1
    // на собственном тинте (замер task #29: warning 2.76, success 2.88).
    for fam in ["warning", "success"] {
        let before = ratio_on_tint(&set, &format!("label-{fam}-tertiary"), fam);
        let after = ratio_on_tint(&set, &format!("badge-label-{fam}"), fam);
        assert!(
            before < UI_FLOOR,
            "предпосылка класса: страничный `label-{fam}-tertiary` обязан \
             проваливать {UI_FLOOR}:1 на тинте (иначе тест не о том), получено {before:.2}:1"
        );
        assert!(
            after >= UI_FLOOR - 1e-9,
            "`badge-label-{fam}` (тот же контракт, но против поверхности) обязан \
             держать {UI_FLOOR}:1, получено {after:.2}:1"
        );
        assert!(
            after > before,
            "резолв против поверхности обязан ПОДНЯТЬ контраст на тинте: \
             {fam} before={before:.2} after={after:.2}"
        );
    }
}
