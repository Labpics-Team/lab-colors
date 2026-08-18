//! Контракт категориальных и distinction-отношений Program (V5d).
//!
//! Категория для Core — объявленный точный family-образ без клиентской
//! семантики. Каждый закон требует положительного свидетельства; отрицательное
//! дополнение чужой проверки не образует ни категорию, ни различие.

use crate::family::FamilyDefinitionDigestV2;
use crate::family_artifact::{
    AdmittedFamilyArtifactV2, FamilyArtifactBundleV2, FamilyArtifactLoaderV1,
    encode_raw_bitmap24_family_artifact_v2_for_test,
};
use crate::lcs_occurrence::ColorSignal;
use crate::program_boundary_tests::CommitProgramUpdateForTest as _;
use crate::{Srgb8, program};

struct LoadedCategoryFixtureV1 {
    semantic: program::FamilySemanticReleaseV2,
    artifact: AdmittedFamilyArtifactV2,
}

fn category_family(definition: &[u8], values: &[[u8; 3]]) -> LoadedCategoryFixtureV1 {
    let members = values
        .iter()
        .copied()
        .map(Srgb8::new)
        .map(ColorSignal::from_srgb8)
        .collect::<Vec<_>>();
    let definition = FamilyDefinitionDigestV2::from_fixture_bytes_v2(definition);
    let (certificate, encoded) =
        encode_raw_bitmap24_family_artifact_v2_for_test(definition, &members).unwrap();
    let semantic = program::FamilySemanticReleaseV2::from_core(certificate.semantic_release());
    let artifact = FamilyArtifactLoaderV1::load(certificate, encoded).unwrap();
    LoadedCategoryFixtureV1 { semantic, artifact }
}

fn context() -> program::AppearanceContextV1 {
    program::AppearanceContextV1::try_new(64.0, 0.2, program::SurroundV1::Average).unwrap()
}

const REFERENCE_SOURCE: program::SourceIdV1 = program::SourceIdV1::new(1);
const CANDIDATE_SOURCE: program::SourceIdV1 = program::SourceIdV1::new(2);
const REFERENCE_TARGET: program::TargetIdV1 = program::TargetIdV1::new(3);
const CANDIDATE_TARGET: program::TargetIdV1 = program::TargetIdV1::new(4);
const REFERENCE_PAINT: program::PaintIdV1 = program::PaintIdV1::new(5);
const CANDIDATE_PAINT: program::PaintIdV1 = program::PaintIdV1::new(6);
const PORT: program::SurfaceInputPortIdV1 = program::SurfaceInputPortIdV1::new(7);
const SURFACE: program::SurfaceIdV1 = program::SurfaceIdV1::new(8);
const REFERENCE_OCCURRENCE: program::OccurrenceIdV1 = program::OccurrenceIdV1::new(9);
const CANDIDATE_OCCURRENCE: program::OccurrenceIdV1 = program::OccurrenceIdV1::new(10);
const MEMBERSHIP_FAMILY: program::FamilyIdV1 = program::FamilyIdV1::new(11);
const CATEGORY_FAMILY: program::FamilyIdV1 = program::FamilyIdV1::new(12);
const MEMBERSHIP_CONSTRAINT: program::ConstraintIdV1 = program::ConstraintIdV1::new(13);
const CATEGORY_CONSTRAINT: program::ConstraintIdV1 = program::ConstraintIdV1::new(14);
const VISIBLE_CONSTRAINT: program::ConstraintIdV1 = program::ConstraintIdV1::new(15);
const OUTPUT: program::OutputSlotIdV1 = program::OutputSlotIdV1::new(16);

/// Двухцелевой fixed-граф: reference и candidate paints поверх одного surface.
fn fixed_pair_draft(reference: Srgb8, candidate: Srgb8) -> program::DraftV1 {
    let mut draft = program::DraftV1::new();
    draft.push_source(REFERENCE_SOURCE, reference);
    draft.push_source(CANDIDATE_SOURCE, candidate);
    draft.push_fixed_target(REFERENCE_TARGET, REFERENCE_SOURCE);
    draft.push_fixed_target(CANDIDATE_TARGET, CANDIDATE_SOURCE);
    draft.push_solid_paint(REFERENCE_PAINT, REFERENCE_TARGET);
    draft.push_solid_paint(CANDIDATE_PAINT, CANDIDATE_TARGET);
    draft.push_surface_input_port(PORT);
    draft.push_input_surface(SURFACE, PORT);
    draft.push_source_over_occurrence(REFERENCE_OCCURRENCE, REFERENCE_PAINT, SURFACE, context());
    draft.push_source_over_occurrence(CANDIDATE_OCCURRENCE, CANDIDATE_PAINT, SURFACE, context());
    draft.push_output(OUTPUT, CANDIDATE_PAINT);
    draft
}

