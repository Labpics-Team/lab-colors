//! Тесты точного контрфактического пересчёта точки.
//!
//! Независимый эталон ниже исполняет объявленный порядок операций binary64 через
//! точные диадические целые и округляет только на двух границах binary64. Боевой
//! композитор он не вызывает.

use proptest::prelude::*;

use crate::Srgb8;
use crate::appearance::{
    AppearanceBindings, AppearanceGraphSpec, ColorInputId, CompositionProfileV1,
    ExactFinalOwnedPointDomainV1, OccurrenceId, OccurrenceSpec, OpacityInputId, PaintId, PaintSpec,
    PointOccurrenceAbsenceReleaseV1, PointOccurrenceAbsenceReplayErrorV1,
    PointOccurrenceAbsenceStepV1, SurfaceId, SurfaceInputPortId, SurfaceSpec,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReplaySummary {
    target: OccurrenceId,
    root: OccurrenceId,
    release: PointOccurrenceAbsenceReleaseV1,
    normal_root: [u8; 3],
    counterfactual_root: [u8; 3],
    domain: ExactFinalOwnedPointDomainV1,
    steps: Vec<PointOccurrenceAbsenceStepV1>,
}

fn color(index: usize) -> ColorInputId {
    ColorInputId::new(u32::try_from(10 + index).unwrap())
}

fn opacity(index: usize) -> OpacityInputId {
    OpacityInputId::new(u32::try_from(100 + index).unwrap())
}

fn solid(index: usize) -> PaintId {
    PaintId::new(u32::try_from(200 + index * 2).unwrap())
}

fn translucent(index: usize) -> PaintId {
    PaintId::new(u32::try_from(201 + index * 2).unwrap())
}

fn occurrence(index: usize) -> OccurrenceId {
    OccurrenceId::new(u32::try_from(500 + index).unwrap())
}

fn derived_surface(index: usize) -> SurfaceId {
    SurfaceId::new(u32::try_from(400 + index).unwrap())
}

fn chain(
    layers: &[([u8; 3], f64)],
    backdrop: [u8; 3],
) -> (
    crate::appearance::CompiledAppearanceGraph,
    AppearanceBindings,
) {
    assert!(!layers.is_empty());
    let input = SurfaceInputPortId::new(300);
    let input_surface = SurfaceId::new(301);
    let mut colors = Vec::new();
    let mut opacities = Vec::new();
    let mut paints = Vec::new();
    let mut surfaces = vec![SurfaceSpec::Input {
        id: input_surface,
        port: input,
    }];
    let mut occurrences = Vec::new();
    let mut color_bindings = Vec::new();
    let mut opacity_bindings = Vec::new();

    for (index, &(source, alpha)) in layers.iter().enumerate() {
        colors.push(color(index));
        opacities.push(opacity(index));
        paints.push(PaintSpec::Solid {
            id: solid(index),
            color: color(index),
        });
        paints.push(PaintSpec::Opacity {
            id: translucent(index),
            source: solid(index),
            opacity: opacity(index),
        });
        occurrences.push(OccurrenceSpec {
            id: occurrence(index),
            subject: translucent(index),
            against: if index == 0 {
                input_surface
            } else {
                derived_surface(index - 1)
            },
            profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
        });
        surfaces.push(SurfaceSpec::FromOccurrence {
            id: derived_surface(index),
            occurrence: occurrence(index),
        });
        color_bindings.push((color(index), Srgb8::new(source)));
        opacity_bindings.push((opacity(index), alpha));
    }

    // Порядок списков в декларации не является порядком исполнения. Разворот
    // заставляет тесты пересчёта отвергать следование порядку объявления.
    paints.reverse();
    surfaces.reverse();
    occurrences.reverse();
    let graph = AppearanceGraphSpec::new(
        colors,
        vec![input],
        opacities,
        paints,
        surfaces,
        occurrences,
    )
    .compile()
    .unwrap();
    let bindings = AppearanceBindings::new(
        color_bindings,
        vec![(input, Srgb8::new(backdrop))],
        opacity_bindings,
    );
    (graph, bindings)
}

fn replay(layers: &[([u8; 3], f64)], backdrop: [u8; 3], target_index: usize) -> ReplaySummary {
    let (graph, bindings) = chain(layers, backdrop);
    let root_id = occurrence(layers.len() - 1);
    let target_id = occurrence(target_index);
    let root = graph.compile_point_presentation_root(root_id).unwrap();
    let path = graph
        .compile_point_presentation_path(target_id, &root)
        .unwrap();
    let admitted = graph.admit_bindings(&bindings).unwrap();
    let mut workspace = graph.new_workspace().unwrap();
    let evaluation = graph
        .evaluate_admitted_into(&admitted, &mut workspace)
        .unwrap();
    let mut steps = Vec::with_capacity(path.len());
    let result = evaluation
        .replay_point_occurrence_absence_into(
            &path,
            PointOccurrenceAbsenceReleaseV1::BypassOwnBackdropV1,
            &mut steps,
        )
        .unwrap();
    ReplaySummary {
        target: result.target(),
        root: result.root(),
        release: result.release(),
        normal_root: result.normal_root(),
        counterfactual_root: result.counterfactual_root(),
        domain: result.domain(),
        steps: result.steps().to_vec(),
    }
}

fn exact_source_over_oracle(tint: [u8; 3], alpha: f64, backdrop: [u8; 3]) -> [u8; 3] {
    let (alpha_numerator, alpha_denominator_shift) = exact_binary64_dyadic(alpha);
    std::array::from_fn(|channel| {
        let backdrop = i128::from(backdrop[channel]);
        let delta = i128::from(tint[channel]) - backdrop;
        let rounded_product =
            round_dyadic_to_binary64(delta * alpha_numerator, alpha_denominator_shift);
        let (product_numerator, product_denominator_shift) = exact_binary64_dyadic(rounded_product);
        let exact_sum_numerator = (backdrop << product_denominator_shift) + product_numerator;
        let rounded_sum = round_dyadic_to_binary64(exact_sum_numerator, product_denominator_shift);
        let (sum_numerator, sum_denominator_shift) = exact_binary64_dyadic(rounded_sum);
        let denominator = 1_i128 << sum_denominator_shift;
        u8::try_from((sum_numerator + denominator / 2) / denominator).unwrap()
    })
}

/// Декодировать конечные binary64 ограниченного эталона в точные знаковые
/// диадические числа. Корпус свойств удерживает операнды в нормальном диапазоне.
fn exact_binary64_dyadic(value: f64) -> (i128, u32) {
    assert!(value.is_finite());
    if value == 0.0 {
        return (0, 0);
    }
    let bits = value.to_bits();
    let negative = bits >> 63 != 0;
    let exponent = i32::try_from((bits >> 52) & 0x7ff).unwrap();
    assert_ne!(
        exponent, 0,
        "корпус эталона исключает субнормальные операнды"
    );
    let mut mantissa = i128::from((bits & ((1_u64 << 52) - 1)) | (1_u64 << 52));
    if negative {
        mantissa = -mantissa;
    }
    let binary_exponent = exponent - 1023 - 52;
    if binary_exponent >= 0 {
        (mantissa << u32::try_from(binary_exponent).unwrap(), 0)
    } else {
        let denominator_shift = u32::try_from(-binary_exponent).unwrap();
        // Потребитель сдвигает 8-битную подложку на тот же порядок. При 119
        // старший бит занимает позицию 126, а оставшийся запас больше 53-битной
        // мантиссы слагаемого; 120 уже затронул бы знаковый бит i128.
        assert!(
            denominator_shift < 120,
            "диадическое число эталона должно помещаться в i128"
        );
        (mantissa, denominator_shift)
    }
}

/// Округлить ограниченное точное диадическое число до binary64 целочисленным
/// методом до ближайшего с выбором чётного при равенстве.
fn round_dyadic_to_binary64(numerator: i128, denominator_shift: u32) -> f64 {
    if numerator == 0 {
        return 0.0;
    }
    let negative = numerator < 0;
    let magnitude = numerator.unsigned_abs();
    let bit_len = 128_u32 - magnitude.leading_zeros();
    let mut discarded = bit_len.saturating_sub(53);
    let mut significand = magnitude >> discarded;
    if discarded != 0 {
        let remainder_mask = (1_u128 << discarded) - 1;
        let remainder = magnitude & remainder_mask;
        let half = 1_u128 << (discarded - 1);
        if remainder > half || (remainder == half && significand & 1 == 1) {
            significand += 1;
        }
    }
    if significand == 1_u128 << 53 {
        significand >>= 1;
        discarded += 1;
    }
    let significand_bits = 128_u32 - significand.leading_zeros();
    let value_power = i32::try_from(discarded).unwrap() - i32::try_from(denominator_shift).unwrap();
    let unbiased_exponent = value_power + i32::try_from(significand_bits).unwrap() - 1;
    assert!((-1022..=1023).contains(&unbiased_exponent));
    let normalized = significand << (53 - significand_bits);
    let fraction = u64::try_from(normalized - (1_u128 << 52)).unwrap();
    let exponent = u64::try_from(unbiased_exponent + 1023).unwrap();
    let sign = u64::from(negative) << 63;
    f64::from_bits(sign | (exponent << 52) | fraction)
}

fn normal_oracle(layers: &[([u8; 3], f64)], backdrop: [u8; 3]) -> Vec<[u8; 3]> {
    let mut outputs = Vec::with_capacity(layers.len());
    let mut current = backdrop;
    for &(tint, alpha) in layers {
        current = exact_source_over_oracle(tint, alpha, current);
        outputs.push(current);
    }
    outputs
}

fn absent_oracle(layers: &[([u8; 3], f64)], backdrop: [u8; 3], target_index: usize) -> [u8; 3] {
    let normal = normal_oracle(layers, backdrop);
    let mut current = if target_index == 0 {
        backdrop
    } else {
        normal[target_index - 1]
    };
    for &(tint, alpha) in &layers[target_index + 1..] {
        current = exact_source_over_oracle(tint, alpha, current);
    }
    current
}

#[test]
fn red_replay_removes_target_then_replays_the_same_downstream_order() {
    let layers = [
        ([230, 20, 90], 0.37),
        ([10, 210, 40], 0.61),
        ([40, 70, 250], 0.43),
    ];
    let backdrop = [248, 241, 229];
    let result = replay(&layers, backdrop, 0);
    let normal = normal_oracle(&layers, backdrop);
    let absent = absent_oracle(&layers, backdrop, 0);

    assert_eq!(result.target, occurrence(0));
    assert_eq!(result.root, occurrence(2));
    assert_eq!(
        result.release,
        PointOccurrenceAbsenceReleaseV1::BypassOwnBackdropV1
    );
    assert_eq!(result.normal_root, normal[2]);
    assert_eq!(result.counterfactual_root, absent);
    assert_eq!(result.steps.len(), 3);
    assert_eq!(result.steps[0].occurrence(), occurrence(0));
    assert_eq!(result.steps[0].counterfactual_output(), backdrop);
    for (index, expected_normal) in normal.iter().copied().enumerate().skip(1) {
        assert_eq!(result.steps[index].occurrence(), occurrence(index));
        assert_eq!(result.steps[index].normal().output_rgb(), expected_normal);
    }
    assert_ne!(
        absent,
        exact_source_over_oracle(
            layers[1].0,
            layers[1].1,
            exact_source_over_oracle(layers[2].0, layers[2].1, backdrop)
        ),
        "пример обязан обнаруживать обратный порядок оставшегося пересчёта"
    );
}

#[test]
fn exact_owned_domain_is_empty_when_downstream_quantization_erases_the_target() {
    let layers = [([0; 3], 0.01), ([0; 3], 0.95)];
    let result = replay(&layers, [255; 3], 0);
    let normal = normal_oracle(&layers, [255; 3]);
    assert_ne!(
        normal[0], [255; 3],
        "цель обязана изменить собственное наложение"
    );
    assert_eq!(normal[1], absent_oracle(&layers, [255; 3], 0));
    assert_eq!(result.domain, ExactFinalOwnedPointDomainV1::Empty);
}

#[test]
fn exact_owned_domain_is_singleton_only_for_a_final_byte_contribution() {
    let layers = [([0, 80, 240], 0.5)];
    let result = replay(&layers, [255, 240, 220], 0);
    assert_ne!(result.normal_root, result.counterfactual_root);
    assert_eq!(
        result.domain,
        ExactFinalOwnedPointDomainV1::Singleton {
            visible: result.normal_root,
        }
    );
}

#[test]
fn alpha_endpoints_and_a_later_opaque_layer_obey_the_same_domain_law() {
    let backdrop = [31, 47, 89];
    let absent = replay(&[([240, 30, 10], 0.0)], backdrop, 0);
    assert_eq!(absent.normal_root, backdrop);
    assert_eq!(absent.domain, ExactFinalOwnedPointDomainV1::Empty);

    let present = replay(&[([240, 30, 10], 1.0)], backdrop, 0);
    assert_eq!(present.normal_root, [240, 30, 10]);
    assert_eq!(
        present.domain,
        ExactFinalOwnedPointDomainV1::Singleton {
            visible: [240, 30, 10],
        }
    );

    let erased = replay(&[([240, 30, 10], 1.0), ([5, 200, 90], 1.0)], backdrop, 0);
    assert_eq!(erased.normal_root, [5, 200, 90]);
    assert_eq!(erased.counterfactual_root, [5, 200, 90]);
    assert_eq!(erased.domain, ExactFinalOwnedPointDomainV1::Empty);
}

#[test]
fn final_domain_is_scoped_to_one_selected_root_in_a_fanout_graph() {
    let input = SurfaceInputPortId::new(300);
    let input_surface = SurfaceId::new(301);
    let shared_surface = derived_surface(0);
    let mut paints = Vec::new();
    for index in 0..3 {
        paints.push(PaintSpec::Solid {
            id: solid(index),
            color: color(index),
        });
        paints.push(PaintSpec::Opacity {
            id: translucent(index),
            source: solid(index),
            opacity: opacity(index),
        });
    }
    paints.reverse();
    let graph = AppearanceGraphSpec::new(
        (0..3).map(color).collect(),
        vec![input],
        (0..3).map(opacity).collect(),
        paints,
        vec![
            SurfaceSpec::FromOccurrence {
                id: shared_surface,
                occurrence: occurrence(0),
            },
            SurfaceSpec::Input {
                id: input_surface,
                port: input,
            },
        ],
        vec![
            OccurrenceSpec {
                id: occurrence(2),
                subject: translucent(2),
                against: shared_surface,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            },
            OccurrenceSpec {
                id: occurrence(0),
                subject: translucent(0),
                against: input_surface,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            },
            OccurrenceSpec {
                id: occurrence(1),
                subject: translucent(1),
                against: shared_surface,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            },
        ],
    )
    .compile()
    .unwrap();
    let bindings = AppearanceBindings::new(
        vec![
            (color(0), Srgb8::new([240, 30, 10])),
            (color(1), Srgb8::new([5, 200, 90])),
            (color(2), Srgb8::new([220, 180, 70])),
        ],
        vec![(input, Srgb8::new([31, 47, 89]))],
        vec![(opacity(0), 0.5), (opacity(1), 1.0), (opacity(2), 0.25)],
    );
    let admitted = graph.admit_bindings(&bindings).unwrap();
    let mut workspace = graph.new_workspace().unwrap();
    let evaluation = graph
        .evaluate_admitted_into(&admitted, &mut workspace)
        .unwrap();

    let opaque_root = graph
        .compile_point_presentation_root(occurrence(1))
        .unwrap();
    let opaque_path = graph
        .compile_point_presentation_path(occurrence(0), &opaque_root)
        .unwrap();
    let mut opaque_steps = Vec::with_capacity(opaque_path.len());
    let opaque = evaluation
        .replay_point_occurrence_absence_into(
            &opaque_path,
            PointOccurrenceAbsenceReleaseV1::BypassOwnBackdropV1,
            &mut opaque_steps,
        )
        .unwrap();
    assert_eq!(opaque.root(), occurrence(1));
    assert_eq!(opaque.domain(), ExactFinalOwnedPointDomainV1::Empty);
    assert_eq!(
        opaque
            .steps()
            .iter()
            .map(|step| step.occurrence())
            .collect::<Vec<_>>(),
        vec![occurrence(0), occurrence(1)]
    );

    let translucent_root = graph
        .compile_point_presentation_root(occurrence(2))
        .unwrap();
    let translucent_path = graph
        .compile_point_presentation_path(occurrence(0), &translucent_root)
        .unwrap();
    let mut translucent_steps = Vec::with_capacity(translucent_path.len());
    let translucent = evaluation
        .replay_point_occurrence_absence_into(
            &translucent_path,
            PointOccurrenceAbsenceReleaseV1::BypassOwnBackdropV1,
            &mut translucent_steps,
        )
        .unwrap();
    assert_eq!(translucent.root(), occurrence(2));
    assert_ne!(translucent.normal_root(), translucent.counterfactual_root());
    assert_eq!(
        translucent.domain(),
        ExactFinalOwnedPointDomainV1::Singleton {
            visible: translucent.normal_root(),
        }
    );
    assert_eq!(
        translucent
            .steps()
            .iter()
            .map(|step| step.occurrence())
            .collect::<Vec<_>>(),
        vec![occurrence(0), occurrence(2)]
    );
}

#[test]
fn replay_rejects_foreign_evaluation_before_touching_the_replay_buffer() {
    let layers = [([10, 20, 30], 0.5), ([220, 180, 70], 0.25)];
    let (origin, _) = chain(&layers, [255; 3]);
    let root = origin
        .compile_point_presentation_root(occurrence(1))
        .unwrap();
    let path = origin
        .compile_point_presentation_path(occurrence(0), &root)
        .unwrap();
    let (foreign, bindings) = chain(&layers, [255; 3]);
    let admitted = foreign.admit_bindings(&bindings).unwrap();
    let mut workspace = foreign.new_workspace().unwrap();
    let evaluation = foreign
        .evaluate_admitted_into(&admitted, &mut workspace)
        .unwrap();
    let mut steps = Vec::with_capacity(path.len());

    crate::composition::reset_source_over_evaluation_count();
    assert_eq!(
        evaluation.replay_point_occurrence_absence_into(
            &path,
            PointOccurrenceAbsenceReleaseV1::BypassOwnBackdropV1,
            &mut steps,
        ),
        Err(PointOccurrenceAbsenceReplayErrorV1::IncompatibleEvaluation)
    );
    assert!(steps.is_empty());
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
}

#[test]
fn replay_preflights_capacity_and_returns_only_its_appended_range() {
    let layers = [([10, 20, 30], 0.5), ([220, 180, 70], 0.25)];
    let (graph, bindings) = chain(&layers, [255; 3]);
    let root = graph
        .compile_point_presentation_root(occurrence(1))
        .unwrap();
    let path = graph
        .compile_point_presentation_path(occurrence(0), &root)
        .unwrap();
    let admitted = graph.admit_bindings(&bindings).unwrap();
    let mut workspace = graph.new_workspace().unwrap();
    let evaluation = graph
        .evaluate_admitted_into(&admitted, &mut workspace)
        .unwrap();
    let sentinel = *evaluation.occurrence(occurrence(0)).unwrap().certificate();
    let mut short = Vec::with_capacity(path.len());
    short.push(PointOccurrenceAbsenceStepV1::Removed {
        occurrence: OccurrenceId::new(999),
        normal: sentinel,
    });

    crate::composition::reset_source_over_evaluation_count();
    assert_eq!(
        evaluation.replay_point_occurrence_absence_into(
            &path,
            PointOccurrenceAbsenceReleaseV1::BypassOwnBackdropV1,
            &mut short,
        ),
        Err(PointOccurrenceAbsenceReplayErrorV1::InsufficientCapacity)
    );
    assert_eq!(short.len(), 1);
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);

    let mut exact = Vec::with_capacity(path.len() + 1);
    exact.push(PointOccurrenceAbsenceStepV1::Removed {
        occurrence: OccurrenceId::new(999),
        normal: sentinel,
    });
    let storage = (exact.as_ptr(), exact.capacity());
    let (summary, allocations) = crate::test_support::measured_allocations(|| {
        let result = evaluation
            .replay_point_occurrence_absence_into(
                &path,
                PointOccurrenceAbsenceReleaseV1::BypassOwnBackdropV1,
                &mut exact,
            )
            .unwrap();
        (
            result.steps().len(),
            result.steps()[0].occurrence(),
            result.steps()[1].occurrence(),
        )
    });
    assert_eq!(summary, (path.len(), occurrence(0), occurrence(1)));
    assert_eq!(allocations, 0);
    assert_eq!(
        crate::composition::source_over_evaluation_count(),
        path.len() - 1
    );
    assert_eq!((exact.as_ptr(), exact.capacity()), storage);
}

