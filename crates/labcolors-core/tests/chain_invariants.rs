//! Сквозные инварианты ГРАНИЦ КОНФИГА — вырожденные конфиги солвер обязан
//! пройти без паники и без не-конечных чисел в эмиссии.
//!
//! Что уже закрыто в другом месте (НЕ дублируем):
//! - `property_invariants.rs::resolve_named_set_is_total_and_emits_valid_hex_for_any_valid_config`
//!   свипует ЦВЕТА/доли при ФИКСИРОВАННОЙ 6-ролевой структуре (`base_config`);
//! - `config/tests.rs::alias_to_missing_role_is_rejected` / `unknown_family` —
//!   валидатор ловит структурные ошибки ссылок;
//! - граница (`engine.rs`) отклоняет пустой контракт на загрузке.
//!
//! Дыра, которую закрывает этот файл: тот же класс тотальности, но по оси
//! ВЫРОЖДЕННОЙ СТРУКТУРЫ, а не цвета — 0 ролей, 1 роль, пустая палитра,
//! экстремальные ручки нейтрали/сентимента, чисто чёрный/белый бренд. Контракт:
//! `compile_named_role_table` либо честно возвращает `Err`, либо даёт таблицу,
//! чей `resolve_named_set` ТОТАЛЕН — валидный hex, конечные метрики, а
//! CSS-эмиссия каждого решённого цвета не несёт `NaN`/`inf`. Достижение
//! ассертов = доказательство отсутствия паники (класс «вырожденный вход роняет
//! процесс»).

use labcolors_core::BgInput;
use labcolors_core::{
    Brand, Floor, LadderPosition, LadderSource, NamedRoleTable, NeutralAnchors, NeutralConfig,
    NeutralPick, NeutralTint, PaletteFamily, Resolved, RoleRecipe, SentimentCategory,
    SentimentsConfig, ThemeAnchors, ThemeConfig, ThemesConfig, VcPreset, ViewingConditions,
    oklch_css_from_hex, resolve_named_set,
};

/// `ThemeAnchors` с одинаковыми якорями во всех четырёх слотах — вход не о
/// вариативности якорей, а о вырожденности СТРУКТУРЫ.
fn flat_anchors(hex: &str) -> ThemeAnchors {
    ThemeAnchors {
        light: hex.to_string(),
        dark: hex.to_string(),
        light_ic: hex.to_string(),
        dark_ic: hex.to_string(),
    }
}

/// Четыре канонических viewing-conditions (все пресеты тем): свип обязан
/// покрыть и IC-ветки, где перцептивный пол выше.
fn all_vcs() -> [ViewingConditions; 4] {
    [
        ViewingConditions::srgb(),
        ViewingConditions::dim_surround(),
        ViewingConditions::srgb_high_contrast(),
        ViewingConditions::dim_surround_high_contrast(),
    ]
}

/// Экстремальные фоны: концы яркостной оси и середина — там пол/полярность
/// перещёлкиваются, а деление в APCA у краёв самое хрупкое.
const EXTREME_BGS: [&str; 5] = ["#000000", "#FFFFFF", "#808080", "#010101", "#FEFFFE"];

/// Минимальный НЕцветовой каркас, куда подставляются вырожденные ручки:
/// одна палитра/сентимент нужны только чтобы конфиг был структурно валиден,
/// роли добавляются аргументом.
fn scaffold(
    neutral: NeutralConfig,
    sentiments: SentimentsConfig,
    palette: Vec<PaletteFamily>,
    roles: Vec<(String, RoleRecipe)>,
) -> ThemeConfig {
    ThemeConfig::new(
        Brand {
            anchors: flat_anchors("#007AFF"),
        },
        neutral,
        palette,
        sentiments,
        ThemesConfig {
            entries: vec![
                ("light".to_string(), VcPreset::Srgb),
                ("dark".to_string(), VcPreset::Dim),
            ],
        },
        roles,
        vec![],
    )
}

/// Нейтраль со стандартными ручками — общий фон для тестов, варьирующих ДРУГИЕ оси.
fn plain_neutral() -> NeutralConfig {
    NeutralConfig {
        anchors: NeutralAnchors {
            light: "#FBFBFD".to_string(),
            mid: "#8A8A8E".to_string(),
            dark: "#101012".to_string(),
        },
        tint: NeutralTint {
            ratio: 0.10,
            target_mp: 5.0,
            hue_stiffness: 8.0,
            hue_override_deg: Some(286.0),
        },
        edge: None,
        inverted: None,
    }
}

