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
//! | [`muddiness`] | `muddiness` legacy compatibility vectors | `cleanliness::muddiness_from_hex` |
//! | [`evaluate_wcag22`] | `wcag22` | exact final-sRGB8 WCAG 2.2 evaluator |
//! | [`core_version`] | `manifest` | версия ядра |
//!
//! [`solve_glow_point`] — отдельный low-level contract test нативной границы:
//! stable-профиль переносит `Indeterminate` с site + typed evidence без fallback
//! для non-trivial selection, а exact byte-no-op возвращает determinate без CAM16;
//! legacy выбирается только явно. Его CAM16-диагностика не объявлена
//! bit-exact между платформами; `bit-exact` относится лишь к certificate
//! encoded-sRGB8 screen-композитора. Output — algebraic sum
//! `StableExactNoop | LegacyReached | LegacyUnreachable | Indeterminate`, а не
//! независимые provenance-поля с незаконными cross-product.
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
    BgInput, ChromaPolicy, Contract, Gamut, GlowCompositeGuaranteeV1, GlowCompositeProfileV1,
    GlowDecisionProfileV1, GlowDiagnosticProfileV1, GlowTargetStatus as CoreGlowTargetStatus, Hue,
    LadderPosition, LegacyPlatformDependentV1, NumericalCompatibilityReleaseIdV1,
    NumericalDecisionEvidenceV1, NumericalDecisionV1, NumericalIndeterminacyV1, NumericalSiteIdV1,
    Theme as CoreTheme, ViewingConditions, recheck_against, solve, solve_screen_alpha_for_dj,
    srgb_encoded_from_hex,
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

/// Explicit WCAG 2.2 success criterion for one occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Wcag22Criterion {
    /// SC 1.4.3 ordinary text, 4.5:1.
    Sc143TextDefault,
    /// SC 1.4.3 explicitly declared large-scale text, 3:1.
    Sc143TextLargeScale,
    /// SC 1.4.11 required UI component/state information, 3:1.
    Sc1411UiComponentOrState,
    /// SC 1.4.11 required graphical-object information, 3:1.
    Sc1411GraphicalObject,
}

impl Wcag22Criterion {
    fn to_core(self) -> labcolors_core::wcag22::Wcag22CriterionV1 {
        use labcolors_core::wcag22::Wcag22CriterionV1 as Core;
        match self {
            Self::Sc143TextDefault => Core::Sc143TextDefault,
            Self::Sc143TextLargeScale => Core::Sc143TextLargeScale,
            Self::Sc1411UiComponentOrState => Core::Sc1411UiComponentOrState,
            Self::Sc1411GraphicalObject => Core::Sc1411GraphicalObject,
        }
    }
}

/// Total decision on the admitted final-sRGB8 domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Wcag22Decision {
    /// Threshold is proved satisfied.
    Pass,
    /// Threshold is proved unsatisfied.
    Fail,
}

/// Q55 outward luminance enclosure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct Wcag22Q55Bounds {
    /// Inclusive lower bound.
    pub lower: u64,
    /// Inclusive upper bound.
    pub upper: u64,
}

/// Registry-bound numerical evidence transported from Rust core.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Wcag22Evidence {
    /// Evidence class key.
    pub kind: String,
    /// Canonical artifact identity.
    pub artifact_id: String,
    /// Canonical binary artifact digest.
    pub artifact_sha256: String,
    /// Registered bound/threshold-law identity.
    pub bound_id: String,
    /// Replayable full-domain proof identity.
    pub proof_id: String,
    /// Exact committed proof-file digest.
    pub proof_sha256: String,
    /// Canonical self-authenticated proof payload digest.
    pub proof_payload_sha256: String,
    /// Exact generator source digest.
    pub generator_sha256: String,
    /// Exact independent verifier source digest.
    pub verifier_sha256: String,
    /// Typed profile V1 checksum, independent of JSON formatting.
    pub profile_checksum: String,
    /// Canonical profile-source digest.
    pub profile_sha256: String,
}

