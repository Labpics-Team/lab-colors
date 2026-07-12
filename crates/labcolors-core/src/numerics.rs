//! Типизированные решения для branch-sensitive численных sites (#281, #292).
//!
//! Diagnostic `f64` не становится semantic verdict сам по себе. Determinate
//! разрешён только вместе с объявленной гарантией; при отсутствии sound bound
//! или пересечении outward-интервала с границей возвращается `Indeterminate`.
//!
//! Три уровня контракта (#292) разделены типами и не подменяют друг друга:
//!
//! ```text
//! package capability      — registry-строка: что пакет ВООБЩЕ умеет доказать
//!                           для site (bound_status, stable_outcomes, legacy)
//! compiled invocation plan — [`CompiledNumericalPlanV1`]: что БУДЕТ исполнено
//!                           для site при запрошенном профиле; результата не
//!                           содержит и доказательством не является
//! result evidence          — запечатанные свидетельства исполнения
//!                           ([`SoundIntervalEvidenceV1`]); конструируемы
//!                           только производителем, не вызывающим кодом
//! ```
//!
//! Caller-created значение (голый интервал из двух `f64`) НЕ повышается до
//! sound evidence: классификатор принимает только запечатанное свидетельство,
//! а план компилируется fail-closed из machine-readable registry-строки.

/// Stable outcomes admitted for a migrated branch-sensitive site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StableNumericalOutcomeV1 {
    /// Determinate branch follows from exact finite/integer/rational evidence.
    BitExact,
    /// No semantic branch is selected.
    Indeterminate,
}

impl StableNumericalOutcomeV1 {
    /// Stable manifest key.
    pub fn key(self) -> &'static str {
        match self {
            Self::BitExact => "bit-exact",
            Self::Indeterminate => "indeterminate",
        }
    }
}

/// Sound-bound availability for a migrated numerical site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericalBoundStatusV1 {
    /// No sound bound has been admitted.
    Unavailable,
}

impl NumericalBoundStatusV1 {
    /// Stable manifest key.
    pub fn key(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
        }
    }
}

/// Whether a stable profile may silently choose another decision path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericalFallbackStatusV1 {
    /// No fallback; compatibility requires an explicit profile.
    None,
}

impl NumericalFallbackStatusV1 {
    /// Stable manifest key.
    pub fn key(self) -> &'static str {
        match self {
            Self::None => "none",
        }
    }
}

/// Machine-readable registry row required by research lock #281.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct NumericalSiteRecordV1 {
    /// Stable site identity.
    pub site_id: NumericalSiteIdV1,
    /// Branch-sensitive operations.
    pub operations: &'static str,
    /// Input/output domain.
    pub domain: &'static str,
    /// Semantic branch affected by the value.
    pub branch_effect: &'static str,
    /// Lawful stable outcomes.
    pub stable_outcomes: &'static [StableNumericalOutcomeV1],
    /// Sound-bound availability.
    pub bound_status: NumericalBoundStatusV1,
    /// Executable boundary corpus identifiers.
    pub boundary_corpus: &'static str,
    /// Required cross-runtime comparison scope.
    pub runtime_matrix: &'static str,
    /// Fallback status.
    pub fallback_status: NumericalFallbackStatusV1,
    /// Explicit compatibility profile, if one exists.
    pub legacy_profile: Option<&'static str>,
}

