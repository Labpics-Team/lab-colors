//! Формальный wire-контракт: смещения, порядок байтов, f64-биты и лимит записей.
use super::*;

#[kani::proof]
#[kani::unwind(10)]
fn fixed_reads_are_exact_and_atomic() {
    let bytes: [u8; 8] = kani::any();
    let offset: usize = kani::any();
    let mut reader = WireReader::new(&bytes);
    reader.offset = offset;
    let result = reader.read_u32();
    let valid = offset <= 4;
    assert!(
        result.is_ok() == valid,
        "program wire must reject overflowing or truncated fixed reads"
    );
    if let Ok(value) = result {
        let expected = u32::from(bytes[offset])
            + 256 * u32::from(bytes[offset + 1])
            + 65536 * u32::from(bytes[offset + 2])
            + 16777216 * u32::from(bytes[offset + 3]);
        assert!(
            value == expected && reader.offset == offset + 4,
            "program wire must preserve little endian bytes and cursor"
        );
    } else {
        assert!(
            reader.offset == offset,
            "program wire failure must preserve the cursor"
        );
    }
    kani::cover!(valid, "fixed read accepted");
    kani::cover!(!valid, "fixed read rejected");
}

#[kani::proof]
#[kani::unwind(10)]
fn floating_wire_keeps_every_bit() {
    let bits: u64 = kani::any();
    let bytes = bits.to_le_bytes();
    let mut reader = WireReader::new(&bytes);
    let value = reader.read_f64_bits().unwrap();
    assert!(
        value.to_bits() == bits,
        "wire float transport must preserve every binary64 bit"
    );
    assert!(
        reader.finish().is_ok(),
        "complete float read must consume exactly eight bytes"
    );
    kani::cover!(bits == (-0.0_f64).to_bits(), "negative zero transport");
    kani::cover!(
        bits == f64::INFINITY.to_bits(),
        "infinity stays raw transport"
    );
    kani::cover!(bits == f64::NAN.to_bits(), "NaN stays raw transport");
}

#[kani::proof]
#[kani::unwind(6)]
fn section_count_preserves_resource_limit() {
    let count: u32 = kani::any();
    let bytes = count.to_le_bytes();
    let mut reader = WireReader::new(&bytes);
    reader.enter(WireSectionV1::Sources);
    let result = reader.read_count();
    let expected = if count <= MAX_SECTION_ENTRIES_V1 {
        Ok(count)
    } else {
        Err(ProgramWireErrorV1::ResourceExhausted {
            section: WireSectionV1::Sources,
        })
    };
    assert!(
        result == expected,
        "program section length must never exceed its admitted budget"
    );
    assert!(reader.finish().is_ok());
    kani::cover!(count == 0, "empty section");
    kani::cover!(count == MAX_SECTION_ENTRIES_V1, "budget boundary");
    kani::cover!(count > MAX_SECTION_ENTRIES_V1, "overbudget section");
}
