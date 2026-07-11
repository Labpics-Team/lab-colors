//! Проекция результата резолва в JSON-строку — «широкая» часть JS↔wasm моста.
//!
//! ПОЧЕМУ строка, а не пообъектная сборка: профилирование границы
//! (`bench/wasm-boundary.bench.mjs`) показало, что `resolveTheme` даже на
//! кэш-хите тратит сотни микросекунд в проекции — ~106 role-объектов × ~10
//! свойств ≈ тысяча FFI-вызовов `Reflect::set`, каждый с маршалингом ключа.
//! Одна UTF-8 строка через границу + нативный `JSON.parse` на стороне JS
//! заменяет их все одним переходом (адаптер — в `lib.rs`).
//!
//! Байт-идентичность старой пообъектной проекции — по построению:
//! - числа пишутся кратчайшей десятичной записью, однозначно
//!   восстанавливающей double (гарантия `Display` для `f64` в std);
//!   `JSON.parse` обязан вернуть ближайший double — т.е. исходные биты;
//! - порядок свойств у `JSON.parse` равен текстовому порядку ключей, и он
//!   здесь литерально повторяет порядок старых `Reflect::set`;
//! - строки уходят без потерь (экранирование обратимо), не-ASCII — как есть
//!   (граница передаёт UTF-8).
//!
//! JSON не представляет NaN/∞: по построению солвер отдаёт конечные числа,
//! а при нарушении инварианта проекция отвечает честной структурной ошибкой
//! (`internal_error`), не тихим `null`.
//!
//! Модуль framework-free (как `dto`): ни `js_sys`, ни `wasm_bindgen` — вся
//! логика тестируется нативным `cargo test`.

use std::fmt::Write as _;

use crate::dto::{ResolvedTheme, RoleOutcome};
use crate::error::BindingError;