/// Atomic WCAG 2.2 assessment; Swift performs no contrast math.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Wcag22Assessment {
    /// Immutable profile identity.
    pub profile_id: String,
    /// Exact declared occurrence criterion.
    pub criterion: Wcag22Criterion,
    /// Normalised final foreground bytes as hex.
    pub foreground: String,
    /// Normalised final background bytes as hex.
    pub background: String,
    /// Foreground Q55 enclosure.
    pub foreground_luminance: Wcag22Q55Bounds,
    /// Background Q55 enclosure.
    pub background_luminance: Wcag22Q55Bounds,
    /// Fixed-point scale (`2^55`).
    pub q55_scale: u64,
    /// Exact Pass/Fail result.
    pub decision: Wcag22Decision,
    /// Sealed evidence identities.
    pub evidence: Wcag22Evidence,
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

/// Общий exact point-composite payload determinate Glow outcome.
///
/// Semantic provenance задаёт владеющий вариант [`GlowPointDecision`], а не
/// независимые поля, из которых можно собрать невозможную комбинацию.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct GlowPointValue {
    /// Exact composite profile.
    pub composite_profile: GlowCompositeProfile,
    /// Exact composite guarantee.
    pub composite_guarantee: GlowCompositeGuarantee,
    /// Canonical layer alpha.
    pub alpha: f64,
    /// Shortest-roundtrip CSS alpha.
    pub alpha_css: String,
    /// Requested target.
    pub target_dj: f64,
    /// Diagnostic achieved value.
    pub achieved_dj: f64,
    /// Exact point composite.
    pub composite_hex: String,
}

/// Low-level point Glow decision как сумма только допустимых provenance-state.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum GlowPointDecision {
    /// Stable byte-exact no-op: target недостижим без appearance selection.
    StableExactNoop {
        /// Exact point-composite payload.
        value: GlowPointValue,
    },
    /// Explicit legacy CAM16/libm selection достиг target.
    LegacyReached {
        /// Exact point-composite payload выбранного состояния.
        value: GlowPointValue,
    },
    /// Explicit legacy CAM16/libm selection не достиг target и вернул максимум.
    LegacyUnreachable {
        /// Exact point-composite payload диагностически выбранного максимума.
        value: GlowPointValue,
    },
    /// Stable path не выбрал состояние без sound bound и не сделал fallback.
    Indeterminate {
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
    /// Невалидный low-level Glow request на публичной границе.
    #[error("невалидный glow request: {reason}")]
    InvalidGlowRequest {
        /// Человекочитаемая причина.
        reason: String,
    },
    /// FFI-adapter получил internal postcondition failure либо вариант core-
    /// контракта, который эта версия нативной поверхности не умеет представить
    /// без потери смысла. Это не невалидный пользовательский request.
    #[error("несовместимый контракт ядра: {reason}")]
    IncompatibleCoreContract {
        /// Человекочитаемая причина internal/core-surface несовместимости.
        reason: String,
    },
}

