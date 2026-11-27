//! Публичный native путь computed Program: owned-данные и host acceptance.
use std::sync::{Arc, Mutex, Weak};

use labcolors::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn runtime() -> Result<Arc<ProgramRuntime>, LabColorsError> {
    compile_program_wire(
        include_bytes!("../../labcolors-core/tests/fixtures/visible-separation.lcpw").to_vec(),
        8,
    )
}

fn field_runtime() -> Result<Arc<ProgramRuntime>, LabColorsError> {
    // Existing public-builder fixture: fixed white at opacity 0.5 with a
    // report-only nominal constraint. Every small input field remains feasible.
    compile_program_wire(
        include_bytes!("../../labcolors-core/tests/fixtures/nominal-visible-report.lcpw").to_vec(),
        8,
    )
}

fn rgb(gray: u8) -> ProgramRgb8 {
    ProgramRgb8 {
        red: gray,
        green: gray,
        blue: gray,
    }
}

#[test]
fn native_reference_hierarchy_preserves_owned_pairs_and_explains_fixed_conflict() -> TestResult {
    let runtime = compile_program_wire(
        include_bytes!("../../labcolors-core/tests/fixtures/reference-hierarchy.lcpw").to_vec(),
        8,
    )?;
    let dark = runtime.update_observed(1, vec![9], vec![0; 3], 1)?;
    assert_eq!(dark.state(), "ready");
    let report = dark
        .assessment_report()?
        .ok_or("reference hierarchy report")?;
    let pair = &report.tonal_hierarchy[0];
    assert_eq!(
        (
            pair.constraint_id,
            pair.stronger_occurrence_id,
            pair.weaker_occurrence_id
        ),
        (30, 5, 15)
    );
    assert!(pair.actual_gap >= pair.required_gap);
    assert!(pair.actual_ratio >= pair.required_ratio);
    assert_eq!(pair.required_gap, pair.required_ratio.ln());
    assert_eq!(pair.stronger_polarity, ProgramTonalPolarity::Lighter);
    assert_eq!(report.reference_selections.len(), 2);
    let selection = &report.reference_selections[0];
    assert_eq!(selection.target_id, 10);
    assert_eq!(selection.occurrence_id, 5);
    assert_eq!(
        selection.preference,
        ProgramReferencePreference::ReferenceStrength
    );
    assert_eq!(selection.cases[0].reference_backdrop, rgb(255));
    assert_eq!(selection.cases[0].actual_backdrop, rgb(0));
    assert_eq!(
        dark.reference_selections()?,
        Some(report.reference_selections.clone())
    );
    runtime.update_observed(2, vec![9], vec![255; 3], 1)?;
    assert_eq!(dark.assessment_report()?, Some(report));

    let fixed = compile_program_wire(
        include_bytes!("../../labcolors-core/tests/fixtures/reference-hierarchy-fixed.lcpw")
            .to_vec(),
        8,
    )?;
    let rejected = fixed.update_observed(1, vec![9], vec![0; 3], 1)?;
    assert_eq!(rejected.state(), "unsatisfied");
    assert_eq!(rejected.reference_selections()?, None);
    let diagnostics = rejected.diagnostics()?.ok_or("fixed relation diagnostic")?;
    let witness = diagnostics.witness.ok_or("fixed relation witness")?;
    assert!(
        matches!(witness.violations.as_slice(), [ProgramDiagnostic::TonalHierarchy {
        stronger_occurrence_id: 5, weaker_occurrence_id: 15, required_gap, actual_gap, ..
    }] if actual_gap < required_gap)
    );
    assert_eq!(
        fixed.update_observed(2, vec![9], vec![255; 3], 1)?.state(),
        "ready"
    );
    Ok(())
}

