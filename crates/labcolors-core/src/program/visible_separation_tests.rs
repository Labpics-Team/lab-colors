use super::*;
use crate::program_wire::{
    ProgramAssessmentVerdictV1, ProgramConstraintAssessmentV1, ProgramOptimalityV1,
    ProgramRuntimeErrorV1, ProgramScenarioV1, ProgramSnapshotStateV1, compile_program_wire,
};

fn declaration(minimum: f64, maximum: f64, hard: bool) -> ProgramWireBuilderV1 {
    let mut builder = ProgramWireBuilderV1::new();
    builder
        .computed_gray_target(2, 128, 128, Srgb8::new([128; 3]), 2.0)
        .surface_input_port(6)
        .solid_paint(3, 2)
        .input_surface(7, 6)
        .source_over_occurrence(8, 3, 7, 64.0, 0.2, 1)
        .visible_separation(hard, 10, 8, minimum, maximum)
        .output(17, 3);
    builder
}

fn wire(builder: ProgramWireBuilderV1, targets: &[u32]) -> Result<Vec<u8>, String> {
    builder
        .finish_assignment(targets)
        .map_err(|error| format!("wire: {error:?}"))
}

#[test]
fn raw_wire_rejects_nonfinite_reversed_and_out_of_range_bounds() -> Result<(), String> {
    for (minimum, maximum) in [
        (f64::NAN, 1.0),
        (0.0, f64::NAN),
        (f64::NEG_INFINITY, 1.0),
        (0.0, f64::INFINITY),
        (-0.01, 1.0),
        (0.2, 0.1),
        (0.0, 2.0001),
    ] {
        assert!(matches!(
            compile_program_wire(&wire(declaration(minimum, maximum, true), &[2])?),
            Err(ProgramRuntimeErrorV1::Wire)
        ));
    }
    Ok(())
}

#[test]
fn pair_separation_wire_rejects_invalid_members_bounds_modes_and_versions() -> Result<(), String> {
    let bytes = |right, minimum: f64, maximum: f64, hard| -> Result<Vec<u8>, String> {
        let mut builder = declaration(0.0, 2.0, true);
        builder.source_over_occurrence(9, 3, 7, 64.0, 0.2, 1);
        let section = builder.constraint_section(hard);
        push_u32(section, 12);
        section.push(KIND_VISIBLE_PAIR_SEPARATION);
        push_u32(section, 8);
        push_u32(section, right);
        section.extend(minimum.to_le_bytes());
        section.extend(maximum.to_le_bytes());
        wire(builder, &[2])
    };
    assert!(compile_program_wire(&bytes(9, 0.0, 2.0, true)?).is_ok());
    for wire in [
        bytes(8, 0.0, 2.0, true)?,
        bytes(9, f64::NAN, 2.0, true)?,
        bytes(9, -0.1, 2.0, true)?,
        bytes(9, 1.0, 0.5, true)?,
        bytes(9, 0.0, 2.1, true)?,
        bytes(9, 0.0, 2.0, false)?,
    ] {
        assert!(matches!(
            compile_program_wire(&wire),
            Err(ProgramRuntimeErrorV1::Wire)
        ));
    }
    for version in [1_u16, 2] {
        let mut builder = ProgramWireBuilderV1::new();
        let section = builder.constraint_section(true);
        push_u32(section, 12);
        section.push(KIND_VISIBLE_PAIR_SEPARATION);
        push_u32(section, 8);
        push_u32(section, 9);
        section.extend(0.0_f64.to_le_bytes());
        section.extend(2.0_f64.to_le_bytes());
        let mut bytes = builder.finish().map_err(|e| format!("wire: {e:?}"))?;
        bytes[4..6].copy_from_slice(&version.to_le_bytes());
        assert!(matches!(
            compile_program_wire(&bytes),
            Err(ProgramRuntimeErrorV1::Wire)
        ));
    }
    Ok(())
}

