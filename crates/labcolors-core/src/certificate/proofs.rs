//! Границы позиционного reader, до доверия дайджестам и без моделирования SHA.
use super::*;

#[kani::proof]
#[kani::unwind(10)]
fn reader_bounds_and_failure_atomicity() {
    let bytes: [u8; 8] = kani::any();
    let length: usize = kani::any();
    let offset: usize = kani::any();
    let mut reader = Reader {
        bytes: &bytes,
        offset,
    };
    let valid = offset <= bytes.len() && length <= bytes.len().saturating_sub(offset);
    let result = reader.read_exact(length);
    assert!(
        result.is_ok() == valid,
        "reader must reject every out-of-bounds or overflowing span"
    );
    match result {
        Ok(value) if valid => {
            assert!(
                value == &bytes[offset..offset + length],
                "reader must return exactly the requested bytes"
            );
            assert!(reader.offset == offset + length);
        }
        Ok(_) => {} // Неверный успех уже опровергнут первым утверждением.
        Err(error) => {
            assert!(error == CertificateErrorV1::TruncatedInput);
            assert!(
                reader.offset == offset,
                "failed exact read must not advance the cursor"
            );
        }
    }
    kani::cover!(valid && length > 0, "nonempty read");
    kani::cover!(valid && length == 0, "empty read");
    kani::cover!(!valid, "invalid span");
    kani::cover!(offset.checked_add(length).is_none(), "offset overflow");
}

#[kani::proof]
#[kani::unwind(10)]
fn reader_big_endian_and_length_prefix() {
    let bytes: [u8; 8] = kani::any();
    let mut reader = Reader::new(&bytes);
    let first = reader.read_u32().unwrap();
    let expected = u32::from(bytes[0]) * 16777216
        + u32::from(bytes[1]) * 65536
        + u32::from(bytes[2]) * 256
        + u32::from(bytes[3]);
    assert!(
        first == expected,
        "certificate integers must be decoded in network byte order"
    );
    let result = reader.read_length_delimited();
    let length = usize::from(bytes[4]) * 256 + usize::from(bytes[5]);
    assert!(
        result.is_ok() == (length <= 2),
        "length prefix must never admit truncated payload"
    );
    if let Ok(value) = result {
        assert!(value == &bytes[6..6 + length]);
        assert!(reader.is_finished() == (length == 2));
    } else {
        // Prefix уже прочитан. Атомарность read_exact не подменяет всю транзакцию.
        assert!(reader.offset == 6);
    }
    kani::cover!(length == 2, "complete delimited payload");
    kani::cover!(length > 2, "truncated delimited payload");
}

#[kani::proof]
#[kani::unwind(42)]
fn revision_is_exact_lowercase_hex() {
    let bytes: [u8; 40] = kani::any();
    let valid = bytes
        .iter()
        .all(|b| matches!(*b, b'0'..=b'9' | b'a'..=b'f'));
    assert!(
        is_canonical_revision(&bytes) == valid,
        "revision must contain exactly forty lowercase hexadecimal digits"
    );
    assert!(!is_canonical_revision(&bytes[..39]));
    kani::cover!(valid, "canonical revision");
    kani::cover!(!valid, "malformed revision");
}

#[kani::proof]
fn wire_resource_length_is_exact() {
    let length: usize = kani::any();
    let limit: usize = kani::any();
    let result = validate_length(length, limit);
    assert!(
        result.is_ok() == (length > 0 && length <= limit),
        "wire resource admission must preserve nonempty bounded length"
    );
    kani::cover!(result.is_ok(), "length admitted");
    kani::cover!(
        result == Err(CertificateErrorV1::InvalidLength),
        "empty length"
    );
    kani::cover!(
        result == Err(CertificateErrorV1::ResourceLimitExceeded),
        "overbudget length"
    );
}

#[kani::proof]
fn ledger_budget_cannot_overflow_or_exceed_capacity() {
    let records: usize = kani::any();
    let accounted: usize = kani::any();
    let bytes: usize = kani::any();
    let exact = accounted as u128 + bytes as u128 + ADMISSION_METADATA_BYTES_V1 as u128;
    let allowed = records < MAX_ADMISSION_ENTRIES_V1 && exact <= MAX_ADMISSION_BYTES_V1 as u128;
    let result = admission_budget(records, accounted, bytes);
    assert!(
        result.is_ok() == allowed,
        "ledger budget must reject all count size and overflow violations"
    );
    match result {
        Ok(value) => assert!(
            value as u128 == exact,
            "ledger accounting must equal the exact retained byte cost"
        ),
        Err(error) => assert!(error == CertificateErrorV1::AdmissionCapacityExceeded),
    }
    kani::cover!(
        allowed && exact == MAX_ADMISSION_BYTES_V1 as u128,
        "exact byte capacity admitted"
    );
    kani::cover!(
        records == MAX_ADMISSION_ENTRIES_V1,
        "entry capacity rejected"
    );
    kani::cover!(exact > usize::MAX as u128, "machine word overflow rejected");
    kani::cover!(
        records == 0 && accounted == 0 && bytes == 1,
        "first ledger entry admitted"
    );
}
