use proptest::prelude::*;

use crate::Srgb8;
use crate::appearance::{OccurrenceId, PaintId, SurfaceInputPortId};
use crate::observation::{
    ObservationPayloadInput, ObservationState, ObservationStreamId, ObservationUpdateInput,
    ObservedScenarioSetInput, Revision, ScenarioId, ScenarioInput, SurfaceInputBinding,
    UnknownReasonId,
};
use crate::recheck::{
    CompiledFixedRecheckV1, EncodedPaintCandidateV1, ExactOccurrenceRequirementV1,
    FinalRecheckOutcomeV1, HoldErrorV1, RecheckProtocolErrorV1, ReuseErrorV1,
    checked_evidence_count,
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

fn ready_state(
    stream: ObservationStreamId,
    revision: u64,
    schema: Vec<SurfaceInputPortId>,
    scenarios: Vec<ScenarioInput>,
) -> ObservationState {
    let mut state = ObservationState::new(stream, schema).unwrap();
    state
        .apply(ObservationUpdateInput {
            stream,
            revision: Revision::new(revision),
            payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput { scenarios }),
        })
        .unwrap();
    state
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

fn opaque(bytes: [u8; 3]) -> EncodedPaintCandidateV1 {
    EncodedPaintCandidateV1::new(PAINT, Srgb8::new(bytes), 1.0).unwrap()
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
    let state = ready_state(
        STREAM,
        4,
        vec![SURFACE_B, SURFACE_A],
        vec![
            scenario(9, [(SURFACE_A, [255; 3]), (SURFACE_B, [0; 3])]),
            scenario(3, [(SURFACE_B, [90; 3]), (SURFACE_A, [120; 3])]),
        ],
    );

    crate::composition::reset_source_over_evaluation_count();
    let FinalRecheckOutcomeV1::Verified(verified) =
        plan.recheck(&state, opaque([18, 52, 86])).unwrap()
    else {
        panic!("opaque fixed Paint must satisfy every exact occurrence");
    };

    assert_eq!(crate::composition::source_over_evaluation_count(), 4);
    assert_eq!(verified.paint().id(), PAINT);
    assert_eq!(verified.paint().source(), Srgb8::new([18, 52, 86]));
    assert_eq!(verified.paint().opacity_bits(), 1.0_f64.to_bits());
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
            .all(|evidence| evidence.actual() == evidence.target())
    );
    assert!(verified.occurrences().iter().all(|evidence| {
        evidence.constraint() == crate::constraints::ExactConstraintIdentityV1::FinalSrgb8IdentityV1
            && evidence.release() == crate::constraints::ExactIdentityReleaseV1::V1
            && evidence.capability()
                == crate::constraints::ExactIdentityCapabilityV1::FinalOccurrenceSrgb8IdentityV1
            && evidence.invocation() == evidence.target()
    }));
}

#[test]
fn any_failed_case_returns_one_infeasible_outcome_without_partial_verified_value() {
    let plan = one_occurrence([64, 64, 64]);
    let state = ready_state(
        STREAM,
        1,
        vec![SURFACE_A],
        vec![
            scenario(1, [(SURFACE_A, [0; 3])]),
            scenario(2, [(SURFACE_A, [255; 3])]),
        ],
    );
    let candidate = EncodedPaintCandidateV1::new(PAINT, Srgb8::new([0; 3]), 0.25).unwrap();

    let FinalRecheckOutcomeV1::Infeasible(failure) = plan.recheck(&state, candidate).unwrap()
    else {
        panic!("one failing final occurrence must reject the whole fixed candidate");
    };
    assert_eq!(failure.occurrence(), OCCURRENCE_A);
    assert_eq!(failure.surface(), SURFACE_A);
    assert_eq!(failure.provenance(), &[ScenarioId::new(1)]);
    assert_eq!(
        failure.physical_program(),
        crate::appearance::PhysicalProgramIdentityV1::SolidOpacityOverSurfaceEncodedSrgb8V1
    );
    assert_eq!(failure.target(), Srgb8::new([64; 3]));
    assert_ne!(failure.actual(), failure.target());
    assert_eq!(
        Srgb8::new(failure.physical_certificate().output_rgb()),
        failure.actual()
    );
    assert_eq!(failure.invocation(), failure.target());
    assert_eq!(
        failure.constraint(),
        crate::constraints::ExactConstraintIdentityV1::FinalSrgb8IdentityV1
    );
    assert_eq!(
        failure.release(),
        crate::constraints::ExactIdentityReleaseV1::V1
    );
    assert_eq!(
        failure.capability(),
        crate::constraints::ExactIdentityCapabilityV1::FinalOccurrenceSrgb8IdentityV1
    );
}

