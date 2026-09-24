use super::*;
use crate::Srgb8;
use crate::field_effect::{
    CarrierIntentV1, DevicePixelRatioV1, FieldEvaluationRequestV1, FieldEvaluationScratchV1,
    FieldEvidenceIdentityV1, FieldEvidenceV1, FieldExtentV1, FieldGeometryV1, FieldOperationV1,
    FieldOperatorInstanceIdV1, FieldOutputCapabilityV1, FieldPrecisionV1, FieldQuantizationV1,
    FieldRasterIdentityV1, FieldRasterViewV1, FieldRenderCapabilityV1, FieldRendererCapabilityV1,
    FieldRendererIdV1, FieldRequestIdV1, FieldSceneRevisionV1, FieldWorkingSpaceV1,
    PremultipliedRgba8V1, evaluate_whole_field, verify_exact_reference_for_tq,
};
use crate::observation::{ObservationStreamId, Revision};
use crate::program::wire::ProgramWireBuilderV1;
use crate::program_wire::{
    ProgramMaterializationAuthorityErrorV1, ProgramPointSinkHostErrorV1, ProgramPointSinkIntentV1,
    ProgramScenarioV1, compile_program_wire_v1,
};

const OUTPUT: u32 = 17;
const ROOT: u32 = 9;
const OCCURRENCE: u32 = 8;
const SINK_OUTPUT: u32 = 91;

#[derive(Default)]
struct Host {
    stamp: Option<crate::program_wire::ProgramPointSinkStampV1>,
}

impl ProgramPointSinkHostV1 for Host {
    fn try_install(
        &mut self,
        intent: ProgramPointSinkIntentV1,
    ) -> Result<(), ProgramPointSinkHostErrorV1> {
        if self
            .stamp
            .is_some_and(|stamp| intent.expected_stamp() != stamp)
        {
            return Err(ProgramPointSinkHostErrorV1::Rejected);
        }
        self.stamp = Some(intent.desired_stamp());
        Ok(())
    }
}

fn point_wire(expected: Srgb8) -> Vec<u8> {
    let mut builder = ProgramWireBuilderV1::new();
    builder
        .source(1, Srgb8::new([0x40; 3]))
        .fixed_target(2, 1)
        .surface_input_port(6)
        .opacity_input(5, 0.5)
        .solid_paint(3, 2)
        .opacity_paint(4, 3, 5)
        .input_surface(7, 6)
        .source_over_occurrence(OCCURRENCE, 4, 7, 64.0, 0.2, 2)
        .presentation_root(ROOT, OCCURRENCE)
        .presentation_target(ROOT, OCCURRENCE)
        .exact_visible_unary(true, 10, OCCURRENCE, expected)
        .output(OUTPUT, 4);
    builder.finish().unwrap()
}

fn point_attachment() -> ProgramAttachmentV1<Host> {
    compile_program_wire_v1(&point_wire(Srgb8::new([0x60; 3])))
        .unwrap()
        .attach(7, OUTPUT, SINK_OUTPUT, ROOT, OCCURRENCE, Host::default())
        .unwrap()
}

#[test]
fn point_tq_re_reads_current_attachment_and_binds_revisions() {
    let mut attachment = point_attachment();
    attachment
        .update_observed(1, &[ProgramScenarioV1::new(1, vec![Srgb8::new([0x80; 3])])])
        .unwrap();

    let mut state = AuthorityStateV1::new();
    assert_eq!(
        state
            .admit_modeled_point_technical_quality(&attachment, AuthorityExpectedCurrentV1::Vacant)
            .unwrap(),
        AuthorityAdmissionOutcomeV1::Installed
    );
    let first = state.read(AuthorityIdV1::TechnicalQuality).unwrap();

    attachment
        .update_observed(2, &[ProgramScenarioV1::new(1, vec![Srgb8::new([0x80; 3])])])
        .unwrap();

    assert!(matches!(
        state
            .admit_modeled_point_technical_quality(&attachment, AuthorityExpectedCurrentV1::Vacant),
        Err(TechnicalQualityAdmissionErrorV1::Authority(
            AuthorityAdmissionErrorV1::ExpectedCurrentRequired
        ))
    ));
    assert_eq!(
        state
            .admit_modeled_point_technical_quality(
                &attachment,
                AuthorityExpectedCurrentV1::Exact(first),
            )
            .unwrap(),
        AuthorityAdmissionOutcomeV1::Replaced
    );
    let second = state.read(AuthorityIdV1::TechnicalQuality).unwrap();
    assert_ne!(
        first, second,
        "revision/sink binding must change TQ identity"
    );
}

