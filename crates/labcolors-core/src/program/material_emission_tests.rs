use super::*;
use crate::program_wire::{ProgramScenarioV1, ProgramSnapshotStateV1, compile_program_wire};

#[test]
fn transparent_use_has_exact_zero_separation_from_an_uncertain_material() -> Result<(), String> {
    let mut builder = ProgramWireBuilderV1::new();
    builder
        .source(1, Srgb8::new([246, 107, 57]))
        .fixed_target(2, 1)
        .solid_paint(3, 2)
        .opacity_input(4, 0.72)
        .opacity_paint(5, 3, 4)
        .surface_input_port(6)
        .input_surface(7, 6)
        .source_over_occurrence(8, 5, 7, 0.0, 0.0, 0)
        .occurrence_surface(9, 8)
        .opacity_input(10, 0.0)
        .opacity_paint(11, 3, 10)
        .source_over_occurrence(12, 11, 9, 0.0, 0.0, 0)
        .visible_separation(true, 13, 12, 0.0, 0.0)
        .output(17, 11);
    let bytes = builder
        .finish_assignment(&[])
        .map_err(|error| format!("wire: {error:?}"))?;
    let owner = compile_program_wire(&bytes).map_err(|error| format!("compile: {error:?}"))?;
    let mut session = owner
        .instantiate(1)
        .map_err(|error| format!("session: {error:?}"))?;
    let snapshot = session
        .update_observed(
            1,
            &[ProgramScenarioV1::new(1, vec![Srgb8::new([107, 127, 114])])],
        )
        .map_err(|error| format!("update: {error:?}"))?;
    assert_eq!(snapshot.state(), ProgramSnapshotStateV1::Ready);
    let report = snapshot.assessment_report().ok_or("report")?;
    let crate::program_wire::ProgramConstraintAssessmentV1::VisibleSeparation {
        measurement, ..
    } = report.cells().first().ok_or("cell")?.assessment()
    else {
        return Err("separation report".into());
    };
    assert_eq!(measurement.distance_bounds(), [0.0, 0.0]);
    Ok(())
}

fn notice(raster: bool, computed_ink: bool) -> Result<Vec<u8>, String> {
    let mut builder = ProgramWireBuilderV1::new();
    builder
        .source(1, Srgb8::new([246, 107, 57]))
        .fixed_target(2, 1)
        .solid_paint(3, 2)
        .opacity_input(4, 0.72)
        .opacity_paint(5, 3, 4)
        .surface_input_port(6)
        .input_surface(7, 6);
    let (adapting, background, surround) = if raster {
        (0.0, 0.0, 0)
    } else {
        (64.0, 0.2, 1)
    };
    builder
        .source_over_occurrence(8, 5, 7, adapting, background, surround)
        .occurrence_surface(9, 8);
    if computed_ink {
        builder.computed_gray_target(12, 0, 255, Srgb8::new([36; 3]), 2.0);
    } else {
        builder.source(11, Srgb8::new([36; 3])).fixed_target(12, 11);
    }
    builder
        .solid_paint(13, 12)
        .source_over_occurrence(14, 13, 9, adapting, background, surround)
        .wcag22_visible_unary(true, 15, 14, 1)
        .output(17, 5)
        .output(18, 13);
    builder
        .finish_assignment(if computed_ink { &[12] } else { &[] })
        .map_err(|error| format!("wire: {error:?}"))
}

#[test]
fn raster_admission_rejects_the_observed_false_positive_and_changes_identity() -> Result<(), String>
{
    let mut identities = Vec::new();
    for (raster, expected) in [
        (false, ProgramSnapshotStateV1::Ready),
        (true, ProgramSnapshotStateV1::Unsatisfied),
    ] {
        let owner = compile_program_wire(&notice(raster, false)?)
            .map_err(|error| format!("compile: {error:?}"))?;
        identities.push(owner.content_identity());
        let mut session = owner
            .instantiate(1)
            .map_err(|error| format!("session: {error:?}"))?;
        let snapshot = session
            .update_observed(
                1,
                &[ProgramScenarioV1::new(1, vec![Srgb8::new([107, 127, 114])])],
            )
            .map_err(|error| format!("update: {error:?}"))?;
        assert_eq!(snapshot.state(), expected);
        if raster {
            assert!(snapshot.outputs().is_empty());
        }
    }
    assert_ne!(identities[0], identities[1]);
    Ok(())
}

#[test]
fn search_finds_useful_ink_for_the_material_instead_of_stopping_at_nominal_success()
-> Result<(), String> {
    let owner = compile_program_wire(&notice(true, true)?)
        .map_err(|error| format!("compile: {error:?}"))?;
    let mut session = owner
        .instantiate(1)
        .map_err(|error| format!("session: {error:?}"))?;
    let snapshot = session
        .update_observed(
            1,
            &[ProgramScenarioV1::new(1, vec![Srgb8::new([107, 127, 114])])],
        )
        .map_err(|error| format!("update: {error:?}"))?;
    assert_eq!(snapshot.state(), ProgramSnapshotStateV1::Ready);
    let paint = snapshot
        .outputs()
        .iter()
        .find(|paint| paint.slot() == 17)
        .ok_or("material")?;
    assert_eq!(paint.source().bytes(), [246, 107, 57]);
    assert_eq!(paint.opacity(), 0.72);
    let ink = snapshot
        .outputs()
        .iter()
        .find(|paint| paint.slot() == 18)
        .ok_or("ink")?
        .source()
        .bytes();
    assert_ne!(ink, [36; 3]);
    for background in [[207, 112, 73], [204, 110, 70], [210, 116, 76]] {
        let assessment = crate::wcag22::evaluate_wcag22_srgb8(
            ink,
            background,
            crate::wcag22::Wcag22CriterionV1::Sc143TextDefault,
        )
        .map_err(|error| format!("actual pixel: {error:?}"))?;
        assert!(matches!(
            assessment,
            crate::wcag22::Wcag22AssessmentV1::Evaluated {
                decision: crate::wcag22::Wcag22ApplicableDecisionV1::Pass,
                ..
            }
        ));
    }
    Ok(())
}
