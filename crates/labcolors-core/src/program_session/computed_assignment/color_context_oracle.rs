use super::tests::{compile_assignment, program, update};
use super::*;
use crate::color_intent::{ColorIdentityDomainV1, IdentityOptionsV1};
use crate::lcs_occurrence::{
    AdaptingLuminanceCdM2, AppearanceContextSchemaReleaseId, BackgroundLuminanceRatio,
    IEC_SRGB_D65_XYZ_FRAME_V1, SurroundProfileId,
};
use crate::observation::{
    ObservationPayloadInput, ObservedScenarioSetInput, ScenarioId, ScenarioInput,
    SurfaceInputBinding,
};

const LOWER: [[u8; 3]; 2] = [[90, 110, 130], [115, 125, 135]];
const UPPER: [[u8; 3]; 2] = [[93, 113, 133], [118, 128, 138]];
const ANCHORS: [[u8; 3]; 2] = [[121, 132, 143], [121, 132, 143]];
const BACKDROPS: [[u8; 3]; 2] = [[150, 140, 130], [151, 141, 131]];
const REQUIRED: [u8; 3] = [119, 126, 135];

// Independent finite oracle: integer source-over at alpha=1/2, separately
// transcribed published 2021 Oklab matrices, complete 64×64 source product.
// It does not call Core composition, Lab projection, score ordering or bounds.
fn half_over(source: [u8; 3], backdrop: [u8; 3]) -> [u8; 3] {
    std::array::from_fn(|i| ((u16::from(source[i]) + u16::from(backdrop[i]) + 1) / 2) as u8)
}

fn oracle_lab(rgb: [u8; 3]) -> [f64; 3] {
    let [r, g, b] = rgb.map(|value| {
        let v = f64::from(value) / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    });
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

fn distance(a: [u8; 3], b: [u8; 3]) -> f64 {
    let a = oracle_lab(a);
    let b = oracle_lab(b);
    let d = std::array::from_fn::<_, 3, _>(|i| a[i] - b[i]);
    (d[0] * d[0] + d[1] * d[1]) + d[2] * d[2]
}

fn cube(lower: [u8; 3], upper: [u8; 3]) -> Vec<[u8; 3]> {
    let mut values = Vec::new();
    for red in lower[0]..=upper[0] {
        for green in lower[1]..=upper[1] {
            for blue in lower[2]..=upper[2] {
                values.push([red, green, blue]);
            }
        }
    }
    values
}

fn oracle(intrinsic: bool) -> (usize, [[u8; 3]; 2]) {
    let mut best: Option<([f64; 2], [[u8; 3]; 2])> = None;
    let mut feasible = 0;
    for parent in cube(LOWER[0], UPPER[0]) {
        for child in cube(LOWER[1], UPPER[1]) {
            let tuple = [parent, child];
            let mut scores = [0.0_f64; 2];
            let mut valid = true;
            for backdrop in BACKDROPS {
                let visible = [
                    half_over(parent, backdrop),
                    half_over(child, half_over(parent, backdrop)),
                ];
                valid &= visible[1] == REQUIRED;
                for i in 0..2 {
                    scores[i] = scores[i].max(distance(
                        if intrinsic { tuple[i] } else { visible[i] },
                        ANCHORS[i],
                    ));
                }
            }
            if !valid {
                continue;
            }
            feasible += 1;
            if best.as_ref().is_none_or(|(old_scores, old_tuple)| {
                scores[0]
                    .total_cmp(&old_scores[0])
                    .then_with(|| scores[1].total_cmp(&old_scores[1]))
                    .then_with(|| tuple.cmp(old_tuple))
                    == Ordering::Less
            }) {
                best = Some((scores, tuple));
            }
        }
    }
    (
        feasible,
        best.expect("the independent finite fixture has a feasible tuple")
            .1,
    )
}

fn color_target(index: usize) -> Target {
    let anchor = Srgb8::new(ANCHORS[index]);
    let mut declaration = ComputedTargetV1::color(
        anchor,
        2.0,
        ColorUseIntentV1 {
            identity: ColorIdentityDomainV1::new(
                anchor,
                false,
                IdentityOptionsV1 {
                    hue_deviation: Some(180.0),
                    chroma: Some([0.0, 1.0]),
                    lightness: None,
                },
            )
            .unwrap(),
            appearance: Some(OccurrenceId::new(5 + index as u32 * 10)),
            tone: None,
        },
    );
    // A test-sized domain checks complete search against enumeration. The
    // production color constructor deliberately leaves full RGB freedom.
    declaration.domain = AssignmentDomainV1::Cube {
        lower: LOWER[index],
        upper: UPPER[index],
    };
    declaration.preference = index as u64;
    Target::new(
        TargetId::new(10 + index as u32 * 10),
        TargetIntentV1::Computed(declaration),
    )
}

fn context() -> AppearanceContextId {
    AppearanceContextId::from_inputs(
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
        IEC_SRGB_D65_XYZ_FRAME_V1,
        AdaptingLuminanceCdM2::try_new(64.0).unwrap(),
        BackgroundLuminanceRatio::try_new(0.2).unwrap(),
        SurroundProfileId::AverageV1,
    )
}

fn coupled(reverse: bool) -> Program<CoreProgramEvaluatorsV1> {
    let mut declaration = program(
        color_target(0),
        CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new(REQUIRED)),
        CoreProgramEvaluatorsV1,
    );
    declaration.targets.push(color_target(1));
    declaration.opacities = vec![OpacityInput::new(OpacityInputId::new(30), 0.5)];
    declaration.paints = vec![
        Paint::Solid {
            id: PaintId::new(3),
            target: TargetId::new(10),
        },
        Paint::Opacity {
            id: PaintId::new(40),
            source: PaintId::new(3),
            opacity: OpacityInputId::new(30),
        },
        Paint::Solid {
            id: PaintId::new(13),
            target: TargetId::new(20),
        },
        Paint::Opacity {
            id: PaintId::new(41),
            source: PaintId::new(13),
            opacity: OpacityInputId::new(30),
        },
    ];
    declaration.surfaces.push(Surface::FromOccurrence {
        id: SurfaceId::new(14),
        occurrence: OccurrenceId::new(5),
    });
    declaration.occurrences = vec![
        Occurrence::new(
            OccurrenceId::new(5),
            PaintId::new(40),
            SurfaceId::new(4),
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(),
        ),
        Occurrence::new(
            OccurrenceId::new(15),
            PaintId::new(41),
            SurfaceId::new(14),
            CompositionProfile::EncodedSrgb8SourceOverV1,
            context(),
        ),
    ];
    declaration.constraints = ConstraintSet::new(
        vec![ConstraintInvocation::visible_unary_hard(
            ConstraintId::new(6),
            OccurrenceId::new(15),
            CoreProgramConstraintInvocationV1::ExactSrgb8(Srgb8::new(REQUIRED)),
        )],
        vec![],
    );
    declaration.outputs = vec![
        OutputBinding::new(OutputSlotId::new(7), PaintId::new(40)),
        OutputBinding::new(OutputSlotId::new(17), PaintId::new(41)),
    ];
    if reverse {
        declaration.targets.reverse();
        declaration.paints.reverse();
        declaration.surfaces.reverse();
        declaration.occurrences.reverse();
        declaration.outputs.reverse();
    }
    declaration
}