#[test]
fn point_tq_refuses_non_ready_attachment_without_minting_authority() {
    let attachment = point_attachment();
    let mut state = AuthorityStateV1::new();
    assert!(matches!(
        state
            .admit_modeled_point_technical_quality(&attachment, AuthorityExpectedCurrentV1::Vacant),
        Err(TechnicalQualityAdmissionErrorV1::Materialization(
            ProgramMaterializationAuthorityErrorV1::NotReady
        ))
    ));
    assert_eq!(state.read(AuthorityIdV1::TechnicalQuality), None);
}

fn field_request_with_capability<'a>(
    source: &'a [PremultipliedRgba8V1],
    destination: &'a [PremultipliedRgba8V1],
    revision: u64,
    capability: FieldRenderCapabilityV1,
) -> FieldEvaluationRequestV1<'a> {
    let extent = FieldExtentV1::try_new(1, 1).unwrap();
    let source = FieldRasterViewV1::try_new(FieldRasterIdentityV1::new(1), extent, source).unwrap();
    let destination =
        FieldRasterViewV1::try_new(FieldRasterIdentityV1::new(2), extent, destination).unwrap();
    FieldEvaluationRequestV1::try_new(
        FieldRequestIdV1::new(3),
        FieldOperatorInstanceIdV1::new(4),
        FieldGeometryV1::new(extent),
        DevicePixelRatioV1::try_new(1).unwrap(),
        FieldWorkingSpaceV1::EncodedSrgb8PremultipliedV1,
        FieldPrecisionV1::FixedQ32V1,
        FieldQuantizationV1::RoundHalfUpSrgb8V1,
        capability,
        FieldSceneRevisionV1::mint_for_test(ObservationStreamId::new(6), Revision::new(revision)),
        CarrierIntentV1::Present,
        FieldOperationV1::PremultipliedSourceOver {
            source,
            destination,
        },
    )
    .unwrap()
}

fn field_request<'a>(
    source: &'a [PremultipliedRgba8V1],
    destination: &'a [PremultipliedRgba8V1],
    revision: u64,
) -> FieldEvaluationRequestV1<'a> {
    field_request_with_capability(
        source,
        destination,
        revision,
        FieldRenderCapabilityV1::new(
            FieldRendererCapabilityV1::exact_reference(FieldRendererIdV1::new(5)),
            FieldOutputCapabilityV1::PremultipliedRgba8V1,
        ),
    )
}

#[test]
fn field_tq_accepts_only_fresh_exact_reference_replay() {
    let source = [PremultipliedRgba8V1::try_new([64, 32, 16, 128]).unwrap()];
    let destination = [PremultipliedRgba8V1::try_new([20, 20, 20, 255]).unwrap()];
    let request = field_request(&source, &destination, 7);
    let certificate = evaluate_whole_field(
        &request,
        FieldEvidenceV1::ExactReferenceWholeRaster {
            identity: FieldEvidenceIdentityV1::new(8),
        },
        &mut FieldEvaluationScratchV1::new(),
    )
    .unwrap();
    let replay = verify_exact_reference_for_tq(&certificate, &request).unwrap();

    let mut state = AuthorityStateV1::new();
    assert_eq!(
        state
            .admit_exact_reference_field_technical_quality(
                replay,
                AuthorityExpectedCurrentV1::Vacant,
            )
            .unwrap(),
        AuthorityAdmissionOutcomeV1::Installed
    );
    assert!(state.read(AuthorityIdV1::TechnicalQuality).is_some());
}

