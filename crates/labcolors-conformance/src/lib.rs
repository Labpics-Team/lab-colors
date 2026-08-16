//! Платформо-нейтральный **conformance-пак** движка `labcolors`.
//!
//! # Что это и зачем
//!
//! Ядро (`labcolors-core`) задаёт канонический выход, но не все его поля имеют
//! byte-exact cross-runtime guarantee: трансцендентные `f64` сравниваются по
//! [`DRIFT_TOL`], exact hex/enum — точно. У движка несколько поверхностей — WASM
//! (`@labpics/colors`) и Swift через UniFFI. Пак задаёт исполняемый контракт:
//! набор векторов «вход → канонический выход» и правила сравнения, которые
//! обязан пройти конкретный биндинг, прежде чем называться conformant.
//!
//! # Честность конструкции
//!
//! Векторы не вписаны руками — они ДЕРИВИРОВАНЫ из публичного API ядра
//! генератором ([`bin/gen`](../gen/index.html)) и закоммичены. Ожидаемые
//! значения — это то, что выдаёт ядро, а не то, что «должно бы». Внешняя правда
//! (опубликованные WCAG-якоря) сверяется ОТДЕЛЬНО раннером-референсом
//! (`tests/reference_runner.rs`). Равенство с опубликованным стандартом относится
//! только к проверяемому WCAG-подмножеству; остальные векторы характеризуют
//! версионированное поведение ядра, не доказывая человеческий смысл координат.
//!
//! # Семейства векторов
//!
//! | Файл | Что фиксирует | Источник в ядре |
//! |------|---------------|-----------------|
//! | `contrasts.json` | (fg, bg, тема) → (Ys candidate score, WCAG) | `recheck_against` |
//! | `ladders.json` | позиция лестницы → (α_light, α_dark) | `LadderPosition::alpha_pair` |
//! | `alpha.json` | подложка→α: композит и α_min | `alpha::composite_hex` / `alpha::min_alpha_hex` |
//! | `solve.json` | (bg, контракт, тема) → цвет или типизированный failure | `solve` |
//! | `wcag22.json` | final sRGB8 pair + criterion → exact assessment | `wcag22::evaluate_wcag22_hex` |
//! | `manifest.json` | версии, дайджест, счётчики, capability manifest | `numerical_capability_manifest_v2` |
//!
//! Версия пака ([`PACK_VERSION`]) привязана к версии ядра ([`core_version`]):
//! при легитимной смене канона генератор перегенерирует векторы, а
//! раннер-референс ловит любой дрейф.
//! `manifest.numericalCapabilities` — это projection core-owned capability
//! manifest (coverage `migrated-sites-only-v1`): перечислены только уже
//! мигрированные typed-decision sites; полнота аудита исторических `f64`
//! branches остаётся в #291.

use serde::{Deserialize, Serialize};

use labcolors_core::alpha::{composite_hex, min_alpha_hex};
use labcolors_core::{
    BgInput, ChromaPolicy, Contract, Gamut, Hue, LadderPosition, ViewingConditions, fnv1a_32,
    recheck_against, solve,
};

/// Семантическая версия conformance-пака. Меняется при изменении СХЕМЫ или
/// состава векторов; значения векторов при этом диктует канон ядра.
pub const PACK_VERSION: &str = "10.0.0";

/// Версия ядра, к которой привязан пак. Все крейты воркспейса делят одну версию
/// (`version.workspace = true`), поэтому собственная `CARGO_PKG_VERSION` этого
/// крейта тождественна версии `labcolors-core` — при релизном бампе они
/// двигаются в ногу.
#[must_use]
pub fn core_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

// ─────────────────────────────────────────────────────────────────────────────
// Тема → условия просмотра. Каноническая карта живёт в ядре (`Theme`); здесь —
// только разбор kebab-ключа вектора, чтобы биндинги и генератор говорили одним
// словарём тем ("light" | "dark" | "light-ic" | "dark-ic").
// ─────────────────────────────────────────────────────────────────────────────

/// Условия просмотра для kebab-ключа темы — ЛОКАЛЬНЫЙ fixture-словарь пака
/// (C5.1: канонический словарь тем принадлежит клиентскому конфигу; ядро
/// встроенных имён не несёт). Ключи совпадают со словарём labui-паспорта.
/// Паникует на неизвестной теме — ключи в паке контролируются генератором,
/// внешний вход сюда не попадает.
fn vc_for_theme(theme_key: &str) -> ViewingConditions {
    use labcolors_core::VcPreset;
    let preset = match theme_key {
        "light" => VcPreset::Srgb,
        "dark" => VcPreset::Dim,
        "light-ic" => VcPreset::SrgbIc,
        "dark-ic" => VcPreset::DimIc,
        other => panic!("ключ темы в паке всегда канонический, получено: {other}"),
    };
    preset.viewing_conditions()
}

/// Все четыре канонические темы в стабильном порядке.
const THEMES: [&str; 4] = ["light", "dark", "light-ic", "dark-ic"];

// ─────────────────────────────────────────────────────────────────────────────
// Семейство: контрасты
// ─────────────────────────────────────────────────────────────────────────────