/// Сериализовать [`ResolvedTheme`] в JSON, литерально повторяющий форму
/// `.d.ts`-контракта: `{ theme, background, vars, roles }`. Построено
/// генерически из вектора ролей — ни одна роль здесь не поименована, набор
/// растёт без правок этой функции (как и старая проекция).
pub fn resolved_json(resolved: &ResolvedTheme) -> Result<String, BindingError> {
    let mut vars = String::with_capacity(resolved.roles.len() * 72);
    let mut roles = String::with_capacity(resolved.roles.len() * 320);

    for entry in &resolved.roles {
        let css_var = format!("--lab-{}", entry.role_key);
        if !roles.is_empty() {
            roles.push(',');
        }
        push_str_lit(&mut roles, &entry.role_key);
        roles.push_str(":{\"cssVar\":");
        push_str_lit(&mut roles, &css_var);
        match &entry.outcome {
            RoleOutcome::Color(c) => {
                field_str(&mut roles, "kind", "color");
                field_str(&mut roles, "hex", &c.hex);
                field_num(&mut roles, "lc", c.lc)?;
                field_num(&mut roles, "wcagRatio", c.wcag_ratio)?;
                field_bool(&mut roles, "compressed", c.compressed);
                field_bool(&mut roles, "hueVanished", c.hue_vanished);
                field_opt_num(&mut roles, "achievedDj", c.achieved_dj)?;
                field_bool(&mut roles, "floorOverride", c.floor_override);
                field_opt_num(&mut roles, "legalFloor", c.legal_floor)?;
                // Единая форма эмиссии: oklch и для солида (hex остаётся
                // данными роли; синтаксис переменной один на все исходы).
                let css = oklch_css(&c.hex, None)?;
                field_str(&mut roles, "css", &css);
                push_var(&mut vars, &css_var, &css);
            }
            RoleOutcome::Translucent(r) => {
                field_str(&mut roles, "kind", "translucent");
                field_str(&mut roles, "tintHex", &r.tint_hex);
                field_num(&mut roles, "alpha", r.alpha)?;
                field_str(&mut roles, "compositeHex", &r.composite_hex);
                field_num(&mut roles, "compositeLc", r.composite_lc)?;
                field_num(&mut roles, "compositeWcag", r.composite_wcag)?;
                field_bool(&mut roles, "alphaCoerced", r.alpha_coerced);
                field_bool(&mut roles, "floorCoerced", r.floor_coerced);
                // Переменная несёт тинт в oklch со слэш-альфой — браузер
                // композитит на живой подложке; форма едина с солидами.
                let css = oklch_css(&r.tint_hex, Some(r.alpha))?;
                field_str(&mut roles, "css", &css);
                push_var(&mut vars, &css_var, &css);
            }
            RoleOutcome::Glow(g) => {
                // Свечение: слои для screen-наложения потребителем.
                // --lab-<role> несёт halo (единая oklch-форма), сателлиты
                // --lab-<role>-core / --lab-<role>-alpha — анатомия и
                // решённая интенсивность (число, не цвет).
                field_str(&mut roles, "kind", "glow");
                field_str(&mut roles, "coreHex", &g.core_hex);
                field_str(&mut roles, "haloHex", &g.halo_hex);
                field_num(&mut roles, "alpha", g.alpha)?;
                let canonical_alpha =
                    labcolors_core::css_alpha_value(g.alpha).map_err(|reason| {
                        BindingError::Internal {
                            reason: format!("glow alpha не сериализуется: {reason}"),
                        }
                    })?;
                if canonical_alpha != g.alpha_css {
                    return Err(BindingError::Internal {
                        reason: format!(
                            "glow alphaCss рассинхронизирован: ожидался {canonical_alpha}, получен {}",
                            g.alpha_css
                        ),
                    });
                }
                field_str(&mut roles, "alphaCss", &g.alpha_css);
                field_str(&mut roles, "compositeProfile", g.composite_profile.key());
                field_str(
                    &mut roles,
                    "compositeGuarantee",
                    g.composite_guarantee.key(),
                );
                field_opt_str(
                    &mut roles,
                    "diagnosticProfile",
                    g.diagnostic_profile.map(|profile| profile.key()),
                );
                field_str(&mut roles, "decisionProfile", g.decision_profile.key());
                roles.push_str(",\"decisionGuarantee\":{\"kind\":");
                match g.decision_guarantee {
                    labcolors_core::DecisionGuaranteeV1::BitExact => {
                        push_str_lit(&mut roles, "bit-exact");
                    }
                    labcolors_core::DecisionGuaranteeV1::OutwardIntervalV1(interval) => {
                        push_str_lit(&mut roles, "outward-interval-v1");
                        field_num(&mut roles, "lower", interval.lower())?;
                        field_num(&mut roles, "upper", interval.upper())?;
                    }
                    labcolors_core::DecisionGuaranteeV1::LegacyPlatformDependentV1 => {
                        push_str_lit(&mut roles, "legacy-platform-dependent-v1");
                    }
                    _ => {
                        return Err(BindingError::Internal {
                            reason: "проекция: неизвестный DecisionGuaranteeV1".to_string(),
                        });
                    }
                }
                roles.push('}');
                field_str(&mut roles, "constraintLayer", g.constraint_layer.key());
                field_num(&mut roles, "targetDj", g.target_dj)?;
                field_str(&mut roles, "targetStatus", g.target_status.key());
                field_str(&mut roles, "haloCompositeHex", &g.halo_composite_hex);
                field_num(&mut roles, "haloAchievedDj", g.halo_achieved_dj)?;
                field_str(&mut roles, "coreCompositeHex", &g.core_composite_hex);
                field_num(&mut roles, "coreAchievedDj", g.core_achieved_dj)?;
                // Aliases совместимости старого неоднозначного контракта.
                field_num(&mut roles, "achievedDj", g.halo_achieved_dj)?;
                field_bool(
                    &mut roles,
                    "degraded",
                    g.target_status == labcolors_core::GlowTargetStatus::Unreachable,
                );
                let halo_css = oklch_css(&g.halo_hex, None)?;
                let core_css = oklch_css(&g.core_hex, None)?;
                field_str(&mut roles, "css", &halo_css);
                push_var(&mut vars, &css_var, &halo_css);
                push_var(&mut vars, &format!("{css_var}-core"), &core_css);
                push_var(&mut vars, &format!("{css_var}-alpha"), &g.alpha_css);
            }
            RoleOutcome::GlowIndeterminate(g) => {
                field_str(&mut roles, "kind", "glow-indeterminate");
                field_str(&mut roles, "sourceHex", &g.source_hex);
                field_num(&mut roles, "targetDj", g.target_dj)?;
                field_str(&mut roles, "constraintLayer", g.constraint_layer.key());
                field_str(&mut roles, "decisionProfile", g.decision_profile.key());
                field_str(&mut roles, "numericalSiteId", g.site_id.key());
                field_str(&mut roles, "reason", g.evidence.reason_key());
                roles.push_str(",\"bounds\":{\"kind\":");
                match g.evidence {
                    labcolors_core::NumericalIndeterminacyV1::SoundBoundUnavailable => {
                        push_str_lit(&mut roles, "unavailable");
                    }
                    labcolors_core::NumericalIndeterminacyV1::IntervalOverlap(interval) => {
                        push_str_lit(&mut roles, "outward");
                        field_num(&mut roles, "lower", interval.lower())?;
                        field_num(&mut roles, "upper", interval.upper())?;
                    }
                    _ => {
                        return Err(BindingError::Internal {
                            reason: "проекция: неизвестный NumericalIndeterminacyV1".to_string(),
                        });
                    }
                }
                roles.push('}');
                // Stable Indeterminate emits no halo/core/alpha vars. The
                // caller sees a typed terminal outcome instead of a stale or
                // platform-selected fallback colour.
            }
            RoleOutcome::Material(m) => {
                // Материал (#89): тинт 01 (oklch/α) над опаковой базой 02 (oklch).
                // --lab-<role> несёт солид-канон (= тон, опаковый) как SOLID-
                // фолбэк; --lab-<role>-01 — тинт со слэш-альфой; --lab-<role>-02 —
                // база. Тон/01/02 несут один тон (композит T над T есть T).
                field_str(&mut roles, "kind", "material");
                field_str(&mut roles, "toneHex", &m.tone_hex);
                field_num(&mut roles, "alpha", m.alpha)?;
                field_num(&mut roles, "worstContrast", m.worst_contrast)?;
                field_num(&mut roles, "floor", m.floor)?;
                field_bool(&mut roles, "guaranteed", m.guaranteed);
                field_bool(&mut roles, "poleWhite", m.pole_white);
                field_num(&mut roles, "achievedDj", m.achieved_dj)?;
                field_bool(&mut roles, "toneCompressed", m.tone_compressed);
                field_bool(&mut roles, "hueVanished", m.hue_vanished);
                field_bool(&mut roles, "distinct", m.distinct);
                let solid_css = oklch_css(&m.tone_hex, None)?;
                let tint_css = oklch_css(&m.tone_hex, Some(m.alpha))?;
                field_str(&mut roles, "css", &solid_css);
                // --lab-<role> = солид-канон; -01 = тинт (α); -02 = опаковая база.
                push_var(&mut vars, &css_var, &solid_css);
                push_var(&mut vars, &format!("{css_var}-01"), &tint_css);
                push_var(&mut vars, &format!("{css_var}-02"), &solid_css);
            }
            RoleOutcome::None => {
                field_str(&mut roles, "kind", "none");
            }
            RoleOutcome::Unreachable { code, message } => {
                field_str(&mut roles, "kind", "unreachable");
                field_str(&mut roles, "code", code);
                field_str(&mut roles, "message", message);
            }
        }
        roles.push('}');
    }

    let mut out = String::with_capacity(
        vars.len() + roles.len() + resolved.background.len() + resolved.theme.len() + 64,
    );
    out.push_str("{\"theme\":");
    push_str_lit(&mut out, resolved.theme);
    out.push_str(",\"background\":");
    push_str_lit(&mut out, &resolved.background);
    out.push_str(",\"vars\":{");
    out.push_str(&vars);
    out.push_str("},\"roles\":{");
    out.push_str(&roles);
    out.push_str("}}");
    Ok(out)
}

