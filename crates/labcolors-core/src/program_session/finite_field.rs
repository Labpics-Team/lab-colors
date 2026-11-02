//! Locked finite field исполняется тем же Program generation и Session.
//! Допустимое множество — одна authored tuple; отдельного поиска цвета нет.
use super::*;
use crate::authority::{AuthorityExpectedCurrentV1, AuthorityStateV1};
use crate::field_effect::final_raster::*;
use crate::field_effect::*;
use crate::observation::ObservationHeadViewV1;
use crate::session::SessionState;
use crate::sha256::Hasher;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FinalRasterRefusalV1 {
    Unsupported,
    Transparent {
        pixel_index: usize,
    },
    Convention {
        pixel_index: usize,
        composite: Srgb8,
        interval: [u8; 2],
    },
    TechnicalQuality,
    CleanConvention,
    Stale,
    ProgramBinding,
    OutputBinding,
    ContextBinding,
    ResourceExhausted,
}

/// Успешный report владеет ровно одной материализацией. Закрытый Vec
/// резервируется до commit; point и refusal не платят за raster storage.
pub(super) struct FinalRasterMaterializationOwnerV1 {
    value: Vec<FinalRasterMaterializationV1>,
}
impl FinalRasterMaterializationOwnerV1 {
    fn try_new(materialization: FinalRasterMaterializationV1) -> Result<Self, ()> {
        let mut value = Vec::new();
        try_reserve_program_evaluation_buffer(&mut value, 1)?;
        value.push(materialization);
        Ok(Self { value })
    }
    fn materialization(&self) -> Option<&FinalRasterMaterializationV1> {
        let [materialization] = self.value.as_slice() else {
            return None;
        };
        Some(materialization)
    }
}

/// Owner удерживает certificate, точные входы и все final pixels.
/// Bytes доступны только после current evaluation.
pub(crate) struct FinalRasterMaterializationV1 {
    observation: RevisionBoundObservationV1,
    payload: Shared<FieldObservedPayloadV1>,
    cleanliness_assessment: FinalRasterCleanlinessAssessmentV1,
    declaration: FieldOperatorDeclarationV1,
    scene: FieldSceneRevisionV1,
    output: ProgramFieldOutputBindingV1,
    content: ProgramContentIdentityV9,
    certificate: FieldWholeRasterCertificateV1,
    pixels: Box<[PremultipliedRgba8V1]>,
    subject: [u8; 32],
    authority: AuthorityStateV1,
}
impl FinalRasterMaterializationV1 {
    pub(crate) fn subject(&self) -> [u8; 32] {
        self.subject
    }
    pub(crate) fn certificate(&self) -> &FieldWholeRasterCertificateV1 {
        &self.certificate
    }
    pub(crate) fn observation(&self) -> &RevisionBoundObservationV1 {
        &self.observation
    }
    pub(crate) fn pixels(&self) -> &[PremultipliedRgba8V1] {
        &self.pixels
    }
    pub(crate) fn payload(&self) -> &FieldObservedPayloadV1 {
        self.payload.as_ref()
    }
    pub(crate) fn cleanliness_requirement(
        &self,
    ) -> Result<FinalRasterCleanlinessRequirementV1, FinalRasterRefusalV1> {
        self.output
            .final_raster_cleanliness()
            .ok_or(FinalRasterRefusalV1::Unsupported)
    }
    pub(crate) fn cleanliness_assessment(&self) -> FinalRasterCleanlinessAssessmentV1 {
        self.cleanliness_assessment
    }
    pub(crate) fn request(&self) -> Result<FieldEvaluationRequestV1<'_>, FieldEvaluationErrorV1> {
        request_for(self.payload(), self.declaration, self.scene)
    }
    pub(crate) fn content(&self) -> [u8; 32] {
        *self.content.as_bytes()
    }
    pub(crate) fn output(&self) -> u32 {
        self.output.output().value()
    }
    pub(crate) fn revision(&self) -> u64 {
        self.scene.revision().value()
    }
    pub(crate) fn check_current(
        &self,
        current: &RevisionBoundObservationV1,
        content: [u8; 32],
        output: u32,
        revision: u64,
        context: [u8; 32],
    ) -> Result<(), FinalRasterRefusalV1> {
        if content != self.content() {
            return Err(FinalRasterRefusalV1::ProgramBinding);
        }
        if output != self.output() {
            return Err(FinalRasterRefusalV1::OutputBinding);
        }
        if context != self.payload().context {
            return Err(FinalRasterRefusalV1::ContextBinding);
        }
        if revision != self.revision() || !self.observation.is_same_binding_as(current) {
            return Err(FinalRasterRefusalV1::Stale);
        }
        self.authority
            .evaluate_program_final_raster(self, current)?;
        Ok(())
    }
}
fn request_for(
    payload: &FieldObservedPayloadV1,
    declaration: FieldOperatorDeclarationV1,
    scene: FieldSceneRevisionV1,
) -> Result<FieldEvaluationRequestV1<'_>, FieldEvaluationErrorV1> {
    let FieldExecutionProfileV1::PremultipliedSourceOver {
        source: FieldValueRefV1::Input(source),
        destination: FieldValueRefV1::Input(destination),
    } = declaration.execution()
    else {
        return Err(FieldEvaluationErrorV1::UnsupportedWorkingSpace);
    };
    FieldEvaluationRequestV1::try_new(
        FieldRequestIdV1::new(scene.revision().value()),
        declaration.id(),
        FieldGeometryV1::new(payload.extent),
        payload.dpr,
        declaration.working_space(),
        declaration.precision(),
        declaration.quantization(),
        FieldRenderCapabilityV1::new(
            FieldRendererCapabilityV1::exact_reference(FieldRendererIdV1::new(1)),
            FieldOutputCapabilityV1::PremultipliedRgba8V1,
        ),
        scene,
        declaration.carrier_intent(),
        FieldOperationV1::PremultipliedSourceOver {
            source: payload.raster(source)?,
            destination: payload.raster(destination)?,
        },
    )
}