#[test]
fn field_tq_replay_rejects_stale_scene_before_authority_exists() {
    let source = [PremultipliedRgba8V1::try_new([64, 32, 16, 128]).unwrap()];
    let destination = [PremultipliedRgba8V1::try_new([20, 20, 20, 255]).unwrap()];
    let request = field_request(&source, &destination, 7);
    let certificate = evaluate_whole_field(
        &request,
        FieldEvidenceV1::ExactReferenceWholeRaster {
            identity: FieldEvidenceIdentityV1::new(8),
        },
        &mut FieldEvaluationScratchV1::new(),
    )
    .unwrap();
    let stale = field_request(&source, &destination, 8);

    assert!(matches!(
        verify_exact_reference_for_tq(&certificate, &stale),
        Err(crate::field_effect::FieldCertificateReplayErrorV1::SceneRevision { .. })
    ));
}

#[test]
fn field_tq_rejects_host_observation_even_when_whole_raster_is_exact() {
    let source = [PremultipliedRgba8V1::try_new([64, 32, 16, 128]).unwrap()];
    let destination = [PremultipliedRgba8V1::try_new([20, 20, 20, 255]).unwrap()];
    let permit = FieldHostConformancePermitV1::mint_for_test(
        FieldRendererIdV1::new(9),
        FieldHostConformanceIdV1::new(10),
    );
    let capability = FieldRenderCapabilityV1::new(
        FieldRendererCapabilityV1::host_conformant(permit),
        FieldOutputCapabilityV1::PremultipliedRgba8V1,
    );
    let request = field_request_with_capability(&source, &destination, 7, capability);

    let mut reference_scratch = FieldEvaluationScratchV1::new();
    let observed_pixels = evaluate_reference_full(&request, &mut reference_scratch)
        .unwrap()
        .to_vec();
    let extent = FieldExtentV1::try_new(1, 1).unwrap();
    let observed_raster =
        FieldRasterViewV1::try_new(FieldRasterIdentityV1::new(11), extent, &observed_pixels)
            .unwrap();
    let observed = ProspectiveObservedRasterV1::from_host_observation(
        FieldEvidenceIdentityV1::new(12),
        request_digest(&request),
        request.scene_revision(),
        FieldOutputCapabilityV1::PremultipliedRgba8V1,
        permit,
        observed_raster,
    );
    let certificate = evaluate_whole_field(
        &request,
        FieldEvidenceV1::ProspectiveObservedWholeRaster(observed),
        &mut FieldEvaluationScratchV1::new(),
    )
    .unwrap();

    assert!(matches!(
        verify_exact_reference_for_tq(&certificate, &request),
        Err(
            crate::field_effect::FieldCertificateReplayErrorV1::EvidenceClass {
                actual: crate::field_effect::FieldEvidenceClassV1::ProspectiveObservedWholeRaster
            }
        )
    ));
}

#[test]
fn tq_never_occupies_a_foreign_authority_lane() {
    let mut attachment = point_attachment();
    attachment
        .update_observed(1, &[ProgramScenarioV1::new(1, vec![Srgb8::new([0x80; 3])])])
        .unwrap();
    let mut state = AuthorityStateV1::new();
    state
        .admit_modeled_point_technical_quality(&attachment, AuthorityExpectedCurrentV1::Vacant)
        .unwrap();
    assert!(state.read(AuthorityIdV1::TechnicalQuality).is_some());
    assert_eq!(state.read(AuthorityIdV1::CleanConvention), None);
    assert_eq!(state.read(AuthorityIdV1::HumanCleanEvidence), None);
}
