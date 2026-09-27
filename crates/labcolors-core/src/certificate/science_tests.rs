use super::*;
use crate::Srgb8;
use crate::authority::clean_convention::{
    CleanConventionAdmissionKindV1, CleanConventionScopeV1, CleanConventionSelectionV1,
};
use crate::authority::evaluation::PointQualityProfileV1;
use crate::authority::test_support::{Host, point_attachment_for};
use crate::authority::{AuthorityExpectedCurrentV1, AuthorityStateV1};
use crate::clean_set::EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V1;
use crate::program_wire::{ProgramAttachmentV1, ProgramScenarioV1};
use crate::sha256::Hasher;

fn profile() -> PointQualityProfileV1 {
    PointQualityProfileV1::declared_point(
        CleanConventionSelectionV1::select(
            EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V1,
            CleanConventionScopeV1::ModeledSrgb8Point,
            CleanConventionAdmissionKindV1::DeclaredPackagePolicyCandidate,
        )
        .unwrap(),
    )
}
fn attachment() -> ProgramAttachmentV1<Host> {
    let mut value = point_attachment_for(Srgb8::new([64; 3]), 0.5, Srgb8::new([96; 3]));
    observe(&mut value, 1);
    value
}
fn observe(value: &mut ProgramAttachmentV1<Host>, revision: u64) {
    value
        .update_observed(
            revision,
            &[ProgramScenarioV1::new(1, vec![Srgb8::new([128; 3])])],
        )
        .unwrap();
}
fn install(state: &mut AuthorityStateV1, value: &ProgramAttachmentV1<Host>) {
    let tq = state.read(AuthorityIdV1::TechnicalQuality).map_or(
        AuthorityExpectedCurrentV1::Vacant,
        AuthorityExpectedCurrentV1::Exact,
    );
    let cc = state.read(AuthorityIdV1::CleanConvention).map_or(
        AuthorityExpectedCurrentV1::Vacant,
        AuthorityExpectedCurrentV1::Exact,
    );
    state
        .admit_modeled_point_technical_quality(value, tq)
        .unwrap();
    state
        .admit_modeled_point_clean_convention(value, Some(profile().convention()), cc)
        .unwrap();
}
fn digest(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut h = Hasher::new();
    h.update(domain);
    h.update(bytes);
    *h.finalize().as_bytes()
}
/// Независимый позиционный конструктор, не production encoder/attestation.
fn hostile_wire(key: &AdmissionKeyV1, payload: &[u8], authority: u8, kind: u8) -> Vec<u8> {
    let mut b = Vec::from(b"LCEN\x00\x01\x01");
    b.push(authority);
    b.extend(1_u16.to_be_bytes());
    for text in [key.runtime_artifact_id(), key.producer_revision()] {
        b.extend((text.len() as u16).to_be_bytes());
        b.extend(text.as_bytes());
    }
    b.extend(key.producer_content_identity());
    let context = key.context_id();
    b.extend((context.len() as u16).to_be_bytes());
    b.extend(context.as_bytes());
    b.push(kind);
    b.extend(1_u16.to_be_bytes());
    b.extend((payload.len() as u32).to_be_bytes());
    b.extend(payload);
    let hash = digest(b"labpics.colors/certificate-payload/v1\0", payload);
    b.extend(hash);
    let hash = digest(b"labpics.colors/certificate-envelope/v1\0", &b);
    b.extend(hash);
    b
}

#[test]
fn independent_wire_binds_exact_profile_subject_both_branches_and_color() {
    let a = attachment();
    let mut state = AuthorityStateV1::new();
    install(&mut state, &a);
    let evaluated = state
        .evaluate_declared_modeled_point(&a, Some(profile()))
        .unwrap();
    let issued = DeclaredPointCertificateV1::issue(&evaluated).unwrap();
    let decoded =
        UntrustedEnvelopeV1::decode_class(issued.as_bytes(), EnvelopeClassV1::DeclaredPoint)
            .unwrap();
    let b = decoded.payload_bytes();
    assert_eq!(b.len(), 302);
    assert_eq!(&b[..6], b"LCPQ\x00\x01");
    assert_eq!(&b[6..38], profile().identity());
    assert_eq!(&b[38..70], EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V1);
    assert_eq!(&b[70..73], &[1, 1, 0]);
    assert_eq!(&b[73..105], evaluated.subject_identity());
    assert_eq!(b[105], 1);
    assert_eq!(b[202], 2);
    for (offset, owner) in [(106, evaluated.technical()), (203, evaluated.convention())] {
        assert_eq!(&b[offset..offset + 32], owner.release_identity().as_bytes());
        assert_eq!(
            &b[offset + 32..offset + 64],
            owner.applicability_identity().as_bytes()
        );
        assert_eq!(
            &b[offset + 64..offset + 96],
            owner.provenance_identity().as_bytes()
        );
    }
    assert_eq!(&b[299..], &[96; 3]);
    let independent = hostile_wire(decoded.admission_key(), b, 4, 2);
    assert_eq!(issued.as_bytes(), independent);
    let verified = VerifiedPointCertificateV1::verify(&independent, &evaluated).unwrap();
    assert!(core::ptr::eq(verified.evaluation(), &evaluated));
    assert_eq!(verified.as_bytes(), issued.as_bytes());
    assert!(core::ptr::eq(issued.evaluation(), &evaluated));
    assert_eq!(issued.try_to_bytes().unwrap(), independent);
    assert_eq!(
        decoded.admission_key().try_clone().unwrap(),
        *decoded.admission_key()
    );
}

