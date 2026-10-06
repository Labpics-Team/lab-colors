use super::*;

// Граница после decode: layout строит настоящий encoder; дайджесты являются
// непрозрачными 256-битными входами. Их криптографическое вычисление не моделируется.
// Это надмножество прошедших decode конвертов, не разрешение на публичную подделку.
fn admitted_shape(
    context: bool,
    identity: [u8; 32],
    payload: u8,
    payload_sha256: [u8; 32],
    binding_sha256: [u8; 32],
) -> (UntrustedEnvelopeV1, TrustedProducerAttestationV1) {
    let key = AdmissionKeyV1::try_new(
        "r",
        CertificateOperationV1::IssueCertificate,
        if context { "b" } else { "a" },
        "0123456789abcdef0123456789abcdef01234567",
        identity,
    )
    .unwrap();
    let mut canonical_bytes = encode_prefix(&key, &[payload], payload_sha256).unwrap();
    let key_prefix_len = canonical_bytes.len() - WIRE_U32_BYTES_V1 - 1 - WIRE_DIGEST_BYTES_V1;
    canonical_bytes.extend_from_slice(&binding_sha256);
    let attestation = TrustedProducerAttestationV1 {
        key: key.try_clone().unwrap(),
        payload_sha256,
        binding_sha256,
    };
    (
        UntrustedEnvelopeV1 {
            key,
            key_prefix_len,
            payload_len: 1,
            payload_sha256,
            binding_sha256,
            canonical_bytes,
        },
        attestation,
    )
}

#[test]
fn lifecycle_replay_conflict_and_capacity_use_the_real_ledger() {
    for current_context in [false, true] {
        for next_context in [false, true] {
            for same_payload in [false, true] {
                let (first, first_attestation) =
                    admitted_shape(current_context, [3; 32], 17, [4; 32], [5; 32]);
                let (next, next_attestation) = admitted_shape(
                    next_context,
                    [3; 32],
                    if same_payload { 17 } else { 18 },
                    [4; 32],
                    [5; 32],
                );
                let entry = next.canonical_bytes.len() + ADMISSION_METADATA_BYTES_V1;
                for accounted in [
                    0,
                    MAX_ADMISSION_BYTES_V1 - entry,
                    MAX_ADMISSION_BYTES_V1 - entry + 1,
                    MAX_ADMISSION_BYTES_V1,
                    usize::MAX,
                ] {
                    let mut state = AdmissionStateV1::new();
                    state
                        .admit(&first, &first.key, Some(&first_attestation))
                        .unwrap();
                    state.accounted_bytes = accounted;
                    let original = state.records[0].canonical_bytes.clone();
                    let result = state.admit(&next, &next.key, Some(&next_attestation));
                    let expected = if current_context == next_context {
                        if same_payload {
                            Ok(AdmissionOutcomeV1::DuplicateNoop)
                        } else {
                            Err(CertificateErrorV1::BindingConflict)
                        }
                    } else if accounted <= MAX_ADMISSION_BYTES_V1 - entry {
                        Ok(AdmissionOutcomeV1::Accepted)
                    } else {
                        Err(CertificateErrorV1::AdmissionCapacityExceeded)
                    };
                    assert_eq!(result, expected);
                    assert_eq!(state.records[0].canonical_bytes, original);
                    if result == Ok(AdmissionOutcomeV1::Accepted) {
                        assert_eq!(state.len(), 2);
                        assert_eq!(state.accounted_bytes, accounted + entry);
                        assert_eq!(state.records[1].canonical_bytes, next.canonical_bytes);
                    } else {
                        assert_eq!(state.len(), 1);
                        assert_eq!(state.accounted_bytes, accounted);
                    }
                }
            }
        }
    }
}

#[test]
fn lifecycle_duplicate_requires_every_attestation_byte() {
    let (envelope, attestation) = admitted_shape(false, [3; 32], 17, [4; 32], [5; 32]);
    let mut state = AdmissionStateV1::new();
    state
        .admit(&envelope, &envelope.key, Some(&attestation))
        .unwrap();
    let accounted = state.accounted_bytes;
    assert_eq!(
        state.admit(&envelope, &envelope.key, None),
        Err(CertificateErrorV1::MissingProducerAttestation)
    );
    for axis in 0..3 {
        for index in 0..32 {
            let mut wrong = TrustedProducerAttestationV1 {
                key: attestation.key.try_clone().unwrap(),
                payload_sha256: attestation.payload_sha256,
                binding_sha256: attestation.binding_sha256,
            };
            match axis {
                0 => wrong.key.producer_content_identity[index] ^= 1,
                1 => wrong.payload_sha256[index] ^= 1,
                _ => wrong.binding_sha256[index] ^= 1,
            }
            assert_eq!(
                state.admit(&envelope, &envelope.key, Some(&wrong)),
                Err(CertificateErrorV1::ProducerBindingMismatch)
            );
            assert_eq!(state.len(), 1);
            assert_eq!(state.accounted_bytes, accounted);
        }
    }
    assert_eq!(
        state.admit(&envelope, &envelope.key, Some(&attestation)),
        Ok(AdmissionOutcomeV1::DuplicateNoop)
    );
}

#[test]
fn lifecycle_full_ledger_preserves_replay_and_rejects_new_keys() {
    fn packet(context: &str) -> (UntrustedEnvelopeV1, TrustedProducerAttestationV1) {
        let key = AdmissionKeyV1::try_new(
            "r",
            CertificateOperationV1::IssueCertificate,
            context,
            "0123456789abcdef0123456789abcdef01234567",
            [3; 32],
        )
        .unwrap();
        let payload = producer_payload_v1(b"opaque").unwrap();
        let attestation = producer_attestation_v1(key.try_clone().unwrap(), &payload).unwrap();
        let envelope = CertificateEnvelopeV1::issue_from_trusted_producer(
            payload,
            producer_attestation_v1(key, &producer_payload_v1(b"opaque").unwrap()).unwrap(),
        )
        .unwrap();
        (
            UntrustedEnvelopeV1::decode(envelope.as_bytes()).unwrap(),
            attestation,
        )
    }
    let mut state = AdmissionStateV1::new();
    for ordinal in 0..MAX_ADMISSION_ENTRIES_V1 {
        let (value, attestation) = packet(&format!("ctx-{ordinal}"));
        assert_eq!(
            state.admit(&value, &value.key, Some(&attestation)),
            Ok(AdmissionOutcomeV1::Accepted)
        );
    }
    let accounted = state.accounted_bytes;
    let (duplicate, attestation) = packet("ctx-0");
    assert_eq!(
        state.admit(&duplicate, &duplicate.key, Some(&attestation)),
        Ok(AdmissionOutcomeV1::DuplicateNoop)
    );
    let (new, attestation) = packet("new");
    assert_eq!(
        state.admit(&new, &new.key, Some(&attestation)),
        Err(CertificateErrorV1::AdmissionCapacityExceeded)
    );
    assert_eq!(state.len(), MAX_ADMISSION_ENTRIES_V1);
    assert_eq!(state.accounted_bytes, accounted);
}
