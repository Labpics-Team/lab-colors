//! Независимый конечный oracle проверяет отсечения относительно машинной policy.

use super::*;
use crate::session::SessionState;
use crate::spaces::oklab::{srgb_linear_to_lms_root, srgb_linear_to_oklab, srgb8_box_oklab_bounds};
use crate::spaces::srgb::srgb_linear_from_srgb8;

fn point(rgb: [u8; 3]) -> [f64; 3] {
    srgb_linear_to_oklab(srgb_linear_from_srgb8(Srgb8::new(rgb)))
}

fn roots(rgb: [u8; 3]) -> [f64; 3] {
    srgb_linear_to_lms_root(srgb_linear_from_srgb8(Srgb8::new(rgb)))
}

fn score(value: [f64; 3], anchor: [f64; 3]) -> f64 {
    let l = value[0] - anchor[0];
    let a = value[1] - anchor[1];
    let b = value[2] - anchor[2];
    (l * l + a * a) + b * b
}

// Oracle не вызывает domain.value, split или traversal проверяемого solver.
fn visit_box(lower: [u8; 3], upper: [u8; 3], mut visit: impl FnMut([u8; 3])) {
    for r in lower[0]..=upper[0] {
        for g in lower[1]..=upper[1] {
            for b in lower[2]..=upper[2] {
                visit([r, g, b]);
            }
        }
    }
}

fn values(domain: AssignmentDomainV1) -> Vec<[u8; 3]> {
    match domain {
        AssignmentDomainV1::Gray { lower, upper } => (lower..=upper).map(|v| [v; 3]).collect(),
        AssignmentDomainV1::Cube { lower, upper } => {
            let mut result = Vec::new();
            visit_box(lower, upper, |rgb| result.push(rgb));
            result
        }
    }
}

#[test]
fn product_bounds_enclose_exhaustive_small_boxes_and_machine_scores() {
    let anchors = [
        [0, 0, 0],
        [255, 255, 255],
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [24, 24, 24],
        [128, 128, 128],
        [230, 230, 230],
        [0, 122, 255],
        [125, 124, 126],
    ]
    .map(point);
    let mut region_count = 0_u64;
    let mut point_count = 0_u64;
    let mut comparisons = 0_u64;
    let mut singletons = 0_u64;
    let mut stronger_than_lightness = 0_u64;
    let mut check = |lower, upper| {
        region_count += 1;
        let domain = AssignmentDomainV1::Cube { lower, upper };
        let bounds = srgb8_box_oklab_bounds(lower, upper);
        let lower_scores = anchors.map(|anchor| domain.lower_score(anchor));
        for (anchor, lower_score) in anchors.iter().zip(lower_scores) {
            let nearest_l = anchor[0].clamp(bounds.0[0], bounds.1[0]) - anchor[0];
            if lower_score > nearest_l * nearest_l {
                stronger_than_lightness += 1;
            }
        }
        visit_box(lower, upper, |rgb| {
            point_count += 1;
            let actual = point(rgb);
            assert_eq!(
                actual.map(f64::to_bits),
                lab(Srgb8::new(rgb)).map(f64::to_bits)
            );
            for (axis, value) in actual.iter().enumerate() {
                assert!(
                    bounds.0[axis] <= *value && *value <= bounds.1[axis],
                    "coordinate {axis}: {lower:?}..{upper:?}, {rgb:?}, {actual:?}, {bounds:?}"
                );
            }
            for (anchor, lower_score) in anchors.iter().zip(lower_scores) {
                comparisons += 1;
                let actual_score = score(actual, *anchor);
                assert_eq!(
                    actual_score.to_bits(),
                    squared_distance(actual, *anchor).to_bits()
                );
                assert!(
                    lower_score <= actual_score,
                    "false exclusion: {lower:?}..{upper:?}, {rgb:?}, {lower_score} > {actual_score}"
                );
                if lower == upper {
                    singletons += 1;
                    assert_eq!(lower_score.to_bits(), actual_score.to_bits(), "singleton");
                }
            }
        });
    };
    for origin in [0_u8, 123, 251] {
        let upper = [origin + 4; 3];
        visit_box([origin; 3], upper, |lower| {
            visit_box(lower, upper, |end| check(lower, end));
        });
    }
    for (lower, upper) in [
        ([248, 0, 0], [255, 7, 7]),
        ([0, 248, 0], [7, 255, 7]),
        ([0, 0, 248], [7, 7, 255]),
        ([120, 18, 70], [126, 24, 76]),
        ([32, 80, 128], [36, 84, 132]),
        ([20, 90, 180], [24, 94, 184]),
        ([100, 100, 100], [105, 105, 105]),
    ] {
        check(lower, upper);
    }
    assert_eq!(
        (region_count, point_count, comparisons, singletons),
        (10_132, 130_970, 1_309_700, 3_750)
    );
    assert!(
        stronger_than_lightness > 0,
        "полный bound должен использовать цветовые координаты"
    );
}