#[test]
fn waiting_stale_infeasible_verified_and_hold_are_distinct() {
    let plan = one_occurrence([10, 20, 30]);
    let candidate = opaque([10, 20, 30]);
    let mut state = ObservationState::new(STREAM, vec![SURFACE_A]).unwrap();

    crate::composition::reset_source_over_evaluation_count();
    assert!(matches!(
        plan.recheck(&state, candidate).unwrap(),
        FinalRecheckOutcomeV1::Waiting(_)
    ));
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);

    state
        .apply(ObservationUpdateInput {
            stream: STREAM,
            revision: Revision::new(1),
            payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
                scenarios: vec![scenario(1, [(SURFACE_A, [255; 3])])],
            }),
        })
        .unwrap();
    let FinalRecheckOutcomeV1::Verified(previous) = plan.recheck(&state, candidate).unwrap() else {
        panic!("ready exact occurrence must verify");
    };

    state
        .apply(ObservationUpdateInput {
            stream: STREAM,
            revision: Revision::new(2),
            payload: ObservationPayloadInput::Unknown(UnknownReasonId::new(1)),
        })
        .unwrap();
    let FinalRecheckOutcomeV1::Stale(stale) = plan.recheck(&state, candidate).unwrap() else {
        panic!("lost current evidence must be stale");
    };
    assert_eq!(crate::composition::source_over_evaluation_count(), 1);
    let hold = stale
        .hold(&previous)
        .expect("matching previous evidence may be presented");
    assert_eq!(hold.previous(), &previous);
    assert_eq!(hold.current_revision(), Revision::new(2));
    assert_eq!(state.current_set(), None);

    let unrelated = one_occurrence([1, 2, 3]);
    let unrelated_state = ready_state(
        STREAM,
        3,
        vec![SURFACE_A],
        vec![scenario(1, [(SURFACE_A, [255; 3])])],
    );
    let FinalRecheckOutcomeV1::Verified(other) = unrelated
        .recheck(&unrelated_state, opaque([1, 2, 3]))
        .unwrap()
    else {
        panic!("control evidence");
    };
    assert_eq!(
        stale.hold(&other),
        Err(HoldErrorV1::PreviousEvidenceMismatch)
    );
}

#[test]
fn hold_rejects_only_the_paint_mismatch() {
    let plan = one_occurrence([10, 20, 30]);
    let candidate = opaque([10, 20, 30]);
    let mut state = ready_state(
        STREAM,
        1,
        vec![SURFACE_A],
        vec![scenario(1, [(SURFACE_A, [255; 3])])],
    );
    let FinalRecheckOutcomeV1::Verified(previous) = plan.recheck(&state, candidate).unwrap() else {
        panic!("control evidence");
    };
    state
        .apply(ObservationUpdateInput {
            stream: STREAM,
            revision: Revision::new(2),
            payload: ObservationPayloadInput::Unknown(UnknownReasonId::new(1)),
        })
        .unwrap();
    let changed_paint = opaque([10, 20, 31]);
    let FinalRecheckOutcomeV1::Stale(stale) = plan.recheck(&state, changed_paint).unwrap() else {
        panic!("lost current evidence must be stale");
    };

    assert_eq!(
        stale.hold(&previous),
        Err(HoldErrorV1::PreviousEvidenceMismatch)
    );
}

