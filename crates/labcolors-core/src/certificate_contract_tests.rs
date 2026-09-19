use super::*;

const REVISION: &[u8] = b"0123456789abcdef0123456789abcdef01234567";

// Независимая сборка positional-полей; production encoder здесь не вызывается.
fn wire(runtime: &[u8], revision: &[u8], context: &[u8], payload: &[u8]) -> Vec<u8> {
    fn text(bytes: &mut Vec<u8>, value: &[u8]) {
        bytes.extend_from_slice(&u16::try_from(value.len()).unwrap().to_be_bytes());
        bytes.extend_from_slice(value);
    }
    fn digest(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
        let mut hasher = crate::sha256::Hasher::new();
        hasher.update(domain);
        hasher.update(bytes);
        *hasher.finalize().as_bytes()
    }
    let mut bytes = b"LCEN\x00\x01\x01\x00\x00\x01".to_vec();
    text(&mut bytes, runtime);
    text(&mut bytes, revision);
    bytes.extend_from_slice(&[0x11; 32]);
    text(&mut bytes, context);
    bytes.extend_from_slice(&[1, 0, 1]);
    bytes.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes.extend_from_slice(&digest(b"labpics.colors/certificate-payload/v1\0", payload));
    let binding = digest(b"labpics.colors/certificate-envelope/v1\0", &bytes);
    bytes.extend_from_slice(&binding);
    bytes
}

#[test]
fn structural_truncation_precedes_selector_text_and_digest_errors() {
    assert_eq!(
        UntrustedEnvelopeV1::decode(b"LCEN\x00\x01\xff"),
        Err(CertificateErrorV1::TruncatedInput)
    );
    let valid = wire(b"runtime", REVISION, b"context", b"body");
    assert!(UntrustedEnvelopeV1::decode(&valid).is_ok());
    for offset in [6, 7, 12, valid.len() - 64] {
        let mut bytes = valid.clone();
        bytes[offset] = 0xff;
        bytes.pop();
        assert_eq!(
            UntrustedEnvelopeV1::decode(&bytes),
            Err(CertificateErrorV1::TruncatedInput),
            "earlier malformed byte at {offset} must not hide truncation"
        );
    }
}

#[test]
fn lengths_precede_text_revision_and_selectors() {
    for bytes in [
        wire(&[0xff], REVISION, b"", b"body"),
        wire(b"runtime", b"UPPERCASE", b"", b"body"),
    ] {
        assert_eq!(
            UntrustedEnvelopeV1::decode(&bytes),
            Err(CertificateErrorV1::InvalidLength)
        );
    }
    let mut bytes = wire(b"", REVISION, b"context", b"body");
    bytes[6] = 0xff;
    assert_eq!(
        UntrustedEnvelopeV1::decode(&bytes),
        Err(CertificateErrorV1::InvalidLength)
    );
    let mut bytes = wire(b"runtime", REVISION, b"context", b"");
    let payload_type = bytes.len() - 64 - 4 - 3;
    bytes[payload_type] = 0xff;
    assert_eq!(
        UntrustedEnvelopeV1::decode(&bytes),
        Err(CertificateErrorV1::InvalidLength)
    );
}

#[test]
fn text_and_revision_precede_reserved_selectors() {
    let mut bytes = wire(&[0xff], REVISION, b"context", b"body");
    bytes[6] = 0xff;
    assert_eq!(
        UntrustedEnvelopeV1::decode(&bytes),
        Err(CertificateErrorV1::InvalidUtf8)
    );
    let mut revision = REVISION.to_vec();
    revision[0] = b'A';
    let mut bytes = wire(b"runtime", &revision, b"context", b"body");
    bytes[7] = 1;
    assert_eq!(
        UntrustedEnvelopeV1::decode(&bytes),
        Err(CertificateErrorV1::NonCanonicalRevision)
    );
    revision[0] = 0xff;
    assert_eq!(
        UntrustedEnvelopeV1::decode(&wire(b"runtime", &revision, b"context", b"body")),
        Err(CertificateErrorV1::InvalidUtf8)
    );
}

#[test]
fn exact_packet_length_precedes_digest_validation() {
    let mut bytes = wire(b"runtime", REVISION, b"context", b"body");
    let payload_digest_start = bytes.len() - 64;
    bytes[payload_digest_start] ^= 1;
    bytes.push(0);
    assert_eq!(
        UntrustedEnvelopeV1::decode(&bytes),
        Err(CertificateErrorV1::TrailingBytes)
    );
}

