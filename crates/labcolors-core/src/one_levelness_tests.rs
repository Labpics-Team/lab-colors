//! Гейт одноуровневости цветных лейблов (ратификация ch5c, M1 — класс Б).
//!
//! КЛАСС ДЕФЕКТА, который закрывает гейт: цветной `label-<family>-<level>` несёт
//! НЕ ТОТ контракт читаемости, что нейтральный лейбл того же уровня. До
//! ратификации это была α-рампа @72/@52/@32 поверх тинта семьи — 40/40
//! нарушений одноуровневости (danger-S light 38.0 Lc против neutral-S 66.4;
//! light-тема нелегальна до 1.69:1). См. scratchpad/ch5c-ratification.md §2.
//!
//! ИНВАРИАНТ (мандат владельца 2026-07-03 «одноуровневое одноуровнево»):
//! для каждой темы × уровня × семьи
//!   | |Lc(label-<family>-L)| − |Lc(label-neutral-L)| | ≤ TOL
//! ЛИБО роль честно несёт флаг деградации (`compressed` / `hue_vanished`) — тогда
//! контракт занят ближайшим изученным, а не молча провален. Плюс ratio-полы уровня
//! (AA text 4.5 / 4.5 / AA UI 3.0 / —) держатся у цветного лейбла как у нейтрали
//! (полы НЕТОРГУЕМЫ).
//!
//! TOL = 5.0 Lc — DECLARED инженерный допуск (квантование + разница путей
//! резолва нейтрали и оттенка), ратифицирован в ch5c-ratification-build.md.

use crate::config::fixture::labui_reference;
use crate::semantic::{NamedRoleTable, resolve_named_set};
use crate::{BgInput, Resolved, ViewingConditions};

/// Инженерный допуск одноуровневости (Lc).
const TOL: f64 = 5.0;

/// Семьи, чьи лейблы обязаны быть одноуровневы с нейтралью.
const FAMILIES: &[&str] = &["brand", "danger", "warning", "success", "info"];

/// (уровень, имя нейтральной роли, ratio-пол или `None`).
const LEVELS: &[(&str, &str, Option<f64>)] = &[
    ("primary", "label-primary", Some(4.5)),
    ("secondary", "label-secondary", Some(4.5)),
    ("tertiary", "label-tertiary", Some(3.0)),
    ("quaternary", "label-quaternary", None),
];

/// Тема: имя, фон резолва, условия просмотра.
struct Theme {
    name: &'static str,
    bg: &'static str,
    vc: ViewingConditions,
}

fn themes() -> Vec<Theme> {
    vec![
        Theme {
            name: "light",
            bg: "#FFFFFF",
            vc: ViewingConditions::srgb(),
        },
        Theme {
            name: "dark",
            bg: "#101012",
            vc: ViewingConditions::dim_surround(),
        },
    ]
}

/// |Lc| решённой роли по имени в наборе (Color или Translucent-композит).
fn abs_lc(set: &[(String, Resolved)], role: &str) -> f64 {
    set.iter()
        .find(|(n, _)| n == role)
        .and_then(|(_, r)| r.lc())
        .unwrap_or_else(|| panic!("роль `{role}` обязана решиться в контраст-исход"))
        .abs()
}

/// Роль по имени.
fn role<'a>(set: &'a [(String, Resolved)], name: &str) -> &'a Resolved {
    &set.iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("роль `{name}` есть в наборе"))
        .1
}

/// WCAG-отношение контраст-исхода: солид (Color) — против фона; полупрозрачный
/// (Translucent, напр. ladder-роль в RED-proof) — его КОМПОЗИТ против фона. Так
/// проверка ratio-пола применима к обоим рецептам, а не только к ратифицированному.
fn role_wcag(res: &Resolved) -> Option<f64> {
    if let Some(s) = res.solved() {
        Some(s.wcag_ratio())
    } else {
        res.translucent().map(|t| t.composite_wcag())
    }
}

