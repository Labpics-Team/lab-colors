use proptest::prelude::*;

use crate::Srgb8;
use crate::appearance::{EncodedPointPaintV1, OccurrenceId, PaintId, SurfaceInputPortId};
use crate::constraints::{DisplayReadabilityCurveV1, HardDecision, ReadabilityPolarityV1};
use crate::joint::{
    CandidateOrdinalV1, JointCandidateSetV1, JointCandidateTupleV1, JointConstraintIdV1,
    JointVisibleTargetV1, PointwiseJointHardConstraintV1, PointwiseJointPointProgramV1,
};
use crate::observation::{
    ObservationPayloadInput, ObservationState, ObservationStreamId, ObservationUpdateInput,
    ObservedScenarioSetInput, Revision, ScenarioId, ScenarioInput, SurfaceInputBinding,
};
use crate::recheck::{
    BoundReadabilityRecheckV1, CompiledFixedRecheckV1, CompiledReadabilityRecheckV1,
    ExactOccurrenceRequirementV1, FixedRecheckBindErrorV1, FixedRecheckDecisionV1,
    JointReadabilityResolutionV1, PointSupportAdmissionErrorV1, PointSupportCriterionAssessmentV1,
    PointSupportCriterionRequirementV1, PointSupportDropFractionV1, PointSupportEvaluationErrorV1,
    PointSupportOccurrenceV1, PointSupportStabilityAssessmentV1, PointSupportStabilityPolicyV1,
    PointSupportStatusV1, ReadabilityOccurrenceV1, RecheckProtocolErrorV1, checked_evidence_count,
    evaluate_point_support_v1, resolve_across_all_samples,
};
use crate::solve::Floor;
use crate::wcag22::{Wcag22ApplicableDecisionV1, Wcag22CriterionV1};

const PAINT: PaintId = PaintId::new(7);
const OCCURRENCE_A: OccurrenceId = OccurrenceId::new(11);
const OCCURRENCE_B: OccurrenceId = OccurrenceId::new(12);
const SURFACE_A: SurfaceInputPortId = SurfaceInputPortId::new(21);
const SURFACE_B: SurfaceInputPortId = SurfaceInputPortId::new(22);
const STREAM: ObservationStreamId = ObservationStreamId::new(31);

// Joint re-solve bridge fixtures (C8d step 3): two distinct linked paints over a
// single observed root backdrop surface.
const JLOWER: PaintId = PaintId::new(41);
const JUPPER: PaintId = PaintId::new(42);
const JROOT: SurfaceInputPortId = SurfaceInputPortId::new(51);

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

fn bound_readability(
    occurrences: Vec<ReadabilityOccurrenceV1>,
    schema: &[SurfaceInputPortId],
) -> BoundReadabilityRecheckV1 {
    CompiledReadabilityRecheckV1::new(occurrences)
        .expect("non-empty, de-duplicated descriptors compile")
        .bind(schema)
        .expect("schema must carry every descriptor's surface port")
}

#[test]
fn unified_occurrence_solid_alpha1_byte_identical() {
    // N1: a solid role modeled at opacity=OPAQUE composites to its own source
    // bytes over any backdrop, so its readability (lc, wcag) is BYTE-identical
    // to the legacy colour-only `recheck_against` — the §16 singleton contract.
    let tint = [0x00, 0x57, 0xBB];
    let backdrop = [0xFF, 0xFF, 0xFF];
    let bound = bound_readability(
        vec![ReadabilityOccurrenceV1::new(
            OCCURRENCE_A,
            SURFACE_A,
            opaque(tint),
            Floor::AaText,
        )],
        &[SURFACE_A],
    );
    let observed = observation(
        1,
        vec![SURFACE_A],
        vec![scenario(1, [(SURFACE_A, backdrop)])],
    );

    let report = bound.recheck(observed).unwrap();
    assert_eq!(report.verdicts().len(), 1);
    let measurement = report.verdicts()[0].measurement();

    let vc = crate::spaces::vc::ViewingConditions::srgb();
    let legacy = crate::semantic::recheck_against("#FFFFFF", &["#0057BB"], &vc).unwrap()[0];
    assert_eq!(measurement.lc().to_bits(), legacy.0.to_bits());
    assert_eq!(measurement.wcag().to_bits(), legacy.1.to_bits());
    // A solid role clears the AA text floor against white by a wide margin.
    assert!(!report.verdicts()[0].is_violation());
}