proptest! {
    // Зафиксированный O1a-B CI-ратчет: 2^11 случаев не даёт изменению default
    // `proptest` молча ослабить корпус; другой бюджет требует явного решения.
    #![proptest_config(ProptestConfig::with_cases(2_048))]

    #[test]
    fn replay_matches_an_independent_encoded_srgb8_oracle(
        sources in prop::array::uniform4(any::<[u8; 3]>()),
        backdrop in any::<[u8; 3]>(),
        alpha_numerators in prop::array::uniform4(0_u16..=u16::MAX),
        target_index in 0_usize..4,
    ) {
        let alphas = alpha_numerators.map(|value| f64::from(value) / f64::from(u16::MAX));
        let layers = std::array::from_fn::<_, 4, _>(|index| (sources[index], alphas[index]));
        let expected_normal = normal_oracle(&layers, backdrop);
        let expected_absent = absent_oracle(&layers, backdrop, target_index);
        let result = replay(&layers, backdrop, target_index);

        prop_assert_eq!(result.normal_root, expected_normal[3]);
        prop_assert_eq!(result.counterfactual_root, expected_absent);
        prop_assert_eq!(result.steps.len(), 4 - target_index);
        let expected_domain = if expected_normal[3] == expected_absent {
            ExactFinalOwnedPointDomainV1::Empty
        } else {
            ExactFinalOwnedPointDomainV1::Singleton {
                visible: expected_normal[3],
            }
        };
        prop_assert_eq!(result.domain, expected_domain);
    }
}