/// Один контраст-вектор: знаковая кандидатная оценка `Lc` по `Ys` и
/// юридический WCAG-ratio переднего плана на фоне под темой. Два числа
/// отчитываются раздельно и не смешиваются.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContrastVector {
    /// Передний план, `#RRGGBB`.
    pub fg: String,
    /// Фон, `#RRGGBB`.
    pub bg: String,
    /// Тема просмотра (kebab-ключ).
    pub theme: String,
    /// Знаковая кандидатная оценка `Lc` по `Ys` из замороженной SAPC-shaped кривой;
    /// не LPC/readability evidence.
    pub lc: f64,
    /// Контраст-ratio WCAG 2.1 (1–21).
    pub wcag_ratio: f64,
}

/// Курированный набор пар (fg, bg): опубликованные WCAG-якоря (чёрное/белое =
/// 21:1, граница AA-текста `#767676`, шаг ниже `#777777`), бренд и нейтрали.
/// Пары — ВХОД; ожидаемые числа диктует ядро.
const CONTRAST_PAIRS: [(&str, &str); 10] = [
    ("#000000", "#FFFFFF"), // предельный 21:1
    ("#FFFFFF", "#000000"), // симметричный предел
    ("#767676", "#FFFFFF"), // учебниковая граница AA-текста ≈ 4.54:1
    ("#777777", "#FFFFFF"), // на один 8-битный шаг светлее — уже < 4.5:1
    ("#007AFF", "#FFFFFF"), // бренд на белом
    ("#FFFFFF", "#007AFF"), // белое на бренде
    ("#0A0A10", "#FFFFFF"), // near-black label на near-white
    ("#F7F7FF", "#101012"), // near-white на near-black (тёмная тема)
    ("#71717A", "#FFFFFF"), // вторичный лейбл labui
    ("#007AFF", "#101012"), // бренд на тёмном
];

