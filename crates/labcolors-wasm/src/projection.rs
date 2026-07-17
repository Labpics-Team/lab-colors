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

use crate::dto::{GlowColor, GlowIndeterminateColor, MaterialColor, ResolvedTheme, RoleOutcome};
use crate::error::BindingError;

/// Project the core-owned WCAG22 assessment without recomputing any math.
pub fn wcag22_json(
    assessment: &labcolors_core::wcag22::Wcag22AssessmentV1,
) -> Result<String, BindingError> {
    use labcolors_core::NumericalDecisionEvidenceV1;
    use labcolors_core::wcag22::{Wcag22ApplicableDecisionV1, Wcag22AssessmentV1};

    let Wcag22AssessmentV1::Evaluated {
        profile_id,
        criterion,
        measurement,
        decision,
        evidence,
        ..
    } = assessment
    else {
        return Err(BindingError::Internal {
            reason: "pair evaluator returned report-only NotEvaluated".to_string(),
        });
    };
    let NumericalDecisionEvidenceV1::CanonicalFiniteBounded(evidence_payload) = evidence else {
        return Err(BindingError::Internal {
            reason: "WCAG22 assessment carried a non-bounded evidence class".to_string(),
        });
    };
    let artifact_id = evidence_payload.artifact_id();
    let bound_id = evidence_payload.bound_id();
    let proof_id = evidence_payload.proof_id();
    let profile = labcolors_core::wcag22::wcag22_profile_v1();
    if profile.profile_id != *profile_id
        || profile.artifact_id != artifact_id
        || profile.bound_id != bound_id
        || profile.proof_id != proof_id
    {
        return Err(BindingError::Internal {
            reason: "WCAG22 assessment/profile evidence identities drifted".to_string(),
        });
    }

    let hex = |bytes: [u8; 3]| format!("#{:02X}{:02X}{:02X}", bytes[0], bytes[1], bytes[2]);
    let criterion = criterion.key();
    let decision = match decision {
        Wcag22ApplicableDecisionV1::Pass => "pass",
        Wcag22ApplicableDecisionV1::Fail => "fail",
        _ => {
            return Err(BindingError::Internal {
                reason: "unknown core WCAG22 decision variant".to_string(),
            });
        }
    };

    Ok(format!(
        concat!(
            "{{\"kind\":\"evaluated\",\"profileId\":\"{}\",",
            "\"criterion\":\"{}\",\"foreground\":\"{}\",\"background\":\"{}\",",
            "\"foregroundLuminanceQ55\":{{\"lower\":\"{}\",\"upper\":\"{}\"}},",
            "\"backgroundLuminanceQ55\":{{\"lower\":\"{}\",\"upper\":\"{}\"}},",
            "\"q55Scale\":\"{}\",\"decision\":\"{}\",",
            "\"evidence\":{{\"kind\":\"canonical-finite-bounded\",",
            "\"artifactId\":\"{}\",\"artifactSha256\":\"{}\",",
            "\"boundId\":\"{}\",\"proofId\":\"{}\",",
            "\"proofSha256\":\"{}\",\"proofPayloadSha256\":\"{}\",",
            "\"generatorSha256\":\"{}\",\"verifierSha256\":\"{}\",",
            "\"profileChecksum\":\"{}\",\"profileSha256\":\"{}\"}}}}"
        ),
        profile_id.key(),
        criterion,
        hex(measurement.foreground),
        hex(measurement.background),
        measurement.foreground_luminance.lower(),
        measurement.foreground_luminance.upper(),
        measurement.background_luminance.lower(),
        measurement.background_luminance.upper(),
        labcolors_core::wcag22::Wcag22LuminanceBoundsQ55V1::scale(),
        decision,
        artifact_id.key(),
        profile.artifact_sha256,
        bound_id.key(),
        proof_id.key(),
        profile.proof_sha256,
        profile.proof_payload_sha256,
        profile.generator_sha256,
        profile.verifier_sha256,
        profile.profile_checksum,
        profile.source_sha256,
    ))
}

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
                let degraded = glow_degraded_from_provenance(g)?;
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
                field_str(
                    &mut roles,
                    "compositeProfile",
                    glow_composite_profile_key(g.composite_profile)?,
                );
                field_str(
                    &mut roles,
                    "compositeGuarantee",
                    glow_composite_guarantee_key(g.composite_guarantee)?,
                );
                field_str(
                    &mut roles,
                    "layerRecipeProfile",
                    glow_layer_recipe_profile_key(g.layer_recipe_profile)?,
                );
                field_str(
                    &mut roles,
                    "appearanceDiagnosticProfile",
                    glow_diagnostic_profile_key(g.appearance_diagnostic_profile)?,
                );
                let selection_diagnostic_profile = match g.selection_diagnostic_profile {
                    Some(profile) => Some(glow_diagnostic_profile_key(profile)?),
                    None => None,
                };
                field_opt_str(
                    &mut roles,
                    "selectionDiagnosticProfile",
                    selection_diagnostic_profile,
                );
                field_str(
                    &mut roles,
                    "decisionProfile",
                    glow_decision_profile_key(g.decision_outcome.decision_profile())?,
                );
                field_glow_decision_guarantee(&mut roles, &g.decision_outcome)?;
                field_str(
                    &mut roles,
                    "constraintLayer",
                    glow_constraint_layer_key(g.constraint_layer)?,
                );
                field_num(&mut roles, "targetDj", g.target_dj)?;
                field_str(
                    &mut roles,
                    "targetStatus",
                    glow_target_status_key(g.target_status)?,
                );
                field_str(&mut roles, "haloCompositeHex", &g.halo_composite_hex);
                field_num(&mut roles, "haloAchievedDj", g.halo_achieved_dj)?;
                field_str(&mut roles, "coreCompositeHex", &g.core_composite_hex);
                field_num(&mut roles, "coreAchievedDj", g.core_achieved_dj)?;
                // Aliases совместимости старого неоднозначного контракта.
                field_num(&mut roles, "achievedDj", g.halo_achieved_dj)?;
                field_bool(&mut roles, "degraded", degraded);
                let halo_css = oklch_css(&g.halo_hex, None)?;
                let core_css = oklch_css(&g.core_hex, None)?;
                field_str(&mut roles, "css", &halo_css);
                push_var(&mut vars, &css_var, &halo_css);
                push_var(&mut vars, &format!("{css_var}-core"), &core_css);
                push_var(&mut vars, &format!("{css_var}-alpha"), &g.alpha_css);
            }
            RoleOutcome::GlowIndeterminate(g) => {
                validate_glow_indeterminate_provenance(g)?;
                field_str(&mut roles, "kind", "glow-indeterminate");
                field_str(&mut roles, "sourceHex", &g.source_hex);
                field_num(&mut roles, "targetDj", g.target_dj)?;
                field_str(
                    &mut roles,
                    "constraintLayer",
                    glow_constraint_layer_key(g.constraint_layer)?,
                );
                field_str(
                    &mut roles,
                    "decisionProfile",
                    glow_decision_profile_key(g.decision_profile)?,
                );
                field_str(
                    &mut roles,
                    "numericalSiteId",
                    numerical_site_id_key(g.site_id)?,
                );
                fields_numerical_indeterminacy(&mut roles, g.evidence)?;
                // Неопределённая stable-ветвь не эмитит переменные halo/core/alpha:
                // вызывающий код получает типизированный терминальный исход, а не
                // устаревший или выбранный платформой резервный цвет.
            }
            RoleOutcome::Material(m) => {
                let guaranteed = material_guaranteed_from_provenance(m)?;
                // Материал (whitepaper §3.7): тинт 01 (oklch/α) над опаковой базой 02 (oklch).
                // --lab-<role> несёт солид-канон (= тон, опаковый) как SOLID-
                // фолбэк; --lab-<role>-01 — тинт со слэш-альфой; --lab-<role>-02 —
                // база. Тон/01/02 несут один тон (композит T над T есть T).
                field_str(&mut roles, "kind", "material");
                field_str(&mut roles, "toneHex", &m.tone_hex);
                field_num(&mut roles, "alpha", m.alpha)?;
                field_num(&mut roles, "worstContrast", m.worst_contrast)?;
                field_material_alpha_guarantee(&mut roles, m.alpha_guarantee)?;
                field_str(
                    &mut roles,
                    "alphaStatus",
                    material_alpha_status_key(m.alpha_status)?,
                );
                field_num(&mut roles, "floor", m.floor)?;
                field_bool(&mut roles, "guaranteed", guaranteed);
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
            RoleOutcome::Failure {
                category,
                code,
                message,
            } => {
                field_str(&mut roles, "kind", "failure");
                field_str(&mut roles, "category", category);
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

/// Proof-capable V2-проекция. Форма совпадает с `numericalCapabilities`
/// conformance pack 4. Это единственная public adapter projection.
pub fn capability_manifest_json() -> String {
    let manifest = labcolors_core::numerical_capability_manifest_v2();
    let mut out = String::with_capacity(512);
    out.push_str("{\"schemaVersion\":");
    let _ = write!(out, "{}", manifest.schema_version);
    out.push_str(",\"coverage\":");
    push_str_lit(&mut out, manifest.coverage.key());
    out.push_str(",\"sites\":[");
    for (index, site) in manifest.sites.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"siteId\":");
        push_str_lit(&mut out, site.site_id.key());
        push_key_array(
            &mut out,
            "stableOutcomes",
            site.stable_outcomes.iter().map(|v| v.key()),
        );
        push_key_array(
            &mut out,
            "compatibilityReleases",
            site.compatibility_releases.iter().map(|v| v.key()),
        );
        push_key_array(
            &mut out,
            "evidenceClasses",
            site.evidence_classes.iter().map(|v| v.key()),
        );
        push_key_array(
            &mut out,
            "artifactIds",
            site.artifact_ids.iter().map(|v| v.key()),
        );
        push_key_array(&mut out, "boundIds", site.bound_ids.iter().map(|v| v.key()));
        push_key_array(&mut out, "proofIds", site.proof_ids.iter().map(|v| v.key()));
        push_key_array(
            &mut out,
            "runtimeAttestations",
            site.runtime_attestations.iter().map(|v| v.key()),
        );
        out.push('}');
    }
    out.push_str("],\"checksum\":");
    push_str_lit(&mut out, &manifest.checksum.hex());
    out.push('}');
    out
}