// Enum identity and its registry row are emitted by one declaration. A new
// migrated site therefore cannot exist without machine-readable metadata.
macro_rules! define_numerical_registry_v1 {
    ($(
        $(#[$variant_meta:meta])*
        $variant:ident => {
            key: $key:literal,
            operations: $operations:literal,
            domain: $domain:literal,
            branch_effect: $branch_effect:literal,
            stable_outcomes: [$($stable_outcome:path),+ $(,)?],
            bound_status: $bound_status:path,
            boundary_corpus: $boundary_corpus:literal,
            runtime_matrix: $runtime_matrix:literal,
            fallback_status: $fallback_status:path,
            legacy_profile: $legacy_profile:expr $(,)?
        }
    ),+ $(,)?) => {
        /// Зарегистрированный migrated site, где число влияет на semantic branch.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[non_exhaustive]
        pub enum NumericalSiteIdV1 {
            $($(#[$variant_meta])* $variant),+
        }

        impl NumericalSiteIdV1 {
            /// Стабильный wire/registry key.
            pub fn key(self) -> &'static str {
                match self {
                    $(Self::$variant => $key),+
                }
            }
        }

        const NUMERICAL_REGISTRY_V1: &[NumericalSiteRecordV1] = &[
            $(NumericalSiteRecordV1 {
                site_id: NumericalSiteIdV1::$variant,
                operations: $operations,
                domain: $domain,
                branch_effect: $branch_effect,
                stable_outcomes: &[$($stable_outcome),+],
                bound_status: $bound_status,
                boundary_corpus: $boundary_corpus,
                runtime_matrix: $runtime_matrix,
                fallback_status: $fallback_status,
                legacy_profile: $legacy_profile,
            }),+
        ];
    };
}

define_numerical_registry_v1! {
    /// Glow: первый state, достигший target, либо глобальный максимум.
    GlowTargetOrMaximumV1 => {
        key: "glow-target-or-maximum-v1",
        operations: "CAM16 forward powf; CAM16-UCS J-prime; abs; target >=; maximum ordering",
        domain: "encoded sRGB8 point screen states -> diagnostic CAM16-UCS delta J-prime",
        branch_effect: "first reached state versus global maximum and reached/unreachable status",
        stable_outcomes: [
            StableNumericalOutcomeV1::BitExact,
            StableNumericalOutcomeV1::Indeterminate,
        ],
        bound_status: NumericalBoundStatusV1::Unavailable,
        boundary_corpus: "glow stable-indeterminate; exact no-op; finite-state compositor; half-tie alpha",
        runtime_matrix: "active: native x86_64 + wasm32; native arm64 required before any cross-runtime CAM16 decision claim; exact bytes only for compositor",
        fallback_status: NumericalFallbackStatusV1::None,
        legacy_profile: Some(crate::glow::GlowDecisionProfileV1::LegacyPlatformDependentV1.key()),
    },
}

/// Registry уже мигрированных typed-decision sites V1.
///
/// Это не заявление о завершённом аудите старых `f64` branches: его владелец —
/// #291. Новый site, переводимый на stable typed decision, обязан получить
/// строку до изменения runtime behavior.
pub fn numerical_registry_v1() -> &'static [NumericalSiteRecordV1] {
    NUMERICAL_REGISTRY_V1
}

/// Конечный упорядоченный интервал-ЗАЯВЛЕНИЕ `[lower, upper]`.
///
/// Конструктор проверяет только форму (конечность, порядок). Сам по себе тип
/// НЕ доказательство того, что истинное значение лежит внутри: доказанность
/// принадлежит исключительно запечатанному [`SoundIntervalEvidenceV1`],
/// произведённому допущенным backend'ом. Ни один такой backend сегодня не
/// допущен ([`NumericalBoundStatusV1::Unavailable`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutwardIntervalV1 {
    lower: f64,
    upper: f64,
}

impl OutwardIntervalV1 {
    /// Создаёт конечный непустой интервал; перепутанные/NaN bounds — ошибка.
    pub fn try_new(lower: f64, upper: f64) -> Result<Self, String> {
        if !lower.is_finite() || !upper.is_finite() || lower > upper {
            return Err(format!(
                "outward interval вне домена: lower={lower}, upper={upper}"
            ));
        }
        Ok(Self { lower, upper })
    }

    /// Нижняя доказанная граница.
    pub fn lower(self) -> f64 {
        self.lower
    }

    /// Верхняя доказанная граница.
    pub fn upper(self) -> f64 {
        self.upper
    }
}

/// Запрошенный класс исполнения при компиляции плана вызова (#292).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericalProfileRequestV1 {
    /// Только доказуемые методы: exact finite state, иначе честный отказ.
    StableExactV1,
    /// Явно запрошенный прежний platform/libm-dependent путь (compatibility).
    LegacyPlatformDependentV1,
}

impl NumericalProfileRequestV1 {
    /// Стабильный wire/registry key.
    pub fn key(self) -> &'static str {
        match self {
            Self::StableExactV1 => "stable-exact-v1",
            Self::LegacyPlatformDependentV1 => "legacy-platform-dependent-v1",
        }
    }
}

