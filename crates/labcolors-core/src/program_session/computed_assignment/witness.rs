//! Свидетельство отвергнутого авторского tuple после остановки поиска.
//! Проверка не предлагает новый кандидат и не изменяет search accounting.

use super::*;
use crate::program_diagnostics::{
    ProgramAttemptTargetV1, ProgramAttemptWitnessV1, ProgramDiagnosticSubjectV1,
    ProgramDiagnosticV1, ProgramDiagnosticsV1,
};

fn push_violation(
    violations: &mut Vec<ProgramDiagnosticV1>,
    violation: ProgramDiagnosticV1,
) -> Result<(), ProgramPhysicalCaseObserverErrorV1> {
    violations
        .try_reserve(1)
        .map_err(|_| ProgramPhysicalCaseObserverErrorV1::ResourceExhausted)?;
    violations.push(violation);
    Ok(())
}

struct IntentWitnessV1<'a> {
    selection: &'a ComputedSelectionV1,
    values: &'a [Srgb8],
    violations: &'a mut Vec<ProgramDiagnosticV1>,
}

impl ProgramPhysicalCaseObserverV1 for IntentWitnessV1<'_> {
    fn observe(
        &mut self,
        case_index: usize,
        evaluation: &AppearanceEvaluationView<'_, '_>,
    ) -> Result<(), ProgramPhysicalCaseObserverErrorV1> {
        for (target, &source) in self.selection.targets.iter().zip(self.values) {
            let actual = if let Some(slot) = target.appearance {
                Srgb8::new(
                    evaluation
                        .occurrence_at(slot)
                        .ok_or(ProgramPhysicalCaseObserverErrorV1::InternalInvariant)?
                        .visible(),
                )
            } else {
                source
            };
            let occurrence_id = target
                .effective_intent()
                .and_then(|intent| intent.appearance)
                .map(|id| id.value());
            if target.effective_intent().is_some_and(|intent| {
                !intent
                    .identity
                    .contains(target.identity_point(source, actual))
            }) {
                push_violation(
                    self.violations,
                    ProgramDiagnosticV1::ColorIdentity {
                        target_id: target.id.value(),
                        case_index,
                        occurrence_id: if target.declaration.reference_appearance {
                            None
                        } else {
                            occurrence_id
                        },
                        actual: target.identity_point(source, actual),
                    },
                )?;
            }
            let distance_squared = squared_distance(lab(actual), target.anchor_lab);
            let maximum_distance = f64::from_bits(target.declaration.max_delta_bits);
            if distance_squared > maximum_distance * maximum_distance {
                push_violation(
                    self.violations,
                    ProgramDiagnosticV1::MaxChange {
                        target_id: target.id.value(),
                        case_index,
                        occurrence_id,
                        actual,
                        distance_squared,
                        maximum_distance,
                    },
                )?;
            }
        }
        Ok(())
    }
}