fn pair_relation() -> program::DirectedRelationV1<program::TargetIdV1> {
    program::DirectedRelationV1::try_new(REFERENCE_TARGET, vec![CANDIDATE_TARGET]).unwrap()
}

fn commit_single_scenario(owner: &program::OwnerV1, session: &mut program::SessionV1) {
    let black = [Srgb8::new([0; 3])];
    let scenarios = [program::ScenarioV1::new(17, &black)];
    owner
        .commit(
            session,
            program::UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
}

/// Проецирует клетки первого сертификата в пары `(constraint, assessment)`,
/// не различая Verified/Conflict формы.
fn certificate_cells<'a>(
    certificate: &program::CertificateV1<'a>,
) -> Vec<(program::ConstraintIdV1, program::AssessmentV1<'a>)> {
    match certificate {
        program::CertificateV1::Verified(verified) => verified
            .cells()
            .map(|cell| (cell.constraint(), cell.assessment()))
            .collect(),
        program::CertificateV1::Conflict(conflict) => conflict
            .cells()
            .map(|cell| (cell.constraint(), cell.assessment()))
            .collect(),
    }
}

#[test]
fn candidate_that_keeps_family_bytes_but_loses_declared_category_violates() {
    // Кандидат остаётся точным членом membership-family (его exact bytes
    // сохранены), но объявленная категория — другой family-образ, в котором
    // кандидата нет. Положительная категория обязана нарушиться, при этом
    // membership того же кандидата остаётся Pass: категория не выводится из
    // чужого положительного свидетельства.
    let reference = Srgb8::new([0, 0, 10]);
    let candidate = Srgb8::new([0, 0, 20]);
    let membership = category_family(b"v5d/membership", &[[0, 0, 10], [0, 0, 20]]);
    let category = category_family(b"v5d/category", &[[0, 0, 10], [0, 0, 30]]);
    let mut draft = fixed_pair_draft(reference, candidate);
    draft.push_family(MEMBERSHIP_FAMILY, membership.semantic);
    draft.push_family(CATEGORY_FAMILY, category.semantic);
    draft.push_intrinsic_family_membership_report_only(
        MEMBERSHIP_CONSTRAINT,
        CANDIDATE_TARGET,
        MEMBERSHIP_FAMILY,
    );
    draft.push_intrinsic_family_category_relation_hard(
        CATEGORY_CONSTRAINT,
        pair_relation(),
        CATEGORY_FAMILY,
    );
    draft.push_exact_visible_unary_report_only(VISIBLE_CONSTRAINT, CANDIDATE_OCCURRENCE, candidate);

    let owner = draft.compile().unwrap();
    let mut session = owner
        .instantiate_with_family_artifacts(
            19,
            FamilyArtifactBundleV2::from_artifacts(vec![membership.artifact, category.artifact]),
        )
        .unwrap_or_else(|failure| panic!("artifact admission failed: {:?}", failure.cause()));
    commit_single_scenario(&owner, &mut session);
    let evidence = session.evidence();
    assert_eq!(evidence.kind(), program::StateKindV1::Failed);
    let Some(program::CertificateV1::Conflict(conflict)) = evidence.certificates().next() else {
        panic!("a hard category violation over the only state must be an exhaustive conflict");
    };
    let mut membership_verdict = None;
    let mut category_proof = None;
    for cell in conflict.cells() {
        if cell.constraint() == MEMBERSHIP_CONSTRAINT {
            membership_verdict = Some(cell.assessment().verdict());
        }
        if cell.constraint() == CATEGORY_CONSTRAINT {
            let program::AssessmentV1::Relation(relation) = cell.assessment() else {
                panic!("category evidence must remain relation evidence");
            };
            assert_eq!(relation.verdict(), program::VerdictV1::Violation);
            category_proof = Some(relation.members().next().unwrap().proof());
        }
    }
    assert_eq!(
        membership_verdict,
        Some(program::VerdictV1::Pass),
        "exact family bytes must stay members of the membership family",
    );
    assert_eq!(
        category_proof,
        Some(program::RelationMemberProofV1::FamilyCategoryViolation(
            program::FamilyCategoryViolationKindV1::CandidateEndpoint,
        )),
        "the violation must name the endpoint that lost the declared category",
    );
}

