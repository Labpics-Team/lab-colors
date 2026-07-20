use proptest::prelude::*;

use crate::Srgb8;
use crate::appearance::{EncodedPointPaintV1, OccurrenceId, PaintId, SurfaceInputPortId};
use crate::observation::{
    ObservationPayloadInput, ObservationState, ObservationStreamId, ObservationUpdateInput,
    ObservedScenarioSetInput, Revision, ScenarioId, ScenarioInput, SurfaceInputBinding,
};
use crate::recheck::{
    CompiledFixedRecheckV1, ExactOccurrenceRequirementV1, FixedRecheckBindErrorV1,
    FixedRecheckDecisionV1, RecheckProtocolErrorV1, checked_evidence_count,
};

const PAINT: PaintId = PaintId::new(7);
const OCCURRENCE_A: OccurrenceId = OccurrenceId::new(11);
const OCCURRENCE_B: OccurrenceId = OccurrenceId::new(12);
const SURFACE_A: SurfaceInputPortId = SurfaceInputPortId::new(21);
const SURFACE_B: SurfaceInputPortId = SurfaceInputPortId::new(22);
const STREAM: ObservationStreamId = ObservationStreamId::new(31);

fn scenario(
    id: u32,
    bindings: impl IntoIterator<Item = (SurfaceInputPortId, [u8; 3])>,
) -> ScenarioInput {
    ScenarioInput {
        id: ScenarioId::new(id),
        bindings: bindings
            .into_iter()
            .map(|(port, bytes)| SurfaceInputBinding {
                port,
                value: Srgb8::new(bytes),
            })
            .collect(),
    }
}

fn observation(
    revision: u64,
    schema: Vec<SurfaceInputPortId>,
    scenarios: Vec<ScenarioInput>,
) -> crate::observation::RevisionBoundObservationV1 {
    let mut state = ObservationState::new(STREAM, schema).unwrap();
    state
        .apply(ObservationUpdateInput {
            stream: STREAM,
            revision: Revision::new(revision),
            payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput { scenarios }),
        })
        .unwrap();
    state.current_observation().unwrap()
}

fn one_occurrence(target: [u8; 3]) -> CompiledFixedRecheckV1 {
    CompiledFixedRecheckV1::new(
        PAINT,
        vec![ExactOccurrenceRequirementV1::new(
            OCCURRENCE_A,
            SURFACE_A,
            Srgb8::new(target),
        )],
    )
    .unwrap()
}

fn encoded_paint(
    id: PaintId,
    source: Srgb8,
    opacity: f64,
) -> Result<EncodedPointPaintV1, crate::composition::OpacityAdmissionErrorV1> {
    Ok(EncodedPointPaintV1::from_admitted(
        id,
        source,
        crate::composition::AdmittedOpacityV1::new(opacity)?,
    ))
}

fn opaque(bytes: [u8; 3]) -> EncodedPointPaintV1 {
    encoded_paint(PAINT, Srgb8::new(bytes), 1.0).unwrap()
}

#[test]
fn construction_prebinds_candidate_and_surface_indices() {
    let plan = CompiledFixedRecheckV1::new(
        PAINT,
        vec![
            ExactOccurrenceRequirementV1::new(OCCURRENCE_B, SURFACE_B, Srgb8::new([18, 52, 86])),
            ExactOccurrenceRequirementV1::new(OCCURRENCE_A, SURFACE_A, Srgb8::new([18, 52, 86])),
        ],
    )
    .unwrap();
    let candidate = opaque([18, 52, 86]);
    let bound = plan
        .bind(&[SURFACE_A, SURFACE_B], candidate)
        .expect("sorted schema must prebind");
    assert_eq!(bound.paint(), candidate);
}

#[test]
fn bind_rejects_actual_paint_mismatch_and_missing_surface() {
    let wrong = encoded_paint(PaintId::new(99), Srgb8::new([0; 3]), 1.0).unwrap();
    assert_eq!(
        one_occurrence([0; 3]).bind(&[SURFACE_A], wrong),
        Err(FixedRecheckBindErrorV1::PaintMismatch {
            expected: PAINT,
            actual: PaintId::new(99),
        })
    );
    assert_eq!(
        one_occurrence([0; 3]).bind(&[SURFACE_B], opaque([0; 3])),
        Err(FixedRecheckBindErrorV1::MissingSurfacePort(SURFACE_A))
    );
}

