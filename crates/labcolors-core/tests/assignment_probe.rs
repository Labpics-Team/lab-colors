//! Общая математическая проба назначения; этот модуль не является public API.

#[path = "assignment/experiment.rs"]
mod experiment;
#[path = "assignment/oracle.rs"]
mod oracle;

use experiment::{Constraint, Domain, Floor, Outcome, Problem, Rgb, Source, Target};
use std::time::Instant;

fn gray_target(id: &'static str, anchor: u8, lower: u8, upper: u8) -> Target {
    Target {
        id,
        anchor: [anchor; 3],
        domain: Domain::Gray { lower, upper },
        // Синтетическая политика этой серии; не эстетический default Core.
        max_delta: 0.3,
    }
}

fn contrast(lighter: Source, darker: Source, floor: Floor) -> Constraint {
    Constraint::Contrast {
        lighter,
        darker,
        floor,
    }
}

fn against_light(anchor: u8, lower: u8, upper: u8, observed: u8) -> Problem {
    Problem {
        targets: vec![gray_target("opaque-a", anchor, lower, upper)],
        constraints: vec![contrast(
            Source::Fixed([observed; 3]),
            Source::Target(0),
            Floor::FourAndHalf,
        )],
    }
}

fn assigned(outcome: Outcome) -> Vec<Rgb> {
    match outcome {
        Outcome::Assigned(values) => values,
        other => panic!("ожидалось полное назначение, получено {other:?}"),
    }
}

fn complete_matches_oracle(problem: &Problem) -> Vec<Rgb> {
    let full = experiment::solve(problem, false, u64::MAX);
    let pruned = experiment::solve(problem, true, u64::MAX);
    let reference = oracle::solve(problem);
    assert_eq!(full.outcome, pruned.outcome);
    let values = assigned(full.outcome);
    assert_eq!(reference.best.as_ref(), Some(&values));
    assert!(reference.feasible > 0);
    assert!(reference.visited >= reference.feasible);
    assert!(experiment::admissible(problem, &values));
    values
}

#[test]
fn feasible_anchor_stays_exact_instead_of_maximizing_contrast() {
    let problem = against_light(90, 0, 120, 255);
    let result = complete_matches_oracle(&problem);
    assert_eq!(result, vec![[90; 3]]);
    // Контрпример максимизатору: более тёмный цвет также допустим, но авторская
    // опора предпочтительна при уже выполненном минимальном контрасте.
    assert!(experiment::admissible(&problem, &[[60; 3]]));
    assert_eq!(experiment::solve(&problem, true, 1).examined, 1);
}

#[test]
fn zero_anchor_tolerance_rejects_an_otherwise_acceptable_replacement() {
    // 119 не проходит контраст с белым при 4,5:1; 118 проходит, но
    // не совпадает с авторской опорой. Ноль запрещает даже такую замену.
    let mut exact = against_light(119, 118, 118, 255);
    exact.targets[0].max_delta = 0.0;
    let mut relaxed = exact.clone();
    relaxed.targets[0].max_delta = 0.3;

    assert!(!experiment::admissible(&exact, &[[118; 3]]));
    assert!(experiment::admissible(&relaxed, &[[118; 3]]));
    for pruning in [false, true] {
        assert_eq!(
            experiment::solve(&exact, pruning, u64::MAX).outcome,
            Outcome::Unsatisfied
        );
        assert_eq!(
            experiment::solve(&relaxed, pruning, u64::MAX).outcome,
            Outcome::Assigned(vec![[118; 3]])
        );
    }
    assert_eq!(oracle::solve(&exact).best, None);
    assert_eq!(oracle::solve(&relaxed).best, Some(vec![[118; 3]]));
}

#[test]
fn gap_constraint_rejects_an_otherwise_acceptable_anchor() {
    // Изолируем Gap: нулевое расстояние между равными цветами меньше
    // положительного требования. Без Gap та же опора допустима.
    let accepted = Problem {
        targets: vec![gray_target("gap-fixture", 90, 90, 90)],
        constraints: vec![],
    };
    let mut rejected = accepted.clone();
    rejected.constraints.push(Constraint::Gap {
        lighter: Source::Target(0),
        darker: Source::Fixed([90; 3]),
        minimum_q55: 1 << 50,
    });

    assert!(experiment::admissible(&accepted, &[[90; 3]]));
    assert!(!experiment::admissible(&rejected, &[[90; 3]]));
    for pruning in [false, true] {
        assert_eq!(
            experiment::solve(&accepted, pruning, u64::MAX).outcome,
            Outcome::Assigned(vec![[90; 3]])
        );
        assert_eq!(
            experiment::solve(&rejected, pruning, u64::MAX).outcome,
            Outcome::Unsatisfied
        );
    }
    assert_eq!(oracle::solve(&accepted).best, Some(vec![[90; 3]]));
    assert_eq!(oracle::solve(&rejected).best, None);
}

