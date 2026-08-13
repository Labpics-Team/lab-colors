//! Исполняемый exit-гейт V5: слои V5a–V5d составляются в одной Program.
//!
//! Гейт строит один составной Program через production-путь SelectionRelease
//! (без тестового шва) и требует, чтобы каждый слой оставил своё точное
//! свидетельство: доверенный family artifact (V5a), расщепление semantic
//! release и artifact receipt (V5b), визуальные/intrinsic отношения и
//! единственный авторский порядок над hard-допустимым множеством (V5c),
//! категориальные и distinction-адаптеры с положительными сертификатами (V5d).
//! Пропуск любой оси — release, context, scenario или identity — валит гейт.

use crate::family::FamilyDefinitionDigestV2;
use crate::family_artifact::{
    AdmittedFamilyArtifactV2, FamilyArtifactBundleV2, FamilyArtifactLoaderV1,
    FixtureFamilyArtifactCodecV1, encode_fixture_family_artifact_v2,
    encode_raw_bitmap24_family_artifact_v2_for_test,
};
use crate::lcs_occurrence::ColorSignal;
use crate::program_boundary_tests::CommitProgramUpdateForTest as _;
use crate::program_session::{JointCandidateStateV1, TargetCandidateChoiceV1};
use crate::program_session::{TargetCandidateId, TargetId};
use crate::selection_release::{
    SelectionCandidateKeyV1, SelectionReleaseV1, admit_selection_release_v1,
    materialise_joint_selection_v1,
};
use crate::{Srgb8, program};

const REFERENCE_SOURCE: program::SourceIdV1 = program::SourceIdV1::new(1);
const REFERENCE_TARGET: program::TargetIdV1 = program::TargetIdV1::new(2);
const FINITE_TARGET: program::TargetIdV1 = program::TargetIdV1::new(3);
const TWIN_SOURCE: program::SourceIdV1 = program::SourceIdV1::new(25);
const TWIN_TARGET: program::TargetIdV1 = program::TargetIdV1::new(26);
const TWIN_PAINT: program::PaintIdV1 = program::PaintIdV1::new(27);
const OUTSIDE_CANDIDATE: program::TargetCandidateIdV1 = program::TargetCandidateIdV1::new(4);
const INSIDE_CANDIDATE: program::TargetCandidateIdV1 = program::TargetCandidateIdV1::new(5);
const REFERENCE_PAINT: program::PaintIdV1 = program::PaintIdV1::new(6);
const FINITE_PAINT: program::PaintIdV1 = program::PaintIdV1::new(7);
const PORT: program::SurfaceInputPortIdV1 = program::SurfaceInputPortIdV1::new(8);
const SURFACE: program::SurfaceIdV1 = program::SurfaceIdV1::new(9);
const REFERENCE_OCCURRENCE: program::OccurrenceIdV1 = program::OccurrenceIdV1::new(10);
const FINITE_OCCURRENCE: program::OccurrenceIdV1 = program::OccurrenceIdV1::new(11);
const CATEGORY_FAMILY: program::FamilyIdV1 = program::FamilyIdV1::new(12);
const ROOT: program::PresentationRootIdV1 = program::PresentationRootIdV1::new(13);
const MEMBERSHIP_CONSTRAINT: program::ConstraintIdV1 = program::ConstraintIdV1::new(14);
const CATEGORY_CONSTRAINT: program::ConstraintIdV1 = program::ConstraintIdV1::new(15);
const DISTINCTION_CONSTRAINT: program::ConstraintIdV1 = program::ConstraintIdV1::new(16);
const EXACT_RELATION_CONSTRAINT: program::ConstraintIdV1 = program::ConstraintIdV1::new(17);
const CLEAN_CONSTRAINT: program::ConstraintIdV1 = program::ConstraintIdV1::new(18);
const VISIBLE_CONSTRAINT: program::ConstraintIdV1 = program::ConstraintIdV1::new(19);
const OUTPUT: program::OutputSlotIdV1 = program::OutputSlotIdV1::new(20);

