//! Нативная ABI-граница ядра `labcolors-core` через UniFFI. Сгенерированная
//! Swift-поверхность доказывает динамический вызов ядра в текущем активном
//! Linux x86_64 conformance-гейте; Apple ABI, macOS/arm64 и iOS этим прогоном
//! не аттестованы.
//!
//! # Архитектурный закон
//!
//! Рантайм-ядро Rust — обязательный слой на КАЖДОЙ платформе; запечённые
//! build-time токены — лишь опция, не архитектура. Этот крейт зовёт ядро В
//! РАНТАЙМЕ (через FFI), а не сериализует его выход на этапе сборки. Swift
//! получает те же перцептивные вычисления (CIECAM16 / LCS / LPC / WCAG), что и
//! WASM-граница — из одного и того же Rust-ядра.
//!
//! # Срез API — «рантайм-контраст-ядро»
//!
//! Экспортируется минимальный ОСМЫСЛЕННЫЙ срез — ровно примитивы, которые зовёт
//! реактивный рантайм, и ровно те, что покрывают семейства conformance-пака:
//!
//! | Экспорт | Семейство пака | Функция ядра |
//! |---------|----------------|--------------|
//! | [`contrast`] / [`recheck`] | `contrasts` | `recheck_against` |
//! | [`solve_contrast`] | `solve` | `solve` |
//! | [`ladder_alpha`] | `ladders` | `LadderPosition::alpha_pair` |
//! | [`composite`] / [`min_alpha`] | `alpha` | `alpha::composite_hex` / `alpha::min_alpha_hex` |
//! | [`muddiness`] | `muddiness` | `cleanliness::muddiness_from_hex` |
//! | [`core_version`] | `manifest` | версия ядра |
//!
//! [`solve_glow_point`] — отдельный low-level contract test нативной границы:
//! stable-профиль переносит `Indeterminate` с site + typed evidence без fallback,
//! legacy выбирается только явно. Его CAM16-диагностика не объявлена
//! bit-exact между платформами; `bit-exact` относится лишь к certificate
//! encoded-sRGB8 screen-композитора.
//!
//! Резолв полной темы из JSON-конфига (агностичный движок) СОЗНАТЕЛЬНО вне
//! среза: он требует serde-границы конфига (живёт в WASM-крейте) — перенос её
//! в нативный крейт либо дублировал бы её (риск дрейфа), либо тянул бы
//! wasm-bindgen в нативный граф. Это отдельный следующий шаг (см. PR).
//!
//! Хью/хрома у [`solve_contrast`] фиксированы нейтралью (серый) — резолв
//! хью-независим, срез детерминирован; параметризация хью — естественное
//! расширение.

use labcolors_core::alpha::{composite_hex, min_alpha_hex};
use labcolors_core::cleanliness::muddiness_from_hex;
use labcolors_core::{
    BgInput, ChromaPolicy, Contract, DecisionGuaranteeV1, Gamut, GlowCompositeGuaranteeV1,
    GlowCompositeProfileV1, GlowDecisionProfileV1, GlowDiagnosticProfileV1,
    GlowTargetStatus as CoreGlowTargetStatus, Hue, LadderPosition, NumericalDecisionV1,
    NumericalIndeterminacyV1, NumericalSiteIdV1, Theme as CoreTheme, ViewingConditions,
    recheck_against, solve, solve_screen_alpha_for_dj,
};

// Регистрирует UniFFI-scaffolding под namespace = имя крейта (`labcolors`).
// Обязателен для чисто-proc-macro крейта.
uniffi::setup_scaffolding!();

// ─────────────────────────────────────────────────────────────────────────────
// Словарь границы
// ─────────────────────────────────────────────────────────────────────────────

/// Тема просмотра — стабильный словарь границы (`light` / `dark` / `light-ic` /
/// `dark-ic`). Отображается в канонические условия просмотра ядра.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Theme {
    /// Светлая: average surround.
    Light,
    /// Тёмная: dim surround.
    Dark,
    /// Светлая, повышенный контраст.
    LightIc,
    /// Тёмная, повышенный контраст.
    DarkIc,
}

impl Theme {
    fn to_core(self) -> CoreTheme {
        match self {
            Theme::Light => CoreTheme::Light,
            Theme::Dark => CoreTheme::Dark,
            Theme::LightIc => CoreTheme::LightIc,
            Theme::DarkIc => CoreTheme::DarkIc,
        }
    }