/// Единая CSS-форма эмиссии: `oklch(L% C H)` / `oklch(L% C H / A)`.
/// Байт-точность реконструкции доказана round-trip тестом ядра на решётке
/// куба. Hex к этому месту валиден по построению (солвер/лестница), но при
/// невозможном парсе — честная структурная ошибка, НЕ тихая подмена формы:
/// потребитель ждёт oklch, а полупрозрачная роль при подмене ещё и потеряла
/// бы альфу.
fn oklch_css(hex: &str, alpha: Option<f64>) -> Result<String, BindingError> {
    labcolors_core::oklch_css_from_hex(hex, alpha).map_err(|reason| BindingError::Internal {
        reason: format!("резолвнутый цвет не сериализуется в oklch: {reason}"),
    })
}

/// Запись в словарь `vars`: `"--lab-<key>":"<css>"` (с запятой-разделителем).
fn push_var(vars: &mut String, key: &str, value: &str) {
    if !vars.is_empty() {
        vars.push(',');
    }
    push_str_lit(vars, key);
    vars.push(':');
    push_str_lit(vars, value);
}

/// Поле-строка: `,"name":"<escaped>"`. Имена полей — статические ASCII
/// идентификаторы контракта, их экранировать не нужно; значения — нужно.
fn field_str(out: &mut String, name: &str, value: &str) {
    out.push_str(",\"");
    out.push_str(name);
    out.push_str("\":");
    push_str_lit(out, value);
}

