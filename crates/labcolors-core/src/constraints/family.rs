//! Метаданные code-owned evaluator-а принадлежности точному образу family.

use crate::family::{
    AdmittedFamilySetV1, FamilyMembershipMeasurementV1, FamilyMembershipPassV1,
    FamilyMembershipViolationV1,
};
use crate::lcs_occurrence::ColorSignal;

use super::HardDecision;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FamilyMembershipIdentityV1 {
    ExactImageMembershipV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FamilyMembershipReleaseV1 {
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FamilyMembershipCapabilityV1 {
    Iec61966Srgb8D65V1,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FamilyMembershipV1;

impl FamilyMembershipV1 {
    pub(crate) const fn identity(self) -> FamilyMembershipIdentityV1 {
        FamilyMembershipIdentityV1::ExactImageMembershipV1
    }

    pub(crate) const fn release(self) -> FamilyMembershipReleaseV1 {
        FamilyMembershipReleaseV1::V1
    }

    pub(crate) const fn capability(self) -> FamilyMembershipCapabilityV1 {
        FamilyMembershipCapabilityV1::Iec61966Srgb8D65V1
    }

    pub(crate) fn assess(
        self,
        family: &AdmittedFamilySetV1,
        signal: ColorSignal,
    ) -> (
        FamilyMembershipMeasurementV1,
        HardDecision<FamilyMembershipPassV1, FamilyMembershipViolationV1>,
    ) {
        family.assess(signal)
    }
}