#[test]
fn fixed_candidate_is_verified_only_after_every_final_occurrence_passes() {
    let plan = CompiledFixedRecheckV1::new(
        PAINT,
        vec![
            ExactOccurrenceRequirementV1::new(OCCURRENCE_B, SURFACE_B, Srgb8::new([18, 52, 86])),
            ExactOccurrenceRequirementV1::new(OCCURRENCE_A, SURFACE_A, Srgb8::new([18, 52, 86])),
        ],
    )
    .unwrap();
    let candidate = opaque([18, 52, 86]);
    let bound = plan.bind(&[SURFACE_A, SURFACE_B], candidate).unwrap();
    let observed = observation(
        4,
        vec![SURFACE_B, SURFACE_A],
        vec![
            scenario(9, [(SURFACE_A, [255; 3]), (SURFACE_B, [0; 3])]),
            scenario(3, [(SURFACE_B, [90; 3]), (SURFACE_A, [120; 3])]),
        ],
    );

    crate::composition::reset_source_over_evaluation_count();
    let FixedRecheckDecisionV1::Verified(verified) = bound.recheck(observed).unwrap() else {
        panic!("opaque fixed Paint must satisfy every exact occurrence");
    };

    assert_eq!(crate::composition::source_over_evaluation_count(), 4);
    assert_eq!(verified.paint(), candidate);
    assert_eq!(verified.observation().stream(), STREAM);
    assert_eq!(verified.observation().revision(), Revision::new(4));
    assert_eq!(verified.occurrences().len(), 4);
    assert_eq!(
        verified
            .occurrences()
            .iter()
            .map(|evidence| evidence.occurrence())
            .collect::<Vec<_>>(),
        vec![OCCURRENCE_A, OCCURRENCE_B, OCCURRENCE_A, OCCURRENCE_B]
    );
    assert!(
        verified
            .occurrences()
            .iter()
            .all(|evidence| { evidence.surface() == SURFACE_A || evidence.surface() == SURFACE_B })
    );
    assert!(verified.occurrences().iter().all(|evidence| {
        evidence.actual() == evidence.target()
            && evidence.invocation() == evidence.target()
            && Srgb8::new(evidence.physical_certificate().output_rgb()) == evidence.actual()
            && evidence.physical_program()
                == crate::appearance::PhysicalProgramIdentityV1::SolidOpacityOverSurfaceEncodedSrgb8V1
            && evidence.constraint()
                == crate::constraints::ExactConstraintIdentityV1::FinalSrgb8IdentityV1
            && evidence.release() == crate::constraints::ExactIdentityReleaseV1::V1
            && evidence.capability()
                == crate::constraints::ExactIdentityCapabilityV1::FinalOccurrenceSrgb8IdentityV1
    }));
    assert_eq!(verified.provenance(0), Some(&[ScenarioId::new(3)][..]));
}

#[test]
fn any_exact_violation_returns_violation_without_partial_verified_value() {
    let candidate = encoded_paint(PAINT, Srgb8::new([0; 3]), 0.25).unwrap();
    let bound = one_occurrence([64; 3])
        .bind(&[SURFACE_A], candidate)
        .unwrap();
    let observed = observation(
        1,
        vec![SURFACE_A],
        vec![
            scenario(1, [(SURFACE_A, [0; 3])]),
            scenario(2, [(SURFACE_A, [255; 3])]),
        ],
    );

    let FixedRecheckDecisionV1::Violation(violation) = bound.recheck(observed).unwrap() else {
        panic!("one failing occurrence must reject the fixed candidate");
    };
    assert_eq!(violation.occurrence(), OCCURRENCE_A);
    assert_eq!(violation.surface(), SURFACE_A);
    assert_eq!(violation.provenance(), &[ScenarioId::new(1)]);
    assert_eq!(violation.paint(), candidate);
    assert_eq!(violation.observation().revision(), Revision::new(1));
    assert_eq!(violation.target(), Srgb8::new([64; 3]));
    assert_ne!(violation.actual(), violation.target());
    assert_eq!(
        Srgb8::new(violation.physical_certificate().output_rgb()),
        violation.actual()
    );
    assert_eq!(violation.invocation(), violation.target());
    assert_eq!(
        violation.constraint(),
        crate::constraints::ExactConstraintIdentityV1::FinalSrgb8IdentityV1
    );
    assert_eq!(
        violation.release(),
        crate::constraints::ExactIdentityReleaseV1::V1
    );
    assert_eq!(
        violation.capability(),
        crate::constraints::ExactIdentityCapabilityV1::FinalOccurrenceSrgb8IdentityV1
    );
    assert_eq!(
        violation.physical_program(),
        crate::appearance::PhysicalProgramIdentityV1::SolidOpacityOverSurfaceEncodedSrgb8V1
    );
}

#[test]
fn recheck_rejects_observation_from_a_different_schema() {
    let bound = one_occurrence([0; 3])
        .bind(&[SURFACE_A], opaque([0; 3]))
        .unwrap();
    let observed = observation(
        1,
        vec![SURFACE_A, SURFACE_B],
        vec![scenario(1, [(SURFACE_A, [0; 3]), (SURFACE_B, [0; 3])])],
    );
    assert_eq!(
        bound.recheck(observed),
        Err(RecheckProtocolErrorV1::ObservationSchemaMismatch)
    );
}

#[test]
fn cardinality_overflow_is_rejected_before_compositing() {
    assert_eq!(
        checked_evidence_count(usize::MAX, 2),
        Err(RecheckProtocolErrorV1::ResourceExhausted)
    );
}

proptest! {
    #[test]
    fn opaque_exact_recheck_matches_final_encoded_bytes(
        source in any::<[u8; 3]>(),
        backdrop in any::<[u8; 3]>(),
    ) {
        let candidate = opaque(source);
        let bound = one_occurrence(source)
            .bind(&[SURFACE_A], candidate)
            .unwrap();
        let observed = observation(
            1,
            vec![SURFACE_A],
            vec![scenario(1, [(SURFACE_A, backdrop)])],
        );
        let decision = bound.recheck(observed).unwrap();
        prop_assert!(matches!(decision, FixedRecheckDecisionV1::Verified(_)));
    }
}
