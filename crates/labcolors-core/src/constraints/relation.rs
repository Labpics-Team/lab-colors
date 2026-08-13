use crate::Srgb8;
use crate::constraints::{FamilyMembershipV2, HardDecision, ProgramConstraintContentV1};
use crate::family::{
    FamilyId, FamilyMembershipMeasurementV2, FamilyMembershipPassV1, FamilyMembershipViolationV1,
};
use crate::family_artifact::{AdmittedFamilyArtifactV2, BoundFamilyArtifactBundleV2};
use crate::lcs_occurrence::ColorSignal;

fn exact_srgb8_equal(left: Srgb8, right: Srgb8) -> bool {
    left.bytes() == right.bytes()
}

/// Принадлежащая Core identity точного байтового равенства однородных endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExactSrgb8RelationIdentityV1 {
    HomogeneousEndpointEqualityV1,
}

/// Версия точного закона для пары endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExactSrgb8RelationReleaseV1 {
    V1,
}

/// Capability намеренно уже перцептивной либо renderer identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExactSrgb8RelationCapabilityV1 {
    HomogeneousEncodedSrgb8PairV1,
}

/// Сырая пара хранится независимо от жёсткой классификации.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactSrgb8RelationMeasurementV1 {
    reference: Srgb8,
    candidate: Srgb8,
}

impl ExactSrgb8RelationMeasurementV1 {
    pub(crate) const fn reference(self) -> Srgb8 {
        self.reference
    }

    pub(crate) const fn candidate(self) -> Srgb8 {
        self.candidate
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactSrgb8RelationPassV1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactSrgb8RelationViolationV1;

/// Профиль точного технического отношения без перцептивных утверждений.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ExactSrgb8RelationV1;

impl ExactSrgb8RelationV1 {
    pub(crate) const fn identity(self) -> ExactSrgb8RelationIdentityV1 {
        ExactSrgb8RelationIdentityV1::HomogeneousEndpointEqualityV1
    }

    pub(crate) const fn release(self) -> ExactSrgb8RelationReleaseV1 {
        ExactSrgb8RelationReleaseV1::V1
    }

    pub(crate) const fn capability(self) -> ExactSrgb8RelationCapabilityV1 {
        ExactSrgb8RelationCapabilityV1::HomogeneousEncodedSrgb8PairV1
    }

    pub(crate) fn assess(
        self,
        reference: Srgb8,
        candidate: Srgb8,
    ) -> (
        ExactSrgb8RelationMeasurementV1,
        HardDecision<ExactSrgb8RelationPassV1, ExactSrgb8RelationViolationV1>,
    ) {
        let measurement = ExactSrgb8RelationMeasurementV1 {
            reference,
            candidate,
        };
        let decision = if exact_srgb8_equal(reference, candidate) {
            HardDecision::Pass(ExactSrgb8RelationPassV1)
        } else {
            HardDecision::Violation(ExactSrgb8RelationViolationV1)
        };
        (measurement, decision)
    }
}

/// Принадлежащая Core identity точного байтового различия однородных endpoints.
///
/// Различие — технический факт о encoded байтах, не перцептивная
/// distinguishability и не human-law утверждение.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExactSrgb8DistinctionIdentityV1 {
    HomogeneousEndpointInequalityV1,
}

/// Версия точного distinction-закона для пары endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExactSrgb8DistinctionReleaseV1 {
    V1,
}

/// Applicability намеренно ограничена парой encoded sRGB8 без модели
/// наблюдателя: закон не заявляет видимой различимости.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExactSrgb8DistinctionCapabilityV1 {
    HomogeneousEncodedSrgb8PairV1,
}

/// Сырая пара сохраняется независимо от вердикта различия.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactSrgb8DistinctionMeasurementV1 {
    reference: Srgb8,
    candidate: Srgb8,
}

impl ExactSrgb8DistinctionMeasurementV1 {
    pub(crate) const fn reference(self) -> Srgb8 {
        self.reference
    }