#[test]
fn hold_rejects_only_the_stream_mismatch() {
    let plan = one_occurrence([10, 20, 30]);
    let candidate = opaque([10, 20, 30]);
    let original = ready_state(
        STREAM,
        1,
        vec![SURFACE_A],
        vec![scenario(1, [(SURFACE_A, [255; 3])])],
    );
    let FinalRecheckOutcomeV1::Verified(previous) = plan.recheck(&original, candidate).unwrap()
    else {
        panic!("control evidence");
    };

    let other_stream = ObservationStreamId::new(32);
    let mut state = ready_state(
        other_stream,
        1,
        vec![SURFACE_A],
        vec![scenario(1, [(SURFACE_A, [255; 3])])],
    );
    state
        .apply(ObservationUpdateInput {
            stream: other_stream,
            revision: Revision::new(2),
            payload: ObservationPayloadInput::Unknown(UnknownReasonId::new(1)),
        })
        .unwrap();
    let FinalRecheckOutcomeV1::Stale(stale) = plan.recheck(&state, candidate).unwrap() else {
        panic!("lost current evidence must be stale");
    };

    assert_eq!(
        stale.hold(&previous),
        Err(HoldErrorV1::PreviousEvidenceMismatch)
    );
}

#[test]
fn hold_rejects_only_the_observed_set_mismatch() {
    let plan = one_occurrence([10, 20, 30]);
    let candidate = opaque([10, 20, 30]);
    let original = ready_state(
        STREAM,
        1,
        vec![SURFACE_A],
        vec![scenario(1, [(SURFACE_A, [255; 3])])],
    );
    let FinalRecheckOutcomeV1::Verified(previous) = plan.recheck(&original, candidate).unwrap()
    else {
        panic!("control evidence");
    };

    let mut state = ready_state(
        STREAM,
        1,
        vec![SURFACE_A],
        vec![scenario(1, [(SURFACE_A, [254; 3])])],
    );
    state
        .apply(ObservationUpdateInput {
            stream: STREAM,
            revision: Revision::new(2),
            payload: ObservationPayloadInput::Unknown(UnknownReasonId::new(1)),
        })
        .unwrap();
    let FinalRecheckOutcomeV1::Stale(stale) = plan.recheck(&state, candidate).unwrap() else {
        panic!("lost current evidence must be stale");
    };

    assert_eq!(
        stale.hold(&previous),
        Err(HoldErrorV1::PreviousEvidenceMismatch)
    );
}

#[test]
fn duplicate_physical_scenarios_evaluate_once_and_retain_all_provenance() {
    let plan = one_occurrence([7, 8, 9]);
    let state = ready_state(
        STREAM,
        1,
        vec![SURFACE_A],
        vec![
            scenario(30, [(SURFACE_A, [250; 3])]),
            scenario(10, [(SURFACE_A, [250; 3])]),
            scenario(20, [(SURFACE_A, [250; 3])]),
        ],
    );

    crate::composition::reset_source_over_evaluation_count();
    let FinalRecheckOutcomeV1::Verified(verified) =
        plan.recheck(&state, opaque([7, 8, 9])).unwrap()
    else {
        panic!("opaque candidate must pass the grouped physical case");
    };
    assert_eq!(crate::composition::source_over_evaluation_count(), 1);
    assert_eq!(verified.occurrences().len(), 1);
    assert_eq!(
        verified.provenance(0).unwrap(),
        &[
            ScenarioId::new(10),
            ScenarioId::new(20),
            ScenarioId::new(30)
        ]
    );
}