#[test]
fn translucent_occurrence_rechecked_over_support() {
    // N: a translucent {tint, alpha<1} is one descriptor whose composite is
    // rechecked over EVERY backdrop sample. A white tint at 0.6 clears AA over
    // black (composite #999999) but drops below it over #B4B4B4 (composite
    // #E1E1E1). The breaching sample yields a TYPED Violation carrying B's
    // provenance — a value the colour-only path structurally cannot produce.
    let paint = encoded_paint(PAINT, Srgb8::new([255, 255, 255]), 0.6).unwrap();
    let bound = bound_readability(
        vec![ReadabilityOccurrenceV1::new(
            OCCURRENCE_A,
            SURFACE_A,
            paint,
            Floor::AaText,
        )],
        &[SURFACE_A],
    );
    let observed = observation(
        1,
        vec![SURFACE_A],
        vec![
            scenario(1, [(SURFACE_A, [0, 0, 0])]),
            scenario(2, [(SURFACE_A, [180, 180, 180])]),
        ],
    );

    let report = bound.recheck(observed).unwrap();
    assert!(report.is_breached());

    let violations: Vec<usize> = (0..report.verdicts().len())
        .filter(|&index| report.verdicts()[index].is_violation())
        .collect();
    assert_eq!(violations.len(), 1, "only the #B4B4B4 sample breaches AA");
    let breaching = violations[0];
    assert_eq!(
        report.provenance(breaching),
        Some(&[ScenarioId::new(2)][..])
    );
    assert_eq!(report.verdicts()[breaching].surface(), SURFACE_A);
    assert_eq!(report.verdicts()[breaching].occurrence(), OCCURRENCE_A);
    // The typed violation payload carries a negative surplus (below the floor).
    assert!(report.verdicts()[breaching].surplus() < 0.0);
    // The other sample (black backdrop) passed — this is not a global reject.
    assert!(!report.verdicts()[0].is_violation());
    assert!(report.verdicts()[0].surplus() > 0.0);
    // Pin the COMPOSITED readability, not the opaque tint. This is what makes
    // the alpha mutation bite: the passing sample reads the composite #999999
    // (wcag ≈ 7.371), NOT opaque white over black (21.000); the breaching sample
    // reads the composite #E1E1E1 (wcag ≈ 1.586), NOT opaque white over #B4B4B4
    // (2.073). An "always-OPAQUE" recheck would still split PASS/VIOLATION with
    // the same provenance and surplus signs, so only these value pins prove the
    // paint's real opacity is threaded through the compositing recheck.
    assert!(
        (report.verdicts()[0].measurement().wcag() - 7.371).abs() < 1e-3,
        "passing sample must read composited #999999 (≈7.371), not opaque white (21.000): {}",
        report.verdicts()[0].measurement().wcag(),
    );
    assert!(
        (report.verdicts()[breaching].measurement().wcag() - 1.586).abs() < 1e-3,
        "breaching sample must read composited #E1E1E1 (≈1.586), not opaque white (2.073): {}",
        report.verdicts()[breaching].measurement().wcag(),
    );
}

#[test]
fn finite_shape_polarity_typed_decision() {
    // N: finite / shape / polarity are core-owned. Every verdict's scalars are
    // finite by construction and the direction is a TYPED polarity, not a host
    // `Number.isFinite` / `Math.abs` guard.

    // Dark-on-light: #000000 over white — normal polarity, a wide pass.
    let bound = bound_readability(
        vec![ReadabilityOccurrenceV1::new(
            OCCURRENCE_A,
            SURFACE_A,
            opaque([0, 0, 0]),
            Floor::AaText,
        )],
        &[SURFACE_A],
    );
    let observed = observation(
        1,
        vec![SURFACE_A],
        vec![scenario(1, [(SURFACE_A, [255; 3])])],
    );
    let report = bound.recheck(observed).unwrap();
    let verdict = report.verdicts()[0];
    assert!(verdict.measurement().lc().is_finite() && verdict.measurement().wcag().is_finite());
    assert!(matches!(verdict.decision(), HardDecision::Pass(_)));
    assert_eq!(verdict.polarity(), ReadabilityPolarityV1::DarkOnLight);
    assert!(verdict.surplus() > 0.0);

    // Light-on-dark: #FFFFFF over black — reverse polarity, still a pass.
    let bound = bound_readability(
        vec![ReadabilityOccurrenceV1::new(
            OCCURRENCE_A,
            SURFACE_A,
            opaque([255, 255, 255]),
            Floor::AaText,
        )],
        &[SURFACE_A],
    );
    let observed = observation(1, vec![SURFACE_A], vec![scenario(1, [(SURFACE_A, [0; 3])])]);
    let report = bound.recheck(observed).unwrap();
    assert_eq!(
        report.verdicts()[0].polarity(),
        ReadabilityPolarityV1::LightOnDark
    );

    // Indistinct: identical fg/bg — a typed `Indistinct`, `lc == 0`, never NaN.
    let bound = bound_readability(
        vec![ReadabilityOccurrenceV1::new(
            OCCURRENCE_A,
            SURFACE_A,
            opaque([128, 128, 128]),
            Floor::None,
        )],
        &[SURFACE_A],
    );
    let observed = observation(
        1,
        vec![SURFACE_A],
        vec![scenario(1, [(SURFACE_A, [128; 3])])],
    );
    let report = bound.recheck(observed).unwrap();
    let verdict = report.verdicts()[0];
    assert_eq!(verdict.polarity(), ReadabilityPolarityV1::Indistinct);
    assert_eq!(verdict.measurement().lc(), 0.0);
    // A decorative floor (`None`) always passes: the identity ratio clears it.
    assert!(!verdict.is_violation());
}

