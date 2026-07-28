//! Минимальный внутренний контракт кандидата поверхности Program.

use crate::program_boundary_tests::CommitProgramUpdateForTest as _;
use crate::{Srgb8, program};
use proptest::prelude::*;

// Степень двойки даёт точную бинарную сетку с обеими публичными границами
// интервала, не смешивая проверяемое свойство с десятичным округлением.
const OPACITY_GRID_DENOMINATOR: u16 = 1024;

#[test]
fn staged_program_api_is_module_qualified_without_transport_prefixes() {
    let source = program::SourceIdV1::new(1);
    let target = program::TargetIdV1::new(2);
    let input = program::SurfaceInputPortIdV1::new(3);
    let paint = program::PaintIdV1::new(4);
    let surface = program::SurfaceIdV1::new(5);
    let occurrence = program::OccurrenceIdV1::new(6);
    let root = program::PresentationRootIdV1::new(9);
    let constraint = program::ConstraintIdV1::new(7);
    let output = program::OutputSlotIdV1::new(8);
    let context =
        program::AppearanceContextV1::try_new(64.0, 0.2, program::SurroundV1::Average).unwrap();
    let mut draft = program::DraftV1::new();

    draft.push_source(source, Srgb8::new([0, 0, 0]));
    draft.push_fixed_target(target, source);
    draft.push_surface_input_port(input);
    draft.push_solid_paint(paint, target);
    draft.push_input_surface(surface, input);
    draft.push_source_over_occurrence(occurrence, paint, surface, context);
    draft.push_point_presentation_root(root, occurrence);
    draft.push_point_presentation_target(root, occurrence);
    draft.push_exact_visible_unary_hard(constraint, occurrence, Srgb8::new([0, 0, 0]));
    draft.push_output(output, paint);

    let owner = draft.compile().unwrap();
    assert_eq!(owner.point_presentation_count(), 1);
    let mut session = owner.instantiate(1).unwrap();
    let white = [Srgb8::new([255, 255, 255])];
    let scenarios = [program::ScenarioV1::new(1, &white)];
    let projection = owner
        .commit(
            &mut session,
            program::UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            },
        )
        .unwrap();
    let mut certificates = projection.certificates();
    let Some(program::CertificateV1::Verified(certificate)) = certificates.next() else {
        panic!("the exact program must retain one Verified certificate");
    };
    assert!(certificates.next().is_none());
    let mut outputs = certificate.outputs();
    let Some(result) = outputs.next() else {
        panic!("the exact program must retain one certified Paint output");
    };
    assert!(outputs.next().is_none());
    assert_eq!(result.output_slot(), output);
    assert_eq!(result.source(), Srgb8::new([0, 0, 0]));
}

#[test]
fn staged_internal_failure_keeps_fact_and_contract_as_one_consistent_value() {
    let source = program::UpdateInvariantFailureV1::OwnerAuthority;
    assert_eq!(
        source.contract(),
        program::UpdateInvariantV1::OwnerAuthority
    );

    let error = program::UpdateErrorV1::InternalInvariant { source };
    assert_eq!(error.kind(), program::UpdateErrorKindV1::InternalInvariant);
}

#[test]
fn finite_paint_value_admission_is_typed_and_canonical() {
    for opacity in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            program::PaintValueV1::try_new(Srgb8::new([1, 2, 3]), opacity),
            Err(program::PaintValueErrorV1::NonFiniteOpacity),
        );
    }
    for opacity in [-f64::EPSILON, 1.0 + f64::EPSILON] {
        assert_eq!(
            program::PaintValueV1::try_new(Srgb8::new([1, 2, 3]), opacity),
            Err(program::PaintValueErrorV1::OpacityOutsideUnitInterval),
        );
    }

    let canonical = program::PaintValueV1::try_new(Srgb8::new([1, 2, 3]), -0.0).unwrap();
    assert_eq!(canonical.source(), Srgb8::new([1, 2, 3]));
    assert_eq!(canonical.opacity().to_bits(), 0.0_f64.to_bits());
}

#[test]
fn finite_paint_domain_is_nonempty_before_it_can_enter_a_draft() {
    assert_eq!(
        program::FinitePaintDomainV1::try_new(Vec::new()),
        Err(program::FinitePaintDomainErrorV1::Empty),
    );

    let candidate = program::TargetCandidateV1::new(
        program::TargetCandidateIdV1::new(1),
        program::PaintValueV1::opaque(Srgb8::new([1, 2, 3])),
    );
    assert!(program::FinitePaintDomainV1::try_new(vec![candidate]).is_ok());
}

proptest! {
    #[test]
    fn every_nonempty_finite_paint_domain_is_admitted(
        source in any::<[u8; 3]>(),
        candidate_count in 1_usize..64,
    ) {
        let candidates = (0..candidate_count)
            .map(|index| {
                program::TargetCandidateV1::new(
                    program::TargetCandidateIdV1::new(index as u32),
                    program::PaintValueV1::opaque(Srgb8::new(source)),
                )
            })
            .collect();

        prop_assert!(program::FinitePaintDomainV1::try_new(candidates).is_ok());
    }

    #[test]
    fn admitted_finite_paint_value_round_trips_source_and_unit_opacity(
        source in any::<[u8; 3]>(),
        opacity_numerator in 0_u16..=OPACITY_GRID_DENOMINATOR,
    ) {
        let opacity = f64::from(opacity_numerator) / f64::from(OPACITY_GRID_DENOMINATOR);
        let value = program::PaintValueV1::try_new(Srgb8::new(source), opacity).unwrap();

        prop_assert_eq!(value.source(), Srgb8::new(source));
        prop_assert_eq!(value.opacity().to_bits(), opacity.to_bits());
    }
}