// Категория: reference и inside-кандидат внутри, outside-кандидат вне.
// Внутренние сигналы ахроматичны: нейтральная ось clean-set принимает их,
// поэтому clean-предикат судит независимо от категории.
const REFERENCE_SIGNAL: [u8; 3] = [10, 10, 10];
const INSIDE_SIGNAL: [u8; 3] = [20, 20, 20];
const OUTSIDE_SIGNAL: [u8; 3] = [255, 0, 255];

struct GateFamilyV1 {
    semantic: program::FamilySemanticReleaseV2,
    artifact: AdmittedFamilyArtifactV2,
}

fn gate_family(codec: Option<FixtureFamilyArtifactCodecV1>) -> GateFamilyV1 {
    let members = [REFERENCE_SIGNAL, INSIDE_SIGNAL]
        .into_iter()
        .map(Srgb8::new)
        .map(ColorSignal::from_srgb8)
        .collect::<Vec<_>>();
    let definition = FamilyDefinitionDigestV2::from_fixture_bytes_v2(b"v5-exit-gate/category");
    match codec {
        None => {
            let (certificate, encoded) =
                encode_raw_bitmap24_family_artifact_v2_for_test(definition, &members).unwrap();
            GateFamilyV1 {
                semantic: program::FamilySemanticReleaseV2::from_core(
                    certificate.semantic_release(),
                ),
                artifact: FamilyArtifactLoaderV1::load(certificate, encoded).unwrap(),
            }
        }
        Some(codec) => {
            let (certificate, encoded) =
                encode_fixture_family_artifact_v2(definition, &members, codec).unwrap();
            GateFamilyV1 {
                semantic: program::FamilySemanticReleaseV2::from_core(
                    certificate.semantic_release(),
                ),
                artifact: FamilyArtifactLoaderV1::load_fixture(certificate, encoded).unwrap(),
            }
        }
    }
}

fn gate_context() -> program::AppearanceContextV1 {
    program::AppearanceContextV1::try_new(64.0, 0.2, program::SurroundV1::Average).unwrap()
}

/// Оси гейта, каждую из которых состав обязан нести. Пропуск одной оси меняет
/// либо компилируемость, либо адрес содержимого, либо состав сертификата.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GateAxisV1 {
    Complete,
    WithoutCategoryRelation,
    WithoutDistinctionRelation,
    WithoutExactRelation,
    WithoutCleanSet,
    WithoutMembership,
}