fn point_support_occurrence(
    source: [u8; 3],
    opacity: f64,
    criterion: PointSupportCriterionRequirementV1,
    baseline_backdrop: [u8; 3],
) -> PointSupportOccurrenceV1 {
    PointSupportOccurrenceV1::try_new(
        Srgb8::new(source),
        opacity,
        criterion,
        PointSupportStabilityPolicyV1::RetainBaselineRatioSurplus {
            baseline_backdrop: Srgb8::new(baseline_backdrop),
        },
    )
    .unwrap()
}

fn point_support_drop(value: f64) -> PointSupportDropFractionV1 {
    PointSupportDropFractionV1::try_new(value).unwrap()
}

#[test]
fn point_support_recomposes_source_alpha_over_the_current_backdrop() {
    // Baseline: white at alpha .4 over black -> #666666. If that old composite
    // were reused as an opaque source over the new white backdrop it would clear
    // 3:1. The replayable occurrence correctly recomposes to white-over-white.
    let occurrence = point_support_occurrence(
        [255; 3],
        0.4,
        PointSupportCriterionRequirementV1::Required(Wcag22CriterionV1::Sc1411UiComponentOrState),
        [0; 3],
    );
    let report = evaluate_point_support_v1(
        &[occurrence],
        &[Srgb8::new([255; 3])],
        point_support_drop(1.0),
    )
    .unwrap();

    assert_eq!(report.status(), PointSupportStatusV1::ReconcileRequired);
    assert!(report.has_required_criterion_failure());
    assert!(report.has_stability_failure());
    let cell = &report.cells()[0];
    let composition = cell.composition();
    assert_eq!(composition.subject_rgb(), [255; 3]);
    assert_eq!(composition.backdrop_rgb(), [255; 3]);
    assert_eq!(composition.output_rgb(), [255; 3]);
    assert_eq!(composition.replay(), composition.output_rgb());
    let PointSupportCriterionAssessmentV1::Required(criterion) = cell.criterion() else {
        panic!("the exact UI criterion must be evaluated")
    };
    assert_eq!(
        criterion.criterion(),
        Wcag22CriterionV1::Sc1411UiComponentOrState
    );
    assert_eq!(criterion.decision(), Wcag22ApplicableDecisionV1::Fail);
    assert_eq!(criterion.measurement().foreground, [255; 3]);
    assert_eq!(criterion.measurement().background, [255; 3]);
    let PointSupportStabilityAssessmentV1::Evaluated(retained) = cell.stability() else {
        panic!("the stability lane must retain its explicit baseline")
    };
    assert_eq!(retained.baseline_composition().output_rgb(), [0x66; 3]);
    assert_eq!(retained.anchor_ratio(), 3.0);
    assert_eq!(retained.required_surplus(), 0.0);
    assert_eq!(retained.current_surplus(), -2.0);
    assert_eq!(retained.margin(), -2.0);

    let vc = crate::spaces::vc::ViewingConditions::srgb();
    let wrong =
        crate::semantic::recheck_against_u32(0x00FF_FFFF, &[0x0066_6666], &vc).unwrap()[0].1;
    assert!(
        wrong > 3.0,
        "the old composite-as-opaque mutation must produce the opposite floor result"
    );
}