#[test]
fn category_pass_requires_positive_witness_for_every_endpoint() {
    // Негативное дополнение недостаточно: пара проходит только когда ОБА
    // endpoint несут собственный inclusion witness, и каждый непокрытый
    // endpoint называется типизированно.
    let inside_a = Srgb8::new([1, 2, 3]);
    let inside_b = Srgb8::new([4, 5, 6]);
    let outside = Srgb8::new([7, 8, 9]);
    let cases = [
        (inside_a, inside_b, program::VerdictV1::Pass, None),
        (
            inside_a,
            outside,
            program::VerdictV1::Violation,
            Some(program::FamilyCategoryViolationKindV1::CandidateEndpoint),
        ),
        (
            outside,
            inside_b,
            program::VerdictV1::Violation,
            Some(program::FamilyCategoryViolationKindV1::ReferenceEndpoint),
        ),
        (
            outside,
            outside,
            program::VerdictV1::Violation,
            Some(program::FamilyCategoryViolationKindV1::BothEndpoints),
        ),
    ];
    for (reference, candidate, expected_verdict, expected_violation) in cases {
        let category = category_family(b"v5d/positive-witness", &[[1, 2, 3], [4, 5, 6]]);
        let semantic = category.semantic;
        let mut draft = fixed_pair_draft(reference, candidate);
        draft.push_family(CATEGORY_FAMILY, semantic);
        draft.push_intrinsic_family_category_relation_hard(
            CATEGORY_CONSTRAINT,
            pair_relation(),
            CATEGORY_FAMILY,
        );
        draft.push_exact_visible_unary_report_only(
            VISIBLE_CONSTRAINT,
            CANDIDATE_OCCURRENCE,
            candidate,
        );
        let owner = draft.compile().unwrap();
        let mut session = owner
            .instantiate_with_family_artifacts(
                20,
                FamilyArtifactBundleV2::from_artifacts(vec![category.artifact]),
            )
            .unwrap_or_else(|failure| panic!("artifact admission failed: {:?}", failure.cause()));
        commit_single_scenario(&owner, &mut session);
        let evidence = session.evidence();
        let certificate = evidence.certificates().next().unwrap();
        let cells = certificate_cells(&certificate);
        let (_, assessment) = cells
            .iter()
            .find(|(constraint, _)| *constraint == CATEGORY_CONSTRAINT)
            .expect("category relation evidence must be retained");
        let program::AssessmentV1::Relation(relation) = assessment else {
            panic!("category evidence must remain relation evidence");
        };
        let relation = *relation;
        assert_eq!(relation.verdict(), expected_verdict, "{reference:?}");
        let member = relation.members().next().unwrap();
        match expected_violation {
            None => {
                assert_eq!(
                    member.proof(),
                    program::RelationMemberProofV1::FamilyCategoryPass,
                    "a pass must carry the positive category witness",
                );
                // Положительное измерение обоих endpoints связано с точным
                // semantic release категории — свидетельство, а не дополнение.
                let program::RelationMeasurementV1::FamilyCategory(measurement) =
                    member.measurement()
                else {
                    panic!("category members must retain typed category measurement");
                };
                assert_eq!(measurement.reference().semantic(), semantic);
                assert_eq!(measurement.candidate().semantic(), semantic);
                assert_eq!(measurement.reference().signal(), reference);
                assert_eq!(measurement.candidate().signal(), candidate);
            }
            Some(kind) => {
                assert_eq!(
                    member.proof(),
                    program::RelationMemberProofV1::FamilyCategoryViolation(kind),
                    "{reference:?}",
                );
            }
        }
    }
}