fn plain_sentiments() -> SentimentsConfig {
    SentimentsConfig { categories: vec![] }
}

/// Проверить, что таблица ТОТАЛЬНА на всех экстремальных фонах × VC: каждый
/// решённый цвет — валидный `#RRGGBB`, его oklch-эмиссия без `NaN`/`inf` и
/// парсится в три КОНЕЧНЫХ компоненты, а метрики (Lc/WCAG, α, dJ') конечны.
/// Панику ловит сам факт достижения ассертов.
///
/// Возвращает число ДОСТИЖИМЫХ (эмитирующих цвет) исходов за весь свип — гард
/// не-вакуумности для вызывающих: тест, чей контракт заявляет эмиссию, обязан
/// увидеть >0, иначе конечность/эмитируемость не проверилась ни разу (все роли
/// решились в None/Unreachable — зелёный впустую).
#[must_use]
fn assert_table_is_total(table: &NamedRoleTable, label: &str) -> usize {
    let mut reachable = 0usize;
    for bg_hex in EXTREME_BGS {
        let bg = BgInput::solid(bg_hex).expect("экстремальный фон валиден");
        for vc in all_vcs() {
            let set = resolve_named_set(&bg, table, &vc);
            for (name, resolved) in &set {
                if assert_resolved_is_finite_and_emittable(resolved, label, name, bg_hex) {
                    reachable += 1;
                }
            }
        }
    }
    reachable
}

/// Ни один исход не несёт не-конечного числа, а каждый hex решается в oklch без
/// `NaN`/`inf`. `Resolved` — `#[non_exhaustive]`: неучтённый вариант обязан
/// падать громко, а не пройти молча. Возвращает `true`, если исход ЭМИТИРУЕТ
/// цвет (Color/Translucent/Glow) — то есть ассерты эмитируемости реально
/// сработали, а не пропущены пустыми ветками None/Unreachable.
fn assert_resolved_is_finite_and_emittable(
    resolved: &Resolved,
    label: &str,
    name: &str,
    bg: &str,
) -> bool {
    let emittable = |hex: &str, alpha: Option<f64>| {
        assert_valid_hex(hex, label, name, bg);
        let css = oklch_css_from_hex(hex, alpha)
            .unwrap_or_else(|e| panic!("{label}/{name}@{bg}: hex {hex} не сериализуется: {e}"));
        let low = css.to_ascii_lowercase();
        assert!(
            !low.contains("nan") && !low.contains("inf"),
            "{label}/{name}@{bg}: oklch несёт нечисло: {css}"
        );
        assert_oklch_components_finite(&css, label, name, bg);
    };

    match resolved {
        Resolved::Color { solved, .. } => {
            emittable(solved.hex(), None);
            assert!(
                solved.lc().is_finite() && solved.wcag_ratio().is_finite(),
                "{label}/{name}@{bg}: не-конечная метрика Lc/WCAG"
            );
            true
        }
        Resolved::Translucent(r) => {
            assert!(
                (0.0..=1.0).contains(&r.alpha()) && r.alpha().is_finite(),
                "{label}/{name}@{bg}: α translucent вне [0,1]/нечисло: {}",
                r.alpha()
            );
            emittable(r.tint_hex(), Some(r.alpha()));
            true
        }
        Resolved::Glow(g) => {
            assert!(
                (0.0..=1.0).contains(&g.alpha()) && g.alpha().is_finite(),
                "{label}/{name}@{bg}: α glow вне [0,1]/нечисло: {}",
                g.alpha()
            );
            emittable(g.core_hex(), None);
            emittable(g.halo_hex(), None);
            true
        }
        Resolved::None => false,
        Resolved::Unreachable(_) => false,
        other => panic!("{label}/{name}@{bg}: неучтённый Resolved: {other:?}"),
    }
}

fn assert_valid_hex(hex: &str, label: &str, name: &str, bg: &str) {
    assert!(
        hex.len() == 7 && hex.starts_with('#') && hex[1..].bytes().all(|b| b.is_ascii_hexdigit()),
        "{label}/{name}@{bg}: битый hex {hex}"
    );
}