#[test]
fn every_payload_byte_with_recomputed_hashes_is_rejected() {
    let a = attachment();
    let mut state = AuthorityStateV1::new();
    install(&mut state, &a);
    let evaluated = state
        .evaluate_declared_modeled_point(&a, Some(profile()))
        .unwrap();
    let issued = DeclaredPointCertificateV1::issue(&evaluated).unwrap();
    let parsed =
        UntrustedEnvelopeV1::decode_class(issued.as_bytes(), EnvelopeClassV1::DeclaredPoint)
            .unwrap();
    for index in 0..302 {
        let mut body = parsed.payload_bytes().to_vec();
        body[index] ^= 1;
        let forged = hostile_wire(parsed.admission_key(), &body, 4, 2);
        assert!(
            UntrustedEnvelopeV1::decode_class(&forged, EnvelopeClassV1::DeclaredPoint).is_ok(),
            "hashes valid for {index}"
        );
        assert!(
            VerifiedPointCertificateV1::verify(&forged, &evaluated).is_err(),
            "forged byte {index}"
        );
    }
    assert!(VerifiedPointCertificateV1::verify(issued.as_bytes(), &evaluated).is_ok());
}

#[test]
fn generic_transport_and_semantic_certificate_never_change_classes_implicitly() {
    let a = attachment();
    let mut state = AuthorityStateV1::new();
    install(&mut state, &a);
    let evaluated = state
        .evaluate_declared_modeled_point(&a, Some(profile()))
        .unwrap();
    let issued = DeclaredPointCertificateV1::issue(&evaluated).unwrap();
    assert_eq!(
        UntrustedEnvelopeV1::decode(issued.as_bytes()).unwrap_err(),
        CertificateErrorV1::UnknownAuthorityKind
    );
    let transport = super::super::issue_source_certificate_v1().unwrap();
    assert_eq!(
        VerifiedPointCertificateV1::verify(transport.as_bytes(), &evaluated).unwrap_err(),
        PointCertificateErrorV1::Envelope(CertificateErrorV1::UnknownAuthorityKind)
    );
    let parsed =
        UntrustedEnvelopeV1::decode_class(issued.as_bytes(), EnvelopeClassV1::DeclaredPoint)
            .unwrap();
    for (authority, kind) in [(0, 2), (4, 1), (1, 2), (2, 2), (3, 2), (255, 2), (4, 255)] {
        let mixed = hostile_wire(
            parsed.admission_key(),
            parsed.payload_bytes(),
            authority,
            kind,
        );
        assert!(VerifiedPointCertificateV1::verify(&mixed, &evaluated).is_err());
    }
    assert!(UntrustedEnvelopeV1::decode(transport.as_bytes()).is_ok());
}

#[test]
fn forged_producer_context_and_subject_are_refused_after_integrity_validation() {
    let a = attachment();
    let mut state = AuthorityStateV1::new();
    install(&mut state, &a);
    let evaluated = state
        .evaluate_declared_modeled_point(&a, Some(profile()))
        .unwrap();
    let issued = DeclaredPointCertificateV1::issue(&evaluated).unwrap();
    let parsed =
        UntrustedEnvelopeV1::decode_class(issued.as_bytes(), EnvelopeClassV1::DeclaredPoint)
            .unwrap();
    for field in 0..4 {
        let mut key = parsed.admission_key().try_clone().unwrap();
        let expected = match field {
            0 => {
                key.runtime_artifact_id.push('x');
                CertificateErrorV1::RuntimeArtifactMismatch
            }
            1 => {
                key.producer_revision.replace_range(
                    ..1,
                    if key.producer_revision.starts_with('0') {
                        "1"
                    } else {
                        "0"
                    },
                );
                CertificateErrorV1::ProducerRevisionMismatch
            }
            2 => {
                key.producer_content_identity[0] ^= 1;
                CertificateErrorV1::ContentIdentityMismatch
            }
            _ => {
                key.context_id.push('x');
                CertificateErrorV1::ContextMismatch
            }
        };
        let forged = hostile_wire(&key, parsed.payload_bytes(), 4, 2);
        assert!(UntrustedEnvelopeV1::decode_class(&forged, EnvelopeClassV1::DeclaredPoint).is_ok());
        assert_eq!(
            VerifiedPointCertificateV1::verify(&forged, &evaluated).unwrap_err(),
            PointCertificateErrorV1::Envelope(expected)
        );
    }
}

