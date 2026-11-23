use labcolors_core::Srgb8;
use labcolors_core::program_wire::{
    ProgramConstraintAssessmentV1, ProgramDiagnosticV1, ProgramScenarioV1, ProgramSnapshotStateV1,
    TonalPolarityV1, compile_program_wire,
};

#[test]
fn public_wire_preserves_reference_hierarchy_and_owned_measurements() {
    let owner = compile_program_wire(include_bytes!("fixtures/reference-hierarchy.lcpw")).unwrap();
    let mut session = owner.instantiate(8).unwrap();
    let snapshot = session
        .update_observed(1, &[ProgramScenarioV1::new(9, vec![Srgb8::new([0; 3])])])
        .unwrap();
    assert_eq!(snapshot.state(), ProgramSnapshotStateV1::Ready);
    let report = snapshot.assessment_report().unwrap();
    assert!(report.cells().iter().any(|cell| matches!(
        cell.assessment(),
        ProgramConstraintAssessmentV1::TonalHierarchy { .. }
    )));
    let pair = report.tonal_hierarchy()[0];
    assert_eq!(
        (
            pair.constraint_id(),
            pair.case_index(),
            pair.stronger_occurrence_id(),
            pair.weaker_occurrence_id()
        ),
        (30, 0, 5, 15)
    );
    assert!(pair.measurement().satisfied());
    assert_eq!(
        pair.measurement().stronger_polarity(),
        TonalPolarityV1::Lighter
    );
    assert_eq!(
        pair.measurement().weaker_polarity(),
        TonalPolarityV1::Lighter
    );
    assert!(pair.measurement().required_ratio() > 1.0);
    assert_eq!(
        pair.measurement().required_gap(),
        pair.measurement().required_ratio().ln()
    );
    assert_eq!(report.reference_selections().len(), 2);
    for (index, reference) in report.reference_selections().iter().enumerate() {
        assert_eq!(reference.target_id(), [10, 20][index]);
        assert_eq!(reference.occurrence_id(), [5, 15][index]);
        let capacity = reference.capacity().unwrap();
        assert!(capacity.complete());
        assert_eq!(capacity.ratio_lower(), 1.0);
        assert_eq!(capacity.ratio_upper(), 1.0);
        assert_eq!(reference.cases().len(), 1);
        let case = reference.cases()[0];
        assert_eq!(
            case.reference_visible(),
            Srgb8::new([[68; 3], [102; 3]][index])
        );
        assert_eq!(case.reference_backdrop(), Srgb8::new([255; 3]));
        assert_eq!(case.actual_visible(), snapshot.outputs()[index].source());
        assert_eq!(case.actual_backdrop(), Srgb8::new([0; 3]));
        assert!(case.actual_contrast() >= 4.5);
    }
    let reference_selections = report.reference_selections().to_vec();
    let restored = session
        .update_observed(2, &[ProgramScenarioV1::new(9, vec![Srgb8::new([255; 3])])])
        .unwrap();
    assert_eq!(restored.outputs()[0].source(), Srgb8::new([68; 3]));
    assert_eq!(restored.outputs()[1].source(), Srgb8::new([102; 3]));
    assert_eq!(
        snapshot.assessment_report().unwrap().tonal_hierarchy()[0],
        pair
    );
    drop(session);
    assert_eq!(
        snapshot.assessment_report().unwrap().reference_selections(),
        reference_selections
    );
}

#[test]
fn fixed_relation_conflict_has_pair_evidence_and_recovers() {
    let owner =
        compile_program_wire(include_bytes!("fixtures/reference-hierarchy-fixed.lcpw")).unwrap();
    let mut session = owner.instantiate(8).unwrap();
    let conflict = session
        .update_observed(1, &[ProgramScenarioV1::new(9, vec![Srgb8::new([0; 3])])])
        .unwrap();
    assert_eq!(conflict.state(), ProgramSnapshotStateV1::Unsatisfied);
    assert!(conflict.outputs().is_empty());
    let witness = conflict.diagnostics().unwrap().witness().unwrap();
    assert!(
        matches!(witness.violations(), [ProgramDiagnosticV1::TonalHierarchy {
        stronger_occurrence_id: 5, weaker_occurrence_id: 15,
        stronger_polarity: TonalPolarityV1::Lighter, weaker_polarity: TonalPolarityV1::Lighter,
        required_gap, actual_gap, ..
    }] if actual_gap < required_gap)
    );
    let restored = session
        .update_observed(2, &[ProgramScenarioV1::new(9, vec![Srgb8::new([255; 3])])])
        .unwrap();
    assert_eq!(restored.state(), ProgramSnapshotStateV1::Ready);
    assert!(
        restored.assessment_report().unwrap().tonal_hierarchy()[0]
            .measurement()
            .satisfied()
    );
}