#[test]
fn point_support_drop_endpoints_keep_required_floor_independent() {
    let occurrence = point_support_occurrence(
        [0; 3],
        1.0,
        PointSupportCriterionRequirementV1::Required(Wcag22CriterionV1::Sc143TextDefault),
        [255; 3],
    );

    let retained_loss = evaluate_point_support_v1(
        &[occurrence],
        &[Srgb8::new([0xFE; 3])],
        point_support_drop(0.0),
    )
    .unwrap();
    assert!(!retained_loss.has_required_criterion_failure());
    assert!(retained_loss.has_stability_failure());

    let allowed_drop = evaluate_point_support_v1(
        &[occurrence],
        &[Srgb8::new([0x76; 3])],
        point_support_drop(1.0),
    )
    .unwrap();
    assert_eq!(allowed_drop.status(), PointSupportStatusV1::Stable);
    assert!(!allowed_drop.has_required_criterion_failure());
    assert!(!allowed_drop.has_stability_failure());

    let required_failure = evaluate_point_support_v1(
        &[occurrence],
        &[Srgb8::new([0x74; 3])],
        point_support_drop(1.0),
    )
    .unwrap();
    assert_eq!(
        required_failure.status(),
        PointSupportStatusV1::ReconcileRequired
    );
    assert!(
        required_failure.has_required_criterion_failure(),
        "dropFraction=1 must not weaken the independently required criterion"
    );
    let PointSupportCriterionAssessmentV1::Required(assessment) =
        required_failure.cells()[0].criterion()
    else {
        panic!("required criterion assessment missing")
    };
    assert_eq!(assessment.decision(), Wcag22ApplicableDecisionV1::Fail);
}

#[test]
fn point_support_worst_tie_preserves_submitted_sample_then_occurrence_order() {
    let occurrence = point_support_occurrence(
        [0; 3],
        1.0,
        PointSupportCriterionRequirementV1::Required(Wcag22CriterionV1::Sc143TextDefault),
        [255; 3],
    );
    let report = evaluate_point_support_v1(
        &[occurrence, occurrence],
        &[Srgb8::new([255; 3]), Srgb8::new([255; 3])],
        point_support_drop(0.0),
    )
    .unwrap();

    assert_eq!(report.status(), PointSupportStatusV1::Stable);
    assert_eq!(report.cells().len(), 4);
    let worst = report.minimum_stability_cell().unwrap();
    assert_eq!((worst.sample_index(), worst.occurrence_index()), (0, 0));
    assert!(report.primary_failure_cell().is_none());
}

#[test]
fn point_support_not_requested_never_fabricates_a_required_pass() {
    let occurrence = point_support_occurrence(
        [255, 0, 0],
        0.0,
        PointSupportCriterionRequirementV1::NotRequested,
        [0x12, 0x34, 0x56],
    );
    let report = evaluate_point_support_v1(
        &[occurrence],
        &[Srgb8::new([0x12, 0x34, 0x56])],
        point_support_drop(1.0),
    )
    .unwrap();

    assert_eq!(report.status(), PointSupportStatusV1::Stable);
    let cell = &report.cells()[0];
    assert_eq!(cell.composition().output_rgb(), [0x12, 0x34, 0x56]);
    assert_eq!(
        cell.criterion(),
        &PointSupportCriterionAssessmentV1::NotRequested
    );
    let PointSupportStabilityAssessmentV1::Evaluated(stability) = cell.stability() else {
        panic!("enabled stability assessment missing")
    };
    assert_eq!(stability.current_ratio(), 1.0);
}