#[test]
fn distinction_requires_positive_byte_inequality_not_a_complement() {
    // Distinction — положительный факт неравенства encoded байтов; равная пара
    // нарушает и на intrinsic, и на visible уровне.
    let shared = Srgb8::new([0x40; 3]);
    let different = Srgb8::new([0x41; 3]);
    for (candidate, expected, proof) in [
        (
            different,
            program::VerdictV1::Pass,
            program::RelationMemberProofV1::ExactSrgb8DistinctionPass,
        ),
        (
            shared,
            program::VerdictV1::Violation,
            program::RelationMemberProofV1::ExactSrgb8DistinctionViolation,
        ),
    ] {
        for visible in [false, true] {
            let mut draft = fixed_pair_draft(shared, candidate);
            if visible {
                draft.push_exact_visible_distinction_hard(
                    CATEGORY_CONSTRAINT,
                    program::DirectedRelationV1::try_new(
                        REFERENCE_OCCURRENCE,
                        vec![CANDIDATE_OCCURRENCE],
                    )
                    .unwrap(),
                );
            } else {
                draft.push_exact_intrinsic_distinction_hard(CATEGORY_CONSTRAINT, pair_relation());
            }
            draft.push_exact_visible_unary_report_only(
                VISIBLE_CONSTRAINT,
                CANDIDATE_OCCURRENCE,
                candidate,
            );
            let owner = draft.compile().unwrap();
            let mut session = owner.instantiate(21).unwrap();
            commit_single_scenario(&owner, &mut session);
            let evidence = session.evidence();
            let certificate = evidence.certificates().next().unwrap();
            let cells = certificate_cells(&certificate);
            let (_, assessment) = cells
                .iter()
                .find(|(constraint, _)| *constraint == CATEGORY_CONSTRAINT)
                .expect("distinction evidence must be retained");
            let program::AssessmentV1::Relation(relation) = assessment else {
                panic!("distinction evidence must remain relation evidence");
            };
            let relation = *relation;
            assert_eq!(relation.verdict(), expected, "visible={visible}");
            let member = relation.members().next().unwrap();
            assert_eq!(member.proof(), proof, "visible={visible}");
            let program::RelationMeasurementV1::ExactSrgb8Distinction(measurement) =
                member.measurement()
            else {
                panic!("distinction members must retain the raw byte pair");
            };
            assert_eq!(measurement.reference(), shared);
            assert_eq!(measurement.candidate(), candidate);
        }
    }
}

fn category_relation_identity(offset: u32, reverse_candidates: bool) -> program::ContentIdentityV9 {
    let sources = [0, 1, 2].map(|index| program::SourceIdV1::new(offset + index));
    let targets = [0, 1, 2].map(|index| program::TargetIdV1::new(offset + 10 + index));
    let paints = [0, 1, 2].map(|index| program::PaintIdV1::new(offset + 20 + index));
    let occurrences = [0, 1, 2].map(|index| program::OccurrenceIdV1::new(offset + 30 + index));
    let port = program::SurfaceInputPortIdV1::new(offset + 40);
    let surface = program::SurfaceIdV1::new(offset + 41);
    let family = program::FamilyIdV1::new(offset + 42);
    let category = category_family(b"v5d/identity", &[[0x20, 0x20, 0x20]]);
    let mut draft = program::DraftV1::new();
    for index in 0..3 {
        draft.push_source(sources[index], Srgb8::new([0x20 + index as u8; 3]));
        draft.push_fixed_target(targets[index], sources[index]);
        draft.push_solid_paint(paints[index], targets[index]);
    }
    draft.push_family(family, category.semantic);
    draft.push_surface_input_port(port);
    draft.push_input_surface(surface, port);
    for index in 0..3 {
        draft.push_source_over_occurrence(occurrences[index], paints[index], surface, context());
    }
    let mut target_candidates = vec![targets[1], targets[2]];
    let mut occurrence_candidates = vec![occurrences[1], occurrences[2]];
    if reverse_candidates {
        target_candidates.reverse();
        occurrence_candidates.reverse();
    }
    draft.push_intrinsic_family_category_relation_hard(
        program::ConstraintIdV1::new(offset + 50),
        program::DirectedRelationV1::try_new(targets[0], target_candidates.clone()).unwrap(),
        // Категория объявляется через opaque FamilyIdV1 того же прогона.
        family,
    );
    draft.push_exact_intrinsic_distinction_hard(
        program::ConstraintIdV1::new(offset + 51),
        program::DirectedRelationV1::try_new(targets[0], target_candidates).unwrap(),
    );
    draft.push_exact_visible_distinction_hard(
        program::ConstraintIdV1::new(offset + 52),
        program::DirectedRelationV1::try_new(occurrences[0], occurrence_candidates).unwrap(),
    );
    draft.push_output(program::OutputSlotIdV1::new(offset + 53), paints[0]);
    draft.compile().unwrap().content_identity()
}