/// Опциональная строка: `None` эмитится как явный `null`, чтобы отсутствие
/// выполненной диагностики нельзя было спутать с потерянным полем контракта.
fn field_opt_str(out: &mut String, name: &str, value: Option<&str>) {
    match value {
        Some(value) => field_str(out, name, value),
        None => {
            out.push_str(",\"");
            out.push_str(name);
            out.push_str("\":null");
        }
    }
}

/// Поле-число. НЕ-конечное значение — нарушение инварианта солвера: честная
/// ошибка вместо невалидного JSON или тихой подмены.
fn field_num(out: &mut String, name: &str, value: f64) -> Result<(), BindingError> {
    if !value.is_finite() {
        return Err(BindingError::Internal {
            reason: format!("проекция: не-конечное число в поле {name}: {value}"),
        });
    }
    // `{}` для f64 — кратчайшая десятичная запись, восстанавливающая биты.
    let _ = write!(out, ",\"{name}\":{value}");
    Ok(())
}

/// Опциональное число: `None` эмитится как `null` (как `JsValue::NULL` раньше).
fn field_opt_num(out: &mut String, name: &str, value: Option<f64>) -> Result<(), BindingError> {
    match value {
        Some(v) => field_num(out, name, v),
        None => {
            out.push_str(",\"");
            out.push_str(name);
            out.push_str("\":null");
            Ok(())
        }
    }
}

fn field_bool(out: &mut String, name: &str, value: bool) {
    out.push_str(",\"");
    out.push_str(name);
    out.push_str("\":");
    out.push_str(if value { "true" } else { "false" });
}