#[test]
fn native_nominal_visible_reports_preserve_the_final_color_and_typed_rejection() -> TestResult {
    // Public JS V3 builder: fixed white at opacity 0.5 over one input, with a
    // report-only nominal-visible constraint and an empty assignment.
    let bytes =
        include_bytes!("../../labcolors-core/tests/fixtures/nominal-visible-report.lcpw").to_vec();
    let runtime = compile_program_wire(bytes, 1)?;
    let first = runtime.update_observed(1, vec![9], vec![255; 3], 1)?;
    let first_report = first.assessment_report()?.ok_or("first nominal report")?;
    assert_eq!(first.state(), "ready");
    assert_eq!(first_report.cells.len(), 1);
    assert_eq!(
        first.cleanliness_assessment()?,
        Some(first_report.cleanliness.clone())
    );
    assert_eq!(
        first_report.cleanliness,
        ProgramCleanlinessAssessment {
            status: ProgramCleanlinessStatus::Conforms,
            profile: Some("nominal-srgb8-v2".into()),
            basis: Some("declared-convention".into()),
            scope: Some("declared-final-points".into()),
            accepted: 1,
            rejected: 0,
            unavailable: 0,
        }
    );
    assert_eq!(
        first_report.cells[0].assessment,
        ProgramConstraintAssessment::NominalSrgb8VisiblePass { visible: rgb(255) },
    );

    let changed = runtime.update_observed(2, vec![9], vec![1, 1, 3], 1)?;
    assert_eq!(
        changed.state(),
        "ready",
        "a report-only rejection does not gate the frame"
    );
    let report = changed
        .assessment_report()?
        .ok_or("changed nominal report")?;
    assert_eq!(
        changed.cleanliness_assessment()?,
        Some(report.cleanliness.clone())
    );
    assert_eq!(
        report.cleanliness.status,
        ProgramCleanlinessStatus::Violates
    );
    assert_eq!(
        (
            report.cleanliness.accepted,
            report.cleanliness.rejected,
            report.cleanliness.unavailable
        ),
        (0, 1, 0)
    );
    let ProgramConstraintAssessment::NominalSrgb8VisibleRejected {
        visible,
        rejected_blue_lower,
        rejected_blue_upper,
    } = report.cells[0].assessment
    else {
        return Err("a visible nominal rejection must retain its own assessment kind".into());
    };
    assert_eq!(
        visible,
        ProgramRgb8 {
            red: 128,
            green: 128,
            blue: 129
        }
    );
    assert!((rejected_blue_lower..=rejected_blue_upper).contains(&129));
    drop(runtime);
    assert_eq!(first.assessment_report()?, Some(first_report));
    Ok(())
}

#[derive(Clone, Copy)]
enum Decision {
    Accept,
    Refuse,
    Error,
}

struct Capture {
    snapshots: Arc<Mutex<Vec<Arc<ProgramSnapshot>>>>,
    decision: Decision,
}

impl ProgramUpdateHost for Capture {
    fn accept(&self, snapshot: Arc<ProgramSnapshot>) -> Result<bool, LabColorsError> {
        assert_eq!(snapshot.snapshot_version(), 2);
        assert_eq!(snapshot.revision(), 2);
        self.snapshots
            .lock()
            .expect("snapshot storage")
            .push(snapshot);
        match self.decision {
            Decision::Accept => Ok(true),
            Decision::Refuse => Ok(false),
            Decision::Error => Err(LabColorsError::Refused {
                code: "host_batch_rejected".into(),
                operation: "hostBatch".into(),
            }),
        }
    }
}