    fn vc(self) -> ViewingConditions {
        self.to_core().viewing_conditions()
    }
}

/// Контракт резолва — какой контраст обязан достичь передний план.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum ContractSpec {
    /// Текст: цель `Lc`, юридический пол WCAG AA-text (4.5:1).
    Text {
        /// Целевой перцептивный `Lc`.
        lc: f64,
    },
    /// UI-элемент: цель `Lc`, пол WCAG AA-UI (3:1).
    Ui {
        /// Целевой перцептивный `Lc`.
        lc: f64,
    },
    /// Декоративная полоса `[floor, ceiling]` без юридического пола.
    Range {
        /// Нижняя граница `Lc`.
        floor: f64,
        /// Верхняя граница `Lc`.
        ceiling: f64,
    },
}

impl ContractSpec {
    fn to_core(self) -> Contract {
        match self {
            ContractSpec::Text { lc } => Contract::text(lc),
            ContractSpec::Ui { lc } => Contract::ui(lc),
            ContractSpec::Range { floor, ceiling } => Contract::range(floor, ceiling),
        }
    }
}

/// Пара контрастов переднего плана на фоне: перцептивный `Lc` и юридический
/// WCAG-ratio. Отчитываются РАЗДЕЛЬНО (инвариант ядра — не смешивать).
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct Contrast {
    /// Знаковый перцептивный контраст (LPC `Lc`).
    pub lc: f64,
    /// Контраст-ratio WCAG 2.1 (1–21).
    pub wcag_ratio: f64,
}

/// Резолвнутый цвет и достигнутые им контрасты.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Solved {
    /// Резолвнутый цвет, `#RRGGBB`.
    pub hex: String,
    /// Знаковый перцептивный `Lc` на отданном hex.
    pub lc: f64,
    /// WCAG-ratio на отданном hex.
    pub wcag_ratio: f64,
    /// Юридический пол переопределил перцептивную цель.
    pub floor_override: bool,
}

/// Explicit profile numerical Glow decision; default намеренно отсутствует.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum GlowDecisionProfile {
    /// Stable path: без sound bound возвращает Indeterminate.
    StableV1,
    /// Явный прежний CAM16/libm-dependent runtime path.
    LegacyPlatformDependentV1,
}

impl GlowDecisionProfile {
    fn to_core(self) -> GlowDecisionProfileV1 {
        match self {
            Self::StableV1 => GlowDecisionProfileV1::StableV1,
            Self::LegacyPlatformDependentV1 => GlowDecisionProfileV1::LegacyPlatformDependentV1,
        }
    }
}

/// Guarantee semantic target/max decision с неразделимым evidence.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum GlowDecisionGuarantee {
    /// Decision следует из exact integer/rational state.
    BitExact,
    /// Decision следует из доказанного непересекающегося outward interval.
    OutwardIntervalV1 {
        /// Нижняя граница.
        lower: f64,
        /// Верхняя граница.
        upper: f64,
    },
    /// Explicit legacy CAM16/libm-dependent path.
    LegacyPlatformDependentV1,
}

/// Exact point-composite profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum GlowCompositeProfile {
    /// Encoded sRGB8 screen v1.
    EncodedSrgb8ScreenV1,
}

/// Exact point-composite guarantee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum GlowCompositeGuarantee {
    /// Byte-exact certificate.
    BitExact,
}

/// Appearance diagnostic identity; не decision guarantee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum GlowDiagnosticProfile {
    /// CAM16-UCS J-prime transcription from Li et al. 2017.
    Cam16UcsJPrimeLi2017V1,
}

/// Результат проверки target по конечному point-composite домену.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum GlowTargetStatus {
    /// Target достигнут.
    Reached,
    /// Target недостижим; выбран максимум только в explicit legacy profile.
    Unreachable,
}

/// Зарегистрированный branch-sensitive site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NumericalSiteId {
    /// Glow target-or-maximum selection.
    GlowTargetOrMaximumV1,
}

/// Неразделимая причина Indeterminate вместе с её evidence.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum NumericalIndeterminacy {
    /// Для site нет sound error bound.
    SoundBoundUnavailable,
    /// Sound outward interval пересекает semantic boundary.
    IntervalOverlap {
        /// Нижняя доказанная граница.
        lower: f64,
        /// Верхняя доказанная граница.
        upper: f64,
    },
}