#[test]
fn visible_objective_and_bounds_match_exhaustive_coupled_cube_oracle() {
    let (feasible, expected) = oracle(false);
    assert_eq!(feasible, 120);
    assert_ne!(
        expected,
        oracle(true).1,
        "the oracle distinguishes intrinsic and composed objectives"
    );
    let mut identity = None;
    for reverse in [false, true] {
        let owner = compile_assignment(coupled(reverse), 65_536).unwrap();
        if let Some(expected_identity) = identity {
            assert_eq!(owner.owner_generation.content_identity, expected_identity);
        } else {
            identity = Some(owner.owner_generation.content_identity);
        }
        let mut session = owner.instantiate(ObservationStreamId::new(8)).unwrap();
        let mut observed = update(1, 0);
        let mut cases: Vec<_> = BACKDROPS
            .into_iter()
            .enumerate()
            .map(|(index, backdrop)| ScenarioInput {
                id: ScenarioId::new(9 + index as u32),
                bindings: vec![SurfaceInputBinding::new(
                    SurfaceInputPortId::new(2),
                    ColorSignal::from_srgb8(Srgb8::new(backdrop)),
                )],
            })
            .collect();
        if reverse {
            cases.reverse();
        }
        observed.payload =
            ObservationPayloadInput::Scenarios(ObservedScenarioSetInput { scenarios: cases });
        session.prepare_update(observed).unwrap().commit();
        let result = session
            .state()
            .current_verified()
            .expect("a feasible tuple exists")
            .computed_for_test();
        assert_eq!(&*result.values, &expected.map(Srgb8::new));
        assert_eq!(result.search.completion, CompletionV1::Complete);
        assert_eq!(result.search.open_regions, 0);
        for backdrop in BACKDROPS {
            assert_eq!(
                half_over(expected[1], half_over(expected[0], backdrop)),
                REQUIRED
            );
        }
    }
}

#[test]
fn contextual_search_preserves_a_raw_computed_domain_when_its_anchor_is_outside() {
    let mut declaration = coupled(false);
    declaration.targets[0] = tests::target(10, 150, 0, 3, 0);
    let TargetIntentV1::Computed(child) = &mut declaration.targets[1].intent else {
        panic!()
    };
    child.domain = AssignmentDomainV1::Cube {
        lower: ANCHORS[1],
        upper: ANCHORS[1],
    };
    declaration.constraints = ConstraintSet::new(vec![], vec![]);
    let owner = compile_assignment(declaration, 4096).unwrap();
    let mut session = owner.instantiate(ObservationStreamId::new(8)).unwrap();
    session.prepare_update(update(1, 255)).unwrap().commit();
    let result = session
        .state()
        .current_verified()
        .unwrap()
        .computed_for_test();
    assert_eq!(result.values[0], Srgb8::new([3; 3]));
    assert_eq!(result.values[1], Srgb8::new(ANCHORS[1]));
    assert_eq!(result.search.completion, CompletionV1::Complete);
}