#[test]
fn declaration_permutations_produce_one_canonical_requirement_and_evidence_order() {
    let requirement_a =
        ExactOccurrenceRequirementV1::new(OCCURRENCE_A, SURFACE_A, Srgb8::new([1, 2, 3]));
    let requirement_b =
        ExactOccurrenceRequirementV1::new(OCCURRENCE_B, SURFACE_B, Srgb8::new([1, 2, 3]));
    let left = CompiledFixedRecheckV1::new(PAINT, vec![requirement_b, requirement_a]).unwrap();
    let right = CompiledFixedRecheckV1::new(PAINT, vec![requirement_a, requirement_b]).unwrap();
    assert_eq!(left, right);

    let first = ready_state(
        STREAM,
        9,
        vec![SURFACE_B, SURFACE_A],
        vec![
            scenario(2, [(SURFACE_B, [0; 3]), (SURFACE_A, [255; 3])]),
            scenario(1, [(SURFACE_A, [8; 3]), (SURFACE_B, [9; 3])]),
        ],
    );
    let second = ready_state(
        STREAM,
        9,
        vec![SURFACE_A, SURFACE_B],
        vec![
            scenario(1, [(SURFACE_B, [9; 3]), (SURFACE_A, [8; 3])]),
            scenario(2, [(SURFACE_A, [255; 3]), (SURFACE_B, [0; 3])]),
        ],
    );
    let candidate = opaque([1, 2, 3]);
    assert_eq!(
        left.recheck(&first, candidate),
        right.recheck(&second, candidate)
    );
}

#[test]
fn singleton_recheck_is_differentially_equal_to_g1a_and_point_program() {
    let source = Srgb8::new([0, 40, 200]);
    let backdrop = Srgb8::new([240, 230, 220]);
    let alpha = 0.5;
    let occurrence = crate::appearance::PointOpacityOverSurfaceV1::evaluate(
        source.bytes(),
        alpha,
        backdrop.bytes(),
    )
    .unwrap();
    let target = Srgb8::new(occurrence.visible());
    let g1a = crate::analog::ExactAlphaProgramV1::evaluate(
        crate::analog::AuthoredAlphaBindingIdV1::Standalone,
        target,
        source,
        crate::composition::AdmittedOpacityV1::new(alpha).unwrap(),
        backdrop,
    )
    .unwrap();
    let plan = one_occurrence(target.bytes());
    let state = ready_state(
        STREAM,
        1,
        vec![SURFACE_A],
        vec![scenario(1, [(SURFACE_A, backdrop.bytes())])],
    );
    let candidate = EncodedPaintCandidateV1::new(PAINT, source, alpha).unwrap();
    let FinalRecheckOutcomeV1::Verified(verified) = plan.recheck(&state, candidate).unwrap() else {
        panic!("the same point occurrence and exact evaluator must pass V1a");
    };
    let evidence = &verified.occurrences()[0];

    assert_eq!(evidence.actual().bytes(), occurrence.visible());
    assert_eq!(
        evidence.physical_program(),
        crate::appearance::PointOpacityOverSurfaceV1::physical_identity()
    );
    assert_eq!(
        evidence.program_occurrence_binding(),
        occurrence.program_occurrence_binding()
    );
    assert_eq!(evidence.physical_certificate(), *occurrence.certificate());
    assert_eq!(evidence.physical_certificate(), *g1a.certificate());
}

#[test]
fn production_g1a_uses_the_same_bound_evaluator_instead_of_manual_evidence() {
    let analog = include_str!("analog.rs");
    assert_eq!(
        analog
            .matches("assess(&occurrence, &ExactSrgb8IdentityV1")
            .count(),
        1
    );
    for forbidden in [
        "ExactSrgb8IdentityV1::evaluate",
        "enum ExactIdentityReleaseV1",
        "enum ExactIdentityCapabilityV1",
        "fn constraint_identity",
    ] {
        assert!(
            !analog.contains(forbidden),
            "G1a still bypasses F1a: {forbidden}"
        );
    }
}