/// Low-level point Glow decision, одинаково типизированный для Rust/Swift/WASM.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum GlowPointDecision {
    /// Selected state с раздельными decision/composite guarantees.
    Determinate {
        /// Explicit request profile.
        decision_profile: GlowDecisionProfile,
        /// Guarantee semantic target/max decision.
        decision_guarantee: GlowDecisionGuarantee,
        /// Exact composite profile.
        composite_profile: GlowCompositeProfile,
        /// Exact composite guarantee.
        composite_guarantee: GlowCompositeGuarantee,
        /// Diagnostic appearance identity.
        diagnostic_profile: Option<GlowDiagnosticProfile>,
        /// Canonical layer alpha.
        alpha: f64,
        /// Shortest-roundtrip CSS alpha.
        alpha_css: String,
        /// Requested target.
        target_dj: f64,
        /// Legacy/exact target status.
        target_status: GlowTargetStatus,
        /// Diagnostic achieved value.
        achieved_dj: f64,
        /// Exact point composite.
        composite_hex: String,
    },
    /// No selected state; caller gets typed site + evidence and no fallback.
    Indeterminate {
        /// Explicit request profile.
        decision_profile: GlowDecisionProfile,
        /// Registered site.
        site_id: NumericalSiteId,
        /// Typed reason and its sound interval, if present.
        evidence: NumericalIndeterminacy,
    },
}

/// Ошибки границы — сматчиваемые на Swift-стороне (бросаются как исключения).
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ColorError {
    /// Невалидный hex-цвет на входе.
    #[error("невалидный цвет: {reason}")]
    InvalidColor {
        /// Человекочитаемая причина.
        reason: String,
    },
    /// Альфа вне `[0,1]` или не конечна.
    #[error("альфа вне [0,1]: {reason}")]
    InvalidAlpha {
        /// Человекочитаемая причина.
        reason: String,
    },
    /// Неизвестный ключ позиции лестницы.
    #[error("неизвестная позиция лестницы: {key}")]
    UnknownLadderPosition {
        /// Запрошенный ключ.
        key: String,
    },
    /// Контракт недостижим; `code` — стабильная причина (тождественна кодам
    /// WASM-границы: `floor_unreachable`, `exceeds_range`, …).
    #[error("контракт недостижим: {code}")]
    Unreachable {
        /// Стабильный машинный код причины.
        code: String,
    },
    /// Невалидный low-level Glow request или неизвестный forward variant.
    #[error("невалидный glow request: {reason}")]
    InvalidGlowRequest {
        /// Человекочитаемая причина.
        reason: String,
    },
}

