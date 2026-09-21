use labcolors_core::certificate::{
    CertificateAuthorityKindV1, CertificateOperationV1, CertificatePayloadTypeV1,
    MAX_ENVELOPE_BYTES_V1, UntrustedEnvelopeV1,
};

const CORPUS: &str = include_str!("../contracts/certificate-envelope-v1/reference-vectors.tsv");

fn bytes(hex: &str) -> Vec<u8> {
    assert_eq!(hex.len() % 2, 0, "reference bytes must be complete");
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |byte| match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                _ => panic!("non-canonical reference hexadecimal digit"),
            };
            digit(pair[0]) * 16 + digit(pair[1])
        })
        .collect()
}

// Публичный Core decoder принимает bytes независимого Node/crypto
// oracle, а не packet, выпущенный внутренним encoder той же реализации.
#[test]
fn public_decoder_matches_independent_r13_corpus() {
    let mut accepted = 0;
    let mut rejected = 0;
    let mut names = std::collections::HashSet::new();
    for line in CORPUS
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 10, "invalid corpus row");
        let name = fields[0];
        assert!(names.insert(name), "duplicate corpus case: {name}");
        let input = bytes(fields[2]);
        let decoded = UntrustedEnvelopeV1::decode(&input);
        if fields[1] != "ok" {
            assert_eq!(
                decoded.expect_err(name).code(),
                fields[1],
                "wrong refusal for {name}"
            );
            rejected += 1;
            continue;
        }
        let envelope = decoded.unwrap_or_else(|error| panic!("{name}: {error}"));
        let key = envelope.admission_key();
        assert_eq!(
            key.operation(),
            CertificateOperationV1::IssueCertificate,
            "{name}"
        );
        assert_eq!(
            key.authority_kind(),
            CertificateAuthorityKindV1::GenericTypedCertificate,
            "{name}"
        );
        assert_eq!(key.authority_version(), 1, "{name}");
        assert_eq!(
            key.payload_type(),
            CertificatePayloadTypeV1::NonSemanticTransportPayloadV1,
            "{name}"
        );
        assert_eq!(key.payload_version(), 1, "{name}");
        assert_eq!(
            key.runtime_artifact_id().as_bytes(),
            bytes(fields[3]),
            "{name}"
        );
        assert_eq!(key.producer_revision(), fields[4], "{name}");
        assert_eq!(
            key.producer_content_identity().as_slice(),
            bytes(fields[5]),
            "{name}"
        );
        assert_eq!(key.context_id().as_bytes(), bytes(fields[6]), "{name}");
        assert_eq!(
            envelope.payload_len(),
            fields[7].parse::<u32>().expect("reference payload length"),
            "{name}"
        );
        assert_eq!(
            envelope.payload_sha256().as_slice(),
            bytes(fields[8]),
            "{name}"
        );
        assert_eq!(
            envelope.binding_sha256().as_slice(),
            bytes(fields[9]),
            "{name}"
        );
        accepted += 1;
    }
    assert!(
        accepted > 0 && rejected > 0,
        "corpus must distinguish acceptance from rejection"
    );
}

#[test]
fn public_decoder_checks_total_cap_before_packet_contents() {
    let over_cap = vec![0xff; MAX_ENVELOPE_BYTES_V1 + 1];
    assert_eq!(
        UntrustedEnvelopeV1::decode(&over_cap).unwrap_err().code(),
        "resource_limit_exceeded"
    );
    let at_cap = &over_cap[..MAX_ENVELOPE_BYTES_V1];
    assert_eq!(
        UntrustedEnvelopeV1::decode(at_cap).unwrap_err().code(),
        "invalid_magic"
    );
}