#[test]
fn category_and_distinction_identity_ignore_opaque_names_and_declaration_order() {
    let canonical = category_relation_identity(100, false);
    assert_eq!(
        canonical,
        category_relation_identity(1_000, false),
        "opaque ID renaming must not change content identity",
    );
    assert_eq!(
        canonical,
        category_relation_identity(100, true),
        "candidate declaration order must not change content identity",
    );
}

#[test]
fn category_distinction_and_exact_relations_have_distinct_content_identities() {
    // Три directional-закона над одной topology обязаны давать три разных
    // адреса содержимого: identity различает закон, а не только рёбра.
    let identity_for = |kind: u8| {
        let mut draft = fixed_pair_draft(Srgb8::new([0x20; 3]), Srgb8::new([0x21; 3]));
        match kind {
            0 => {
                draft.push_exact_intrinsic_relation_hard(CATEGORY_CONSTRAINT, pair_relation());
            }
            1 => {
                draft.push_exact_intrinsic_distinction_hard(CATEGORY_CONSTRAINT, pair_relation());
            }
            _ => {
                let category = category_family(b"v5d/distinct-laws", &[[0x20, 0x20, 0x20]]);
                draft.push_family(CATEGORY_FAMILY, category.semantic);
                draft.push_intrinsic_family_category_relation_hard(
                    CATEGORY_CONSTRAINT,
                    pair_relation(),
                    CATEGORY_FAMILY,
                );
            }
        }
        draft.push_exact_visible_unary_report_only(
            VISIBLE_CONSTRAINT,
            CANDIDATE_OCCURRENCE,
            Srgb8::new([0x21; 3]),
        );
        draft.compile().unwrap().content_identity()
    };
    let exact = identity_for(0);
    let distinction = identity_for(1);
    let category = identity_for(2);
    assert_ne!(exact, distinction);
    assert_ne!(exact, category);
    assert_ne!(distinction, category);
}

#[test]
fn category_identity_binds_the_exact_declared_family_edge() {
    // Программа объявляет две категории; закон, назвавший первую, обязан иметь
    // другой адрес содержимого, чем закон, назвавший вторую. Иначе identity
    // потеряла бы ребро constraint→family.
    let identity_for = |use_second: bool| {
        let first = category_family(b"v5d/edge-first", &[[1, 1, 1]]);
        let second = category_family(b"v5d/edge-second", &[[2, 2, 2]]);
        let second_family = program::FamilyIdV1::new(60);
        let first_anchor = program::ConstraintIdV1::new(61);
        let second_anchor = program::ConstraintIdV1::new(62);
        let mut draft = fixed_pair_draft(Srgb8::new([1; 3]), Srgb8::new([1; 3]));
        draft.push_family(CATEGORY_FAMILY, first.semantic);
        draft.push_family(second_family, second.semantic);
        // Обе категории заякорены константными membership-ограничениями, так
        // что программы различаются ТОЛЬКО тем, какую категорию называет
        // relation: без ребра constraint→family адреса бы совпали.
        draft.push_intrinsic_family_membership_report_only(
            first_anchor,
            CANDIDATE_TARGET,
            CATEGORY_FAMILY,
        );
        draft.push_intrinsic_family_membership_report_only(
            second_anchor,
            CANDIDATE_TARGET,
            second_family,
        );
        draft.push_intrinsic_family_category_relation_hard(
            CATEGORY_CONSTRAINT,
            pair_relation(),
            if use_second {
                second_family
            } else {
                CATEGORY_FAMILY
            },
        );
        draft.push_exact_visible_unary_report_only(
            VISIBLE_CONSTRAINT,
            CANDIDATE_OCCURRENCE,
            Srgb8::new([1; 3]),
        );
        draft.compile().unwrap().content_identity()
    };
    assert_ne!(
        identity_for(false),
        identity_for(true),
        "category identity must follow the exact declared family edge",
    );
}