/// Дериватор контраст-векторов: полное декартово произведение пар × темы.
#[must_use]
pub fn generate_contrasts() -> Vec<ContrastVector> {
    let mut out = Vec::with_capacity(CONTRAST_PAIRS.len() * THEMES.len());
    for &(fg, bg) in &CONTRAST_PAIRS {
        for &theme in &THEMES {
            let vc = vc_for_theme(theme);
            let pair = recheck_against(bg, &[fg], &vc)
                .expect("фикстуры пака — валидные hex")
                .pop()
                .expect("ровно один передний план");
            out.push(ContrastVector {
                fg: fg.to_string(),
                bg: bg.to_string(),
                theme: theme.to_string(),
                lc: pair.0,
                wcag_ratio: pair.1,
            });
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Семейство: лестницы
// ─────────────────────────────────────────────────────────────────────────────

/// Один вектор лестницы: стабильный ключ позиции и её пер-темная пара альф.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LadderVector {
    /// Kebab-ключ позиции (`label-primary`, `fill-secondary`, …).
    pub position: String,
    /// Альфа в светлых темах (light / light-ic).
    pub alpha_light: f64,
    /// Альфа в тёмных темах (dark / dark-ic).
    pub alpha_dark: f64,
}

/// Дериватор лестниц: каждая каноническая позиция и её `(light, dark)`-альфы.
#[must_use]
pub fn generate_ladders() -> Vec<LadderVector> {
    LadderPosition::ALL
        .iter()
        .map(|&p| {
            let (light, dark) = p.alpha_pair();
            LadderVector {
                position: p.key().to_string(),
                alpha_light: light,
                alpha_dark: dark,
            }
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Семейство: подложка → α
// ─────────────────────────────────────────────────────────────────────────────

/// Один вектор альфа-алгебры: прямой ход (композит тинта при α над фоном) и
/// нижняя граница разрешимости (α_min тинта над фоном).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlphaVector {
    /// Тинт (кроющий цвет), `#RRGGBB`.
    pub tint: String,
    /// Запрошенная альфа, `[0,1]`.
    pub alpha: f64,
    /// Фон (подложка), `#RRGGBB`.
    pub bg: String,
    /// Композит `α·tint + (1−α)·bg`, квантованный до `#RRGGBB`.
    pub composite: String,
    /// Минимально разрешимая α, при которой тинт остаётся в гамуте над этим
    /// фоном (нижняя граница инверсии композита).
    pub min_alpha: f64,
}

/// Тройки (тинт, α, фон) для альфа-алгебры: нейтральный тинт labui над светлым
/// и тёмным фоном на разных уровнях лестницы, бренд и обязательный v2 half-tie
/// из ADR-0004, различающий byte-reference от старого нормализованного пути.
const ALPHA_CASES: [(&str, f64, &str); 7] = [
    ("#787880", 0.2, "#FFFFFF"),   // нейтральная заливка @20 на белом
    ("#787880", 0.122, "#FFFFFF"), // граница @12 на белом
    ("#787880", 0.361, "#101012"), // нейтральная заливка @36 на тёмном
    ("#007AFF", 0.122, "#FFFFFF"), // бренд-заливка @12 на белом
    ("#101012", 0.122, "#FFFFFF"), // тень @12 на белом
    ("#FFFFFF", 0.5, "#007AFF"),   // белый полупрозрачный на бренде
    ("#C0B2FA", 0.122, "#000000"), // half-tie: канал 250 × .122 = 30.5 → 31
];

/// Дериватор альфа-векторов.
#[must_use]
pub fn generate_alpha() -> Vec<AlphaVector> {
    ALPHA_CASES
        .iter()
        .map(|&(tint, alpha, bg)| {
            let composite = composite_hex(tint, alpha, bg).expect("валидные hex/α");
            let min_alpha = min_alpha_hex(tint, bg).expect("валидные hex");
            AlphaVector {
                tint: tint.to_string(),
                alpha,
                bg: bg.to_string(),
                composite,
                min_alpha,
            }
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Семейство: резолв (снапшоты токенов)
// ─────────────────────────────────────────────────────────────────────────────

/// Тип контракта резолва — параметризованный, в терминах публичного API ядра.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ContractSpec {
    /// Текст: цель Ys candidate score `Lc`, юридический пол WCAG AA-text (4.5:1).
    Text {
        /// Целевая кандидатная оценка `Lc` по `Ys`.
        lc: f64,
    },
    /// UI-элемент: цель Ys candidate score `Lc`, пол WCAG AA-UI (3:1).
    Ui {
        /// Целевая кандидатная оценка `Lc` по `Ys`.
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
    /// В [`Contract`] ядра.
    fn to_core(self) -> Contract {
        match self {
            ContractSpec::Text { lc } => Contract::text(lc),
            ContractSpec::Ui { lc } => Contract::ui(lc),
            ContractSpec::Range { floor, ceiling } => Contract::range(floor, ceiling),
        }
    }
}

/// Исход резолва: успешный цвет либо типизированный терминальный failure.
/// Категория и код проецируются из одного core-owned boundary descriptor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SolveOutcome {
    /// Контракт удовлетворён.
    Solved {
        /// Резолвнутый цвет, `#RRGGBB`.
        hex: String,
        /// Знаковая кандидатная оценка `Lc` по `Ys` на отданном hex.
        lc: f64,
        /// WCAG-ratio на отданном hex.
        wcag_ratio: f64,
        /// Frozen pre-cutover report that a caller-owned final-emission hard
        /// predicate moved the analytic candidate. The generic `solve` vectors
        /// declare no such predicate, so this remains `false` by construction.
        floor_override: bool,
    },
    /// Resolver не вернул цвет; category отделяет доказанную недостижимость от
    /// unresolved и rejected исходов.
    Failure {
        /// Стабильная семантическая категория core failure.
        category: String,
        /// Стабильный машинный код конкретной причины.
        code: String,
    },
}

/// Ошибка построения conformance-пака. Генератор не имеет права превращать
/// внутренний/неизвестный вариант ядра в физически правдоподобный solve outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PackGenerationError {
    /// Core-generated data violated a core postcondition.
    InternalCoreInvariant { reason: String },
    /// The core introduced a failure without a public boundary descriptor. The
    /// adapter must be upgraded before regenerating artifacts.
    IncompatibleCoreContract { reason: String },
}

impl core::fmt::Display for PackGenerationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InternalCoreInvariant { reason } => {
                write!(f, "internal core invariant failure: {reason}")
            }
            Self::IncompatibleCoreContract { reason } => {
                write!(f, "incompatible core contract: {reason}")
            }
        }
    }
}

impl std::error::Error for PackGenerationError {}

/// Core-owned публичная проекция failure. Conformance не поддерживает второй
/// словарь категорий/кодов: внутренний failure либо будущий вариант без
/// boundary descriptor закрывает генерацию целиком.
pub fn solve_failure_wire(
    err: &labcolors_core::SolveFailure,
) -> Result<(&'static str, &'static str), PackGenerationError> {
    if let Some(boundary) = err.boundary() {
        return Ok((boundary.category().as_str(), boundary.code()));
    }
    match err {
        labcolors_core::SolveFailure::InternalInvariant(reason) => {
            Err(PackGenerationError::InternalCoreInvariant {
                reason: reason.clone(),
            })
        }
        _ => Err(PackGenerationError::IncompatibleCoreContract {
            reason: err.to_string(),
        }),
    }
}

/// Один вектор резолва: вход (bg, контракт, тема) и канонический исход.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolveVector {
    /// Фон, `#RRGGBB`.
    pub bg: String,
    /// Контракт резолва.
    pub contract: ContractSpec,
    /// Тема просмотра (kebab-ключ).
    pub theme: String,
    /// Канонический исход.
    pub outcome: SolveOutcome,
}

/// Кейсы резолва: успешные контракты на светлом фоне и реальные публичные
/// failure paths на тёмном, брендовом и среднем фоне. Каждый failure остаётся
/// типизированным терминалом, не тихим клипом.
const SOLVE_CASES: [(&str, ContractSpec, &str); 8] = [
    ("#FFFFFF", ContractSpec::Text { lc: 60.0 }, "light"),
    ("#FFFFFF", ContractSpec::Ui { lc: 45.0 }, "light"),
    (
        "#FFFFFF",
        ContractSpec::Range {
            floor: 30.0,
            ceiling: 60.0,
        },
        "light",
    ),
    ("#101012", ContractSpec::Text { lc: 75.0 }, "dark"),
    ("#007AFF", ContractSpec::Text { lc: 60.0 }, "light"),
    // Floorless target lies in the frozen candidate curve's open dead-zone gap: neither exact zero
    // nor the minimum non-zero boundary is within the declared ±1 Lc budget.
    (
        "#FFFFFF",
        ContractSpec::Range {
            floor: 3.0,
            ceiling: 5.0,
        },
        "light",
    ),
    // W5 regression: the generic candidate-score solver no longer infers a
    // WCAG criterion from the legacy `Text` transport tag. This mid-grey case
    // therefore characterises the explicit Ys target instead of the removed
    // implicit-floor failure branch.
    ("#6E6E6E", ContractSpec::Text { lc: 20.0 }, "light"),
    // Намеренно недостижимо: цель Lc 150 превышает всё, что белый фон способен
    // дать (макс ≈ 107 у чёрного) → typed ExceedsRange, не клип.
    ("#FFFFFF", ContractSpec::Text { lc: 150.0 }, "light"),
];

/// Дериватор резолв-векторов. Нейтральная (серая) хрома — резолв
/// хью-независим, вектор детерминирован; хью/хрома — естественное расширение
/// среза (см. PR).
pub fn generate_solve() -> Result<Vec<SolveVector>, PackGenerationError> {
    SOLVE_CASES
        .iter()
        .map(
            |&(bg, contract, theme)| -> Result<SolveVector, PackGenerationError> {
                let vc = vc_for_theme(theme);
                let bg_input = BgInput::solid(bg).expect("валидный hex фона");
                let outcome = match solve(
                    bg_input,
                    contract.to_core(),
                    Hue::deg(0.0),
                    ChromaPolicy::Neutral,
                    &vc,
                    Gamut::Srgb,
                ) {
                    Ok(s) => {
                        let report = recheck_against(bg, &[s.hex()], &vc).map_err(|reason| {
                            PackGenerationError::InternalCoreInvariant {
                                reason: format!(
                                    "generated solved colour failed report projection: {reason}"
                                ),
                            }
                        })?;
                        let [(measured_lc, wcag_ratio)] = report.as_slice() else {
                            return Err(PackGenerationError::InternalCoreInvariant {
                                reason: format!(
                                    "single solved colour produced {} report entries",
                                    report.len()
                                ),
                            });
                        };
                        if measured_lc.to_bits() != s.lc().to_bits() {
                            return Err(PackGenerationError::InternalCoreInvariant {
                                reason: format!(
                                    "solved/report Lc mismatch for {}: {} != {}",
                                    s.hex(),
                                    s.lc(),
                                    measured_lc
                                ),
                            });
                        }
                        SolveOutcome::Solved {
                            hex: s.hex().to_string(),
                            lc: s.lc(),
                            wcag_ratio: *wcag_ratio,
                            floor_override: s.final_emission_adjusted(),
                        }
                    }
                    Err(error) => {
                        let (category, code) = solve_failure_wire(&error)?;
                        SolveOutcome::Failure {
                            category: category.to_string(),
                            code: code.to_string(),
                        }
                    }
                };
                Ok(SolveVector {
                    bg: bg.to_string(),
                    contract,
                    theme: theme.to_string(),
                    outcome,
                })
            },
        )
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Семейство: exact WCAG 2.2 final-sRGB8 assessment
// ─────────────────────────────────────────────────────────────────────────────

/// One cross-runtime exact WCAG 2.2 assessment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wcag22Vector {
    /// Final foreground bytes.
    pub foreground: String,
    /// Final background bytes.
    pub background: String,
    /// Explicit occurrence-level criterion key.
    pub criterion: String,
    /// Immutable evaluator profile.
    pub profile_id: String,
    /// Exact terminal decision.
    pub decision: String,
    /// Q55 bounds are decimal strings to remain exact in JavaScript.
    pub foreground_lower_q55: String,
    pub foreground_upper_q55: String,
    pub background_lower_q55: String,
    pub background_upper_q55: String,
    pub q55_scale: String,
    /// Sealed evidence identities and digests.
    pub evidence_kind: String,
    pub artifact_id: String,
    pub artifact_sha256: String,
    pub bound_id: String,
    pub proof_id: String,
    pub proof_sha256: String,
    pub proof_payload_sha256: String,
    pub generator_sha256: String,
    pub verifier_sha256: String,
    pub profile_checksum: String,
    pub profile_sha256: String,
}

const WCAG22_CASES: [(&str, &str, labcolors_core::wcag22::Wcag22CriterionV1); 6] = [
    (
        "#000000",
        "#FFFFFF",
        labcolors_core::wcag22::Wcag22CriterionV1::Sc143TextDefault,
    ),
    (
        "#FFFFFF",
        "#000000",
        labcolors_core::wcag22::Wcag22CriterionV1::Sc1411GraphicalObject,
    ),
    (
        "#89BB09",
        "#8212DB",
        labcolors_core::wcag22::Wcag22CriterionV1::Sc1411UiComponentOrState,
    ),
    (
        "#898CB8",
        "#3E2217",
        labcolors_core::wcag22::Wcag22CriterionV1::Sc143TextDefault,
    ),
    (
        "#8A8A8A",
        "#FFFFFF",
        labcolors_core::wcag22::Wcag22CriterionV1::Sc143TextDefault,
    ),
    (
        "#8A8A8A",
        "#FFFFFF",
        labcolors_core::wcag22::Wcag22CriterionV1::Sc143TextLargeScale,
    ),
];

/// Generate exact vectors by transporting the core assessment, never by
/// reimplementing the formula in the conformance crate.
pub fn generate_wcag22() -> Result<Vec<Wcag22Vector>, PackGenerationError> {
    use labcolors_core::NumericalDecisionEvidenceV1;
    use labcolors_core::wcag22::{Wcag22ApplicableDecisionV1, Wcag22AssessmentV1};

    WCAG22_CASES
        .iter()
        .map(|&(foreground, background, criterion)| {
            let assessment =
                labcolors_core::wcag22::evaluate_wcag22_hex(foreground, background, criterion)
                    .map_err(|error| PackGenerationError::InternalCoreInvariant {
                        reason: error.to_string(),
                    })?;
            let Wcag22AssessmentV1::Evaluated {
                profile_id,
                criterion: assessed_criterion,
                measurement,
                decision,
                evidence,
                ..
            } = assessment
            else {
                return Err(PackGenerationError::InternalCoreInvariant {
                    reason: "pair evaluator returned NotEvaluated".to_string(),
                });
            };
            let NumericalDecisionEvidenceV1::CanonicalFiniteBounded(evidence_payload) = evidence
            else {
                return Err(PackGenerationError::IncompatibleCoreContract {
                    reason: "WCAG22 evidence is not canonical-finite-bounded".to_string(),
                });
            };
            let artifact_id = evidence_payload.artifact_id();
            let bound_id = evidence_payload.bound_id();
            let proof_id = evidence_payload.proof_id();
            let profile = labcolors_core::wcag22::wcag22_profile_v1();
            let criterion = assessed_criterion.key();
            let decision = match decision {
                Wcag22ApplicableDecisionV1::Pass => "pass",
                Wcag22ApplicableDecisionV1::Fail => "fail",
                _ => {
                    return Err(PackGenerationError::IncompatibleCoreContract {
                        reason: "unknown WCAG22 decision".to_string(),
                    });
                }
            };
            let hex = |bytes: [u8; 3]| format!("#{:02X}{:02X}{:02X}", bytes[0], bytes[1], bytes[2]);
            Ok(Wcag22Vector {
                foreground: hex(measurement.foreground),
                background: hex(measurement.background),
                criterion: criterion.to_string(),
                profile_id: profile_id.key().to_string(),
                decision: decision.to_string(),
                foreground_lower_q55: measurement.foreground_luminance.lower().to_string(),
                foreground_upper_q55: measurement.foreground_luminance.upper().to_string(),
                background_lower_q55: measurement.background_luminance.lower().to_string(),
                background_upper_q55: measurement.background_luminance.upper().to_string(),
                q55_scale: labcolors_core::wcag22::Wcag22LuminanceBoundsQ55V1::scale().to_string(),
                evidence_kind: "canonical-finite-bounded".to_string(),
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
            })
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Агрегат пака + сериализация + дайджест
// ─────────────────────────────────────────────────────────────────────────────

/// Имя файла манифеста в каталоге векторов.
pub const MANIFEST_FILE: &str = "manifest.json";

/// Имена файлов семейств в КАНОНИЧЕСКОМ порядке — единый источник порядка для
/// генератора, дайджеста и раннера-референса (дайджест зависит от порядка).
pub const FAMILY_FILES: [&str; 5] = [
    "contrasts.json",
    "ladders.json",
    "alpha.json",
    "solve.json",
    "wcag22.json",
];

/// Каноническая толерантность сравнения f64 для conformance-
/// пака. Эта константа — SSOT правила сравнения. Наблюдаемый libm-шум
/// (`powf`/`atan2`/`ln` расходятся на несколько ULP между платформами) —
/// порядка `1e-13`; реальный дрейф (не тот surround, опечатка в матрице,
/// путаница единиц) сдвигает значения на целые единицы. `1e-6` заведомо выше
/// наблюдавшегося шума и заведомо ниже настоящей регрессии из corpus. Для
/// libm-dependent путей bit identity между runtime не гарантируется; evidence
/// ограничено аттестованной матрицей. Поэтому conformant-ность числовых полей
/// определяется этой толерантностью (hex/enum/строки — по своему профилю).
pub const DRIFT_TOL: f64 = 1e-6;

/// Счётчики векторов по семействам — для манифеста и отчёта PR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    /// Контраст-векторы.
    pub contrasts: usize,
    /// Векторы лестниц.
    pub ladders: usize,
    /// Альфа-векторы.
    pub alpha: usize,
    /// Резолв-векторы.
    pub solve: usize,
    /// Exact WCAG 2.2 vectors.
    pub wcag22: usize,
    /// Итого.
    pub total: usize,
}

/// Canonical numerical capability manifest (#289/#292): core registry
/// projection c versioned schema, coverage и drift-checksum. Replaces the
/// former `numericalSites[].legacyProfile` rows (pack 2.x); the verifier
/// consumes this projection instead of a hand-written semantic registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityManifestProjectionV2 {
    /// Capability schema version (independent version domain).
    pub schema_version: u32,
    /// Registry coverage key (`migrated-sites-only-v1`).
    pub coverage: String,
    /// Capability rows sorted by UTF-8 `siteId` bytes.
    pub sites: Vec<CapabilitySiteProjectionV2>,
    /// FNV-1a-32 drift-checksum canonical preimage, 8 lowercase hex.
    pub checksum: String,
}

/// One site capability row (no selected mode; manifest describes the build).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySiteProjectionV2 {
    /// Stable site identity key.
    pub site_id: String,
    /// Lawful stable outcome keys.
    pub stable_outcomes: Vec<String>,
    /// Registered compatibility release keys.
    pub compatibility_releases: Vec<String>,
    /// Mintable evidence class keys.
    pub evidence_classes: Vec<String>,
    /// Canonical finite artifact IDs (empty = no evidence, not implicit support).
    pub artifact_ids: Vec<String>,
    /// Registered error bound IDs (empty when none are admitted).
    pub bound_ids: Vec<String>,
    /// Replayable proof artifact IDs.
    pub proof_ids: Vec<String>,
    /// Runtime attestation IDs (empty until #258).
    pub runtime_attestations: Vec<String>,
}

/// Generate the release-facing capability manifest directly from the core SSOT.
#[must_use]
pub fn generate_capability_manifest_v2() -> CapabilityManifestProjectionV2 {
    let manifest = labcolors_core::numerical_capability_manifest_v2();
    CapabilityManifestProjectionV2 {
        schema_version: manifest.schema_version,
        coverage: manifest.coverage.key().to_string(),
        sites: manifest
            .sites
            .iter()
            .map(|site| CapabilitySiteProjectionV2 {
                site_id: site.site_id.key().to_string(),
                stable_outcomes: site
                    .stable_outcomes
                    .iter()
                    .map(|v| v.key().to_string())
                    .collect(),
                compatibility_releases: site
                    .compatibility_releases
                    .iter()
                    .map(|v| v.key().to_string())
                    .collect(),
                evidence_classes: site
                    .evidence_classes
                    .iter()
                    .map(|v| v.key().to_string())
                    .collect(),
                artifact_ids: site
                    .artifact_ids
                    .iter()
                    .map(|v| v.key().to_string())
                    .collect(),
                bound_ids: site.bound_ids.iter().map(|v| v.key().to_string()).collect(),
                proof_ids: site.proof_ids.iter().map(|v| v.key().to_string()).collect(),
                runtime_attestations: site
                    .runtime_attestations
                    .iter()
                    .map(|v| v.key().to_string())
                    .collect(),
            })
            .collect(),
        checksum: manifest.checksum.hex(),
    }
}

/// Манифест пака: версии, дайджест и счётчики. `packDigest` — FNV-1a-32
/// (примитив ядра) над каноническими байтами всех семейств; любой дрейф
/// значений или состава меняет дайджест.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// Версия схемы/состава пака ([`PACK_VERSION`]).
    pub pack_version: String,
    /// Версия ядра, к которой привязан пак ([`core_version`]).
    pub core_version: String,
    /// FNV-1a-32 над каноническими байтами семейств, 8 hex-символов.
    pub pack_digest: String,
    /// Счётчики по семействам.
    pub counts: Counts,
    /// Canonical numerical capability manifest (core registry projection).
    pub numerical_capabilities: CapabilityManifestProjectionV2,
}