#[test]
fn point_support_admission_fails_before_the_first_composite() {
    crate::composition::reset_source_over_evaluation_count();
    for invalid in [f64::NAN, f64::NEG_INFINITY, f64::INFINITY] {
        assert_eq!(
            PointSupportDropFractionV1::try_new(invalid),
            Err(PointSupportAdmissionErrorV1::DropFractionNonFinite)
        );
    }
    for invalid in [-f64::MIN_POSITIVE, 1.0 + f64::EPSILON] {
        assert_eq!(
            PointSupportDropFractionV1::try_new(invalid),
            Err(PointSupportAdmissionErrorV1::DropFractionOutsideUnitInterval)
        );
    }
    for invalid in [f64::NAN, f64::NEG_INFINITY, f64::INFINITY] {
        assert_eq!(
            PointSupportOccurrenceV1::try_new(
                Srgb8::new([0; 3]),
                invalid,
                PointSupportCriterionRequirementV1::NotRequested,
                PointSupportStabilityPolicyV1::Disabled,
            ),
            Err(PointSupportAdmissionErrorV1::OpacityNonFinite)
        );
    }
    for invalid in [-f64::MIN_POSITIVE, 1.0 + f64::EPSILON] {
        assert_eq!(
            PointSupportOccurrenceV1::try_new(
                Srgb8::new([0; 3]),
                invalid,
                PointSupportCriterionRequirementV1::NotRequested,
                PointSupportStabilityPolicyV1::Disabled,
            ),
            Err(PointSupportAdmissionErrorV1::OpacityOutsideUnitInterval)
        );
    }
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);

    let occurrence = point_support_occurrence(
        [0; 3],
        1.0,
        PointSupportCriterionRequirementV1::NotRequested,
        [255; 3],
    );
    crate::composition::reset_source_over_evaluation_count();
    assert_eq!(
        evaluate_point_support_v1(&[], &[Srgb8::new([255; 3])], point_support_drop(0.0)),
        Err(PointSupportEvaluationErrorV1::EmptyOccurrences)
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);

    crate::composition::reset_source_over_evaluation_count();
    assert_eq!(
        evaluate_point_support_v1(&[occurrence], &[], point_support_drop(0.0)),
        Err(PointSupportEvaluationErrorV1::EmptyBackdrops)
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
}