#[test]
fn cleanliness_and_category_are_independent_hard_constraints_over_the_full_two_by_two() {
    // Cleanliness-принадлежность не образует категорию, а категория не
    // расширяет cleanliness: все четыре комбинации вердиктов достижимы.
    let clean_member = Srgb8::new([0, 200, 70]);
    let clean_nonmember = Srgb8::new([0, 200, 71]);
    let root = program::PresentationRootIdV1::new(22);
    let clean_constraint = program::ConstraintIdV1::new(23);
    let cases = [
        (
            clean_member,
            [[0, 200, 70]].as_slice(),
            program::StateKindV1::Ready,
            [program::VerdictV1::Pass, program::VerdictV1::Pass],
        ),
        (
            clean_nonmember,
            [[0, 200, 71]].as_slice(),
            program::StateKindV1::Failed,
            [program::VerdictV1::Pass, program::VerdictV1::Violation],
        ),
        (
            clean_member,
            [[255, 0, 255]].as_slice(),
            program::StateKindV1::Failed,
            [program::VerdictV1::Violation, program::VerdictV1::Pass],
        ),
        (
            clean_nonmember,
            [[255, 0, 255]].as_slice(),
            program::StateKindV1::Failed,
            [program::VerdictV1::Violation, program::VerdictV1::Violation],
        ),
    ];
    for (signal, category_members, expected_kind, expected_verdicts) in cases {
        let category = category_family(b"v5d/clean-vs-category", category_members);
        let mut draft = fixed_pair_draft(signal, signal);
        draft.push_family(CATEGORY_FAMILY, category.semantic);
        draft.push_point_presentation_root(root, CANDIDATE_OCCURRENCE);
        draft.push_point_presentation_target(root, CANDIDATE_OCCURRENCE);
        draft.push_intrinsic_family_category_relation_hard(
            CATEGORY_CONSTRAINT,
            pair_relation(),
            CATEGORY_FAMILY,
        );
        draft.push_declared_srgb8_clean_set_hard(clean_constraint, root, CANDIDATE_OCCURRENCE);
        let owner = draft.compile().unwrap();
        let mut session = owner
            .instantiate_with_family_artifacts(
                24,
                FamilyArtifactBundleV2::from_artifacts(vec![category.artifact]),
            )
            .unwrap_or_else(|failure| panic!("artifact admission failed: {:?}", failure.cause()));
        commit_single_scenario(&owner, &mut session);
        let evidence = session.evidence();
        assert_eq!(evidence.kind(), expected_kind, "{signal:?}");
        let certificate = evidence.certificates().next().unwrap();
        let mut verdicts = [program::VerdictV1::Violation; 2];
        for (constraint, assessment) in certificate_cells(&certificate) {
            if constraint == CATEGORY_CONSTRAINT {
                verdicts[0] = assessment.verdict();
            } else if constraint == clean_constraint {
                verdicts[1] = assessment.verdict();
            } else {
                panic!("unexpected constraint cell");
            }
        }
        assert_eq!(verdicts, expected_verdicts, "{signal:?}");
    }
}