/// Строковый литерал JSON: кавычки + обратимое экранирование. Управляющие
/// символы — по спецификации JSON; всё остальное (включая не-ASCII) — как
/// есть: граница передаёт UTF-8, `JSON.parse` его принимает.
fn push_str_lit(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{
        GlowColor, GlowIndeterminateColor, ResolvedTheme, RgbaColor, RoleEntry, RoleOutcome,
        SolvedColor,
    };

    fn color_entry(key: &str) -> RoleEntry {
        RoleEntry {
            role_key: key.to_string(),
            outcome: RoleOutcome::Color(SolvedColor {
                hex: "#D5D5D7".to_string(),
                lc: 62.375,
                wcag_ratio: 7.25,
                compressed: false,
                hue_vanished: false,
                achieved_dj: None,
                floor_override: true,
                legal_floor: Some(4.5),
            }),
        }
    }

    fn fixture() -> ResolvedTheme {
        ResolvedTheme {
            theme: "dark",
            background: "#3A3A3C".to_string(),
            roles: vec![
                color_entry("label-primary"),
                RoleEntry {
                    role_key: "spacer".to_string(),
                    outcome: RoleOutcome::None,
                },
                RoleEntry {
                    role_key: "veil".to_string(),
                    outcome: RoleOutcome::Translucent(RgbaColor {
                        tint_hex: "#89CFF0".to_string(),
                        alpha: 0.35,
                        composite_hex: "#55757F".to_string(),
                        composite_lc: -41.5,
                        composite_wcag: 3.125,
                        alpha_coerced: true,
                        floor_coerced: false,
                    }),
                },
                RoleEntry {
                    role_key: "pulse".to_string(),
                    outcome: RoleOutcome::Glow(GlowColor {
                        core_hex: "#A0C5FF".to_string(),
                        halo_hex: "#4A8FFF".to_string(),
                        alpha: 0.036_045_459_685_627_89,
                        alpha_css: "0.03604545968562789".to_string(),
                        target_dj: 2.3006,
                        composite_profile:
                            labcolors_core::GlowCompositeProfileV1::EncodedSrgb8ScreenV1,
                        composite_guarantee: labcolors_core::GlowCompositeGuaranteeV1::BitExact,
                        diagnostic_profile: Some(
                            labcolors_core::GlowDiagnosticProfileV1::Cam16UcsJPrimeLi2017V1,
                        ),
                        decision_profile:
                            labcolors_core::GlowDecisionProfileV1::LegacyPlatformDependentV1,
                        decision_guarantee:
                            labcolors_core::DecisionGuaranteeV1::LegacyPlatformDependentV1,
                        constraint_layer: labcolors_core::GlowConstraintLayer::Halo,
                        target_status: labcolors_core::GlowTargetStatus::Reached,
                        halo_composite_hex: "#13151B".to_string(),
                        halo_achieved_dj: 2.373_123_785_729_128,
                        core_composite_hex: "#15171B".to_string(),
                        core_achieved_dj: 3.235_504_076_619_437,
                    }),
                },
                RoleEntry {
                    role_key: "impossible".to_string(),
                    outcome: RoleOutcome::Unreachable {
                        code: "gamut_exhausted",
                        message: "нет цвета: \"предел\"\nвторая строка".to_string(),
                    },
                },
            ],
        }
    }

    /// Форма и порядок — литерально старая проекция: сравнение с собранным
    /// вручную эталоном (oklch-строки берём у того же ядра, что и код).
    #[test]
    fn shape_and_order_match_the_reflect_projection() {
        let json = resolved_json(&fixture()).unwrap();
        let css_label = labcolors_core::oklch_css_from_hex("#D5D5D7", None).unwrap();
        let css_veil = labcolors_core::oklch_css_from_hex("#89CFF0", Some(0.35)).unwrap();
        let css_halo = labcolors_core::oklch_css_from_hex("#4A8FFF", None).unwrap();
        let css_core = labcolors_core::oklch_css_from_hex("#A0C5FF", None).unwrap();
        let expected = format!(
            concat!(
                "{{\"theme\":\"dark\",\"background\":\"#3A3A3C\",\"vars\":{{",
                "\"--lab-label-primary\":\"{lab}\",",
                "\"--lab-veil\":\"{veil}\",",
                "\"--lab-pulse\":\"{halo}\",",
                "\"--lab-pulse-core\":\"{core}\",",
                "\"--lab-pulse-alpha\":\"0.03604545968562789\"",
                "}},\"roles\":{{",
                "\"label-primary\":{{\"cssVar\":\"--lab-label-primary\",\"kind\":\"color\",",
                "\"hex\":\"#D5D5D7\",\"lc\":62.375,\"wcagRatio\":7.25,\"compressed\":false,",
                "\"hueVanished\":false,\"achievedDj\":null,\"floorOverride\":true,",
                "\"legalFloor\":4.5,\"css\":\"{lab}\"}},",
                "\"spacer\":{{\"cssVar\":\"--lab-spacer\",\"kind\":\"none\"}},",
                "\"veil\":{{\"cssVar\":\"--lab-veil\",\"kind\":\"translucent\",",
                "\"tintHex\":\"#89CFF0\",\"alpha\":0.35,\"compositeHex\":\"#55757F\",",
                "\"compositeLc\":-41.5,\"compositeWcag\":3.125,\"alphaCoerced\":true,",
                "\"floorCoerced\":false,\"css\":\"{veil}\"}},",
                "\"pulse\":{{\"cssVar\":\"--lab-pulse\",\"kind\":\"glow\",",
                "\"coreHex\":\"#A0C5FF\",\"haloHex\":\"#4A8FFF\",",
                "\"alpha\":0.03604545968562789,\"alphaCss\":\"0.03604545968562789\",",
                "\"compositeProfile\":\"encoded-srgb8-screen-v1\",",
                "\"compositeGuarantee\":\"bit-exact\",",
                "\"diagnosticProfile\":\"cam16-ucs-jprime-li2017-v1\",",
                "\"decisionProfile\":\"legacy-platform-dependent-v1\",",
                "\"decisionGuarantee\":{{\"kind\":\"legacy-platform-dependent-v1\"}},",
                "\"constraintLayer\":\"halo\",\"targetDj\":2.3006,\"targetStatus\":\"reached\",",
                "\"haloCompositeHex\":\"#13151B\",\"haloAchievedDj\":2.373123785729128,",
                "\"coreCompositeHex\":\"#15171B\",\"coreAchievedDj\":3.235504076619437,",
                "\"achievedDj\":2.373123785729128,\"degraded\":false,\"css\":\"{halo}\"}},",
                "\"impossible\":{{\"cssVar\":\"--lab-impossible\",\"kind\":\"unreachable\",",
                "\"code\":\"gamut_exhausted\",",
                "\"message\":\"нет цвета: \\\"предел\\\"\\nвторая строка\"}}",
                "}}}}"
            ),
            lab = css_label,
            veil = css_veil,
            halo = css_halo,
            core = css_core,
        );
        assert_eq!(json, expected);
    }

    #[test]
    fn outward_interval_certificates_survive_json_projection() {
        let interval = labcolors_core::OutwardIntervalV1::try_new(0.9, 1.1).unwrap();
        let mut theme = fixture();
        let RoleOutcome::Glow(glow) = &mut theme.roles[3].outcome else {
            panic!("fixture pulse must be Glow");
        };
        glow.decision_guarantee = labcolors_core::DecisionGuaranteeV1::OutwardIntervalV1(interval);
        theme.roles.push(RoleEntry {
            role_key: "uncertain-pulse".to_string(),
            outcome: RoleOutcome::GlowIndeterminate(GlowIndeterminateColor {
                source_hex: "#4A8FFF".to_string(),
                target_dj: 2.3006,
                decision_profile: labcolors_core::GlowDecisionProfileV1::StableV1,
                site_id: labcolors_core::NumericalSiteIdV1::GlowTargetOrMaximumV1,
                evidence: labcolors_core::NumericalIndeterminacyV1::IntervalOverlap(interval),
                constraint_layer: labcolors_core::GlowConstraintLayer::Halo,
            }),
        });

        let value: serde_json::Value =
            serde_json::from_str(&resolved_json(&theme).unwrap()).unwrap();
        let guarantee = &value["roles"]["pulse"]["decisionGuarantee"];
        assert_eq!(guarantee["kind"], "outward-interval-v1");
        assert_eq!(guarantee["lower"].as_f64(), Some(0.9));
        assert_eq!(guarantee["upper"].as_f64(), Some(1.1));
        let uncertain = &value["roles"]["uncertain-pulse"];
        assert_eq!(uncertain["reason"], "interval-overlap");
        assert_eq!(uncertain["bounds"]["kind"], "outward");
        assert_eq!(uncertain["bounds"]["lower"].as_f64(), Some(0.9));
        assert_eq!(uncertain["bounds"]["upper"].as_f64(), Some(1.1));
    }

    /// Материал (#89) проецируется в контрактные CSS-переменные: `--lab-<role>` =
    /// солид-канон (oklch), `--lab-<role>-01` = тинт (oklch со слэш-альфой),
    /// `--lab-<role>-02` = опаковая база (oklch). Плюс полный набор полей исхода.
    /// Пин ИМЁН переменных — та поверхность, что потребляет labui-material.css.
    #[test]
    fn material_projects_two_layer_css_vars() {
        use crate::dto::MaterialColor;
        let theme = ResolvedTheme {
            theme: "light",
            background: "#FFFFFF".to_string(),
            roles: vec![RoleEntry {
                role_key: "bg-material-base".to_string(),
                outcome: RoleOutcome::Material(MaterialColor {
                    tone_hex: "#B4B4BC".to_string(),
                    alpha: 0.6375,
                    worst_contrast: 4.61,
                    floor: 4.5,
                    guaranteed: true,
                    pole_white: false,
                    achieved_dj: 18.25,
                    tone_compressed: false,
                    hue_vanished: false,
                    distinct: true,
                }),
            }],
        };
        let json = resolved_json(&theme).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();

        // Три контрактные переменные: солид-канон, тинт 01 (α), база 02.
        let solid = labcolors_core::oklch_css_from_hex("#B4B4BC", None).unwrap();
        let tint = labcolors_core::oklch_css_from_hex("#B4B4BC", Some(0.6375)).unwrap();
        assert_eq!(v["vars"]["--lab-bg-material-base"].as_str().unwrap(), solid);
        assert_eq!(
            v["vars"]["--lab-bg-material-base-01"].as_str().unwrap(),
            tint,
            "-01 обязан нести тинт oklch со слэш-альфой"
        );
        assert_eq!(
            v["vars"]["--lab-bg-material-base-02"].as_str().unwrap(),
            solid,
            "-02 обязан нести опаковую базу"
        );

        // Полный набор полей исхода.
        let r = &v["roles"]["bg-material-base"];
        assert_eq!(r["kind"], "material");
        assert_eq!(r["cssVar"], "--lab-bg-material-base");
        assert_eq!(r["toneHex"], "#B4B4BC");
        assert_eq!(r["alpha"].as_f64().unwrap(), 0.6375);
        assert_eq!(r["worstContrast"].as_f64().unwrap(), 4.61);
        assert_eq!(r["floor"].as_f64().unwrap(), 4.5);
        assert_eq!(r["guaranteed"], true);
        assert_eq!(r["poleWhite"], false);
        assert_eq!(r["achievedDj"].as_f64().unwrap(), 18.25);
        assert_eq!(r["toneCompressed"], false);
        assert_eq!(r["hueVanished"], false);
        assert_eq!(r["distinct"], true);
        assert_eq!(r["css"].as_str().unwrap(), solid);
    }

    /// Числа переживают декаду через кратчайшую десятичную запись: биты double
    /// после парсинга равны исходным (и JSON синтаксически валиден для serde).
    #[test]
    fn numbers_round_trip_bit_exact() {
        let mut theme = fixture();
        // Неудобные значения: двоично-непредставимые и субнормально-мелкие.
        if let RoleOutcome::Color(c) = &mut theme.roles[0].outcome {
            c.lc = 0.1 + 0.2; // 0.30000000000000004
            c.wcag_ratio = 1.000_000_000_000_000_2;
            c.achieved_dj = Some(3.9e-17);
        }
        let json = resolved_json(&theme).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let role = &v["roles"]["label-primary"];
        assert_eq!(
            role["lc"].as_f64().unwrap().to_bits(),
            (0.1_f64 + 0.2).to_bits()
        );
        assert_eq!(
            role["wcagRatio"].as_f64().unwrap().to_bits(),
            1.000_000_000_000_000_2_f64.to_bits()
        );
        assert_eq!(
            role["achievedDj"].as_f64().unwrap().to_bits(),
            3.9e-17_f64.to_bits()
        );
    }

    /// Враждебный ключ роли (конфиг — пользовательский ввод) экранируется
    /// обратимо: serde возвращает ключ байт-в-байт.
    #[test]
    fn hostile_role_keys_escape_reversibly() {
        let theme = ResolvedTheme {
            theme: "light",
            background: "#FFFFFF".to_string(),
            roles: vec![RoleEntry {
                role_key: "we\"ird\\key\n\t\u{0001}".to_string(),
                outcome: RoleOutcome::None,
            }],
        };
        let json = resolved_json(&theme).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let roles = v["roles"].as_object().unwrap();
        assert!(roles.contains_key("we\"ird\\key\n\t\u{0001}"));
        assert_eq!(
            roles["we\"ird\\key\n\t\u{0001}"]["cssVar"]
                .as_str()
                .unwrap(),
            "--lab-we\"ird\\key\n\t\u{0001}"
        );
    }

    /// НЕ-конечное число — честная структурная ошибка, не невалидный JSON.
    #[test]
    fn non_finite_numbers_are_a_structured_error() {
        let mut theme = fixture();
        if let RoleOutcome::Color(c) = &mut theme.roles[0].outcome {
            c.lc = f64::NAN;
        }
        match resolved_json(&theme) {
            Err(BindingError::Internal { reason }) => {
                assert!(reason.contains("не-конечное"), "reason: {reason}");
            }
            other => panic!("ожидалась Internal-ошибка, получено: {other:?}"),
        }
    }
}