#[test]
fn native_acceptance_preserves_revision_on_false_and_error_and_retains_owned_proposals()
-> TestResult {
    let runtime = runtime()?;
    let first = runtime.update_observed(1, vec![9], vec![255; 3], 1)?;
    let first_table = first.copy_output_table()?;
    let retained = Arc::new(Mutex::new(Vec::new()));
    for decision in [Decision::Refuse, Decision::Error] {
        let result = runtime.update_observed_with(
            2,
            vec![9],
            vec![128; 3],
            1,
            Box::new(Capture {
                snapshots: Arc::clone(&retained),
                decision,
            }),
        );
        match decision {
            Decision::Refuse => assert!(!result?),
            Decision::Error => match result {
                Err(LabColorsError::Refused { code, operation }) => {
                    assert_eq!(code, "host_batch_rejected");
                    assert_eq!(operation, "hostBatch");
                }
                Ok(_) => panic!("callback error must abort the proposal"),
            },
            Decision::Accept => unreachable!(),
        }
        let replay = runtime.update_observed(1, vec![9], vec![255; 3], 1)?;
        assert_eq!(replay.revision(), 1);
        assert_eq!(replay.copy_output_table()?, first_table);
    }
    let accepted = runtime.update_observed_with(
        2,
        vec![9],
        vec![128; 3],
        1,
        Box::new(Capture {
            snapshots: Arc::clone(&retained),
            decision: Decision::Accept,
        }),
    )?;
    assert!(accepted);
    let snapshots = retained.lock().expect("snapshot storage");
    assert_eq!(snapshots.len(), 3);
    let accepted = &snapshots[2];
    assert_ne!(accepted.copy_output_table()?, first_table);
    assert_eq!(
        snapshots[0].copy_output_table()?,
        accepted.copy_output_table()?
    );
    assert_eq!(
        snapshots[1].assessment_report()?,
        accepted.assessment_report()?
    );
    assert_eq!(
        runtime
            .update_observed(2, vec![9], vec![128; 3], 1)?
            .assessment_report()?,
        accepted.assessment_report()?,
    );
    drop(runtime);
    assert_eq!(first.copy_output_table()?, first_table);
    assert_eq!(accepted.revision(), 2);
    Ok(())
}

#[test]
fn native_reports_preserve_composition_provenance_and_owned_conflict_witness() -> TestResult {
    let runtime = runtime()?;
    let ready = runtime.update_observed(1, vec![9], vec![255; 3], 1)?;
    assert_eq!(ready.state(), "ready");
    assert_eq!(ready.snapshot_version(), 2);
    assert_eq!(ready.reason(), None);
    assert_eq!(ready.optimality().as_deref(), Some("exact"));
    assert_eq!(ready.diagnostics()?, None);
    let report = ready.assessment_report()?.ok_or("ready report")?;
    assert_eq!(
        ready.cleanliness_assessment()?,
        Some(report.cleanliness.clone())
    );
    assert_eq!(
        report.cleanliness,
        ProgramCleanlinessAssessment {
            status: ProgramCleanlinessStatus::NotAssessed,
            profile: None,
            basis: None,
            scope: None,
            accepted: 0,
            rejected: 0,
            unavailable: 0,
        }
    );
    assert_eq!(report.content_identity.len(), 32);
    assert_eq!(report.stream_id, 8);
    assert_eq!(report.revision, 1);
    assert_eq!(
        report.scenario_provenance,
        [ProgramScenarioProvenance {
            case_index: 0,
            scenario_id: 9
        }]
    );
    let cell = &report.cells[0];
    assert_eq!(
        (cell.constraint_id, cell.case_index, cell.hard),
        (6, 0, true)
    );
    let ProgramConstraintAssessment::VisibleSeparation {
        verdict,
        visible,
        backdrop,
        distance,
        distance_lower,
        distance_upper,
        minimum,
        maximum,
    } = cell.assessment
    else {
        return Err("typed separation required".into());
    };
    assert_eq!(verdict, ProgramAssessmentVerdict::Pass);
    assert_eq!(visible, rgb(192));
    assert_eq!(backdrop, rgb(255));
    assert_eq!((minimum, maximum), (0.18, 0.21));
    assert!((minimum..=maximum).contains(&distance));
    assert_eq!((distance_lower, distance_upper), (distance, distance));
    let mut changed = ready.assessment_report()?.ok_or("independent report")?;
    changed.cells.clear();
    changed.content_identity.fill(0);
    assert_eq!(ready.assessment_report()?, Some(report.clone()));

    let conflict =
        runtime.update_observed(2, vec![9, 10], vec![255, 255, 255, 128, 128, 128], 1)?;
    assert_eq!(conflict.state(), "unsatisfied");
    assert_eq!(conflict.output_count(), 0);
    assert_eq!(conflict.optimality(), None);
    assert_eq!(conflict.reason(), None);
    assert_eq!(conflict.assessment_report()?, None);
    assert_eq!(conflict.cleanliness_assessment()?, None);
    let diagnostics = conflict.diagnostics()?.ok_or("conflict diagnostics")?;
    assert!(diagnostics.search_visited <= diagnostics.search_budget);
    let witness = diagnostics
        .witness
        .as_ref()
        .ok_or("authored attempt witness")?;
    assert_eq!(witness.case_scenario_ids, [vec![10], vec![9]]);
    assert_eq!(
        witness.targets,
        [ProgramAttemptTarget {
            target_id: 10,
            source: rgb(128)
        }]
    );
    assert!(
        witness
            .violations
            .contains(&ProgramDiagnostic::HardConstraint {
                constraint_id: 6,
                case_index: 0,
                field_case: None,
                subject: ProgramDiagnosticSubject::Occurrence { occurrence_id: 5 },
            })
    );
    let mut changed = conflict.diagnostics()?.ok_or("independent witness")?;
    changed
        .witness
        .as_mut()
        .ok_or("copied witness")?
        .case_scenario_ids[0]
        .clear();
    assert_eq!(conflict.diagnostics()?, Some(diagnostics.clone()));

    let unknown = runtime.update_unknown(3, 17)?;
    assert_eq!(unknown.revision(), 3);
    assert_eq!(unknown.diagnostics()?, None);
    assert_eq!(unknown.assessment_report()?, None);
    assert_eq!(unknown.cleanliness_assessment()?, None);
    assert_eq!(unknown.output_count(), 0);
    let recovered = runtime.update_observed(4, vec![9], vec![255; 3], 1)?;
    assert_eq!(recovered.state(), "ready");
    assert_eq!(recovered.diagnostics()?, None);
    drop(runtime);
    assert_eq!(conflict.diagnostics()?, Some(diagnostics));
    assert_eq!(ready.assessment_report()?, Some(report));
    Ok(())
}