pub(super) fn capture<E: ProgramConstraintEvaluatorSetV1>(
    selection: &ComputedSelectionV1,
    runtime: &mut ProgramEvaluationRuntimeV1<'_>,
    epoch: &ProgramEpochV1<E>,
    scenarios: NonEmptyScenarioSetV1<'_>,
    arena: &mut ProgramEvaluationArenaV1<E>,
    search: SearchEvidenceV1,
) -> Result<ProgramDiagnosticsV1, ProgramSessionEvaluationError<ProgramEvaluatorError<E>>> {
    if search.visited == 0
        || selection.budget == 0 && search.completion == CompletionV1::BudgetExhausted
    {
        return Ok(ProgramDiagnosticsV1 {
            search_visited: search.visited,
            search_budget: selection.total_budget,
            witness: None,
        });
    }
    let mut values = buffer(selection.targets.len(), Srgb8::new([0; 3]))
        .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    for (value, target) in values.iter_mut().zip(selection.targets.iter()) {
        *value = target.declaration.anchor;
    }
    apply::<E>(selection, runtime, &values)?;
    arena.clear();
    let mut violations = Vec::new();
    let mut collector = IntentWitnessV1 {
        selection,
        values: &values,
        violations: &mut violations,
    };
    scan_program_candidate(
        runtime,
        epoch,
        scenarios,
        0,
        ProgramEvaluationPhaseV1::Hard,
        ProgramCandidateCollectionV1 {
            evidence: ProgramConstraintEvidenceCaptureV1::Report {
                cells: &mut arena.cells,
                relation_members: &mut arena.relation_members,
            },
            outputs: None,
            point_causal: None,
            physical_case: Some(&mut collector),
        },
        None,
    )?;
    let intent_count = violations.len();
    for cell in &arena.cells {
        if cell.result().is_violation() {
            if let ProgramConstraintResultV1::Violation(
                ProgramConstraintViolationEvidenceV1::Relation(span),
            ) = cell.result()
            {
                for member in span
                    .get(&arena.relation_members)
                    .ok_or(ProgramSessionEvaluationError::InternalInvariant)?
                {
                    if let ProgramRelationMemberEvidenceV1::TonalHierarchy {
                        stronger,
                        weaker,
                        measurement: CoreRelationMeasurementV1::TonalHierarchy(measurement),
                        ..
                    } = member
                    {
                        if !measurement.satisfied() {
                            push_violation(
                                &mut violations,
                                ProgramDiagnosticV1::TonalHierarchy {
                                    constraint_id: cell.constraint().value(),
                                    case_index: cell.case_index(),
                                    stronger_occurrence_id: stronger.occurrence.value(),
                                    weaker_occurrence_id: weaker.occurrence.value(),
                                    required_gap: measurement.required_gap(),
                                    actual_gap: measurement.actual_gap(),
                                    required_ratio: measurement.required_ratio(),
                                    actual_ratio: measurement.actual_ratio(),
                                    stronger_polarity: measurement.stronger_polarity(),
                                    weaker_polarity: measurement.weaker_polarity(),
                                },
                            )
                            .map_err(map_physical_case_error)?;
                        }
                    }
                }
            }

            if matches!(
                cell.subject(),
                ProgramConstraintSubjectV1::ComputedVisibleRelation
            ) {
                continue;
            }
            push_violation(
                &mut violations,
                ProgramDiagnosticV1::HardConstraint {
                    constraint_id: cell.constraint().value(),
                    case_index: cell.case_index(),
                    subject: subject(cell.subject()),
                },
            )
            .map_err(map_physical_case_error)?;
        }
    }
    let mut targets = Vec::new();
    targets
        .try_reserve_exact(selection.targets.len())
        .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    for (target, &value) in selection.targets.iter().zip(&values) {
        targets.push(ProgramAttemptTargetV1 {
            target_id: target.id.value(),
            source: value,
        });
        if !target.declaration.domain.contains(value) {
            push_violation(
                &mut violations,
                ProgramDiagnosticV1::TargetDomain {
                    target_id: target.id.value(),
                    source: value,
                },
            )
            .map_err(map_physical_case_error)?;
        }
    }
    // Keep the public witness order: hard conditions, domains, then per-case
    // intent violations. Rotation needs no second buffer or physical pass.
    violations.rotate_left(intent_count);

    let mut case_scenario_ids = Vec::new();
    case_scenario_ids
        .try_reserve_exact(scenarios.len().get())
        .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
    for case_index in 0..scenarios.len().get() {
        let provenance = scenarios
            .observation()
            .provenance(case_index)
            .ok_or(ProgramSessionEvaluationError::InternalInvariant)?;
        let mut ids = Vec::new();
        ids.try_reserve_exact(provenance.len())
            .map_err(|_| ProgramSessionEvaluationError::ResourceExhausted)?;
        ids.extend(provenance.iter().map(|id| id.value()));
        case_scenario_ids.push(ids);
    }
    // Этот tuple уже отвергнут до остановки. Повторный законный успех означал
    // бы недетерминизм или ошибку поиска, а не новый fallback после бюджета.
    if violations.is_empty() {
        return Err(ProgramSessionEvaluationError::InternalInvariant);
    }
    Ok(ProgramDiagnosticsV1 {
        search_visited: search.visited,
        search_budget: selection.total_budget,
        witness: Some(ProgramAttemptWitnessV1 {
            case_scenario_ids,
            targets,
            violations,
        }),
    })
}

