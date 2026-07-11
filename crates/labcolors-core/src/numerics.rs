//! Типизированные решения для branch-sensitive численных sites (#281).
//!
//! Diagnostic `f64` не становится semantic verdict сам по себе. Determinate
//! разрешён только вместе с объявленной гарантией; при отсутствии sound bound
//! или пересечении outward-интервала с границей возвращается `Indeterminate`.

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
        legacy_profile: Some("legacy-platform-dependent-v1"),
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

/// Доказанный outward-интервал, содержащий истинное значение.
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

/// Классифицирует `value >= target` только по доказанному outward-интервалу.
pub fn classify_at_least_v1(
    site_id: NumericalSiteIdV1,
    interval: OutwardIntervalV1,
    target: f64,
) -> Result<NumericalDecisionV1<AtLeastDecisionV1>, String> {
    if !target.is_finite() {
        return Err(format!("target не конечен: {target}"));
    }
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
        site_id,
        evidence: NumericalIndeterminacyV1::IntervalOverlap(interval),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_overlap_never_becomes_a_tie_break() {
        let site = NumericalSiteIdV1::GlowTargetOrMaximumV1;
        let overlap = OutwardIntervalV1::try_new(0.9, 1.1).unwrap();
        assert!(matches!(
            classify_at_least_v1(site, overlap, 1.0).unwrap(),
            NumericalDecisionV1::Indeterminate {
                evidence: NumericalIndeterminacyV1::IntervalOverlap(_),
                ..
            }
        ));

        let exact_target = OutwardIntervalV1::try_new(1.0, 1.0).unwrap();
        assert!(matches!(
            classify_at_least_v1(site, exact_target, 1.0).unwrap(),
            NumericalDecisionV1::Determinate {
                value: AtLeastDecisionV1::Meets,
                ..
            }
        ));
        let below = OutwardIntervalV1::try_new(0.0, f64::from_bits(1.0_f64.to_bits() - 1)).unwrap();
        let below_decision = classify_at_least_v1(site, below, 1.0).unwrap();
        assert!(matches!(
            below_decision,
            NumericalDecisionV1::Determinate {
                value: AtLeastDecisionV1::Below,
                guarantee: DecisionGuaranteeV1::OutwardIntervalV1(certificate),
            } if certificate == below
        ));

        let touches_from_below = OutwardIntervalV1::try_new(0.9, 1.0).unwrap();
        assert!(matches!(
            classify_at_least_v1(site, touches_from_below, 1.0).unwrap(),
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
        let valid = OutwardIntervalV1::try_new(0.0, 1.0).unwrap();
        assert!(
            classify_at_least_v1(NumericalSiteIdV1::GlowTargetOrMaximumV1, valid, f64::NAN,)
                .is_err()
        );
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
                && row.legacy_profile == Some("legacy-platform-dependent-v1")
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