struct Reentrant {
    runtime: Weak<ProgramRuntime>,
}
impl ProgramUpdateHost for Reentrant {
    fn accept(&self, snapshot: Arc<ProgramSnapshot>) -> Result<bool, LabColorsError> {
        let runtime = self
            .runtime
            .upgrade()
            .expect("runtime remains live during callback");
        let Err(LabColorsError::Refused { code, operation }) = runtime.update_unknown(3, 17) else {
            panic!("reentrant update must be rejected");
        };
        assert_eq!(code, "program_busy");
        assert_eq!(operation, "updateUnknown");
        assert_eq!(snapshot.revision(), 2);
        assert!(snapshot.assessment_report()?.is_some());
        Ok(true)
    }
}

#[test]
fn native_callback_can_read_its_snapshot_but_cannot_reenter_its_runtime() -> TestResult {
    let runtime = runtime()?;
    runtime.update_observed(1, vec![9], vec![255; 3], 1)?;
    let accepted = runtime.update_observed_with(
        2,
        vec![9],
        vec![128; 3],
        1,
        Box::new(Reentrant {
            runtime: Arc::downgrade(&runtime),
        }),
    )?;
    assert!(accepted);
    assert_eq!(runtime.update_unknown(3, 17)?.revision(), 3);
    Ok(())
}

struct Panics;
impl ProgramUpdateHost for Panics {
    fn accept(&self, _: Arc<ProgramSnapshot>) -> Result<bool, LabColorsError> {
        panic!("simulated host unwind");
    }
}

