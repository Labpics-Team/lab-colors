//! RED contract for the sole concrete Core package seam.
//!
//! This integration crate deliberately has no access to Core-private generic
//! evaluator/session machinery. It must compile using only one hidden,
//! concrete package module once that seam is linked after the P3 + weak-owner
//! rebase.

use labcolors_core::Srgb8;
use labcolors_core::package_bridge::{
    PackageProgramCertificateV1, PackageProgramInstantiateErrorV1, PackageProgramOperationV1,
    PackageProgramOwnerV1, PackageProgramScenarioV1, PackageProgramSessionV1,
    PackageProgramStateKindV1, PackageProgramStateViewV1, PackageProgramUpdateErrorKindV1,
    PackageProgramUpdateV1,
};

fn exact_size<I: ExactSizeIterator>(iterator: I) -> I {
    iterator
}

#[allow(dead_code)]
fn wasm_can_use_only_the_concrete_owner_and_session(
    owner: &PackageProgramOwnerV1,
    session: &mut PackageProgramSessionV1,
    scenarios: &[PackageProgramScenarioV1<'_>],
) -> Result<(), PackageProgramInstantiateErrorV1> {
    let _independent_session = owner.instantiate(0xA11CE)?;
    let update = PackageProgramUpdateV1::Observed {
        revision: 1,
        scenarios,
    };
    let view = session.update(update).expect("well-formed update");
    assert_projection_is_linear(view);
    Ok(())
}

fn assert_projection_is_linear(view: PackageProgramStateViewV1<'_>) {
    let _kind: PackageProgramStateKindV1 = view.kind();
    let _revision: Option<u64> = view.revision();
    let certificates = exact_size(view.certificates());
    let certificate_count = certificates.len();
    for certificate in certificates {
        let _: PackageProgramCertificateV1<'_> = certificate;
    }
    for operation in exact_size(view.operations()) {
        match operation {
            PackageProgramOperationV1::Set {
                output_slot,
                source,
                opacity,
                certificate_index,
            } => {
                let _: u32 = output_slot;
                let _: Srgb8 = source;
                assert!(opacity.is_finite() && (0.0..=1.0).contains(&opacity));
                assert!(certificate_index < certificate_count);
            }
            PackageProgramOperationV1::Remove { output_slot } => {
                let _: u32 = output_slot;
            }
            PackageProgramOperationV1::Hold {
                output_slot,
                certificate_index,
            } => {
                let _: u32 = output_slot;
                assert!(certificate_index < certificate_count);
            }
        }
    }
}

#[allow(dead_code)]
fn unknown_is_revision_bound_without_a_stream_or_generation_field(
    session: &mut PackageProgramSessionV1,
) {
    let update = PackageProgramUpdateV1::Unknown {
        revision: 2,
        reason_id: 7,
    };
    let _ = session.update(update);
}

#[allow(dead_code)]
fn owner_expiry_is_a_closed_package_error(
    error: labcolors_core::package_bridge::PackageProgramUpdateErrorV1,
) {
    assert_eq!(error.kind(), PackageProgramUpdateErrorKindV1::OwnerExpired);
}

#[test]
fn red_contract_is_linked_by_the_concrete_package_module() {
    // Reaching this test means the external crate compiled without importing
    // Program<E>, evaluator traits, Session<Plan>, or numeric generations.
    assert_eq!(core::mem::size_of::<Srgb8>(), 3);
}