#[test]
fn actual_roots_are_finite_and_monotone_on_small_edges() {
    let mut comparisons = 0;
    for origin in [0_u8, 123, 251] {
        visit_box([origin; 3], [origin + 4; 3], |rgb| {
            let value = roots(rgb);
            assert!(value.iter().all(|x| x.is_finite()));
            for axis in 0..3 {
                if rgb[axis] == origin + 4 {
                    continue;
                }
                let mut next = rgb;
                next[axis] += 1;
                let successor = roots(next);
                for channel in 0..3 {
                    comparisons += 1;
                    assert!(
                        value[channel] <= successor[channel],
                        "G edge {rgb:?}->{next:?}, channel {channel}, {value:?}->{successor:?}"
                    );
                }
            }
        });
    }
    assert_eq!(comparisons, 2_700);
}

#[test]
fn signed_bounds_zero_and_gray_have_discriminating_cases() {
    let lower = [0; 3];
    let upper = [4; 3];
    let naive = (point(lower), point(upper));
    let bounds = srgb8_box_oklab_bounds(lower, upper);
    let mut sign_blind_witness = None;
    visit_box(lower, upper, |rgb| {
        for (axis, value) in point(rgb).iter().enumerate() {
            assert!(bounds.0[axis] <= *value && *value <= bounds.1[axis]);
            if *value < naive.0[axis] || *value > naive.1[axis] {
                sign_blind_witness = Some(rgb);
            }
        }
    });
    assert!(
        sign_blind_witness.is_some(),
        "простая оценка только RGB-концами должна обнаруживаться"
    );
    for rgb in [[0; 3], [1; 3], [128; 3], [255; 3], [125, 124, 126]] {
        let domain = AssignmentDomainV1::Cube {
            lower: rgb,
            upper: rgb,
        };
        let center = point(rgb);
        assert_eq!(domain.lower_score(center).to_bits(), 0_f64.to_bits());
        for axis in 0..3 {
            for adjacent in [center[axis].next_down(), center[axis].next_up()] {
                let mut anchor = center;
                anchor[axis] = adjacent;
                assert_eq!(
                    domain.lower_score(anchor).to_bits(),
                    score(center, anchor).to_bits()
                );
            }
        }
    }
    for (lower, upper) in [(0, 4), (0, 255), (120, 124), (251, 255)] {
        let domain = AssignmentDomainV1::Gray { lower, upper };
        for anchor in [[0; 3], [255; 3], [125, 124, 126]].map(point) {
            for gray in lower..=upper {
                assert!(domain.lower_score(anchor) <= score(point([gray; 3]), anchor));
            }
        }
    }
}