#[test]
fn native_callback_unwind_does_not_poison_or_advance_the_runtime() -> TestResult {
    let runtime = runtime()?;
    let first = runtime.update_observed(1, vec![9], vec![255; 3], 1)?;
    let Err(LabColorsError::Refused { code, operation }) =
        runtime.update_observed_with(2, vec![9], vec![128; 3], 1, Box::new(Panics))
    else {
        return Err("host unwind must abort the proposal".into());
    };
    assert_eq!(code, "program_host_protocol");
    assert_eq!(operation, "updateObservedWith");
    let replay = runtime.update_observed(1, vec![9], vec![255; 3], 1)?;
    assert_eq!(replay.copy_output_table()?, first.copy_output_table()?);
    assert_eq!(
        runtime
            .update_observed(2, vec![9], vec![128; 3], 1)?
            .revision(),
        2
    );
    Ok(())
}

#[test]
fn unexpected_foreign_exception_uses_the_real_uniffi_error_conversion_without_panicking() {
    let error =
        uniffi::UnexpectedUniFFICallbackError::new("foreign exception outside declared error type");
    let result = <Result<bool, LabColorsError> as uniffi::LiftReturn<labcolors::UniFfiTag>>::handle_callback_unexpected_error(error);
    let Err(LabColorsError::Refused { code, operation }) = result else {
        panic!("foreign callback exception must become a typed failure");
    };
    assert_eq!(code, "program_host_protocol");
    assert_eq!(operation, "ProgramUpdateHost.accept");
    let point_error = <Result<bool, ProgramPointSinkCallbackError> as uniffi::LiftReturn<
        labcolors::UniFfiTag,
    >>::handle_callback_unexpected_error(
        uniffi::UnexpectedUniFFICallbackError::new("foreign point callback exception"),
    );
    assert!(matches!(
        point_error,
        Err(ProgramPointSinkCallbackError::InvocationFailed)
    ));
}

#[path = "../../labcolors-core/src/test_support.rs"]
mod test_support;

#[test]
fn native_field_rejection_and_callback_error_preserve_the_previous_composition() -> TestResult {
    let runtime = field_runtime()?;
    let previous = runtime.update_observed(1, vec![9], vec![255; 3], 1)?;
    let previous_table = previous.copy_output_table()?;
    let retained = Arc::new(Mutex::new(Vec::new()));
    for decision in [Decision::Refuse, Decision::Error] {
        let result = runtime.update_declared_field_with(
            2,
            vec![17; 3],
            0,
            rgb(0),
            rgb(2),
            Box::new(Capture {
                snapshots: Arc::clone(&retained),
                decision,
            }),
        );
        match decision {
            Decision::Refuse => assert!(!result?),
            Decision::Error => assert!(matches!(result,
                Err(LabColorsError::Refused { code, operation })
                    if code == "host_batch_rejected" && operation == "hostBatch")),
            Decision::Accept => unreachable!(),
        }
        let replay = runtime.update_observed(1, vec![9], vec![255; 3], 1)?;
        assert_eq!(replay.copy_output_table()?, previous_table);
        assert_eq!(replay.field_assessment()?, None);
    }
    assert!(runtime.update_declared_field_with(
        2,
        vec![17; 3],
        0,
        rgb(0),
        rgb(2),
        Box::new(Capture {
            snapshots: Arc::clone(&retained),
            decision: Decision::Accept,
        }),
    )?);
    let snapshots = retained.lock().expect("snapshot storage");
    assert_eq!(snapshots.len(), 3);
    let field = snapshots[2].field_assessment()?.ok_or("field metadata")?;
    assert_eq!(field.source_model, "opaque-affine-encoded-srgb8-half-up-v1");
    assert_eq!(
        (field.domain.as_str(), field.rendering.as_str()),
        ("whole-field", "unknown")
    );
    assert_eq!(
        (field.input_index, field.from, field.to, field.region_count),
        (0, rgb(0), rgb(2), 7)
    );
    assert_eq!(
        field.witnesses,
        vec![
            ProgramFieldWitness {
                numerator: 0,
                denominator: 1,
                nominal: rgb(0)
            },
            ProgramFieldWitness {
                numerator: 1,
                denominator: 4,
                nominal: rgb(1)
            },
            ProgramFieldWitness {
                numerator: 3,
                denominator: 4,
                nominal: rgb(2)
            },
        ]
    );
    for snapshot in snapshots.iter() {
        assert_eq!(snapshot.state(), "ready");
        assert_eq!(snapshot.revision(), 2);
        assert_eq!(snapshot.field_assessment()?, Some(field.clone()));
    }
    // The varying slot is not a second source of truth beside the descriptor.
    let replay = runtime.update_declared_field(2, vec![199; 3], 0, rgb(0), rgb(2))?;
    assert_eq!(replay.field_assessment()?, Some(field.clone()));
    drop(runtime);
    assert_eq!(previous.copy_output_table()?, previous_table);
    assert_eq!(snapshots[0].field_assessment()?, Some(field));
    Ok(())
}

