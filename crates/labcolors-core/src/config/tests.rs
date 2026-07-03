//! Тесты границы конфига:
//! 1. Байт-в-байт: `resolve_named_set(labui_reference)` эмитит идентично
//!    `resolve_set(RoleTable::default)` по всем 240 точкам golden-грида.
//! 2. RED-proof байт-в-байт: мутация одного рецепта фикстуры роняет тест.
//! 3. Валидатор: за-предельное значение КАЖДОЙ ручки даёт `ConfigError` +
//!    RED-proof мутацией предела (валидный vs невалидный на границе).
//! 4. Лестница/альфа: Ladder/AlphaAnalog компилируются в полупрозрачные специи;
//!    diff=пусто против consumedRoles; S_PERC_MIN-идентичность; значенческая
//!    сверка со стабом labui (light+dark) + RED-proof мутаций.

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
        // полупрозрачная роль: тинт + фактическая альфа — то, что эмитится `--lab-*`.
        Resolved::Translucent(r) => format!("rgba({},{})", r.tint_hex(), r.alpha()),
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

    // Фикстура несёт 20 core-ролей ПЛЮС акцентную/сентимент/FX/альфа
    // лестницу. Байт-в-байт гарантия — на СОЛВЕР-ролях (имена = Role::key()):
    // именно их пинит golden-грид. Нейтральные заливки/границы НАМЕРЕННО
    // расходятся с дефолт-таблицей: они эмитятся лестницей rgba(mid, α) —
    // полупрозрачность обязана ложиться на любую поверхность, солвер-солид
    // её терял; их значенческая истина — сверка со стабом
    // (representative_roles_match_stub_values_light_and_dark).
    // Плюс роли, покинувшие паспорт по закону семантики: separator — токена
    // нет (бордер и сепаратор едины, компонент применяет бордер), shadow-* —
    // полупрозрачных лестница под стаб-именами fx-shadow-* (солид над контентом был бы
    // грязью), border-strong — пол различимости AaUi (зеркалится: дефолт-таблица
    // несёт тот же пол).
    const LADDER_MIGRATED: [&str; 11] = [
        "fill-primary",
        "fill-secondary",
        "fill-tertiary",
        "fill-quaternary",
        "border-base",
        "border-soft",
        "separator",
        "shadow-minor",
        "shadow-ambient",
        "shadow-penumbra",
        "shadow-major",
    ];
    let core_keys: Vec<&'static str> = Role::ALL
        .iter()
        .map(|r| r.key())
        .filter(|k| !LADDER_MIGRATED.contains(k))
        .collect();
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
    // 9 солвер-ролей (20 − 6 лестничных − separator − 4 теней, ушедших из
    // паспорта по закону семантики) × 2 VC × 6 фонов = 108.
    assert_eq!(
        compared, 108,
        "должно сравниться ровно 108 солвер-точек (пин не вакуумный)"
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
    let cfg = with_role_recipe("icon", RoleRecipe::DecorativeLc { magnitude: 0.0 });
    assert!(matches!(
        cfg.validate(),
        Err(ConfigError::OutOfBounds { handle, .. }) if handle == "roles.icon.magnitude"
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
    // RED-proof: 0.0 валиден (ничего не исключает — компилируется).
    let mut lo = labui_reference();
    lo.sentiments.categories[1].hue_floor_deg = Some(0.0);
    assert_eq!(lo.validate(), Ok(()));
    // 359.999 проходит проверку ДИАПАЗОНА (не OutOfBounds), но полный
    // preflight честно ловит деривационную коллизию: такой пол исключает
    // почти весь круг (`h < f` нелегален) — легальная дуга сентимента пуста.
    // Ассерт `Ok` здесь был бы ложноположительным preflight-ом (validate =
    // компиляция по построению, деривационные ошибки видит).
    let mut hi = labui_reference();
    hi.sentiments.categories[1].hue_floor_deg = Some(359.999);
    assert!(
        !matches!(hi.validate(), Err(ConfigError::OutOfBounds { .. })),
        "359.999 внутри полуинтервала [0,360) — диапазонная проверка проходит"
    );
    assert!(
        hi.validate().is_err(),
        "пол 359.999 опустошает легальную дугу — деривационная ошибка"
    );
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
// 4. Честные заглушки нереализованных рецептов.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ladder_recipe_compiles_to_translucent_spec() {
    // Ladder — не заглушка: компилируется в RoleSpec::Ladder.
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
fn alpha_analog_recipe_compiles_to_translucent_spec() {
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
// diff=пусто против consumedRoles labui.
// ─────────────────────────────────────────────────────────────────────────────

/// Полный контракт `--lab-*` labui из `packages/colors-stub/roles.json`
/// (снят 2026-07-02, источник в шапке файла: генерируется из
/// `reference/labui-tokens-snapshot.dtcg.json`). Захардкожен здесь как SSOT для
/// diff-теста — при регенерации roles.json обновить этот список синхронно.
///
/// Имена без префикса `--lab-`. IC-режимы зарезервированы (в roles.json не
/// перечислены), поэтому и здесь их нет.
///
/// Компромисс: это ЗЕРКАЛО roles.json, не живой файл. Класс дрейфа зеркала
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
    // FX shadow — полупрозрачных лестница тёмного якоря под стаб-именами.
    "fx-shadow-minor",
    "fx-shadow-ambient",
    "fx-shadow-penumbra",
    "fx-shadow-major",
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
    // Пары бейджа: заливка законом пары, лейбл — nested resolve потребителя.
    "badge-fill-brand",
    "badge-fill-danger",
    "badge-fill-warning",
    "badge-fill-success",
    "badge-fill-info",
    "badge-fill-static-dark",
    "badge-fill-static-light",
    // Прочие эмитируемые нейтральные (icon/none — core; separator НЕ токен:
    // бордер и сепаратор едины, компонент применяет бордер-токен).
    "icon",
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
    // СУЖЕНО (ADR-0002 labui §1, 2026-07-03): базовый фон остаётся ВХОДОМ
    // (bg-primary/secondary/... — маппинг потребителя на тона), но выведенные
    // ТОНА лестницы фонов (bg-tone-*) — легитимные dJ'-эмиссии солвера:
    // «еле отличимо»-ступени — контракт движка, не рукописные hex потребителя.
    (
        "bg-primary*",
        "набор фонов = конфиг потребителя, не роль эмиссии",
    ),
    (
        "bg-secondary*",
        "набор фонов = конфиг потребителя, не роль эмиссии",
    ),
    (
        "bg-tertiary*",
        "набор фонов = конфиг потребителя, не роль эмиссии",
    ),
    (
        "bg-grouped-*",
        "набор фонов = конфиг потребителя, не роль эмиссии",
    ),
    (
        "bg-overlay-*",
        "оверлеи → alpha.rs-роли (вне поглощаемого GAP)",
    ),
    // Компонентные алиасы — конфиг-алиасы, не рецепты. Бейдж сузился законом
    // пары: badge-fill-* стали первоклассной эмиссией (RoleRecipe::PairFill,
    // crate::pair), коллапс остаётся только за лейблами бейджа — те решаются
    // nested resolve потребителя от выведенной заливки.
    (
        "badge-label-*",
        "лейбл бейджа — nested resolve от заливки пары",
    ),
    ("control-bg", "компонентный алиас, не рецепт эмиссии"),
];

/// diff = ПУСТО: каждая consumedRole labui (минус удаляемые по коллапсу)
/// эмитируется фикстурой. Несущий тест поглощения акцентного GAP #59.
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
    // не исполнен). Предикат ВЫВОДИТСЯ из деклараций COLLAPSED_ROLES — второй,
    // вручную синхронизируемый список условий гнил бы молча (новый паттерн в
    // декларации без правки предиката = тест перестаёт кусаться).
    for (name, _) in table.entries() {
        if let Some((pattern, why)) = COLLAPSED_ROLES
            .iter()
            .find(|(p, _)| matches_collapsed_pattern(name, p))
        {
            panic!(
                "фикстура эмитит коллапс-роль `{name}` (паттерн `{pattern}`: {why}) — \
                 коллапс контракта нарушен"
            );
        }
    }
    // Значенческий гард сопоставителя (RED-proof против немого предиката):
    // коллапс-имена ловятся, легитимная FX-роль `fx-glow-inverted` — нет
    // (она НЕ инвертированный лейбл/бордер).
    let hits = |name: &str| {
        COLLAPSED_ROLES
            .iter()
            .any(|(p, _)| matches_collapsed_pattern(name, p))
    };
    assert!(hits("label-on-accent") && hits("bg-material-thick") && hits("tint-static-dark-4"));
    assert!(!hits("fx-glow-inverted") && !hits("label-danger-primary"));
    // COLLAPSED_ROLES не пуст — декларация причин присутствует.
    assert!(!COLLAPSED_ROLES.is_empty());
}

/// Glob-сопоставление паттернов [`COLLAPSED_ROLES`] (`*` — любая подстрока):
/// сегменты между `*` обязаны входить по порядку; без ведущей/замыкающей `*`
/// первый/последний сегмент заякорен на начало/конец имени.
fn matches_collapsed_pattern(name: &str, pattern: &str) -> bool {
    let segments: Vec<&str> = pattern.split('*').collect();
    let mut pos = 0usize;
    for (i, seg) in segments.iter().enumerate() {
        if seg.is_empty() {
            continue;
        }
        let Some(found) = name[pos..].find(seg) else {
            return false;
        };
        if i == 0 && found != 0 {
            return false; // без ведущей `*` — якорь на начало
        }
        pos += found + seg.len();
    }
    // Без замыкающей `*` — якорь на конец имени.
    pattern.ends_with('*') || pos == name.len()
}

// ─────────────────────────────────────────────────────────────────────────────
// S_PERC_MIN — деривационная идентичность из конфиг-якорей.
// ─────────────────────────────────────────────────────────────────────────────

/// `S_PERC_MIN`, пересчитанный из хром 4 сентимент-якорей labui, совпадает с
/// замороженной константой (`0.068_703_9`, допуск 1e-4) — закон
/// `2·C_rep·sin(20°/2)` остаётся законом, сегодняшнее значение — его частный
/// случай при labui-якорях.
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
// Сентимент — деривационная идентичность (тинт == сырой якорь при
// labui-бренде).
// ─────────────────────────────────────────────────────────────────────────────

/// Деривационная идентичность: при бренде labui сентимент-тинт
/// совпадает с СЫРЫМ якорем семейства (по всем 4 темам) для сентиментов,
/// ОТСТОЯЩИХ от бренда дальше перцептивного порога `s_min`.
///
/// ЧЕСТНАЯ НАХОДКА (не подгонка): для Danger/Success/Warning идентичность
/// держится (их семейства далеки от синего бренда labui). Для **Info** она НЕ
/// держится: Info→Blue (Oklab h≈259.9°) отстоит от бренда `#007AFF` (h≈257.4°)
/// лишь на ≈2.5° — НИЖЕ порога разделения (`S_PERC_MIN`≈0.0687 хорды ≈ 20.5° при
/// хроме blue ≈0.19: 2·asin(0.0687/0.38); прежняя оценка «3.5°» занижала
/// фактическое смещение Info (≈18–20°, до ≈277.9° в фиолетово-синий) почти
/// на порядок). Сентимент-солвер КОРРЕКТНО смещает Info, чтобы он был отличим от
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
// полупрозрачная эмиссия + RED-proof мутаций (позиция/семейство/альфа → RED).
// ─────────────────────────────────────────────────────────────────────────────

/// Резолв Ladder-роли несёт rgba(тинт, α) + солид-композит на фоне резолва.
/// Тинт brand-роли по светлой теме == светлый якорь бренда (эмитится напрямую);
/// композит — то, что реально показывается на белом фоне.
#[test]
fn ladder_emits_translucent_with_composite_over_bg() {
    let table = labui_reference().compile_named_role_table().unwrap();
    let bg = BgInput::solid("#FFFFFF").unwrap();
    let vc = ViewingConditions::srgb();
    let set = resolve_named_set(&bg, &table, &vc);

    let (_, res) = set
        .iter()
        .find(|(n, _)| n == "fill-brand-secondary")
        .unwrap();
    let Resolved::Translucent(r) = res else {
        panic!("fill-brand-secondary: ожидался Translucent, получено {res:?}");
    };
    // Тинт brand light = #007AFF (эмитится напрямую).
    assert_eq!(r.tint_hex(), "#007AFF", "тинт brand-роли (светлая тема)");
    // Альфа позиции fill-secondary = @8.
    assert!((r.alpha() - 0.078).abs() < 1e-12, "альфа fill-secondary @8");
    // Композит #007AFF@0.078 над #FFFFFF — то, что реально красится.
    let want_composite = crate::alpha::composite_hex("#007AFF", 0.078, "#FFFFFF").unwrap();
    assert_eq!(r.composite_hex(), want_composite, "композит на белом фоне");
    // Контраст меряется на КОМПОЗИТЕ, не на тинте: у прозрачной заливки @8 над
    // белым композит почти белый — WCAG близок к 1 и заведомо МЕНЬШЕ контраста
    // солидного тинта (#007AFF на белом ≈ 4.0) — нетавтологичная проверка того,
    // что замер идёт по правильному цвету.
    let solid_wcag = crate::wcag::contrast_ratio(
        crate::spaces::srgb::srgb_encoded_from_hex("#007AFF").expect("валидный hex"),
        crate::spaces::srgb::srgb_encoded_from_hex("#FFFFFF").expect("валидный hex"),
    );
    assert!(
        r.composite_wcag() < 1.2 && r.composite_wcag() < solid_wcag / 2.0,
        "WCAG обязан меряться по композиту (почти белому), не по тинту: composite={}, solid={}",
        r.composite_wcag(),
        solid_wcag
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
    let Resolved::Translucent(r) = res else {
        panic!("ожидался Translucent")
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
        res.translucent().unwrap().tint_hex().to_string()
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
        res.translucent().unwrap().tint_hex().to_string()
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
        let r = res.translucent().unwrap();
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
// Класс «имена без значений»: значенческий тест фикстуры против стаба.
//
// Класс дефекта: роль присутствует в diff-тесте по ИМЕНИ, но эмитит НЕ ТО
// значение (напр. нейтральный skeleton, ошибочно взятый из семейства blue).
// Здесь эмиссия rgba(тинт, α) представителя каждой группы сверяется со строкой
// стаба contract.css ПОБАЙТНО (нормализованный формат), в light И dark.
// ─────────────────────────────────────────────────────────────────────────────

/// Нормализовать [`Resolved::Translucent`] в пару (rgb-строка, α): rgb сверяется со
/// стабом ПОБАЙТОВО, α — числом с допуском (Display-сравнение f64 хрупко:
/// хвост вида 0.07800000000000001 после будущего рефакторинга формулы уронил
/// бы тест по ФОРМАТУ, маскируя семантику).
fn translucent_to_parts(res: &Resolved) -> (String, f64) {
    let r = res
        .translucent()
        .unwrap_or_else(|| panic!("ожидался Resolved::Translucent, получено {res:?}"));
    let rgb = crate::spaces::srgb::srgb_encoded_from_hex(r.tint_hex()).unwrap();
    let ch = |v: f64| (v * 255.0).round() as u8;
    (
        format!("rgb({} {} {})", ch(rgb[0]), ch(rgb[1]), ch(rgb[2])),
        r.alpha(),
    )
}

/// Разбить стаб-литерал `rgb(R G B / A)` / `rgb(R G B)` на (rgb-строка, α):
/// солид без слэша несёт α = 1.
fn split_stub_rgba(stub: &str) -> (String, f64) {
    match stub.split_once(" / ") {
        Some((rgb, a)) => (
            format!("{rgb})"),
            a.trim_end_matches(')').parse().expect("α стаба — число"),
        ),
        None => (stub.to_string(), 1.0),
    }
}

/// Сверить эмиссию роли со стаб-литералом: rgb побайтово, α с допуском 1e-12
/// (тот же допуск, что у соседних численных сверок α).
#[track_caller]
fn assert_matches_stub(role: &str, theme: &str, got: &Resolved, want: &str) {
    let (got_rgb, got_alpha) = translucent_to_parts(got);
    let (want_rgb, want_alpha) = split_stub_rgba(want);
    assert_eq!(
        got_rgb, want_rgb,
        "ЗНАЧЕНИЕ РАЗОШЛОСЬ ({theme}) `{role}`: rgb {got_rgb} != стаб {want_rgb}"
    );
    assert!(
        (got_alpha - want_alpha).abs() < 1e-12,
        "ЗНАЧЕНИЕ РАЗОШЛОСЬ ({theme}) `{role}`: α {got_alpha} != стаб {want_alpha}"
    );
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
        // Края нейтрали пер-темные: контур (edge) и инверт — из стаба дословно.
        ("fx-focus-ring-neutral", "rgb(16 16 18)", "rgb(246 248 250)"),
        (
            "fx-glow-inverted",
            "rgb(176 176 185 / 0.522)",
            "rgb(60 60 67 / 0.522)",
        ),
        // Мигрированные на лестницу нейтральные заливки/границы: rgba(mid, α)
        // с пер-темными парами — дословно значения стаба (истина миграции).
        (
            "fill-primary",
            "rgb(120 120 128 / 0.2)",
            "rgb(120 120 128 / 0.361)",
        ),
        (
            "fill-secondary",
            "rgb(120 120 128 / 0.161)",
            "rgb(120 120 128 / 0.322)",
        ),
        (
            "fill-tertiary",
            "rgb(120 120 128 / 0.122)",
            "rgb(120 120 128 / 0.239)",
        ),
        (
            "fill-quaternary",
            "rgb(120 120 128 / 0.078)",
            "rgb(120 120 128 / 0.161)",
        ),
        (
            "border-base",
            "rgb(120 120 128 / 0.161)",
            "rgb(120 120 128 / 0.2)",
        ),
        (
            "border-soft",
            "rgb(120 120 128 / 0.078)",
            "rgb(120 120 128 / 0.122)",
        ),
        // Нейтральные: skeleton highlight #787880 @4; base — алиас
        // fill-quaternary (наследование слабых заливок: четверичная заливка =
        // disabled-уровень, скелетон = будущая форма), эмиссию алиаса
        // проверяет граница. glow-neutral белый @52.
        (
            "fx-skeleton-highlight",
            "rgb(120 120 128 / 0.039)",
            "rgb(120 120 128 / 0.039)",
        ),
        // Тени: тёмный якорь нейтрали (#101012) в ОБЕИХ темах, полупрозрачность by design —
        // солид над картинкой/стеклом закрывал бы контент пятном.
        (
            "fx-shadow-minor",
            "rgb(16 16 18 / 0.012)",
            "rgb(16 16 18 / 0.02)",
        ),
        (
            "fx-shadow-ambient",
            "rgb(16 16 18 / 0.02)",
            "rgb(16 16 18 / 0.039)",
        ),
        (
            "fx-shadow-penumbra",
            "rgb(16 16 18 / 0.039)",
            "rgb(16 16 18 / 0.122)",
        ),
        (
            "fx-shadow-major",
            "rgb(16 16 18 / 0.122)",
            "rgb(16 16 18 / 0.2)",
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
        let got_l = &set_l.iter().find(|(n, _)| n == role).unwrap().1;
        let got_d = &set_d.iter().find(|(n, _)| n == role).unwrap().1;
        assert_matches_stub(role, "light", got_l, want_light);
        assert_matches_stub(role, "dark", got_d, want_dark);
    }
}

/// RED-proof значенческого теста: мутация ОДНОЙ альфы (skeleton-highlight
/// @4 → NeutralFillPrimary @36 на тёмной) роняет сверку — тест кусается,
/// не green-from-birth.
#[test]
fn value_test_bites_on_alpha_mutation() {
    let mut cfg = labui_reference();
    for (name, recipe) in &mut cfg.roles {
        if name == "fx-skeleton-highlight" {
            *recipe = RoleRecipe::Ladder {
                source: LadderSource::Neutral(crate::config::NeutralPick::Mid),
                position: LadderPosition::NeutralFillPrimary,
            };
        }
    }
    let table = cfg.compile_named_role_table().unwrap();
    let bg_dark = BgInput::solid("#101012").unwrap();
    let set = resolve_named_set(&bg_dark, &table, &ViewingConditions::dim_surround());
    let (got_rgb, got_alpha) = translucent_to_parts(
        &set.iter()
            .find(|(n, _)| n == "fx-skeleton-highlight")
            .unwrap()
            .1,
    );
    assert_eq!(got_rgb, "rgb(120 120 128)", "мутация двигает ТОЛЬКО альфу");
    assert!(
        (got_alpha - 0.039).abs() > 1e-9,
        "RED-proof значенческого теста провален: мутация альфы НЕ сдвинула эмиссию"
    );
}

/// Сторона пары — идентичность семьи НА РЕЗОЛВ-УРОВНЕ. Носитель класса —
/// БРЕНД под dark-IC: источник Brand несёт сырые якоря, и его dark-ic
/// (#409CFF, Y = 0.321) пересекает кроссовер 0.30. Сентименты (включая
/// info) разведены солвером и порог не straddle-ят — на них мутация
/// «сторона от vc» поведенчески неразличима (выживший мутант M3
/// верификатора). Мутация semantic.rs srgb→vc обязана уронить ЭТОТ тест.
#[test]
fn pair_side_is_family_stable_across_themes_at_resolve_level() {
    let table = labui_reference().compile_named_role_table().unwrap();
    let bg_dark = BgInput::solid("#101012").unwrap();
    let set = resolve_named_set(
        &bg_dark,
        &table,
        &ViewingConditions::dim_surround_high_contrast(),
    );
    let (_, res) = set
        .iter()
        .find(|(n, _)| n == "badge-fill-brand")
        .expect("паспорт несёт badge-fill-brand");
    let fill = res
        .translucent()
        .expect("заливка пары эмитится лестничной сантехникой");
    // Светлая сторона семьи: тёмная заливка (белый строго выигрывает
    // штатную полярность — Y ниже выведенной границы WCAG).
    let enc =
        crate::spaces::srgb::srgb_encoded_from_hex(fill.tint_hex()).expect("эмиссия валидный hex");
    let lin = [
        crate::spaces::srgb::srgb_gamma_inv(enc[0]),
        crate::spaces::srgb::srgb_gamma_inv(enc[1]),
        crate::spaces::srgb::srgb_gamma_inv(enc[2]),
    ];
    let y = 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2];
    assert!(
        y < 0.17913,
        "badge-fill-brand в dark-IC обязан быть утемнён под светлую сторону семьи (Y={y:.4})"
    );
}

/// Дубликаты ключей всех словарей отвергаются (повтор имени = неоднозначный
/// lookup), включая алиас, затеняющий роль.
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
        // Зонд-рецепт: dj-якорей в фикстуре больше нет (нейтральные
        // заливки/границы уехали на лестницу), а предел ручки — свойство МЕНЮ.
        let mut c = labui_reference();
        c.roles.push((
            "probe-dj".to_string(),
            RoleRecipe::DjAnchor {
                light: bad,
                dark: 5.0,
            },
        ));
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
/// контракта терялись бы при эмиссии.
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
fn translucent_resolve_rejects_out_of_domain_spec() {
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

/// Границы α AlphaAnalog: ровно 1.0 валидна, 1.0+ε — нет (RED-proof грани).
#[test]
fn alpha_analog_boundary_is_exact() {
    let mut c = labui_reference();
    c.roles.push((
        "probe-alpha-boundary".to_string(),
        RoleRecipe::AlphaAnalog {
            of: LadderSource::Brand,
            alpha: 1.0,
        },
    ));
    assert!(c.validate().is_ok(), "α=1.0 легальна");
    if let Some((_, RoleRecipe::AlphaAnalog { alpha, .. })) = c
        .roles
        .iter_mut()
        .find(|(n, _)| n == "probe-alpha-boundary")
    {
        *alpha = 1.0 + 1e-9;
    }
    assert!(
        matches!(c.validate(), Err(ConfigError::OutOfBounds { .. })),
        "α чуть выше 1 обязана быть отвергнута"
    );
}

/// Edge/Inverted без соответствующего поля конфига — честная ошибка, не выдумка.
#[test]
fn missing_neutral_quads_are_rejected() {
    let mut c = labui_reference();
    c.neutral.edge = None;
    assert!(matches!(
        c.compile_named_role_table(),
        Err(ConfigError::MissingNeutralAnchors {
            field: "neutral.edge",
            ..
        })
    ));
    let mut c = labui_reference();
    c.neutral.inverted = None;
    assert!(matches!(
        c.compile_named_role_table(),
        Err(ConfigError::MissingNeutralAnchors {
            field: "neutral.inverted",
            ..
        })
    ));
}

/// `validate()` — полный preflight ПО ПОСТРОЕНИЮ (компиляция с отброшенным
/// результатом): для любого конфига validate и compile дают одинаковый исход
/// и байт-в-байт одинаковую ошибку. Корпус — деривационные ошибки, которые
/// структурная фаза не видит (иначе `Ok` preflight-а был бы ложноположительным).
#[test]
fn validate_is_a_complete_preflight() {
    let ok = labui_reference();
    assert!(
        ok.validate().is_ok(),
        "канонический конфиг проходит preflight"
    );
    assert!(ok.compile_named_role_table().is_ok());

    let assert_parity = |c: &ThemeConfig, want: &str| {
        let v = c.validate().expect_err("validate обязан падать");
        let k = c
            .compile_named_role_table()
            .expect_err("compile обязан падать");
        assert_eq!(
            format!("{v:?}"),
            format!("{k:?}"),
            "validate и compile разошлись — полнота preflight нарушена"
        );
        let got = format!("{v:?}");
        assert!(got.contains(want), "ждали {want}, получено {got}");
    };

    // Ахроматичная нейтраль без override — деривационная ошибка подтона.
    let mut c = labui_reference();
    c.neutral.tint.hue_override_deg = None;
    c.neutral.anchors.dark = "#101010".to_string();
    assert_parity(&c, "AchromaticHueSource");

    // Edge-роль без четвёрки edge — деривационная ошибка края нейтрали.
    let mut c = labui_reference();
    c.neutral.edge = None;
    assert_parity(&c, "MissingNeutralAnchors");

    // Битый hex в ЗАДАННОЙ, но никем не используемой четвёрке edge:
    // задекларированные данные валидируются даже без ссылающихся ролей —
    // мёртвый битый hex не должен ждать первую ссылку, чтобы всплыть.
    let mut c = labui_reference();
    c.roles.retain(|(_, r)| {
        !matches!(
            r,
            RoleRecipe::Ladder {
                source: LadderSource::Neutral(NeutralPick::Edge),
                ..
            } | RoleRecipe::AlphaAnalog {
                of: LadderSource::Neutral(NeutralPick::Edge),
                ..
            }
        )
    });
    let kept: std::collections::BTreeSet<&str> = c.roles.iter().map(|(n, _)| n.as_str()).collect();
    c.aliases
        .retain(|(_, target)| kept.contains(target.as_str()));
    c.neutral.edge = Some(crate::ladder::ThemeAnchors {
        light: "не-hex".to_string(),
        dark: "#F6F8FA".to_string(),
        light_ic: "#101012".to_string(),
        dark_ic: "#F6F8FA".to_string(),
    });
    assert_parity(&c, "InvalidHex");
}

/// `RoleSpec` публичен: alpha-analog-спека с недоменной α, собранная в обход
/// валидатора конфига, резолвится в честный `Unreachable`, а не в
/// правдоподобный hex через кламп резолвера инверсии. Недоменный СОЛИД по
/// построению невозможен ([`crate::ladder::LadderTint::new`] валидирует домен
/// квада) — гард по солиду остаётся глубинной защитой.
#[test]
fn alpha_analog_spec_bypassing_validator_is_rejected() {
    use crate::ladder::LadderTint;
    use crate::semantic::{NamedRoleTable, RoleChroma, RoleSpec};

    let tint = LadderTint::new([[0.5, 0.5, 0.5]; 4]).expect("валидный квад");
    let bg = BgInput::solid("#FFFFFF").unwrap();
    for alpha in [1.0 + 1e-9, 0.0, -0.5, f64::NAN, f64::INFINITY] {
        let table = NamedRoleTable::new(
            vec![(
                "probe".to_string(),
                RoleSpec::AlphaAnalog { of: tint, alpha },
            )],
            vec![],
            RoleChroma::Neutral,
        );
        let set = crate::semantic::resolve_named_set(&bg, &table, &ViewingConditions::srgb());
        let (_, r) = set.iter().find(|(n, _)| n == "probe").expect("роль есть");
        assert!(
            matches!(r, Resolved::Unreachable(_)),
            "α={alpha}: ждали Unreachable (честный отказ), получено {r:?}"
        );
    }
}

/// Ошибка сентимент-солвера наружу — СВОИМ вариантом, не [`ConfigError::InvalidHex`]:
/// потребитель матчится по вариантам, и ошибка политики/геометрии под маской
/// ошибки парсинга hex ломала бы это различение. Пустая легальная дуга
/// (пол 359.999 у категории) — ровно такой случай.
#[test]
fn sentiment_solver_errors_surface_as_their_own_variant() {
    let mut c = labui_reference();
    c.sentiments.categories[1].hue_floor_deg = Some(359.999);
    match c.compile_named_role_table() {
        Err(ConfigError::SentimentResolution { sentiment, .. }) => {
            assert_eq!(sentiment, c.sentiments.categories[1].name);
        }
        other => panic!("ждали SentimentResolution, получено {other:?}"),
    }
}

/// Ахроматичные источники оттенка: серая нейтраль без override — ошибка;
/// серый бренд — сентимент честно равен сырому якорю (разведение отключено).
#[test]
fn achromatic_hue_sources_are_handled_honestly() {
    let mut c = labui_reference();
    c.neutral.tint.hue_override_deg = None;
    c.neutral.anchors.dark = "#101010".to_string(); // чистый серый: хрома ≈ 0
    assert!(matches!(
        c.compile_named_role_table(),
        Err(ConfigError::AchromaticHueSource { .. })
    ));

    let mut c = labui_reference();
    // Серый бренд: все четыре режима ахроматичны.
    c.brand.anchors = crate::ladder::ThemeAnchors {
        light: "#808080".to_string(),
        dark: "#808080".to_string(),
        light_ic: "#808080".to_string(),
        dark_ic: "#808080".to_string(),
    };
    let table = c.compile_named_role_table().expect("серый бренд легален");
    let set = crate::semantic::resolve_named_set(
        &BgInput::solid("#FFFFFF").unwrap(),
        &table,
        &crate::spaces::vc::ViewingConditions::srgb(),
    );
    let (_, r) = set
        .iter()
        .find(|(n, _)| n == "label-danger-primary")
        .expect("роль есть");
    let Resolved::Translucent(r) = r else {
        panic!("ожидался Translucent");
    };
    assert_eq!(
        r.tint_hex(),
        "#FF3B30",
        "при сером бренде сентимент = сырой якорь семейства (разведение отключено)"
    );
}

/// Волна 2 ADR-0002 labui §5 — КОМПОЗИЦИОННЫЙ контракт FX-стека теней.
///
/// Прежний контракт держал только пер-токенный порядок (|Lc| каждой ступени
/// сама по себе). Закон владельца сильнее: токены НАСЛАИВАЮТСЯ (minor под
/// ambient под penumbra под major), и прогрессивным обязан быть СУММАРНЫЙ
/// эффект composited-стека. Здесь стек компонуется честной альфа-композицией
/// (`alpha::composite_over_encoded`, тот же оператор, что у браузера) слой за
/// слоем над светлым фоном паспорта, и проверяется:
///   (1) каждый слой меняет пиксели: state_k ≠ state_{k-1} на 8-битной сетке
///       (класс `composite_distinct`, ADR-0002 lab-colors);
///   (2) различимость стека от фона строго растёт: |ΔJ'|(state_k, bg)
///       возрастает по k — прогрессия именно КОМПОЗИЦИИ, не отдельных ступеней.
///
/// Тёмная тема намеренно не в этом тесте: elevation тёмной темы — тональная
/// лестница фонов (bg-tone-*, dj-anchor контракты этого же поезда), тень на
/// тёмном вырождается физически (тинт ≈ фон — класс, признанный ladder.rs);
/// glow-стека не существует (fx-glow-* — одиночные позиции @52).
#[test]
fn fx_shadow_stack_composition_is_strictly_progressive_on_light() {
    use crate::alpha::composite_over_encoded;
    use crate::lcs::LcsColor;
    use crate::spaces::srgb::{hex_from_srgb_encoded, srgb_encoded_from_hex};

    let cfg = labui_reference();
    let table = cfg
        .compile_named_role_table()
        .expect("фикстура labui компилируется");
    let vc = ViewingConditions::srgb();
    let bg_hex = "#FFFFFF"; // светлый якорь паспорта — фон резолва светлой темы
    let bg = BgInput::solid(bg_hex).unwrap();
    let set = resolve_named_set(&bg, &table, &vc);

    let stack = [
        "fx-shadow-minor",
        "fx-shadow-ambient",
        "fx-shadow-penumbra",
        "fx-shadow-major",
    ];
    let bg_jp = LcsColor::from_hex_with_vc(bg_hex, &vc).unwrap().jp;
    let mut state = srgb_encoded_from_hex(bg_hex).unwrap();
    let mut prev_delta = 0.0_f64;
    for name in stack {
        let (_, resolved) = set
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("{name} отсутствует в наборе"));
        let t = resolved
            .translucent()
            .unwrap_or_else(|| panic!("{name} должен быть Translucent"));
        let tint = srgb_encoded_from_hex(t.tint_hex()).unwrap();
        let prev_hex = hex_from_srgb_encoded(state);
        state = composite_over_encoded(tint, t.alpha(), state);
        let state_hex = hex_from_srgb_encoded(state);
        // (1) слой меняет пиксели поверх уже наслоённого стека.
        assert_ne!(
            state_hex, prev_hex,
            "{name}: наслоение слоя не изменило композит ({state_hex}) — вырожденная ступень стека"
        );
        // (2) суммарная различимость стека от фона строго растёт.
        let jp = LcsColor::from_hex_with_vc(&state_hex, &vc).unwrap().jp;
        let delta = (jp - bg_jp).abs();
        assert!(
            delta > prev_delta,
            "{name}: композиция стека не прогрессивна: |ΔJ'| {delta:.4} ≤ пред. {prev_delta:.4}"
        );
        prev_delta = delta;
    }
}