    pub(crate) const fn candidate(self) -> Srgb8 {
        self.candidate
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactSrgb8DistinctionPassV1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactSrgb8DistinctionViolationV1;

/// Профиль точного технического различия без перцептивных утверждений.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ExactSrgb8DistinctionV1;

impl ExactSrgb8DistinctionV1 {
    pub(crate) const fn identity(self) -> ExactSrgb8DistinctionIdentityV1 {
        ExactSrgb8DistinctionIdentityV1::HomogeneousEndpointInequalityV1
    }

    pub(crate) const fn release(self) -> ExactSrgb8DistinctionReleaseV1 {
        ExactSrgb8DistinctionReleaseV1::V1
    }

    pub(crate) const fn capability(self) -> ExactSrgb8DistinctionCapabilityV1 {
        ExactSrgb8DistinctionCapabilityV1::HomogeneousEncodedSrgb8PairV1
    }

    pub(crate) fn assess(
        self,
        reference: Srgb8,
        candidate: Srgb8,
    ) -> (
        ExactSrgb8DistinctionMeasurementV1,
        HardDecision<ExactSrgb8DistinctionPassV1, ExactSrgb8DistinctionViolationV1>,
    ) {
        let measurement = ExactSrgb8DistinctionMeasurementV1 {
            reference,
            candidate,
        };
        let decision = if exact_srgb8_equal(reference, candidate) {
            HardDecision::Violation(ExactSrgb8DistinctionViolationV1)
        } else {
            HardDecision::Pass(ExactSrgb8DistinctionPassV1)
        };
        (measurement, decision)
    }
}

/// Принадлежащая Core identity положительной категориальной принадлежности
/// обоих однородных endpoints одному объявленному точному family-образу.
///
/// Категория для Core — объявленный exact family без клиентской семантики.
/// Закон требует положительного свидетельства включения каждого endpoint;
/// отрицательное дополнение любой другой проверки не образует категорию.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FamilyCategoryRelationIdentityV1 {
    HomogeneousEndpointCategoryMembershipV1,
}

/// Версия категориального закона для пары endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FamilyCategoryRelationReleaseV1 {
    V1,
}

/// Applicability: пара encoded sRGB8, судимая допущенным IEC 61966 sRGB8 D65
/// family-образом. Неопределённости нет — включение в точное множество.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FamilyCategoryRelationCapabilityV1 {
    HomogeneousIec61966Srgb8D65PairV1,
}

/// Положительные membership-измерения обоих endpoints против одного semantic
/// release. Измерения сохраняются независимо от жёсткой классификации.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FamilyCategoryRelationMeasurementV1 {
    reference: FamilyMembershipMeasurementV2,
    candidate: FamilyMembershipMeasurementV2,
}

impl FamilyCategoryRelationMeasurementV1 {
    pub(crate) const fn reference(self) -> FamilyMembershipMeasurementV2 {
        self.reference
    }

    pub(crate) const fn candidate(self) -> FamilyMembershipMeasurementV2 {
        self.candidate
    }
}

/// Положительное категориальное свидетельство: оба endpoint несут собственный
/// inclusion witness. Semantic release и сигналы живут в измерении.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FamilyCategoryRelationPassV1 {
    reference: FamilyMembershipPassV1,
    candidate: FamilyMembershipPassV1,
}

/// Нарушение называет endpoint, оставшийся без положительного свидетельства.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FamilyCategoryRelationViolationV1 {
    ReferenceEndpoint,
    CandidateEndpoint,
    BothEndpoints,
}

/// Категориальный профиль над однородной парой. Переиспользует единственный
/// membership-закон family, не изобретая второй источник истины включения.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FamilyCategoryRelationV1;

impl FamilyCategoryRelationV1 {
    pub(crate) const fn identity(self) -> FamilyCategoryRelationIdentityV1 {
        FamilyCategoryRelationIdentityV1::HomogeneousEndpointCategoryMembershipV1
    }

    pub(crate) const fn release(self) -> FamilyCategoryRelationReleaseV1 {
        FamilyCategoryRelationReleaseV1::V1
    }

    pub(crate) const fn capability(self) -> FamilyCategoryRelationCapabilityV1 {
        FamilyCategoryRelationCapabilityV1::HomogeneousIec61966Srgb8D65PairV1
    }