/// Метод решения, допущенный скомпилированным планом.
///
/// Интервального метода в типе НЕТ намеренно: ни один sound-bound backend не
/// допущен ([`NumericalBoundStatusV1::Unavailable`]), поэтому план физически
/// не может пообещать интервальное доказательство — это тип-уровневая форма
/// текущей package capability, а не пропуск.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PlannedDecisionMethodV1 {
    /// Точная проверка конечного integer/байтового состояния домена.
    ExactFiniteStateV1,
    /// Честный typed-отказ от stable branch, где exact не применим.
    RefuseIndeterminateV1,
    /// Явно выбранный legacy platform-dependent путь (не evidence-класс).
    LegacyPlatformDependentV1,
}

/// Скомпилированный план вызова: что будет исполнено для site при данном
/// запросе. План выводится fail-closed из machine-readable registry-строки
/// (package capability) и НЕ содержит результата — план ≠ evidence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompiledNumericalPlanV1 {
    site_id: NumericalSiteIdV1,
    request: NumericalProfileRequestV1,
    methods: [Option<PlannedDecisionMethodV1>; 2],
}

impl CompiledNumericalPlanV1 {
    /// Компилирует план для зарегистрированного site.
    ///
    /// # Errors
    ///
    /// Site отсутствует в registry либо запрошенный профиль незаконен для его
    /// capability-строки (например, legacy без объявленного профиля).
    pub fn compile(
        site_id: NumericalSiteIdV1,
        request: NumericalProfileRequestV1,
    ) -> Result<Self, String> {
        let row = numerical_registry_v1()
            .iter()
            .find(|row| row.site_id == site_id)
            .ok_or_else(|| format!("site {} отсутствует в registry V1", site_id.key()))?;
        Self::compile_from_row(row, request)
    }

    /// Компиляция из явной строки capability — отделена от lookup, чтобы
    /// fail-closed ветви были проверяемы на синтетических строках.
    pub(crate) fn compile_from_row(
        row: &NumericalSiteRecordV1,
        request: NumericalProfileRequestV1,
    ) -> Result<Self, String> {
        let methods = match request {
            NumericalProfileRequestV1::StableExactV1 => {
                // Методы выводятся из объявленных lawful outcomes строки:
                // BitExact → точный конечно-состоянийный метод, Indeterminate →
                // честный отказ. bound_status Unavailable не даёт интервального
                // метода — его нет и в типе метода.
                let exact = row
                    .stable_outcomes
                    .contains(&StableNumericalOutcomeV1::BitExact);
                let refuse = row
                    .stable_outcomes
                    .contains(&StableNumericalOutcomeV1::Indeterminate);
                if !exact && !refuse {
                    return Err(format!(
                        "site {} не объявляет ни одного stable outcome — stable-план невозможен",
                        row.site_id.key()
                    ));
                }
                [
                    exact.then_some(PlannedDecisionMethodV1::ExactFiniteStateV1),
                    refuse.then_some(PlannedDecisionMethodV1::RefuseIndeterminateV1),
                ]
            }
            NumericalProfileRequestV1::LegacyPlatformDependentV1 => {
                if row.legacy_profile.is_none() {
                    return Err(format!(
                        "site {} не объявляет legacy compatibility profile — legacy-план запрещён",
                        row.site_id.key()
                    ));
                }
                [
                    Some(PlannedDecisionMethodV1::LegacyPlatformDependentV1),
                    None,
                ]
            }
        };
        // Компактный план без «дыр»: методы в порядке исполнения.
        let mut packed = [None, None];
        for (slot, method) in methods.into_iter().flatten().enumerate() {
            packed[slot] = Some(method);
        }
        Ok(Self {
            site_id: row.site_id,
            request,
            methods: packed,
        })
    }

    /// Site, для которого скомпилирован план.
    pub fn site_id(&self) -> NumericalSiteIdV1 {
        self.site_id
    }

    /// Запрошенный профиль исполнения.
    pub fn request(&self) -> NumericalProfileRequestV1 {
        self.request
    }

    /// Методы в порядке исполнения (непустой по построению).
    pub fn methods(&self) -> impl Iterator<Item = PlannedDecisionMethodV1> + '_ {
        self.methods.iter().flatten().copied()
    }
}

