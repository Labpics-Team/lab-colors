//! Тесты границы конфига (CH-02 t1):
//! 1. Байт-в-байт: `resolve_named_set(labui_reference)` эмитит идентично
//!    `resolve_set(RoleTable::default)` по всем 240 точкам golden-грида.
//! 2. RED-proof байт-в-байт: мутация одного рецепта фикстуры роняет тест.
//! 3. Валидатор: за-предельное значение КАЖДОЙ ручки даёт `ConfigError` +
//!    RED-proof мутацией предела (валидный vs невалидный на границе).
//! 4. t2: Ladder/AlphaAnalog компилируются в rgba-специи; diff=пусто против
//!    consumedRoles; S_PERC_MIN-идентичность; значенческая сверка со стабом
//!    labui (light+dark) + RED-proof мутаций.

use super::*;
use crate::ladder::LadderPosition;
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
        // rgba-роль: тинт + фактическая альфа — то, что эмитится `--lab-*`.
        Resolved::Rgba(r) => format!("rgba({},{})", r.tint_hex(), r.alpha()),
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

    // Фикстура t2 несёт 20 сегодняшних ролей ПЛЮС акцентную/сентимент/FX/альфа
    // лестницу (t2). Байт-в-байт гарантия — на 20 СЕГОДНЯШНИХ ролях (имена =
    // Role::key()): именно их пинит owner-approved golden. Проверяем, что каждая
    // из 20 присутствует и эмитит идентично дефолтной таблице на всех точках.
    let core_keys: Vec<&'static str> = Role::ALL.iter().map(|r| r.key()).collect();
    for key in &core_keys {
        assert!(
            table.entries().iter().any(|(n, _)| n == key),
            "фикстура labui обязана нести сегодняшнюю роль `{key}`"
        );
    }

    let (vcs, bgs) = grid();
    let mut compared = 0usize;
    for (vc, _vc_name) in vcs {
        for bg_hex in bgs {
            let bg = BgInput::solid(bg_hex).unwrap();
            let named = resolve_named_set(&bg, &table, &vc);
            let default_map = default_by_key(&bg, &vc);

            // Сравниваем ТОЛЬКО 20 сегодняшних ролей (акцентные — новые, у них нет
            // дефолт-аналога; их покрывает diff=пусто тест против consumedRoles).
            for (name, res) in &named {
                if !core_keys.contains(&name.as_str()) {
                    continue;
                }
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
    assert_eq!(
        compared, 240,
        "должно сравниться ровно 240 сегодняшних точек"
    );
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
    cfg.brand.anchors.light = "not-a-hex".to_string();
    assert!(matches!(
        cfg.validate(),
        Err(ConfigError::InvalidHex { field, .. }) if field == "brand.anchors.light"
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
    // Уникальное имя алиаса: дубликат существующего поймался бы раньше как
    // DuplicateKey — здесь проверяется именно различимая ошибка ссылки.
    cfg.aliases
        .push(("probe-unique-alias".to_string(), "no-such-role".to_string()));
    assert!(matches!(
        cfg.validate(),
        Err(ConfigError::UnknownRole { role, .. }) if role == "no-such-role"
    ));
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. Честные заглушки t2.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ladder_recipe_compiles_to_rgba_spec() {
    // t2: Ladder больше не заглушка — компилируется в RoleSpec::Ladder.
    let cfg = with_role_recipe(
        "fill-primary",
        RoleRecipe::Ladder {
            source: LadderSource::Brand,
            position: LadderPosition::FillPrimary,
        },
    );
    assert_eq!(cfg.validate(), Ok(()));
    let table = cfg
        .compile_named_role_table()
        .expect("Ladder компилируется");
    let (_, spec) = table
        .entries()
        .iter()
        .find(|(n, _)| n == "fill-primary")
        .unwrap();
    assert!(
        matches!(spec, RoleSpec::Ladder { alpha_light, alpha_dark, .. }
            if (*alpha_light - 0.122).abs() < 1e-12 && (*alpha_dark - 0.122).abs() < 1e-12),
        "Ladder(FillPrimary) обязан нести альфу @12 (обе темы); получено {spec:?}"
    );
}

#[test]
fn alpha_analog_recipe_compiles_to_rgba_spec() {
    let cfg = with_role_recipe(
        "fill-primary",
        RoleRecipe::AlphaAnalog {
            of: LadderSource::Brand,
            alpha: 0.122,
        },
    );
    assert_eq!(cfg.validate(), Ok(()));
    let table = cfg
        .compile_named_role_table()
        .expect("AlphaAnalog компилируется");
    let (_, spec) = table
        .entries()
        .iter()
        .find(|(n, _)| n == "fill-primary")
        .unwrap();
    assert!(
        matches!(spec, RoleSpec::AlphaAnalog { alpha, .. } if (*alpha - 0.122).abs() < 1e-12),
        "AlphaAnalog обязан нести запрошенную альфу; получено {spec:?}"
    );
}

#[test]
fn ladder_source_referencing_missing_family_is_rejected() {
    let cfg = with_role_recipe(
        "fill-primary",
        RoleRecipe::Ladder {
            source: LadderSource::Family("nonexistent".to_string()),
            position: LadderPosition::FillPrimary,
        },
    );
    assert!(matches!(
        cfg.validate(),
        Err(ConfigError::UnknownFamily { family, .. }) if family == "nonexistent"
    ));
}

#[test]
fn alpha_analog_alpha_out_of_bounds_is_rejected() {
    for bad in [0.0, 1.5] {
        let cfg = with_role_recipe(
            "fill-primary",
            RoleRecipe::AlphaAnalog {
                of: LadderSource::Brand,
                alpha: bad,
            },
        );
        assert!(
            matches!(cfg.validate(), Err(ConfigError::OutOfBounds { .. })),
            "alpha={bad} обязана отклоняться"
        );
    }
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

// ─────────────────────────────────────────────────────────────────────────────
// t2: diff=пусто против consumedRoles labui.
// ─────────────────────────────────────────────────────────────────────────────

/// Полный контракт `--lab-*` labui из `packages/colors-stub/roles.json`
/// (снят 2026-07-02, источник в шапке файла: генерируется из
/// `reference/labui-tokens-snapshot.dtcg.json`). Захардкожен здесь как SSOT для
/// diff-теста — при регенерации roles.json обновить этот список синхронно.
///
/// Имена без префикса `--lab-`. IC-режимы зарезервированы (в roles.json не
/// перечислены), поэтому и здесь их нет.
///
/// Компромисс t2: это ЗЕРКАЛО roles.json, не живой файл. Класс дрейфа зеркала
/// закрывается гардами поезда labui (consumed-contract против живой эмиссии) —
/// там diff проверяется против фактического потребления, не против копии.
const LABUI_CONSUMED_ROLES: &[&str] = &[
    // Backgrounds — ВХОДЫ (набор фонов = конфиг потребителя), не роли эмиссии.
    // Labels (core neutral).
    "label-primary",
    "label-secondary",
    "label-tertiary",
    "label-quaternary",
    // Labels — brand/сентименты.
    "label-brand-primary",
    "label-brand-secondary",
    "label-brand-tertiary",
    "label-brand-quaternary",
    "label-danger-primary",
    "label-danger-secondary",
    "label-danger-tertiary",
    "label-danger-quaternary",
    "label-warning-primary",
    "label-warning-secondary",
    "label-warning-tertiary",
    "label-warning-quaternary",
    "label-success-primary",
    "label-success-secondary",
    "label-success-tertiary",
    "label-success-quaternary",
    "label-info-primary",
    "label-info-secondary",
    "label-info-tertiary",
    "label-info-quaternary",
    // Fills (core neutral).
    "fill-primary",
    "fill-secondary",
    "fill-tertiary",
    "fill-quaternary",
    "fill-none",
    // Fills — brand/сентименты.
    "fill-brand-primary",
    "fill-brand-secondary",
    "fill-brand-tertiary",
    "fill-brand-quaternary",
    "fill-danger-primary",
    "fill-danger-secondary",
    "fill-danger-tertiary",
    "fill-danger-quaternary",
    "fill-warning-primary",
    "fill-warning-secondary",
    "fill-warning-tertiary",
    "fill-warning-quaternary",
    "fill-success-primary",
    "fill-success-secondary",
    "fill-success-tertiary",
    "fill-success-quaternary",
    "fill-info-primary",
    "fill-info-secondary",
    "fill-info-tertiary",
    "fill-info-quaternary",
    // Border (core neutral).
    "border-strong",
    "border-base",
    "border-soft",
    "border-ghost",
    // Border — brand/сентименты.
    "border-brand-strong",
    "border-brand-base",
    "border-brand-soft",
    "border-danger-strong",
    "border-danger-base",
    "border-danger-soft",
    "border-warning-strong",
    "border-warning-base",
    "border-warning-soft",
    "border-success-strong",
    "border-success-base",
    "border-success-soft",
    "border-info-strong",
    "border-info-base",
    "border-info-soft",
    // FX (не-теневые).
    "fx-focus-ring-brand",
    "fx-focus-ring-danger",
    "fx-focus-ring-warning",
    "fx-focus-ring-neutral",
    "fx-glow-brand",
    "fx-glow-danger",
    "fx-glow-warning",
    "fx-glow-neutral",
    "fx-glow-inverted",
    "fx-skeleton-base",
    "fx-skeleton-highlight",
    // FX shadow — эмитятся как shadow-* (labui читает как fx-shadow-* через alias).
    "shadow-minor",
    "shadow-ambient",
    "shadow-penumbra",
    "shadow-major",
    // Component.
    "fill-accent",
    "fill-neutral",
    "fill-danger",
    "fill-accent-tinted",
    "fill-neutral-tinted",
    "fill-danger-tinted",
    "label-accent",
    "label-danger",
    "border-accent",
    "border-neutral",
    "border-danger",
    "border-focus",
    // Прочие эмитируемые нейтральные (icon/separator/none — core).
    "icon",
    "separator",
    "none",
];

/// Роли consumedRoles labui, УДАЛЯЕМЫЕ по коллапсу контракта (inventory §4):
/// каждая с причиной. Diff-тест исключает их из требуемого покрытия — они не
/// эмитируются движком (роль решается от фактического фона / материал = флаг).
const COLLAPSED_ROLES: &[(&str, &str)] = &[
    // Материал = ФЛАГ фона (Backgrounds+Materials схлопнуты), не роль эмиссии.
    ("bg-material-*", "материал = флаг фона, не роль"),
    // Роль решается от ФАКТИЧЕСКОГО фона — static-*/inverted-* не нужны.
    ("*-static-dark-*", "роль от фона: статик-тёмный фон = вход"),
    (
        "*-static-light-*",
        "роль от фона: статик-светлый фон = вход",
    ),
    ("label-inverted-*", "роль от фона: инверсия = вход-фон"),
    ("border-inverted", "роль от фона: инверсия = вход-фон"),
    // on-* лейблы выброшены (солвер от фона снизу, 36→~4).
    ("label-on-accent", "on-* выброшены: лейбл решается от фона"),
    ("label-on-neutral", "on-* выброшены: лейбл решается от фона"),
    ("label-on-danger", "on-* выброшены: лейбл решается от фона"),
    // Фоны/оверлеи — ВХОДЫ (набор фонов = конфиг потребителя) или alpha.rs-роли.
    ("bg-*", "набор фонов = конфиг потребителя, не роль эмиссии"),
    (
        "bg-overlay-*",
        "оверлеи → alpha.rs-роли (вне поглощаемого GAP)",
    ),
    // Компонентные алиасы (badge/control) — конфиг-алиасы, не рецепты.
    ("badge-*", "компонентный алиас, не рецепт эмиссии"),
    ("control-bg", "компонентный алиас, не рецепт эмиссии"),
];

/// diff = ПУСТО: каждая consumedRole labui (минус удаляемые по коллапсу)
/// эмитируется фикстурой. Это несущий тест t2 — поглощение акцентного GAP #59.
///
/// Удаляемые перечислены явно с причиной ([`COLLAPSED_ROLES`]) — тест не «прощает»
/// их молча, а декларирует, ПОЧЕМУ они не эмитируются (материал=флаг, роль от
/// фона, on-* выброшены, фоны=входы, алиасы).
#[test]
fn consumed_roles_diff_is_empty_against_labui_contract() {
    let cfg = labui_reference();
    let table = cfg
        .compile_named_role_table()
        .expect("фикстура labui компилируется");
    // Покрытие = эмитируемые роли ∪ компонентные алиасы (стаб алиасит нейтральные
    // компонентные роли через var() на core-роли — они покрыты алиасом, не рецептом).
    let mut covered: std::collections::HashSet<&str> =
        table.entries().iter().map(|(n, _)| n.as_str()).collect();
    for (alias, _) in &cfg.aliases {
        covered.insert(alias.as_str());
    }

    // Каждая требуемая (не-коллапс) роль обязана быть покрыта (рецептом или алиасом).
    let mut missing = Vec::new();
    for role in LABUI_CONSUMED_ROLES {
        if !covered.contains(role) {
            missing.push(*role);
        }
    }
    assert!(
        missing.is_empty(),
        "diff НЕ пуст: фикстура не эмитирует consumedRoles labui: {missing:?}\n\
         (удаляемые по коллапсу перечислены в COLLAPSED_ROLES с причинами)"
    );

    // Обратная сторона: фикстура не эмитит НИ ОДНОЙ коллапс-роли (иначе коллапс
    // не исполнен). Проверяем по конкретным маркерам удаляемых семейств
    // (`fx-glow-inverted` — легитимная FX-роль, НЕ инвертированный лейбл/бордер).
    for (name, _) in table.entries() {
        let collapsed = name.contains("static")
            || name.starts_with("label-inverted")
            || name == "border-inverted"
            || name.starts_with("label-on-")
            || name.starts_with("bg-")
            || name.starts_with("badge-")
            || name == "control-bg"
            || name.contains("material");
        assert!(
            !collapsed,
            "фикстура эмитит коллапс-роль `{name}` — коллапс контракта нарушен"
        );
    }
    // COLLAPSED_ROLES не пуст — декларация причин присутствует.
    assert!(!COLLAPSED_ROLES.is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// t2 №д: S_PERC_MIN — деривационная идентичность из конфиг-якорей.
// ─────────────────────────────────────────────────────────────────────────────

/// `S_PERC_MIN`, пересчитанный из хром 4 сентимент-якорей labui, совпадает с
/// замороженной константой (`0.068_703_9`, допуск 1e-4) — закон
/// `2·C_rep·sin(20°/2)` остаётся законом, сегодняшнее значение — его частный
/// случай при labui-якорях (поправка t2 №д).
#[test]
fn s_perc_min_recomputed_from_config_anchors_matches_frozen() {
    let recomputed = labui_reference()
        .sentiment_s_perc_min()
        .expect("фикстура валидна");
    let frozen = crate::sentiment::s_perc_min_frozen();
    assert!(
        (recomputed - frozen).abs() < 1e-4,
        "S_PERC_MIN(labui-якоря) = {recomputed} != замороженной {frozen} (допуск 1e-4)"
    );
    // Нетавтологичный пин самой замороженной величины.
    assert!(
        (recomputed - 0.068_703_9).abs() < 1e-4,
        "S_PERC_MIN = {recomputed} != 0.068_703_9 (Witzel 2013 · 20°)"
    );
}

/// RED-proof пересчёта: подмена якоря сентимента (danger red → зелёный, иная
/// хрома) сдвигает `S_PERC_MIN` — иначе пересчёт был бы слеп к якорям.
#[test]
fn s_perc_min_recompute_bites_on_anchor_mutation() {
    let base = labui_reference()
        .sentiment_s_perc_min()
        .expect("фикстура валидна");
    let mut cfg = labui_reference();
    // Danger маппится на red; подменим red-якорь на серый (низкая хрома) →
    // C_rep падает → S_PERC_MIN падает.
    for fam in &mut cfg.palette {
        if fam.key == "red" {
            fam.anchors.light = "#808080".to_string();
        }
    }
    let mutated = cfg
        .sentiment_s_perc_min()
        .expect("мутация якоря сохраняет валидность");
    assert!(
        (base - mutated).abs() > 1e-3,
        "RED-proof провален: подмена якоря НЕ сдвинула S_PERC_MIN ({base} vs {mutated})"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// t2 №г: сентимент — деривационная идентичность (тинт == сырой якорь при
// labui-бренде).
// ─────────────────────────────────────────────────────────────────────────────

/// Деривационная идентичность (поправка t2 №г): при бренде labui сентимент-тинт
/// совпадает с СЫРЫМ якорем семейства (по всем 4 темам) для сентиментов,
/// ОТСТОЯЩИХ от бренда дальше перцептивного порога `s_min`.
///
/// ЧЕСТНАЯ НАХОДКА (не подгонка): для Danger/Success/Warning идентичность
/// держится (их семейства далеки от синего бренда labui). Для **Info** она НЕ
/// держится: Info→Blue (Oklab h≈259.9°) отстоит от бренда `#007AFF` (h≈257.4°)
/// лишь на ≈2.5° — НИЖЕ порога разделения (`S_PERC_MIN`≈0.0687 хорды ≈ 3.5° при
/// хроме blue). Сентимент-солвер КОРРЕКТНО смещает Info, чтобы он был отличим от
/// бренда (иначе «информационный» и «брендовый» синий слились бы). Это
/// заземлённое поведение солвера (#20/#55/#65), а не баг: сырой якорь совпадал
/// бы лишь если бренд был далёк от синего. Расхождение задокументировано, не
/// спрятано — отдельным тестом [`info_is_displaced_from_blue_brand_by_design`].
#[test]
fn sentiment_tint_is_raw_family_anchor_when_brand_is_hue_distant() {
    let cfg = labui_reference();
    let table = cfg.compile_named_role_table().unwrap();

    // Сентименты, чьи семейства ДАЛЕКИ от синего бренда (> s_min): идентичность
    // держится. Info исключён намеренно (см. доку теста + отдельный тест ниже).
    //
    // Проверяем на СВЕТЛОЙ теме — каноническом кейсе поправки г (бренд labui =
    // светлый `#007AFF`). Пер-темные варианты имеют СВОЙ пер-темный бренд-оттенок
    // (reference §2), поэтому их разведение отличается — это отдельная нюансировка
    // (см. `per_theme_brand_shifts_sentiment_displacement`), не нарушение г.
    let cases: &[(&str, &str)] = &[
        ("fill-danger-primary", "red"),
        ("fill-success-primary", "green"),
        ("fill-warning-primary", "orange"),
    ];
    let vc = ViewingConditions::srgb(); // светлая тема, brand = #007AFF

    for (role, fam_key) in cases {
        let fam = cfg.palette.iter().find(|f| &f.key == fam_key).unwrap();
        let (_, spec) = table.entries().iter().find(|(n, _)| n == role).unwrap();
        let RoleSpec::Ladder { tint, .. } = spec else {
            panic!("{role}: ожидался Ladder-спек, получено {spec:?}");
        };
        let got_hex = crate::spaces::srgb::hex_from_srgb_encoded(tint.for_vc(&vc));
        let want_hex = crate::spaces::srgb::hex_from_srgb_encoded(
            crate::spaces::srgb::srgb_encoded_from_hex(&fam.anchors.light).unwrap(),
        );
        assert_eq!(
            got_hex, want_hex,
            "ДЕРИВАЦИОННАЯ ИДЕНТИЧНОСТЬ НЕ СОШЛАСЬ (светлая тема): `{role}`: \
             сентимент-тинт {got_hex} != сырой якорь {fam_key} {want_hex}. \
             Сентимент-солвер сместил оттенок при labui-бренде — осмыслить, не прятать."
        );
    }
}

/// ЧЕСТНАЯ ФИКСАЦИЯ расхождения деривационной идентичности для Info (не подгонка).
///
/// Info→Blue отстоит от синего бренда labui лишь на ≈2.5° Oklab — ниже
/// перцептивного порога разделения. Сентимент-солвер СМЕЩАЕТ Info прочь от
/// бренда (иначе информационный и брендовый синий слились бы). Тест закрепляет:
/// (1) Info-тинт ≠ сырой якорь blue (смещён), (2) но остаётся синим (не уехал в
/// другой квадрант). Это поведение по построению — задокументировано тестом,
/// а не спрятано.
#[test]
fn info_is_displaced_from_blue_brand_by_design() {
    let cfg = labui_reference();
    let table = cfg.compile_named_role_table().unwrap();
    let (_, spec) = table
        .entries()
        .iter()
        .find(|(n, _)| n == "fill-info-primary")
        .unwrap();
    let RoleSpec::Ladder { tint, .. } = spec else {
        panic!("fill-info-primary: ожидался Ladder");
    };
    let vc = ViewingConditions::srgb();
    let got_hex = crate::spaces::srgb::hex_from_srgb_encoded(tint.for_vc(&vc));
    // (1) Смещён от сырого якоря blue #3E87FF.
    assert_ne!(
        got_hex, "#3E87FF",
        "Info НЕ смещён от бренда — солвер разделения не сработал (регресс #20/#55)"
    );
    // (2) Остался синим (Oklab-оттенок в сине-фиолетовой полосе 230–290°),
    // не уехал в другой квадрант.
    let hue = crate::accent::oklab_hue_of(&got_hex);
    assert!(
        (230.0..=290.0).contains(&hue),
        "смещённый Info уехал из сине-фиолетовой полосы: h={hue:.1}° ({got_hex})"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// t2: rgba-эмиссия + RED-proof мутаций (позиция/семейство/альфа → RED).
// ─────────────────────────────────────────────────────────────────────────────

/// Резолв Ladder-роли несёт rgba(тинт, α) + солид-композит на фоне резолва.
/// Тинт brand-роли по светлой теме == светлый якорь бренда (эмитится напрямую);
/// композит — то, что реально показывается на белом фоне.
#[test]
fn ladder_emits_rgba_with_composite_over_bg() {
    let table = labui_reference().compile_named_role_table().unwrap();
    let bg = BgInput::solid("#FFFFFF").unwrap();
    let vc = ViewingConditions::srgb();
    let set = resolve_named_set(&bg, &table, &vc);

    let (_, res) = set
        .iter()
        .find(|(n, _)| n == "fill-brand-secondary")
        .unwrap();
    let Resolved::Rgba(r) = res else {
        panic!("fill-brand-secondary: ожидался Rgba, получено {res:?}");
    };
    // Тинт brand light = #007AFF (эмитится напрямую).
    assert_eq!(r.tint_hex(), "#007AFF", "тинт brand-роли (светлая тема)");
    // Альфа позиции fill-secondary = @8.
    assert!((r.alpha() - 0.078).abs() < 1e-12, "альфа fill-secondary @8");
    // Композит #007AFF@0.078 над #FFFFFF — то, что реально красится.
    let want_composite = crate::alpha::composite_hex("#007AFF", 0.078, "#FFFFFF").unwrap();
    assert_eq!(r.composite_hex(), want_composite, "композит на белом фоне");
    // Контраст меряется на композите (близок к нулю для очень прозрачной заливки).
    assert!(
        r.composite_wcag() >= 1.0 && r.composite_wcag() <= 21.0,
        "WCAG композита вне [1,21]: {}",
        r.composite_wcag()
    );
}

/// RED-proof: подмена ПОЗИЦИИ лестницы (fill-secondary @8 → label-primary солид)
/// меняет эмитируемую альфу — иначе рецепт был бы слеп к позиции.
#[test]
fn ladder_bites_on_position_mutation() {
    let mut cfg = labui_reference();
    for (name, recipe) in &mut cfg.roles {
        if name == "fill-brand-secondary" {
            *recipe = RoleRecipe::Ladder {
                source: LadderSource::Brand,
                position: LadderPosition::LabelPrimary, // солид вместо @8
            };
        }
    }
    let table = cfg.compile_named_role_table().unwrap();
    let bg = BgInput::solid("#FFFFFF").unwrap();
    let set = resolve_named_set(&bg, &table, &ViewingConditions::srgb());
    let (_, res) = set
        .iter()
        .find(|(n, _)| n == "fill-brand-secondary")
        .unwrap();
    let Resolved::Rgba(r) = res else {
        panic!("ожидался Rgba")
    };
    assert!(
        (r.alpha() - 1.0).abs() < 1e-12,
        "RED-proof позиции провален: альфа не сменилась на солид (1.0), а = {}",
        r.alpha()
    );
}

/// RED-proof: подмена СЕМЕЙСТВА источника (danger→red на success→green) меняет
/// эмитируемый тинт — иначе рецепт был бы слеп к источнику.
#[test]
fn ladder_bites_on_family_source_mutation() {
    let base = labui_reference().compile_named_role_table().unwrap();
    let bg = BgInput::solid("#FFFFFF").unwrap();
    let vc = ViewingConditions::srgb();
    let base_tint = {
        let set = resolve_named_set(&bg, &base, &vc);
        let (_, res) = set
            .iter()
            .find(|(n, _)| n == "fill-danger-primary")
            .unwrap();
        res.rgba().unwrap().tint_hex().to_string()
    };

    let mut cfg = labui_reference();
    for (name, recipe) in &mut cfg.roles {
        if name == "fill-danger-primary" {
            *recipe = RoleRecipe::Ladder {
                source: LadderSource::Family("green".to_string()),
                position: LadderPosition::FillPrimary,
            };
        }
    }
    let mutated = cfg.compile_named_role_table().unwrap();
    let mutated_tint = {
        let set = resolve_named_set(&bg, &mutated, &vc);
        let (_, res) = set
            .iter()
            .find(|(n, _)| n == "fill-danger-primary")
            .unwrap();
        res.rgba().unwrap().tint_hex().to_string()
    };
    assert_ne!(
        base_tint, mutated_tint,
        "RED-proof семейства провален: подмена danger→green НЕ сменила тинт ({base_tint})"
    );
}

/// AlphaAnalog-рецепт (#119): солид-цель фиксирована, тинт выводится
/// композит-инверсией. RED-proof: разные α (обе ≥ α_min) дают разный тинт;
/// композит фактической пары ТОЧНО равен солид-цели (теорема тождества #119).
///
/// Фон подобран так, чтобы солид был разрешим при α < 1 (иначе солид над белым
/// вырождается в α_min≈1 — это физика, не баг: полностью насыщенный солид над
/// белым воспроизводится только сплошным цветом).
#[test]
fn alpha_analog_recipe_inverts_and_bites_on_alpha() {
    // Солид-цель = серое семейство `#787880` (точный кейс живых Figma-пар
    // `alpha.rs`), фон — белый: инверсия разрешима при α < 1 (α_min ≈ 0.5), тинт
    // осмысленно меняется с α. (Насыщенный солид с maxed-каналом над белым дал бы
    // α_min = 1 — это физика насыщенного цвета, не годится для RED-proof альфы.)
    let mut base = labui_reference();
    base.palette.push(PaletteFamily {
        key: "probe".to_string(),
        anchors: ThemeAnchors {
            light: "#787880".to_string(),
            dark: "#787880".to_string(),
            light_ic: "#787880".to_string(),
            dark_ic: "#787880".to_string(),
        },
    });
    let bg = BgInput::solid("#FFFFFF").unwrap();
    let vc = ViewingConditions::srgb();

    let resolve_analog = |alpha: f64| -> (String, f64, String) {
        let mut cfg = base.clone();
        cfg.roles.push((
            "probe-tinted".to_string(),
            RoleRecipe::AlphaAnalog {
                of: LadderSource::Family("probe".to_string()),
                alpha,
            },
        ));
        let table = cfg.compile_named_role_table().unwrap();
        let set = resolve_named_set(&bg, &table, &vc);
        let (_, res) = set.iter().find(|(n, _)| n == "probe-tinted").unwrap();
        let r = res.rgba().unwrap();
        (
            r.tint_hex().to_string(),
            r.alpha(),
            r.composite_hex().to_string(),
        )
    };

    let (tint_low, a_low, comp_low) = resolve_analog(0.5);
    let (tint_high, a_high, comp_high) = resolve_analog(0.9);
    // Обе α разрешимы над близким фоном → тинт различается по α (кусается).
    assert!(
        tint_low != tint_high || (a_low - a_high).abs() > 1e-6,
        "RED-proof альфы провален: α=0.5 и α=0.9 дали одно ({tint_low}@{a_low} vs {tint_high}@{a_high})"
    );
    // Теорема тождества #119: композит фактической пары равен солид-цели
    // `#787880` в пределах границы квантования 8-бит (при α<1 точное побайтное
    // восстановление тинта не гарантируется, но композит держится в ±несколько
    // LSB — гарантия из документации `crate::alpha`).
    let target = crate::spaces::srgb::srgb_encoded_from_hex("#787880").unwrap();
    for comp in [&comp_low, &comp_high] {
        let got = crate::spaces::srgb::srgb_encoded_from_hex(comp).unwrap();
        for c in 0..3 {
            let lsb = (got[c] - target[c]).abs() * 255.0;
            assert!(
                lsb <= 3.0,
                "композит альфа-аналога {comp} канал {c} отклонился на {lsb:.2} LSB \
                 от солид-цели #787880 (> 3 LSB — инверсия сломана)"
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// t2 (класс «имена без значений»): значенческий тест фикстуры против стаба.
//
// Класс дефекта: роль присутствует в diff-тесте по ИМЕНИ, но эмитит НЕ ТО
// значение (напр. нейтральный skeleton, ошибочно взятый из семейства blue).
// Здесь эмиссия rgba(тинт, α) представителя каждой группы сверяется со строкой
// стаба contract.css ПОБАЙТНО (нормализованный формат), в light И dark.
// ─────────────────────────────────────────────────────────────────────────────

/// Нормализовать [`Resolved::Rgba`] в канонический `rgb(R G B / A)` (формат стаба
/// labui): тинт-hex → десятичные каналы, альфа как есть. Солид (α=1) → `rgb(R G B)`.
fn rgba_to_stub_string(res: &Resolved) -> String {
    let r = res
        .rgba()
        .unwrap_or_else(|| panic!("ожидался Resolved::Rgba, получено {res:?}"));
    let rgb = crate::spaces::srgb::srgb_encoded_from_hex(r.tint_hex()).unwrap();
    let ch = |v: f64| (v * 255.0).round() as u8;
    let (rr, gg, bb) = (ch(rgb[0]), ch(rgb[1]), ch(rgb[2]));
    if (r.alpha() - 1.0).abs() < 1e-9 {
        format!("rgb({rr} {gg} {bb})")
    } else {
        // Стаб печатает альфу без ведущего нуля целой части и без хвостовых нулей
        // (0.722, 0.2, 0.078…); {} по f64 это воспроизводит для наших величин.
        format!("rgb({rr} {gg} {bb} / {})", r.alpha())
    }
}

/// Значенческая сверка представителей групп против стаба labui в light И dark.
/// Закрывает класс «имя есть, значение врёт»: skeleton = нейтраль #787880 с
/// пер-темной альфой, glow-neutral = белый @52, акценты = пер-темный якорь.
///
/// Исключены НАМЕРЕННО расходящиеся роли (с комментарием-ссылкой):
/// - `border-info-*`/`label-info-*`/`fill-info-*` — оттенок смещён сентимент-
///   солвером относительно бренда (тест `info_is_displaced_from_blue_brand_by_design`);
/// - `fx-focus-ring-neutral` (dark), `fx-glow-inverted`, `fill-neutral` —
///   задокументированные gap-и (пер-темный нейтральный край / inverted-якоря /
///   PROVISIONAL-литерал не выводятся из тройки neutral.anchors).
#[test]
fn representative_roles_match_stub_values_light_and_dark() {
    let table = labui_reference().compile_named_role_table().unwrap();
    let bg_light = BgInput::solid("#FFFFFF").unwrap();
    let bg_dark = BgInput::solid("#101012").unwrap();

    // (роль, стаб-light, стаб-dark). Значения — из contract.css (2026-07-02).
    let cases: &[(&str, &str, &str)] = &[
        // Акцент/сентимент: пер-темный тинт, альфа @72/@12/@52.
        (
            "label-danger-secondary",
            "rgb(255 59 48 / 0.722)",
            "rgb(255 58 58 / 0.722)",
        ),
        (
            "fill-brand-primary",
            "rgb(0 122 255 / 0.122)",
            "rgb(74 143 255 / 0.122)",
        ),
        (
            "border-success-base",
            "rgb(52 199 89 / 0.2)",
            "rgb(48 209 88 / 0.2)",
        ),
        (
            "fx-glow-brand",
            "rgb(0 122 255 / 0.522)",
            "rgb(74 143 255 / 0.522)",
        ),
        // Нейтральные: skeleton #787880 с ПЕР-ТЕМНОЙ альфой (base @8/@12), glow-neutral белый @52.
        (
            "fx-skeleton-base",
            "rgb(120 120 128 / 0.078)",
            "rgb(120 120 128 / 0.122)",
        ),
        (
            "fx-skeleton-highlight",
            "rgb(120 120 128 / 0.039)",
            "rgb(120 120 128 / 0.039)",
        ),
        (
            "fx-glow-neutral",
            "rgb(255 255 255 / 0.522)",
            "rgb(255 255 255 / 0.522)",
        ),
    ];

    for (role, want_light, want_dark) in cases {
        let set_l = resolve_named_set(&bg_light, &table, &ViewingConditions::srgb());
        let set_d = resolve_named_set(&bg_dark, &table, &ViewingConditions::dim_surround());
        let got_l = rgba_to_stub_string(&set_l.iter().find(|(n, _)| n == role).unwrap().1);
        let got_d = rgba_to_stub_string(&set_d.iter().find(|(n, _)| n == role).unwrap().1);
        assert_eq!(
            &got_l, want_light,
            "ЗНАЧЕНИЕ РАЗОШЛОСЬ (light) `{role}`: эмиссия {got_l} != стаб {want_light}"
        );
        assert_eq!(
            &got_d, want_dark,
            "ЗНАЧЕНИЕ РАЗОШЛОСЬ (dark) `{role}`: эмиссия {got_d} != стаб {want_dark}"
        );
    }
}

/// RED-proof значенческого теста: мутация ОДНОЙ альфы (skeleton-base dark @12→@2)
/// роняет сверку — тест кусается, не green-from-birth.
#[test]
fn value_test_bites_on_alpha_mutation() {
    let mut cfg = labui_reference();
    for (name, recipe) in &mut cfg.roles {
        if name == "fx-skeleton-base" {
            // Подменяем позицию на FillQuaternary (@2) — dark-альфа уедет с @12 на @2.
            *recipe = RoleRecipe::Ladder {
                source: LadderSource::Neutral(crate::config::NeutralPick::Mid),
                position: LadderPosition::FillQuaternary,
            };
        }
    }
    let table = cfg.compile_named_role_table().unwrap();
    let bg_dark = BgInput::solid("#101012").unwrap();
    let set = resolve_named_set(&bg_dark, &table, &ViewingConditions::dim_surround());
    let got = rgba_to_stub_string(&set.iter().find(|(n, _)| n == "fx-skeleton-base").unwrap().1);
    assert_ne!(
        got, "rgb(120 120 128 / 0.122)",
        "RED-proof значенческого теста провален: мутация альфы НЕ сдвинула эмиссию"
    );
}

/// Валидатор CodeRabbit-раунда: дубликаты ключей всех словарей отвергаются
/// (повтор имени = неоднозначный lookup), включая алиас, затеняющий роль.
#[test]
fn validator_rejects_duplicate_dictionary_keys() {
    let mut c = labui_reference();
    c.roles.push(c.roles[0].clone());
    assert!(matches!(
        c.validate(),
        Err(ConfigError::DuplicateKey {
            dictionary: "roles",
            ..
        })
    ));

    let mut c = labui_reference();
    c.palette.push(c.palette[0].clone());
    assert!(matches!(
        c.validate(),
        Err(ConfigError::DuplicateKey {
            dictionary: "palette",
            ..
        })
    ));

    let mut c = labui_reference();
    let role_name = c.roles[0].0.clone();
    c.aliases.push((role_name, c.roles[1].0.clone()));
    assert!(matches!(
        c.validate(),
        Err(ConfigError::DuplicateKey {
            dictionary: "roles∪aliases",
            ..
        })
    ));
}

/// preferred_side — закрытое меню {-1, +1}: 0 и 2 отвергаются.
#[test]
fn validator_rejects_preferred_side_outside_closed_menu() {
    for bad in [0i8, 2, -3] {
        let mut c = labui_reference();
        c.sentiments.categories[0].preferred_side = Some(bad);
        assert!(
            matches!(c.validate(), Err(ConfigError::OutOfBounds { .. })),
            "preferred_side={bad} обязан быть отвергнут"
        );
    }
    let mut c = labui_reference();
    c.sentiments.categories[0].preferred_side = Some(-1);
    assert!(c.validate().is_ok(), "-1 легален");
}

/// Неконечные значения ручек (∞/NaN) отвергаются и open-сверху пределами.
#[test]
fn validator_rejects_non_finite_handles() {
    for bad in [f64::INFINITY, f64::NAN] {
        let mut c = labui_reference();
        if let Some((_, RoleRecipe::DjAnchor { light, .. })) = c
            .roles
            .iter_mut()
            .find(|(_, r)| matches!(r, RoleRecipe::DjAnchor { .. }))
        {
            *light = bad;
        } else {
            panic!("в фикстуре обязан быть dj_anchor");
        }
        assert!(
            matches!(c.validate(), Err(ConfigError::OutOfBounds { .. })),
            "dj={bad} обязан быть отвергнут"
        );
    }
}

/// Ошибки ссылок различимы по виду: сентимент/роль/семейство — разные варианты.
#[test]
fn validator_reference_errors_are_distinguishable() {
    let mut c = labui_reference();
    c.roles.push((
        "probe-bad-sentiment".to_string(),
        RoleRecipe::Ladder {
            source: LadderSource::Sentiment("nonexistent".to_string()),
            position: LadderPosition::LabelPrimary,
        },
    ));
    assert!(matches!(
        c.validate(),
        Err(ConfigError::UnknownSentiment { .. })
    ));

    let mut c = labui_reference();
    c.aliases
        .push(("probe-alias".to_string(), "nonexistent-role".to_string()));
    assert!(matches!(c.validate(), Err(ConfigError::UnknownRole { .. })));
}

/// IC-наследование альф закреплено: позиция отдаёт альфу базовой темы и в
/// IC-режиме (IC меняет тинт, не прозрачность — стаб без ic-скоупов).
#[test]
fn ic_inherits_base_theme_alpha() {
    use crate::spaces::vc::ViewingConditions;
    let pos = crate::ladder::LadderPosition::SkeletonBase;
    let light = ViewingConditions::srgb();
    let dark = ViewingConditions::dim_surround();
    let light_ic = ViewingConditions::srgb_high_contrast();
    let dark_ic = ViewingConditions::dim_surround_high_contrast();
    assert_eq!(pos.alpha_for_vc(&light), pos.alpha_for_vc(&light_ic));
    assert_eq!(pos.alpha_for_vc(&dark), pos.alpha_for_vc(&dark_ic));
    // Пер-темная пара реально различается (skeleton-base @8/@12).
    assert!((pos.alpha_for_vc(&light) - pos.alpha_for_vc(&dark)).abs() > 1e-6);
}

/// Алиасы переносятся в скомпилированную таблицу — без переноса алиасные роли
/// контракта терялись бы при эмиссии (major CodeRabbit r2).
#[test]
fn compiled_table_carries_aliases() {
    let table = labui_reference()
        .compile_named_role_table()
        .expect("фикстура компилируется");
    let aliases = table.aliases();
    assert!(!aliases.is_empty(), "фикстура несёт алиасы");
    assert!(
        aliases
            .iter()
            .any(|(a, t)| a == "fill-neutral-tinted" && t == "fill-primary"),
        "алиас fill-neutral-tinted→fill-primary обязан пережить компиляцию"
    );
}

/// Сборка RoleSpec в обход валидатора не даёт правдоподобного мусора:
/// невалидная α/тинт резолвятся в Unreachable, не в тихий кламп.
#[test]
fn rgba_resolve_rejects_out_of_domain_spec() {
    use crate::semantic::{NamedRoleTable, Resolved, RoleChroma, RoleSpec, resolve_named_set};
    use crate::solve::BgInput;
    use crate::spaces::vc::ViewingConditions;
    let tint = crate::ladder::LadderTint::new([[0.5, 0.5, 0.5]; 4]).expect("валидный тинт");
    for bad_alpha in [f64::NAN, 0.0, 1.5] {
        let table = NamedRoleTable::new(
            vec![(
                "probe".to_string(),
                RoleSpec::Ladder {
                    tint,
                    alpha_light: bad_alpha,
                    alpha_dark: bad_alpha,
                },
            )],
            vec![],
            RoleChroma::Neutral,
        );
        let set = resolve_named_set(
            &BgInput::solid("#FFFFFF").unwrap(),
            &table,
            &ViewingConditions::srgb(),
        );
        assert!(
            matches!(set[0].1, Resolved::Unreachable(_)),
            "α={bad_alpha} обязана дать Unreachable, не цвет"
        );
    }
    // Мусорный quad отвергается конструктором тинта с именем режима.
    assert_eq!(
        crate::ladder::LadderTint::new([[2.0, 0.5, 0.5]; 4]).unwrap_err(),
        "light"
    );
}
