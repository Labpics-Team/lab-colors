//! Сериализация [`ResolvedTheme`] в JSON-строку — проекция «одним пересечением
//! границы» (задача #54).
//!
//! Прежняя проекция строила JS-объект по полю за раз: ~30 ролей × ~10 свойств
//! = сотни FFI-вызовов `Reflect::set` + интернирование JS-строки на каждый
//! ключ, на КАЖДЫЙ вызов `resolveTheme`, включая cache-hit. Здесь тот же
//! объект описывается одной JSON-строкой, а адаптер ([`crate::lib`]) отдаёт её
//! нативному `JSON.parse` — два пересечения границы вместо сотен.
//!
//! ЖЕЛЕЗНЫЙ ИНВАРИАНТ — выход по-значению идентичен прежнему:
//! - порядок вставки ключей на каждом уровне повторяет прежний порядок
//!   `Reflect::set` (top-level: `theme`, `background`, `vars`, `roles`; роль:
//!   `cssVar`, `kind`, …данные исхода…, `css`); JSON.parse сохраняет порядок
//!   не-индексных ключей — `Object.keys` неотличим;
//! - каждый f64 пишется кратчайшей round-trip формой (ryu — тот же алгоритм,
//!   что внутри serde_json), так что `JSON.parse` восстанавливает битово тот
//!   же double, который прежде нёс `JsValue::from_f64` (включая `-0.0`);
//! - `None`-опции эмитятся как `null` — то же, что прежний `JsValue::NULL`;
//! - строки экранируются по JSON — парс возвращает идентичную строку.
//!
//! Модуль свободен от wasm-bindgen/js-sys, поэтому тестируется нативным
//! `cargo test`; кросс-граничная идентичность залочена JS-оракулом
//! `packages/colors/test/resolve-projection-parity.test.mjs` (golden снят на
//! до-оптимизационной проекции).

use std::fmt::Write as _;

use crate::dto::{ResolvedTheme, RoleOutcome};
use crate::error::BindingError;