#[test]
fn fixed_strong_source_is_not_replaced_when_another_target_is_repaired() {
    let mut problem = against_light(180, 90, 180, 255);
    problem.constraints.push(Constraint::Gap {
        lighter: Source::Target(0),
        darker: Source::Fixed([16; 3]),
        minimum_q55: 1 << 50,
    });
    let result = complete_matches_oracle(&problem);
    assert_ne!(result[0], problem.targets[0].anchor);
    assert_eq!(result[0], [118; 3]);
    assert!(matches!(
        problem.constraints[1],
        Constraint::Gap {
            darker: Source::Fixed([16, 16, 16]),
            ..
        }
    ));
}

#[test]
fn assignable_primary_anchor_survives_secondary_repair() {
    let problem = Problem {
        targets: vec![
            gray_target("opaque-a", 16, 0, 80),
            gray_target("opaque-b", 180, 90, 180),
        ],
        constraints: vec![
            contrast(
                Source::Fixed([255; 3]),
                Source::Target(0),
                Floor::FourAndHalf,
            ),
            contrast(
                Source::Fixed([255; 3]),
                Source::Target(1),
                Floor::FourAndHalf,
            ),
            Constraint::Gap {
                lighter: Source::Target(1),
                darker: Source::Target(0),
                minimum_q55: 1 << 50,
            },
        ],
    };
    assert!(!experiment::admissible(&problem, &[[16; 3], [180; 3]]));
    let result = complete_matches_oracle(&problem);
    assert_eq!(result, vec![[16; 3], [118; 3]]);
}

#[test]
fn opposite_polarity_consumer_preserves_bright_primary_and_repairs_weaker_secondary() {
    let problem = Problem {
        targets: vec![
            gray_target("consumer-two/x", 240, 200, 255),
            gray_target("consumer-two/y", 80, 70, 180),
        ],
        constraints: vec![
            contrast(
                Source::Target(0),
                Source::Fixed([16; 3]),
                Floor::FourAndHalf,
            ),
            contrast(
                Source::Target(1),
                Source::Fixed([16; 3]),
                Floor::FourAndHalf,
            ),
            Constraint::Gap {
                lighter: Source::Target(0),
                darker: Source::Target(1),
                minimum_q55: 1 << 50,
            },
        ],
    };
    // На этой серой оси все значения выше перехода sRGB 0.04045.
    // Независимая формула WCAG даёт нижнюю границу 123.0713... байта,
    // поэтому первый допустимый encoded secondary равен 124, а не 123.
    let linear = |value: u8| ((f64::from(value) / 255.0 + 0.055) / 1.055).powf(2.4);
    let minimum_linear = 4.5 * (linear(16) + 0.05) - 0.05;
    let encoded_boundary = 255.0 * (1.055 * minimum_linear.powf(1.0 / 2.4) - 0.055);
    assert!(encoded_boundary > 123.0 && encoded_boundary < 124.0);
    let secondary = encoded_boundary.ceil() as u8;

    assert!((linear(240) + 0.05) / (linear(16) + 0.05) > 4.5);
    assert!((linear(80) + 0.05) / (linear(16) + 0.05) < 4.5);
    assert!(linear(240) - linear(80) > 1.0 / 32.0);
    assert!(!experiment::admissible(&problem, &[[240; 3], [80; 3]]));
    assert!(!experiment::admissible(
        &problem,
        &[[240; 3], [secondary - 1; 3]]
    ));
    assert!(experiment::admissible(
        &problem,
        &[[240; 3], [secondary; 3]]
    ));
    let result = complete_matches_oracle(&problem);
    assert_eq!(result, vec![[240; 3], [secondary; 3]]);
    assert_eq!(result[0], problem.targets[0].anchor);
    assert_ne!(result[1], problem.targets[1].anchor);
    assert!(linear(result[0][0]) - linear(result[1][0]) >= 1.0 / 32.0);
    assert!(experiment::admissible(&problem, &[[240; 3], [140; 3]]));
}

