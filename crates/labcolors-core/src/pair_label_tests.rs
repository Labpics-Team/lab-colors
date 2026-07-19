//! Характеризация boundary-адаптера `RoleSpec::PairLabel`.
//!
//! Класс, который закрывают эти тесты: контраст `label ↔ tinted-fill` у бейджа
//! на тинт-фоне был ЭМЕРДЖЕНТНЫМ, не гарантированным. Обычные `label-*` роли
//! решаются против ФОНА СТРАНИЦЫ и достигают своего WCAG-пола там; тинт-бейдж
//! (`labui/lab-badge.ts`, `type=tinted`) кладёт их на `fill-*-primary` (@12
//! семейный тинт) — на этой подложке контраст ниже, и для warning/success
//! «цветной» лейбл (доля 0.4757, как `label-*-tertiary`) оседает к ~2.9:1 < 3:1.
//! `PairLabel` решает оттеночный лейбл ПРОТИВ тинт-поверхности, поэтому пол
//! гарантирован по построению; при недостижимости тон клампится (флаг
//! `compressed`), а не молча выдаётся за точное выполнение контракта.
//!
//! Appearance-граф описывает только физическую цепочку заливки:
//! solid paint → opacity paint → occurrence на контексте → derived surface.
//! Downstream-солвер
//! лейбла получает эту подложку без изменения своей математики. Тесты
//! доказывают точность этой границы подложки, но не заявляют наличие
//! финального label paint/occurrence или публичного recipe-контракта.
//!
//! Четыре группы:
//!  1. `shipped_*` — реальный контракт labui (`label-<fam>-primary` на
//!     `fill-<fam>-primary`) держит UI-пол на тинте во всех темах (гвардит то,
//!     что уже отгружено, — near-black лейбл сейчас даёт 13–18:1).
//!  2. `pair_label_*` — новая роль держит пол против тинт-поверхности во всех
//!     client-defined families × light/dark (± IC).
//!  3. `pair_label_beats_page_resolved_label` — дифференциальный RED-proof:
//!     ТОТ ЖЕ контракт (доля 0.4757, `AaUi`), решённый против страницы
//!     (`label-<fam>-tertiary`), проваливает 3:1 на тинте у warning/success,
//!     а `PairLabel` (против поверхности) — держит. Разница ТОЛЬКО в подложке
//!     резолва: если бы `resolve_pair_label` целил фон страницы, тест бы упал.
//!  4. `fill_occurrence_backdrop_*` / `pair_fill_output_*` — appearance-путь
//!     подложки байт-идентичен замороженному
//!     ручному composite-oracle (матрица 5 семей × 4 режима × 6 фонов + property), включая
//!     типизированный отказ выбранной alpha-ветви; поверхность PairLabel НЕ является
//!     эмитированным PairFill (санитарный witness против ложного ребра).

use proptest::prelude::*;

use crate::config::fixture::labui_reference;
use crate::semantic::{resolve_pair_label, resolve_pair_label_manual_composite_oracle};
use crate::solve::Floor;
use crate::{
    BgInput, LadderSource, LadderTint, Resolved, RoleRecipe, RoleSpec, SolveFailure,
    ViewingConditions, resolve_named_set,
};