fn gate_draft(family: &GateFamilyV1, axis: GateAxisV1) -> program::DraftV1 {
    let mut draft = program::DraftV1::new();
    draft.push_source(REFERENCE_SOURCE, Srgb8::new(REFERENCE_SIGNAL));
    draft.push_fixed_target(REFERENCE_TARGET, REFERENCE_SOURCE);
    draft.push_finite_target(
        FINITE_TARGET,
        program::FinitePaintDomainV1::try_new(vec![
            program::TargetCandidateV1::new(
                OUTSIDE_CANDIDATE,
                program::PaintValueV1::opaque(Srgb8::new(OUTSIDE_SIGNAL)),
            ),
            program::TargetCandidateV1::new(
                INSIDE_CANDIDATE,
                program::PaintValueV1::opaque(Srgb8::new(INSIDE_SIGNAL)),
            ),
        ])
        .unwrap(),
    );
    // Единственный авторский порядок: production admission + materialisation.
    // Первый ранг отдаётся outside-кандидату, чтобы гейт доказал, что hard-
    // категория пересекает допустимое множество ДО применения порядка.
    let admitted = admit_selection_release_v1(SelectionReleaseV1::new(
        1,
        vec![
            vec![SelectionCandidateKeyV1::new(
                b"outside".to_vec().into_boxed_slice(),
            )]
            .into_boxed_slice(),
            vec![SelectionCandidateKeyV1::new(
                b"inside".to_vec().into_boxed_slice(),
            )]
            .into_boxed_slice(),
        ]
        .into_boxed_slice(),
    ))
    .unwrap();
    let selection = materialise_joint_selection_v1(
        &admitted,
        &[
            (
                JointCandidateStateV1::new(vec![TargetCandidateChoiceV1::new(
                    TargetId::new(FINITE_TARGET.value()),
                    TargetCandidateId::new(OUTSIDE_CANDIDATE.value()),
                )]),
                SelectionCandidateKeyV1::new(b"outside".to_vec().into_boxed_slice()),
            ),
            (
                JointCandidateStateV1::new(vec![TargetCandidateChoiceV1::new(
                    TargetId::new(FINITE_TARGET.value()),
                    TargetCandidateId::new(INSIDE_CANDIDATE.value()),
                )]),
                SelectionCandidateKeyV1::new(b"inside".to_vec().into_boxed_slice()),
            ),
        ],
    )
    .unwrap();
    draft.set_materialised_joint_selection(selection).unwrap();
    draft.push_family(CATEGORY_FAMILY, family.semantic);
    draft.push_solid_paint(REFERENCE_PAINT, REFERENCE_TARGET);
    draft.push_solid_paint(FINITE_PAINT, FINITE_TARGET);
    draft.push_surface_input_port(PORT);
    draft.push_input_surface(SURFACE, PORT);
    draft.push_source_over_occurrence(
        REFERENCE_OCCURRENCE,
        REFERENCE_PAINT,
        SURFACE,
        gate_context(),
    );
    draft.push_source_over_occurrence(FINITE_OCCURRENCE, FINITE_PAINT, SURFACE, gate_context());
    draft.push_point_presentation_root(ROOT, FINITE_OCCURRENCE);
    draft.push_point_presentation_target(ROOT, FINITE_OCCURRENCE);
    if axis != GateAxisV1::WithoutMembership {
        draft.push_intrinsic_family_membership_report_only(
            MEMBERSHIP_CONSTRAINT,
            FINITE_TARGET,
            CATEGORY_FAMILY,
        );
    }
    if axis != GateAxisV1::WithoutCategoryRelation {
        draft.push_intrinsic_family_category_relation_hard(
            CATEGORY_CONSTRAINT,
            program::DirectedRelationV1::try_new(REFERENCE_TARGET, vec![FINITE_TARGET]).unwrap(),
            CATEGORY_FAMILY,
        );
    } else {
        // Family остаётся использованным, чтобы ось проверяла именно закон.
        draft.push_intrinsic_family_membership_hard(
            program::ConstraintIdV1::new(21),
            FINITE_TARGET,
            CATEGORY_FAMILY,
        );
    }
    if axis != GateAxisV1::WithoutDistinctionRelation {
        draft.push_exact_visible_distinction_hard(
            DISTINCTION_CONSTRAINT,
            program::DirectedRelationV1::try_new(REFERENCE_OCCURRENCE, vec![FINITE_OCCURRENCE])
                .unwrap(),
        );
    }
    if axis != GateAxisV1::WithoutExactRelation {
        // Twin повторяет reference-байты: exact-отношение остаётся Pass и
        // сосуществует с distinction-отношением к другому кандидату.
        draft.push_source(TWIN_SOURCE, Srgb8::new(REFERENCE_SIGNAL));
        draft.push_fixed_target(TWIN_TARGET, TWIN_SOURCE);
        draft.push_solid_paint(TWIN_PAINT, TWIN_TARGET);
        draft.push_exact_intrinsic_relation_hard(
            EXACT_RELATION_CONSTRAINT,
            program::DirectedRelationV1::try_new(REFERENCE_TARGET, vec![TWIN_TARGET]).unwrap(),
        );
    }
    if axis != GateAxisV1::WithoutCleanSet {
        draft.push_declared_srgb8_clean_set_hard(CLEAN_CONSTRAINT, ROOT, FINITE_OCCURRENCE);
    }
    draft.push_exact_visible_unary_report_only(
        VISIBLE_CONSTRAINT,
        FINITE_OCCURRENCE,
        Srgb8::new(INSIDE_SIGNAL),
    );
    draft.push_output(OUTPUT, FINITE_PAINT);
    draft
}

struct GateRunV1 {
    kind: program::StateKindV1,
    selected_state_index: Option<usize>,
    selection_release_identity_present: bool,
    proofs: Vec<program::RelationMemberProofV1>,
    constraints: Vec<program::ConstraintIdV1>,
    scenario_case_count: usize,
}