#[test]
fn joint_assignment_repairs_a_pair_that_independent_repairs_break() {
    let problem = Problem {
        targets: vec![
            gray_target("opaque-a", 80, 0, 90),
            gray_target("opaque-b", 170, 100, 190),
        ],
        constraints: vec![
            contrast(Source::Target(1), Source::Target(0), Floor::Three),
            contrast(
                Source::Fixed([255; 3]),
                Source::Target(1),
                Floor::FourAndHalf,
            ),
        ],
    };
    let independent_first = Problem {
        targets: vec![problem.targets[0].clone()],
        constraints: vec![contrast(
            Source::Fixed([170; 3]),
            Source::Target(0),
            Floor::Three,
        )],
    };
    let independent_second = against_light(170, 100, 190, 255);
    let sequential = vec![
        complete_matches_oracle(&independent_first)[0],
        complete_matches_oracle(&independent_second)[0],
    ];
    assert_eq!(sequential, vec![[80; 3], [118; 3]]);
    assert!(!experiment::admissible(&problem, &sequential));
    let result = complete_matches_oracle(&problem);
    assert_ne!(result[0], sequential[0]);
    assert_ne!(result[1], problem.targets[1].anchor);
}

#[test]
fn final_encoded_color_is_rechecked_at_the_quantization_boundary() {
    let linear = 1.05_f64 / 4.5 - 0.05;
    let encoded = 255.0 * (1.055 * linear.powf(1.0 / 2.4) - 0.055);
    assert!(encoded > 118.5 && encoded < 119.0);
    let nearest = encoded.round() as u8;
    let problem = against_light(nearest, 118, 119, 255);
    assert_eq!(nearest, 119);
    assert!(!experiment::admissible(&problem, &[[nearest; 3]]));
    assert_eq!(complete_matches_oracle(&problem), vec![[118; 3]]);
}

#[test]
fn empty_feasible_set_is_proved_without_relaxing_any_constraint() {
    let problem = against_light(150, 130, 160, 255);
    let full = experiment::solve(&problem, false, u64::MAX);
    let pruned = experiment::solve(&problem, true, u64::MAX);
    let reference = oracle::solve(&problem);
    assert_eq!(full.outcome, Outcome::Unsatisfied);
    assert_eq!(pruned.outcome, Outcome::Unsatisfied);
    assert_eq!(full.examined, 32); // 31 значений и отдельная проверка опоры.
    assert_eq!(pruned.pruned, 31);
    assert_eq!(reference.visited, 31);
    assert_eq!(reference.feasible, 0);
    assert_eq!(reference.best, None);
}

#[test]
fn exhausted_budget_reports_incompleteness_even_with_a_feasible_incumbent() {
    let problem = against_light(130, 80, 130, 255);
    for budget in [0, 1] {
        let run = experiment::solve(&problem, false, budget);
        assert_eq!(run.examined, budget);
        assert_eq!(run.outcome, Outcome::Unresolved { incumbent: None });
    }
    let run = experiment::solve(&problem, false, 2);
    assert_eq!(run.examined, 2);
    match run.outcome {
        Outcome::Unresolved {
            incumbent: Some(values),
        } => {
            assert!(experiment::admissible(&problem, &values));
        }
        other => panic!("допустимый кандидат ещё не доказывает лучший результат: {other:?}"),
    }
    assert_eq!(complete_matches_oracle(&problem), vec![[118; 3]]);
}

#[test]
fn changed_observed_source_requires_a_new_valid_assignment() {
    let before = against_light(160, 0, 180, 255);
    let after = against_light(160, 0, 180, 220);
    let old = complete_matches_oracle(&before);
    assert!(!experiment::admissible(&after, &old));
    let new = complete_matches_oracle(&after);
    assert_ne!(old, new);
}

#[test]
fn opaque_id_renaming_cannot_change_a_unique_preferred_assignment() {
    let original = against_light(150, 80, 160, 255);
    let mut renamed = original.clone();
    renamed.targets[0].id = "unrelated-consumer/任意";
    assert_eq!(
        complete_matches_oracle(&original),
        complete_matches_oracle(&renamed)
    );
}