/// Спроецировать чистый [`ResolvedTheme`] в JSON-текст того самого объекта,
/// который описывает `.d.ts`. Как и прежняя проекция — генерически по вектору
/// ролей: ни одна роль здесь не поименована, смена набора проходит насквозь.
pub(crate) fn resolved_to_json(resolved: &ResolvedTheme) -> Result<String, BindingError> {
    // vars и roles наполняются в одном проходе по ролям (как раньше — в одном
    // цикле Reflect::set), но текстово это два разных объекта — два буфера.
    let mut vars = String::with_capacity(resolved.roles.len() * 64);
    let mut roles = String::with_capacity(resolved.roles.len() * 256);

    for entry in &resolved.roles {
        let css_var = format!("--lab-{}", entry.role_key);
        if !roles.is_empty() {
            roles.push(',');
        }
        push_json_str(&mut roles, &entry.role_key);
        roles.push_str(":{\"cssVar\":");
        push_json_str(&mut roles, &css_var);
        match &entry.outcome {
            RoleOutcome::Color(c) => {
                roles.push_str(",\"kind\":\"color\",\"hex\":");
                push_json_str(&mut roles, &c.hex);
                roles.push_str(",\"lc\":");
                push_f64(&mut roles, c.lc)?;
                roles.push_str(",\"wcagRatio\":");
                push_f64(&mut roles, c.wcag_ratio)?;
                roles.push_str(",\"compressed\":");
                push_bool(&mut roles, c.compressed);
                roles.push_str(",\"hueVanished\":");
                push_bool(&mut roles, c.hue_vanished);
                roles.push_str(",\"achievedDj\":");
                push_opt_f64(&mut roles, c.achieved_dj)?;
                roles.push_str(",\"floorOverride\":");
                push_bool(&mut roles, c.floor_override);
                roles.push_str(",\"legalFloor\":");
                push_opt_f64(&mut roles, c.legal_floor)?;
                // Единая форма эмиссии: oklch и для солида (hex остаётся
                // данными роли; синтаксис переменной один на все исходы).
                let css = oklch_css(&c.hex, None)?;
                roles.push_str(",\"css\":");
                push_json_str(&mut roles, &css);
                push_var(&mut vars, &css_var, &css);
            }
            RoleOutcome::Translucent(r) => {
                roles.push_str(",\"kind\":\"translucent\",\"tintHex\":");
                push_json_str(&mut roles, &r.tint_hex);
                roles.push_str(",\"alpha\":");
                push_f64(&mut roles, r.alpha)?;
                roles.push_str(",\"compositeHex\":");
                push_json_str(&mut roles, &r.composite_hex);
                roles.push_str(",\"compositeLc\":");
                push_f64(&mut roles, r.composite_lc)?;
                roles.push_str(",\"compositeWcag\":");
                push_f64(&mut roles, r.composite_wcag)?;
                roles.push_str(",\"alphaCoerced\":");
                push_bool(&mut roles, r.alpha_coerced);
                roles.push_str(",\"floorCoerced\":");
                push_bool(&mut roles, r.floor_coerced);
                // Переменная несёт тинт в oklch со слэш-альфой — браузер
                // композитит на живой подложке; форма едина с солидами.
                let css = oklch_css(&r.tint_hex, Some(r.alpha))?;
                roles.push_str(",\"css\":");
                push_json_str(&mut roles, &css);
                push_var(&mut vars, &css_var, &css);
            }
            RoleOutcome::Glow(g) => {
                // Свечение: слои для screen-наложения потребителем.
                // --lab-<role> несёт halo (единая oklch-форма), сателлиты
                // --lab-<role>-core / --lab-<role>-alpha — анатомия и
                // решённая интенсивность (число, не цвет).
                roles.push_str(",\"kind\":\"glow\",\"coreHex\":");
                push_json_str(&mut roles, &g.core_hex);
                roles.push_str(",\"haloHex\":");
                push_json_str(&mut roles, &g.halo_hex);
                roles.push_str(",\"alpha\":");
                push_f64(&mut roles, g.alpha)?;
                roles.push_str(",\"achievedDj\":");
                push_f64(&mut roles, g.achieved_dj)?;
                roles.push_str(",\"degraded\":");
                push_bool(&mut roles, g.degraded);
                let halo_css = oklch_css(&g.halo_hex, None)?;
                let core_css = oklch_css(&g.core_hex, None)?;
                roles.push_str(",\"css\":");
                push_json_str(&mut roles, &halo_css);
                push_var(&mut vars, &css_var, &halo_css);
                push_var(&mut vars, &format!("{css_var}-core"), &core_css);
                push_var(
                    &mut vars,
                    &format!("{css_var}-alpha"),
                    &format!("{:.4}", g.alpha),
                );
            }
            RoleOutcome::None => {
                roles.push_str(",\"kind\":\"none\"");
            }
            RoleOutcome::Unreachable { code, message } => {
                roles.push_str(",\"kind\":\"unreachable\",\"code\":");
                push_json_str(&mut roles, code);
                roles.push_str(",\"message\":");
                push_json_str(&mut roles, message);
            }
        }
        roles.push('}');
    }

    let mut out = String::with_capacity(vars.len() + roles.len() + resolved.background.len() + 64);
    out.push_str("{\"theme\":");
    push_json_str(&mut out, resolved.theme);
    out.push_str(",\"background\":");
    push_json_str(&mut out, &resolved.background);
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
/// потребитель ждёт oklch, а полупрозрачная роль при подмене ещё и потеряла бы альфу.
fn oklch_css(hex: &str, alpha: Option<f64>) -> Result<String, BindingError> {
    labcolors_core::oklch_css_from_hex(hex, alpha).map_err(|reason| BindingError::Internal {
        reason: format!("резолвнутый цвет не сериализуется в oklch: {reason}"),
    })
}

/// Пара `"имя":"значение"` в буфер объекта `vars` (значения переменных —
/// всегда строки, включая `--lab-<role>-alpha` с её фиксированным `{:.4}`).
fn push_var(vars: &mut String, name: &str, value: &str) {
    if !vars.is_empty() {
        vars.push(',');
    }
    push_json_str(vars, name);
    vars.push(':');
    push_json_str(vars, value);
}

/// JSON-строка с экранированием. Ключи ролей приходят из пользовательского
/// конфига, `message` — из `Display` ядра, поэтому экранируются ВСЕ строки
/// одинаково (hex/oklch безопасны, но единообразие дешевле исключений).
/// `JSON.parse` восстанавливает идентичную строку.
fn push_json_str(buf: &mut String, s: &str) {
    buf.push('"');
    for ch in s.chars() {
        match ch {
            '"' => buf.push_str("\\\""),
            '\\' => buf.push_str("\\\\"),
            '\n' => buf.push_str("\\n"),
            '\r' => buf.push_str("\\r"),
            '\t' => buf.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(buf, "\\u{:04x}", c as u32);
            }
            c => buf.push(c),
        }
    }
    buf.push('"');
}

