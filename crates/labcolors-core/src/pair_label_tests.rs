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
//! `compressed`), а не молча выдаётся за точное выполнение контракта.
//!
//! Четыре группы:
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
//!  4. `migration_*` / `emitted_pair_fill_*` — differential миграции #307:
//!     production-путь через appearance-граф байт-идентичен замороженному
//!     legacy oracle (матрица 5 семей × 4 режима × 6 фонов + property), включая
//!     публичные типизированные отказы; поверхность PairLabel НЕ является
//!     эмитированным PairFill (санитарный witness против ложного ребра, #305).

use proptest::prelude::*;

use crate::config::fixture::labui_reference;
use crate::semantic::{resolve_pair_label, resolve_pair_label_legacy_oracle};
use crate::solve::Floor;
use crate::{
    BgInput, LadderSource, LadderTint, Resolved, RoleRecipe, RoleSpec, Unreachable,
    ViewingConditions, resolve_named_set,
};

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

/// Юр. пол UI (WCAG 1.4.11, 3:1) из SSOT контракта [`Floor::AaUi`] — локальная
/// копия числа запрещена (#307): тест обязан проверять тот же пол, который
/// энфорсит резолвер. Консервативный дефолт порога тинт-бейджа (короткая
/// пилюля-индикатор — UI-объект, не длинный текст; 4.5:1 на светлом тинте
/// вынудил бы near-black и убил бы «цветной» вид). Порог 3:1 vs 4.5:1 —
/// открытый вопрос владельцу (см. отчёт task #29).
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
        let set = resolve_named_set(&bg, &table, &vc);
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
        let set = resolve_named_set(&bg, &table, &vc);
        for (fam, _) in families() {
            let role = format!("badge-label-{fam}");
            // Решается цветом (не Unreachable/None) и держит пол на тинте.
            let r = ratio_on_tint(&set, &role, fam);
            assert!(
                r >= floor,
                "[{tname}] `{role}` обязан держать {floor}:1 против тинт-поверхности \
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
    let floor = ui_floor();
    let table = labui_with_badge_labels();
    let bg = BgInput::solid("#FFFFFF").unwrap();
    let set = resolve_named_set(&bg, &table, &ViewingConditions::srgb());

    // warning/success — семьи, где страничный `label-*-tertiary` проваливает 3:1
    // на собственном тинте (замер task #29: warning 2.76, success 2.88).
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

// ── 4. Differential-матрица миграции #307: граф == замороженный legacy oracle ──

/// Скомпилированные параметры `RoleSpec::PairLabel` конкретной роли таблицы —
/// вход обоих путей differential-а (production-граф и legacy oracle получают
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
fn migration_backgrounds() -> [&'static str; 6] {
    [
        "#000000", "#101012", "#767676", "#FFF4E0", "#0000FF", "#FFFFFF",
    ]
}

/// Полный differential §8.3 ТЗ #307: production-путь (appearance-граф) обязан
/// быть РАВЕН замороженному legacy oracle по всем полям `Resolved` — вариант,
/// финальные байты, флаги, unreachable-причины. Никакого approximate equality:
/// `assert_eq!` по `PartialEq` сравнивает и все числовые поля (одинаковые биты
/// по построению одного downstream-солвера), а hex сверяется отдельно, чтобы
/// байтовая эмиссия оставалась закреплённой даже при эволюции `PartialEq`.
#[test]
fn migration_differential_matrix_matches_frozen_legacy_oracle_exactly() {
    let table = labui_with_badge_labels();
    for (tname, _, vc) in themes() {
        for bg_hex in migration_backgrounds() {
            let bg = BgInput::solid(bg_hex).unwrap();
            for (fam, _) in families() {
                let role = format!("badge-label-{fam}");
                let (tint, fraction, floor, alpha_light, alpha_dark) =
                    pair_label_spec(&table, &role);
                let production =
                    resolve_pair_label(&bg, tint, fraction, floor, alpha_light, alpha_dark, &vc);
                let oracle = resolve_pair_label_legacy_oracle(
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
                     идентичен замороженному legacy oracle по всем полям"
                );
                if let (Resolved::Color { solved: p, .. }, Resolved::Color { solved: o, .. }) =
                    (&production, &oracle)
                {
                    assert_eq!(
                        p.hex(),
                        o.hex(),
                        "[{tname}/{bg_hex}] `{role}`: финальные байты эмиссии"
                    );
                }
            }
        }
    }
}

/// Публичные исходы невалидной альфы (RoleSpec публичен — спека, собранная в
/// обход валидатора конфига, обязана давать ПРЕЖНИЙ типизированный отказ):
/// вариант, причина и точный текст заморожены миграцией байт-в-байт.
#[test]
fn migration_preserves_public_invalid_alpha_outcomes_exactly() {
    let table = labui_with_badge_labels();
    let (tint, fraction, floor, _, _) = pair_label_spec(&table, "badge-label-warning");
    let bg = BgInput::solid("#FFFFFF").unwrap();
    let vc = ViewingConditions::srgb();
    for bad_alpha in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, 1.5] {
        let production = resolve_pair_label(&bg, tint, fraction, floor, bad_alpha, bad_alpha, &vc);
        let oracle =
            resolve_pair_label_legacy_oracle(&bg, tint, fraction, floor, bad_alpha, bad_alpha, &vc);
        assert_eq!(
            production, oracle,
            "публичный тип/текст отказа по α={bad_alpha} обязан быть заморожен"
        );
        assert!(
            matches!(
                production,
                Resolved::Unreachable(Unreachable::InvalidInput(_))
            ),
            "невалидная α обязана давать типизированный InvalidInput, не панику/кламп"
        );
    }
}

/// Санитарный witness против ложного ребра `PairFill → PairLabel` (regression
/// witness #305 остаётся честным): эмитированный `PairFill` — отдельно
/// сдвинутый солид, он НЕ равен тинт-поверхности (композиту `fill-*-primary`),
/// против которой решается `PairLabel`. Если миграция когда-либо подменит
/// derived backdrop эмитированным PairFill, differential-матрица разойдётся
/// именно потому, что эти значения различны — что и закрепляет этот тест.
#[test]
fn emitted_pair_fill_differs_from_the_pair_label_surface() {
    let mut cfg = labui_reference();
    for (fam, source) in families() {
        cfg.roles
            .push((format!("pair-fill-{fam}"), RoleRecipe::PairFill { source }));
    }
    let table = cfg
        .compile_named_role_table()
        .expect("labui + pair-fill компилируется");
    let bg = BgInput::solid("#FFFFFF").unwrap();
    let set = resolve_named_set(&bg, &table, &ViewingConditions::srgb());
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

// Property-differential: произвольные источник/контекст/альфы/доля/пол/режим.
// Единственный источник различий между путями — сама миграция; любые входы в
// объявленном домене обязаны давать идентичный Resolved.
proptest! {
    #[test]
    fn migration_differential_property_holds_on_arbitrary_inputs(
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
        let oracle = resolve_pair_label_legacy_oracle(
            &bg, tint, fraction, floor, alpha_light, alpha_dark, &vc,
        );
        prop_assert_eq!(production, oracle);
    }
}