#[test]
fn native_field_admission_refuses_wrong_shape_and_conflicting_direction_before_commit() -> TestResult
{
    let runtime = field_runtime()?;
    let previous = runtime.update_observed(1, vec![9], vec![255; 3], 1)?;
    let retained = Arc::new(Mutex::new(Vec::new()));
    for (surfaces, input) in [(vec![0; 2], 0), (vec![0; 6], 0), (vec![0; 3], 1)] {
        let result = runtime.update_declared_field_with(
            2,
            surfaces,
            input,
            rgb(0),
            rgb(2),
            Box::new(Capture {
                snapshots: Arc::clone(&retained),
                decision: Decision::Accept,
            }),
        );
        assert!(
            matches!(result, Err(LabColorsError::Refused { code, operation })
            if code == "program_update" && operation == "updateDeclaredFieldWith")
        );
    }
    assert!(retained.lock().expect("snapshot storage").is_empty());
    assert_eq!(
        runtime
            .update_observed(1, vec![9], vec![255; 3], 1)?
            .field_assessment()?,
        None
    );
    let accepted = runtime.update_declared_field(2, vec![0; 3], 0, rgb(0), rgb(2))?;
    for revision in [1, 2] {
        assert!(
            matches!(runtime.update_declared_field(revision, vec![0; 3], 0, rgb(2), rgb(0)),
            Err(LabColorsError::Refused { code, operation })
                if code == "program_update" && operation == "updateDeclaredField")
        );
    }
    let reversed = runtime.update_declared_field(3, vec![0; 3], 0, rgb(2), rgb(0))?;
    assert_eq!(
        reversed
            .field_assessment()?
            .ok_or("reversed descriptor")?
            .from,
        rgb(2)
    );
    assert_eq!(
        accepted
            .field_assessment()?
            .ok_or("accepted descriptor")?
            .from,
        rgb(0)
    );
    assert_eq!(previous.field_assessment()?, None);
    Ok(())
}

#[test]
fn native_field_callback_unwind_recovery_and_unknown_keep_scope_revision_bound() -> TestResult {
    let runtime = field_runtime()?;
    runtime.update_observed(1, vec![9], vec![255; 3], 1)?;
    assert!(runtime.update_declared_field_with(
        2,
        vec![0; 3],
        0,
        rgb(0),
        rgb(2),
        Box::new(Reentrant {
            runtime: Arc::downgrade(&runtime)
        }),
    )?);
    let current = runtime.update_declared_field(2, vec![0; 3], 0, rgb(0), rgb(2))?;
    let field = current.field_assessment()?.ok_or("current field")?;
    let result =
        runtime.update_declared_field_with(3, vec![0; 3], 0, rgb(2), rgb(0), Box::new(Panics));
    assert!(
        matches!(result, Err(LabColorsError::Refused { code, operation })
        if code == "program_host_protocol" && operation == "updateDeclaredFieldWith")
    );
    let replay = runtime.update_declared_field(2, vec![0; 3], 0, rgb(0), rgb(2))?;
    assert_eq!(replay.field_assessment()?, Some(field.clone()));
    let unknown = runtime.update_unknown(3, 17)?;
    assert_eq!(unknown.state(), "stale");
    assert_eq!(unknown.field_assessment()?, None);
    let mut changed = current
        .field_assessment()?
        .ok_or("independent field copy")?;
    changed.from = rgb(255);
    changed.witnesses.clear();
    assert_eq!(current.field_assessment()?, Some(field.clone()));
    drop(runtime);
    assert_eq!(current.field_assessment()?, Some(field));
    assert_eq!(unknown.field_assessment()?, None);
    Ok(())
}