proptest! {
    #[test]
    fn every_physical_case_is_required_not_just_any_case(
        first in any::<[u8; 3]>(),
        mut second in any::<[u8; 3]>(),
    ) {
        if second == first {
            second[0] = second[0].wrapping_add(1);
        }
        let (passing, failing) = if first < second { (first, second) } else { (second, first) };
        let plan = one_occurrence(passing);
        let state = ready_state(
            STREAM,
            1,
            vec![SURFACE_A],
            vec![
                scenario(1, [(SURFACE_A, failing)]),
                scenario(2, [(SURFACE_A, passing)]),
            ],
        );
        let candidate = EncodedPaintCandidateV1::new(PAINT, Srgb8::new([99, 88, 77]), 0.0)
            .unwrap();

        prop_assert!(matches!(
            plan.recheck(&state, candidate),
            Ok(FinalRecheckOutcomeV1::Infeasible(_))
        ));
    }

    #[test]
    fn verified_evidence_binds_visible_target_backdrop_and_evaluator_identity(
        source in any::<[u8; 3]>(),
        backdrop in any::<[u8; 3]>(),
    ) {
        let plan = one_occurrence(source);
        let state = ready_state(
            STREAM,
            1,
            vec![SURFACE_A],
            vec![scenario(5, [(SURFACE_A, backdrop)])],
        );
        let FinalRecheckOutcomeV1::Verified(verified) = plan.recheck(&state, opaque(source)).unwrap()
        else {
            return Err(TestCaseError::fail("opaque exact candidate was not verified"));
        };
        let evidence = &verified.occurrences()[0];
        let physical = evidence.physical_certificate();

        prop_assert_eq!(evidence.surface(), SURFACE_A);
        prop_assert_eq!(evidence.invocation(), Srgb8::new(source));
        prop_assert_eq!(evidence.target(), Srgb8::new(source));
        prop_assert_eq!(evidence.actual(), Srgb8::new(source));
        prop_assert_eq!(physical.backdrop_rgb(), backdrop);
        prop_assert_eq!(physical.output_rgb(), source);
        prop_assert_eq!(evidence.constraint(), crate::constraints::ExactConstraintIdentityV1::FinalSrgb8IdentityV1);
        prop_assert_eq!(evidence.release(), crate::constraints::ExactIdentityReleaseV1::V1);
        prop_assert_eq!(evidence.capability(), crate::constraints::ExactIdentityCapabilityV1::FinalOccurrenceSrgb8IdentityV1);
    }
}

#[test]
fn exact_revision_and_full_context_are_required_for_reuse() {
    let plan = one_occurrence([10, 20, 30]);
    let candidate = opaque([10, 20, 30]);
    let state = ready_state(
        STREAM,
        7,
        vec![SURFACE_A],
        vec![scenario(1, [(SURFACE_A, [255; 3])])],
    );
    let FinalRecheckOutcomeV1::Verified(verified) = plan.recheck(&state, candidate).unwrap() else {
        panic!("control evidence");
    };
    assert_eq!(verified.reuse_for(&plan, &state, candidate), Ok(()));

    let changed_revision = ready_state(
        STREAM,
        8,
        vec![SURFACE_A],
        vec![scenario(1, [(SURFACE_A, [255; 3])])],
    );
    assert_eq!(
        verified.reuse_for(&plan, &changed_revision, candidate),
        Err(ReuseErrorV1::ObservationMismatch)
    );
    let changed_stream = ready_state(
        ObservationStreamId::new(32),
        7,
        vec![SURFACE_A],
        vec![scenario(1, [(SURFACE_A, [255; 3])])],
    );
    assert_eq!(
        verified.reuse_for(&plan, &changed_stream, candidate),
        Err(ReuseErrorV1::ObservationMismatch)
    );
    let changed_payload = ready_state(
        STREAM,
        7,
        vec![SURFACE_A],
        vec![scenario(1, [(SURFACE_A, [0; 3])])],
    );
    assert_eq!(
        verified.reuse_for(&plan, &changed_payload, candidate),
        Err(ReuseErrorV1::ObservationMismatch)
    );
    let changed_invocation = one_occurrence([10, 20, 31]);
    assert_eq!(
        verified.reuse_for(&changed_invocation, &state, candidate),
        Err(ReuseErrorV1::RequirementMismatch)
    );
    let changed_occurrence = CompiledFixedRecheckV1::new(
        PAINT,
        vec![ExactOccurrenceRequirementV1::new(
            OCCURRENCE_B,
            SURFACE_A,
            Srgb8::new([10, 20, 30]),
        )],
    )
    .unwrap();
    assert_eq!(
        verified.reuse_for(&changed_occurrence, &state, candidate),
        Err(ReuseErrorV1::RequirementMismatch)
    );
    let changed_surface = CompiledFixedRecheckV1::new(
        PAINT,
        vec![ExactOccurrenceRequirementV1::new(
            OCCURRENCE_A,
            SURFACE_B,
            Srgb8::new([10, 20, 30]),
        )],
    )
    .unwrap();
    assert_eq!(
        verified.reuse_for(&changed_surface, &state, candidate),
        Err(ReuseErrorV1::RequirementMismatch)
    );
    assert_eq!(
        verified.reuse_for(&plan, &state, opaque([10, 20, 31])),
        Err(ReuseErrorV1::PaintMismatch)
    );
}