#[test]
fn same_rgb_new_revision_and_other_attachment_do_not_reuse_a_certificate() {
    let mut a = attachment();
    let mut state = AuthorityStateV1::new();
    install(&mut state, &a);
    let evaluated = state
        .evaluate_declared_modeled_point(&a, Some(profile()))
        .unwrap();
    let old = DeclaredPointCertificateV1::issue(&evaluated)
        .unwrap()
        .try_to_bytes()
        .unwrap();
    observe(&mut a, 2);
    assert!(
        state
            .evaluate_declared_modeled_point(&a, Some(profile()))
            .is_err()
    );
    install(&mut state, &a);
    let current = state
        .evaluate_declared_modeled_point(&a, Some(profile()))
        .unwrap();
    assert_eq!(
        current.materialization().terminal_composite(),
        Srgb8::new([96; 3])
    );
    assert_eq!(
        VerifiedPointCertificateV1::verify(&old, &current).unwrap_err(),
        PointCertificateErrorV1::Envelope(CertificateErrorV1::ContentIdentityMismatch)
    );
    let fresh = DeclaredPointCertificateV1::issue(&current)
        .unwrap()
        .try_to_bytes()
        .unwrap();
    let b = attachment();
    let mut other = AuthorityStateV1::new();
    install(&mut other, &b);
    let different = other
        .evaluate_declared_modeled_point(&b, Some(profile()))
        .unwrap();
    assert!(VerifiedPointCertificateV1::verify(&fresh, &different).is_err());
    a.update_unknown(3, 71).unwrap();
    assert!(
        state
            .evaluate_declared_modeled_point(&a, Some(profile()))
            .is_err()
    );
    observe(&mut a, 4);
    install(&mut state, &a);
    let recovered = state
        .evaluate_declared_modeled_point(&a, Some(profile()))
        .unwrap();
    let fresh = DeclaredPointCertificateV1::issue(&recovered).unwrap();
    assert!(VerifiedPointCertificateV1::verify(fresh.as_bytes(), &recovered).is_ok());
}

#[test]
fn malformed_and_oversized_inputs_preserve_sources_and_static_errors() {
    use crate::test_support::{AllocatorEvents, measured_allocator_events};
    let a = attachment();
    let mut state = AuthorityStateV1::new();
    install(&mut state, &a);
    let before = state;
    let render = a.current_render();
    let evaluated = state
        .evaluate_declared_modeled_point(&a, Some(profile()))
        .unwrap();
    let issued = DeclaredPointCertificateV1::issue(&evaluated).unwrap();
    for end in 0..issued.as_bytes().len() {
        assert!(VerifiedPointCertificateV1::verify(&issued.as_bytes()[..end], &evaluated).is_err());
    }
    let mut trailing = issued.try_to_bytes().unwrap();
    trailing.push(0);
    assert_eq!(
        VerifiedPointCertificateV1::verify(&trailing, &evaluated).unwrap_err(),
        PointCertificateErrorV1::Envelope(CertificateErrorV1::TrailingBytes)
    );
    let oversized = vec![0; super::super::MAX_TUPLE_BYTES_V1 + PAYLOAD_BYTES + 33];
    let (rejected, events) =
        measured_allocator_events(|| VerifiedPointCertificateV1::verify(&oversized, &evaluated));
    assert_eq!(
        rejected.unwrap_err(),
        PointCertificateErrorV1::Envelope(CertificateErrorV1::ResourceLimitExceeded)
    );
    assert_eq!(events, AllocatorEvents::default());
    assert_eq!(format!("{issued:?}"), "DeclaredPointCertificateV1 { .. }");
    assert_eq!(
        PointCertificateErrorV1::DifferentCurrentEvaluation.to_string(),
        "different_current_evaluation"
    );
    assert!(state == before);
    assert_eq!(a.current_render(), render);
}