#[test]
fn separation_is_v3_only_and_both_bounds_change_content_identity() -> Result<(), String> {
    let make = |minimum, maximum| -> Result<_, String> {
        compile_program_wire(&wire(declaration(minimum, maximum, true), &[2])?)
            .map_err(|error| format!("compile: {error:?}"))
    };
    let baseline = make(0.0, 0.5)?.content_identity();
    assert_eq!(baseline, make(-0.0, 0.5)?.content_identity());
    assert_ne!(baseline, make(0.01, 0.5)?.content_identity());
    assert_ne!(baseline, make(0.0, 0.51)?.content_identity());
    for version in [1_u16, 2] {
        let mut builder = ProgramWireBuilderV1::new();
        builder
            .source(1, Srgb8::new([128; 3]))
            .fixed_target(2, 1)
            .surface_input_port(6)
            .solid_paint(3, 2)
            .input_surface(7, 6)
            .source_over_occurrence(8, 3, 7, 64.0, 0.2, 1)
            .visible_separation(true, 10, 8, 0.0, 0.5)
            .output(17, 3);
        let mut bytes = builder
            .finish()
            .map_err(|error| format!("wire: {error:?}"))?;
        bytes[4..6].copy_from_slice(&version.to_le_bytes());
        assert!(matches!(
            compile_program_wire(&bytes),
            Err(ProgramRuntimeErrorV1::Wire)
        ));
    }
    Ok(())
}

#[test]
fn hard_conflict_has_no_outputs_and_report_only_never_vetoes() -> Result<(), String> {
    for hard in [true, false] {
        let owner = compile_program_wire(&wire(declaration(0.1, 0.2, hard), &[2])?)
            .map_err(|error| format!("compile: {error:?}"))?;
        let mut session = owner
            .instantiate(4)
            .map_err(|error| format!("instantiate: {error:?}"))?;
        let snapshot = session
            .update_observed(1, &[ProgramScenarioV1::new(9, vec![Srgb8::new([128; 3])])])
            .map_err(|error| format!("update: {error:?}"))?;
        if hard {
            assert_eq!(snapshot.state(), ProgramSnapshotStateV1::Unsatisfied);
            assert!(snapshot.outputs().is_empty());
            assert!(snapshot.optimality().is_none());
        } else {
            assert_eq!(snapshot.state(), ProgramSnapshotStateV1::Ready);
            assert_eq!(snapshot.optimality(), Some(ProgramOptimalityV1::Exact));
            let report = snapshot.assessment_report().ok_or("ready report")?;
            let cell = report.cells().first().ok_or("one report cell")?;
            assert!(!cell.hard());
            let ProgramConstraintAssessmentV1::VisibleSeparation {
                verdict,
                measurement,
            } = cell.assessment()
            else {
                return Err("typed separation report".into());
            };
            assert_eq!(verdict, ProgramAssessmentVerdictV1::Violation);
            assert_eq!(measurement.distance(), 0.0);
            assert_eq!(measurement.visible(), measurement.backdrop());
        }
    }
    Ok(())
}

#[test]
fn zero_separation_on_fixed_program_accepts_equal_and_rejects_different_backdrop()
-> Result<(), String> {
    let mut builder = ProgramWireBuilderV1::new();
    builder
        .source(1, Srgb8::new([128; 3]))
        .fixed_target(2, 1)
        .surface_input_port(6)
        .solid_paint(3, 2)
        .input_surface(7, 6)
        .source_over_occurrence(8, 3, 7, 64.0, 0.2, 1)
        .visible_separation(true, 10, 8, 0.0, 0.0)
        .output(17, 3);
    let owner = compile_program_wire(&wire(builder, &[])?)
        .map_err(|error| format!("fixed V3 compile: {error:?}"))?;
    let mut session = owner
        .instantiate(4)
        .map_err(|error| format!("instantiate: {error:?}"))?;
    for (revision, background, state) in [
        (1, 128, ProgramSnapshotStateV1::Ready),
        (2, 255, ProgramSnapshotStateV1::Unsatisfied),
        (3, 128, ProgramSnapshotStateV1::Ready),
    ] {
        let result = session
            .update_observed(
                revision,
                &[ProgramScenarioV1::new(9, vec![Srgb8::new([background; 3])])],
            )
            .map_err(|error| format!("update: {error:?}"))?;
        assert_eq!(result.state(), state);
        if state == ProgramSnapshotStateV1::Ready {
            assert_eq!(result.optimality(), Some(ProgramOptimalityV1::Exact));
            assert_eq!(result.outputs()[0].source(), Srgb8::new([128; 3]));
        } else {
            assert!(result.outputs().is_empty());
        }
    }
    Ok(())
}