    pub(crate) fn assess(
        self,
        family: &AdmittedFamilyArtifactV2,
        reference: Srgb8,
        candidate: Srgb8,
    ) -> (
        FamilyCategoryRelationMeasurementV1,
        HardDecision<FamilyCategoryRelationPassV1, FamilyCategoryRelationViolationV1>,
    ) {
        let (reference_measurement, reference_decision) =
            FamilyMembershipV2.assess(family, ColorSignal::from_srgb8(reference));
        let (candidate_measurement, candidate_decision) =
            FamilyMembershipV2.assess(family, ColorSignal::from_srgb8(candidate));
        let measurement = FamilyCategoryRelationMeasurementV1 {
            reference: reference_measurement,
            candidate: candidate_measurement,
        };
        let decision = match (reference_decision, candidate_decision) {
            (HardDecision::Pass(reference), HardDecision::Pass(candidate)) => {
                HardDecision::Pass(FamilyCategoryRelationPassV1 {
                    reference,
                    candidate,
                })
            }
            (HardDecision::Pass(_), HardDecision::Violation(_)) => {
                HardDecision::Violation(FamilyCategoryRelationViolationV1::CandidateEndpoint)
            }
            (HardDecision::Violation(_), HardDecision::Pass(_)) => {
                HardDecision::Violation(FamilyCategoryRelationViolationV1::ReferenceEndpoint)
            }
            (HardDecision::Violation(_), HardDecision::Violation(_)) => {
                HardDecision::Violation(FamilyCategoryRelationViolationV1::BothEndpoints)
            }
        };
        (measurement, decision)
    }
}

/// Принадлежащая Core identity точного равенства source одного intrinsic Paint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExactSrgb8IntrinsicUnaryIdentityV1 {
    SourceEqualityV1,
}

/// Версия точного unary-закона над source Paint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExactSrgb8IntrinsicUnaryReleaseV1 {
    V1,
}

/// Capability не обещает равенство alpha или результата композитинга.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExactSrgb8IntrinsicUnaryCapabilityV1 {
    EncodedSrgb8PaintSourceV1,
}

/// Сырые expected и actual сохраняются независимо от вердикта.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactSrgb8IntrinsicUnaryMeasurementV1 {
    expected: Srgb8,
    actual: Srgb8,
}

impl ExactSrgb8IntrinsicUnaryMeasurementV1 {
    pub(crate) const fn expected(self) -> Srgb8 {
        self.expected
    }

