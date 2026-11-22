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

fn rgb(gray: u8) -> ProgramRgb8 {
    ProgramRgb8 {
        red: gray,
        green: gray,
        blue: gray,
    }
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
fn native_acceptance_preserves_revision_on_false_and_error_then_returns_the_same_snapshot()
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
            Decision::Refuse => assert!(result?.is_none()),
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
    let accepted = runtime
        .update_observed_with(
            2,
            vec![9],
            vec![128; 3],
            1,
            Box::new(Capture {
                snapshots: Arc::clone(&retained),
                decision: Decision::Accept,
            }),
        )?
        .ok_or("expected accepted snapshot")?;
    let snapshots = retained.lock().expect("snapshot storage");
    assert_eq!(snapshots.len(), 3);
    assert!(Arc::ptr_eq(&accepted, &snapshots[2]));
    assert_ne!(accepted.copy_output_table()?, first_table);
    assert_eq!(
        snapshots[0].copy_output_table()?,
        accepted.copy_output_table()?
    );
    assert_eq!(
        snapshots[1].assessment_report()?,
        accepted.assessment_report()?
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
    let accepted = runtime
        .update_observed_with(
            2,
            vec![9],
            vec![128; 3],
            1,
            Box::new(Reentrant {
                runtime: Arc::downgrade(&runtime),
            }),
        )?
        .ok_or("accepted outer update")?;
    assert_eq!(accepted.revision(), 2);
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

#[path = "../../labcolors-core/src/test_support.rs"]
mod test_support;

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
fn native_commit_does_not_allocate_after_host_acceptance() -> TestResult {
    let (control, allocations) =
        test_support::measured_allocations(|| Vec::<u8>::with_capacity(64));
    assert_eq!(allocations, 1, "allocator observer must detect real work");
    drop(control);
    let runtime = runtime()?;
    runtime.update_observed(1, vec![9], vec![255; 3], 1)?;
    let (accepted, events) = test_support::measured_allocator_events(|| {
        runtime.update_observed_with(2, vec![9], vec![128; 3], 1, Box::new(FinalCheckpoint))
    });
    assert_eq!(events.alloc, 0);
    assert_eq!(events.realloc, 0);
    assert_eq!(accepted?.ok_or("accepted snapshot")?.revision(), 2);
    Ok(())
}