#[test]
fn prefix_scores_survive_changed_prefixes_and_pruned_branches() {
    for (primary, secondary, fixed, primary_is_lighter) in [
        (gray_target("a", 20, 16, 18), [170; 3], [255; 3], false),
        (gray_target("a", 230, 232, 234), [80; 3], [16; 3], true),
    ] {
        let (lower, upper) = if primary_is_lighter {
            (123, 126)
        } else {
            (116, 119)
        };
        let (lighter, darker) = if primary_is_lighter {
            (Source::Target(1), Source::Fixed(fixed))
        } else {
            (Source::Fixed(fixed), Source::Target(1))
        };
        let (gap_lighter, gap_darker) = if primary_is_lighter {
            (Source::Target(0), Source::Target(1))
        } else {
            (Source::Target(1), Source::Target(0))
        };
        let problem = Problem {
            targets: vec![
                primary,
                Target {
                    id: "b",
                    anchor: secondary,
                    domain: Domain::Gray { lower, upper },
                    max_delta: 0.3,
                },
            ],
            constraints: vec![
                contrast(lighter, darker, Floor::FourAndHalf),
                Constraint::Gap {
                    lighter: gap_lighter,
                    darker: gap_darker,
                    minimum_q55: 1 << 50,
                },
            ],
        };
        let values = complete_matches_oracle(&problem);
        assert_ne!(values[0], problem.targets[0].anchor);
        assert_ne!(values[1], problem.targets[1].anchor);
    }
}

#[test]
fn changing_assignment_invalidates_gap_luminance_bounds() {
    let problem = Problem {
        targets: vec![gray_target("a", 88, 80, 84), gray_target("b", 80, 80, 85)],
        constraints: vec![Constraint::Gap {
            lighter: Source::Target(1),
            darker: Source::Target(0),
            minimum_q55: 1,
        }],
    };
    let values = complete_matches_oracle(&problem);
    assert_eq!(values, vec![[84; 3], [85; 3]]);
}

fn full_color_problem(
    id: &'static str,
    anchor: Rgb,
    fixed: Rgb,
    fixed_is_lighter: bool,
) -> Problem {
    let (lighter, darker) = if fixed_is_lighter {
        (Source::Fixed(fixed), Source::Target(0))
    } else {
        (Source::Target(0), Source::Fixed(fixed))
    };
    Problem {
        targets: vec![Target {
            id,
            anchor,
            domain: Domain::full(),
            max_delta: 0.10,
        }],
        constraints: vec![contrast(lighter, darker, Floor::FourAndHalf)],
    }
}

#[test]
#[ignore = "полный sRGB8 oracle и замеры выполняются явно в release"]
fn exhaustive_full_srgb_assignment_experiment() {
    let cases = [
        (
            "light_fixed_saturated_fill",
            full_color_problem("consumer-one/x", [0, 136, 255], [255; 3], true),
        ),
        (
            "dark_fixed_light_fill",
            full_color_problem("consumer-two/y", [60, 100, 130], [0; 3], false),
        ),
    ];
    for (name, problem) in cases {
        println!("INPUT case={name} problem={problem:?}");
        assert!(!experiment::admissible(
            &problem,
            &[problem.targets[0].anchor]
        ));
        let mut expected = None;
        for repeat in 0..2 {
            // AB/BA не смешивает прогрев с последовательностью стратегий.
            for pruning in if repeat == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let started = Instant::now();
                let run = experiment::solve(&problem, pruning, u64::MAX);
                let elapsed = started.elapsed();
                println!(
                    "RUN case={name} invocation={} pruning={pruning} elapsed_ns={} examined={} pruned_values={} outcome={:?}",
                    repeat + 1,
                    elapsed.as_nanos(),
                    run.examined,
                    run.pruned,
                    run.outcome
                );
                let values = assigned(run.outcome);
                assert!(experiment::admissible(&problem, &values));
                assert_ne!(values[0], problem.targets[0].anchor);
                if let Some(previous) = &expected {
                    assert_eq!(previous, &values);
                }
                expected = Some(values);
            }
        }
        let started = Instant::now();
        let reference = oracle::solve(&problem);
        let elapsed = started.elapsed();
        println!(
            "ORACLE case={name} elapsed_ns={} visited={} feasible={} best={:?}",
            elapsed.as_nanos(),
            reference.visited,
            reference.feasible,
            reference.best
        );
        assert_eq!(reference.visited, 1 << 24);
        assert_eq!(reference.best, expected);
        let result = reference.best.expect("перебор нашёл допустимый результат");
        println!(
            "DELTA case={name} delta_ok={:.12}",
            oracle::delta(result[0], problem.targets[0].anchor)
        );
    }
    let feasible = full_color_problem("different-id", [0, 90, 190], [255; 3], true);
    let run = experiment::solve(&feasible, true, 1);
    assert_eq!(run.outcome, Outcome::Assigned(vec![[0, 90, 190]]));
    assert_eq!(run.examined, 1);
}