#[test]
fn canonical_tuple_cannot_be_reinterpreted_under_a_different_surface_schema() {
    let surface_x = SurfaceInputPortId::new(20);
    let plan = one_occurrence([10, 20, 30]);
    let candidate = opaque([10, 20, 30]);
    let original = ready_state(
        STREAM,
        7,
        vec![SURFACE_A, SURFACE_B],
        vec![scenario(1, [(SURFACE_A, [11; 3]), (SURFACE_B, [22; 3])])],
    );
    let FinalRecheckOutcomeV1::Verified(verified) = plan.recheck(&original, candidate).unwrap()
    else {
        panic!("control evidence");
    };

    // Обе canonical tuple побайтно равны: только immutable schema объясняет,
    // какой surface означает каждая позиция.
    let mut reinterpreted = ready_state(
        STREAM,
        7,
        vec![surface_x, SURFACE_A],
        vec![scenario(1, [(surface_x, [11; 3]), (SURFACE_A, [22; 3])])],
    );
    assert_eq!(
        verified.reuse_for(&plan, &reinterpreted, candidate),
        Err(ReuseErrorV1::ObservationMismatch)
    );

    reinterpreted
        .apply(ObservationUpdateInput {
            stream: STREAM,
            revision: Revision::new(8),
            payload: ObservationPayloadInput::Unknown(UnknownReasonId::new(9)),
        })
        .unwrap();
    let FinalRecheckOutcomeV1::Stale(stale) = plan.recheck(&reinterpreted, candidate).unwrap()
    else {
        panic!("lost current evidence must be stale");
    };
    assert_eq!(
        stale.hold(&verified),
        Err(HoldErrorV1::PreviousEvidenceMismatch)
    );
}

#[test]
fn infeasible_witness_keeps_the_authored_paint_identity() {
    let second_paint = PaintId::new(8);
    let requirement = || {
        vec![ExactOccurrenceRequirementV1::new(
            OCCURRENCE_A,
            SURFACE_A,
            Srgb8::new([64; 3]),
        )]
    };
    let first_plan = CompiledFixedRecheckV1::new(PAINT, requirement()).unwrap();
    let second_plan = CompiledFixedRecheckV1::new(second_paint, requirement()).unwrap();
    let state = ready_state(
        STREAM,
        1,
        vec![SURFACE_A],
        vec![scenario(1, [(SURFACE_A, [0; 3])])],
    );
    let first_candidate = EncodedPaintCandidateV1::new(PAINT, Srgb8::new([0; 3]), 0.25).unwrap();
    let second_candidate =
        EncodedPaintCandidateV1::new(second_paint, Srgb8::new([0; 3]), 0.25).unwrap();

    let FinalRecheckOutcomeV1::Infeasible(first) =
        first_plan.recheck(&state, first_candidate).unwrap()
    else {
        panic!("control candidate must fail");
    };
    let FinalRecheckOutcomeV1::Infeasible(second) =
        second_plan.recheck(&state, second_candidate).unwrap()
    else {
        panic!("control candidate must fail");
    };

    assert_eq!(first.paint().id(), PAINT);
    assert_eq!(second.paint().id(), second_paint);
    assert_ne!(first.paint(), second.paint());
    assert_eq!(first.physical_certificate(), second.physical_certificate());
}