fn passes(foreground: [u8; 3], background: [u8; 3], criterion: Wcag22CriterionV1) -> bool {
    matches!(
        crate::wcag22::evaluate_wcag22_srgb8(foreground, background, criterion)
            .expect("конечная RGB8-пара допускает оценку"),
        crate::wcag22::Wcag22AssessmentV1::Evaluated {
            decision: crate::wcag22::Wcag22ApplicableDecisionV1::Pass,
            ..
        }
    )
}

fn oracle_one(
    domain: AssignmentDomainV1,
    anchor: [u8; 3],
    max_delta: f64,
    background: [u8; 3],
) -> Option<[u8; 3]> {
    let candidates = values(domain);
    let allowed = |rgb| passes(rgb, background, Wcag22CriterionV1::Sc143TextDefault);
    if candidates.contains(&anchor) && allowed(anchor) {
        return Some(anchor);
    }
    let center = point(anchor);
    candidates
        .into_iter()
        .filter(|&rgb| allowed(rgb))
        .map(|rgb| (score(point(rgb), center), rgb))
        .filter(|(distance, _)| *distance <= max_delta * max_delta)
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(_, rgb)| rgb)
}

#[test]
fn program_assignment_matches_full_cartesian_oracle_at_tolerance_boundaries() {
    let domains = [
        AssignmentDomainV1::Gray {
            lower: 115,
            upper: 121,
        },
        AssignmentDomainV1::Cube {
            lower: [116, 116, 116],
            upper: [120, 120, 120],
        },
        AssignmentDomainV1::Cube {
            lower: [24, 109, 226],
            upper: [28, 113, 230],
        },
    ];
    for domain in domains {
        for anchor in [[118; 3], [125; 3], [24, 112, 230]] {
            let nearest = values(domain)
                .into_iter()
                .map(|rgb| score(point(rgb), point(anchor)))
                .min_by(f64::total_cmp)
                .expect("корпус содержит точки")
                .sqrt();
            for max_delta in [
                0.0,
                nearest.next_down().max(0.0),
                nearest,
                nearest.next_up(),
                1.0,
            ] {
                for backdrop in [0_u8, 128, 255] {
                    let declaration =
                        ComputedTargetV1::new(domain, Srgb8::new(anchor), max_delta, 0)
                            .expect("корпус задаёт допустимую декларацию");
                    let owner = tests::compile_assignment(
                        tests::program(
                            Target::new(TargetId::new(10), TargetIntentV1::Computed(declaration)),
                            Wcag22CriterionV1::Sc143TextDefault,
                            Wcag22Srgb8V1,
                        ),
                        EXECUTION_NODE_LIMIT_V1,
                    )
                    .expect("корпус компилируется");
                    let mut session = owner
                        .instantiate(ObservationStreamId::new(8))
                        .expect("stream допускается");
                    session
                        .prepare_update(tests::update(1, backdrop))
                        .expect("наблюдение допускается")
                        .commit();
                    let expected = oracle_one(domain, anchor, max_delta, [backdrop; 3]);
                    match expected {
                        Some(rgb) => assert_eq!(
                            tests::rgb(&session).bytes(),
                            rgb,
                            "{domain:?}, anchor {anchor:?}, max_delta {max_delta}, backdrop {backdrop}"
                        ),
                        None => assert!(
                            matches!(session.state(), SessionState::Failed { .. }),
                            "пустое конечное множество должно завершиться Unsatisfied: {domain:?}, {anchor:?}, {max_delta}, {backdrop}"
                        ),
                    }
                }
            }
        }
    }
}