/// Массив статических wire-ключей: `,"name":["a","b"]`; пустой — явный `[]`.
fn push_key_array<'a>(out: &mut String, name: &str, keys: impl Iterator<Item = &'a str>) {
    out.push_str(",\"");
    out.push_str(name);
    out.push_str("\":[");
    for (index, key) in keys.enumerate() {
        if index > 0 {
            out.push(',');
        }
        push_str_lit(out, key);
    }
    out.push(']');
}

fn unknown_output_variant(type_name: &str) -> BindingError {
    BindingError::Internal {
        reason: format!("проекция: неизвестный вариант {type_name}"),
    }
}

fn glow_degraded_from_provenance(glow: &GlowColor) -> Result<bool, BindingError> {
    use labcolors_core::glow::GlowDecisionOutcomeV1;
    // Атомарный decision_outcome (#292) уже делает незаконную пару
    // profile × guarantee непредставимой; проекции осталось сверить его с
    // селекционной диагностикой и статусом цели — они по-прежнему приходят
    // отдельными полями, и их рассинхрон был бы порчей provenance выше.
    match (
        &glow.decision_outcome,
        glow.selection_diagnostic_profile,
        glow.target_status,
    ) {
        (
            GlowDecisionOutcomeV1::StableExactNoop { .. },
            None,
            labcolors_core::GlowTargetStatus::ExactNoopUnreachable,
        ) => Ok(true),
        (
            GlowDecisionOutcomeV1::Compatibility {
                release_id:
                    labcolors_core::NumericalCompatibilityReleaseIdV1::GlowCam16UcsJPrimeTargetOrMaxV1,
                ..
            },
            Some(labcolors_core::GlowDiagnosticProfileV1::Cam16UcsJPrimeLi2017V1),
            labcolors_core::GlowTargetStatus::LegacyReached,
        ) => Ok(false),
        (
            GlowDecisionOutcomeV1::Compatibility {
                release_id:
                    labcolors_core::NumericalCompatibilityReleaseIdV1::GlowCam16UcsJPrimeTargetOrMaxV1,
                ..
            },
            Some(labcolors_core::GlowDiagnosticProfileV1::Cam16UcsJPrimeLi2017V1),
            labcolors_core::GlowTargetStatus::LegacyUnreachable,
        ) => Ok(true),
        _ => Err(BindingError::Internal {
            reason: "проекция: несогласованный Glow provenance".to_string(),
        }),
    }
}