struct FinalCheckpoint;
impl ProgramUpdateHost for FinalCheckpoint {
    fn accept(&self, snapshot: Arc<ProgramSnapshot>) -> Result<bool, LabColorsError> {
        assert_eq!(snapshot.revision(), 2);
        assert_eq!(snapshot.copy_output_table()?.len(), 5);
        assert!(snapshot.assessment_report()?.is_some());
        test_support::reset_allocator_events();
        Ok(true)
    }
}

#[test]
fn native_commit_and_real_uniffi_success_return_do_not_allocate_after_host_acceptance() -> TestResult
{
    let (control, allocations) =
        test_support::measured_allocations(|| Vec::<u8>::with_capacity(64));
    assert_eq!(allocations, 1, "allocator observer must detect real work");
    drop(control);
    let runtime = runtime()?;
    runtime.update_observed(1, vec![9], vec![255; 3], 1)?;
    let (lowered, events) = test_support::measured_allocator_events(|| {
        let accepted =
            runtime.update_observed_with(2, vec![9], vec![128; 3], 1, Box::new(FinalCheckpoint));
        <Result<bool, LabColorsError> as uniffi::LowerReturn<labcolors::UniFfiTag>>::lower_return(
            accepted,
        )
    });
    assert_eq!(events.alloc, 0);
    assert_eq!(events.realloc, 0);
    assert_eq!(
        lowered.map_err(|_| "UniFFI success return was rejected")?,
        1
    );
    assert_eq!(
        runtime
            .update_observed(2, vec![9], vec![128; 3], 1)?
            .revision(),
        2
    );
    Ok(())
}

#[test]
fn native_gradient_stops_replay_and_local_witness_survive_solids() -> TestResult {
    let runtime = field_runtime()?;
    let stops = vec![
        ProgramGradientStop {
            at: 0.0,
            color: rgb(0),
        },
        ProgramGradientStop {
            at: 0.125,
            color: rgb(1),
        },
        ProgramGradientStop {
            at: 1.0,
            color: rgb(3),
        },
    ];
    let positions = stops.iter().map(|stop| stop.at).collect();
    let colors = vec![0, 0, 0, 1, 1, 1, 3, 3, 3];
    let snapshot = runtime.update_gradient(1, vec![9; 3], 0, positions, colors.clone())?;
    let report = snapshot.gradient_assessment()?.ok_or("gradient scope")?;
    assert_eq!(report.stops, stops);
    assert_eq!(report.region_count, 11);
    assert_eq!(report.rendering, "unknown");
    let last = report.witnesses.last().ok_or("last witness")?;
    assert_eq!(
        (
            last.segment_index,
            last.local.numerator,
            last.local.denominator
        ),
        (1, 3, 4)
    );
    assert_eq!(last.local.nominal, rgb(3));
    assert!(
        matches!(snapshot.field_assessment(), Err(LabColorsError::Refused { code, .. }) if code == "program_gradient_assessment_required")
    );
    let moved = vec![0.0, 0.75, 1.0];
    assert!(
        runtime
            .update_gradient(1, vec![9; 3], 0, moved.clone(), colors.clone())
            .is_err()
    );
    assert!(!runtime.update_gradient_with(
        2,
        vec![0; 3],
        0,
        moved,
        colors,
        Box::new(Capture {
            snapshots: Arc::new(Mutex::new(Vec::new())),
            decision: Decision::Refuse,
        })
    )?);
    let solid = runtime.update_observed(2, vec![1], vec![0; 3], 1)?;
    assert!(solid.gradient_assessment()?.is_none());
    assert_eq!(snapshot.gradient_assessment()?, Some(report));
    Ok(())
}