/// f64 кратчайшей round-trip формой (ryu): `JSON.parse` вернёт битово тот же
/// double, что нёс прежний `JsValue::from_f64`, включая `-0.0` → `-0`.
/// Контрасты/альфы конечны по построению ядра; нечисло здесь — нарушение
/// инварианта, и это честная структурная ошибка, а не невалидный JSON наружу.
fn push_f64(buf: &mut String, v: f64) -> Result<(), BindingError> {
    if !v.is_finite() {
        return Err(BindingError::Internal {
            reason: format!("неконечное число в проекции результата: {v}"),
        });
    }
    let mut b = ryu::Buffer::new();
    buf.push_str(b.format_finite(v));
    Ok(())
}

fn push_opt_f64(buf: &mut String, v: Option<f64>) -> Result<(), BindingError> {
    match v {
        Some(v) => push_f64(buf, v),
        None => {
            buf.push_str("null");
            Ok(())
        }
    }
}

fn push_bool(buf: &mut String, v: bool) {
    buf.push_str(if v { "true" } else { "false" });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{GlowColor, RgbaColor, RoleEntry, SolvedColor};

    fn theme_with(roles: Vec<RoleEntry>) -> ResolvedTheme {
        ResolvedTheme {
            theme: "dark",
            background: "#3A3A3C".into(),
            roles,
        }
    }

    /// Точная форма и ПОРЯДОК ключей для всех пяти исходов — то, что прежде
    /// задавала последовательность Reflect::set. Ожидание собирается из тех же
    /// `oklch_css`, что и продакшен-путь: тест фиксирует рамку вокруг них.
    #[test]
    fn exact_shape_and_key_order_for_every_outcome() {
        let solved = SolvedColor {
            hex: "#AABBCC".into(),
            lc: 61.25,
            wcag_ratio: 4.5,
            compressed: false,
            hue_vanished: true,
            achieved_dj: None,
            floor_override: false,
            legal_floor: Some(4.5),
        };
        let translucent = RgbaColor {
            tint_hex: "#112233".into(),
            alpha: 0.25,
            composite_hex: "#223344".into(),
            composite_lc: -12.5,
            composite_wcag: 1.5,
            alpha_coerced: true,
            floor_coerced: false,
        };
        let glow = GlowColor {
            core_hex: "#FFEEDD".into(),
            halo_hex: "#DDEEFF".into(),
            alpha: 0.5,
            achieved_dj: 7.75,
            degraded: false,
        };
        let resolved = theme_with(vec![
            RoleEntry {
                role_key: "text".into(),
                outcome: RoleOutcome::Color(solved.clone()),
            },
            RoleEntry {
                role_key: "veil".into(),
                outcome: RoleOutcome::Translucent(translucent),
            },
            RoleEntry {
                role_key: "pulse".into(),
                outcome: RoleOutcome::Glow(glow),
            },
            RoleEntry {
                role_key: "zero".into(),
                outcome: RoleOutcome::None,
            },
            RoleEntry {
                role_key: "impossible".into(),
                outcome: RoleOutcome::Unreachable {
                    code: "gamut_exhausted",
                    message: "нет цвета: \"край\" гаммы\n".into(),
                },
            },
        ]);

        let css_text = oklch_css("#AABBCC", None).unwrap();
        let css_veil = oklch_css("#112233", Some(0.25)).unwrap();
        let css_halo = oklch_css("#DDEEFF", None).unwrap();
        let css_core = oklch_css("#FFEEDD", None).unwrap();

        let expected = format!(
            concat!(
                "{{\"theme\":\"dark\",\"background\":\"#3A3A3C\",\"vars\":{{",
                "\"--lab-text\":\"{ct}\",",
                "\"--lab-veil\":\"{cv}\",",
                "\"--lab-pulse\":\"{ch}\",\"--lab-pulse-core\":\"{cc}\",\"--lab-pulse-alpha\":\"0.5000\"",
                "}},\"roles\":{{",
                "\"text\":{{\"cssVar\":\"--lab-text\",\"kind\":\"color\",\"hex\":\"#AABBCC\",",
                "\"lc\":61.25,\"wcagRatio\":4.5,\"compressed\":false,\"hueVanished\":true,",
                "\"achievedDj\":null,\"floorOverride\":false,\"legalFloor\":4.5,\"css\":\"{ct}\"}},",
                "\"veil\":{{\"cssVar\":\"--lab-veil\",\"kind\":\"translucent\",\"tintHex\":\"#112233\",",
                "\"alpha\":0.25,\"compositeHex\":\"#223344\",\"compositeLc\":-12.5,",
                "\"compositeWcag\":1.5,\"alphaCoerced\":true,\"floorCoerced\":false,\"css\":\"{cv}\"}},",
                "\"pulse\":{{\"cssVar\":\"--lab-pulse\",\"kind\":\"glow\",\"coreHex\":\"#FFEEDD\",",
                "\"haloHex\":\"#DDEEFF\",\"alpha\":0.5,\"achievedDj\":7.75,\"degraded\":false,\"css\":\"{ch}\"}},",
                "\"zero\":{{\"cssVar\":\"--lab-zero\",\"kind\":\"none\"}},",
                "\"impossible\":{{\"cssVar\":\"--lab-impossible\",\"kind\":\"unreachable\",",
                "\"code\":\"gamut_exhausted\",\"message\":\"нет цвета: \\\"край\\\" гаммы\\n\"}}",
                "}}}}",
            ),
            ct = css_text,
            cv = css_veil,
            ch = css_halo,
            cc = css_core,
        );

        assert_eq!(resolved_to_json(&resolved).unwrap(), expected);
    }

    /// Каждый f64 обязан пережить JSON round-trip битово — включая -0.0 и
    /// значения без короткой десятичной записи.
    // Литералы выписаны полной (не кратчайшей) формой намеренно: они и есть
    // тест-данные «значение без короткой десятичной записи», побитно.
    #[expect(clippy::excessive_precision)]
    #[test]
    fn f64_survives_json_round_trip_bit_for_bit() {
        for v in [
            0.0f64,
            -0.0,
            4.5,
            1.0 / 3.0,
            f64::MIN_POSITIVE,
            2.2250738585072011e-308, // субнормальная граница
            106.04099999999998,
            -61.249999999999993,
            1e300,
        ] {
            let mut s = String::new();
            push_f64(&mut s, v).unwrap();
            let parsed: f64 = serde_json::from_str(&s).unwrap();
            assert_eq!(
                parsed.to_bits(),
                v.to_bits(),
                "{v:?} → {s} → {parsed:?}: биты разошлись"
            );
        }
    }

    /// Неконечное значение — честная структурная ошибка, не NaN-JSON наружу.
    #[test]
    fn non_finite_is_a_structured_error() {
        let mut s = String::new();
        assert!(push_f64(&mut s, f64::NAN).is_err());
        assert!(push_f64(&mut s, f64::INFINITY).is_err());
    }

    /// Ключи ролей приходят из конфига потребителя: кавычки, бэкслэши и
    /// управляющие символы обязаны пережить парс идентично.
    #[test]
    fn strings_escape_to_valid_json_and_round_trip() {
        for s in [
            "обычный",
            "с \"кавычками\"",
            "back\\slash",
            "tab\tnl\nctl\u{1}",
        ] {
            let mut buf = String::new();
            push_json_str(&mut buf, s);
            let parsed: String = serde_json::from_str(&buf).unwrap();
            assert_eq!(parsed, s);
        }
    }

    /// Сериализация целиком — валидный JSON (парсится строгим serde_json);
    /// структурная совместимость с JSON.parse гарантирована грамматикой.
    #[test]
    fn output_is_valid_json() {
        let resolved = theme_with(vec![RoleEntry {
            role_key: "text".into(),
            outcome: RoleOutcome::Color(SolvedColor {
                hex: "#AABBCC".into(),
                lc: 61.25,
                wcag_ratio: 4.5,
                compressed: false,
                hue_vanished: false,
                achieved_dj: Some(-0.0),
                floor_override: false,
                legal_floor: None,
            }),
        }]);
        let json = resolved_to_json(&resolved).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["roles"]["text"]["kind"], "color");
        assert_eq!(value["theme"], "dark");
    }
}