fn validate_glow_indeterminate_provenance(
    glow: &GlowIndeterminateColor,
) -> Result<(), BindingError> {
    match glow.decision_profile {
        labcolors_core::GlowDecisionProfileV1::StableV1 => Ok(()),
        _ => Err(BindingError::Internal {
            reason: "проекция: несогласованный GlowIndeterminate provenance".to_string(),
        }),
    }
}

fn material_guaranteed_from_provenance(material: &MaterialColor) -> Result<bool, BindingError> {
    match (material.alpha_status, material.alpha_guarantee) {
        (
            labcolors_core::MaterialAlphaStatusV1::Satisfied,
            labcolors_core::MaterialAlphaGuaranteeV1::TransparentEndpointCharacterizedV1 { .. },
        ) if material.alpha.to_bits() == 0.0_f64.to_bits() => Ok(true),
        (
            labcolors_core::MaterialAlphaStatusV1::Satisfied,
            labcolors_core::MaterialAlphaGuaranteeV1::BisectionBracketCharacterizedV1 {
                upper_alpha,
                ..
            },
        ) if material.alpha.to_bits() == upper_alpha.to_bits() => Ok(true),
        (
            labcolors_core::MaterialAlphaStatusV1::Degraded,
            labcolors_core::MaterialAlphaGuaranteeV1::OpaqueEndpointCharacterizedV1 { .. },
        ) if material.alpha.to_bits() == 1.0_f64.to_bits() => Ok(false),
        _ => Err(BindingError::Internal {
            reason: "проекция: несогласованный Material provenance".to_string(),
        }),
    }
}

fn glow_composite_profile_key(
    profile: labcolors_core::GlowCompositeProfileV1,
) -> Result<&'static str, BindingError> {
    match profile {
        labcolors_core::GlowCompositeProfileV1::EncodedSrgb8ScreenV1 => {
            Ok("encoded-srgb8-screen-v1")
        }
        _ => Err(unknown_output_variant("GlowCompositeProfileV1")),
    }
}

fn glow_composite_guarantee_key(
    guarantee: labcolors_core::GlowCompositeGuaranteeV1,
) -> Result<&'static str, BindingError> {
    match guarantee {
        labcolors_core::GlowCompositeGuaranteeV1::BitExact => Ok("bit-exact"),
        _ => Err(unknown_output_variant("GlowCompositeGuaranteeV1")),
    }
}

fn glow_layer_recipe_profile_key(
    profile: labcolors_core::GlowLayerRecipeProfileV1,
) -> Result<&'static str, BindingError> {
    match profile {
        labcolors_core::GlowLayerRecipeProfileV1::Cam16JPrimeOklabCuspV1 => {
            Ok("cam16-jprime-oklab-cusp-v1")
        }
        _ => Err(unknown_output_variant("GlowLayerRecipeProfileV1")),
    }
}

fn glow_diagnostic_profile_key(
    profile: labcolors_core::GlowDiagnosticProfileV1,
) -> Result<&'static str, BindingError> {
    match profile {
        labcolors_core::GlowDiagnosticProfileV1::Cam16UcsJPrimeLi2017V1 => {
            Ok("cam16-ucs-jprime-li2017-v1")
        }
        _ => Err(unknown_output_variant("GlowDiagnosticProfileV1")),
    }
}

fn glow_decision_profile_key(
    profile: labcolors_core::GlowDecisionProfileV1,
) -> Result<&'static str, BindingError> {
    match profile {
        labcolors_core::GlowDecisionProfileV1::StableV1 => Ok("stable-v1"),
        labcolors_core::GlowDecisionProfileV1::LegacyPlatformDependentV1 => {
            Ok("legacy-platform-dependent-v1")
        }
        _ => Err(unknown_output_variant("GlowDecisionProfileV1")),
    }
}

fn glow_target_status_key(
    status: labcolors_core::GlowTargetStatus,
) -> Result<&'static str, BindingError> {
    match status {
        labcolors_core::GlowTargetStatus::ExactNoopUnreachable => Ok("exact-noop-unreachable"),
        labcolors_core::GlowTargetStatus::LegacyReached => Ok("legacy-reached"),
        labcolors_core::GlowTargetStatus::LegacyUnreachable => Ok("legacy-unreachable"),
        _ => Err(unknown_output_variant("GlowTargetStatus")),
    }
}

fn glow_constraint_layer_key(
    layer: labcolors_core::GlowConstraintLayer,
) -> Result<&'static str, BindingError> {
    match layer {
        labcolors_core::GlowConstraintLayer::Halo => Ok("halo"),
        _ => Err(unknown_output_variant("GlowConstraintLayer")),
    }
}