/// Весь пак в памяти. `serialize_family` даёт КАНОНИЧЕСКИЕ байты каждого файла
/// (pretty JSON, LF), из которых считается дайджест и которые пишет `gen`.
#[derive(Debug, Clone, PartialEq)]
pub struct Pack {
    /// Контраст-векторы.
    pub contrasts: Vec<ContrastVector>,
    /// Векторы лестниц.
    pub ladders: Vec<LadderVector>,
    /// Альфа-векторы.
    pub alpha: Vec<AlphaVector>,
    /// Резолв-векторы.
    pub solve: Vec<SolveVector>,
    /// Exact final-sRGB8 WCAG 2.2 vectors.
    pub wcag22: Vec<Wcag22Vector>,
}

impl Pack {
    /// Сгенерировать весь пак из канона ядра.
    pub fn generate() -> Result<Self, PackGenerationError> {
        Ok(Pack {
            contrasts: generate_contrasts(),
            ladders: generate_ladders(),
            alpha: generate_alpha(),
            solve: generate_solve()?,
            wcag22: generate_wcag22()?,
        })
    }

    /// Счётчики семейств.
    #[must_use]
    pub fn counts(&self) -> Counts {
        let contrasts = self.contrasts.len();
        let ladders = self.ladders.len();
        let alpha = self.alpha.len();
        let solve = self.solve.len();
        let wcag22 = self.wcag22.len();
        Counts {
            contrasts,
            ladders,
            alpha,
            solve,
            wcag22,
            total: contrasts + ladders + alpha + solve + wcag22,
        }
    }