/// Приватная печать: наличие поля этого типа делает внешнюю конструкцию
/// структуры литералом невозможной.
#[derive(Debug, Clone, Copy, PartialEq)]
struct EvidenceSeal;

/// Запечатанное интервальное свидетельство: interval с provenance
/// (site), произведённый ИСПОЛНИТЕЛЕМ, а не вызывающим кодом.
///
/// Публичного конструктора нет намеренно: production-производитель появится
/// только вместе с допущенным sound-bound backend'ом и обновлением
/// registry-строки (`bound_status`). До этого единственная фабрикация —
/// test-only, для закрепления семантики границы классификатора.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoundIntervalEvidenceV1 {
    site_id: NumericalSiteIdV1,
    interval: OutwardIntervalV1,
    _seal: EvidenceSeal,
}

impl SoundIntervalEvidenceV1 {
    /// Test-only фабрикация для проверки границ классификатора. НЕ является
    /// производством доказательств: закрепляет семантику `>=`-границы, а не
    /// доказанность интервала.
    #[cfg(test)]
    pub(crate) fn fabricated_for_boundary_tests(
        site_id: NumericalSiteIdV1,
        interval: OutwardIntervalV1,
    ) -> Self {
        Self {
            site_id,
            interval,
            _seal: EvidenceSeal,
        }
    }

    /// Site, которому принадлежит свидетельство.
    pub fn site_id(&self) -> NumericalSiteIdV1 {
        self.site_id
    }

    /// Заявленный интервал свидетельства.
    pub fn interval(&self) -> OutwardIntervalV1 {
        self.interval
    }
}

/// Неразделимое evidence, почему stable semantic decision нельзя принять.
///
/// Вариант владеет своим доказательством: невозможно сконструировать
/// `IntervalOverlap` без interval или приписать outward bound отсутствующему
/// sound model.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum NumericalIndeterminacyV1 {
    /// Для используемого transcendental backend нет sound error bound.
    SoundBoundUnavailable,
    /// Доказанный interval пересекает semantic boundary.
    IntervalOverlap(OutwardIntervalV1),
}

impl NumericalIndeterminacyV1 {
    /// Стабильный wire key причины.
    pub fn reason_key(self) -> &'static str {
        match self {
            Self::SoundBoundUnavailable => "sound-bound-unavailable",
            Self::IntervalOverlap(_) => "interval-overlap",
        }
    }
}

/// Класс доказательства determinate-решения.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum DecisionGuaranteeV1 {
    /// Решение следует только из exact integer/rational state.
    BitExact,
    /// Решение следует из непересекающегося outward-интервала.
    OutwardIntervalV1(OutwardIntervalV1),
    /// Явно выбранный прежний platform/libm-dependent путь.
    LegacyPlatformDependentV1,
}

impl DecisionGuaranteeV1 {
    /// Стабильный wire key.
    pub fn key(self) -> &'static str {
        match self {
            Self::BitExact => "bit-exact",
            Self::OutwardIntervalV1(_) => "outward-interval-v1",
            Self::LegacyPlatformDependentV1 => "legacy-platform-dependent-v1",
        }
    }

    /// Доказанный interval, если именно он является certificate решения.
    pub fn interval(self) -> Option<OutwardIntervalV1> {
        match self {
            Self::OutwardIntervalV1(interval) => Some(interval),
            Self::BitExact | Self::LegacyPlatformDependentV1 => None,
        }
    }
}

/// Semantic result с явным numerical proof class.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum NumericalDecisionV1<T> {
    /// Решение принято под указанной гарантией.
    Determinate {
        /// Предметный результат.
        value: T,
        /// Почему branch считается доказанным в объявленном профиле.
        guarantee: DecisionGuaranteeV1,
    },
    /// Stable branch не выбран.
    Indeterminate {
        /// Зарегистрированный site.
        site_id: NumericalSiteIdV1,
        /// Причина и её неразделимое numerical evidence.
        evidence: NumericalIndeterminacyV1,
    },
}

/// Результат проверки контракта `value >= target`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AtLeastDecisionV1 {
    /// Весь interval лежит ниже target.
    Below,
    /// Весь interval держит target.
    Meets,
}