#[test]
fn native_independent_field_surface_exposes_per_input_reports_and_validates_shape() -> TestResult {
    let runtime = compile_program_wire(
        include_bytes!("../../labcolors-core/tests/fixtures/independent-nominal-fields.lcpw")
            .to_vec(),
        8,
    )?;
    let observed = runtime.update_observed(1, vec![9], vec![255; 6], 2)?;
    assert!(observed.field_assessments()?.is_empty());

    let malformed = runtime.update_independent_gradients(
        2,
        vec![0; 5],
        vec![0, 1],
        vec![0, 2, 5],
        vec![0.0, 1.0, 0.0, 0.5, 1.0],
        vec![0; 15],
    );
    assert!(matches!(
        malformed,
        Err(LabColorsError::Refused { code, operation })
            if code == "program_update" && operation == "updateIndependentGradients"
    ));

    let retained = Arc::new(Mutex::new(Vec::new()));
    let accepted = runtime.update_independent_gradients_with(
        2,
        vec![0; 6],
        vec![0, 1],
        vec![0, 2, 5],
        vec![0.0, 1.0, 0.0, 0.5, 1.0],
        vec![0, 0, 0, 10, 10, 10, 20, 20, 20, 30, 30, 30, 40, 40, 40],
        Box::new(Capture {
            snapshots: Arc::clone(&retained),
            decision: Decision::Accept,
        }),
    )?;
    assert!(accepted);
    let snapshots = retained.lock().expect("snapshot storage");
    let assessments = snapshots[0].field_assessments()?;
    assert_eq!(assessments.len(), 2);
    assert_eq!(assessments[0].input_index, 0);
    assert_eq!(assessments[0].stops.len(), 2);
    assert_eq!(assessments[1].input_index, 1);
    assert_eq!(assessments[1].stops.len(), 3);
    let outputs = snapshots[0].copy_output_table()?;
    assert_eq!(outputs.len(), 2 * 5);
    let mut first_input_outputs = 0;
    let mut second_input_outputs = 0;
    for output in outputs.chunks_exact(5) {
        match output[0] as u32 {
            7 => {
                assert_eq!(&output[1..4], &[96.0; 3]);
                first_input_outputs += 1;
            }
            17 => {
                assert_eq!(&output[1..4], &[160.0; 3]);
                second_input_outputs += 1;
            }
            slot => panic!("unexpected output slot {slot}"),
        }
        assert_eq!(output[4], 1.0);
    }
    assert_eq!(first_input_outputs, 1);
    assert_eq!(second_input_outputs, 1);
    drop(snapshots);

    for decision in [Decision::Refuse, Decision::Error] {
        let runtime = compile_program_wire(
            include_bytes!("../../labcolors-core/tests/fixtures/independent-nominal-fields.lcpw")
                .to_vec(),
            8,
        )?;
        runtime.update_observed(1, vec![9], vec![255; 6], 2)?;
        let result = runtime.update_independent_gradients_with(
            2,
            vec![0; 6],
            vec![0, 1],
            vec![0, 2, 5],
            vec![0.0, 1.0, 0.0, 0.5, 1.0],
            vec![0, 0, 0, 10, 10, 10, 20, 20, 20, 30, 30, 30, 40, 40, 40],
            Box::new(Capture {
                snapshots: Arc::new(Mutex::new(Vec::new())),
                decision,
            }),
        );
        match decision {
            Decision::Refuse => assert!(!result?),
            Decision::Error => assert!(result.is_err()),
            Decision::Accept => unreachable!(),
        }
        // Both callback outcomes leave revision 1 current, so revision 2 is reusable.
        assert_eq!(
            runtime
                .update_observed(2, vec![9], vec![0; 6], 2)?
                .revision(),
            2
        );
    }
    Ok(())
}