    /// Дайджест пака: FNV-1a-32 над конкатенацией канонических байтов всех
    /// семейств в порядке [`FAMILY_FILES`]. 8 hex-символов. Значение зависит от
    /// платформы генерации (последний ULP f64 в сериализации) — это отпечаток
    /// КОНКРЕТНОГО закоммиченного артефакта, а не кросс-платформенный инвариант.
    #[must_use]
    pub fn digest(&self) -> String {
        let mut buf = String::new();
        for (_name, bytes) in self.families() {
            buf.push_str(&bytes);
        }
        format!("{:08x}", fnv1a_32(buf.as_bytes()))
    }

    /// Манифест пака (версии, дайджест, счётчики).
    #[must_use]
    pub fn manifest(&self) -> Manifest {
        Manifest {
            pack_version: PACK_VERSION.to_string(),
            core_version: core_version().to_string(),
            pack_digest: self.digest(),
            counts: self.counts(),
            numerical_capabilities: generate_capability_manifest_v2(),
        }
    }

    /// Пары `(имя_файла, канонические_байты)` каждого семейства в порядке
    /// [`FAMILY_FILES`]. Канонические байты — pretty JSON с LF-переводами строк.
    #[must_use]
    pub fn families(&self) -> Vec<(&'static str, String)> {
        vec![
            (FAMILY_FILES[0], to_canonical_json(&self.contrasts)),
            (FAMILY_FILES[1], to_canonical_json(&self.ladders)),
            (FAMILY_FILES[2], to_canonical_json(&self.alpha)),
            (FAMILY_FILES[3], to_canonical_json(&self.solve)),
            (FAMILY_FILES[4], to_canonical_json(&self.wcag22)),
        ]
    }
}