#[test]
fn category_relation_filters_states_but_selection_stays_with_the_release_order() {
    // Категория — hard-фильтр допустимого множества; выбор внутри допустимого
    // множества принадлежит единственному авторскому total preorder. Оба
    // категориальных кандидата допустимы, выбирается первый по порядку
    // release, а не «более категориальный».
    let outside = program::TargetCandidateIdV1::new(30);
    let first_inside = program::TargetCandidateIdV1::new(31);
    let second_inside = program::TargetCandidateIdV1::new(32);
    let reference_signal = Srgb8::new([0, 0, 10]);
    let outside_signal = Srgb8::new([9, 9, 9]);
    let first_signal = Srgb8::new([0, 0, 20]);
    let second_signal = Srgb8::new([0, 0, 30]);
    let category = category_family(b"v5d/selection", &[[0, 0, 10], [0, 0, 20], [0, 0, 30]]);
    let finite_target = program::TargetIdV1::new(33);
    let finite_paint = program::PaintIdV1::new(34);
    let finite_occurrence = program::OccurrenceIdV1::new(35);
    let mut draft = program::DraftV1::new();
    draft.push_source(REFERENCE_SOURCE, reference_signal);
    draft.push_fixed_target(REFERENCE_TARGET, REFERENCE_SOURCE);
    draft.push_finite_target(
        finite_target,
        program::FinitePaintDomainV1::try_new(vec![
            program::TargetCandidateV1::new(outside, program::PaintValueV1::opaque(outside_signal)),
            program::TargetCandidateV1::new(
                first_inside,
                program::PaintValueV1::opaque(first_signal),
            ),
            program::TargetCandidateV1::new(
                second_inside,
                program::PaintValueV1::opaque(second_signal),
            ),
        ])
        .unwrap(),
    );
    draft
        .set_joint_selection(vec![
            program::JointStateV1::new(vec![program::JointChoiceV1::new(finite_target, outside)]),
            program::JointStateV1::new(vec![program::JointChoiceV1::new(
                finite_target,
                first_inside,
            )]),
            program::JointStateV1::new(vec![program::JointChoiceV1::new(
                finite_target,
                second_inside,
            )]),
        ])
        .unwrap();
    draft.push_family(CATEGORY_FAMILY, category.semantic);
    draft.push_solid_paint(REFERENCE_PAINT, REFERENCE_TARGET);
    draft.push_solid_paint(finite_paint, finite_target);
    draft.push_surface_input_port(PORT);
    draft.push_input_surface(SURFACE, PORT);
    draft.push_source_over_occurrence(REFERENCE_OCCURRENCE, REFERENCE_PAINT, SURFACE, context());
    draft.push_source_over_occurrence(finite_occurrence, finite_paint, SURFACE, context());
    draft.push_intrinsic_family_category_relation_hard(
        CATEGORY_CONSTRAINT,
        program::DirectedRelationV1::try_new(REFERENCE_TARGET, vec![finite_target]).unwrap(),
        CATEGORY_FAMILY,
    );
    draft.push_exact_visible_unary_report_only(VISIBLE_CONSTRAINT, finite_occurrence, first_signal);
    draft.push_output(OUTPUT, finite_paint);

    let owner = draft.compile().unwrap();
    let mut session = owner
        .instantiate_with_family_artifacts(
            36,
            FamilyArtifactBundleV2::from_artifacts(vec![category.artifact]),
        )
        .unwrap_or_else(|failure| panic!("artifact admission failed: {:?}", failure.cause()));
    commit_single_scenario(&owner, &mut session);
    let evidence = session.evidence();
    let Some(program::CertificateV1::Verified(verified)) = evidence.certificates().next() else {
        panic!("two category members remain hard-feasible");
    };
    // Состояние 0 (вне категории) отклонено жёстким законом; выбор между
    // допустимыми состояниями 1 и 2 делает только материализованный release.
    assert_eq!(verified.selected_state_index(), Some(1));
    assert_eq!(verified.outputs().next().unwrap().source(), first_signal);
    assert!(
        verified.selection_release_identity().is_some(),
        "finite selection must remain bound to the authored selection release",
    );
}