#[test]
fn joint_assignment_matches_independent_tuple_oracle_and_preference_order() {
    let domain = AssignmentDomainV1::Gray {
        lower: 116,
        upper: 124,
    };
    let second_domain = AssignmentDomainV1::Gray {
        lower: 218,
        upper: 230,
    };
    let anchors = [[124; 3], [218; 3]];
    let candidates = [values(domain), values(second_domain)];
    for reverse_preference in [false, true] {
        let order = if reverse_preference { [1, 0] } else { [0, 1] };
        let mut expected: Option<([f64; 2], [[u8; 3]; 2])> = None;
        for &a in &candidates[0] {
            for &b in &candidates[1] {
                if !passes(b, a, Wcag22CriterionV1::Sc143TextLargeScale) {
                    continue;
                }
                let tuple = [a, b];
                let scores = order.map(|i| score(point(tuple[i]), point(anchors[i])));
                let ordered = order.map(|i| tuple[i]);
                let better = expected.as_ref().is_none_or(|(best, rgb)| {
                    scores[0]
                        .total_cmp(&best[0])
                        .then(scores[1].total_cmp(&best[1]))
                        .then(ordered.cmp(&order.map(|i| rgb[i])))
                        == Ordering::Less
                });
                if better {
                    expected = Some((scores, tuple));
                }
            }
        }
        let (_, expected) = expected.expect("joint-корпус имеет допустимый соседний случай");
        assert!(
            !passes(
                anchors[1],
                anchors[0],
                Wcag22CriterionV1::Sc143TextLargeScale
            ),
            "authored tuple должна требовать поиск"
        );
        for reverse_declarations in [false, true] {
            let mut declaration = tests::nested(false);
            for (index, target) in declaration.targets.iter_mut().enumerate() {
                target.intent = TargetIntentV1::Computed(
                    ComputedTargetV1::new(
                        if index == 0 { domain } else { second_domain },
                        Srgb8::new(anchors[index]),
                        1.0,
                        if reverse_preference {
                            1 - index as u64
                        } else {
                            index as u64
                        },
                    )
                    .expect("joint declaration допускается"),
                );
            }
            if reverse_declarations {
                declaration.targets.reverse();
            }
            let owner = tests::compile_assignment(declaration, EXECUTION_NODE_LIMIT_V1)
                .expect("joint declaration компилируется");
            let mut session = owner
                .instantiate(ObservationStreamId::new(8))
                .expect("stream допускается");
            session
                .prepare_update(tests::update(1, 255))
                .expect("наблюдение допускается")
                .commit();
            let current = session
                .state()
                .current_verified()
                .expect("полное допустимое множество должно завершиться ready")
                .computed_for_test();
            let actual: Vec<_> = current
                .arena
                .storage
                .outputs
                .iter()
                .map(|output| output.paint.source().bytes())
                .collect();
            assert_eq!(
                actual, expected,
                "preference {reverse_preference}, declarations {reverse_declarations}"
            );
        }
    }
}