/// Числовые компоненты `oklch(L% C H[ / A])` конечны; `H=none` — нормативное
/// состояние отсутствующего hue у ахромата, а не нечисловая ошибка.
fn assert_oklch_components_finite(css: &str, label: &str, name: &str, bg: &str) {
    let inner = css
        .strip_prefix("oklch(")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or_else(|| panic!("{label}/{name}@{bg}: не форма oklch(...): {css}"));
    let (lch, alpha) = match inner.split_once(" / ") {
        Some((lch, a)) => (lch, Some(a)),
        None => (inner, None),
    };
    for part in lch.split_whitespace() {
        if part == "none" {
            continue;
        }
        let v: f64 = part
            .trim_end_matches('%')
            .parse()
            .unwrap_or_else(|_| panic!("{label}/{name}@{bg}: компонента не число: {part} в {css}"));
        assert!(
            v.is_finite(),
            "{label}/{name}@{bg}: не-конечная компонента {css}"
        );
    }
    if let Some(a) = alpha {
        let av: f64 = a
            .parse()
            .unwrap_or_else(|_| panic!("{label}/{name}@{bg}: α не число: {a} в {css}"));
        assert!(av.is_finite(), "{label}/{name}@{bg}: не-конечная α {css}");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// СТРУКТУРНЫЕ ВЫРОЖДЕНИЯ
// ─────────────────────────────────────────────────────────────────────────────

/// РОВНО одна роль (текст-якорь без пола и без hue) на пустой палитре/сентименте:
/// самый тонкий контракт, что вообще эмитит цвет. Компилируется, тотален, эмитит
/// одну роль. Закрывает «минимальный ненулевой контракт роняет резолв».
#[test]
fn single_role_config_is_total() {
    let cfg = scaffold(
        plain_neutral(),
        plain_sentiments(),
        vec![],
        vec![(
            "solo".to_string(),
            RoleRecipe::TextAnchor {
                fraction: 0.5,
                floor: Floor::None,
                hue: None,
            },
        )],
    );
    let table = cfg
        .compile_named_role_table()
        .expect("одноролевой конфиг валиден ⇒ компилируется");
    assert_eq!(table.entries().len(), 1, "ровно одна роль в контракте");
    // Контракт заявляет эмиссию: TextAnchor 0.5 без пола обязан РЕШИТЬСЯ в цвет
    // хотя бы на части свипа — иначе ассерты эмитируемости не сработали.
    let reachable = assert_table_is_total(&table, "single-role");
    assert!(
        reachable > 0,
        "single-role: роль не эмитила цвет ни разу — тест вакуумен"
    );
}

/// НОЛЬ ролей на уровне ЯДРА (не границы, что его отклоняет): `compile` не
/// паникует — либо `Err`, либо пустая таблица, чей `resolve_named_set` тотален
/// (пустой набор). Закрывает «пустой словарь ролей роняет ядро».
#[test]
fn zero_role_config_is_meaningful_not_panic() {
    let cfg = scaffold(plain_neutral(), plain_sentiments(), vec![], vec![]);
    match cfg.compile_named_role_table() {
        Ok(table) => {
            assert!(table.entries().is_empty(), "нет ролей ⇒ пустая таблица");
            // Тотальность на пустом контракте: резолв не паникует и даёт пусто.
            let bg = BgInput::solid("#FFFFFF").unwrap();
            let set = resolve_named_set(&bg, &table, &ViewingConditions::srgb());
            assert!(set.is_empty(), "пустой контракт ⇒ пустой набор");
        }
        Err(_) => { /* честный отказ — тоже валидный, не-паникующий исход */
        }
    }
}

/// Экстремальные ручки НЕЙТРАЛИ (ratio 0.0 и 1.0, огромный target_mp, жёсткость
/// у краёв): каждая комбинация либо честно `Err`, либо тотальна. Ловит деление
/// на ноль / переполнение в кривой нейтрали на вырожденных ручках.
#[test]
fn extreme_neutral_tint_knobs_never_panic_or_emit_nonfinite() {
    let knob_sets = [
        (0.0, 5.0, 8.0),     // нулевой тинт
        (1.0, 5.0, 8.0),     // полный тинт
        (0.10, 200.0, 8.0),  // недостижимо большой target_mp
        (0.10, 0.0, 0.0),    // нулевые target_mp и жёсткость
        (0.10, 5.0, 1000.0), // экстремальная жёсткость (жёсткая стена)
    ];
    let ladder_role = (
        "neutral-fill".to_string(),
        RoleRecipe::Ladder {
            source: LadderSource::Neutral(NeutralPick::Mid),
            position: LadderPosition::NeutralFillPrimary,
            floor: None,
        },
    );
    for (ratio, target_mp, hue_stiffness) in knob_sets {
        let neutral = NeutralConfig {
            anchors: NeutralAnchors {
                light: "#FBFBFD".to_string(),
                mid: "#8A8A8E".to_string(),
                dark: "#101012".to_string(),
            },
            tint: NeutralTint {
                ratio,
                target_mp,
                hue_stiffness,
                hue_override_deg: None,
            },
            edge: None,
            inverted: None,
        };
        let cfg = scaffold(
            neutral,
            plain_sentiments(),
            vec![],
            vec![ladder_role.clone()],
        );
        if let Ok(table) = cfg.compile_named_role_table() {
            // Чистый гард тотальности/не-паники: достижимость на экстремальных
            // ручках не обязательна (роль может честно уйти в Unreachable).
            let _ = assert_table_is_total(
                &table,
                &format!("neutral(r={ratio},mp={target_mp},k={hue_stiffness})"),
            );
        }
    }
}

/// Сентимент на вырожденной одноцветной палитре не роняет резолв нечислом.
#[test]
fn sentiment_anchor_never_panics_or_emits_nonfinite() {
    let family = PaletteFamily {
        key: "fam".to_string(),
        anchors: flat_anchors("#FF3B30"),
    };
    let sentiments = SentimentsConfig {
        categories: vec![SentimentCategory {
            name: "alert".to_string(),
            family: "fam".to_string(),
        }],
    };
    {
        let cfg = scaffold(
            plain_neutral(),
            sentiments,
            vec![family.clone()],
            vec![(
                "alert-fill".to_string(),
                RoleRecipe::Ladder {
                    source: LadderSource::Sentiment("alert".to_string()),
                    position: LadderPosition::FillPrimary,
                    floor: None,
                },
            )],
        );
        if let Ok(table) = cfg.compile_named_role_table() {
            // Чистый гард тотальности/не-паники (как neutral выше).
            let _ = assert_table_is_total(&table, "sentiment(anchor-v2)");
        }
    }
}

/// Чисто чёрный и чисто белый БРЕНД (нулевая хрома, край гамута): роль лестницы
/// бренда либо честно недостижима, либо решается — но не паникует и не эмитит NaN.
/// Ловит atan2-неопределённость оттенка у ахроматического якоря на пути эмиссии.
#[test]
fn achromatic_extreme_brand_never_panics() {
    for brand_hex in ["#000000", "#FFFFFF"] {
        let cfg = ThemeConfig::new(
            Brand {
                anchors: flat_anchors(brand_hex),
            },
            plain_neutral(),
            vec![],
            plain_sentiments(),
            ThemesConfig {
                entries: vec![
                    ("light".to_string(), VcPreset::Srgb),
                    ("dark".to_string(), VcPreset::Dim),
                ],
            },
            vec![
                (
                    "brand-fill".to_string(),
                    RoleRecipe::Ladder {
                        source: LadderSource::Brand,
                        position: LadderPosition::FillPrimary,
                        floor: None,
                    },
                ),
                (
                    "brand-label".to_string(),
                    RoleRecipe::TextAnchor {
                        fraction: 0.9,
                        floor: Floor::AaText,
                        hue: Some(LadderSource::Brand),
                    },
                ),
            ],
            vec![],
        );
        if let Ok(table) = cfg.compile_named_role_table() {
            // Ахром-бренд обязан РЕАЛЬНО эмитить (не только не паниковать): чёрный
            // лейбл на белом / белый на чёрном достижим — свип проходит путь
            // эмиссии на ахром-якоре (atan2-край), а не только его отсутствие.
            let reachable =
                assert_table_is_total(&table, &format!("achromatic-brand({brand_hex})"));
            assert!(
                reachable > 0,
                "achromatic-brand({brand_hex}): ни одной эмиссии на свипе — путь ахром-якоря не пройден"
            );
        }
    }
}