fn numerical_site_id_key(
    site_id: labcolors_core::NumericalSiteIdV1,
) -> Result<&'static str, BindingError> {
    match site_id {
        labcolors_core::NumericalSiteIdV1::GlowTargetOrMaximumV1 => Ok("glow-target-or-maximum-v1"),
        _ => Err(unknown_output_variant("NumericalSiteIdV1")),
    }
}

fn material_alpha_status_key(
    status: labcolors_core::MaterialAlphaStatusV1,
) -> Result<&'static str, BindingError> {
    match status {
        labcolors_core::MaterialAlphaStatusV1::Satisfied => Ok("satisfied"),
        labcolors_core::MaterialAlphaStatusV1::Degraded => Ok("degraded"),
        _ => Err(unknown_output_variant("MaterialAlphaStatusV1")),
    }
}

fn material_numerical_profile_key(
    profile: labcolors_core::MaterialNumericalProfileV1,
) -> Result<&'static str, BindingError> {
    match profile {
        labcolors_core::MaterialNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1 => {
            Ok("encoded-srgb-byte-scale-affine-platform-binary64-powf-v1")
        }
        _ => Err(unknown_output_variant("MaterialNumericalProfileV1")),
    }
}

/// Поле сертификата определённого решения. Функция владеет всей вложенной
/// формой контракта, чтобы новый вариант сертификата не усложнял проекцию роли.
/// Wire-ключ (`bit-exact | legacy-platform-dependent-v1`) берётся из
/// core-owned `guarantee_wire_key()` — единого migration-адаптера прежнего
/// guarantee-словаря; enum non-exhaustive, поэтому неизвестный будущий вариант
/// остаётся честной структурной ошибкой, а не тихим новым ключом на проводе.
fn field_glow_decision_guarantee(
    out: &mut String,
    outcome: &labcolors_core::glow::GlowDecisionOutcomeV1,
) -> Result<(), BindingError> {
    use labcolors_core::glow::GlowDecisionOutcomeV1;
    let key = match outcome {
        GlowDecisionOutcomeV1::StableExactNoop { .. }
        | GlowDecisionOutcomeV1::Compatibility { .. } => outcome.guarantee_wire_key(),
        _ => {
            return Err(BindingError::Internal {
                reason: "проекция: неподдерживаемый GlowDecisionOutcomeV1".to_string(),
            });
        }
    };
    out.push_str(",\"decisionGuarantee\":{\"kind\":");
    push_str_lit(out, key);
    out.push('}');
    Ok(())
}

/// Поле свидетельства выбора material-alpha. Функция владеет полной вложенной
/// формой контракта и каноническим порядком полей для интервальной и обеих
/// граничных ветвей.
fn field_material_alpha_guarantee(
    out: &mut String,
    guarantee: labcolors_core::MaterialAlphaGuaranteeV1,
) -> Result<(), BindingError> {
    out.push_str(",\"alphaGuarantee\":{\"kind\":");
    match guarantee {
        labcolors_core::MaterialAlphaGuaranteeV1::BisectionBracketCharacterizedV1 {
            iterations,
            lower_alpha,
            upper_alpha,
            numerical_profile,
        } => {
            push_str_lit(out, "bisection-bracket-characterized-v1");
            field_num(out, "iterations", f64::from(iterations))?;
            field_num(out, "lowerAlpha", lower_alpha)?;
            field_num(out, "upperAlpha", upper_alpha)?;
            field_str(
                out,
                "numericalProfile",
                material_numerical_profile_key(numerical_profile)?,
            );
        }
        labcolors_core::MaterialAlphaGuaranteeV1::TransparentEndpointCharacterizedV1 {
            numerical_profile,
        } => {
            push_str_lit(out, "transparent-endpoint-characterized-v1");
            field_str(
                out,
                "numericalProfile",
                material_numerical_profile_key(numerical_profile)?,
            );
        }
        labcolors_core::MaterialAlphaGuaranteeV1::OpaqueEndpointCharacterizedV1 {
            numerical_profile,
        } => {
            push_str_lit(out, "opaque-endpoint-characterized-v1");
            field_str(
                out,
                "numericalProfile",
                material_numerical_profile_key(numerical_profile)?,
            );
        }
        _ => {
            return Err(BindingError::Internal {
                reason: "проекция: неизвестный MaterialAlphaGuaranteeV1".to_string(),
            });
        }
    }
    out.push('}');
    Ok(())
}