impl<Evaluation> CompiledProgram<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    pub(crate) fn validate_final_field_payload(
        &self,
        payload: &FieldObservedPayloadV1,
    ) -> Result<(), FinalRasterRefusalV1> {
        validate(&self.owner_generation, payload)
    }
}
fn validate<Evaluation>(
    epoch: &ProgramEpochV1<Evaluation>,
    payload: &FieldObservedPayloadV1,
) -> Result<(), FinalRasterRefusalV1>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    if !epoch.outputs.is_empty()
        || epoch.field_outputs.len() != 1
        || !epoch.field_outputs[0].is_final_opaque()
        || epoch.field.inputs().len() != 2
        || epoch.field.operators().len() != 1
    {
        return Err(FinalRasterRefusalV1::Unsupported);
    }
    if !epoch
        .field
        .inputs()
        .iter()
        .zip(&payload.inputs)
        .all(|(declaration, input)| {
            declaration.id() == input.port
                && declaration.kind() == FieldInputKindV1::PremultipliedRgba8V1
        })
    {
        return Err(FinalRasterRefusalV1::ProgramBinding);
    }
    if !matches!(
        epoch.field.operators()[0].execution(),
        FieldExecutionProfileV1::PremultipliedSourceOver {
            source: FieldValueRefV1::Input(_),
            destination: FieldValueRefV1::Input(_)
        }
    ) {
        return Err(FinalRasterRefusalV1::Unsupported);
    }
    Ok(())
}