fn producer(
    runtime: &str,
    context: &str,
    body: &[u8],
) -> (
    AdmissionKeyV1,
    TrustedProducerAttestationV1,
    UntrustedEnvelopeV1,
) {
    let key = AdmissionKeyV1::try_new(
        runtime,
        CertificateOperationV1::IssueCertificate,
        context,
        core::str::from_utf8(REVISION).unwrap(),
        [0x11; 32],
    )
    .unwrap();
    let payload = producer_payload_v1(body).unwrap();
    let attestation = producer_attestation_v1(key.try_clone().unwrap(), &payload).unwrap();
    let envelope = CertificateEnvelopeV1::issue_from_trusted_producer(
        payload,
        producer_attestation_v1(
            key.try_clone().unwrap(),
            &producer_payload_v1(body).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    (
        key,
        attestation,
        UntrustedEnvelopeV1::decode(envelope.as_bytes()).unwrap(),
    )
}

// Считаем retained allocation, включая spare capacity, а не длину входного
// envelope. Strings, digests или buckets нельзя добавить бесплатно.
fn retained_bytes(state: &AdmissionStateV1) -> usize {
    core::mem::size_of_val(state)
        + state.records.capacity() * core::mem::size_of::<AdmissionRecord>()
        + state
            .records
            .iter()
            .map(|record| record.canonical_bytes.capacity())
            .sum::<usize>()
}

#[test]
fn ledger_accounts_for_retained_tuple_and_collection_storage() {
    let (key, attestation, envelope) = producer(&"r".repeat(128), &"c".repeat(256), b"x");
    let mut state = AdmissionStateV1::new();
    assert_eq!(
        state.admit(&envelope, &key, Some(&attestation)),
        Ok(AdmissionOutcomeV1::Accepted)
    );
    assert!(retained_bytes(&state) <= state.accounted_bytes());
}

#[test]
fn entry_capacity_is_reached_by_real_admissions_without_eviction() {
    let (key, attestation, envelope) = producer("runtime", "first", b"body");
    let mut state = AdmissionStateV1::new();
    assert_eq!(
        state.admit(&envelope, &key, Some(&attestation)),
        Ok(AdmissionOutcomeV1::Accepted)
    );
    for index in 1..MAX_ADMISSION_ENTRIES_V1 {
        let (key, attestation, envelope) = producer("runtime", &index.to_string(), b"body");
        assert_eq!(
            state.admit(&envelope, &key, Some(&attestation)),
            Ok(AdmissionOutcomeV1::Accepted)
        );
    }
    let prior_bytes = state.accounted_bytes();
    let (new_key, new_attestation, new_envelope) = producer("runtime", "overflow", b"body");
    assert_eq!(
        state.admit(&new_envelope, &new_key, Some(&new_attestation)),
        Err(CertificateErrorV1::AdmissionCapacityExceeded)
    );
    assert_eq!(state.len(), MAX_ADMISSION_ENTRIES_V1);
    assert_eq!(state.accounted_bytes(), prior_bytes);
    assert_eq!(
        state.admit(&envelope, &key, Some(&attestation)),
        Ok(AdmissionOutcomeV1::DuplicateNoop)
    );
    assert!(retained_bytes(&state) <= state.accounted_bytes());
}

#[test]
fn byte_capacity_is_reached_by_real_admissions_without_eviction() {
    let runtime = "r".repeat(128);
    // 543 wire bytes + 128 metadata bytes + body = ровно 1 MiB на entry.
    let body = vec![0x55; 1_047_905];
    let mut state = AdmissionStateV1::new();
    for index in 0..8 {
        let (key, attestation, envelope) = producer(&runtime, &format!("{index:0256}"), &body);
        assert_eq!(
            state.admit(&envelope, &key, Some(&attestation)),
            Ok(AdmissionOutcomeV1::Accepted)
        );
    }
    assert_eq!(state.accounted_bytes(), MAX_ADMISSION_BYTES_V1);
    assert!(retained_bytes(&state) <= MAX_ADMISSION_BYTES_V1);
    let (key, attestation, envelope) = producer(&runtime, &format!("{:0256}", 0), &body);
    assert_eq!(
        state.admit(&envelope, &key, Some(&attestation)),
        Ok(AdmissionOutcomeV1::DuplicateNoop)
    );
    let (_, other_attestation, other_envelope) =
        producer(&runtime, &format!("{:0256}", 0), b"other");
    assert_eq!(
        state.admit(&other_envelope, &key, Some(&other_attestation)),
        Err(CertificateErrorV1::BindingConflict)
    );
    let (key, attestation, envelope) = producer(&runtime, &format!("{:0256}", 8), b"new");
    assert_eq!(
        state.admit(&envelope, &key, Some(&attestation)),
        Err(CertificateErrorV1::AdmissionCapacityExceeded)
    );
    assert_eq!(state.len(), 8);
    assert_eq!(state.accounted_bytes(), MAX_ADMISSION_BYTES_V1);
}

#[test]
fn debug_is_static_across_distinct_identifiers_sizes_and_ledger_counts() {
    for (runtime, context, body) in [
        ("r".to_string(), "c".to_string(), vec![1]),
        (
            "secret-runtime-".repeat(8),
            "private-context-".repeat(16),
            vec![0xa5; 257],
        ),
    ] {
        let (key, attestation, decoded) = producer(&runtime, &context, &body);
        let payload = producer_payload_v1(&body).unwrap();
        let envelope = CertificateEnvelopeV1::issue_from_trusted_producer(
            producer_payload_v1(&body).unwrap(),
            producer_attestation_v1(key.try_clone().unwrap(), &payload).unwrap(),
        )
        .unwrap();
        assert_eq!(format!("{key:?}"), "AdmissionKeyV1 { .. }");
        assert_eq!(
            format!("{payload:?}"),
            "NonSemanticTransportPayloadV1 { .. }"
        );
        assert_eq!(
            format!("{attestation:?}"),
            "TrustedProducerAttestationV1 { .. }"
        );
        assert_eq!(format!("{envelope:?}"), "CertificateEnvelopeV1 { .. }");
        assert_eq!(format!("{decoded:?}"), "UntrustedEnvelopeV1 { .. }");
        let mut state = AdmissionStateV1::new();
        assert_eq!(format!("{state:?}"), "AdmissionStateV1 { .. }");
        state.admit(&decoded, &key, Some(&attestation)).unwrap();
        assert_eq!(format!("{state:?}"), "AdmissionStateV1 { .. }");
    }
}

#[test]
fn maximum_producer_packet_reserves_its_binding_before_issue() {
    let key = AdmissionKeyV1::try_new(
        &"r".repeat(MAX_RUNTIME_ARTIFACT_ID_BYTES_V1),
        CertificateOperationV1::IssueCertificate,
        &"c".repeat(MAX_CONTEXT_ID_BYTES_V1),
        core::str::from_utf8(REVISION).unwrap(),
        [0x11; 32],
    )
    .unwrap();
    let body = vec![0x55; MAX_PAYLOAD_BYTES_V1];
    let payload = producer_payload_v1(&body).unwrap();
    let prefix =
        encode_prefix(&key, payload.as_bytes(), payload_digest(payload.as_bytes())).unwrap();
    // Независимое число: fixed wire fields и три text fields дают 543 bytes
    // вместе с последним digest. Проверяем обе границы до append issuer-а.
    assert_eq!(prefix.len() + 32, MAX_PAYLOAD_BYTES_V1 + 543);
    assert!(prefix.capacity() >= prefix.len() + 32);
    assert!(prefix.capacity() <= MAX_ENVELOPE_BYTES_V1);
    let attestation = producer_attestation_v1(key.try_clone().unwrap(), &payload).unwrap();
    let envelope =
        CertificateEnvelopeV1::issue_from_trusted_producer(payload, attestation).unwrap();
    let mut exported = envelope.try_to_bytes().unwrap();
    assert_eq!(exported.as_slice(), envelope.as_bytes());
    exported[0] ^= 1;
    assert!(UntrustedEnvelopeV1::decode(envelope.as_bytes()).is_ok());
    assert_eq!(envelope.admission_key(), &key);
}

#[test]
fn reader_overflow_is_truncated_input_on_every_pointer_width() {
    let mut reader = Reader::new(&[0]);
    assert_eq!(reader.read_exact(1).unwrap(), &[0]);
    assert_eq!(
        reader.read_exact(usize::MAX).unwrap_err(),
        CertificateErrorV1::TruncatedInput
    );
}