fn subject(value: ProgramConstraintSubjectV1) -> ProgramDiagnosticSubjectV1 {
    match value {
        ProgramConstraintSubjectV1::ComputedVisibleRelation => {
            ProgramDiagnosticSubjectV1::ComputedVisibleRelation
        }
        ProgramConstraintSubjectV1::VisibleUnary { occurrence, .. } => {
            ProgramDiagnosticSubjectV1::Occurrence {
                occurrence_id: occurrence.value(),
            }
        }
        ProgramConstraintSubjectV1::IntrinsicUnary { target } => {
            ProgramDiagnosticSubjectV1::Target {
                target_id: target.value(),
            }
        }
        ProgramConstraintSubjectV1::IntrinsicRelation { reference } => {
            ProgramDiagnosticSubjectV1::IntrinsicRelation {
                reference_target_id: reference.value(),
            }
        }
        ProgramConstraintSubjectV1::VisibleRelation { reference, .. } => {
            ProgramDiagnosticSubjectV1::VisibleRelation {
                reference_occurrence_id: reference.value(),
            }
        }
        ProgramConstraintSubjectV1::PointPresentation { target, terminal } => {
            ProgramDiagnosticSubjectV1::Presentation {
                root_id: target.root().value(),
                occurrence_id: target.occurrence().value(),
                terminal_id: terminal.value(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{compile_assignment, program, target, update};
    use super::*;
    use crate::color_intent::{ColorIdentityDomainV1, IdentityOptionsV1};
    use crate::constraints::CountingProgramWcag22Srgb8V1;
    use crate::session::SessionState;

    #[test]
    fn zero_budget_has_no_witness_and_runs_no_attempt_evaluator() -> Result<(), String> {
        let evaluator = CountingProgramWcag22Srgb8V1::default();
        let owner = compile_assignment(
            program(
                target(10, 150, 0, 3, 0),
                Wcag22CriterionV1::Sc143TextDefault,
                evaluator.clone(),
            ),
            0,
        )
        .map_err(|error| format!("compile: {error:?}"))?;
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .map_err(|error| format!("instantiate: {error:?}"))?;
        session
            .prepare_update(update(1, 255))
            .map_err(|error| format!("update: {error:?}"))?
            .commit();
        let SessionState::Incomplete { cause, .. } = session.state() else {
            return Err("budget zero is incomplete".into());
        };
        assert!(cause.diagnostics().witness().is_none());
        assert_eq!(cause.diagnostics().search_visited(), 0);
        assert_eq!(cause.diagnostics().search_budget(), 0);
        assert!(evaluator.calls().is_empty());
        Ok(())
    }

    #[test]
    fn out_of_domain_authored_source_keeps_ids_without_claiming_a_minimal_conflict()
    -> Result<(), String> {
        let owner = compile_assignment(
            program(
                target(10, 150, 0, 3, 0),
                Wcag22CriterionV1::Sc143TextDefault,
                Wcag22Srgb8V1,
            ),
            1,
        )
        .map_err(|error| format!("compile: {error:?}"))?;
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .map_err(|error| format!("instantiate: {error:?}"))?;
        session
            .prepare_update(update(1, 255))
            .map_err(|error| format!("update: {error:?}"))?
            .commit();
        let SessionState::Incomplete { cause, .. } = session.state() else {
            return Err("one visit is incomplete".into());
        };
        assert_eq!(cause.diagnostics().search_visited(), 1);
        let witness = cause.diagnostics().witness().ok_or("authored witness")?;
        assert_eq!(witness.targets()[0].target_id(), 10);
        assert_eq!(witness.targets()[0].source(), Srgb8::new([150; 3]));
        assert_eq!(witness.case_scenario_ids(), &[vec![9]]);
        assert!(
            witness
                .violations()
                .contains(&ProgramDiagnosticV1::TargetDomain {
                    target_id: 10,
                    source: Srgb8::new([150; 3])
                })
        );
        assert!(
            witness
                .violations()
                .contains(&ProgramDiagnosticV1::HardConstraint {
                    constraint_id: 6,
                    case_index: 0,
                    subject: ProgramDiagnosticSubjectV1::Occurrence { occurrence_id: 5 },
                })
        );
        Ok(())
    }

    #[test]
    fn transparent_color_attempt_reports_visible_identity_and_max_change() -> Result<(), String> {
        let source = Srgb8::new([0, 103, 244]);
        let color = ComputedTargetV1::color(
            source,
            0.0,
            ColorUseIntentV1 {
                identity: ColorIdentityDomainV1::new(source, false, IdentityOptionsV1::default())
                    .map_err(|error| format!("identity: {error:?}"))?,
                appearance: Some(OccurrenceId::new(5)),
                tone: None,
            },
        );
        let mut declaration = program(
            Target::new(TargetId::new(10), TargetIntentV1::Computed(color)),
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22Srgb8V1,
        );
        declaration.constraints = ConstraintSet::new(vec![], vec![]);
        declaration
            .opacities
            .push(OpacityInput::new(OpacityInputId::new(11), 0.0));
        declaration.paints.push(Paint::Opacity {
            id: PaintId::new(12),
            source: PaintId::new(3),
            opacity: OpacityInputId::new(11),
        });
        declaration.occurrences[0].subject = PaintId::new(12);
        declaration.outputs[0] = OutputBinding::new(OutputSlotId::new(7), PaintId::new(12));
        let owner =
            compile_assignment(declaration, 1).map_err(|error| format!("compile: {error:?}"))?;
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .map_err(|error| format!("instantiate: {error:?}"))?;
        session
            .prepare_update(update(1, 255))
            .map_err(|error| format!("update: {error:?}"))?
            .commit();
        let SessionState::Incomplete { cause, .. } = session.state() else {
            return Err("budgeted attempt is incomplete".into());
        };
        let witness = cause.diagnostics().witness().ok_or("visible witness")?;
        assert!(
            witness
                .violations()
                .contains(&ProgramDiagnosticV1::ColorIdentity {
                    target_id: 10,
                    case_index: 0,
                    occurrence_id: Some(5),
                    actual: Srgb8::new([255; 3]),
                })
        );
        assert!(witness.violations().iter().any(|value| matches!(value,
            ProgramDiagnosticV1::MaxChange { target_id: 10, case_index: 0, occurrence_id: Some(5), actual, distance_squared, maximum_distance }
                if *actual == Srgb8::new([255; 3]) && *distance_squared > 0.0 && *maximum_distance == 0.0)));
        assert!(session.state().current_verified().is_none());
        Ok(())
    }

    #[test]
    fn reference_attempt_reports_source_identity_separately_from_visible_max_change() {
        let source = Srgb8::new([0, 103, 244]);
        let mut color = ComputedTargetV1::color(
            source,
            0.0,
            ColorUseIntentV1 {
                identity: ColorIdentityDomainV1::new(source, true, IdentityOptionsV1::default())
                    .expect(
                        "forced-neutral identity options should accept the colored source anchor",
                    ),
                appearance: Some(OccurrenceId::new(5)),
                tone: None,
            },
        );
        color.reference_appearance = true;
        let mut declaration = program(
            Target::new(TargetId::new(10), TargetIntentV1::Computed(color)),
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22Srgb8V1,
        );
        declaration.constraints = ConstraintSet::new(vec![], vec![]);
        declaration.reference_inputs =
            Some(vec![(SurfaceInputPortId::new(2), Srgb8::new([255; 3]))]);
        declaration
            .opacities
            .push(OpacityInput::new(OpacityInputId::new(11), 0.0));
        declaration.paints.push(Paint::Opacity {
            id: PaintId::new(12),
            source: PaintId::new(3),
            opacity: OpacityInputId::new(11),
        });
        declaration.occurrences[0].subject = PaintId::new(12);
        declaration.outputs[0] = OutputBinding::new(OutputSlotId::new(7), PaintId::new(12));
        let owner = compile_assignment(declaration, 1)
            .expect("transparent reference fixture should compile with a one-attempt budget");
        let mut session = owner
            .instantiate(ObservationStreamId::new(8))
            .expect("compiled witness fixture should instantiate observation stream 8");
        session.prepare_update(update(1, 255)).expect("well-formed witness observation should be admitted even when solving remains incomplete").commit();
        let SessionState::Incomplete { cause, .. } = session.state() else {
            panic!("one authored-source attempt cannot satisfy forced source neutrality");
        };
        let witness = cause
            .diagnostics()
            .witness()
            .expect("incomplete authored-source attempt should include a diagnostic witness");
        assert!(
            witness
                .violations()
                .contains(&ProgramDiagnosticV1::ColorIdentity {
                    target_id: 10,
                    case_index: 0,
                    occurrence_id: None,
                    actual: source,
                })
        );
        assert!(
            !witness
                .violations()
                .iter()
                .any(|value| matches!(value, ProgramDiagnosticV1::MaxChange { .. }))
        );
    }
}