/// Собрать нарушения гейта одноуровневости на скомпилированной таблице. Пустой
/// вектор = гейт зелёный. Каждая строка — человеко-читаемое нарушение с числами.
fn one_levelness_violations(table: &NamedRoleTable) -> Vec<String> {
    let mut out = Vec::new();
    for theme in themes() {
        let bg = BgInput::solid(theme.bg).expect("валидный фон темы");
        let set = resolve_named_set(&bg, table, &theme.vc);
        for (level, neutral_role, floor) in LEVELS {
            let neutral_lc = abs_lc(&set, neutral_role);
            for family in FAMILIES {
                let fam_role_name = format!("label-{family}-{level}");
                let fam = role(&set, &fam_role_name);
                let fam_lc = abs_lc(&set, &fam_role_name);
                let delta = (fam_lc - neutral_lc).abs();
                let honest = fam.compressed() || fam.hue_vanished();

                // 1. Одноуровневость: |ΔLc| ≤ TOL ИЛИ честный флаг деградации.
                if delta > TOL && !honest {
                    out.push(format!(
                        "{}/{fam_role_name}: |Lc| {fam_lc:.1} против нейтрали {neutral_lc:.1} \
                         (Δ {delta:.1} > {TOL}) без флага compressed/hue-vanished",
                        theme.name
                    ));
                }

                // 2. Ratio-пол уровня НЕТОРГУЕМ: держится у цветного как у нейтрали.
                if let Some(min_ratio) = floor {
                    let wcag = role_wcag(fam).unwrap_or_else(|| {
                        panic!("{fam_role_name}: контраст-исход обязан нести WCAG")
                    });
                    if wcag + 1e-9 < *min_ratio {
                        out.push(format!(
                            "{}/{fam_role_name}: WCAG {wcag:.2} < ratio-пол {min_ratio} — \
                             пол уровня пробит",
                            theme.name
                        ));
                    }
                }
            }
        }
    }
    out
}

/// GREEN: на ратифицированной фикстуре labui гейт одноуровневости держится —
/// каждый цветной лейбл несёт контракт своего уровня в оттенке семьи (Δ ≤ TOL)
/// или честно флагирован, и ratio-полы уровней стоят.
#[test]
fn one_levelness_holds_on_labui_reference() {
    let table = labui_reference()
        .compile_named_role_table()
        .expect("фикстура labui компилируется");
    let violations = one_levelness_violations(&table);
    assert!(
        violations.is_empty(),
        "гейт одноуровневости обязан быть зелёным на фикстуре, нарушения:\n{}",
        violations.join("\n")
    );
}

/// RED-proof: возврат ОДНОЙ роли к прежнему ladder-рецепту (α-рампа поверх тинта
/// семьи) роняет гейт с конкретными числами. Доказывает, что гейт кусается —
/// зелёный-с-рождения был бы багом (тест-театр).
///
/// `label-danger-secondary` → `Ladder(sent danger, LabelSecondary)`: на светлой
/// теме композит @72 даёт |Lc| ~38 против нейтрали ~66 (Δ ~28 ≫ TOL), и роль
/// становится Translucent — у неё НЕТ флага compressed/hue-vanished, то есть это
/// молчаливая деградация, ровно тот класс, что гейт обязан ловить.
#[test]
fn red_proof_ladder_recipe_breaks_one_levelness() {
    use crate::LadderPosition;
    use crate::config::{LadderSource, RoleRecipe};

    // Сначала докажем, что БЕЗ мутации гейт зелёный — иначе RED-proof доказывал
    // бы не то (splice обязан флипать green→red, а не red→red).
    let green = labui_reference().compile_named_role_table().unwrap();
    assert!(
        one_levelness_violations(&green).is_empty(),
        "предусловие RED-proof: реальная фикстура зелёная"
    );

    // Мутация: одна роль возвращается к ladder-рецепту.
    let mut cfg = labui_reference();
    for (name, recipe) in &mut cfg.roles {
        if name == "label-danger-secondary" {
            *recipe = RoleRecipe::Ladder {
                source: LadderSource::Sentiment("danger".to_string()),
                position: LadderPosition::LabelSecondary,
                floor: None,
            };
        }
    }
    let mutated = cfg
        .compile_named_role_table()
        .expect("мутант валиден (ladder-рецепт легален структурно)");
    let violations = one_levelness_violations(&mutated);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("light/label-danger-secondary")),
        "ladder-рецепт обязан уронить гейт одноуровневости на light/label-danger-secondary, \
         нарушения:\n{}",
        violations.join("\n")
    );
}