/// (имя семьи, источник лестницы) — 5 цветных семей тинт-бейджа labui
/// (нейтраль/статики используют `label-primary`/семейный примитив, покрыты
/// отдельно) .
fn families() -> [(&'static str, LadderSource); 5] {
    [
        ("brand", LadderSource::Brand),
        ("danger", LadderSource::Family("red".to_string())),
        ("warning", LadderSource::Family("orange".to_string())),
        ("success", LadderSource::Family("green".to_string())),
        ("info", LadderSource::Family("blue".to_string())),
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

/// Объявленный UI-пол из SSOT-контракта [`Floor::AaUi`]. Локальная
/// копия числа запрещена: тест проверяет тот же пол, который энфорсит
/// резолвер, и не дублирует его политику в фикстуре.
fn ui_floor() -> f64 {
    Floor::AaUi
        .min_ratio()
        .expect("AaUi несёт числовой юр. пол")
}

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
    let floor = ui_floor();
    let table = labui_reference().compile_named_role_table().unwrap();
    for (tname, bg_hex, vc) in themes() {
        let bg = BgInput::solid(bg_hex).unwrap();
        let set = resolve_named_set(&bg, &table, &vc)
            .expect("валидный labui-контракт обязан резолвиться");
        for (fam, _) in families() {
            let r = ratio_on_tint(&set, &format!("label-{fam}-primary"), fam);
            assert!(
                r >= floor,
                "[{tname}] отгруженный тинт-бейдж `label-{fam}-primary` на \
                 `fill-{fam}-primary` обязан держать {floor}:1, получено {r:.2}:1"
            );
        }
    }
}

// ── 2. PairLabel: жёсткий пол против тинт-поверхности, все семьи × темы ────────

#[test]
fn pair_label_clears_ui_floor_against_tinted_surface_all_families_and_themes() {
    let floor = ui_floor();
    let table = labui_with_badge_labels();
    for (tname, bg_hex, vc) in themes() {
        let bg = BgInput::solid(bg_hex).unwrap();
        let set = resolve_named_set(&bg, &table, &vc)
            .expect("валидный PairLabel-контракт обязан резолвиться");
        for (fam, _) in families() {
            let role = format!("badge-label-{fam}");
            // Решается цветом (не SolveFailure/None) и держит пол на тинте.
            let r = ratio_on_tint(&set, &role, fam);
            assert!(
                r >= floor,
                "[{tname}] `{role}` обязан держать {floor}:1 против тинт-поверхности \
                 `fill-{fam}-primary`, получено {r:.2}:1"
            );
        }
    }
}

/// Лейбл тинт-бейджа эмитит другой байтовый цвет, чем основной label семьи.
#[test]
fn pair_label_bytes_differ_from_primary_label() {
    let table = labui_with_badge_labels();
    let bg = BgInput::solid("#FFFFFF").unwrap();
    let set = resolve_named_set(&bg, &table, &ViewingConditions::srgb())
        .expect("валидный PairLabel-контракт обязан резолвиться");
    for (fam, _) in families() {
        let role = format!("badge-label-{fam}");
        let res = &set.iter().find(|(n, _)| n == &role).unwrap().1;
        let Resolved::Color { solved, .. } = res else {
            panic!("`{role}` обязан решиться цветом");
        };
        // Здесь проверяется только различие представления; перцептивный порог из
        // одного `assert_ne!` не выводится.
        let primary = solid_hex(&set, &format!("label-{fam}-primary"));
        assert_ne!(
            solved.hex(),
            primary,
            "`{role}` не должен совпасть байт-в-байт с `label-{fam}-primary`"
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
    let floor = ui_floor();
    let table = labui_with_badge_labels();
    let bg = BgInput::solid("#FFFFFF").unwrap();
    let set = resolve_named_set(&bg, &table, &ViewingConditions::srgb())
        .expect("валидный PairLabel-контракт обязан резолвиться");

    // warning/success — семьи, где предпосылку о провале страничного
    // `label-*-tertiary` на собственном тинте проверяет первый assert ниже.
    for fam in ["warning", "success"] {
        let before = ratio_on_tint(&set, &format!("label-{fam}-tertiary"), fam);
        let after = ratio_on_tint(&set, &format!("badge-label-{fam}"), fam);
        assert!(
            before < floor,
            "предпосылка класса: страничный `label-{fam}-tertiary` обязан \
             проваливать {floor}:1 на тинте (иначе тест не о том), получено {before:.2}:1"
        );
        assert!(
            after >= floor,
            "`badge-label-{fam}` (тот же контракт, но против поверхности) обязан \
             держать {floor}:1, получено {after:.2}:1"
        );
        assert!(
            after > before,
            "резолв против поверхности обязан ПОДНЯТЬ контраст на тинте: \
             {fam} before={before:.2} after={after:.2}"
        );
    }
}

// ── 4. Fill occurrence-derived backdrop == замороженный oracle ──

/// Скомпилированные параметры `RoleSpec::PairLabel` конкретной роли таблицы —
/// вход обоих путей differential-а (appearance-путь и ручной oracle получают
/// РОВНО одни аргументы, различие только в реализации).
fn pair_label_spec(
    table: &crate::NamedRoleTable,
    role: &str,
) -> (LadderTint, f64, Floor, f64, f64) {
    let spec = table
        .entries()
        .iter()
        .find(|(name, _)| name == role)
        .map(|(_, spec)| spec)
        .unwrap_or_else(|| panic!("роль `{role}` обязана существовать в таблице"));
    match spec {
        RoleSpec::PairLabel {
            tint,
            fraction,
            floor,
            surface_alpha_light,
            surface_alpha_dark,
        } => (
            *tint,
            *fraction,
            *floor,
            *surface_alpha_light,
            *surface_alpha_dark,
        ),
        other => panic!("`{role}` обязан компилироваться в PairLabel, получено {other:?}"),
    }
}

/// Шесть контекстных фонов differential-матрицы — coverage-выборка с
/// подписанным происхождением каждой точки, НЕ production-правило:
///
/// * `#000000` / `#FFFFFF` — границы sRGB-куба; белый одновременно является
///   отгруженным фоном светлых labui-тем (классы совпадают в этой дизайн-
///   системе по факту фикстуры);
/// * `#101012` — отгруженный фон тёмных labui-тем (фикстура);
/// * `#767676` — опубликованная WCAG-граница серого (≈4.54:1 к белому),
///   напрямую закреплённая
///   `tests/reference_vectors.rs::wcag_published_ratios_via_public_api`;
/// * `#FFF4E0` — хроматический светлый witness: точная warning-поверхность
///   из graph-тестов (`appearance_graph_tests`);
/// * `#0000FF` — насыщенный хроматический угол куба (sRGB primary).
fn differential_backgrounds() -> [&'static str; 6] {
    [
        "#000000", "#101012", "#767676", "#FFF4E0", "#0000FF", "#FFFFFF",
    ]
}

/// Appearance-путь вычисляет fill occurrence и derived backdrop, после чего
/// неизменённый downstream-солвер обязан дать тот же `Resolved`, что и
/// замороженный oracle: вариант,
/// финальные байты, флаги, unreachable-причины. Никакого approximate equality:
/// `assert_eq!` по `PartialEq` сравнивает и все числовые поля (одинаковые биты
/// по построению одного downstream-солвера), а hex сверяется отдельно, чтобы
/// байтовая эмиссия оставалась закреплённой даже при эволюции `PartialEq`.
/// Это differential-доказательство границы подложки, а не наличия финального
/// label occurrence в appearance-графе.
#[test]
fn fill_occurrence_backdrop_matrix_matches_frozen_pair_label_oracle_exactly() {
    let table = labui_with_badge_labels();
    let mut resolved_hits = 0usize;
    for (tname, _, vc) in themes() {
        for bg_hex in differential_backgrounds() {
            let bg = BgInput::solid(bg_hex).unwrap();
            for (fam, _) in families() {
                let role = format!("badge-label-{fam}");
                let (tint, fraction, floor, alpha_light, alpha_dark) =
                    pair_label_spec(&table, &role);
                let production =
                    resolve_pair_label(&bg, tint, fraction, floor, alpha_light, alpha_dark, &vc);
                let oracle = resolve_pair_label_manual_composite_oracle(
                    &bg,
                    tint,
                    fraction,
                    floor,
                    alpha_light,
                    alpha_dark,
                    &vc,
                );
                assert_eq!(
                    production, oracle,
                    "[{tname}/{bg_hex}] `{role}`: production-граф обязан быть \
                     идентичен замороженному ручному composite-oracle по всем полям"
                );
                for outcome in [&production, &oracle] {
                    assert!(
                        !matches!(
                            outcome,
                            Err(SolveFailure::InvalidInput(_)
                                | SolveFailure::GamutUnsupported
                                | SolveFailure::InternalInvariant(_))
                        ),
                        "[{tname}/{bg_hex}] `{role}`: a valid sRGB fixture produced a non-physical error: {outcome:?}"
                    );
                }
                if let (
                    Ok(Resolved::Color { solved: p, .. }),
                    Ok(Resolved::Color { solved: o, .. }),
                ) = (&production, &oracle)
                {
                    resolved_hits += 1;
                    assert_eq!(
                        p.hex(),
                        o.hex(),
                        "[{tname}/{bg_hex}] `{role}`: финальные байты эмиссии"
                    );
                }
            }
        }
    }
    assert!(
        resolved_hits > 0,
        "differential matrix must exercise successful colour resolution"
    );
}

/// Внутренний resolver обязан типизированно отклонять невалидную выбранную
/// альфу до начала физического поиска. Публичный `NamedRoleTable` дополнительно
/// валидирует обе theme-ветви при компиляции конфига.
#[test]
fn fill_occurrence_backdrop_path_rejects_selected_invalid_alpha_exactly() {
    let table = labui_with_badge_labels();
    let (tint, fraction, floor, _, _) = pair_label_spec(&table, "badge-label-warning");
    for bad_alpha in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, 1.5] {
        for (bg_hex, vc, light, dark) in [
            ("#FFFFFF", ViewingConditions::srgb(), bad_alpha, 0.122),
            (
                "#101012",
                ViewingConditions::dim_surround(),
                0.122,
                bad_alpha,
            ),
        ] {
            let bg = BgInput::solid(bg_hex).unwrap();
            let production = resolve_pair_label(&bg, tint, fraction, floor, light, dark, &vc);
            let oracle = resolve_pair_label_manual_composite_oracle(
                &bg, tint, fraction, floor, light, dark, &vc,
            );
            assert_eq!(
                production, oracle,
                "theme-selected alpha branch must stay exact for alpha={bad_alpha}"
            );
            assert!(matches!(production, Err(SolveFailure::InvalidInput(_))));
        }
    }
}

/// Санитарный witness против ложного ребра `PairFill → PairLabel`: эмитированный
/// `PairFill` — отдельно
/// сдвинутый солид, он НЕ равен тинт-поверхности (композиту `fill-*-primary`),
/// против которой решается `PairLabel`. Подмена derived backdrop эмитированным
/// PairFill разойдётся с differential-матрицей именно потому, что эти значения
/// различны.
#[test]
fn pair_fill_output_differs_from_fill_occurrence_derived_backdrop() {
    let mut cfg = labui_reference();
    for (fam, source) in families() {
        cfg.roles
            .push((format!("pair-fill-{fam}"), RoleRecipe::PairFill { source }));
    }
    let table = cfg
        .compile_named_role_table()
        .expect("labui + pair-fill компилируется");
    let bg = BgInput::solid("#FFFFFF").unwrap();
    let set = resolve_named_set(&bg, &table, &ViewingConditions::srgb())
        .expect("валидный PairFill-контракт обязан резолвиться");
    for (fam, _) in families() {
        let pair_fill_composite = set
            .iter()
            .find(|(name, _)| name == &format!("pair-fill-{fam}"))
            .and_then(|(_, resolved)| resolved.translucent())
            .map(|t| t.composite_hex().to_string())
            .unwrap_or_else(|| panic!("pair-fill-{fam} обязан решиться"));
        let surface = surface_hex(&set, fam);
        assert_ne!(
            pair_fill_composite, surface,
            "{fam}: эмитированный PairFill не является поверхностью PairLabel"
        );
    }
}

// Property-differential: произвольные источник/контекст/альфы/доля/пол/режим
// проверяют точную замену пути подложки. Downstream-солвер в обоих путях
// один и тот же.
proptest! {
    #[test]
    fn fill_occurrence_backdrop_property_matches_frozen_pair_label_oracle(
        source in any::<[u8; 3]>(),
        context in any::<[u8; 3]>(),
        alpha_light in 0.0f64..=1.0,
        alpha_dark in 0.0f64..=1.0,
        fraction in 0.01f64..1.0,
        floor_pick in 0usize..3,
        theme_pick in 0usize..4,
    ) {
        let encoded = source.map(|channel| f64::from(channel) / 255.0);
        let tint = LadderTint::new([encoded; 4]).expect("byte/255 всегда в домене");
        let context_hex = format!(
            "#{:02X}{:02X}{:02X}",
            context[0], context[1], context[2]
        );
        let bg = BgInput::solid(&context_hex).expect("байтовый hex валиден");
        let floor = [Floor::AaText, Floor::AaUi, Floor::None][floor_pick];
        let (_, _, vc) = themes()[theme_pick];
        let production = resolve_pair_label(
            &bg, tint, fraction, floor, alpha_light, alpha_dark, &vc,
        );
        let oracle = resolve_pair_label_manual_composite_oracle(
            &bg, tint, fraction, floor, alpha_light, alpha_dark, &vc,
        );
        prop_assert_eq!(production, oracle);
    }
}
