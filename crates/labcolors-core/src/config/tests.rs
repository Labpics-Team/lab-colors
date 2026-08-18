//! Тесты границы конфига:
//! 1. Байт-в-байт: `resolve_named_set(labui_reference)` эмитит идентично
//!    `resolve_set(RoleTable::default)` по всем 240 точкам golden-грида.
//! 2. RED-proof байт-в-байт: мутация одного рецепта фикстуры роняет тест.
//! 3. Валидатор: за-предельное значение КАЖДОЙ ручки даёт `ConfigError` +
//!    RED-proof мутацией предела (валидный vs невалидный на границе).
//! 4. Лестница/альфа: Ladder/AlphaAnalog компилируются в полупрозрачные специи;
//!    семейные источники точно сохраняют клиентские якоря во всех контекстах;
//!    значенческая сверка со стабом labui держит представителей остальных групп.

use super::fixture::labui_reference;
use super::test_support::resolved_repr as repr;
use super::*;
use crate::ladder::LadderPosition;
use crate::semantic::Floor;
use crate::semantic::{Resolved, resolve_named_set};
use crate::{BgInput, Role, RoleTable, ViewingConditions, resolve_set};

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

    // Фикстура несёт 20 core-ролей ПЛЮС семейные/FX/альфа
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
            let named = resolve_named_set(&bg, &table, &vc)
                .expect("валидная labui-фикстура обязана резолвиться");
            let default_map = default_by_key(&bg, &vc);

            // Сравниваем ТОЛЬКО 19 сегодняшних ролей (акцентные — новые, у них нет
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
    // 8 солвер-ролей (19 − 6 лестничных − separator − 4 теней, ушедших из
    // паспорта по закону семантики; словарный канон #92 снёс роль icon) ×
    // 2 VC × 6 фонов = 96.
    assert_eq!(
        compared, 96,
        "должно сравниться ровно 96 солвер-точек (пин не вакуумный)"
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
                hue: None,
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
            let named = resolve_named_set(&bg, &mutated, &vc)
                .expect("валидный recipe-мутант обязан резолвиться");
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
            hue: None,
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
            hue: None,
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
            hue: None,
        },
    );
    assert_eq!(at.validate(), Ok(()), "fraction=1.0 должен быть валиден");
    let over = with_role_recipe(
        "label-primary",
        RoleRecipe::TextAnchor {
            fraction: 1.0 + 1e-9,
            floor: Floor::AaText,
            hue: None,
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
fn decorative_lc_requires_the_core_physical_floor() {
    // Проверяем точную закрытую границу: ближайший меньший binary64 уже
    // недоменен, сам физический пол принимается без переписи.
    let below = f64::from_bits(DECORATIVE_FLOOR_MIN.to_bits() - 1);
    for magnitude in [f64::NAN, f64::INFINITY, 0.0, below] {
        let cfg = with_role_recipe("label-tertiary", RoleRecipe::DecorativeLc { magnitude });
        assert!(
            matches!(
                cfg.validate(),
                Err(ConfigError::OutOfBounds { handle, .. })
                    if handle == "roles.label-tertiary.magnitude"
            ),
            "magnitude={magnitude} обязана быть отклонена"
        );
    }

    let below_error = with_role_recipe(
        "label-tertiary",
        RoleRecipe::DecorativeLc { magnitude: below },
    )
    .validate()
    .expect_err("значение ниже физического пола обязано быть отклонено");
    let ConfigError::OutOfBounds { bound, .. } = below_error else {
        panic!("ожидалась числовая граница декоративного контраста");
    };
    assert_eq!(
        bound,
        format!("magnitude ≥ {DECORATIVE_FLOOR_MIN} Lc (граница декоративной Lc-цели)")
    );
    assert!(
        !bound.contains("DECORATIVE_FLOOR_MIN"),
        "публичная ошибка не должна показывать внутренний идентификатор: {bound}"
    );

    let boundary = with_role_recipe(
        "label-tertiary",
        RoleRecipe::DecorativeLc {
            magnitude: DECORATIVE_FLOOR_MIN,
        },
    );
    assert_eq!(boundary.validate(), Ok(()));
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
            hue: None,
        },
    ));
    assert!(matches!(
        cfg.validate(),
        Err(ConfigError::InvalidName { .. })
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
            floor: None,
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
fn ladder_floor_is_valid_only_for_a_solid_readability_constraint() {
    for (position, floor) in [
        (LadderPosition::FillPrimary, Some(Floor::AaUi)),
        (LadderPosition::BorderStrong, Some(Floor::None)),
    ] {
        let cfg = with_role_recipe(
            "fill-primary",
            RoleRecipe::Ladder {
                source: LadderSource::Brand,
                position,
                floor,
            },
        );
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::InvalidLadderFloor { role, .. }) if role == "fill-primary"
        ));
    }

    let valid = with_role_recipe(
        "fill-primary",
        RoleRecipe::Ladder {
            source: LadderSource::Brand,
            position: LadderPosition::BorderStrong,
            floor: Some(Floor::AaUi),
        },
    );
    assert_eq!(valid.validate(), Ok(()));
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
            floor: None,
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