#[test]
fn unrelated_declarations_preserve_authored_dark_and_fixed_light_intents() {
    let domain = AssignmentDomainV1::Cube {
        lower: [0, 108, 240],
        upper: [0, 122, 255],
    };
    let anchor = [0, 122, 255];
    let criterion = Wcag22CriterionV1::Sc143TextDefault;
    assert!(
        !passes([255; 3], anchor, criterion),
        "цветная опора требует назначения"
    );
    let expected_surface = values(domain)
        .into_iter()
        .filter(|&background| passes([255; 3], background, criterion))
        .map(|rgb| (score(point(rgb), point(anchor)), rgb))
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .expect("допустимое соседнее значение сохраняет fixed-light")
        .1;
    for (ids, dark_anchor) in [([10_u32, 20_u32, 30_u32], 24_u8), ([944, 13, 498], 36)] {
        let dark = tests::compile_assignment(
            tests::program(
                tests::target(ids[0], dark_anchor, 0, 118, 0),
                criterion,
                Wcag22Srgb8V1,
            ),
            EXECUTION_NODE_LIMIT_V1,
        )
        .expect("независимая тёмная декларация компилируется");
        let mut session = dark
            .instantiate(ObservationStreamId::new(8))
            .expect("stream допускается");
        session
            .prepare_update(tests::update(1, 255))
            .expect("наблюдение допускается")
            .commit();
        assert_eq!(
            tests::rgb(&session).bytes(),
            [dark_anchor; 3],
            "допустимый authored primary нельзя ослаблять до порога"
        );

        let mut declaration = tests::nested(false);
        declaration.sources.push(Source::new(
            SourceId::new(ids[2]),
            ColorSignal::from_srgb8(Srgb8::new([255; 3])),
        ));
        declaration.targets = vec![
            Target::new(
                TargetId::new(ids[0]),
                TargetIntentV1::Computed(
                    ComputedTargetV1::new(domain, Srgb8::new(anchor), 1.0, 0)
                        .expect("цветная декларация допускается"),
                ),
            ),
            Target::fixed(TargetId::new(ids[1]), SourceId::new(ids[2])),
        ];
        declaration.paints = vec![
            Paint::Solid {
                id: PaintId::new(3),
                target: TargetId::new(ids[0]),
            },
            Paint::Solid {
                id: PaintId::new(13),
                target: TargetId::new(ids[1]),
            },
        ];
        declaration.constraints = ConstraintSet::new(
            vec![ConstraintInvocation::visible_unary_hard(
                ConstraintId::new(6),
                OccurrenceId::new(15),
                criterion,
            )],
            vec![],
        );
        let light = tests::compile_assignment(declaration, EXECUTION_NODE_LIMIT_V1)
            .expect("независимая светлая декларация компилируется");
        let mut session = light
            .instantiate(ObservationStreamId::new(8))
            .expect("stream допускается");
        session
            .prepare_update(tests::update(1, 255))
            .expect("наблюдение допускается")
            .commit();
        let current = session
            .state()
            .current_verified()
            .expect("допустимая совместная композиция завершается ready")
            .computed_for_test();
        let actual: Vec<_> = current
            .arena
            .storage
            .outputs
            .iter()
            .map(|output| output.paint.source().bytes())
            .collect();
        assert_eq!(
            actual,
            [expected_surface, [255; 3]],
            "fixed-light остаётся неизменным; Core назначает только объявленную переменную"
        );
    }
}

#[test]
fn report_only_and_unbounded_predicates_are_not_region_vetoes() {
    for exact in [None, Some(162_u8)] {
        let mut declaration = tests::program(
            tests::target(10, 158, 160, 164, 0),
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22Srgb8V1,
        );
        let hard = exact
            .into_iter()
            .map(|value| {
                ConstraintInvocation::exact_intrinsic_unary_hard(
                    ConstraintId::new(16),
                    TargetId::new(10),
                    Srgb8::new([value; 3]),
                )
            })
            .collect();
        declaration.constraints = ConstraintSet::new(
            hard,
            vec![ConstraintInvocation::visible_unary_report_only(
                ConstraintId::new(6),
                OccurrenceId::new(5),
                Wcag22CriterionV1::Sc143TextDefault,
            )],
        );
        let expected = exact.unwrap_or(160);
        assert!(
            !passes([expected; 3], [255; 3], Wcag22CriterionV1::Sc143TextDefault),
            "report-only обязан действительно давать нарушение"
        );
        let owner = tests::compile_assignment(declaration, EXECUTION_NODE_LIMIT_V1)
            .expect("mixed-mode декларация компилируется");
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("stream допускается");
        session
            .prepare_update(tests::update(1, 255))
            .expect("наблюдение допускается")
            .commit();
        assert_eq!(
            tests::rgb(&session).bytes(),
            [expected; 3],
            "report не запрещает результат, неподдержанное bound-ом hard проверяется в точке"
        );
    }
}

fn nested_alpha_program(targets: [Target; 2], alpha: [f64; 2]) -> Program<Wcag22Srgb8V1> {
    let mut declaration = tests::nested(false);
    declaration.targets = Vec::from(targets);
    declaration.opacities = vec![
        OpacityInput::new(OpacityInputId::new(40), alpha[0]),
        OpacityInput::new(OpacityInputId::new(50), alpha[1]),
    ];
    declaration.paints.extend([
        Paint::Opacity {
            id: PaintId::new(40),
            source: PaintId::new(3),
            opacity: OpacityInputId::new(40),
        },
        Paint::Opacity {
            id: PaintId::new(50),
            source: PaintId::new(13),
            opacity: OpacityInputId::new(50),
        },
    ]);
    declaration.occurrences[0].subject = PaintId::new(40);
    declaration.occurrences[1].subject = PaintId::new(50);
    declaration
}