    pub(crate) const fn actual(self) -> Srgb8 {
        self.actual
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactSrgb8IntrinsicUnaryPassV1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactSrgb8IntrinsicUnaryViolationV1;

/// Точный unary-профиль проверяет только source и не теряет полный Paint binding.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ExactSrgb8IntrinsicUnaryV1;

impl ExactSrgb8IntrinsicUnaryV1 {
    pub(crate) const fn identity(self) -> ExactSrgb8IntrinsicUnaryIdentityV1 {
        ExactSrgb8IntrinsicUnaryIdentityV1::SourceEqualityV1
    }

    pub(crate) const fn release(self) -> ExactSrgb8IntrinsicUnaryReleaseV1 {
        ExactSrgb8IntrinsicUnaryReleaseV1::V1
    }

    pub(crate) const fn capability(self) -> ExactSrgb8IntrinsicUnaryCapabilityV1 {
        ExactSrgb8IntrinsicUnaryCapabilityV1::EncodedSrgb8PaintSourceV1
    }

    pub(crate) fn assess(
        self,
        expected: Srgb8,
        actual: Srgb8,
    ) -> (
        ExactSrgb8IntrinsicUnaryMeasurementV1,
        HardDecision<ExactSrgb8IntrinsicUnaryPassV1, ExactSrgb8IntrinsicUnaryViolationV1>,
    ) {
        let measurement = ExactSrgb8IntrinsicUnaryMeasurementV1 { expected, actual };
        let decision = if exact_srgb8_equal(expected, actual) {
            HardDecision::Pass(ExactSrgb8IntrinsicUnaryPassV1)
        } else {
            HardDecision::Violation(ExactSrgb8IntrinsicUnaryViolationV1)
        };
        (measurement, decision)
    }
}

/// Закрытый registry intrinsic-unary профилей Core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoreIntrinsicUnaryInvocationV1 {
    ExactSrgb8 { expected: Srgb8 },
    FamilyMembership { family: FamilyId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompiledCoreIntrinsicUnaryInvocationV1 {
    ExactSrgb8 {
        expected: Srgb8,
    },
    FamilyMembership {
        family: FamilyId,
        family_index: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoreIntrinsicUnaryMeasurementV1 {
    ExactSrgb8(ExactSrgb8IntrinsicUnaryMeasurementV1),
    FamilyMembership {
        family: FamilyId,
        measurement: FamilyMembershipMeasurementV2,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoreIntrinsicUnaryPassV1 {
    ExactSrgb8(ExactSrgb8IntrinsicUnaryPassV1),
    FamilyMembership(FamilyMembershipPassV1),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoreIntrinsicUnaryViolationV1 {
    ExactSrgb8(ExactSrgb8IntrinsicUnaryViolationV1),
    FamilyMembership(FamilyMembershipViolationV1),
}

impl CoreIntrinsicUnaryInvocationV1 {
    pub(crate) const fn exact_srgb8(expected: Srgb8) -> Self {
        Self::ExactSrgb8 { expected }
    }

    pub(crate) const fn family_membership(family: FamilyId) -> Self {
        Self::FamilyMembership { family }
    }

    pub(crate) const fn content(self) -> ProgramConstraintContentV1 {
        match self {
            Self::ExactSrgb8 { expected } => {
                let profile = ExactSrgb8IntrinsicUnaryV1;
                ProgramConstraintContentV1::ExactSrgb8IntrinsicUnary {
                    identity: profile.identity(),
                    release: profile.release(),
                    capability: profile.capability(),
                    expected,
                }
            }
            Self::FamilyMembership { .. } => {
                let profile = FamilyMembershipV2;
                ProgramConstraintContentV1::FamilyMembership {
                    identity: profile.identity(),
                    release: profile.release(),
                    capability: profile.capability(),
                }
            }
        }
    }
}

impl CompiledCoreIntrinsicUnaryInvocationV1 {
    pub(crate) fn assess(
        self,
        actual: Srgb8,
        families: &BoundFamilyArtifactBundleV2,
    ) -> Option<(
        CoreIntrinsicUnaryMeasurementV1,
        HardDecision<CoreIntrinsicUnaryPassV1, CoreIntrinsicUnaryViolationV1>,
    )> {
        match self {
            Self::ExactSrgb8 { expected } => {
                let (measurement, decision) = ExactSrgb8IntrinsicUnaryV1.assess(expected, actual);
                let decision = match decision {
                    HardDecision::Pass(proof) => {
                        HardDecision::Pass(CoreIntrinsicUnaryPassV1::ExactSrgb8(proof))
                    }
                    HardDecision::Violation(proof) => {
                        HardDecision::Violation(CoreIntrinsicUnaryViolationV1::ExactSrgb8(proof))
                    }
                };
                Some((
                    CoreIntrinsicUnaryMeasurementV1::ExactSrgb8(measurement),
                    decision,
                ))
            }
            Self::FamilyMembership {
                family,
                family_index,
            } => {
                // Compile-time ordinal и Session projection независимо
                // связываются через semantic release; runtime lookup не читает
                // opaque FamilyId. `None` поэтому означает порчу compiled state.
                let artifact = families.artifact(family_index)?;
                let (measurement, decision) =
                    FamilyMembershipV2.assess(artifact, ColorSignal::from_srgb8(actual));
                let decision = match decision {
                    HardDecision::Pass(proof) => {
                        HardDecision::Pass(CoreIntrinsicUnaryPassV1::FamilyMembership(proof))
                    }
                    HardDecision::Violation(proof) => HardDecision::Violation(
                        CoreIntrinsicUnaryViolationV1::FamilyMembership(proof),
                    ),
                };
                Some((
                    CoreIntrinsicUnaryMeasurementV1::FamilyMembership {
                        family,
                        measurement,
                    },
                    decision,
                ))
            }
        }
    }
}

/// Закрытый registry однородных directional-профилей Core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoreRelationInvocationV1 {
    ExactSrgb8,
    ExactSrgb8Distinction,
    FamilyCategory { family: FamilyId },
}

/// Compiled directional-профиль: семантический family ordinal связывается на
/// компиляции, runtime lookup не читает opaque FamilyId.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompiledCoreRelationInvocationV1 {
    ExactSrgb8,
    ExactSrgb8Distinction,
    FamilyCategory {
        family: FamilyId,
        family_index: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoreRelationMeasurementV1 {
    ExactSrgb8(ExactSrgb8RelationMeasurementV1),
    ExactSrgb8Distinction(ExactSrgb8DistinctionMeasurementV1),
    FamilyCategory {
        family: FamilyId,
        measurement: FamilyCategoryRelationMeasurementV1,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoreRelationPassV1 {
    ExactSrgb8(ExactSrgb8RelationPassV1),
    ExactSrgb8Distinction(ExactSrgb8DistinctionPassV1),
    FamilyCategory(FamilyCategoryRelationPassV1),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoreRelationViolationV1 {
    ExactSrgb8(ExactSrgb8RelationViolationV1),
    ExactSrgb8Distinction(ExactSrgb8DistinctionViolationV1),
    FamilyCategory(FamilyCategoryRelationViolationV1),
}

impl CoreRelationInvocationV1 {
    pub(crate) const fn exact_srgb8() -> Self {
        Self::ExactSrgb8
    }

    pub(crate) const fn exact_srgb8_distinction() -> Self {
        Self::ExactSrgb8Distinction
    }

    pub(crate) const fn family_category(family: FamilyId) -> Self {
        Self::FamilyCategory { family }
    }

    pub(crate) const fn content(self) -> ProgramConstraintContentV1 {
        match self {
            Self::ExactSrgb8 => {
                let profile = ExactSrgb8RelationV1;
                ProgramConstraintContentV1::ExactSrgb8Relation {
                    identity: profile.identity(),
                    release: profile.release(),
                    capability: profile.capability(),
                }
            }
            Self::ExactSrgb8Distinction => {
                let profile = ExactSrgb8DistinctionV1;
                ProgramConstraintContentV1::ExactSrgb8DistinctionRelation {
                    identity: profile.identity(),
                    release: profile.release(),
                    capability: profile.capability(),
                }
            }
            Self::FamilyCategory { .. } => {
                let profile = FamilyCategoryRelationV1;
                ProgramConstraintContentV1::FamilyCategoryRelation {
                    identity: profile.identity(),
                    release: profile.release(),
                    capability: profile.capability(),
                }
            }
        }
    }
}

impl CompiledCoreRelationInvocationV1 {
    pub(crate) fn assess(
        self,
        reference: Srgb8,
        candidate: Srgb8,
        families: &BoundFamilyArtifactBundleV2,
    ) -> Option<(
        CoreRelationMeasurementV1,
        HardDecision<CoreRelationPassV1, CoreRelationViolationV1>,
    )> {
        match self {
            Self::ExactSrgb8 => {
                let (measurement, decision) = ExactSrgb8RelationV1.assess(reference, candidate);
                let decision = match decision {
                    HardDecision::Pass(proof) => {
                        HardDecision::Pass(CoreRelationPassV1::ExactSrgb8(proof))
                    }
                    HardDecision::Violation(proof) => {
                        HardDecision::Violation(CoreRelationViolationV1::ExactSrgb8(proof))
                    }
                };
                Some((CoreRelationMeasurementV1::ExactSrgb8(measurement), decision))
            }
            Self::ExactSrgb8Distinction => {
                let (measurement, decision) = ExactSrgb8DistinctionV1.assess(reference, candidate);
                let decision = match decision {
                    HardDecision::Pass(proof) => {
                        HardDecision::Pass(CoreRelationPassV1::ExactSrgb8Distinction(proof))
                    }
                    HardDecision::Violation(proof) => HardDecision::Violation(
                        CoreRelationViolationV1::ExactSrgb8Distinction(proof),
                    ),
                };
                Some((
                    CoreRelationMeasurementV1::ExactSrgb8Distinction(measurement),
                    decision,
                ))
            }
            Self::FamilyCategory {
                family,
                family_index,
            } => {
                // Compile-time ordinal и Session projection независимо
                // связываются через semantic release; `None` означает порчу
                // compiled state, а не отсутствие категории.
                let artifact = families.artifact(family_index)?;
                let (measurement, decision) =
                    FamilyCategoryRelationV1.assess(artifact, reference, candidate);
                let decision = match decision {
                    HardDecision::Pass(proof) => {
                        HardDecision::Pass(CoreRelationPassV1::FamilyCategory(proof))
                    }
                    HardDecision::Violation(proof) => {
                        HardDecision::Violation(CoreRelationViolationV1::FamilyCategory(proof))
                    }
                };
                Some((
                    CoreRelationMeasurementV1::FamilyCategory {
                        family,
                        measurement,
                    },
                    decision,
                ))
            }
        }
    }
}