/// Замороженный snapshot ожидаемых `--lab-*` имён референс-фикстуры.
/// Это test oracle, не SSOT публичного клиента: production Core не читает его,
/// а актуальность доказывает только diff с текущей fixture-эмиссией.
/// Имена без префикса `--lab-`; IC-режимы не добавляют отдельные имена.
const LABUI_CONSUMED_ROLES: &[&str] = &[
    // Backgrounds — ВХОДЫ (набор фонов = конфиг потребителя), не роли эмиссии.
    // Labels (core neutral).
    "label-primary",
    "label-secondary",
    "label-tertiary",
    "label-quaternary",
    // Labels — бренд и клиентские семейства.
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
    // Fills — бренд и клиентские семейства.
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
    // Border (core neutral). border-ghost — deprecated-алиас канона #92,
    // border-none — честный ноль (оба в контракте roles.json labui).
    "border-strong",
    "border-base",
    "border-soft",
    "border-ghost",
    "border-none",
    // Borders — бренд и клиентские семейства.
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
    "fill-neutral",
    "fill-accent-tinted",
    "fill-neutral-tinted",
    "fill-danger-tinted",
    "label-accent",
    "label-danger",
    "border-accent",
    "border-neutral",
    "border-danger",
    "border-focus",
    // Прочие эмитируемые нейтральные (none — core; icon снят с контракта каноном
    // #92 — глиф красится label-tertiary; separator НЕ токен: бордер и сепаратор
    // едины, компонент применяет бордер-токен).
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
    // Базовый фон остаётся ВХОДОМ
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
    // Компонентная композиция принадлежит клиентскому Program, а не закрытому
    // ролевому меню Core.
    ("badge-*", "client-owned Program composition"),
    ("fill-accent", "client-owned alias"),
    ("fill-danger", "client-owned alias"),
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
// Неприкосновенность клиентских якорей.
// ─────────────────────────────────────────────────────────────────────────────

/// Источник семейства выбирает нужный клиентский якорь для контекста,
/// но никогда не переинтерпретирует и не смещает его физическое значение.
#[test]
fn family_sources_preserve_authored_anchors_in_every_context() {
    let mut cfg = labui_reference();
    for family in &cfg.palette {
        cfg.roles.push((
            format!("probe-family-{}", family.key),
            RoleRecipe::Ladder {
                source: LadderSource::Family(family.key.clone()),
                position: LadderPosition::FillPrimary,
                floor: None,
            },
        ));
    }
    let table = cfg.compile_named_role_table().expect("фикстура валидна");

    for family in &cfg.palette {
        let role = format!("probe-family-{}", family.key);
        let anchors = &family.anchors;
        let (_, spec) = table
            .entries()
            .iter()
            .find(|(name, _)| name == &role)
            .expect("роль есть в фикстуре");
        let RoleSpec::Ladder { tint, .. } = spec else {
            panic!("{role}: ожидался Ladder-спек, получено {spec:?}");
        };
        let modes = [
            ("light", ViewingConditions::srgb(), &anchors.light),
            ("dark", ViewingConditions::dim_surround(), &anchors.dark),
            (
                "light-ic",
                ViewingConditions::srgb_high_contrast(),
                &anchors.light_ic,
            ),
            (
                "dark-ic",
                ViewingConditions::dim_surround_high_contrast(),
                &anchors.dark_ic,
            ),
        ];

        for (mode, vc, authored) in modes {
            let got = crate::spaces::srgb::hex_from_srgb_encoded(tint.for_vc(&vc));
            assert_eq!(
                got, *authored,
                "{role}/{mode}: family source moved authored anchor {authored}"
            );
        }
    }
}

/// A family key is an opaque client ID: consistently renaming the declaration
/// and every reference must compile to the identical physical graph.
#[test]
fn renaming_family_id_and_references_does_not_change_the_compiled_graph() {
    fn rename_source(source: &mut LadderSource, from: &str, to: &str) {
        if let LadderSource::Family(key) = source {
            if key == from {
                *key = to.to_string();
            }
        }
    }

    let original = labui_reference();
    let mut renamed = original.clone();
    renamed
        .palette
        .iter_mut()
        .find(|family| family.key == "red")
        .expect("red fixture family")
        .key = "client-family-42".to_string();

    for (_, recipe) in &mut renamed.roles {
        match recipe {
            RoleRecipe::TextAnchor { hue, .. } => {
                if let Some(source) = hue {
                    rename_source(source, "red", "client-family-42");
                }
            }
            RoleRecipe::Ladder { source, .. }
            | RoleRecipe::Glow { source, .. }
            | RoleRecipe::Material { source, .. } => {
                rename_source(source, "red", "client-family-42");
            }
            RoleRecipe::AlphaAnalog { of, .. } => {
                rename_source(of, "red", "client-family-42");
            }
            RoleRecipe::DjAnchor { .. } | RoleRecipe::DecorativeLc { .. } | RoleRecipe::Zero => {}
        }
    }

    assert_eq!(
        renamed.compile_named_role_table().expect("renamed config"),
        original
            .compile_named_role_table()
            .expect("original config"),
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
    let set =
        resolve_named_set(&bg, &table, &vc).expect("валидная ladder-фикстура обязана резолвиться");

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
    let solid_wcag = crate::spaces::srgb::encoded_srgb_contrast_ratio(
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
                floor: None,
            };
        }
    }
    let table = cfg.compile_named_role_table().unwrap();
    let bg = BgInput::solid("#FFFFFF").unwrap();
    let set = resolve_named_set(&bg, &table, &ViewingConditions::srgb())
        .expect("валидная ladder-фикстура обязана резолвиться");
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
        let set = resolve_named_set(&bg, &base, &vc)
            .expect("валидная базовая ladder-фикстура обязана резолвиться");
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
                floor: None,
            };
        }
    }
    let mutated = cfg.compile_named_role_table().unwrap();
    let mutated_tint = {
        let set = resolve_named_set(&bg, &mutated, &vc)
            .expect("валидный family-мутант обязан резолвиться");
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

/// AlphaAnalog-рецепт: солид-цель фиксирована, тинт выводится
/// композит-инверсией. RED-proof: разные α (обе ≥ α_min) дают разный тинт;
/// exact gate допускает только побайтное равенство финального occurrence цели.
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
        let set =
            resolve_named_set(&bg, &table, &vc).expect("валидный alpha-analog обязан резолвиться");
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
    // Эмиссионный контракт byte-grid точен: фактический occurrence обязан
    // воспроизвести солид-цель побайтно, а не попасть в эвристический LSB-допуск.
    for comp in [&comp_low, &comp_high] {
        assert_eq!(comp, "#787880");
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
/// Исключены намеренно непредставительные `fx-focus-ring-neutral` (dark),
/// `fx-glow-inverted` и `fill-neutral`: их пер-темный нейтральный край, inverted-якоря и
///   задокументированные gap-и (пер-темный нейтральный край / inverted-якоря /
///   PROVISIONAL-литерал не выводятся из тройки neutral.anchors).
#[test]
fn representative_roles_match_stub_values_light_and_dark() {
    let table = labui_reference().compile_named_role_table().unwrap();
    let bg_light = BgInput::solid("#FFFFFF").unwrap();
    let bg_dark = BgInput::solid("#101012").unwrap();

    // (роль, стаб-light, стаб-dark). Значения — из contract.css (2026-07-02).
    let cases: &[(&str, &str, &str)] = &[
        // Ратификация ch5c (M1): `label-<family>-<level>` больше НЕ Ladder@72/52/32
        // (тинт семьи под альфой — 40/40 нарушений одноуровневости), а цветной
        // TextAnchor — держит Lc-контракт уровня в чистом оттенке семьи и
        // резолвится в СОЛИД (`Resolved::Color`), не в Translucent. Его эмиссия и
        // одноуровневость проверяются модулем `src/one_levelness_tests.rs`. Здесь
        // остаётся представитель полупрозрачной СЕМЕЙНОЙ заливки (тот же
        // семейный тинт под альфой рампы — класс «имя есть, значение тинта врёт»).
        (
            "fill-danger-secondary",
            "rgb(255 59 48 / 0.078)",
            "rgb(255 58 58 / 0.078)",
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
        // fx-glow-brand выведен из значенческой сверки со стабом: с 2026-07-03
        // это kind glow (screen-слои + решённая α), а не Ladder@52 — стаб-строка
        // rgba больше не является его контрактом. Новая эмиссия закреплена
        // отдельным тестом `glow_roles_resolve_screen_layers`.
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
        // fx-glow-neutral выведен из сверки со стабом: kind glow с 2026-07-03
        // (см. комментарий у fx-glow-brand выше и тест glow_roles_resolve_screen_layers).
    ];

    for (role, want_light, want_dark) in cases {
        let set_l = resolve_named_set(&bg_light, &table, &ViewingConditions::srgb())
            .expect("валидная светлая fixture обязана резолвиться");
        let set_d = resolve_named_set(&bg_dark, &table, &ViewingConditions::dim_surround())
            .expect("валидная тёмная fixture обязана резолвиться");
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
                floor: None,
            };
        }
    }
    let table = cfg.compile_named_role_table().unwrap();
    let bg_dark = BgInput::solid("#101012").unwrap();
    let set = resolve_named_set(&bg_dark, &table, &ViewingConditions::dim_surround())
        .expect("валидная alpha-mutation fixture обязана резолвиться");
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

    // C5.1: имя темы — ключ клиентского словаря; дубликат делал бы lookup
    // неоднозначным (first-wins тихо хоронит вторую декларацию).
    let mut c = labui_reference();
    c.themes.entries.push(c.themes.entries[0].clone());
    assert!(matches!(
        c.validate(),
        Err(ConfigError::DuplicateKey {
            dictionary: "themes",
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

/// Имена конфига — не весь namespace эмиссии: Glow добавляет `-core/-alpha`,
/// Material — `-01/-02`. Роль или алиас с таким именем раньше проходили
/// preflight, а JSON-проекция молча записывала один `--lab-*` дважды; последний
/// писатель менял тип значения (например, alpha-число превращалось в цвет).
#[test]
fn validator_rejects_role_and_alias_collisions_with_emitted_satellites() {
    let assert_collision = |cfg: ThemeConfig, expected_stem: &str| {
        let expected = format!("--lab-{expected_stem}");
        assert!(
            matches!(
                cfg.validate(),
                Err(ConfigError::DuplicateKey {
                    dictionary: "reserved CSS namespace",
                    key,
                }) if key == expected
            ),
            "коллизия эмитируемого ключа {expected} обязана быть отвергнута"
        );
    };

    // Реальный класс регрессии: существующий glow владеет двумя сателлитами.
    for suffix in ["-core", "-alpha"] {
        let colliding = format!("fx-glow-brand{suffix}");

        let mut role_cfg = labui_reference();
        let ordinary_recipe = role_cfg
            .roles
            .iter()
            .find(|(name, _)| name == "label-primary")
            .expect("фикстура несёт label-primary")
            .1
            .clone();
        role_cfg.roles.push((colliding.clone(), ordinary_recipe));
        assert_collision(role_cfg, &colliding);

        let mut alias_cfg = labui_reference();
        alias_cfg
            .aliases
            .push((colliding.clone(), "label-primary".to_string()));
        assert_collision(alias_cfg, &colliding);
    }

    // Тот же закон обязан закрывать второй многоключевой outcome, а не только
    // конкретный найденный суффикс glow.
    for suffix in ["-01", "-02"] {
        let colliding = format!("probe-material{suffix}");

        let mut role_cfg = labui_reference();
        role_cfg.roles.push((
            "probe-material".to_string(),
            neutral_material(10.0, Floor::AaText),
        ));
        let ordinary_recipe = role_cfg
            .roles
            .iter()
            .find(|(name, _)| name == "label-primary")
            .expect("фикстура несёт label-primary")
            .1
            .clone();
        role_cfg.roles.push((colliding.clone(), ordinary_recipe));
        assert_collision(role_cfg, &colliding);

        let mut alias_cfg = labui_reference();
        alias_cfg.roles.push((
            "probe-material".to_string(),
            neutral_material(10.0, Floor::AaText),
        ));
        alias_cfg
            .aliases
            .push((colliding.clone(), "label-primary".to_string()));
        assert_collision(alias_cfg, &colliding);
    }

    // Алиас многоключевой цели сам становится владельцем полного shape. Это
    // отдельная ветвь: проверка только рецептов ролей пропустила бы её.
    for suffix in ["-core", "-alpha"] {
        let owner = "probe-glow-alias";
        let colliding = format!("{owner}{suffix}");
        let mut cfg = labui_reference();
        cfg.aliases
            .push((owner.to_string(), "fx-glow-brand".to_string()));
        let ordinary_recipe = cfg
            .roles
            .iter()
            .find(|(name, _)| name == "label-primary")
            .expect("фикстура несёт label-primary")
            .1
            .clone();
        cfg.roles.push((colliding.clone(), ordinary_recipe));
        assert_collision(cfg, &colliding);
    }

    for suffix in ["-01", "-02"] {
        let owner = "probe-material-alias";
        let colliding = format!("{owner}{suffix}");
        let mut cfg = labui_reference();
        cfg.roles.push((
            "probe-material".to_string(),
            neutral_material(10.0, Floor::AaText),
        ));
        cfg.aliases
            .push((owner.to_string(), "probe-material".to_string()));
        cfg.aliases
            .push((colliding.clone(), "label-primary".to_string()));
        assert_collision(cfg, &colliding);
    }
}

/// `Zero` не эмитит значение, но его клиентское имя всё равно занято: иначе
/// сателлит другой роли мог бы записать цвет в `cssVar` токена с `kind: "none"`.
/// Закон одинаков для явной zero-роли и алиаса на неё, а также для каждого
/// многоключевого shape, известного core.
#[test]
fn validator_reserves_zero_role_and_alias_primary_names() {
    let assert_collision = |cfg: ThemeConfig, expected_stem: &str| {
        let expected = format!("--lab-{expected_stem}");
        assert!(
            matches!(
                cfg.validate(),
                Err(ConfigError::DuplicateKey {
                    dictionary: "reserved CSS namespace",
                    key,
                }) if key == expected
            ),
            "zero-токен обязан защищать зарезервированный CSS key {expected}"
        );
    };

    let glow_recipe = labui_reference()
        .roles
        .into_iter()
        .find(|(name, _)| name == "fx-glow-brand")
        .expect("фикстура несёт fx-glow-brand")
        .1;

    for (owner, recipe, suffixes) in [
        ("probe-glow", glow_recipe, &["-core", "-alpha"][..]),
        (
            "probe-material",
            neutral_material(10.0, Floor::AaText),
            &["-01", "-02"][..],
        ),
    ] {
        for suffix in suffixes {
            let zero_name = format!("{owner}{suffix}");

            let mut role_cfg = labui_reference();
            role_cfg.roles.push((owner.to_string(), recipe.clone()));
            role_cfg.roles.push((zero_name.clone(), RoleRecipe::Zero));
            assert_collision(role_cfg, &zero_name);

            let mut alias_cfg = labui_reference();
            alias_cfg.roles.push((owner.to_string(), recipe.clone()));
            alias_cfg
                .aliases
                .push((zero_name.clone(), "none".to_string()));
            assert_collision(alias_cfg, &zero_name);
        }
    }
}

/// Гард не должен превращаться в запрет похожих префиксов: резервируются ровно
/// фактически эмитируемые имена, а не все строки, начинающиеся с имени роли.
#[test]
fn emitted_namespace_allows_non_colliding_near_misses() {
    let mut cfg = labui_reference();
    cfg.aliases.push((
        "fx-glow-brand-alpha-extra".to_string(),
        "label-primary".to_string(),
    ));
    cfg.roles.push((
        "probe-material".to_string(),
        neutral_material(10.0, Floor::AaText),
    ));
    cfg.aliases
        .push(("probe-material-03".to_string(), "label-primary".to_string()));

    assert_eq!(cfg.validate(), Ok(()));
}

#[test]
fn compiled_output_binding_set_is_exact_ordered_and_alias_aware() {
    let source = labui_reference();
    let ordinary = source
        .roles
        .iter()
        .find(|(name, _)| name == "label-primary")
        .expect("fixture carries an ordinary role")
        .1
        .clone();
    let glow = source
        .roles
        .iter()
        .find(|(name, _)| name == "fx-glow-brand")
        .expect("fixture carries a Glow role")
        .1
        .clone();

    let mut cfg = source;
    cfg.roles = vec![
        ("plain".to_string(), ordinary),
        ("pulse".to_string(), glow),
        ("glass".to_string(), neutral_material(10.0, Floor::AaText)),
        ("empty".to_string(), RoleRecipe::Zero),
    ];
    cfg.aliases = vec![
        ("pulse-alias".to_string(), "pulse".to_string()),
        ("glass-alias".to_string(), "glass".to_string()),
        ("empty-alias".to_string(), "empty".to_string()),
    ];

    let table = cfg
        .compile_named_role_table()
        .expect("fixture is a valid executable contract");
    assert_eq!(
        table.output_bindings().keys(),
        [
            "--lab-plain",
            "--lab-pulse",
            "--lab-pulse-core",
            "--lab-pulse-alpha",
            "--lab-glass",
            "--lab-glass-01",
            "--lab-glass-02",
            "--lab-empty",
            "--lab-pulse-alias",
            "--lab-pulse-alias-core",
            "--lab-pulse-alias-alpha",
            "--lab-glass-alias",
            "--lab-glass-alias-01",
            "--lab-glass-alias-02",
            "--lab-empty-alias",
        ]
    );
}

#[test]
fn output_binding_compile_errors_preserve_the_config_error_contract() {
    assert_eq!(
        map_output_binding_error(OutputBindingCompileError::InvalidName {
            kind: OutputBindingNameKind::Role,
            value: "bad key".to_string(),
        }),
        ConfigError::InvalidName {
            field: "roles.bad key".to_string(),
            value: "bad key".to_string(),
        }
    );
    assert_eq!(
        map_output_binding_error(OutputBindingCompileError::InvalidName {
            kind: OutputBindingNameKind::Alias,
            value: "bad alias".to_string(),
        }),
        ConfigError::InvalidName {
            field: "aliases.bad alias".to_string(),
            value: "bad alias".to_string(),
        }
    );
    assert_eq!(
        map_output_binding_error(OutputBindingCompileError::UnknownAliasTarget {
            alias: "shortcut".to_string(),
            target: "missing".to_string(),
        }),
        ConfigError::UnknownRole {
            referenced_by: "aliases.shortcut".to_string(),
            role: "missing".to_string(),
        }
    );
    assert_eq!(
        map_output_binding_error(OutputBindingCompileError::DuplicateBinding {
            key: "--lab-pulse-core".to_string(),
        }),
        ConfigError::DuplicateKey {
            dictionary: "reserved CSS namespace",
            key: "--lab-pulse-core".to_string(),
        }
    );
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

/// Ошибки ссылок различимы по виду: роль и семейство — разные варианты.
#[test]
fn validator_reference_errors_are_distinguishable() {
    let mut c = labui_reference();
    c.roles.push((
        "probe-bad-family".to_string(),
        RoleRecipe::Ladder {
            source: LadderSource::Family("nonexistent".to_string()),
            position: LadderPosition::LabelPrimary,
            floor: None,
        },
    ));
    assert!(matches!(
        c.validate(),
        Err(ConfigError::UnknownFamily { .. })
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

/// Прямой NamedRoleTable-конструктор не допускает правдоподобный мусор:
/// невалидная α отвергается до создания executable-таблицы.
#[test]
fn translucent_resolve_rejects_out_of_domain_spec() {
    use crate::semantic::{NamedRoleTable, RoleChroma, RoleSpec};
    let tint = crate::ladder::LadderTint::new([[0.5, 0.5, 0.5]; 4]).expect("валидный тинт");
    for bad_alpha in [f64::NAN, 0.0, 1.5] {
        let result = NamedRoleTable::new(
            vec![(
                "probe".to_string(),
                RoleSpec::Ladder {
                    tint,
                    alpha_light: bad_alpha,
                    alpha_dark: bad_alpha,
                    floor: None,
                },
            )],
            vec![],
            RoleChroma::Neutral,
        );
        assert!(
            matches!(result, Err(crate::solve::SolveFailure::InvalidInput(_))),
            "α={bad_alpha} обязана быть отвергнута до resolve"
        );
    }
    // Мусорный quad отвергается конструктором тинта с именем режима.
    assert_eq!(
        crate::ladder::LadderTint::new([[2.0, 0.5, 0.5]; 4]).unwrap_err(),
        "light"
    );
}

/// The public semantic constructor is an executable boundary of its own: it
/// must reject a role name before it can become an invalid CSS custom property.
#[test]
fn named_role_table_rejects_invalid_role_name_before_materialisation() {
    use crate::semantic::{NamedRoleTable, RoleChroma, RoleSpec};

    for invalid in ["", "bad key", "Upper", "under_score", "роль"] {
        let result = NamedRoleTable::new(
            vec![(invalid.to_string(), RoleSpec::Zero)],
            Vec::new(),
            RoleChroma::Neutral,
        );

        assert!(
            matches!(result, Err(crate::solve::SolveFailure::InvalidInput(_))),
            "invalid role name {invalid:?} must fail before an output manifest exists: {result:?}"
        );
    }
}

/// Aliases reserve public output names too, so the direct constructor applies
/// the same name law to them instead of delegating it to `ThemeConfig`.
#[test]
fn named_role_table_rejects_invalid_alias_name_before_materialisation() {
    use crate::semantic::{NamedRoleTable, RoleChroma, RoleSpec};

    let result = NamedRoleTable::new(
        vec![("valid".to_string(), RoleSpec::Zero)],
        vec![("bad alias".to_string(), "valid".to_string())],
        RoleChroma::Neutral,
    );

    assert!(
        matches!(result, Err(crate::solve::SolveFailure::InvalidInput(_))),
        "invalid alias name must fail before an output manifest exists: {result:?}"
    );
}

#[test]
fn named_role_table_rejects_unknown_alias_target_at_its_own_boundary() {
    use crate::semantic::{NamedRoleTable, RoleChroma, RoleSpec};

    let error = NamedRoleTable::new(
        vec![("valid".to_string(), RoleSpec::Zero)],
        vec![("shortcut".to_string(), "missing".to_string())],
        RoleChroma::Neutral,
    )
    .expect_err("unknown alias target must fail before a table exists");

    assert_eq!(
        error,
        crate::solve::SolveFailure::InvalidInput(
            "alias \"shortcut\" targets unknown executable role \"missing\"".to_string()
        )
    );
}

/// Direct semantic construction must run the same exact namespace collision
/// gate as configuration compilation, including recipe satellites and aliases.
#[test]
fn named_role_table_rejects_role_and_alias_satellite_collisions() {
    use crate::semantic::{NamedRoleTable, RoleChroma, RoleSpec};

    let compiled = labui_reference()
        .compile_named_role_table()
        .expect("reference contract compiles");
    let glow = compiled
        .entries()
        .iter()
        .find_map(|(_, spec)| matches!(spec, RoleSpec::Glow { .. }).then_some(*spec))
        .expect("reference contract carries a Glow recipe");

    for result in [
        NamedRoleTable::new(
            vec![
                ("pulse".to_string(), glow),
                ("pulse-core".to_string(), RoleSpec::Zero),
            ],
            Vec::new(),
            RoleChroma::Neutral,
        ),
        NamedRoleTable::new(
            vec![
                ("pulse".to_string(), glow),
                ("plain".to_string(), RoleSpec::Zero),
            ],
            vec![("pulse-alpha".to_string(), "plain".to_string())],
            RoleChroma::Neutral,
        ),
    ] {
        assert!(
            matches!(result, Err(crate::solve::SolveFailure::InvalidInput(_))),
            "colliding output shapes must fail before a table exists: {result:?}"
        );
    }
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

    // Точный серый без override законен, пока все material-источники ахроматичны;
    // validate и compile обязаны одинаково принять такой контракт.
    let mut c = labui_reference();
    c.neutral.tint.hue_override_deg = None;
    c.neutral.anchors.dark = "#101010".to_string();
    assert!(c.validate().is_ok());
    assert!(c.compile_named_role_table().is_ok());

    // Цветной источник нельзя отложить внутрь neutral-policy: preflight обязан
    // отвергнуть конфликт до первого runtime-resolve.
    for (name, recipe) in &mut c.roles {
        if name == "fill-brand-secondary" {
            *recipe = RoleRecipe::Material {
                source: LadderSource::Brand,
                tone_light: 12.0,
                tone_dark: 12.0,
                floor: Floor::AaUi,
            };
        }
    }
    assert!(matches!(
        c.validate(),
        Err(ConfigError::IncompatibleRolePolicy { ref role, .. })
            if role == "fill-brand-secondary"
    ));
    assert!(matches!(
        c.compile_named_role_table(),
        Err(ConfigError::IncompatibleRolePolicy { ref role, .. })
            if role == "fill-brand-secondary"
    ));
    assert_eq!(
        c.validate().unwrap_err().to_string(),
        format!(
            "material `fill-brand-secondary`: {}",
            RoleSpec::INCOMPATIBLE_CHROMA_REASON
        )
    );

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
/// валидатора конфига, отвергается до создания executable-таблицы. Недоменный СОЛИД по
/// построению невозможен ([`crate::ladder::LadderTint::new`] валидирует домен
/// квада) — гард по солиду остаётся глубинной защитой.
#[test]
fn alpha_analog_spec_bypassing_validator_is_rejected() {
    use crate::ladder::LadderTint;
    use crate::semantic::{NamedRoleTable, RoleChroma, RoleSpec};

    let tint = LadderTint::new([[0.5, 0.5, 0.5]; 4]).expect("валидный квад");
    for alpha in [1.0 + 1e-9, 0.0, -0.5, f64::NAN, f64::INFINITY] {
        let result = NamedRoleTable::new(
            vec![(
                "probe".to_string(),
                RoleSpec::AlphaAnalog { of: tint, alpha },
            )],
            vec![],
            RoleChroma::Neutral,
        );
        assert!(
            matches!(result, Err(crate::solve::SolveFailure::InvalidInput(_))),
            "α={alpha}: ожидался отказ конструктора, получено {result:?}"
        );
    }
}

fn assert_achromatic_hex(hex: &str, context: &str) {
    let [red, green, blue] = crate::srgb8::hex_bytes(hex).expect("canonical emitted hex");
    assert_eq!(
        red, green,
        "{context}: invented red/green direction in {hex}"
    );
    assert_eq!(
        green, blue,
        "{context}: invented green/blue direction in {hex}"
    );
}

fn assert_chromatic_hex(hex: &str, context: &str) {
    let [red, green, blue] = crate::srgb8::hex_bytes(hex).expect("canonical emitted hex");
    assert!(
        red != green || green != blue,
        "{context}: one-byte chromatic direction was discarded in {hex}"
    );
}

/// Exact-gray sources carry neutral identity without an override; the nearest
/// off-axis byte still carries its authored direction.
#[test]
fn achromatic_hue_sources_are_handled_honestly() {
    let mut c = labui_reference();
    c.neutral.tint.hue_override_deg = None;
    c.neutral.anchors.dark = "#101010".to_string();
    let neutral_table = c
        .compile_named_role_table()
        .expect("exact-gray neutral source compiles without an override");
    assert_eq!(neutral_table.chroma(), RoleChroma::Neutral);

    c.neutral.anchors.dark = "#101011".to_string();
    let chromatic_table = c
        .compile_named_role_table()
        .expect("nearest chromatic neutral source retains its direction");
    assert!(matches!(chromatic_table.chroma(), RoleChroma::Curve { .. }));

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
    )
    .expect("валидная ахроматическая brand-фикстура обязана резолвиться");

    let (_, label) = set
        .iter()
        .find(|(name, _)| name == "label-brand-primary")
        .expect("цветная brand-роль есть");
    let Resolved::Color { solved, .. } = label else {
        panic!("ожидался Color");
    };
    assert_achromatic_hex(solved.hex(), "TextAnchor from achromatic Brand");

    // Exact-gray Brand must not affect an unrelated client-owned family source.
    // The role name below is opaque fixture data and carries no Core semantics.
    let (_, r) = set
        .iter()
        .find(|(n, _)| n == "fill-danger-primary")
        .expect("роль есть");
    let Resolved::Translucent(r) = r else {
        panic!("ожидался Translucent");
    };
    assert_eq!(
        r.tint_hex(),
        "#FF3B30",
        "независимый family-источник обязан сохранить клиентский якорь"
    );
}

/// `NeutralPick` — данные, а не запрос на общий подтон таблицы: выбранный точный
/// серый якорь остаётся ахроматическим при цветной policy других нейтралей.
#[test]
fn material_uses_the_selected_neutral_source_before_chroma_classification() {
    let mut config = labui_reference();
    config.neutral.tint.hue_override_deg = None;
    assert!(matches!(
        config.compile_named_role_table().unwrap().chroma(),
        RoleChroma::Curve { .. }
    ));
    for (name, recipe) in &mut config.roles {
        if name == "fill-brand-secondary" {
            *recipe = RoleRecipe::Material {
                source: LadderSource::Neutral(NeutralPick::Light),
                tone_light: 12.0,
                tone_dark: 12.0,
                floor: Floor::AaUi,
            };
        }
    }

    let table = config.compile_named_role_table().unwrap();
    let set = resolve_named_set(
        &BgInput::solid("#FFFFFF").unwrap(),
        &table,
        &ViewingConditions::srgb(),
    )
    .expect("exact-gray material source must resolve under a chromatic table policy");
    let (_, Resolved::Material(material)) = set
        .iter()
        .find(|(name, _)| name == "fill-brand-secondary")
        .expect("material role exists")
    else {
        panic!("fill-brand-secondary must resolve to Material");
    };
    assert_achromatic_hex(material.tint_hex(), "selected Neutral(Light) Material");
    assert_achromatic_hex(material.base_hex(), "selected Neutral(Light) Material");
}

/// Transitional characterization only: these paths are removed with the closed
/// recipe menu. The durable law lives in `Srgb8`, `SourceHuePlan` and the generic
/// curve tests: exact emitted gray has no hue; the nearest off-axis byte does.
#[test]
fn every_hue_consuming_path_preserves_achromatic_source_identity() {
    let viewing_conditions = [
        ViewingConditions::srgb(),
        ViewingConditions::dim_surround(),
        ViewingConditions::srgb_high_contrast(),
        ViewingConditions::dim_surround_high_contrast(),
    ];

    for vc in viewing_conditions {
        let mut config = labui_reference();
        config.brand.anchors = crate::ladder::ThemeAnchors {
            light: "#808080".to_string(),
            dark: "#808080".to_string(),
            light_ic: "#808080".to_string(),
            dark_ic: "#808080".to_string(),
        };
        for (name, recipe) in &mut config.roles {
            match name.as_str() {
                "label-brand-secondary" => {
                    *recipe = RoleRecipe::TextAnchor {
                        fraction: 0.5,
                        floor: Floor::AaUi,
                        hue: Some(LadderSource::Brand),
                    };
                }
                "fill-brand-secondary" => {
                    *recipe = RoleRecipe::Material {
                        source: LadderSource::Brand,
                        tone_light: 12.0,
                        tone_dark: 12.0,
                        floor: Floor::AaUi,
                    };
                }
                _ => {}
            }
        }

        let table = config
            .compile_named_role_table()
            .expect("achromatic Brand recipes compile");
        let background = if vc.is_dark_theme() {
            BgInput::solid("#101010").unwrap()
        } else {
            BgInput::solid("#FFFFFF").unwrap()
        };
        let set = resolve_named_set(&background, &table, &vc)
            .expect("achromatic source-derived recipes resolve");

        for role in ["label-brand-primary", "label-brand-secondary"] {
            let (_, Resolved::Color { solved, .. }) = set
                .iter()
                .find(|(name, _)| name == role)
                .unwrap_or_else(|| panic!("missing {role}"))
            else {
                panic!("{role} must resolve to Color");
            };
            assert_achromatic_hex(solved.hex(), role);
        }

        let (_, Resolved::Material(material)) = set
            .iter()
            .find(|(name, _)| name == "fill-brand-secondary")
            .expect("material role exists")
        else {
            panic!("fill-brand-secondary must resolve to Material");
        };
        assert_achromatic_hex(material.tint_hex(), "Material tint");
        assert_achromatic_hex(material.base_hex(), "Material base");
    }
}

#[test]
fn nearest_chromatic_source_survives_every_current_source_consuming_path() {
    let mut config = labui_reference();
    config.brand.anchors = crate::ladder::ThemeAnchors {
        light: "#808081".to_string(),
        dark: "#808081".to_string(),
        light_ic: "#808081".to_string(),
        dark_ic: "#808081".to_string(),
    };
    for (name, recipe) in &mut config.roles {
        match name.as_str() {
            "label-brand-secondary" => {
                *recipe = RoleRecipe::TextAnchor {
                    fraction: 0.5,
                    floor: Floor::AaUi,
                    hue: Some(LadderSource::Brand),
                };
            }
            "fill-brand-secondary" => {
                *recipe = RoleRecipe::Material {
                    source: LadderSource::Brand,
                    tone_light: 12.0,
                    tone_dark: 12.0,
                    floor: Floor::AaUi,
                };
            }
            _ => {}
        }
    }

    let table = config.compile_named_role_table().unwrap();
    let set = resolve_named_set(
        &BgInput::solid("#FFFFFF").unwrap(),
        &table,
        &ViewingConditions::srgb(),
    )
    .unwrap();

    for role in ["label-brand-primary", "label-brand-secondary"] {
        let (_, Resolved::Color { solved, .. }) = set
            .iter()
            .find(|(name, _)| name == role)
            .unwrap_or_else(|| panic!("missing {role}"))
        else {
            panic!("{role} must resolve to Color");
        };
        assert_chromatic_hex(solved.hex(), role);
    }

    let (_, Resolved::Material(material)) = set
        .iter()
        .find(|(name, _)| name == "fill-brand-secondary")
        .expect("material role exists")
    else {
        panic!("fill-brand-secondary must resolve to Material");
    };
    assert_chromatic_hex(material.base_hex(), "Material");
}

#[test]
fn achromatic_solid_floor_never_invents_hue() {
    let mut floor_config = labui_reference();
    floor_config.brand.anchors = crate::ladder::ThemeAnchors {
        light: "#E0E0E0".to_string(),
        dark: "#E0E0E0".to_string(),
        light_ic: "#E0E0E0".to_string(),
        dark_ic: "#E0E0E0".to_string(),
    };
    let floor_table = floor_config.compile_named_role_table().unwrap();
    let floor_set = resolve_named_set(
        &BgInput::solid("#FFFFFF").unwrap(),
        &floor_table,
        &ViewingConditions::srgb(),
    )
    .unwrap();
    let (_, Resolved::Translucent(border)) = floor_set
        .iter()
        .find(|(name, _)| name == "border-brand-strong")
        .expect("solid border exists")
    else {
        panic!("border-brand-strong must resolve to Translucent");
    };
    assert!(
        border.floor_coerced(),
        "fixture must execute the floor-shift branch"
    );
    assert_achromatic_hex(border.tint_hex(), "solid floor shift");
}

#[test]
fn adjacent_foreign_source_cannot_change_an_independent_solve() {
    use crate::ladder::LadderTint;
    use crate::semantic::{NamedRoleTable, RoleChroma, RoleSpec, TextAnchor};

    let tint = |hex: &str| {
        let encoded = crate::spaces::srgb::srgb_encoded_from_hex(hex).unwrap();
        LadderTint::new([encoded; 4]).unwrap()
    };
    let senior_anchor = TextAnchor::new(0.9, Floor::AaText)
        .unwrap()
        .with_hue(tint("#FF3B30"));
    let junior_anchor = TextAnchor::new(0.8, Floor::AaText)
        .unwrap()
        .with_hue(tint("#808080"));
    let table = NamedRoleTable::new(
        vec![
            ("senior".to_string(), RoleSpec::Anchor(senior_anchor)),
            ("junior".to_string(), RoleSpec::Anchor(junior_anchor)),
        ],
        Vec::new(),
        RoleChroma::Neutral,
    )
    .expect("two source identities are a valid table");

    let background = BgInput::solid("#6F6F6F").unwrap();
    let vc = ViewingConditions::srgb();
    let set = resolve_named_set(&background, &table, &vc).unwrap();
    let (_, junior) = set
        .iter()
        .find(|(name, _)| name == "junior")
        .expect("junior exists");
    let Resolved::Color { solved, .. } = junior else {
        panic!("junior must resolve to Color");
    };
    assert_achromatic_hex(solved.hex(), "independent achromatic source");

    let isolated = NamedRoleTable::new(
        vec![("junior".to_string(), RoleSpec::Anchor(junior_anchor))],
        Vec::new(),
        RoleChroma::Neutral,
    )
    .unwrap();
    let isolated_set = resolve_named_set(&background, &isolated, &vc).unwrap();
    let Resolved::Color {
        solved: isolated_solved,
        ..
    } = &isolated_set[0].1
    else {
        panic!("isolated junior must resolve to Color");
    };
    assert_eq!(solved.hex(), isolated_solved.hex());
    assert_eq!(
        junior, &isolated_set[0].1,
        "an unrelated adjacent source must not change value or provenance"
    );
}

#[test]
fn all_achromatic_material_is_lawful_under_a_neutral_table_policy() {
    use crate::ladder::LadderTint;
    use crate::semantic::{DjMagnitude, NamedRoleTable, RoleChroma, RoleSpec};

    let gray = crate::spaces::srgb::srgb_encoded_from_hex("#808080").unwrap();
    let table = NamedRoleTable::new(
        vec![(
            "material".to_string(),
            RoleSpec::Material {
                hue: Some(LadderTint::new([gray; 4]).unwrap()),
                tone: DjMagnitude::new(12.0, 12.0),
                floor: Floor::AaUi,
            },
        )],
        Vec::new(),
        RoleChroma::Neutral,
    )
    .expect("all-achromatic source needs no chromatic table policy");
    let set = resolve_named_set(
        &BgInput::solid("#FFFFFF").unwrap(),
        &table,
        &ViewingConditions::srgb(),
    )
    .unwrap();
    let Resolved::Material(material) = &set[0].1 else {
        panic!("material must resolve");
    };
    assert_achromatic_hex(material.tint_hex(), "neutral-policy Material");
}

/// КОМПОЗИЦИОННЫЙ контракт FX-стека теней.
///
/// Прежний контракт держал только пер-токенный порядок (|Lc| каждой ступени
/// сама по себе). Закон владельца сильнее: токены НАСЛАИВАЮТСЯ (minor под
/// ambient под penumbra под major), и прогрессивным обязан быть СУММАРНЫЙ
/// эффект composited-стека. Здесь стек компонуется честной альфа-композицией
/// (`alpha::composite_over_encoded`, тот же оператор, что у браузера) слой за
/// слоем над светлым фоном паспорта, и проверяется:
///   (1) каждый слой меняет пиксели: state_k ≠ state_{k-1} на 8-битной сетке
///       (класс `composite_distinct`);
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
    let set =
        resolve_named_set(&bg, &table, &vc).expect("валидный shadow-stack обязан резолвиться");

    let stack = [
        "fx-shadow-minor",
        "fx-shadow-ambient",
        "fx-shadow-penumbra",
        "fx-shadow-major",
    ];
    let bg_jp = LcsColor::from_hex_with_vc(bg_hex, &vc).unwrap().jp();
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
        state = composite_over_encoded(tint, t.alpha(), state)
            .expect("эмитированные tint/alpha и предыдущий композит лежат в домене");
        let state_hex = hex_from_srgb_encoded(state);
        // (1) слой меняет пиксели поверх уже наслоённого стека.
        assert_ne!(
            state_hex, prev_hex,
            "{name}: наслоение слоя не изменило композит ({state_hex}) — вырожденная ступень стека"
        );
        // (2) суммарная различимость стека от фона строго растёт.
        let jp = LcsColor::from_hex_with_vc(&state_hex, &vc).unwrap().jp();
        let delta = (jp - bg_jp).abs();
        assert!(
            delta > prev_delta,
            "{name}: композиция стека не прогрессивна: |ΔJ'| {delta:.4} ≤ пред. {prev_delta:.4}"
        );
        prev_delta = delta;
    }
}

/// Kind glow: screen-слои + решённая интенсивность.
///
/// Закрепляет новую эмиссию fx-glow-* (взамен выведенных из стаб-сверки
/// Ladder@52-строк): (а) на тёмной базе паспорта свечение решается БЕЗ
/// деградации, halo = пер-темный якорь источника, α ∈ (0, 1], фактический шаг
/// в допуске квантования от контрактной ступени Base; (б) на белом фоне
/// белое нейтральное свечение возвращает явный typed target status (на белом
/// screen — point-no-op reference-профиля), не молчание и не ошибка.
#[test]
fn glow_roles_resolve_screen_layers() {
    let cfg = labui_reference();
    let table = cfg
        .compile_named_role_table()
        .expect("фикстура labui компилируется");

    // (а) тёмная база: полноценное свечение бренда.
    let bg_dark = BgInput::solid("#101012").unwrap();
    let vc_dark = ViewingConditions::dim_surround();
    let set = resolve_named_set(&bg_dark, &table, &vc_dark)
        .expect("валидный Glow-контракт обязан резолвиться");
    let (_, res) = set
        .iter()
        .find(|(n, _)| n == "fx-glow-brand")
        .expect("fx-glow-brand в наборе");
    let g = match res {
        Resolved::Glow(g) => g,
        other => panic!("fx-glow-brand должен быть Resolved::Glow, получено {other:?}"),
    };
    assert_eq!(
        g.target_status(),
        crate::glow::GlowTargetStatus::LegacyReached,
        "бренд-свечение на тёмной базе достигает target"
    );
    assert_eq!(g.halo_hex(), "#4A8FFF", "halo = пер-темный якорь бренда");
    assert!(g.alpha() > 0.0 && g.alpha() <= 1.0);
    let target = crate::glow::GlowStep::Base.target_dj();
    assert!(
        g.halo_achieved_dj() >= target - 1e-9 && g.halo_achieved_dj() - target < 0.5,
        "шаг ступени Base: достигнуто {:.4} (ожидалось [цель, цель+0.5))",
        g.halo_achieved_dj()
    );
    // Анатомия: core светлее halo (пересвет).
    let vc = &vc_dark;
    let jp = |hex: &str| {
        crate::lcs::LcsColor::from_hex_with_vc(hex, vc)
            .unwrap()
            .jp()
    };
    assert!(jp(g.core_hex()) > jp(g.halo_hex()), "core светлее halo");

    // (б) белое свечение на белом — честная деградация.
    let bg_white = BgInput::solid("#FFFFFF").unwrap();
    let set = resolve_named_set(&bg_white, &table, &ViewingConditions::srgb())
        .expect("валидный Glow-контракт обязан резолвиться");
    let (_, res) = set
        .iter()
        .find(|(n, _)| n == "fx-glow-neutral")
        .expect("fx-glow-neutral в наборе");
    match res {
        Resolved::Glow(g) => {
            assert_eq!(
                g.target_status(),
                crate::glow::GlowTargetStatus::LegacyUnreachable,
                "белое свечение на белом обязано сообщить недостижимость"
            );
            assert!(
                g.halo_achieved_dj() < 0.5,
                "screen над белым гаснет физически"
            );
        }
        other => panic!("fx-glow-neutral должен быть Resolved::Glow, получено {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. Пустой контракт отклоняется на загрузке.
//    Агностичность: ядро не знает ролей — конфиг несёт СВОЙ словарь; голый
//    контракт (без ролей и алиасов) — честная ошибка на загрузке, не тихий приём.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn empty_contract_is_rejected_at_load() {
    // Голый контракт: валидная структура, но ни ролей, ни алиасов — честная
    // ошибка НА ЗАГРУЗКЕ (validate = компиляция), не тихий пустой приём.
    let mut empty = labui_reference();
    empty.roles.clear();
    empty.aliases.clear();
    assert_eq!(
        empty.validate(),
        Err(ConfigError::EmptyContract),
        "конфиг без ролей/алиасов обязан отклоняться"
    );
    assert_eq!(
        empty.compile_named_role_table().err(),
        Some(ConfigError::EmptyContract),
        "отказ на компиляции (загрузке), не на использовании"
    );

    // Сообщение по-русски и называет выход.
    let msg = ConfigError::EmptyContract.to_string();
    assert!(
        msg.contains("контракт пуст") && msg.contains("roles"),
        "сообщение по-русски и подсказывает выход: {msg:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Материал (whitepaper, «Точечные композиции»): двухслойный контракт «тинт 01 (α) + база 02» с ВЫВЕДЕННОЙ α.
// ─────────────────────────────────────────────────────────────────────────────

/// Заменить роль на произвольный рецепт и вернуть её резолв на `bg_hex`/`vc`.
fn resolve_role_recipe(
    role: &str,
    recipe: RoleRecipe,
    bg_hex: &str,
    vc: &ViewingConditions,
) -> Resolved {
    let cfg = with_role_recipe(role, recipe);
    let table = cfg
        .compile_named_role_table()
        .expect("material-конфиг компилируется");
    let bg = BgInput::solid(bg_hex).unwrap();
    resolve_named_set(&bg, &table, vc)
        .expect("валидный material-контракт обязан резолвиться")
        .into_iter()
        .find(|(n, _)| n == role)
        .map(|(_, r)| r)
        .expect("роль присутствует в резолве")
}

/// Нейтральный material-рецепт на данном |ΔJ'| тона (обе темы).
fn neutral_material(tone: f64, floor: Floor) -> RoleRecipe {
    RoleRecipe::Material {
        source: LadderSource::Neutral(NeutralPick::Mid),
        tone_light: tone,
        tone_dark: tone,
        floor,
    }
}

/// Резолв нейтрального материала на белом фоне (светлая тема).
fn material_on_white(tone: f64) -> Resolved {
    resolve_role_recipe(
        "fill-brand-secondary",
        neutral_material(tone, Floor::AaText),
        "#FFFFFF",
        &ViewingConditions::srgb(),
    )
}

/// Двухслойность + солид-канон байт-точен + AA-гарантия держится.
#[test]
fn material_two_layer_solid_canon_byte_exact_and_guaranteed() {
    let res = material_on_white(12.0);
    let Resolved::Material(m) = &res else {
        panic!("ожидался Material, получено {res:?}");
    };
    // Тинт 01 = база 02 = солид-канон (один тон).
    assert_eq!(m.tint_hex(), m.base_hex(), "01 и 02 — один тон");
    assert_eq!(m.tint_hex(), m.solid_hex(), "солид-канон = тон");
    // Композит 01-над-02 БАЙТ-ТОЧНО равен тону (композит T над T есть T).
    let solid = crate::alpha::composite_hex(m.tint_hex(), m.alpha(), m.base_hex()).unwrap();
    assert_eq!(
        &solid,
        m.solid_hex(),
        "солид-канон 01-над-02 разошёлся с тоном"
    );
    // α выведена в (0,1] и держит пол.
    assert!(
        m.alpha() > 0.0 && m.alpha() <= 1.0,
        "α вне (0,1]: {}",
        m.alpha()
    );
    assert_eq!(
        m.alpha_status(),
        crate::material::MaterialAlphaStatusV1::Satisfied,
        "AA-floor обязан иметь typed satisfied status"
    );
    assert!(m.worst_contrast() >= m.floor() - 1e-9, "worst < floor");
    assert!((m.floor() - 4.5).abs() < 1e-12, "AA-text пол = 4.5");
}

/// Гарантия читаемости пересчитываема из эмитированных `01`/`02`: public core
/// recheck побитно совпадает с сохранённым conservative verdict, а независимые
/// byte-scale consumer probes не опускаются ниже него.
#[test]
fn material_guarantee_recomputable_over_worst_backdrop() {
    let res = material_on_white(15.0);
    let Resolved::Material(m) = &res else {
        panic!("ожидался Material");
    };
    let tint = crate::spaces::srgb::srgb_encoded_from_hex(m.tint_hex()).unwrap();
    let recomputed = crate::material::worst_contrast_encoded(
        tint,
        m.alpha(),
        &crate::material::BackdropBox::FULL,
        m.pole(),
    )
    .unwrap();
    assert_eq!(recomputed.to_bits(), m.worst_contrast().to_bits());

    // Independent official scalar order. The old version of this test called
    // alpha::composite_over_encoded, which is the normalized-expanded profile
    // and therefore could not prove material consumer parity.
    let pole_lum = if matches!(m.pole(), crate::material::Pole::White) {
        1.0
    } else {
        0.0
    };
    let probes = [0.0, 0.039_28, 0.039_280_000_000_000_01, 0.5, 1.0];
    let mut measured_min = f64::INFINITY;
    for red in probes {
        for green in probes {
            for blue in probes {
                let background = [red, green, blue];
                let composite = core::array::from_fn(|channel| {
                    let tint_byte = (tint[channel] * 255.0).round();
                    let background_byte_scale = background[channel] * 255.0;
                    (background_byte_scale + m.alpha() * (tint_byte - background_byte_scale))
                        / 255.0
                });
                measured_min = measured_min.min(crate::spaces::srgb::relative_luminance_ratio(
                    pole_lum,
                    crate::spaces::srgb::encoded_srgb_relative_luminance(composite),
                ));
            }
        }
    }
    assert!(m.worst_contrast() <= measured_min);
    assert!(
        m.worst_contrast() >= 4.5,
        "conservative verdict ниже AA-пола"
    );
}

/// Нейтральный материал БАЙТ-в-байт переиспользует тон dj-anchor (та же физика
/// поверхности), а не изобретает второй путь.
#[test]
fn neutral_material_tone_matches_dj_anchor() {
    let vc = ViewingConditions::srgb();
    let mat = resolve_role_recipe(
        "fill-brand-secondary",
        neutral_material(14.0, Floor::AaText),
        "#FFFFFF",
        &vc,
    );
    let Resolved::Material(m) = &mat else {
        panic!("ожидался Material");
    };
    let dj = resolve_role_recipe(
        "fill-brand-secondary",
        RoleRecipe::DjAnchor {
            light: 14.0,
            dark: 14.0,
        },
        "#FFFFFF",
        &vc,
    );
    let dj_hex = dj.solved().expect("dj-anchor решается в цвет").hex();
    assert_eq!(
        m.tint_hex(),
        dj_hex,
        "нейтральный материал обязан нести тот же тон, что dj-anchor"
    );
}

/// Семейный (brand) материал несёт ОТТЕНОК семьи — его тон отличается от
/// нейтрального на том же |ΔJ'| (акцент-стекло разблокировано).
#[test]
fn accent_material_tone_carries_family_hue() {
    let vc = ViewingConditions::srgb();
    let neutral = material_on_white(22.0);
    let brand = resolve_role_recipe(
        "fill-brand-secondary",
        RoleRecipe::Material {
            source: LadderSource::Brand,
            tone_light: 22.0,
            tone_dark: 22.0,
            floor: Floor::AaText,
        },
        "#FFFFFF",
        &vc,
    );
    let (Resolved::Material(n), Resolved::Material(b)) = (&neutral, &brand) else {
        panic!("ожидались Material");
    };
    assert_ne!(
        n.tint_hex(),
        b.tint_hex(),
        "brand-материал обязан отличаться от нейтрального (оттенок семьи)"
    );
}

/// Порядок тиров ВЫВОДИТСЯ физикой, не подбором: на светлой теме тон дальше от
/// белого (крупнее |ΔJ'| = base) требует ПЛОТНЕЕ α, чем ближе (subtle).
#[test]
fn material_base_denser_than_subtle_light_theme() {
    let alpha_of = |tone: f64| match material_on_white(tone) {
        Resolved::Material(m) => m.alpha(),
        other => panic!("ожидался Material, получено {other:?}"),
    };
    let subtle = alpha_of(6.0);
    let base = alpha_of(26.0);
    assert!(
        base > subtle,
        "base ({base}) обязан быть плотнее subtle ({subtle})"
    );
}

/// RED-proof тона: разный |ΔJ'| обязан дать разный тон (рецепт не слеп к тиру).
#[test]
fn material_bites_on_tone_mutation() {
    let tone_of = |tone: f64| match material_on_white(tone) {
        Resolved::Material(m) => m.tint_hex().to_string(),
        other => panic!("ожидался Material, получено {other:?}"),
    };
    assert_ne!(
        tone_of(8.0),
        tone_of(28.0),
        "RED-proof: разный |ΔJ'| дал одинаковый тон — рецепт слеп к тиру"
    );
}

/// Тон-база различима от фона (|ΔJ'| ≈ цель) и отмечена distinct.
#[test]
fn material_tone_is_distinguishable_from_bg() {
    let res = material_on_white(15.0);
    let Resolved::Material(m) = &res else {
        panic!("ожидался Material");
    };
    assert!(
        m.distinct(),
        "тон обязан быть отличим от фона на 8-битной сетке"
    );
    assert!(
        (m.achieved_dj() - 15.0).abs() < 2.5,
        "achieved_dj {} далёк от цели 15.0",
        m.achieved_dj()
    );
}

/// Тёмная тема: тёмная поверхность → белый коммит-полюс, гарантия держится.
#[test]
fn material_dark_theme_white_pole_guaranteed() {
    let res = resolve_role_recipe(
        "fill-brand-secondary",
        neutral_material(15.0, Floor::AaText),
        "#101012",
        &ViewingConditions::dim_surround(),
    );
    let Resolved::Material(m) = &res else {
        panic!("ожидался Material");
    };
    assert!(
        matches!(m.pole(), crate::material::Pole::White),
        "тёмная поверхность обязана коммитить белый полюс"
    );
    assert_eq!(
        m.alpha_status(),
        crate::material::MaterialAlphaStatusV1::Satisfied,
        "AA-floor обязан иметь typed satisfied status и на тёмной теме"
    );
}

/// Валидатор: material без пола читаемости отвергается на загрузке.
#[test]
fn material_floor_none_rejected() {
    let cfg = with_role_recipe("fill-brand-secondary", neutral_material(10.0, Floor::None));
    assert!(
        matches!(
            cfg.validate(),
            Err(ConfigError::MaterialFloorRequired { role }) if role == "fill-brand-secondary"
        ),
        "floor=none обязан быть отвергнут"
    );
}

/// Валидатор: неположительный |ΔJ'| тона отвергается (нет различимой поверхности).
#[test]
fn material_non_positive_tone_rejected() {
    let cfg = with_role_recipe(
        "fill-brand-secondary",
        RoleRecipe::Material {
            source: LadderSource::Neutral(NeutralPick::Mid),
            tone_light: 0.0,
            tone_dark: 10.0,
            floor: Floor::AaText,
        },
    );
    assert!(
        matches!(
            cfg.validate(),
            Err(ConfigError::OutOfBounds { handle, .. }) if handle == "roles.fill-brand-secondary.tone_light"
        ),
        "tone_light=0 обязан быть отвергнут"
    );
}

/// Валидатор: material со ссылкой на несуществующее семейство отвергается.
#[test]
fn material_unknown_family_rejected() {
    let cfg = with_role_recipe(
        "fill-brand-secondary",
        RoleRecipe::Material {
            source: LadderSource::Family("нет-такого".to_string()),
            tone_light: 10.0,
            tone_dark: 10.0,
            floor: Floor::AaText,
        },
    );
    assert!(
        matches!(cfg.validate(), Err(ConfigError::UnknownFamily { .. })),
        "ссылка на несуществующее семейство обязана быть отвергнута"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// C7d characterization corpus: каждый публичный construction path материала
// закреплён БИТ-в-бит на текущей физике ДО lowering. Он обязан пройти без
// изменений после переноса исполнения в общий Program-путь: пропущенный
// construction path, смена компоситора/порядка, один backdrop вместо коридора
// или деградация без свидетельства ломают эти пины.
// ─────────────────────────────────────────────────────────────────────────────

/// Один закреплённый вектор корпуса: контекст, вход и точные битовые выходы.
struct MaterialCorpusVector {
    label: &'static str,
    source: fn() -> LadderSource,
    tone_light: f64,
    tone_dark: f64,
    floor: Floor,
    vc: fn() -> ViewingConditions,
    bg_hex: &'static str,
    tone_hex: &'static str,
    alpha_bits: u64,
    worst_bits: u64,
    achieved_dj_bits: u64,
    pole_white: bool,
}

/// Полный корпус: все источники (Brand / Family / все пять NeutralPick) во всех
/// четырёх темах паспорта плюс серый фон. Смешанные tone_light≠tone_dark и оба
/// пола (AaText/AaUi) входят в выборку.
fn material_characterization_corpus() -> Vec<MaterialCorpusVector> {
    fn srgb() -> ViewingConditions {
        ViewingConditions::srgb()
    }
    fn dim() -> ViewingConditions {
        ViewingConditions::dim_surround()
    }
    fn srgb_ic() -> ViewingConditions {
        ViewingConditions::srgb_high_contrast()
    }
    fn dim_ic() -> ViewingConditions {
        ViewingConditions::dim_surround_high_contrast()
    }
    fn mid() -> LadderSource {
        LadderSource::Neutral(NeutralPick::Mid)
    }
    fn light() -> LadderSource {
        LadderSource::Neutral(NeutralPick::Light)
    }
    fn dark() -> LadderSource {
        LadderSource::Neutral(NeutralPick::Dark)
    }
    fn edge() -> LadderSource {
        LadderSource::Neutral(NeutralPick::Edge)
    }
    fn inverted() -> LadderSource {
        LadderSource::Neutral(NeutralPick::Inverted)
    }
    fn brand() -> LadderSource {
        LadderSource::Brand
    }
    fn purple() -> LadderSource {
        LadderSource::Family("purple".to_string())
    }
    #[rustfmt::skip]
    let corpus = vec![
        // ── srgb (светлая), #FFFFFF ─────────────────────────────────────────
        MaterialCorpusVector { label: "neutral-mid/srgb", source: mid, tone_light: 12.0, tone_dark: 14.0, floor: Floor::AaText, vc: srgb, bg_hex: "#FFFFFF", tone_hex: "#D6D6E0", alpha_bits: 0x3fe14d5bdf014d24, worst_bits: 0x4012000000000002, achieved_dj_bits: 0x4028316ec16bfd18, pole_white: false },
        MaterialCorpusVector { label: "neutral-light/srgb", source: light, tone_light: 9.0, tone_dark: 9.0, floor: Floor::AaUi, vc: srgb, bg_hex: "#FFFFFF", tone_hex: "#E1E1E1", alpha_bits: 0x3fd953f339f0f17d, worst_bits: 0x4008000000000001, achieved_dj_bits: 0x4021d4eb3ff155d0, pole_white: false },
        MaterialCorpusVector { label: "neutral-dark/srgb", source: dark, tone_light: 11.0, tone_dark: 11.0, floor: Floor::AaUi, vc: srgb, bg_hex: "#FFFFFF", tone_hex: "#D9D9E3", alpha_bits: 0x3fda2c1ec0af1ef2, worst_bits: 0x4008000000000001, achieved_dj_bits: 0x40264854a4095b68, pole_white: false },
        MaterialCorpusVector { label: "neutral-edge/srgb", source: edge, tone_light: 10.0, tone_dark: 10.0, floor: Floor::AaText, vc: srgb, bg_hex: "#FFFFFF", tone_hex: "#DDDDE6", alpha_bits: 0x3fe0c302b2c13d45, worst_bits: 0x4012000000000002, achieved_dj_bits: 0x4023d1b0d337e540, pole_white: false },
        MaterialCorpusVector { label: "neutral-inverted/srgb", source: inverted, tone_light: 13.0, tone_dark: 13.0, floor: Floor::AaText, vc: srgb, bg_hex: "#FFFFFF", tone_hex: "#D3D3DD", alpha_bits: 0x3fe18c1bebb958a9, worst_bits: 0x4012000000000002, achieved_dj_bits: 0x402a1ed191c26ab8, pole_white: false },
        MaterialCorpusVector { label: "brand/srgb", source: brand, tone_light: 22.0, tone_dark: 18.0, floor: Floor::AaText, vc: srgb, bg_hex: "#FFFFFF", tone_hex: "#B5BAC1", alpha_bits: 0x3fe4086250007280, worst_bits: 0x4012000000000002, achieved_dj_bits: 0x4035f37bf4fa5200, pole_white: false },
        MaterialCorpusVector { label: "family-purple/srgb", source: purple, tone_light: 16.0, tone_dark: 16.0, floor: Floor::AaUi, vc: srgb, bg_hex: "#FFFFFF", tone_hex: "#CEC8D2", alpha_bits: 0x3fdc3523dcf40701, worst_bits: 0x4008000000000001, achieved_dj_bits: 0x403027ede6ed1abc, pole_white: false },
        // ── dim (тёмная), #101012 ───────────────────────────────────────────
        MaterialCorpusVector { label: "neutral-mid/dim", source: mid, tone_light: 12.0, tone_dark: 14.0, floor: Floor::AaText, vc: dim, bg_hex: "#101012", tone_hex: "#2D2D33", alpha_bits: 0x3fe4d1e7fe28d58c, worst_bits: 0x4012000000000000, achieved_dj_bits: 0x402c088852a3aeb1, pole_white: true },
        MaterialCorpusVector { label: "neutral-light/dim", source: light, tone_light: 9.0, tone_dark: 9.0, floor: Floor::AaUi, vc: dim, bg_hex: "#101012", tone_hex: "#232323", alpha_bits: 0x3fdedf445a6c8d35, worst_bits: 0x4008000000000000, achieved_dj_bits: 0x4022148cd5a1d431, pole_white: true },
        MaterialCorpusVector { label: "neutral-dark/dim", source: dark, tone_light: 11.0, tone_dark: 11.0, floor: Floor::AaUi, vc: dim, bg_hex: "#101012", tone_hex: "#27272D", alpha_bits: 0x3fdf81fdd297a6c1, worst_bits: 0x4008000000000000, achieved_dj_bits: 0x402668319c957807, pole_white: true },
        MaterialCorpusVector { label: "neutral-edge/dim", source: edge, tone_light: 10.0, tone_dark: 10.0, floor: Floor::AaText, vc: dim, bg_hex: "#101012", tone_hex: "#21262B", alpha_bits: 0x3fe40b0c57df5fa6, worst_bits: 0x4012000000000000, achieved_dj_bits: 0x40243de1f7538c49, pole_white: true },
        MaterialCorpusVector { label: "neutral-inverted/dim", source: inverted, tone_light: 13.0, tone_dark: 13.0, floor: Floor::AaText, vc: dim, bg_hex: "#101012", tone_hex: "#2B2B31", alpha_bits: 0x3fe49f84665de8cc, worst_bits: 0x4012000000000000, achieved_dj_bits: 0x402a2a534bb93399, pole_white: true },
        MaterialCorpusVector { label: "brand/dim", source: brand, tone_light: 22.0, tone_dark: 18.0, floor: Floor::AaText, vc: dim, bg_hex: "#101012", tone_hex: "#33363C", alpha_bits: 0x3fe5afa483913ece, worst_bits: 0x4012000000000000, achieved_dj_bits: 0x4031d5399ccd0c3e, pole_white: true },
        MaterialCorpusVector { label: "family-purple/dim", source: purple, tone_light: 16.0, tone_dark: 16.0, floor: Floor::AaUi, vc: dim, bg_hex: "#101012", tone_hex: "#343037", alpha_bits: 0x3fe083b9c39a26ac, worst_bits: 0x4008000000000001, achieved_dj_bits: 0x402fde43cb629035, pole_white: true },
        // ── srgb-ic ─────────────────────────────────────────────────────────
        MaterialCorpusVector { label: "neutral-mid/srgb-ic", source: mid, tone_light: 12.0, tone_dark: 14.0, floor: Floor::AaText, vc: srgb_ic, bg_hex: "#FFFFFF", tone_hex: "#D6D6E0", alpha_bits: 0x3fe14d5bdf014d24, worst_bits: 0x4012000000000002, achieved_dj_bits: 0x4028316ec16bfd18, pole_white: false },
        MaterialCorpusVector { label: "brand/srgb-ic", source: brand, tone_light: 22.0, tone_dark: 18.0, floor: Floor::AaText, vc: srgb_ic, bg_hex: "#FFFFFF", tone_hex: "#B5BAC2", alpha_bits: 0x3fe40647c3868b55, worst_bits: 0x4012000000000000, achieved_dj_bits: 0x4035eb3157a096e8, pole_white: false },
        MaterialCorpusVector { label: "family-purple/srgb-ic", source: purple, tone_light: 16.0, tone_dark: 16.0, floor: Floor::AaUi, vc: srgb_ic, bg_hex: "#FFFFFF", tone_hex: "#CEC8D2", alpha_bits: 0x3fdc3523dcf40701, worst_bits: 0x4008000000000001, achieved_dj_bits: 0x403027ede6ed1abc, pole_white: false },
        // ── dim-ic ──────────────────────────────────────────────────────────
        MaterialCorpusVector { label: "neutral-mid/dim-ic", source: mid, tone_light: 12.0, tone_dark: 14.0, floor: Floor::AaText, vc: dim_ic, bg_hex: "#101012", tone_hex: "#2D2D33", alpha_bits: 0x3fe4d1e7fe28d58c, worst_bits: 0x4012000000000000, achieved_dj_bits: 0x402c088852a3aeb1, pole_white: true },
        MaterialCorpusVector { label: "brand/dim-ic", source: brand, tone_light: 22.0, tone_dark: 18.0, floor: Floor::AaText, vc: dim_ic, bg_hex: "#101012", tone_hex: "#33373C", alpha_bits: 0x3fe5c37fb50b1c72, worst_bits: 0x4012000000000000, achieved_dj_bits: 0x403222d5f10b5a2e, pole_white: true },
        MaterialCorpusVector { label: "family-purple/dim-ic", source: purple, tone_light: 16.0, tone_dark: 16.0, floor: Floor::AaUi, vc: dim_ic, bg_hex: "#101012", tone_hex: "#343037", alpha_bits: 0x3fe083b9c39a26ac, worst_bits: 0x4008000000000001, achieved_dj_bits: 0x402fde43cb629035, pole_white: true },
        // ── серый фон: полюс переворачивается на светлой теме ────────────────
        MaterialCorpusVector { label: "neutral-mid/gray-bg", source: mid, tone_light: 12.0, tone_dark: 12.0, floor: Floor::AaText, vc: srgb, bg_hex: "#7F7F7F", tone_hex: "#61616A", alpha_bits: 0x3febbb7a9c1a37aa, worst_bits: 0x4012000000000000, achieved_dj_bits: 0x40283945e563ccc8, pole_white: true },
    ];
    corpus
}

/// Бит-в-бит корпус всех construction paths конфига: tone hex, α, worst,
/// |ΔJ'|, полюс, статус и согласованный bracket. Любое изменение
/// компоситора, порядка операций, коридора или выбора кандидата ломает пины.
#[test]
fn material_lowering_characterization_corpus_is_bit_stable() {
    for vector in material_characterization_corpus() {
        let recipe = RoleRecipe::Material {
            source: (vector.source)(),
            tone_light: vector.tone_light,
            tone_dark: vector.tone_dark,
            floor: vector.floor,
        };
        let resolved = resolve_role_recipe(
            "fill-brand-secondary",
            recipe,
            vector.bg_hex,
            &(vector.vc)(),
        );
        let Resolved::Material(m) = &resolved else {
            panic!("{}: ожидался Material, получено {resolved:?}", vector.label);
        };
        assert_eq!(
            m.tint_hex(),
            vector.tone_hex,
            "{}: tone drift",
            vector.label
        );
        assert_eq!(
            m.alpha().to_bits(),
            vector.alpha_bits,
            "{}: alpha drift (got 0x{:016x})",
            vector.label,
            m.alpha().to_bits()
        );
        assert_eq!(
            m.worst_contrast().to_bits(),
            vector.worst_bits,
            "{}: worst-contrast drift (got 0x{:016x})",
            vector.label,
            m.worst_contrast().to_bits()
        );
        assert_eq!(
            m.achieved_dj().to_bits(),
            vector.achieved_dj_bits,
            "{}: achieved-dj drift (got 0x{:016x})",
            vector.label,
            m.achieved_dj().to_bits()
        );
        assert_eq!(
            matches!(m.pole(), crate::material::Pole::White),
            vector.pole_white,
            "{}: pole drift",
            vector.label
        );
        assert_eq!(
            m.alpha_status(),
            crate::material::MaterialAlphaStatusV1::Satisfied,
            "{}: status drift",
            vector.label
        );
        assert!(
            !m.tone_compressed(),
            "{}: unexpected compression",
            vector.label
        );
        assert!(m.distinct(), "{}: tone must stay distinct", vector.label);
        // Согласованность bracket-свидетельства: выбранная α — верхний
        // кандидат, нижний лежит ровно на предыдущем бите поиска.
        match m.alpha_guarantee() {
            crate::material::MaterialAlphaGuaranteeV1::BisectionBracketCharacterizedV1 {
                iterations,
                lower_alpha,
                upper_alpha,
                ..
            } => {
                assert_eq!(iterations, 60, "{}: bisection depth drift", vector.label);
                assert_eq!(
                    upper_alpha.to_bits(),
                    vector.alpha_bits,
                    "{}: bracket upper != selected alpha",
                    vector.label
                );
                assert!(
                    lower_alpha < upper_alpha,
                    "{}: bracket must stay ordered",
                    vector.label
                );
            }
            other => panic!("{}: ожидался bracket, получено {other:?}", vector.label),
        }
        // Пол корпуса выполнен: worst держит запрошенный floor.
        let floor_ratio = vector.floor.min_ratio().expect("corpus floors are legal");
        assert!(
            m.worst_contrast() >= floor_ratio,
            "{}: worst {} ниже пола {}",
            vector.label,
            m.worst_contrast(),
            floor_ratio
        );
    }
}

/// Прямая граница `NamedRoleTable::new` (в обход конфига) с `hue: None`
/// закреплена отдельно: она обязана пережить lowering тем же битовым выходом.
#[test]
fn material_direct_boundary_hue_none_is_bit_stable() {
    use crate::semantic::{DjMagnitude, NamedRoleTable, RoleChroma, RoleSpec};
    let table = NamedRoleTable::new(
        vec![(
            "m".to_string(),
            RoleSpec::Material {
                hue: None,
                tone: DjMagnitude::new(12.0, 12.0),
                floor: Floor::AaText,
            },
        )],
        Vec::new(),
        RoleChroma::Neutral,
    )
    .unwrap();
    let set = resolve_named_set(
        &BgInput::solid("#FFFFFF").unwrap(),
        &table,
        &ViewingConditions::srgb(),
    )
    .unwrap();
    let Resolved::Material(m) = &set[0].1 else {
        panic!("прямая граница обязана резолвить Material");
    };
    assert_eq!(m.tint_hex(), "#D7D7D7");
    assert_eq!(m.alpha().to_bits(), 0x3fe14808ee3564a2);
    assert_eq!(m.worst_contrast().to_bits(), 0x4012000000000000);
    assert_eq!(m.achieved_dj().to_bits(), 0x40282274443b7010);
}

/// Типизированные конфликты корпуса: хроматический источник при neutral-policy
/// отвергается на обеих границах (конфиг и прямая таблица) с канонической
/// причиной, а не исполняется деградированно.
#[test]
fn material_chromatic_source_conflicts_are_typed_on_both_boundaries() {
    use crate::semantic::{DjMagnitude, NamedRoleTable, RoleChroma, RoleSpec};
    // Прямая граница: chromatic hue + Neutral policy → InvalidInput с
    // канонической причиной.
    let blue = crate::spaces::srgb::srgb_encoded_from_hex("#3E87FF").unwrap();
    let error = NamedRoleTable::new(
        vec![(
            "m".to_string(),
            RoleSpec::Material {
                hue: Some(crate::ladder::LadderTint::new([blue; 4]).unwrap()),
                tone: DjMagnitude::new(12.0, 12.0),
                floor: Floor::AaText,
            },
        )],
        Vec::new(),
        RoleChroma::Neutral,
    )
    .expect_err("chromatic material под neutral-policy обязан отвергаться");
    let crate::SolveFailure::InvalidInput(reason) = &error else {
        panic!("ожидался typed InvalidInput, получено {error:?}");
    };
    assert!(
        reason.contains(RoleSpec::INCOMPATIBLE_CHROMA_REASON),
        "конфликт обязан нести каноническую причину: {reason:?}"
    );
}