fn run_gate(family: GateFamilyV1, axis: GateAxisV1) -> GateRunV1 {
    let owner = gate_draft(&family, axis).compile().unwrap();
    let mut session = owner
        .instantiate_with_family_artifacts(
            22,
            FamilyArtifactBundleV2::from_artifacts(vec![family.artifact]),
        )
        .unwrap_or_else(|failure| panic!("artifact admission failed: {:?}", failure.cause()));
    // Две физические сцены: relation-законы обязаны судить каждую.
    let backdrops = [[Srgb8::new([0; 3])], [Srgb8::new([32; 3])]];
    let scenarios = [
        program::ScenarioV1::new(23, &backdrops[0]),
        program::ScenarioV1::new(24, &backdrops[1]),
    ];
    owner
        .commit(
            &mut session,
            program::UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    let evidence = session.evidence();
    let kind = evidence.kind();
    let certificate = evidence.certificates().next().unwrap();
    let mut proofs = Vec::new();
    let mut constraints = Vec::new();
    let mut scenario_cases = std::collections::BTreeSet::new();
    let (selected_state_index, selection_release_identity_present) = match certificate {
        program::CertificateV1::Verified(verified) => {
            for cell in verified.cells() {
                constraints.push(cell.constraint());
                scenario_cases.insert(cell.case_index());
                if let program::AssessmentV1::Relation(relation) = cell.assessment() {
                    proofs.extend(relation.members().map(|member| member.proof()));
                }
            }
            (
                verified.selected_state_index(),
                verified.selection_release_identity().is_some(),
            )
        }
        program::CertificateV1::Conflict(conflict) => {
            for cell in conflict.cells() {
                constraints.push(cell.constraint());
                scenario_cases.insert(cell.case_index());
                if let program::AssessmentV1::Relation(relation) = cell.assessment() {
                    proofs.extend(relation.members().map(|member| member.proof()));
                }
            }
            (None, conflict.selection_release_identity().is_some())
        }
    };
    GateRunV1 {
        kind,
        selected_state_index,
        selection_release_identity_present,
        proofs,
        constraints,
        scenario_case_count: scenario_cases.len(),
    }
}

#[test]
fn v5_exit_gate_composes_every_layer_in_one_verified_program() {
    let run = run_gate(gate_family(None), GateAxisV1::Complete);
    // V5c: жёсткие законы пересекают допустимое множество, затем единственный
    // авторский порядок выбирает состояние. Первое по порядку состояние
    // (outside) отклонено категорией — выбран inside, а не first-declared.
    assert_eq!(run.kind, program::StateKindV1::Ready);
    assert_eq!(run.selected_state_index, Some(1));
    assert!(
        run.selection_release_identity_present,
        "the certificate must stay bound to the authored selection release",
    );
    // Сценарные оси: обе физические сцены судятся.
    assert_eq!(run.scenario_case_count, 2);
    // V5d: положительные сертификаты категории и различия присутствуют.
    assert!(
        run.proofs
            .contains(&program::RelationMemberProofV1::FamilyCategoryPass),
        "the selected state must carry a positive category certificate",
    );
    assert!(
        run.proofs
            .contains(&program::RelationMemberProofV1::ExactSrgb8DistinctionPass),
        "the selected state must carry a positive distinction certificate",
    );
    assert!(
        run.proofs
            .iter()
            .any(|proof| matches!(proof, program::RelationMemberProofV1::ExactSrgb8Pass)),
        "the homogeneous exact relation must retain its own pass",
    );
    // Каждая объявленная ось оставила свою клетку.
    for constraint in [
        MEMBERSHIP_CONSTRAINT,
        CATEGORY_CONSTRAINT,
        DISTINCTION_CONSTRAINT,
        EXACT_RELATION_CONSTRAINT,
        CLEAN_CONSTRAINT,
        VISIBLE_CONSTRAINT,
    ] {
        assert!(
            run.constraints.contains(&constraint),
            "the composed certificate lost the {constraint:?} axis",
        );
    }
}

#[test]
fn v5_exit_gate_fails_when_any_axis_is_omitted() {
    // Анти-вакуум гейта: состав без любой оси либо теряет обязательную клетку,
    // либо меняет адрес содержимого. Гейт не может пройти «случайно».
    let complete_identity = gate_draft(&gate_family(None), GateAxisV1::Complete)
        .compile()
        .unwrap()
        .content_identity();
    for axis in [
        GateAxisV1::WithoutCategoryRelation,
        GateAxisV1::WithoutDistinctionRelation,
        GateAxisV1::WithoutExactRelation,
        GateAxisV1::WithoutCleanSet,
        GateAxisV1::WithoutMembership,
    ] {
        let identity = gate_draft(&gate_family(None), axis)
            .compile()
            .unwrap()
            .content_identity();
        assert_ne!(
            identity, complete_identity,
            "omitting {axis:?} must change the executable content identity",
        );
    }
    // Пропуск release-оси — не деградация, а типизированный отказ компиляции:
    // конечная цель без материализованного порядка непредставима.
    let family = gate_family(None);
    let mut draft = program::DraftV1::new();
    draft.push_source(REFERENCE_SOURCE, Srgb8::new(REFERENCE_SIGNAL));
    draft.push_fixed_target(REFERENCE_TARGET, REFERENCE_SOURCE);
    draft.push_finite_target(
        FINITE_TARGET,
        program::FinitePaintDomainV1::try_new(vec![program::TargetCandidateV1::new(
            INSIDE_CANDIDATE,
            program::PaintValueV1::opaque(Srgb8::new(INSIDE_SIGNAL)),
        )])
        .unwrap(),
    );
    draft.push_family(CATEGORY_FAMILY, family.semantic);
    draft.push_solid_paint(FINITE_PAINT, FINITE_TARGET);
    draft.push_surface_input_port(PORT);
    draft.push_input_surface(SURFACE, PORT);
    draft.push_source_over_occurrence(FINITE_OCCURRENCE, FINITE_PAINT, SURFACE, gate_context());
    draft.push_intrinsic_family_membership_hard(
        MEMBERSHIP_CONSTRAINT,
        FINITE_TARGET,
        CATEGORY_FAMILY,
    );
    draft.push_exact_visible_unary_hard(
        VISIBLE_CONSTRAINT,
        FINITE_OCCURRENCE,
        Srgb8::new(INSIDE_SIGNAL),
    );
    draft.push_output(OUTPUT, FINITE_PAINT);
    let error = match draft.compile() {
        Ok(_) => panic!("a finite target without the authored release must not compile"),
        Err(error) => error,
    };
    assert_eq!(
        error.kind(),
        program::CompileErrorKindV1::MissingJointSelection,
    );
}

#[test]
fn v5_exit_gate_binds_semantics_to_the_release_not_the_transport_bytes() {
    // V5a/V5b: два lossless-кодека одного семантического release обязаны дать
    // одинаковый вердикт гейта и одинаковый content identity, при этом их
    // artifact receipts различаются — семантика отделена от транспорта.
    let canonical = gate_family(Some(FixtureFamilyArtifactCodecV1::CanonicalMembersV1));
    let reversed = gate_family(Some(FixtureFamilyArtifactCodecV1::ReversedMembersV1));
    assert_eq!(canonical.semantic, reversed.semantic);
    assert_ne!(
        canonical.artifact.artifact_receipt(),
        reversed.artifact.artifact_receipt(),
    );
    let canonical_identity = gate_draft(&canonical, GateAxisV1::Complete)
        .compile()
        .unwrap()
        .content_identity();
    let reversed_identity = gate_draft(&reversed, GateAxisV1::Complete)
        .compile()
        .unwrap()
        .content_identity();
    assert_eq!(canonical_identity, reversed_identity);

    let canonical_run = run_gate(canonical, GateAxisV1::Complete);
    let reversed_run = run_gate(reversed, GateAxisV1::Complete);
    assert_eq!(canonical_run.kind, program::StateKindV1::Ready);
    assert_eq!(canonical_run.kind, reversed_run.kind);
    assert_eq!(
        canonical_run.selected_state_index,
        reversed_run.selected_state_index,
    );
    assert_eq!(canonical_run.proofs, reversed_run.proofs);
}