// Публичный потребитель Core исполняется этим же integration target без features.
mod independent_computed_groups {
    use labcolors_core::Srgb8;
    use labcolors_core::program_wire::{
        ProgramAssessmentVerdictV1, ProgramConstraintAssessmentV1, ProgramScenarioV1,
        ProgramSnapshotStateV1, ProgramSnapshotV1, compile_program_wire,
    };

    // Канонические байты существующего JS ProgramWireBuilderV3: две opaque
    // серые цели, отдельные WCAG hard и наблюдаемый фон для каждой.
    // ID, области, опоры и maxDelta — декларация этого тестового потребителя.
    const WIRE: &[u8] = &[
        76, 67, 80, 87, 3, 0, 0, 1, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 21, 0, 0, 0, 3, 2, 0, 1, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 240, 63, 22, 0, 0, 0, 3, 2, 96, 97, 96, 96, 96, 0, 0, 0, 0, 0, 0, 240,
        63, 0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 2, 0, 0, 0, 21, 0, 0, 0, 22, 0, 0, 0, 2, 0, 0, 0, 31, 0,
        0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 41, 0, 0, 0, 1, 21, 0, 0, 0, 42, 0, 0, 0, 1, 22,
        0, 0, 0, 2, 0, 0, 0, 51, 0, 0, 0, 1, 31, 0, 0, 0, 52, 0, 0, 0, 1, 32, 0, 0, 0, 2, 0, 0, 0,
        61, 0, 0, 0, 41, 0, 0, 0, 51, 0, 0, 0, 0, 0, 0, 0, 0, 0, 80, 64, 154, 153, 153, 153, 153,
        153, 201, 63, 1, 62, 0, 0, 0, 42, 0, 0, 0, 52, 0, 0, 0, 0, 0, 0, 0, 0, 0, 80, 64, 154, 153,
        153, 153, 153, 153, 201, 63, 1, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 81, 0, 0, 0, 9, 61, 0,
        0, 0, 1, 82, 0, 0, 0, 9, 62, 0, 0, 0, 1, 0, 0, 0, 0, 2, 0, 0, 0, 91, 0, 0, 0, 41, 0, 0, 0,
        92, 0, 0, 0, 42, 0, 0, 0,
    ];

    fn declaration(second: u8) -> Vec<u8> {
        let mut bytes = WIRE.to_vec();
        // Только lower/upper/anchor второй computed gray записи V3.
        bytes[43] = second;
        bytes[44] = second + 1;
        bytes[45..48].fill(second);
        bytes
    }

    fn observed(backdrops: &[[u8; 2]]) -> Vec<ProgramScenarioV1> {
        backdrops
            .iter()
            .enumerate()
            .map(|(index, grays)| {
                ProgramScenarioV1::new(
                    index as u32 + 1,
                    grays.iter().map(|&gray| Srgb8::new([gray; 3])).collect(),
                )
            })
            .collect()
    }

    // Независимый перебор четырёх tuple: WCAG 2.2 relative luminance и 4.5:1.
    // https://www.w3.org/TR/2024/REC-WCAG22-20241212/#dfn-relative-luminance
    // В этих случаях опоры либо допустимы, либо всё множество пусто. Допустимая
    // tuple опор единственная имеет нулевой вектор расстояний прямого Oklab V3;
    // любой другой RGB отличается хотя бы в одной цели и имеет больший score.
    fn anchor_oracle(second: u8, backdrops: &[[u8; 2]]) -> Option<[u8; 2]> {
        let linear = |gray: u8| {
            let encoded = f64::from(gray) / 255.0;
            if encoded <= 0.04045 {
                encoded / 12.92
            } else {
                ((encoded + 0.055) / 1.055).powf(2.4)
            }
        };
        let mut feasible = Vec::new();
        for first in 0..=1 {
            for second_value in second..=second + 1 {
                let tuple = [first, second_value];
                if backdrops.iter().all(|backdrop| {
                    tuple.iter().zip(backdrop).all(|(&gray, &background)| {
                        let a = linear(gray);
                        let b = linear(background);
                        (a.max(b) + 0.05) / (a.min(b) + 0.05) >= 4.5
                    })
                }) {
                    feasible.push(tuple);
                }
            }
        }
        let anchors = [0, second];
        if feasible.contains(&anchors) {
            assert_eq!(feasible.len(), 4);
            Some(anchors)
        } else {
            assert!(
                feasible.is_empty(),
                "этот свидетель допускает опоры либо пустое множество"
            );
            None
        }
    }