/// Классифицирует `value >= target` только по ЗАПЕЧАТАННОМУ интервальному
/// свидетельству (#292): голый caller-created интервал этим путём не проходит
/// и потому не может быть повышен до determinate-гарантии.
pub fn classify_at_least_v1(
    proof: SoundIntervalEvidenceV1,
    target: f64,
) -> Result<NumericalDecisionV1<AtLeastDecisionV1>, String> {
    if !target.is_finite() {
        return Err(format!("target не конечен: {target}"));
    }
    let interval = proof.interval();
    if interval.lower() >= target {
        return Ok(NumericalDecisionV1::Determinate {
            value: AtLeastDecisionV1::Meets,
            guarantee: DecisionGuaranteeV1::OutwardIntervalV1(interval),
        });
    }
    if interval.upper() < target {
        return Ok(NumericalDecisionV1::Determinate {
            value: AtLeastDecisionV1::Below,
            guarantee: DecisionGuaranteeV1::OutwardIntervalV1(interval),
        });
    }
    Ok(NumericalDecisionV1::Indeterminate {
        site_id: proof.site_id(),
        evidence: NumericalIndeterminacyV1::IntervalOverlap(interval),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Фабрикация evidence в тестах — единственная разрешённая: конструктор
    /// запечатан приватным полем, production-производитель не существует
    /// (bound_status Unavailable). Тест закрепляет семантику границы
    /// классификатора, не производство доказательств.
    fn evidence(site: NumericalSiteIdV1, lower: f64, upper: f64) -> SoundIntervalEvidenceV1 {
        SoundIntervalEvidenceV1::fabricated_for_boundary_tests(
            site,
            OutwardIntervalV1::try_new(lower, upper).unwrap(),
        )
    }

    #[test]
    fn interval_overlap_never_becomes_a_tie_break() {
        let site = NumericalSiteIdV1::GlowTargetOrMaximumV1;
        assert!(matches!(
            classify_at_least_v1(evidence(site, 0.9, 1.1), 1.0).unwrap(),
            NumericalDecisionV1::Indeterminate {
                evidence: NumericalIndeterminacyV1::IntervalOverlap(_),
                ..
            }
        ));

        assert!(matches!(
            classify_at_least_v1(evidence(site, 1.0, 1.0), 1.0).unwrap(),
            NumericalDecisionV1::Determinate {
                value: AtLeastDecisionV1::Meets,
                ..
            }
        ));
        let below = OutwardIntervalV1::try_new(0.0, f64::from_bits(1.0_f64.to_bits() - 1)).unwrap();
        let below_decision = classify_at_least_v1(
            SoundIntervalEvidenceV1::fabricated_for_boundary_tests(site, below),
            1.0,
        )
        .unwrap();
        assert!(matches!(
            below_decision,
            NumericalDecisionV1::Determinate {
                value: AtLeastDecisionV1::Below,
                guarantee: DecisionGuaranteeV1::OutwardIntervalV1(certificate),
            } if certificate == below
        ));

        let touches_from_below = OutwardIntervalV1::try_new(0.9, 1.0).unwrap();
        assert!(matches!(
            classify_at_least_v1(
                SoundIntervalEvidenceV1::fabricated_for_boundary_tests(site, touches_from_below),
                1.0
            )
            .unwrap(),
            NumericalDecisionV1::Indeterminate {
                evidence: NumericalIndeterminacyV1::IntervalOverlap(certificate),
                ..
            } if certificate == touches_from_below
        ));
    }

    #[test]
    fn invalid_intervals_and_targets_are_rejected_without_normalisation() {
        assert!(OutwardIntervalV1::try_new(2.0, 1.0).is_err());
        assert!(OutwardIntervalV1::try_new(f64::NAN, 1.0).is_err());
        assert!(
            classify_at_least_v1(
                evidence(NumericalSiteIdV1::GlowTargetOrMaximumV1, 0.0, 1.0),
                f64::NAN,
            )
            .is_err()
        );
    }

    // ── #292: package capability ≠ compiled invocation plan ≠ result evidence ──

    /// Stable-план компилируется ИЗ machine-readable registry-строки: для
    /// Glow-сайта (bound_status Unavailable) законны только точный
    /// конечно-состоянийный метод и честный отказ — интервальный метод не
    /// планируем, потому что не допущен ни один sound-bound backend.
    #[test]
    fn stable_plan_for_glow_site_admits_only_exact_check_and_refusal() {
        let plan = CompiledNumericalPlanV1::compile(
            NumericalSiteIdV1::GlowTargetOrMaximumV1,
            NumericalProfileRequestV1::StableExactV1,
        )
        .unwrap();
        assert_eq!(plan.site_id(), NumericalSiteIdV1::GlowTargetOrMaximumV1);
        assert_eq!(plan.request(), NumericalProfileRequestV1::StableExactV1);
        assert_eq!(
            plan.methods().collect::<Vec<_>>(),
            [
                PlannedDecisionMethodV1::ExactFiniteStateV1,
                PlannedDecisionMethodV1::RefuseIndeterminateV1,
            ]
        );
    }

    /// Legacy-план существует только у сайта с ОБЪЯВЛЕННЫМ compatibility
    /// profile; синтетическая строка без него отклоняется типизированно.
    #[test]
    fn legacy_plan_requires_a_declared_compatibility_profile() {
        let plan = CompiledNumericalPlanV1::compile(
            NumericalSiteIdV1::GlowTargetOrMaximumV1,
            NumericalProfileRequestV1::LegacyPlatformDependentV1,
        )
        .unwrap();
        assert_eq!(
            plan.methods().collect::<Vec<_>>(),
            [PlannedDecisionMethodV1::LegacyPlatformDependentV1]
        );

        let mut orphan = *numerical_registry_v1()
            .iter()
            .find(|row| row.site_id == NumericalSiteIdV1::GlowTargetOrMaximumV1)
            .unwrap();
        orphan.legacy_profile = None;
        let refused = CompiledNumericalPlanV1::compile_from_row(
            &orphan,
            NumericalProfileRequestV1::LegacyPlatformDependentV1,
        );
        assert!(refused.is_err(), "legacy без профиля обязан отклоняться");
    }

    /// Sound-интервальное свидетельство несёт свой site и interval в
    /// выданный сертификат без подмены.
    #[test]
    fn interval_evidence_carries_its_provenance_into_the_certificate() {
        let site = NumericalSiteIdV1::GlowTargetOrMaximumV1;
        let interval = OutwardIntervalV1::try_new(2.0, 3.0).unwrap();
        let proof = SoundIntervalEvidenceV1::fabricated_for_boundary_tests(site, interval);
        assert_eq!(proof.site_id(), site);
        assert_eq!(proof.interval(), interval);
        let decision = classify_at_least_v1(proof, 1.0).unwrap();
        assert!(matches!(
            decision,
            NumericalDecisionV1::Determinate {
                value: AtLeastDecisionV1::Meets,
                guarantee: DecisionGuaranteeV1::OutwardIntervalV1(certificate),
            } if certificate == interval
        ));
    }

    #[test]
    fn migrated_registry_is_non_vacuous_unique_and_covers_glow_site() {
        let rows = numerical_registry_v1();
        assert!(!rows.is_empty());
        assert!(rows.iter().any(|row| {
            row.site_id == NumericalSiteIdV1::GlowTargetOrMaximumV1
                && row.stable_outcomes
                    == [
                        StableNumericalOutcomeV1::BitExact,
                        StableNumericalOutcomeV1::Indeterminate,
                    ]
                && row.bound_status == NumericalBoundStatusV1::Unavailable
                && row.fallback_status == NumericalFallbackStatusV1::None
                && row.legacy_profile
                    == Some(crate::glow::GlowDecisionProfileV1::LegacyPlatformDependentV1.key())
        }));
        for (index, row) in rows.iter().enumerate() {
            assert!(!row.operations.is_empty());
            assert!(!row.domain.is_empty());
            assert!(!row.branch_effect.is_empty());
            assert!(!row.stable_outcomes.is_empty());
            assert!(!row.boundary_corpus.is_empty());
            assert!(!row.runtime_matrix.is_empty());
            assert!(
                rows[..index]
                    .iter()
                    .all(|previous| previous.site_id != row.site_id),
                "duplicate numerical site: {}",
                row.site_id.key()
            );
        }
    }
}