/// Стабильный код недостижимости — тождественен маппингу WASM-границы
/// (`labcolors-wasm/src/engine.rs`) и conformance-пака. Общий контракт имён для
/// ВСЕХ биндингов: одна известная причина → один код на любой платформе;
/// неизвестный forward-вариант — несовместимость версии, а не fallback-code.
fn unreachable_code(err: &labcolors_core::Unreachable) -> Result<&'static str, ColorError> {
    use labcolors_core::Unreachable as U;
    match err {
        U::BelowContrastFloor { .. } => Ok("below_contrast_floor"),
        U::ExceedsRange { .. } => Ok("exceeds_range"),
        U::QuantizationGap { .. } => Ok("quantization_gap"),
        U::FloorUnreachable { .. } => Ok("floor_unreachable"),
        U::PolarityMismatch { .. } => Ok("polarity_mismatch"),
        U::GamutUnsupported => Ok("gamut_unsupported"),
        U::InvalidInput(_) => Ok("invalid_input"),
        U::InternalInvariant(reason) => Err(ColorError::IncompatibleCoreContract {
            reason: format!("internal core invariant failure: {reason}"),
        }),
        // Forward core variant — несовместимость adapter, не выдуманный code.
        _ => Err(incompatible_core_variant("Unreachable")),
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

/// Exact WCAG 2.2 assessment of one final foreground/background sRGB8 pair.
///
/// Swift transports the Rust-core decision and evidence; it does not implement
/// relative luminance, thresholds or rounding independently.
#[uniffi::export]
pub fn evaluate_wcag22(
    foreground: String,
    background: String,
    criterion: Wcag22Criterion,
) -> Result<Wcag22Assessment, ColorError> {
    use labcolors_core::wcag22::{Wcag22ApplicableDecisionV1, Wcag22AssessmentV1};

    let core =
        labcolors_core::wcag22::evaluate_wcag22_hex(&foreground, &background, criterion.to_core())
            .map_err(|error| match error {
                labcolors_core::wcag22::Wcag22EvaluationErrorV1::InvalidSrgb8 { field, reason } => {
                    ColorError::InvalidColor {
                        reason: format!("{field}: {reason}"),
                    }
                }
                other => ColorError::IncompatibleCoreContract {
                    reason: other.to_string(),
                },
            })?;
    let Wcag22AssessmentV1::Evaluated {
        profile_id,
        criterion: assessed_criterion,
        measurement,
        decision,
        evidence,
        ..
    } = core
    else {
        return Err(ColorError::IncompatibleCoreContract {
            reason: "pair evaluator returned report-only NotEvaluated".to_string(),
        });
    };
    let NumericalDecisionEvidenceV1::CanonicalFiniteBounded(evidence_payload) = evidence else {
        return Err(ColorError::IncompatibleCoreContract {
            reason: "WCAG22 assessment carried a non-bounded evidence class".to_string(),
        });
    };
    let artifact_id = evidence_payload.artifact_id();
    let bound_id = evidence_payload.bound_id();
    let proof_id = evidence_payload.proof_id();
    let profile = labcolors_core::wcag22::wcag22_profile_v1();
    if profile.profile_id != profile_id
        || profile.artifact_id != artifact_id
        || profile.bound_id != bound_id
        || profile.proof_id != proof_id
    {
        return Err(ColorError::IncompatibleCoreContract {
            reason: "WCAG22 assessment/profile evidence identities drifted".to_string(),
        });
    }
    let hex = |bytes: [u8; 3]| format!("#{:02X}{:02X}{:02X}", bytes[0], bytes[1], bytes[2]);
    let decision = match decision {
        Wcag22ApplicableDecisionV1::Pass => Wcag22Decision::Pass,
        Wcag22ApplicableDecisionV1::Fail => Wcag22Decision::Fail,
        _ => return Err(incompatible_core_variant("Wcag22ApplicableDecisionV1")),
    };
    let mapped_criterion = match assessed_criterion {
        labcolors_core::wcag22::Wcag22CriterionV1::Sc143TextDefault => {
            Wcag22Criterion::Sc143TextDefault
        }
        labcolors_core::wcag22::Wcag22CriterionV1::Sc143TextLargeScale => {
            Wcag22Criterion::Sc143TextLargeScale
        }
        labcolors_core::wcag22::Wcag22CriterionV1::Sc1411UiComponentOrState => {
            Wcag22Criterion::Sc1411UiComponentOrState
        }
        labcolors_core::wcag22::Wcag22CriterionV1::Sc1411GraphicalObject => {
            Wcag22Criterion::Sc1411GraphicalObject
        }
        _ => return Err(incompatible_core_variant("Wcag22CriterionV1")),
    };
    if mapped_criterion != criterion {
        return Err(ColorError::IncompatibleCoreContract {
            reason: "WCAG22 assessment criterion drifted from the requested criterion".to_string(),
        });
    }
    Ok(Wcag22Assessment {
        profile_id: profile_id.key().to_string(),
        criterion: mapped_criterion,
        foreground: hex(measurement.foreground),
        background: hex(measurement.background),
        foreground_luminance: Wcag22Q55Bounds {
            lower: measurement.foreground_luminance.lower(),
            upper: measurement.foreground_luminance.upper(),
        },
        background_luminance: Wcag22Q55Bounds {
            lower: measurement.background_luminance.lower(),
            upper: measurement.background_luminance.upper(),
        },
        q55_scale: labcolors_core::wcag22::Wcag22LuminanceBoundsQ55V1::scale(),
        decision,
        evidence: Wcag22Evidence {
            kind: "canonical-finite-bounded".to_string(),
            artifact_id: artifact_id.key().to_string(),
            artifact_sha256: profile.artifact_sha256.to_string(),
            bound_id: bound_id.key().to_string(),
            proof_id: proof_id.key().to_string(),
            proof_sha256: profile.proof_sha256.to_string(),
            proof_payload_sha256: profile.proof_payload_sha256.to_string(),
            generator_sha256: profile.generator_sha256.to_string(),
            verifier_sha256: profile.verifier_sha256.to_string(),
            profile_checksum: profile.profile_checksum.to_string(),
            profile_sha256: profile.source_sha256.to_string(),
        },
    })
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
/// удовлетворяет контракт (честный отказ, не тихий клип);
/// [`ColorError::IncompatibleCoreContract`] на неизвестном forward-варианте
/// core [`labcolors_core::Unreachable`].
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
        Err(e) => {
            let code = unreachable_code(&e)?;
            Err(ColorError::Unreachable {
                code: code.to_string(),
            })
        }
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

fn incompatible_core_variant(name: &str) -> ColorError {
    ColorError::IncompatibleCoreContract {
        reason: format!("unknown core {name} variant"),
    }
}

fn composite_profile_to_ffi(
    profile: GlowCompositeProfileV1,
) -> Result<GlowCompositeProfile, ColorError> {
    match profile {
        GlowCompositeProfileV1::EncodedSrgb8ScreenV1 => {
            Ok(GlowCompositeProfile::EncodedSrgb8ScreenV1)
        }
        _ => Err(incompatible_core_variant("GlowCompositeProfileV1")),
    }
}

fn composite_guarantee_to_ffi(
    guarantee: GlowCompositeGuaranteeV1,
) -> Result<GlowCompositeGuarantee, ColorError> {
    match guarantee {
        GlowCompositeGuaranteeV1::BitExact => Ok(GlowCompositeGuarantee::BitExact),
        _ => Err(incompatible_core_variant("GlowCompositeGuaranteeV1")),
    }
}

fn numerical_site_to_ffi(site: NumericalSiteIdV1) -> Result<NumericalSiteId, ColorError> {
    match site {
        NumericalSiteIdV1::GlowTargetOrMaximumV1 => Ok(NumericalSiteId::GlowTargetOrMaximumV1),
        _ => Err(incompatible_core_variant("NumericalSiteIdV1")),
    }
}

fn validate_glow_request(tint: &str, background: &str, target_dj: f64) -> Result<(), ColorError> {
    srgb_encoded_from_hex(tint).map_err(|reason| ColorError::InvalidGlowRequest {
        reason: format!("invalid tint: {reason}"),
    })?;
    srgb_encoded_from_hex(background).map_err(|reason| ColorError::InvalidGlowRequest {
        reason: format!("invalid background: {reason}"),
    })?;
    if !target_dj.is_finite() || target_dj <= 0.0 {
        return Err(ColorError::InvalidGlowRequest {
            reason: format!("target_dj должен быть конечным и > 0, получено {target_dj}"),
        });
    }
    Ok(())
}

fn map_prevalidated_glow_core_result<T>(result: Result<T, String>) -> Result<T, ColorError> {
    result.map_err(|reason| ColorError::IncompatibleCoreContract {
        reason: format!("core rejected a prevalidated Glow request: {reason}"),
    })
}

fn glow_point_value_to_ffi(
    value: &labcolors_core::GlowSolve,
) -> Result<GlowPointValue, ColorError> {
    let certificate = value.composite_certificate();
    Ok(GlowPointValue {
        composite_profile: composite_profile_to_ffi(certificate.profile())?,
        composite_guarantee: composite_guarantee_to_ffi(certificate.guarantee())?,
        alpha: value.alpha(),
        alpha_css: value.alpha_css().to_string(),
        target_dj: value.target_dj(),
        achieved_dj: value.achieved_dj(),
        composite_hex: value.composite_hex().to_string(),
    })
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
        _ => Err(incompatible_core_variant("NumericalIndeterminacyV1")),
    }
}

/// Point Glow solve с обязательным numerical profile. Stable uncertainty
/// возвращается data-вариантом `Indeterminate`, не ошибкой и не fallback.
///
/// # Errors
///
/// [`ColorError::InvalidGlowRequest`] на недоменном публичном input.
/// [`ColorError::IncompatibleCoreContract`] если core отверг уже проверенный
/// request либо FFI-adapter получил illegal/unsupported/unknown outcome,
/// который нельзя представить этой версией Glow-specific поверхности.
#[uniffi::export]
pub fn solve_glow_point(
    tint: String,
    background: String,
    target_dj: f64,
    theme: Theme,
    profile: GlowDecisionProfile,
) -> Result<GlowPointDecision, ColorError> {
    validate_glow_request(&tint, &background, target_dj)?;
    // Migration adapter: FFI-профиль → generic typed execution mode (#292).
    let mode = profile.to_core().execution_mode();
    let decision = map_prevalidated_glow_core_result(solve_screen_alpha_for_dj(
        &tint,
        &background,
        target_dj,
        mode,
        &theme.vc(),
    ))?;
    // Атомарный core-результат маппится напрямую: cross-product реконструкция
    // profile × guarantee × status удалена — незаконные комбинации
    // непредставимы уже в core-типе (#292).
    match decision {
        NumericalDecisionV1::Determinate {
            value,
            evidence: NumericalDecisionEvidenceV1::BitExact { .. },
            ..
        } => {
            if value.status() != CoreGlowTargetStatus::ExactNoopUnreachable
                || value.selection_diagnostic_profile().is_some()
            {
                return Err(ColorError::IncompatibleCoreContract {
                    reason: format!("illegal stable Glow value state: {:?}", value.status()),
                });
            }
            Ok(GlowPointDecision::StableExactNoop {
                value: glow_point_value_to_ffi(&value)?,
            })
        }
        NumericalDecisionV1::Compatibility {
            value,
            release_id: NumericalCompatibilityReleaseIdV1::GlowCam16UcsJPrimeTargetOrMaxV1,
            provenance: LegacyPlatformDependentV1,
            ..
        } => {
            if value.selection_diagnostic_profile()
                != Some(GlowDiagnosticProfileV1::Cam16UcsJPrimeLi2017V1)
            {
                return Err(ColorError::IncompatibleCoreContract {
                    reason: "compatibility Glow value без selection diagnostic".to_string(),
                });
            }
            let status = value.status();
            let value = glow_point_value_to_ffi(&value)?;
            match status {
                CoreGlowTargetStatus::LegacyReached => {
                    Ok(GlowPointDecision::LegacyReached { value })
                }
                CoreGlowTargetStatus::LegacyUnreachable => {
                    Ok(GlowPointDecision::LegacyUnreachable { value })
                }
                other => Err(ColorError::IncompatibleCoreContract {
                    reason: format!("illegal compatibility Glow value state: {other:?}"),
                }),
            }
        }
        NumericalDecisionV1::Indeterminate { site_id, evidence } => {
            if profile != GlowDecisionProfile::StableV1 {
                return Err(ColorError::IncompatibleCoreContract {
                    reason: "legacy Glow profile returned an Indeterminate core outcome"
                        .to_string(),
                });
            }
            Ok(GlowPointDecision::Indeterminate {
                site_id: numerical_site_to_ffi(site_id)?,
                evidence: indeterminacy_to_ffi(evidence)?,
            })
        }
        _ => Err(incompatible_core_variant("NumericalDecisionV1")),
    }
}

/// Замороженная legacy-координата `muddiness` для цвета.
///
/// Это `experimental compatibility proxy`: функция воспроизводит исторический
/// числовой API, но не является валидированным на наблюдателях человеческим
/// вердиктом clean/dirty и не должна использоваться как production decision.
/// Legacy-идентификатор сохранён только для совместимости.
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
    fn wcag22_transport_preserves_core_decision_and_evidence() {
        let assessment = evaluate_wcag22(
            "#89BB09".into(),
            "#8212DB".into(),
            Wcag22Criterion::Sc1411GraphicalObject,
        )
        .unwrap();
        assert_eq!(assessment.decision, Wcag22Decision::Fail);
        assert_eq!(assessment.foreground, "#89BB09");
        assert_eq!(assessment.background, "#8212DB");
        assert_eq!(
            assessment.evidence.artifact_id,
            "wcag22-srgb8-luminance-q55-v1"
        );
        assert_eq!(
            assessment.evidence.proof_id,
            "wcag22-srgb8-full-domain-q55-v1"
        );
        assert_eq!(assessment.q55_scale, 1_u64 << 55);
    }

    #[test]
    fn wcag22_transport_maps_core_pass() {
        let assessment = evaluate_wcag22(
            "#000000".into(),
            "#FFFFFF".into(),
            Wcag22Criterion::Sc143TextDefault,
        )
        .unwrap();
        assert_eq!(assessment.decision, Wcag22Decision::Pass);
    }

    #[test]
    fn wcag22_transport_rejects_invalid_hex_without_fallback() {
        assert!(matches!(
            evaluate_wcag22(
                "invalid".into(),
                "#FFFFFF".into(),
                Wcag22Criterion::Sc143TextDefault,
            ),
            Err(ColorError::InvalidColor { .. })
        ));
    }

    #[test]
    fn legacy_solve_maps_to_atomic_compatibility_variants() {
        // Явный compatibility-mode: результат — атомарный LegacyReached/
        // LegacyUnreachable, без реконструкции provenance на границе.
        let decision = solve_glow_point(
            "#C0B2FA".into(),
            "#000000".into(),
            2.3006,
            Theme::Light,
            GlowDecisionProfile::LegacyPlatformDependentV1,
        )
        .unwrap();
        assert!(matches!(
            decision,
            GlowPointDecision::LegacyReached { .. } | GlowPointDecision::LegacyUnreachable { .. }
        ));
    }

    #[test]
    fn prevalidated_core_glow_error_is_not_reclassified_as_public_input() {
        let error = map_prevalidated_glow_core_result::<()>(Err("synthetic core drift".into()))
            .unwrap_err();
        assert!(matches!(
            error,
            ColorError::IncompatibleCoreContract { ref reason }
                if reason.contains("synthetic core drift")
        ));
    }

    #[test]
    fn stable_noop_output_is_an_atomic_variant() {
        let decision = solve_glow_point(
            "#010000".into(),
            "#FE0000".into(),
            2.3006,
            Theme::Light,
            GlowDecisionProfile::StableV1,
        )
        .unwrap();
        assert!(matches!(
            decision,
            GlowPointDecision::StableExactNoop { .. }
        ));
    }

    #[test]
    fn known_unreachable_code_mapping_is_fallible_without_code_drift() {
        use labcolors_core::Unreachable as U;
        let cases = [
            (
                U::BelowContrastFloor { target: 1.0 },
                "below_contrast_floor",
            ),
            (
                U::ExceedsRange {
                    target: 100.0,
                    max_achievable: 90.0,
                },
                "exceeds_range",
            ),
            (
                U::QuantizationGap {
                    target: 50.0,
                    nearest: 49.0,
                },
                "quantization_gap",
            ),
            (
                U::FloorUnreachable {
                    floor: 4.5,
                    max_ratio: 4.0,
                },
                "floor_unreachable",
            ),
            (U::PolarityMismatch { target: -60.0 }, "polarity_mismatch"),
            (U::GamutUnsupported, "gamut_unsupported"),
            (U::InvalidInput("fixture".into()), "invalid_input"),
        ];
        for (error, expected) in cases {
            assert_eq!(unreachable_code(&error).unwrap(), expected);
        }

        let internal = U::InternalInvariant("fixture drift".into());
        assert!(matches!(
            unreachable_code(&internal),
            Err(ColorError::IncompatibleCoreContract { reason })
                if reason == "internal core invariant failure: fixture drift"
        ));
    }

    #[test]
    fn glow_output_sum_is_exhaustive_over_lawful_provenance_states() {
        fn branch_name(decision: &GlowPointDecision) -> &'static str {
            match decision {
                GlowPointDecision::StableExactNoop { .. } => "stable-exact-noop",
                GlowPointDecision::LegacyReached { .. } => "legacy-reached",
                GlowPointDecision::LegacyUnreachable { .. } => "legacy-unreachable",
                GlowPointDecision::Indeterminate { .. } => "indeterminate",
            }
        }

        let decision = solve_glow_point(
            "#010000".into(),
            "#FE0000".into(),
            2.3006,
            Theme::Light,
            GlowDecisionProfile::StableV1,
        )
        .unwrap();
        assert_eq!(
            branch_name(&decision),
            "stable-exact-noop",
            "exhaustive match обязан читать реальный public output"
        );
    }

    #[test]
    fn interval_stays_a_lawful_indeterminate_payload() {
        let interval = labcolors_core::OutwardIntervalV1::try_new(0.9, 1.1).unwrap();
        // Outward evidence остаётся законной частью typed Indeterminate:
        // запрещена только ложная Glow-specific determinate guarantee.
        assert_eq!(
            indeterminacy_to_ffi(NumericalIndeterminacyV1::IntervalOverlap(interval)).unwrap(),
            NumericalIndeterminacy::IntervalOverlap {
                lower: 0.9,
                upper: 1.1,
            }
        );

        let unknown = incompatible_core_variant("FutureGlowContractV2");
        assert!(matches!(
            unknown,
            ColorError::IncompatibleCoreContract { ref reason }
                if reason.contains("FutureGlowContractV2")
        ));
    }

    #[test]
    fn invalid_public_glow_inputs_remain_invalid_glow_requests() {
        for (tint, background, target_dj) in [
            ("not-a-color", "#000000", 2.3006),
            ("#C0B2FA", "not-a-color", 2.3006),
            ("#C0B2FA", "#000000", 0.0),
            ("#C0B2FA", "#000000", -1.0),
            ("#C0B2FA", "#000000", f64::NAN),
            ("#C0B2FA", "#000000", f64::INFINITY),
        ] {
            let error = solve_glow_point(
                tint.into(),
                background.into(),
                target_dj,
                Theme::Light,
                GlowDecisionProfile::StableV1,
            )
            .unwrap_err();
            assert!(matches!(error, ColorError::InvalidGlowRequest { .. }));
        }
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
    fn muddiness_boundary_matches_frozen_core_coordinate() {
        for hex in ["#6B6B2E", "#808080", "#007AFF", "#8A7A50"] {
            let boundary = muddiness(hex.into()).expect("valid public input");
            let core = muddiness_from_hex(hex).expect("valid core input");
            assert_eq!(
                boundary.to_bits(),
                core.to_bits(),
                "FFI boundary drifted for {hex}"
            );
        }

        assert!(matches!(
            muddiness("not-a-colour".into()),
            Err(ColorError::InvalidColor { .. })
        ));
    }

    #[test]
    fn glow_provenance_is_encoded_by_atomic_output_variants() {
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
            GlowPointDecision::LegacyReached { value }
                if value.composite_profile == GlowCompositeProfile::EncodedSrgb8ScreenV1
                    && value.composite_guarantee == GlowCompositeGuarantee::BitExact
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
            GlowPointDecision::StableExactNoop { value }
                if value.composite_hex == "#FFFFFF"
        ));

        let legacy_unreachable = solve_glow_point(
            "#C0B2FA".into(),
            "#FFFFFF".into(),
            2.3006,
            Theme::Light,
            GlowDecisionProfile::LegacyPlatformDependentV1,
        )
        .unwrap();
        assert!(matches!(
            legacy_unreachable,
            GlowPointDecision::LegacyUnreachable { value }
                if value.composite_hex == "#FFFFFF"
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
            GlowPointDecision::StableExactNoop { value }
                if value.composite_profile == GlowCompositeProfile::EncodedSrgb8ScreenV1
                    && value.composite_guarantee == GlowCompositeGuarantee::BitExact
                    && value.achieved_dj.to_bits() == 0.0_f64.to_bits()
                    && value.composite_hex == "#FE0000"
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
                site_id: NumericalSiteId::GlowTargetOrMaximumV1,
                evidence: NumericalIndeterminacy::SoundBoundUnavailable,
            }
        ));
    }
}