#[test]
fn point_support_preserves_exact_identity_for_every_three_to_one_criterion() {
    let criteria = [
        Wcag22CriterionV1::Sc143TextLargeScale,
        Wcag22CriterionV1::Sc1411UiComponentOrState,
        Wcag22CriterionV1::Sc1411GraphicalObject,
    ];
    let occurrences: Vec<_> = criteria
        .iter()
        .copied()
        .map(|criterion| {
            PointSupportOccurrenceV1::try_new(
                Srgb8::new([0; 3]),
                1.0,
                PointSupportCriterionRequirementV1::Required(criterion),
                PointSupportStabilityPolicyV1::Disabled,
            )
            .unwrap()
        })
        .collect();
    let report = evaluate_point_support_v1(
        &occurrences,
        &[Srgb8::new([255; 3])],
        point_support_drop(1.0),
    )
    .unwrap();

    assert_eq!(report.status(), PointSupportStatusV1::Stable);
    for (cell, expected) in report.cells().iter().zip(criteria) {
        let PointSupportCriterionAssessmentV1::Required(assessment) = cell.criterion() else {
            panic!("required exact criterion missing")
        };
        assert_eq!(assessment.criterion(), expected);
        assert_eq!(assessment.decision(), Wcag22ApplicableDecisionV1::Pass);
        assert_eq!(cell.composition().replay(), cell.composition().output_rgb());
    }
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

/// A readability-driven two-paint joint program whose single hard constraint
/// requires the LOWER role to clear the AA-text floor against the observed root
/// backdrop. This is the GENERIC joint program instantiated with the frozen
/// readability curve — no readability type crosses into joint.rs.
fn readability_joint_program() -> PointwiseJointPointProgramV1<DisplayReadabilityCurveV1> {
    PointwiseJointPointProgramV1::with_evaluator(
        DisplayReadabilityCurveV1,
        JROOT,
        JLOWER,
        JUPPER,
        vec![PointwiseJointHardConstraintV1::new(
            JointConstraintIdV1::new(1),
            JointVisibleTargetV1::Lower,
            Floor::AaText,
        )],
    )
    .expect("distinct paints and a unique constraint compile")
}

fn joint_lower(bytes: [u8; 3]) -> EncodedPointPaintV1 {
    encoded_paint(JLOWER, Srgb8::new(bytes), 1.0).unwrap()
}

fn joint_upper() -> EncodedPointPaintV1 {
    encoded_paint(JUPPER, Srgb8::new([255, 255, 255]), 1.0).unwrap()
}

#[test]
fn joint_recheck_flags_sample_broken_by_resolve() {
    // N1 (core-level second-solve-breaks-first): a black lower role is exactly
    // what a solve TARGETING the white sample (A) would pick. Over the WHOLE
    // observed set it clears A (contrast 21) but breaks the dark sample B
    // (#3C3C3C, contrast ≈ 1.9 < AA). The joint re-solve over every admitted
    // sample returns a TYPED Indeterminate whose breaching cell carries B's
    // provenance — even though B was never the solve target. Worst is a post-hoc
    // witness, never a solve input.
    let candidates = JointCandidateSetV1::new(vec![JointCandidateTupleV1::new(
        CandidateOrdinalV1::new(0),
        joint_lower([0, 0, 0]),
        joint_upper(),
    )])
    .unwrap();
    let observed = observation(
        1,
        vec![JROOT],
        vec![
            scenario(1, [(JROOT, [255, 255, 255])]), // A: white — the solve target.
            scenario(2, [(JROOT, [60, 60, 60])]),    // B: broken by that resolve.
        ],
    );

    let resolution = resolve_across_all_samples(
        &readability_joint_program(),
        candidates,
        observed,
        vec![CandidateOrdinalV1::new(0)],
    )
    .unwrap();

    let JointReadabilityResolutionV1::Indeterminate(report) = resolution else {
        panic!("a candidate that breaks sample B cannot be jointly feasible");
    };

    // The breaching sample is identified by its provenance — sample B (id 2).
    let breaching = report
        .cells()
        .iter()
        .find(|cell| !cell.decision().is_pass())
        .expect("the dark backdrop must break the black role");
    assert_eq!(
        report.provenance(breaching.case_index()),
        Some(&[ScenarioId::new(2)][..])
    );
    // The actual solve target (sample A, id 1) passed — a DIFFERENT sample broke.
    let passing = report
        .cells()
        .iter()
        .find(|cell| cell.decision().is_pass())
        .expect("the white target sample must pass");
    assert_eq!(
        report.provenance(passing.case_index()),
        Some(&[ScenarioId::new(1)][..])
    );
    assert_ne!(breaching.case_index(), passing.case_index());
}

#[test]
fn resolve_is_jointly_reverified() {
    // N2: two candidates over samples A (white, id 1) and B (#767676, id 2):
    //   ordinal 0 = #555555 lower — passes A (7.46) but BREAKS B (1.64);
    //   ordinal 1 = #000000 lower — passes BOTH (21 and 4.62).
    // The joint re-solve returns candidate 1, re-verified across the whole set,
    // and the set-breaching candidate 0 is NEVER returned as feasible.
    let both_samples = || {
        observation(
            2,
            vec![JROOT],
            vec![
                scenario(1, [(JROOT, [255, 255, 255])]),
                scenario(2, [(JROOT, [118, 118, 118])]),
            ],
        )
    };

    let candidates = JointCandidateSetV1::new(vec![
        JointCandidateTupleV1::new(
            CandidateOrdinalV1::new(0),
            joint_lower([85, 85, 85]),
            joint_upper(),
        ),
        JointCandidateTupleV1::new(
            CandidateOrdinalV1::new(1),
            joint_lower([0, 0, 0]),
            joint_upper(),
        ),
    ])
    .unwrap();

    let resolution = resolve_across_all_samples(
        &readability_joint_program(),
        candidates,
        both_samples(),
        vec![CandidateOrdinalV1::new(0), CandidateOrdinalV1::new(1)],
    )
    .unwrap();

    let JointReadabilityResolutionV1::Feasible(verified) = resolution else {
        panic!("the black candidate is jointly feasible across both samples");
    };
    // The set-breaching #555555 (ordinal 0) is never selected; the all-passing
    // #000000 (ordinal 1) is, and every fresh re-verify cell passes.
    assert_eq!(verified.ordinal(), CandidateOrdinalV1::new(1));
    assert_eq!(verified.fresh_cells().len(), 2);
    assert!(
        verified
            .fresh_cells()
            .iter()
            .all(|cell| cell.decision().is_pass())
    );

    // When ONLY the set-breaching candidate is offered, the resolve is a typed
    // Indeterminate — a breaching candidate is never dressed up as feasible.
    let only_breaching = JointCandidateSetV1::new(vec![JointCandidateTupleV1::new(
        CandidateOrdinalV1::new(0),
        joint_lower([85, 85, 85]),
        joint_upper(),
    )])
    .unwrap();
    let resolution = resolve_across_all_samples(
        &readability_joint_program(),
        only_breaching,
        both_samples(),
        vec![CandidateOrdinalV1::new(0)],
    )
    .unwrap();
    assert!(matches!(
        resolution,
        JointReadabilityResolutionV1::Indeterminate(_)
    ));
}