/// Канонический JSON: pretty-печать (2 пробела) + завершающий перевод строки,
/// LF везде. Детерминирован по построению (serde_json + ryu), одинаков на любой
/// платформе — основа для дайджеста и чистого diff.
#[must_use]
pub fn to_canonical_json<T: Serialize>(value: &T) -> String {
    let mut s = serde_json::to_string_pretty(value).expect("векторы пака всегда сериализуемы");
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use labcolors_core::numerical_registry_v2;

    #[test]
    fn wcag22_vector_generator_has_no_private_wire_key_vocabulary() {
        let private_prefix: String = ['"', 's', 'c', '-', '1', '.', '4', '.']
            .into_iter()
            .collect();
        assert!(
            !include_str!("lib.rs").contains(&private_prefix),
            "WCAG22 vector keys must come from Wcag22CriterionV1::key()"
        );
    }

    #[test]
    fn solve_failure_wire_is_the_core_projection_and_fails_closed() {
        use labcolors_core::SolveFailure as F;

        let fixtures = [
            (
                F::BelowContrastFloor { target: 1.0 },
                "unreachable",
                "below_contrast_floor",
            ),
            (
                F::ExceedsRange {
                    target: 100.0,
                    max_achievable: 90.0,
                },
                "unreachable",
                "exceeds_range",
            ),
            (
                F::BoundedSearchExhausted {
                    target: 50.0,
                    closest_examined: 49.0,
                },
                "unresolved",
                "bounded_search_exhausted",
            ),
            (
                F::InvalidInput("fixture".into()),
                "rejected",
                "invalid_input",
            ),
        ];
        for (failure, category, code) in fixtures {
            assert_eq!(solve_failure_wire(&failure).unwrap(), (category, code));
        }

        let internal = labcolors_core::SolveFailure::InternalInvariant("fixture drift".into());
        assert!(matches!(
            solve_failure_wire(&internal),
            Err(PackGenerationError::InternalCoreInvariant { reason })
                if reason == "fixture drift"
        ));
    }

    #[test]
    fn generation_is_deterministic() {
        // Дважды сгенерированный пак байт-идентичен — вход всегда даёт тот же
        // выход (детерминизм канона, перенесённый в пак).
        let a = Pack::generate().expect("canonical pack generation");
        let b = Pack::generate().expect("canonical pack generation");
        assert_eq!(a, b, "генерация пака недетерминирована");
        assert_eq!(a.digest(), b.digest(), "дайджест недетерминирован");
    }

    #[test]
    fn counts_are_nonempty_and_consistent() {
        let pack = Pack::generate().expect("canonical pack generation");
        let c = pack.counts();
        assert!(c.total > 0, "пустой пак бессмыслен");
        assert_eq!(
            c.total,
            c.contrasts + c.ladders + c.alpha + c.solve + c.wcag22,
            "итог не сходится с семействами"
        );
        // Лестниц ровно столько, сколько канонических позиций.
        assert_eq!(c.ladders, LadderPosition::ALL.len());
    }

    #[test]
    fn manifest_numerical_registry_is_generated_from_core_ssot() {
        let manifest = Pack::generate()
            .expect("canonical pack generation")
            .manifest();
        assert_eq!(
            manifest.numerical_capabilities,
            generate_capability_manifest_v2()
        );
        assert_eq!(
            manifest.numerical_capabilities.sites.len(),
            numerical_registry_v2().len()
        );
        assert!(manifest.numerical_capabilities.sites.iter().any(|site| {
            site.site_id == "glow-target-or-maximum-v1"
                && site.stable_outcomes == ["bit-exact", "indeterminate"]
                && site.compatibility_releases == ["glow-cam16-ucs-jprime-target-or-max-v1"]
                && site.evidence_classes == ["bit-exact"]
                && site.artifact_ids.is_empty()
                && site.bound_ids.is_empty()
                && site.proof_ids.is_empty()
                && site.runtime_attestations.is_empty()
        }));
        assert!(manifest.numerical_capabilities.sites.iter().any(|site| {
            site.site_id == "wcag22-srgb8-contrast-v1"
                && site.evidence_classes == ["canonical-finite-bounded"]
                && site.artifact_ids == ["wcag22-srgb8-luminance-q55-v1"]
                && site.bound_ids == ["wcag22-srgb8-outward-q55-v1"]
                && site.proof_ids == ["wcag22-srgb8-full-domain-q55-v1"]
        }));
        assert_eq!(manifest.numerical_capabilities.schema_version, 2);
        // Checksum canonical projection: 8 hex, независимо пересчитываем в core.
        assert_eq!(manifest.numerical_capabilities.checksum.len(), 8);
    }

    #[test]
    fn pack_v10_removes_only_the_muddiness_family() {
        // Объём этого лока: версия пака/ядра, счётчики семейств (total 86) и
        // обязательный half-tie-вектор ADR-0004 (нормализованный
        // `(byte/255) * alpha * 255` путь ошибочно отдавал соседний LSB —
        // обязательство унаследовано с pack v2). Байт-в-байт неизменность
        // остальных семейств доказывает `tests/pack_v10_contract.rs`
        // (SHA-256-пины), не эта функция.
        let pack = Pack::generate().expect("canonical pack generation");
        let manifest = pack.manifest();
        assert_eq!(
            PACK_VERSION, "10.0.0",
            "вырезание muddiness-семейства обязано быть pack v10"
        );
        assert_eq!(manifest.pack_version, PACK_VERSION);
        assert_eq!(
            manifest.core_version, "0.3.0",
            "pack v10 остаётся привязан к core 0.3.0"
        );
        assert_eq!(
            pack.alpha.len(),
            7,
            "alpha-family v2+ обязана иметь 7 векторов"
        );
        assert_eq!(manifest.counts.alpha, pack.alpha.len());
        assert_eq!(
            manifest.counts.total, 86,
            "состав векторных семейств изменился"
        );
        assert_eq!(manifest.counts.wcag22, 6);

        let half_tie = pack
            .alpha
            .iter()
            .find(|v| v.tint == "#C0B2FA" && v.alpha == 0.122 && v.bg == "#000000")
            .expect("в паке нет обязательного half-tie из ADR-0004");
        assert_eq!(
            half_tie.composite, "#17161F",
            "byte-reference round-half-up должен выбрать верхний LSB"
        );
    }

    #[test]
    fn canonical_json_serialization_is_deterministic() {
        // Канонический JSON — чистая функция структуры (serde_json + ryu):
        // дважды сериализованное семейство БАЙТ-идентично. Это фундамент
        // байт-точного гейта дрейфа и дайджеста (сравнение по СЕРИАЛИЗАЦИИ, не
        // по parse — парсер serde_json по умолчанию не round-trip-точен для
        // f64, поэтому опираемся на детерминизм сериализации, а не парсинга).
        let pack = Pack::generate().expect("canonical pack generation");
        assert_eq!(
            to_canonical_json(&pack.contrasts),
            to_canonical_json(&pack.contrasts)
        );
        assert_eq!(
            to_canonical_json(&pack.solve),
            to_canonical_json(&pack.solve)
        );
        // Разбор валиден структурно (форма контракта), даже если последний ULP
        // f64 может отличаться — семантическую точность держит tolerance пака.
        let _parsed: Vec<ContrastVector> =
            serde_json::from_str(&to_canonical_json(&pack.contrasts)).unwrap();
    }

    #[test]
    fn solve_pack_contains_solved_and_typed_failure() {
        // Anti-vacuum: corpus исполняет обе ветви и закрепляет категорию вместе
        // с конкретным кодом, а не только новый serde-тег. W5 intentionally
        // removes the implicit criterion from generic `Contract::text`, so the
        // former implicit-floor failure case is now a solved candidate-score vector;
        // explicit final-criterion unreachability is covered in Core/WCAG tests.
        let solve = generate_solve().expect("canonical solve vectors");
        assert!(
            solve
                .iter()
                .any(|v| matches!(v.outcome, SolveOutcome::Solved { .. })),
            "нет ни одного успешного резолва"
        );
        let failures = solve
            .iter()
            .filter_map(|v| match &v.outcome {
                SolveOutcome::Failure { category, code } => Some((category.clone(), code.clone())),
                SolveOutcome::Solved { .. } => None,
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            failures,
            std::collections::BTreeSet::from([
                ("unreachable".into(), "below_contrast_floor".into()),
                ("unreachable".into(), "exceeds_range".into()),
            ]),
            "полный набор категорий/кодов failure сменился"
        );
    }
}