/// Поля свидетельства для неопределённого решения. `reason` и связанный с ним
/// `bounds` эмитятся вместе: тип остаётся единственным источником их формы.
fn fields_numerical_indeterminacy(
    out: &mut String,
    evidence: labcolors_core::NumericalIndeterminacyV1,
) -> Result<(), BindingError> {
    match evidence {
        labcolors_core::NumericalIndeterminacyV1::SoundBoundUnavailable => {
            field_str(out, "reason", "sound-bound-unavailable");
            out.push_str(",\"bounds\":{\"kind\":");
            push_str_lit(out, "unavailable");
        }
        labcolors_core::NumericalIndeterminacyV1::IntervalOverlap(interval) => {
            field_str(out, "reason", "interval-overlap");
            out.push_str(",\"bounds\":{\"kind\":");
            push_str_lit(out, "outward");
            field_num(out, "lower", interval.lower())?;
            field_num(out, "upper", interval.upper())?;
        }
        _ => {
            return Err(BindingError::Internal {
                reason: "проекция: неизвестный NumericalIndeterminacyV1".to_string(),
            });
        }
    }
    out.push('}');
    Ok(())
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
        GlowColor, GlowIndeterminateColor, MaterialColor, ResolvedTheme, RgbaColor, RoleEntry,
        RoleOutcome, SolvedColor,
    };

    /// Берёт атомарный outcome только из полного core-owned product path. Variant
    /// sealed: boundary-тест не может переупаковать чужое genuine evidence.
    fn core_glow_outcome(
        background: &str,
        profile: labcolors_core::GlowDecisionProfileV1,
    ) -> labcolors_core::glow::GlowDecisionOutcomeV1 {
        let tint =
            labcolors_core::LadderTint::new([[74.0 / 255.0, 143.0 / 255.0, 1.0]; 4]).unwrap();
        let table = labcolors_core::NamedRoleTable::new(
            vec![(
                "opaque-client-id".to_string(),
                labcolors_core::RoleSpec::Glow {
                    tint,
                    step: labcolors_core::glow::GlowStep::Base,
                    mode: profile.execution_mode(),
                },
            )],
            Vec::new(),
            labcolors_core::RoleChroma::Neutral,
        )
        .unwrap();
        let resolved = labcolors_core::resolve_named_set(
            &labcolors_core::BgInput::solid(background).unwrap(),
            &table,
            &labcolors_core::ViewingConditions::srgb(),
        );
        let labcolors_core::Resolved::Glow(glow) = &resolved[0].1 else {
            panic!("core fixture must resolve to a terminal Glow outcome");
        };
        glow.decision_outcome()
    }

    fn legacy_outcome() -> labcolors_core::glow::GlowDecisionOutcomeV1 {
        core_glow_outcome(
            "#101012",
            labcolors_core::GlowDecisionProfileV1::LegacyPlatformDependentV1,
        )
    }

    fn stable_exact_noop_outcome() -> labcolors_core::glow::GlowDecisionOutcomeV1 {
        core_glow_outcome("#FFFFFF", labcolors_core::GlowDecisionProfileV1::StableV1)
    }

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
                        layer_recipe_profile:
                            labcolors_core::GlowLayerRecipeProfileV1::Cam16JPrimeOklabCuspV1,
                        appearance_diagnostic_profile:
                            labcolors_core::GlowDiagnosticProfileV1::Cam16UcsJPrimeLi2017V1,
                        selection_diagnostic_profile: Some(
                            labcolors_core::GlowDiagnosticProfileV1::Cam16UcsJPrimeLi2017V1,
                        ),
                        decision_outcome: legacy_outcome(),
                        constraint_layer: labcolors_core::GlowConstraintLayer::Halo,
                        target_status: labcolors_core::GlowTargetStatus::LegacyReached,
                        halo_composite_hex: "#13151B".to_string(),
                        halo_achieved_dj: 2.373_123_785_729_128,
                        core_composite_hex: "#15171B".to_string(),
                        core_achieved_dj: 3.235_504_076_619_437,
                    }),
                },
                RoleEntry {
                    role_key: "impossible".to_string(),
                    outcome: RoleOutcome::Failure {
                        category: "unsupported",
                        code: "gamut_unsupported",
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
                "\"layerRecipeProfile\":\"cam16-jprime-oklab-cusp-v1\",",
                "\"appearanceDiagnosticProfile\":\"cam16-ucs-jprime-li2017-v1\",",
                "\"selectionDiagnosticProfile\":\"cam16-ucs-jprime-li2017-v1\",",
                "\"decisionProfile\":\"legacy-platform-dependent-v1\",",
                "\"decisionGuarantee\":{{\"kind\":\"legacy-platform-dependent-v1\"}},",
                "\"constraintLayer\":\"halo\",\"targetDj\":2.3006,\"targetStatus\":\"legacy-reached\",",
                "\"haloCompositeHex\":\"#13151B\",\"haloAchievedDj\":2.373123785729128,",
                "\"coreCompositeHex\":\"#15171B\",\"coreAchievedDj\":3.235504076619437,",
                "\"achievedDj\":2.373123785729128,\"degraded\":false,\"css\":\"{halo}\"}},",
                "\"impossible\":{{\"cssVar\":\"--lab-impossible\",\"kind\":\"failure\",",
                "\"category\":\"unsupported\",\"code\":\"gamut_unsupported\",",
                "\"message\":\"нет цвета: \\\"предел\\\"\\nвторая строка\"}}",
                "}}}}"
            ),
            lab = css_label,
            veil = css_veil,
            halo = css_halo,
            core = css_core,
        );
        assert_eq!(json, expected);

        // Anti-vacuum: `kind: none` публикует client-owned `cssVar` как
        // метаданные контракта, но никогда не присваивает этому имени значение.
        let projected: serde_json::Value = serde_json::from_str(&json).unwrap();
        let none = &projected["roles"]["spacer"];
        assert_eq!(none["kind"], "none");
        assert_eq!(none["cssVar"], "--lab-spacer");
        assert!(projected["vars"].get("--lab-spacer").is_none());

        // Failure is one terminal wire shape. It preserves the core-owned
        // category/code, emits no CSS value, and carries no superseded alias.
        let failure = &projected["roles"]["impossible"];
        assert_eq!(failure["kind"], "failure");
        assert_eq!(failure["category"], "unsupported");
        assert_eq!(failure["code"], "gamut_unsupported");
        assert!(projected["vars"].get("--lab-impossible").is_none());
        let mut failure_fields = failure
            .as_object()
            .expect("failure role is an object")
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        failure_fields.sort_unstable();
        assert_eq!(
            failure_fields,
            ["category", "code", "cssVar", "kind", "message"]
        );
    }

    #[test]
    fn interval_overlap_indeterminacy_survives_json_projection() {
        let interval = labcolors_core::OutwardIntervalV1::try_new(0.9, 1.1).unwrap();
        let mut theme = fixture();
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
        let uncertain = &value["roles"]["uncertain-pulse"];
        assert_eq!(uncertain["reason"], "interval-overlap");
        assert_eq!(uncertain["bounds"]["kind"], "outward");
        assert_eq!(uncertain["bounds"]["lower"].as_f64(), Some(0.9));
        assert_eq!(uncertain["bounds"]["upper"].as_f64(), Some(1.1));
    }

    #[test]
    fn numerical_certificate_serializers_keep_canonical_wire_order() {
        let interval = labcolors_core::OutwardIntervalV1::try_new(0.9, 1.1).unwrap();
        let numerical_profile = labcolors_core::MaterialNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1;

        let mut exact_decision = String::new();
        field_glow_decision_guarantee(&mut exact_decision, &stable_exact_noop_outcome()).unwrap();
        assert_eq!(
            exact_decision,
            ",\"decisionGuarantee\":{\"kind\":\"bit-exact\"}"
        );

        let mut legacy_decision = String::new();
        field_glow_decision_guarantee(&mut legacy_decision, &legacy_outcome()).unwrap();
        assert_eq!(
            legacy_decision,
            ",\"decisionGuarantee\":{\"kind\":\"legacy-platform-dependent-v1\"}"
        );

        let mut unavailable = String::new();
        fields_numerical_indeterminacy(
            &mut unavailable,
            labcolors_core::NumericalIndeterminacyV1::SoundBoundUnavailable,
        )
        .unwrap();
        assert_eq!(
            unavailable,
            ",\"reason\":\"sound-bound-unavailable\",\"bounds\":{\"kind\":\"unavailable\"}"
        );

        let mut overlap = String::new();
        fields_numerical_indeterminacy(
            &mut overlap,
            labcolors_core::NumericalIndeterminacyV1::IntervalOverlap(interval),
        )
        .unwrap();
        assert_eq!(
            overlap,
            ",\"reason\":\"interval-overlap\",\"bounds\":{\"kind\":\"outward\",\"lower\":0.9,\"upper\":1.1}"
        );

        let mut bracket = String::new();
        field_material_alpha_guarantee(
            &mut bracket,
            labcolors_core::MaterialAlphaGuaranteeV1::BisectionBracketCharacterizedV1 {
                iterations: 60,
                lower_alpha: 0.6374,
                upper_alpha: 0.6375,
                numerical_profile,
            },
        )
        .unwrap();
        assert_eq!(
            bracket,
            ",\"alphaGuarantee\":{\"kind\":\"bisection-bracket-characterized-v1\",\"iterations\":60,\"lowerAlpha\":0.6374,\"upperAlpha\":0.6375,\"numericalProfile\":\"encoded-srgb-byte-scale-affine-platform-binary64-powf-v1\"}"
        );

        let mut transparent_endpoint = String::new();
        field_material_alpha_guarantee(
            &mut transparent_endpoint,
            labcolors_core::MaterialAlphaGuaranteeV1::TransparentEndpointCharacterizedV1 {
                numerical_profile,
            },
        )
        .unwrap();
        assert_eq!(
            transparent_endpoint,
            ",\"alphaGuarantee\":{\"kind\":\"transparent-endpoint-characterized-v1\",\"numericalProfile\":\"encoded-srgb-byte-scale-affine-platform-binary64-powf-v1\"}"
        );

        let mut opaque_endpoint = String::new();
        field_material_alpha_guarantee(
            &mut opaque_endpoint,
            labcolors_core::MaterialAlphaGuaranteeV1::OpaqueEndpointCharacterizedV1 {
                numerical_profile,
            },
        )
        .unwrap();
        assert_eq!(
            opaque_endpoint,
            ",\"alphaGuarantee\":{\"kind\":\"opaque-endpoint-characterized-v1\",\"numericalProfile\":\"encoded-srgb-byte-scale-affine-platform-binary64-powf-v1\"}"
        );
    }

    #[test]
    fn adapter_owns_every_closed_output_vocabulary() {
        assert_eq!(
            glow_composite_profile_key(
                labcolors_core::GlowCompositeProfileV1::EncodedSrgb8ScreenV1,
            )
            .unwrap(),
            "encoded-srgb8-screen-v1"
        );
        assert_eq!(
            glow_composite_guarantee_key(labcolors_core::GlowCompositeGuaranteeV1::BitExact)
                .unwrap(),
            "bit-exact"
        );
        assert_eq!(
            glow_layer_recipe_profile_key(
                labcolors_core::GlowLayerRecipeProfileV1::Cam16JPrimeOklabCuspV1,
            )
            .unwrap(),
            "cam16-jprime-oklab-cusp-v1"
        );
        assert_eq!(
            glow_diagnostic_profile_key(
                labcolors_core::GlowDiagnosticProfileV1::Cam16UcsJPrimeLi2017V1,
            )
            .unwrap(),
            "cam16-ucs-jprime-li2017-v1"
        );
        assert_eq!(
            glow_decision_profile_key(labcolors_core::GlowDecisionProfileV1::StableV1).unwrap(),
            "stable-v1"
        );
        assert_eq!(
            glow_decision_profile_key(
                labcolors_core::GlowDecisionProfileV1::LegacyPlatformDependentV1,
            )
            .unwrap(),
            "legacy-platform-dependent-v1"
        );
        assert_eq!(
            glow_target_status_key(labcolors_core::GlowTargetStatus::ExactNoopUnreachable).unwrap(),
            "exact-noop-unreachable"
        );
        assert_eq!(
            glow_target_status_key(labcolors_core::GlowTargetStatus::LegacyReached).unwrap(),
            "legacy-reached"
        );
        assert_eq!(
            glow_target_status_key(labcolors_core::GlowTargetStatus::LegacyUnreachable).unwrap(),
            "legacy-unreachable"
        );
        assert_eq!(
            glow_constraint_layer_key(labcolors_core::GlowConstraintLayer::Halo).unwrap(),
            "halo"
        );
        assert_eq!(
            numerical_site_id_key(labcolors_core::NumericalSiteIdV1::GlowTargetOrMaximumV1)
                .unwrap(),
            "glow-target-or-maximum-v1"
        );
        assert_eq!(
            material_alpha_status_key(labcolors_core::MaterialAlphaStatusV1::Satisfied).unwrap(),
            "satisfied"
        );
        assert_eq!(
            material_alpha_status_key(labcolors_core::MaterialAlphaStatusV1::Degraded).unwrap(),
            "degraded"
        );
        assert_eq!(
            material_numerical_profile_key(
                labcolors_core::MaterialNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1,
            )
            .unwrap(),
            "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1"
        );

        // Оба конструктивно допустимых атомарных исхода несут ровно прежние
        // wire-ключи guarantee/profile — словарь на проводе не дрейфует.
        assert_eq!(
            stable_exact_noop_outcome().guarantee_wire_key(),
            "bit-exact"
        );
        assert_eq!(
            stable_exact_noop_outcome().decision_profile(),
            labcolors_core::GlowDecisionProfileV1::StableV1
        );
        assert_eq!(
            legacy_outcome().guarantee_wire_key(),
            "legacy-platform-dependent-v1"
        );
        assert_eq!(
            legacy_outcome().decision_profile(),
            labcolors_core::GlowDecisionProfileV1::LegacyPlatformDependentV1
        );
    }

    #[test]
    fn capability_manifest_json_mirrors_proof_capable_core_ssot() {
        let value: serde_json::Value =
            serde_json::from_str(&capability_manifest_json()).expect("валидный JSON");
        let core = labcolors_core::numerical_capability_manifest_v2();
        assert_eq!(value["schemaVersion"], 2);
        assert_eq!(value["coverage"], core.coverage.key());
        assert_eq!(value["checksum"], core.checksum.hex());
        let sites = value["sites"].as_array().expect("sites — массив");
        assert_eq!(sites.len(), core.sites.len());
        for (projected, expected) in sites.iter().zip(core.sites.iter()) {
            assert_eq!(projected["siteId"], expected.site_id.key());
            let proof_ids: Vec<_> = projected["proofIds"]
                .as_array()
                .expect("V2 proofIds — явный массив")
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect();
            assert_eq!(
                proof_ids,
                expected
                    .proof_ids
                    .iter()
                    .map(|value| value.key())
                    .collect::<Vec<_>>()
            );
        }
        assert!(sites.iter().any(|site| {
            site["siteId"] == "wcag22-srgb8-contrast-v1"
                && site["proofIds"][0] == "wcag22-srgb8-full-domain-q55-v1"
        }));
    }

    /// Материал (whitepaper §3.7) проецируется в контрактные CSS-переменные: `--lab-<role>` =
    /// солид-канон (oklch), `--lab-<role>-01` = тинт (oklch со слэш-альфой),
    /// `--lab-<role>-02` = опаковая база (oklch). Плюс полный набор полей исхода.
    /// Пин ИМЁН переменных — публичный CSS-контракт сателлитов материала.
    /// Канонический labui.config.json сейчас НЕ содержит Material-рецептов:
    /// capability generic/synthetic и закреплена release/projection-фикстурами.
    #[test]
    fn material_projects_two_layer_css_vars() {
        let theme = ResolvedTheme {
            theme: "light",
            background: "#FFFFFF".to_string(),
            roles: vec![RoleEntry {
                role_key: "bg-material-base".to_string(),
                outcome: RoleOutcome::Material(MaterialColor {
                    tone_hex: "#B4B4BC".to_string(),
                    alpha: 0.6375,
                    worst_contrast: 4.61,
                    alpha_guarantee:
                        labcolors_core::MaterialAlphaGuaranteeV1::BisectionBracketCharacterizedV1 {
                            iterations: 60,
                            lower_alpha: 0.6374,
                            upper_alpha: 0.6375,
                            numerical_profile:
                                labcolors_core::MaterialNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1,
                        },
                    alpha_status: labcolors_core::MaterialAlphaStatusV1::Satisfied,
                    floor: 4.5,
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
        assert_eq!(
            r["alphaGuarantee"]["kind"],
            "bisection-bracket-characterized-v1"
        );
        assert_eq!(r["alphaGuarantee"]["iterations"], 60);
        assert_eq!(r["alphaGuarantee"]["lowerAlpha"], 0.6374);
        assert_eq!(r["alphaGuarantee"]["upperAlpha"], 0.6375);
        assert_eq!(
            r["alpha"].as_f64().unwrap().to_bits(),
            r["alphaGuarantee"]["upperAlpha"]
                .as_f64()
                .unwrap()
                .to_bits(),
            "serialized alpha обязан побитно совпадать с проходящей upperAlpha"
        );
        assert_eq!(
            r["alphaGuarantee"]["numericalProfile"],
            "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1"
        );
        assert_eq!(r["alphaStatus"], "satisfied");
        assert_eq!(r["floor"].as_f64().unwrap(), 4.5);
        assert_eq!(r["guaranteed"], true);
        assert_eq!(r["poleWhite"], false);
        assert_eq!(r["achievedDj"].as_f64().unwrap(), 18.25);
        assert_eq!(r["toneCompressed"], false);
        assert_eq!(r["hueVanished"], false);
        assert_eq!(r["distinct"], true);
        assert_eq!(r["css"].as_str().unwrap(), solid);
    }

    #[test]
    fn material_terminal_variants_keep_status_guarantee_and_boolean_correlated() {
        let numerical_profile = labcolors_core::MaterialNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1;
        let role = |alpha, alpha_guarantee, alpha_status| {
            RoleOutcome::Material(MaterialColor {
                tone_hex: "#B4B4BC".to_string(),
                alpha,
                worst_contrast: 4.61,
                alpha_guarantee,
                alpha_status,
                floor: 4.5,
                pole_white: false,
                achieved_dj: 18.25,
                tone_compressed: false,
                hue_vanished: false,
                distinct: true,
            })
        };
        let theme = ResolvedTheme {
            theme: "light",
            background: "#FFFFFF".to_string(),
            roles: vec![
                RoleEntry {
                    role_key: "transparent".to_string(),
                    outcome: role(
                        0.0,
                        labcolors_core::MaterialAlphaGuaranteeV1::TransparentEndpointCharacterizedV1 {
                            numerical_profile,
                        },
                        labcolors_core::MaterialAlphaStatusV1::Satisfied,
                    ),
                },
                RoleEntry {
                    role_key: "opaque".to_string(),
                    outcome: role(
                        1.0,
                        labcolors_core::MaterialAlphaGuaranteeV1::OpaqueEndpointCharacterizedV1 {
                            numerical_profile,
                        },
                        labcolors_core::MaterialAlphaStatusV1::Degraded,
                    ),
                },
            ],
        };

        let value: serde_json::Value =
            serde_json::from_str(&resolved_json(&theme).unwrap()).unwrap();
        let transparent = &value["roles"]["transparent"];
        assert_eq!(transparent["alpha"], 0.0);
        assert_eq!(
            transparent["alphaGuarantee"]["kind"],
            "transparent-endpoint-characterized-v1"
        );
        assert_eq!(transparent["alphaStatus"], "satisfied");
        assert_eq!(transparent["guaranteed"], true);

        let opaque = &value["roles"]["opaque"];
        assert_eq!(opaque["alpha"], 1.0);
        assert_eq!(
            opaque["alphaGuarantee"]["kind"],
            "opaque-endpoint-characterized-v1"
        );
        assert_eq!(opaque["alphaStatus"], "degraded");
        assert_eq!(opaque["guaranteed"], false);
    }

    #[test]
    fn projection_rejects_illegal_output_cross_products() {
        let mut glow_theme = fixture();
        let RoleOutcome::Glow(glow) = &mut glow_theme.roles[3].outcome else {
            panic!("fixture pulse must be Glow");
        };
        // Незаконная пара profile × guarantee непредставима атомарным
        // decision_outcome; осталась представимой лишь рассинхронизация
        // outcome ↔ status/selection — её проекция и обязана отвергать.
        glow.decision_outcome = stable_exact_noop_outcome();
        let glow_error = resolved_json(&glow_theme).unwrap_err();
        assert!(matches!(
            glow_error,
            BindingError::Internal { reason } if reason.contains("несогласованный Glow provenance")
        ));

        let indeterminate_theme = ResolvedTheme {
            theme: "light",
            background: "#FFFFFF".to_string(),
            roles: vec![RoleEntry {
                role_key: "indeterminate".to_string(),
                outcome: RoleOutcome::GlowIndeterminate(GlowIndeterminateColor {
                    source_hex: "#4A8FFF".to_string(),
                    target_dj: 2.3006,
                    decision_profile:
                        labcolors_core::GlowDecisionProfileV1::LegacyPlatformDependentV1,
                    site_id: labcolors_core::NumericalSiteIdV1::GlowTargetOrMaximumV1,
                    evidence: labcolors_core::NumericalIndeterminacyV1::SoundBoundUnavailable,
                    constraint_layer: labcolors_core::GlowConstraintLayer::Halo,
                }),
            }],
        };
        let indeterminate_error = resolved_json(&indeterminate_theme).unwrap_err();
        assert!(matches!(
            indeterminate_error,
            BindingError::Internal { reason }
                if reason.contains("несогласованный GlowIndeterminate provenance")
        ));

        let numerical_profile = labcolors_core::MaterialNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1;
        let material_theme = ResolvedTheme {
            theme: "light",
            background: "#FFFFFF".to_string(),
            roles: vec![RoleEntry {
                role_key: "material".to_string(),
                outcome: RoleOutcome::Material(MaterialColor {
                    tone_hex: "#B4B4BC".to_string(),
                    alpha: 0.5,
                    worst_contrast: 4.61,
                    alpha_guarantee:
                        labcolors_core::MaterialAlphaGuaranteeV1::BisectionBracketCharacterizedV1 {
                            iterations: 60,
                            lower_alpha: 0.49,
                            upper_alpha: 0.75,
                            numerical_profile,
                        },
                    alpha_status: labcolors_core::MaterialAlphaStatusV1::Satisfied,
                    floor: 4.5,
                    pole_white: false,
                    achieved_dj: 18.25,
                    tone_compressed: false,
                    hue_vanished: false,
                    distinct: true,
                }),
            }],
        };
        let material_error = resolved_json(&material_theme).unwrap_err();
        assert!(matches!(
            material_error,
            BindingError::Internal { reason }
                if reason.contains("несогласованный Material provenance")
        ));
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

    #[test]
    fn wcag22_projection_preserves_exact_ids_bytes_and_u64_as_strings() {
        let assessment = labcolors_core::wcag22::evaluate_wcag22_hex(
            "#89BB09",
            "#8212DB",
            labcolors_core::wcag22::Wcag22CriterionV1::Sc1411GraphicalObject,
        )
        .unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&wcag22_json(&assessment).unwrap()).unwrap();
        assert_eq!(value["decision"], "fail");
        assert_eq!(value["foreground"], "#89BB09");
        assert_eq!(value["background"], "#8212DB");
        assert_eq!(
            value["evidence"]["artifactId"],
            "wcag22-srgb8-luminance-q55-v1"
        );
        assert!(value["q55Scale"].as_str().unwrap().parse::<u64>().is_ok());
        assert!(
            value["foregroundLuminanceQ55"]["lower"]
                .as_str()
                .unwrap()
                .parse::<u64>()
                .is_ok()
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
