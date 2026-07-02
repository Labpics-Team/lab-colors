//! Тесты границы конфига (CH-02 t1):
//! 1. Байт-в-байт: `resolve_named_set(labui_reference)` эмитит идентично
//!    `resolve_set(RoleTable::default)` по всем 240 точкам golden-грида.
//! 2. RED-proof байт-в-байт: мутация одного рецепта фикстуры роняет тест.
//! 3. Валидатор: за-предельное значение КАЖДОЙ ручки даёт `ConfigError` +
//!    RED-proof мутацией предела (валидный vs невалидный на границе).
//! 4. Заглушки t2: `Ladder`/`AlphaAnalog` дают `NotYetImplemented`.

use super::*;
use crate::solve::Floor;
use crate::{
    BgInput, Resolved, Role, RoleTable, ViewingConditions, resolve_named_set, resolve_set,
};

/// Грид golden: два VC-пресета × шесть фонов — тот же, что в
/// `semantic::tests::resolve_set_golden_hex_is_byte_for_byte_stable` (240 точек).
fn grid() -> ([(ViewingConditions, &'static str); 2], [&'static str; 6]) {
    (
        [
            (ViewingConditions::srgb(), "srgb"),
            (ViewingConditions::dim_surround(), "dim"),
        ],
        [
            "#FFFFFF", "#F2F2F7", "#7F7F7F", "#1C1C1E", "#101012", "#3478F6",
        ],
    )
}

/// Hex/none/UNREACHABLE-представление резолва — как в golden-тесте ядра.
fn repr(res: &Resolved) -> String {
    match res {
        Resolved::Color { solved, .. } => solved.hex().to_string(),
        Resolved::None => "none".to_string(),
        Resolved::Unreachable(_) => "UNREACHABLE".to_string(),
    }
}

/// Собрать карту `role.key() -> hex` из дефолтной таблицы для (bg, vc).
fn default_by_key(bg: &BgInput, vc: &ViewingConditions) -> Vec<(&'static str, String)> {
    resolve_set(bg, &RoleTable::default(), vc)
        .into_iter()
        .map(|(role, res)| (role.key(), repr(&res)))
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. Байт-в-байт эквивалентность на всех 240 точках.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn labui_named_set_is_byte_identical_to_default_role_table() {
    let table = labui_reference()
        .compile_named_role_table()
        .expect("эталонная фикстура labui обязана компилироваться");

    // Фикстура покрывает ровно 20 сегодняшних ролей, имена = Role::key().
    assert_eq!(
        table.entries().len(),
        Role::ALL.len(),
        "фикстура labui должна нести ровно {} ролей",
        Role::ALL.len()
    );

    let (vcs, bgs) = grid();
    let mut compared = 0usize;
    for (vc, _vc_name) in vcs {
        for bg_hex in bgs {
            let bg = BgInput::solid(bg_hex).unwrap();
            let named = resolve_named_set(&bg, &table, &vc);
            let default_map = default_by_key(&bg, &vc);

            for (name, res) in &named {
                let got = repr(res);
                let want = default_map
                    .iter()
                    .find(|(k, _)| k == name)
                    .map(|(_, hex)| hex.clone())
                    .unwrap_or_else(|| panic!("нет дефолтной роли с ключом `{name}`"));
                assert_eq!(
                    got, want,
                    "БАЙТ-ДРИФТ {bg_hex}/{_vc_name} `{name}`: config={got}, default={want}"
                );
                compared += 1;
            }
        }
    }
    // 20 ролей × 2 VC × 6 фонов = 240.
    assert_eq!(compared, 240, "должно сравниться ровно 240 точек");
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. RED-proof байт-в-байт: мутация рецепта фикстуры роняет тест.
//    Доказывает, что тест выше КУСАЕТСЯ (не green-from-birth).
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn byte_identity_test_bites_on_mutated_recipe() {
    // Мутируем ОДИН рецепт: label-primary fraction 0.968 → 0.627 (контракт
    // secondary). Эмиссия label-primary обязана разойтись с дефолтом хотя бы на
    // одном фоне — иначе байт-в-байт тест был бы слеп к рецепту.
    let mut cfg = labui_reference();
    for (name, recipe) in &mut cfg.roles {
        if name == "label-primary" {
            *recipe = RoleRecipe::TextAnchor {
                fraction: 0.627,
                floor: Floor::AaText,
            };
        }
    }
    let mutated = cfg
        .compile_named_role_table()
        .expect("мутант всё ещё валиден (fraction в пределах)");

    let (vcs, bgs) = grid();
    let mut any_diff = false;
    for (vc, _n) in vcs {
        for bg_hex in bgs {
            let bg = BgInput::solid(bg_hex).unwrap();
            let named = resolve_named_set(&bg, &mutated, &vc);
            let default_map = default_by_key(&bg, &vc);
            for (name, res) in &named {
                if name == "label-primary" {
                    let got = repr(res);
                    let want = default_map
                        .iter()
                        .find(|(k, _)| k == name)
                        .map(|(_, hex)| hex.clone())
                        .unwrap();
                    if got != want {
                        any_diff = true;
                    }
                }
            }
        }
    }
    assert!(
        any_diff,
        "RED-proof провален: мутация рецепта label-primary НЕ изменила эмиссию — \
         байт-в-байт тест был бы слеп"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Валидатор: эталон валиден; каждая ручка за пределом даёт ConfigError.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn labui_reference_passes_validation() {
    assert_eq!(labui_reference().validate(), Ok(()));
}

/// Мутировать первый рецепт данного вида и вернуть конфиг.
fn with_role_recipe(name: &str, recipe: RoleRecipe) -> ThemeConfig {
    let mut cfg = labui_reference();
    let entry = cfg
        .roles
        .iter_mut()
        .find(|(rname, _)| rname == name)
        .unwrap_or_else(|| panic!("роль `{name}` отсутствует в фикстуре"));
    entry.1 = recipe;
    cfg
}

#[test]
fn fraction_out_of_bounds_is_rejected() {
    // > 1 отклоняется.
    let over = with_role_recipe(
        "label-primary",
        RoleRecipe::TextAnchor {
            fraction: 1.5,
            floor: Floor::AaText,
        },
    );
    assert!(matches!(
        over.validate(),
        Err(ConfigError::OutOfBounds { handle, .. }) if handle == "roles.label-primary.fraction"
    ));
    // ≤ 0 отклоняется.
    let zero = with_role_recipe(
        "label-primary",
        RoleRecipe::TextAnchor {
            fraction: 0.0,
            floor: Floor::AaText,
        },
    );
    assert!(matches!(
        zero.validate(),
        Err(ConfigError::OutOfBounds { .. })
    ));
}

#[test]
fn fraction_bound_red_proof_at_edges() {
    // RED-proof предела: 1.0 валиден (верхняя граница включительна), 1.0+ε — нет.
    let at = with_role_recipe(
        "label-primary",
        RoleRecipe::TextAnchor {
            fraction: 1.0,
            floor: Floor::AaText,
        },
    );
    assert_eq!(at.validate(), Ok(()), "fraction=1.0 должен быть валиден");
    let over = with_role_recipe(
        "label-primary",
        RoleRecipe::TextAnchor {
            fraction: 1.0 + 1e-9,
            floor: Floor::AaText,
        },
    );
    assert!(
        over.validate().is_err(),
        "fraction чуть выше 1.0 обязан упасть — иначе предел не кусается"
    );
}

#[test]
fn dj_anchor_non_positive_is_rejected() {
    for recipe in [
        RoleRecipe::DjAnchor {
            light: 0.0,
            dark: 5.0,
        },
        RoleRecipe::DjAnchor {
            light: 5.0,
            dark: -1.0,
        },
    ] {
        let cfg = with_role_recipe("fill-primary", recipe);
        assert!(
            matches!(cfg.validate(), Err(ConfigError::OutOfBounds { .. })),
            "нулевой/отрицательный dJ' обязан отклоняться"
        );
    }
}

#[test]
fn dj_anchor_bound_red_proof() {
    // Строго положительный предел: +ε валиден, 0.0 — нет.
    let ok = with_role_recipe(
        "fill-primary",
        RoleRecipe::DjAnchor {
            light: f64::MIN_POSITIVE,
            dark: 1.0,
        },
    );
    assert_eq!(ok.validate(), Ok(()));
    let bad = with_role_recipe(
        "fill-primary",
        RoleRecipe::DjAnchor {
            light: 0.0,
            dark: 1.0,
        },
    );
    assert!(bad.validate().is_err());
}

#[test]
fn decorative_lc_non_positive_is_rejected() {
    let cfg = with_role_recipe("shadow-minor", RoleRecipe::DecorativeLc { magnitude: 0.0 });
    assert!(matches!(
        cfg.validate(),
        Err(ConfigError::OutOfBounds { handle, .. }) if handle == "roles.shadow-minor.magnitude"
    ));
}

#[test]
fn tint_ratio_out_of_bounds_is_rejected() {
    let mut cfg = labui_reference();
    cfg.neutral.tint.ratio = 1.5;
    assert!(matches!(
        cfg.validate(),
        Err(ConfigError::OutOfBounds { handle, .. }) if handle == "neutral.tint.ratio"
    ));
    let mut neg = labui_reference();
    neg.neutral.tint.ratio = -0.01;
    assert!(neg.validate().is_err());
}

#[test]
fn tint_ratio_bound_red_proof_at_edges() {
    // [0, 1] замкнут: 0.0 и 1.0 валидны, вне — нет.
    for r in [0.0, 1.0] {
        let mut cfg = labui_reference();
        cfg.neutral.tint.ratio = r;
        assert_eq!(
            cfg.validate(),
            Ok(()),
            "ratio={r} на границе должен быть валиден"
        );
    }
    let mut over = labui_reference();
    over.neutral.tint.ratio = 1.0 + 1e-9;
    assert!(over.validate().is_err());
}

#[test]
fn target_mp_non_positive_is_rejected() {
    let mut cfg = labui_reference();
    cfg.neutral.tint.target_mp = 0.0;
    assert!(matches!(
        cfg.validate(),
        Err(ConfigError::OutOfBounds { handle, .. }) if handle == "neutral.tint.target_mp"
    ));
}

#[test]
fn hue_stiffness_negative_is_rejected() {
    let mut cfg = labui_reference();
    cfg.neutral.tint.hue_stiffness = -1.0;
    assert!(cfg.validate().is_err());
    // RED-proof: 0.0 валиден (нижняя граница включительна).
    let mut zero = labui_reference();
    zero.neutral.tint.hue_stiffness = 0.0;
    assert_eq!(zero.validate(), Ok(()));
}

#[test]
fn hardness_below_one_is_rejected() {
    let mut cfg = labui_reference();
    cfg.sentiments.hardness = 0.5;
    assert!(matches!(
        cfg.validate(),
        Err(ConfigError::OutOfBounds { handle, .. }) if handle == "sentiments.hardness"
    ));
    // RED-proof: ровно 1.0 валиден.
    let mut at = labui_reference();
    at.sentiments.hardness = 1.0;
    assert_eq!(at.validate(), Ok(()));
}

#[test]
fn chroma_fraction_out_of_bounds_is_rejected() {
    let mut over = labui_reference();
    over.sentiments.chroma_fraction = 1.01;
    assert!(over.validate().is_err());
    let mut zero = labui_reference();
    zero.sentiments.chroma_fraction = 0.0;
    assert!(zero.validate().is_err());
    // RED-proof: ровно 1.0 валиден.
    let mut at = labui_reference();
    at.sentiments.chroma_fraction = 1.0;
    assert_eq!(at.validate(), Ok(()));
}

#[test]
fn hue_floor_out_of_range_is_rejected() {
    let mut over = labui_reference();
    over.sentiments.categories[1].hue_floor_deg = Some(360.0);
    assert!(
        over.validate().is_err(),
        "360° ≡ 0°, за полуинтервалом [0,360)"
    );
    let mut neg = labui_reference();
    neg.sentiments.categories[1].hue_floor_deg = Some(-1.0);
    assert!(neg.validate().is_err());
    // RED-proof: 0.0 валиден, чуть ниже 360 валиден.
    let mut lo = labui_reference();
    lo.sentiments.categories[1].hue_floor_deg = Some(0.0);
    assert_eq!(lo.validate(), Ok(()));
    let mut hi = labui_reference();
    hi.sentiments.categories[1].hue_floor_deg = Some(359.999);
    assert_eq!(hi.validate(), Ok(()));
}

// ─────────────────────────────────────────────────────────────────────────────
// Валидатор: hex / имена / ссылки.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn invalid_hex_is_rejected() {
    let mut cfg = labui_reference();
    cfg.brand.anchor_hex = "not-a-hex".to_string();
    assert!(matches!(
        cfg.validate(),
        Err(ConfigError::InvalidHex { field, .. }) if field == "brand.anchor_hex"
    ));
    let mut neut = labui_reference();
    neut.neutral.anchors.dark = "#GGGGGG".to_string();
    assert!(matches!(
        neut.validate(),
        Err(ConfigError::InvalidHex { .. })
    ));
}

#[test]
fn invalid_role_name_is_rejected() {
    let mut cfg = labui_reference();
    // Заглавные буквы недопустимы ([a-z0-9-]+).
    cfg.roles.push((
        "Label_Bad".to_string(),
        RoleRecipe::TextAnchor {
            fraction: 0.5,
            floor: Floor::AaUi,
        },
    ));
    assert!(matches!(
        cfg.validate(),
        Err(ConfigError::InvalidName { .. })
    ));
}

#[test]
fn sentiment_referencing_missing_family_is_rejected() {
    let mut cfg = labui_reference();
    cfg.sentiments.categories[0].family = "nonexistent".to_string();
    assert!(matches!(
        cfg.validate(),
        Err(ConfigError::UnknownFamily { family, .. }) if family == "nonexistent"
    ));
}

#[test]
fn alias_to_missing_role_is_rejected() {
    let mut cfg = labui_reference();
    cfg.aliases
        .push(("control-bg".to_string(), "no-such-role".to_string()));
    assert!(matches!(
        cfg.validate(),
        Err(ConfigError::UnknownFamily { family, .. }) if family == "no-such-role"
    ));
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. Честные заглушки t2.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ladder_recipe_is_not_yet_implemented() {
    let cfg = with_role_recipe("fill-primary", RoleRecipe::Ladder);
    // Валидация проходит (тип верный, пределов нет), а компиляция — честная заглушка.
    assert_eq!(cfg.validate(), Ok(()));
    assert!(matches!(
        cfg.compile_named_role_table(),
        Err(ConfigError::NotYetImplemented {
            recipe: "ladder",
            ..
        })
    ));
}

#[test]
fn alpha_analog_recipe_is_not_yet_implemented() {
    let cfg = with_role_recipe("fill-primary", RoleRecipe::AlphaAnalog);
    assert!(matches!(
        cfg.compile_named_role_table(),
        Err(ConfigError::NotYetImplemented {
            recipe: "alpha_analog",
            ..
        })
    ));
}

#[test]
fn config_error_display_is_russian_and_informative() {
    let err = ConfigError::OutOfBounds {
        handle: "roles.x.fraction".to_string(),
        value: 2.0,
        bound: "0 < fraction ≤ 1",
    };
    let s = err.to_string();
    assert!(s.contains("roles.x.fraction"));
    assert!(s.contains("вне предела"));
}