    fn assert_ready(snapshot: &ProgramSnapshotV1, expected: [u8; 2]) {
        assert_eq!(snapshot.snapshot_version(), 2);
        assert_eq!(snapshot.state(), ProgramSnapshotStateV1::Ready);
        let mut outputs: Vec<_> = snapshot
            .outputs()
            .iter()
            .map(|output| (output.slot(), output.source(), output.opacity()))
            .collect();
        outputs.sort_by_key(|output| output.0);
        assert_eq!(
            outputs,
            vec![
                (91, Srgb8::new([expected[0]; 3]), 1.0),
                (92, Srgb8::new([expected[1]; 3]), 1.0)
            ]
        );
        let report = snapshot
            .assessment_report()
            .expect("Ready содержит финальные оценки");
        assert_eq!(report.revision(), snapshot.revision());
        let mut rows: Vec<_> = report
            .cells()
            .iter()
            .map(|cell| {
                (
                    cell.constraint_id(),
                    cell.case_index(),
                    cell.hard(),
                    cell.assessment(),
                )
            })
            .collect();
        rows.sort_by_key(|row| row.0);
        let pass = ProgramConstraintAssessmentV1::Wcag22Srgb8 {
            verdict: ProgramAssessmentVerdictV1::Pass,
        };
        assert_eq!(rows, vec![(81, 0, true, pass), (82, 0, true, pass)]);
    }

    fn assert_unsatisfied(snapshot: &ProgramSnapshotV1) {
        assert_eq!(snapshot.state(), ProgramSnapshotStateV1::Unsatisfied);
        assert!(snapshot.outputs().is_empty());
        assert!(snapshot.assessment_report().is_none());
    }

    #[test]
    fn independent_computed_groups_keep_global_anchors_and_lifecycle() {
        let mut session = compile_program_wire(&declaration(96))
            .expect("V3 допускает независимые группы с покрытием каждой цели")
            .instantiate(8)
            .expect("stream допустим");
        let ready = session
            .update_observed(1, &observed(&[[255, 255]]))
            .expect("белый фон принят");
        assert_ready(
            &ready,
            anchor_oracle(96, &[[255, 255]]).expect("опоры допустимы"),
        );
        assert_eq!(
            session
                .update_observed(1, &observed(&[[255, 255]]))
                .expect("повтор принят"),
            ready
        );

        let stale = session.update_unknown(2, 9).expect("неизвестность принята");
        assert_eq!(stale.state(), ProgramSnapshotStateV1::Stale);
        assert!(stale.outputs().is_empty());
        assert!(stale.assessment_report().is_none());
        assert_ready(&ready, [0, 96]);

        assert_eq!(anchor_oracle(96, &[[0, 255]]), None);
        let unsatisfied = session
            .update_observed(3, &observed(&[[0, 255]]))
            .expect("первый фон стал чёрным");
        assert_unsatisfied(&unsatisfied);
        assert_eq!(
            session
                .update_observed(3, &observed(&[[0, 255]]))
                .expect("повтор принят"),
            unsatisfied
        );
        assert_ready(
            &session
                .update_observed(4, &observed(&[[255, 255]]))
                .expect("восстановление принято"),
            [0, 96],
        );

        assert_eq!(anchor_oracle(96, &[[255, 255], [0, 255]]), None);
        assert_unsatisfied(
            &session
                .update_observed(5, &observed(&[[255, 255], [0, 255]]))
                .expect("оба сценария приняты"),
        );
    }

    #[test]
    fn independent_computed_groups_cannot_skip_the_second_hard_constraint() {
        // Первая цель проходит WCAG на белом; только вторая делает tuple невозможной.
        for (second, backdrop) in [(96, [255, 0]), (200, [255, 255])] {
            assert_eq!(anchor_oracle(second, &[backdrop]), None);
            let mut session = compile_program_wire(&declaration(second))
                .expect("непустые области допускаются до оценки композиции")
                .instantiate(8)
                .expect("stream допустим");
            let snapshot = session
                .update_observed(1, &observed(&[backdrop]))
                .expect("фон принят");
            assert_unsatisfied(&snapshot);
        }
    }
}