#[test]
fn nested_alpha_region_search_matches_fixed_program_exhaustion() {
    let anchors = [18_u8, 218_u8];
    let near_opaque = 511.0_f64 / 512.0;
    let mut ready_cases = 0;
    let mut empty_cases = 0;
    for alpha in [
        [0.0, 0.0],
        [0.0, 1.0],
        [0.5, 0.5],
        [0.75, 0.875],
        [near_opaque, near_opaque.next_up()],
        [1.0, 1.0],
    ] {
        let mut expected: Option<([f64; 2], [u8; 2])> = None;
        // Fixed Program не имеет computed selection и не может применить region pruning.
        for first in 20..=24_u8 {
            for second in 220..=224_u8 {
                let mut fixed = nested_alpha_program(
                    [
                        Target::fixed(TargetId::new(10), SourceId::new(30)),
                        Target::fixed(TargetId::new(20), SourceId::new(40)),
                    ],
                    alpha,
                );
                fixed.sources = vec![
                    Source::new(
                        SourceId::new(30),
                        ColorSignal::from_srgb8(Srgb8::new([first; 3])),
                    ),
                    Source::new(
                        SourceId::new(40),
                        ColorSignal::from_srgb8(Srgb8::new([second; 3])),
                    ),
                ];
                let owner = fixed.compile().expect("fixed-контроль компилируется");
                let mut session = owner
                    .instantiate(ObservationStreamId::new(8))
                    .expect("stream допускается");
                session
                    .prepare_update(tests::update(1, 128))
                    .expect("наблюдение допускается")
                    .commit();
                if session.state().current_verified().is_none() {
                    assert!(matches!(session.state(), SessionState::Failed { .. }));
                    continue;
                }
                let tuple = [first, second];
                let scores = [
                    score(point([first; 3]), point([anchors[0]; 3])),
                    score(point([second; 3]), point([anchors[1]; 3])),
                ];
                if expected.as_ref().is_none_or(|(best, best_tuple)| {
                    scores[0]
                        .total_cmp(&best[0])
                        .then(scores[1].total_cmp(&best[1]))
                        .then(tuple.cmp(best_tuple))
                        == Ordering::Less
                }) {
                    expected = Some((scores, tuple));
                }
            }
        }
        let declaration = nested_alpha_program(
            [
                tests::target(10, anchors[0], 20, 24, 0),
                tests::target(20, anchors[1], 220, 224, 1),
            ],
            alpha,
        );
        let owner = tests::compile_assignment(declaration, EXECUTION_NODE_LIMIT_V1)
            .expect("вычисляемый граф компилируется");
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("stream допускается");
        session
            .prepare_update(tests::update(1, 128))
            .expect("наблюдение допускается")
            .commit();
        if let Some((_, expected)) = expected {
            ready_cases += 1;
            let current = session
                .state()
                .current_verified()
                .expect("допустимый alpha-граф даёт ready")
                .computed_for_test();
            let actual: Vec<_> = current
                .arena
                .storage
                .outputs
                .iter()
                .map(|output| output.paint.source().bytes()[0])
                .collect();
            assert_eq!(actual, expected, "alpha {alpha:?}");
        } else {
            empty_cases += 1;
            assert!(
                matches!(session.state(), SessionState::Failed { .. }),
                "alpha {alpha:?}"
            );
        }
    }
    assert!(
        ready_cases > 0 && empty_cases > 0,
        "нужны достижимые оба terminal исхода"
    );
}