#[test]
fn evidence_capacity_overflow_is_rejected_before_compositing() {
    crate::composition::reset_source_over_evaluation_count();
    assert_eq!(checked_evidence_count(3, 4), Ok(12));
    assert_eq!(
        checked_evidence_count(usize::MAX, 2),
        Err(RecheckProtocolErrorV1::ResourceExhausted)
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
}

#[test]
fn ready_recheck_binds_surface_indices_before_the_case_product() {
    let source = include_str!("recheck.rs");
    let (_, ready_tail) = source
        .split_once("    fn recheck_ready(")
        .expect("ready recheck function");
    let (ready_body, _) = ready_tail
        .split_once("\n    }\n}\n\n/// Вычисляет")
        .expect("ready recheck body");

    assert!(ready_body.contains("surface_indices"));
    assert!(!ready_body.contains("binary_search"));
}

#[test]
fn candidate_and_requirement_errors_are_rejected_before_compositing() {
    assert!(CompiledFixedRecheckV1::new(PAINT, vec![]).is_err());
    assert!(
        CompiledFixedRecheckV1::new(
            PAINT,
            vec![
                ExactOccurrenceRequirementV1::new(OCCURRENCE_A, SURFACE_A, Srgb8::new([1; 3]),),
                ExactOccurrenceRequirementV1::new(OCCURRENCE_A, SURFACE_B, Srgb8::new([2; 3]),),
            ],
        )
        .is_err()
    );
    assert!(EncodedPaintCandidateV1::new(PAINT, Srgb8::new([0; 3]), f64::NAN).is_err());

    let state = ready_state(
        STREAM,
        1,
        vec![SURFACE_A],
        vec![scenario(1, [(SURFACE_A, [0; 3])])],
    );
    crate::composition::reset_source_over_evaluation_count();
    let wrong_paint =
        EncodedPaintCandidateV1::new(PaintId::new(8), Srgb8::new([0; 3]), 1.0).unwrap();
    assert_eq!(
        one_occurrence([0; 3]).recheck(&state, wrong_paint),
        Err(RecheckProtocolErrorV1::PaintMismatch {
            expected: PAINT,
            actual: PaintId::new(8),
        })
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);

    let missing_surface_plan = CompiledFixedRecheckV1::new(
        PAINT,
        vec![ExactOccurrenceRequirementV1::new(
            OCCURRENCE_A,
            SURFACE_B,
            Srgb8::new([0; 3]),
        )],
    )
    .unwrap();
    assert_eq!(
        missing_surface_plan.recheck(&state, opaque([0; 3])),
        Err(RecheckProtocolErrorV1::MissingSurfacePort(SURFACE_B))
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
}

#[test]
fn signed_zero_has_one_encoded_paint_identity() {
    let positive = EncodedPaintCandidateV1::new(PAINT, Srgb8::new([1, 2, 3]), 0.0).unwrap();
    let negative = EncodedPaintCandidateV1::new(PAINT, Srgb8::new([1, 2, 3]), -0.0).unwrap();
    assert_eq!(positive, negative);
    assert_eq!(negative.opacity_bits(), 0.0_f64.to_bits());
}

#[test]
fn module_exposes_revision_bound_evidence_not_a_fictional_terminal_certificate() {
    let source = include_str!("recheck.rs");
    assert!(source.contains("pub(crate) struct RevisionBoundRecheckV1"));
    for forbidden in [
        "FinalEmissionCertificate",
        "TerminalEmissionBinding",
        "ProgramIdV1",
        "SessionIdV1",
        "OutputProfileIdV1",
        "RendererProfileIdV1",
        "default_program",
        "default_session",
    ] {
        assert!(
            !source.contains(forbidden),
            "V1a invented a consumer-owned identity: {forbidden}"
        );
    }
}
