use super::*;
use crate::program_wire::{
    ProgramCleanlinessStatusV1, ProgramConstraintAssessmentV1, ProgramOptimalityV1,
    ProgramScenarioV1, ProgramSnapshotStateV1, compile_program_wire,
};

fn point(source: Srgb8, opacity: f64, hard: bool, computed: bool) -> ProgramWireBuilderV1 {
    let mut builder = ProgramWireBuilderV1::new();
    if computed {
        builder.computed_box_target(2, [0; 3], [255; 3], source, 2.0);
    } else {
        builder.source(1, source).fixed_target(2, 1);
    }
    builder
        .surface_input_port(6)
        .opacity_input(5, opacity)
        .solid_paint(3, 2)
        .opacity_paint(4, 3, 5)
        .input_surface(7, 6)
        .source_over_occurrence(8, 4, 7, 64.0, 0.2, 1)
        .nominal_srgb8_visible(hard, 10, 8)
        .output(17, 4);
    builder
}

#[test]
fn nominal_visible_accepts_unchanged_white_and_transparency_without_authority() -> Result<(), String>
{
    for computed in [false, true] {
        for opacity in [1.0, 0.5, 0.0] {
            let source = Srgb8::new([255; 3]);
            let targets = if computed { &[2][..] } else { &[][..] };
            let bytes = point(source, opacity, true, computed)
                .finish_assignment(targets)
                .map_err(|error| format!("encode: {error:?}"))?;
            let owner =
                compile_program_wire(&bytes).map_err(|error| format!("compile: {error:?}"))?;
            let mut session = owner
                .instantiate(1)
                .map_err(|error| format!("instantiate: {error:?}"))?;
            let snapshot = session
                .update_observed(1, &[ProgramScenarioV1::new(1, vec![source])])
                .map_err(|error| format!("update: {error:?}"))?;
            assert_eq!(snapshot.state(), ProgramSnapshotStateV1::Ready);
            assert_eq!(snapshot.optimality(), Some(ProgramOptimalityV1::Exact));
            assert_eq!(snapshot.outputs()[0].source(), source);
            assert_eq!(snapshot.outputs()[0].opacity().to_bits(), opacity.to_bits());
            let summary = snapshot.cleanliness_assessment().ok_or("current summary")?;
            assert_eq!(summary.status(), ProgramCleanlinessStatusV1::Conforms);
            assert_eq!((summary.accepted(), summary.unavailable()), (1, 0));
            let report = snapshot.assessment_report().ok_or("current report")?;
            assert_eq!(
                report.cells()[0].assessment(),
                ProgramConstraintAssessmentV1::NominalSrgb8VisiblePass { visible: source },
            );
        }
    }
    Ok(())
}

#[test]
fn nominal_visible_classifies_final_composite_and_preserves_report_only_result()
-> Result<(), String> {
    let source = Srgb8::new([255; 3]);
    for hard in [true, false] {
        let bytes = point(source, 0.5, hard, false)
            .finish_assignment(&[])
            .map_err(|error| format!("encode: {error:?}"))?;
        let owner = compile_program_wire(&bytes).map_err(|error| format!("compile: {error:?}"))?;
        let mut session = owner
            .instantiate(1)
            .map_err(|error| format!("instantiate: {error:?}"))?;
        let snapshot = session
            .update_observed(1, &[ProgramScenarioV1::new(1, vec![Srgb8::new([1, 1, 3])])])
            .map_err(|error| format!("update: {error:?}"))?;
        if hard {
            assert_eq!(snapshot.state(), ProgramSnapshotStateV1::Unsatisfied);
            assert!(snapshot.outputs().is_empty());
        } else {
            assert_eq!(snapshot.state(), ProgramSnapshotStateV1::Ready);
            assert_eq!(snapshot.outputs()[0].source(), source);
            let summary = snapshot.cleanliness_assessment().ok_or("current summary")?;
            assert_eq!(summary.status(), ProgramCleanlinessStatusV1::Violates);
            assert_eq!((summary.rejected(), summary.unavailable()), (1, 0));
            let report = snapshot.assessment_report().ok_or("current report")?;
            let ProgramConstraintAssessmentV1::NominalSrgb8VisibleRejected {
                visible,
                rejected_blue_interval: [lower, upper],
            } = report.cells()[0].assessment()
            else {
                return Err("typed nominal rejection".into());
            };
            assert_eq!(visible, Srgb8::new([128, 128, 129]));
            assert!((lower..=upper).contains(&129));
        }
    }
    Ok(())
}

#[test]
fn nominal_visible_v1_accepts_invisible_rejected_source_without_owned_domain() -> Result<(), String>
{
    let source = Srgb8::new([128, 128, 129]);
    let bytes = point(source, 0.0, true, false)
        .finish()
        .map_err(|error| format!("encode: {error:?}"))?;
    let owner = compile_program_wire(&bytes).map_err(|error| format!("compile: {error:?}"))?;
    let mut session = owner
        .instantiate(1)
        .map_err(|error| format!("instantiate: {error:?}"))?;
    let snapshot = session
        .update_observed(1, &[ProgramScenarioV1::new(1, vec![Srgb8::new([255; 3])])])
        .map_err(|error| format!("update: {error:?}"))?;
    assert_eq!(snapshot.state(), ProgramSnapshotStateV1::Ready);
    assert_eq!(snapshot.outputs()[0].source(), source);
    assert_eq!(snapshot.outputs()[0].opacity(), 0.0);
    Ok(())
}
