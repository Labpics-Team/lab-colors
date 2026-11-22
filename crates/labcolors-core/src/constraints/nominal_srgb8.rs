//! Объявленная nominal sRGB8-конвенция над видимым point occurrence.
//!
//! Принадлежность цвета множеству не требует отличия от фона или causal owned
//! contribution. Source-over, alpha и конечное кодирование принадлежат одному
//! физическому движку Program; здесь повторного compositor-а нет.

use core::convert::Infallible;

use crate::Srgb8;
use crate::clean_set::{
    ClosedRejectedBlueIntervalV1, EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2,
    ExactNominalSrgb8CleanSetDecisionV1, ExactNominalSrgb8CleanSetV2,
};
use crate::constraints::{
    Evaluator, HardClassifier, HardDecision, ProgramConstraintContentV1,
    ProgramPointEvaluatorContentV1, ProgramPointTargetV1, private,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NominalSrgb8VisibleV2;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NominalSrgb8VisibleIdentityV1;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NominalSrgb8VisibleCapabilityV1;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NominalSrgb8VisiblePassV1;

impl private::EvaluatorSealed for NominalSrgb8VisibleV2 {}
impl private::HardClassifierSealed for NominalSrgb8VisibleV2 {}

impl Evaluator<ProgramPointTargetV1> for NominalSrgb8VisibleV2 {
    type Invocation = ();
    type Identity = NominalSrgb8VisibleIdentityV1;
    type Release = [u8; 32];
    type Capability = NominalSrgb8VisibleCapabilityV1;
    type Measurement = Srgb8;
    type Error = Infallible;

    fn identity(&self) -> Self::Identity {
        NominalSrgb8VisibleIdentityV1
    }

    fn release(&self) -> Self::Release {
        EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2
    }

    fn capability(&self) -> Self::Capability {
        NominalSrgb8VisibleCapabilityV1
    }

    fn evaluate(
        &self,
        target: &ProgramPointTargetV1,
        _invocation: &Self::Invocation,
    ) -> Result<Self::Measurement, Self::Error> {
        Ok(Srgb8::new(target.encoded().visible()))
    }
}

impl ProgramPointEvaluatorContentV1 for NominalSrgb8VisibleV2 {
    fn program_constraint_content_v1(&self, (): ()) -> ProgramConstraintContentV1 {
        ProgramConstraintContentV1::NominalSrgb8Visible {
            release: EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2,
        }
    }
}

impl HardClassifier<(), Srgb8> for NominalSrgb8VisibleV2 {
    type Pass = NominalSrgb8VisiblePassV1;
    type Violation = ClosedRejectedBlueIntervalV1;

    fn classify(
        &self,
        _invocation: &(),
        visible: &Srgb8,
    ) -> HardDecision<Self::Pass, Self::Violation> {
        match ExactNominalSrgb8CleanSetV2.classify(*visible) {
            ExactNominalSrgb8CleanSetDecisionV1::Accepted => {
                HardDecision::Pass(NominalSrgb8VisiblePassV1)
            }
            ExactNominalSrgb8CleanSetDecisionV1::Rejected(interval) => {
                HardDecision::Violation(interval)
            }
        }
    }
}