/// Closed-form differential (Класс В, Фаулер): УНИФИЦИРОВАННАЯ деривация
/// акцентного **ФОНА** берёт СТЕНУ ГАМУТА на КАЖДОМ интерьерном уровне светлоты —
/// не долю. Для 9 уровней × 72 тона (шаг 5°) хрома, которую вернул путь
/// `derive_accent_surface_ramp → accent_balanced`, РАВНА `max_chroma(L, hue)`, а
/// эмитированный цвет ступени РАВЕН выходу примитива баланса на той же `(L, hue)`.
///
/// Это DRY-контракт слайса: второй рецепт `chroma_fraction × max_chroma` СНЯТ,
/// и деривация фона теперь ездит на едином агностичном примитиве. Любой множитель
/// `< 1` (например мутация центра примитива к `0.5 × max_chroma`) немедленно
/// роняет равенство — RED-proof кусается.
///
/// `EPS = 1e-9` (провенанс): `c_ok` ступени и эталон `max_chroma(L, hue)` зовут
/// ОДНУ функцию стены гамута (accent_balanced c_ok = max_chroma по построению),
/// поэтому истинное расхождение либо строго ноль, либо макроскопично (доля стены);
/// «шума в младших битах» тут физически нет — обе величины из единой геометрии.
#[test]
fn accent_surface_derivation_rides_gamut_wall_closed_form() {
    use crate::accent_balance::accent_balanced;
    use crate::derive_accent_surface_ramp;
    use crate::neutral::{CurveParams, NeutralCurve};
    use crate::scale::{jp_to_oklab_l, max_chroma};

    const EPS: f64 = 1e-9;
    // 9 интерьерных уровней светлоты (нейтральный скелет из 9 ступеней).
    const TS: [f64; 9] = [0.10, 0.20, 0.30, 0.40, 0.50, 0.60, 0.70, 0.80, 0.90];

    for theme in themes() {
        let curve = NeutralCurve::with_vc(
            "#FFFFFF",
            "#787880",
            "#101012",
            &CurveParams::default(),
            &theme.vc,
        )
        .expect("нейтральная кривая строится");
        let neutral: Vec<_> = TS.iter().map(|&t| curve.at(t)).collect();

        for h_step in 0..72 {
            let hue = f64::from(h_step) * 5.0;
            let ramp = derive_accent_surface_ramp(&neutral, hue, &theme.vc);
            assert_eq!(ramp.len(), neutral.len());
            for (lvl, surface) in ramp.iter().enumerate() {
                // Светлота уровня наследуется из нейтрали (одноуровневость).
                let l_ok = jp_to_oklab_l(neutral[lvl].jp, &theme.vc);
                let wall = max_chroma(l_ok, hue);
                // ЕДИНЫЙ баланс: хрома ступени == стена гамута (не доля).
                assert!(
                    (surface.c_ok - wall).abs() < EPS,
                    "{}: L={l_ok:.4} h={hue}° уровень {lvl}: c_ok={} != стена {wall} \
                     (рецидив доли chroma_fraction?)",
                    theme.name,
                    surface.c_ok
                );
                // Провенанс DRY: ступень фона — РОВНО выход accent_balanced(L, hue).
                assert_eq!(
                    surface,
                    &accent_balanced(l_ok, hue, &theme.vc),
                    "{}: L={l_ok:.4} h={hue}° уровень {lvl}: surface-ступень обязана \
                     быть выходом единого примитива баланса (DRY)",
                    theme.name
                );
            }
        }
    }
}