pub(super) fn evaluate_final_field_session<Evaluation>(
    plan: &mut ProgramSessionPlan<Evaluation>,
    epoch: &ProgramEpochV1<Evaluation>,
    observation: RevisionBoundObservationV1,
    permit: SessionObservationBindingPermitV1,
) -> ProgramSessionEvaluationResult<Evaluation>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let payload_owner = Shared::clone(
        observation
            .field_payload_owner()
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?,
    );
    let payload = payload_owner.as_ref();
    validate(epoch, payload).map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
    let scene = FieldSceneRevisionV1::from_admitted_observation(&observation, &permit)
        .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
    let declaration = epoch.field.operators()[0];
    let request = request_for(payload, declaration, scene)
        .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
    let mut scratch = FieldEvaluationScratchV1::new();
    let result = match evaluate_final_opaque_reference(
        &request,
        FieldEvidenceIdentityV1::new(scene.revision().value()),
        &mut scratch,
    ) {
        Err(FieldEvaluationErrorV1::ResourceExhausted) => {
            return Err(ProgramSessionEvaluationError::ResourceExhausted);
        }
        Err(FieldEvaluationErrorV1::ObservedRasterMismatch { pixel_index }) => {
            Err(FinalRasterRefusalV1::Transparent { pixel_index })
        }
        Err(_) => Err(FinalRasterRefusalV1::Unsupported),
        Ok(certificate) => {
            let mut pixels = Vec::new();
            pixels
                .try_reserve_exact(scratch.output().len())
                .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
            pixels.extend_from_slice(scratch.output());
            let mut hash = Hasher::new();
            hash.update(b"labcolors.program.modeled-final-raster.v1\0");
            hash.update(epoch.content_identity.as_bytes());
            hash.update(&epoch.field_outputs[0].output().value().to_le_bytes());
            hash.update(&payload.context);
            hash.update(&payload.extent.width().to_le_bytes());
            hash.update(&payload.extent.height().to_le_bytes());
            hash.update(&[payload.dpr.value()]);
            hash.update(&certificate.digest().as_bytes());
            let subject = *hash.finalize().as_bytes();
            let mut materialization = FinalRasterMaterializationV1 {
                observation: observation.clone(),
                payload: payload_owner,
                cleanliness_assessment: FinalRasterCleanlinessAssessmentV1::off(),
                declaration,
                scene,
                output: epoch.field_outputs[0],
                content: epoch
                    .content_identity
                    .legacy()
                    .ok_or(ProgramSessionEvaluationError::InternalInvariant)?,
                certificate,
                pixels: pixels.into_boxed_slice(),
                subject,
                authority: AuthorityStateV1::new(),
            };
            let mut authority = AuthorityStateV1::new();
            if authority
                .admit_program_final_raster_technical_quality(
                    &materialization,
                    &observation,
                    AuthorityExpectedCurrentV1::Vacant,
                )
                .is_err()
            {
                Err(FinalRasterRefusalV1::TechnicalQuality)
            } else {
                let policy = materialization
                    .cleanliness_requirement()
                    .map_err(|_| ProgramSessionEvaluationError::InternalInvariant)?;
                let assessment = match policy {
                    FinalRasterCleanlinessRequirementV1::Off => {
                        Ok(FinalRasterCleanlinessAssessmentV1::off())
                    }
                    FinalRasterCleanlinessRequirementV1::ReportOnlyExactNominalSrgb8V2 => {
                        AuthorityStateV1::assess_program_final_raster_clean_convention(
                            &materialization,
                            &observation,
                        )
                    }
                    FinalRasterCleanlinessRequirementV1::MandatoryExactNominalSrgb8V2 => authority
                        .admit_program_final_raster_clean_convention(
                            &materialization,
                            &observation,
                            AuthorityExpectedCurrentV1::Vacant,
                        )
                        .and_then(|_| {
                            FinalRasterCleanlinessAssessmentV1::assessed(
                                policy,
                                materialization.pixels().len(),
                                0,
                                None,
                            )
                            .map_err(|_| FinalRasterRefusalV1::CleanConvention)
                        }),
                };
                assessment.map(|assessment| {
                    materialization.cleanliness_assessment = assessment;
                    materialization.authority = authority;
                    materialization
                })
            }
        }
    };
    let result = match result {
        Ok(materialization) => Ok(FinalRasterMaterializationOwnerV1::try_new(materialization)
            .map_err(|()| ProgramSessionEvaluationError::ResourceExhausted)?),
        Err(refusal) => Err(refusal),
    };
    let slot = observation.arena_slot();
    let mut arena = plan
        .evaluation_arenas
        .guard(slot)
        .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
    let storage = arena.storage_mut();
    storage.outputs.clear();
    storage.cells.clear();
    storage.point_causal_records.clear();
    storage.point_causal_steps.clear();
    let passed = result.is_ok();
    let report = ProgramReportV1 {
        content_identity: epoch
            .content_identity
            .legacy()
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?,
        selection_release_identity: None,
        observation,
        arena: arena.into_lease(),
        field: Some(result),
    };
    Ok(if passed {
        SessionDecision::Verified(ProgramVerifiedOutcomeV1::Legacy(ProgramVerifiedV1 {
            report,
            origin: CandidateOriginV1::Fixed,
        }))
    } else {
        SessionDecision::Violation(ProgramViolationOutcomeV1::Legacy(ProgramConflictV1 {
            report,
            considered_state_count: 1,
        }))
    })
}

/// Historical evidence не входит: только Ready и связанная raw head.
pub(crate) fn current_final_raster<Evaluation>(
    session: &Session<ProgramSessionPlan<Evaluation>>,
) -> Result<(&FinalRasterMaterializationV1, &RevisionBoundObservationV1), FinalRasterRefusalV1>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    let SessionState::Ready { current } = session.state() else {
        return Err(FinalRasterRefusalV1::Stale);
    };
    let Some(Ok(materialization)) = current
        .legacy()
        .and_then(|value| value.report.field.as_ref())
    else {
        return Err(FinalRasterRefusalV1::Unsupported);
    };
    let ObservationHeadViewV1::Observed(observation) = session.raw_head() else {
        return Err(FinalRasterRefusalV1::Stale);
    };
    let materialization = materialization
        .materialization()
        .ok_or(FinalRasterRefusalV1::Unsupported)?;
    Ok((materialization, observation))
}
pub(crate) fn current_refusal<Evaluation>(
    session: &Session<ProgramSessionPlan<Evaluation>>,
) -> Option<FinalRasterRefusalV1>
where
    Evaluation: ProgramConstraintEvaluatorSetV1,
    ProgramConstraintInvocationOf<Evaluation>: Copy,
{
    match session.state() {
        SessionState::Failed {
            cause: ProgramViolationOutcomeV1::Legacy(cause),
            ..
        } => cause
            .report
            .field
            .as_ref()
            .and_then(|result| result.as_ref().err().copied()),
        _ => None,
    }
}