#[test]
fn category_relation_without_declared_family_is_a_typed_compile_error() {
    let mut draft = fixed_pair_draft(Srgb8::new([1; 3]), Srgb8::new([2; 3]));
    draft.push_intrinsic_family_category_relation_hard(
        CATEGORY_CONSTRAINT,
        pair_relation(),
        CATEGORY_FAMILY,
    );
    draft.push_exact_visible_unary_report_only(
        VISIBLE_CONSTRAINT,
        CANDIDATE_OCCURRENCE,
        Srgb8::new([2; 3]),
    );
    let error = match draft.compile() {
        Ok(_) => panic!("an unresolved category family must not compile"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        program::CompileErrorV1::MissingConstraintFamily {
            constraint: CATEGORY_CONSTRAINT,
            family: CATEGORY_FAMILY,
        },
    );
}

#[test]
fn category_family_declaration_is_used_not_unused() {
    // Категориальное отношение обязано учитывать family как использованный:
    // иначе валидный Program отвергался бы UnusedFamily.
    let build = |with_category_relation: bool| {
        let category = category_family(b"v5d/usage", &[[1, 1, 1]]);
        let mut draft = fixed_pair_draft(Srgb8::new([1; 3]), Srgb8::new([1; 3]));
        draft.push_family(CATEGORY_FAMILY, category.semantic);
        if with_category_relation {
            draft.push_intrinsic_family_category_relation_hard(
                CATEGORY_CONSTRAINT,
                pair_relation(),
                CATEGORY_FAMILY,
            );
        }
        draft.push_exact_visible_unary_report_only(
            VISIBLE_CONSTRAINT,
            CANDIDATE_OCCURRENCE,
            Srgb8::new([1; 3]),
        );
        draft.compile()
    };
    assert!(build(true).is_ok());
    // Анти-вакуум: без категориального отношения та же декларация family
    // остаётся неиспользованной и отвергается — учёт действительно исполняется.
    assert!(matches!(
        build(false),
        Err(program::CompileErrorV1::UnusedFamily {
            family: CATEGORY_FAMILY,
        }),
    ));
}

#[test]
fn category_relation_requires_the_loaded_artifact_before_a_session_exists() {
    let category = category_family(b"v5d/admission", &[[1, 1, 1]]);
    let mut draft = fixed_pair_draft(Srgb8::new([1; 3]), Srgb8::new([1; 3]));
    draft.push_family(CATEGORY_FAMILY, category.semantic);
    draft.push_intrinsic_family_category_relation_hard(
        CATEGORY_CONSTRAINT,
        pair_relation(),
        CATEGORY_FAMILY,
    );
    draft.push_exact_visible_unary_report_only(
        VISIBLE_CONSTRAINT,
        CANDIDATE_OCCURRENCE,
        Srgb8::new([1; 3]),
    );
    let owner = draft.compile().unwrap();
    let failure = match owner.instantiate_with_family_artifacts(37, FamilyArtifactBundleV2::empty())
    {
        Ok(_) => panic!("category semantics require the exact loaded artifact bundle"),
        Err(failure) => failure,
    };
    assert_eq!(
        failure.cause(),
        program::InstantiateErrorV1::FamilyArtifacts(program::FamilyArtifactErrorV2::Missing {
            semantic: category.semantic,
        }),
    );
}

#[test]
fn distinction_and_category_relations_reject_solver_dependent_references() {
    // Направленный reference не вправе двигаться с solver-состоянием и для
    // новых законов: класс дефекта закрыт для всего registry, не для одного
    // варианта.
    let finite_candidate = program::TargetCandidateIdV1::new(40);
    let build = |category: Option<program::FamilySemanticReleaseV2>| {
        let mut draft = program::DraftV1::new();
        draft.push_source(CANDIDATE_SOURCE, Srgb8::new([2; 3]));
        draft.push_fixed_target(CANDIDATE_TARGET, CANDIDATE_SOURCE);
        draft.push_finite_target(
            REFERENCE_TARGET,
            program::FinitePaintDomainV1::try_new(vec![program::TargetCandidateV1::new(
                finite_candidate,
                program::PaintValueV1::opaque(Srgb8::new([1; 3])),
            )])
            .unwrap(),
        );
        draft
            .set_joint_selection(vec![program::JointStateV1::new(vec![
                program::JointChoiceV1::new(REFERENCE_TARGET, finite_candidate),
            ])])
            .unwrap();
        draft.push_solid_paint(REFERENCE_PAINT, REFERENCE_TARGET);
        draft.push_solid_paint(CANDIDATE_PAINT, CANDIDATE_TARGET);
        draft.push_surface_input_port(PORT);
        draft.push_input_surface(SURFACE, PORT);
        draft.push_source_over_occurrence(
            REFERENCE_OCCURRENCE,
            REFERENCE_PAINT,
            SURFACE,
            context(),
        );
        draft.push_source_over_occurrence(
            CANDIDATE_OCCURRENCE,
            CANDIDATE_PAINT,
            SURFACE,
            context(),
        );
        draft.push_exact_visible_unary_hard(
            VISIBLE_CONSTRAINT,
            REFERENCE_OCCURRENCE,
            Srgb8::new([1; 3]),
        );
        if let Some(semantic) = category {
            draft.push_family(CATEGORY_FAMILY, semantic);
            draft.push_intrinsic_family_category_relation_hard(
                CATEGORY_CONSTRAINT,
                pair_relation(),
                CATEGORY_FAMILY,
            );
        } else {
            draft.push_exact_intrinsic_distinction_hard(CATEGORY_CONSTRAINT, pair_relation());
        }
        draft.push_output(OUTPUT, REFERENCE_PAINT);
        draft
    };
    let distinction_error = match build(None).compile() {
        Ok(_) => panic!("a solver-dependent distinction reference must not compile"),
        Err(error) => error,
    };
    assert_eq!(
        distinction_error.kind(),
        program::CompileErrorKindV1::SolverDependentIntrinsicRelationReference,
    );
    let category = category_family(b"v5d/solver-dependent", &[[1, 1, 1], [2, 2, 2]]);
    let category_error = match build(Some(category.semantic)).compile() {
        Ok(_) => panic!("a solver-dependent category reference must not compile"),
        Err(error) => error,
    };
    assert_eq!(
        category_error.kind(),
        program::CompileErrorKindV1::SolverDependentIntrinsicRelationReference,
    );
}