/// Стабильный код недостижимости — тождественен маппингу WASM-границы
/// (`labcolors-wasm/src/engine.rs`) и conformance-пака. Общий контракт имён для
/// ВСЕХ биндингов: одна причина → один код на любой платформе.
fn unreachable_code(err: &labcolors_core::Unreachable) -> &'static str {
    use labcolors_core::Unreachable as U;
    match err {
        U::BelowContrastFloor { .. } => "below_contrast_floor",
        U::ExceedsRange { .. } => "exceeds_range",
        U::QuantizationGap { .. } => "quantization_gap",
        U::FloorUnreachable { .. } => "floor_unreachable",
        U::PolarityMismatch { .. } => "polarity_mismatch",
        U::GamutUnsupported => "gamut_unsupported",
        U::InvalidInput(_) => "invalid_input",
        // `Unreachable` помечен `#[non_exhaustive]`; forward-compat-арм.
        _ => "unreachable",
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Экспорты — рантайм-контраст-ядро
// ─────────────────────────────────────────────────────────────────────────────

/// Версия ядра, к которой привязан этот биндинг (и conformance-пак). Все крейты
/// воркспейса делят одну версию — привязка пак ⇄ ядро ⇄ биндинг однозначна.
#[uniffi::export]
#[must_use]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Контраст одного переднего плана на фоне под темой — `(Lc, WCAG)`. Дешёвый
/// пер-кадровый примитив: один forward перцептивной модели для фона плюс один
/// для переднего плана.
///
/// # Errors
///
/// [`ColorError::InvalidColor`] на невалидном hex.
#[uniffi::export]
pub fn contrast(fg: String, bg: String, theme: Theme) -> Result<Contrast, ColorError> {
    let vc = theme.vc();
    let mut pairs =
        recheck_against(&bg, &[&fg], &vc).map_err(|reason| ColorError::InvalidColor { reason })?;
    let (lc, wcag_ratio) = pairs.pop().expect("ровно один передний план");
    Ok(Contrast { lc, wcag_ratio })
}

/// Перепроверка контрастов многих передних планов на одном фоне под темой.
/// Фон платит свой forward один раз на весь батч — примитив реактивного
/// рантайма для решения «прошли ли уже резолвнутые цвета против сменившегося
/// фона».
///
/// # Errors
///
/// [`ColorError::InvalidColor`] на невалидном hex (фон или любой передний план).
#[uniffi::export]
pub fn recheck(bg: String, fgs: Vec<String>, theme: Theme) -> Result<Vec<Contrast>, ColorError> {
    let vc = theme.vc();
    let fg_refs: Vec<&str> = fgs.iter().map(String::as_str).collect();
    let pairs = recheck_against(&bg, &fg_refs, &vc)
        .map_err(|reason| ColorError::InvalidColor { reason })?;
    Ok(pairs
        .into_iter()
        .map(|(lc, wcag_ratio)| Contrast { lc, wcag_ratio })
        .collect())
}

/// Резолв переднего плана, удовлетворяющего `contract` на фоне `bg` под темой.
/// Хрома нейтральна (серый) — атомарный резолв, хью-независимый.
///
/// # Errors
///
/// [`ColorError::InvalidColor`] на невалидном hex фона;
/// [`ColorError::Unreachable`] со стабильным кодом, если ни один цвет не
/// удовлетворяет контракт (честный отказ, не тихий клип).
#[uniffi::export]
pub fn solve_contrast(
    bg: String,
    contract: ContractSpec,
    theme: Theme,
) -> Result<Solved, ColorError> {
    let vc = theme.vc();
    let bg_input = BgInput::solid(&bg).map_err(|e| ColorError::InvalidColor {
        reason: e.to_string(),
    })?;
    match solve(
        bg_input,
        contract.to_core(),
        Hue::deg(0.0),
        ChromaPolicy::Neutral,
        &vc,
        Gamut::Srgb,
    ) {
        Ok(s) => Ok(Solved {
            hex: s.hex().to_string(),
            lc: s.lc(),
            wcag_ratio: s.wcag_ratio(),
            floor_override: s.floor_override(),
        }),
        Err(e) => Err(ColorError::Unreachable {
            code: unreachable_code(&e).to_string(),
        }),
    }
}

/// Альфа позиции лестницы под темой. `position` — стабильный kebab-ключ
/// (`label-secondary`, `fill-primary`, …).
///
/// # Errors
///
/// [`ColorError::UnknownLadderPosition`] на неизвестном ключе.
#[uniffi::export]
pub fn ladder_alpha(position: String, theme: Theme) -> Result<f64, ColorError> {
    let pos = LadderPosition::ALL
        .iter()
        .copied()
        .find(|p| p.key() == position)
        .ok_or(ColorError::UnknownLadderPosition {
            key: position.clone(),
        })?;
    Ok(pos.alpha_for_vc(&theme.vc()))
}

/// Прямой ход подложка→α: композит `α·tint + (1−α)·bg`, квантованный до
/// `#RRGGBB` (закон Figma/браузера в кодированном sRGB).
///
/// # Errors
///
/// [`ColorError::InvalidAlpha`] при α вне `[0,1]`; [`ColorError::InvalidColor`]
/// на невалидном hex.
#[uniffi::export]
pub fn composite(tint: String, alpha: f64, bg: String) -> Result<String, ColorError> {
    composite_hex(&tint, alpha, &bg).map_err(|reason| {
        if reason.starts_with("alpha") {
            ColorError::InvalidAlpha { reason }
        } else {
            ColorError::InvalidColor { reason }
        }
    })
}

/// Минимально разрешимая α: нижняя граница, при которой тинт остаётся в гамуте
/// над фоном (граница инверсии композита).
///
/// # Errors
///
/// [`ColorError::InvalidColor`] на невалидном hex.
#[uniffi::export]
pub fn min_alpha(tint: String, bg: String) -> Result<f64, ColorError> {
    min_alpha_hex(&tint, &bg).map_err(|reason| ColorError::InvalidColor { reason })
}

/// Point Glow solve с обязательным numerical profile. Stable uncertainty
/// возвращается data-вариантом `Indeterminate`, не ошибкой и не fallback.
///
/// # Errors
///
/// [`ColorError::InvalidGlowRequest`] на недоменном input/internal variant.
fn unknown_glow_variant(name: &str) -> ColorError {
    ColorError::InvalidGlowRequest {
        reason: format!("unknown core {name} variant"),
    }
}

fn decision_guarantee_to_ffi(
    guarantee: DecisionGuaranteeV1,
) -> Result<GlowDecisionGuarantee, ColorError> {
    match guarantee {
        DecisionGuaranteeV1::BitExact => Ok(GlowDecisionGuarantee::BitExact),
        DecisionGuaranteeV1::OutwardIntervalV1(interval) => {
            Ok(GlowDecisionGuarantee::OutwardIntervalV1 {
                lower: interval.lower(),
                upper: interval.upper(),
            })
        }
        DecisionGuaranteeV1::LegacyPlatformDependentV1 => {
            Ok(GlowDecisionGuarantee::LegacyPlatformDependentV1)
        }
        _ => Err(unknown_glow_variant("DecisionGuaranteeV1")),
    }
}

fn composite_profile_to_ffi(
    profile: GlowCompositeProfileV1,
) -> Result<GlowCompositeProfile, ColorError> {
    match profile {
        GlowCompositeProfileV1::EncodedSrgb8ScreenV1 => {
            Ok(GlowCompositeProfile::EncodedSrgb8ScreenV1)
        }
        _ => Err(unknown_glow_variant("GlowCompositeProfileV1")),
    }
}

fn composite_guarantee_to_ffi(
    guarantee: GlowCompositeGuaranteeV1,
) -> Result<GlowCompositeGuarantee, ColorError> {
    match guarantee {
        GlowCompositeGuaranteeV1::BitExact => Ok(GlowCompositeGuarantee::BitExact),
        _ => Err(unknown_glow_variant("GlowCompositeGuaranteeV1")),
    }
}

fn diagnostic_profile_to_ffi(
    profile: GlowDiagnosticProfileV1,
) -> Result<GlowDiagnosticProfile, ColorError> {
    match profile {
        GlowDiagnosticProfileV1::Cam16UcsJPrimeLi2017V1 => {
            Ok(GlowDiagnosticProfile::Cam16UcsJPrimeLi2017V1)
        }
        _ => Err(unknown_glow_variant("GlowDiagnosticProfileV1")),
    }
}

fn target_status_to_ffi(status: CoreGlowTargetStatus) -> Result<GlowTargetStatus, ColorError> {
    match status {
        CoreGlowTargetStatus::Reached => Ok(GlowTargetStatus::Reached),
        CoreGlowTargetStatus::Unreachable => Ok(GlowTargetStatus::Unreachable),
        _ => Err(unknown_glow_variant("GlowTargetStatus")),
    }
}

fn numerical_site_to_ffi(site: NumericalSiteIdV1) -> Result<NumericalSiteId, ColorError> {
    match site {
        NumericalSiteIdV1::GlowTargetOrMaximumV1 => Ok(NumericalSiteId::GlowTargetOrMaximumV1),
        _ => Err(unknown_glow_variant("NumericalSiteIdV1")),
    }
}

fn indeterminacy_to_ffi(
    evidence: NumericalIndeterminacyV1,
) -> Result<NumericalIndeterminacy, ColorError> {
    match evidence {
        NumericalIndeterminacyV1::SoundBoundUnavailable => {
            Ok(NumericalIndeterminacy::SoundBoundUnavailable)
        }
        NumericalIndeterminacyV1::IntervalOverlap(interval) => {
            Ok(NumericalIndeterminacy::IntervalOverlap {
                lower: interval.lower(),
                upper: interval.upper(),
            })
        }
        _ => Err(unknown_glow_variant("NumericalIndeterminacyV1")),
    }
}

#[uniffi::export]
pub fn solve_glow_point(
    tint: String,
    background: String,
    target_dj: f64,
    theme: Theme,
    profile: GlowDecisionProfile,
) -> Result<GlowPointDecision, ColorError> {
    let core_profile = profile.to_core();
    let decision =
        solve_screen_alpha_for_dj(&tint, &background, target_dj, core_profile, &theme.vc())
            .map_err(|reason| ColorError::InvalidGlowRequest { reason })?;
    match decision {
        NumericalDecisionV1::Determinate { value, guarantee } => {
            let certificate = value.composite_certificate();
            Ok(GlowPointDecision::Determinate {
                decision_profile: profile,
                decision_guarantee: decision_guarantee_to_ffi(guarantee)?,
                composite_profile: composite_profile_to_ffi(certificate.profile())?,
                composite_guarantee: composite_guarantee_to_ffi(certificate.guarantee())?,
                diagnostic_profile: value
                    .diagnostic_profile()
                    .map(diagnostic_profile_to_ffi)
                    .transpose()?,
                alpha: value.alpha(),
                alpha_css: value.alpha_css().to_string(),
                target_dj: value.target_dj(),
                target_status: target_status_to_ffi(value.status())?,
                achieved_dj: value.achieved_dj(),
                composite_hex: value.composite_hex().to_string(),
            })
        }
        NumericalDecisionV1::Indeterminate { site_id, evidence } => {
            Ok(GlowPointDecision::Indeterminate {
                decision_profile: profile,
                site_id: numerical_site_to_ffi(site_id)?,
                evidence: indeterminacy_to_ffi(evidence)?,
            })
        }
        _ => Err(ColorError::InvalidGlowRequest {
            reason: "unknown NumericalDecisionV1".to_string(),
        }),
    }
}

/// Оценка мутности («грязи») цвета `[0,1]`: 0 — чистый, 1 — грязный.
///
/// # Errors
///
/// [`ColorError::InvalidColor`] на невалидном hex.
#[uniffi::export]
pub fn muddiness(hex: String) -> Result<f64, ColorError> {
    muddiness_from_hex(&hex).map_err(|reason| ColorError::InvalidColor { reason })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outward_interval_evidence_survives_the_ffi_mapping() {
        let interval = labcolors_core::OutwardIntervalV1::try_new(0.9, 1.1).unwrap();
        assert_eq!(
            decision_guarantee_to_ffi(DecisionGuaranteeV1::OutwardIntervalV1(interval)).unwrap(),
            GlowDecisionGuarantee::OutwardIntervalV1 {
                lower: 0.9,
                upper: 1.1,
            }
        );
        assert_eq!(
            indeterminacy_to_ffi(NumericalIndeterminacyV1::IntervalOverlap(interval)).unwrap(),
            NumericalIndeterminacy::IntervalOverlap {
                lower: 0.9,
                upper: 1.1,
            }
        );
    }

    #[test]
    fn contrast_black_on_white_is_21_wcag() {
        let c = contrast("#000000".into(), "#FFFFFF".into(), Theme::Light).unwrap();
        assert!((c.wcag_ratio - 21.0).abs() < 1e-6);
        assert!(c.lc > 0.0, "тёмное на светлом — положительный Lc");
    }

    #[test]
    fn invalid_hex_is_structured_error() {
        let err = contrast("nope".into(), "#FFFFFF".into(), Theme::Light).unwrap_err();
        assert!(matches!(err, ColorError::InvalidColor { .. }));
    }

    #[test]
    fn solve_text_on_white_meets_floor() {
        let s = solve_contrast(
            "#FFFFFF".into(),
            ContractSpec::Text { lc: 60.0 },
            Theme::Light,
        )
        .unwrap();
        assert!(s.wcag_ratio >= 4.5, "AA-text пол держится");
    }

    #[test]
    fn solve_unreachable_carries_stable_code() {
        let err = solve_contrast(
            "#FFFFFF".into(),
            ContractSpec::Text { lc: 150.0 },
            Theme::Light,
        )
        .unwrap_err();
        match err {
            ColorError::Unreachable { code } => assert_eq!(code, "exceeds_range"),
            other => panic!("ожидался Unreachable, получено {other:?}"),
        }
    }

    #[test]
    fn ladder_alpha_per_theme() {
        // NeutralFillPrimary: light @20, dark @36 — пер-темная пара.
        let light = ladder_alpha("neutral-fill-primary".into(), Theme::Light).unwrap();
        let dark = ladder_alpha("neutral-fill-primary".into(), Theme::Dark).unwrap();
        assert!((light - 0.2).abs() < 1e-9);
        assert!((dark - 0.361).abs() < 1e-9);
    }

    #[test]
    fn unknown_ladder_position_is_error() {
        let err = ladder_alpha("label-quinary".into(), Theme::Light).unwrap_err();
        assert!(matches!(err, ColorError::UnknownLadderPosition { .. }));
    }

    #[test]
    fn composite_and_min_alpha_are_wired() {
        let c = composite("#787880".into(), 0.2, "#FFFFFF".into()).unwrap();
        assert!(c.starts_with('#') && c.len() == 7);
        let m = min_alpha("#787880".into(), "#FFFFFF".into()).unwrap();
        assert!((0.0..=1.0).contains(&m));
    }

    #[test]
    fn muddiness_orders_olive_above_clean_gray() {
        // Значения — факт ядра (см. conformance/vectors/muddiness.json): серый
        // почти чист, олива заметно мутнее. Проверяем ПОРЯДОК и чистоту серого,
        // не магическое число.
        let olive = muddiness("#6B6B2E".into()).unwrap();
        let gray = muddiness("#808080".into()).unwrap();
        assert!(gray < 0.05, "серый должен быть чистым, получено {gray}");
        assert!(olive > gray, "олива должна быть мутнее серого");
    }

    #[test]
    fn glow_stable_indeterminate_and_legacy_guarantees_do_not_collapse() {
        let stable = solve_glow_point(
            "#C0B2FA".into(),
            "#000000".into(),
            2.3006,
            Theme::Light,
            GlowDecisionProfile::StableV1,
        )
        .unwrap();
        assert!(matches!(
            stable,
            GlowPointDecision::Indeterminate {
                decision_profile: GlowDecisionProfile::StableV1,
                site_id: NumericalSiteId::GlowTargetOrMaximumV1,
                evidence: NumericalIndeterminacy::SoundBoundUnavailable,
            }
        ));

        let legacy = solve_glow_point(
            "#C0B2FA".into(),
            "#000000".into(),
            2.3006,
            Theme::Light,
            GlowDecisionProfile::LegacyPlatformDependentV1,
        )
        .unwrap();
        assert!(matches!(
            legacy,
            GlowPointDecision::Determinate {
                decision_profile: GlowDecisionProfile::LegacyPlatformDependentV1,
                decision_guarantee: GlowDecisionGuarantee::LegacyPlatformDependentV1,
                composite_profile: GlowCompositeProfile::EncodedSrgb8ScreenV1,
                composite_guarantee: GlowCompositeGuarantee::BitExact,
                diagnostic_profile: Some(GlowDiagnosticProfile::Cam16UcsJPrimeLi2017V1),
                ..
            }
        ));

        let stable_noop = solve_glow_point(
            "#C0B2FA".into(),
            "#FFFFFF".into(),
            2.3006,
            Theme::Light,
            GlowDecisionProfile::StableV1,
        )
        .unwrap();
        assert!(matches!(
            stable_noop,
            GlowPointDecision::Determinate {
                decision_profile: GlowDecisionProfile::StableV1,
                decision_guarantee: GlowDecisionGuarantee::BitExact,
                diagnostic_profile: None,
                ref composite_hex,
                ..
            } if composite_hex == "#FFFFFF"
        ));
    }

    #[test]
    fn stable_glow_noop_uses_the_quantised_endpoint_not_a_white_special_case() {
        let sub_lsb = solve_glow_point(
            "#010000".into(),
            "#FE0000".into(),
            2.3006,
            Theme::Light,
            GlowDecisionProfile::StableV1,
        )
        .unwrap();
        assert!(matches!(
            sub_lsb,
            GlowPointDecision::Determinate {
                decision_profile: GlowDecisionProfile::StableV1,
                decision_guarantee: GlowDecisionGuarantee::BitExact,
                composite_profile: GlowCompositeProfile::EncodedSrgb8ScreenV1,
                composite_guarantee: GlowCompositeGuarantee::BitExact,
                diagnostic_profile: None,
                target_status: GlowTargetStatus::Unreachable,
                achieved_dj,
                ref composite_hex,
                ..
            } if achieved_dj.to_bits() == 0.0_f64.to_bits()
                && composite_hex == "#FE0000"
        ));

        let crossing = solve_glow_point(
            "#800000".into(),
            "#FE0000".into(),
            2.3006,
            Theme::Light,
            GlowDecisionProfile::StableV1,
        )
        .unwrap();
        assert!(matches!(
            crossing,
            GlowPointDecision::Indeterminate {
                decision_profile: GlowDecisionProfile::StableV1,
                site_id: NumericalSiteId::GlowTargetOrMaximumV1,
                evidence: NumericalIndeterminacy::SoundBoundUnavailable,
            }
        ));
    }
}
