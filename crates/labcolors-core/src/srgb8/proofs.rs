//! Вся принимаемая грамматика: шесть цифр, необязательный одиночный # и отказ.
//! Oracle использует ASCII-биты и wrapping-разности, не production range-код.
use super::*;

fn nibble(byte: u8) -> Option<u8> {
    let letter = byte | 0x20;
    if byte.wrapping_sub(b'0') < 10 {
        Some(byte & 15)
    } else if letter.wrapping_sub(b'a') < 6 {
        Some((letter & 15) + 9)
    } else {
        None
    }
}

#[kani::proof]
#[kani::unwind(18)]
fn hex_admits_exactly_six_hex_digits() {
    let bytes: [u8; 8] = kani::any();
    let length: u8 = kani::any();
    if length > 8 {
        return;
    }
    let slice = &bytes[..usize::from(length)];
    let has_prefix = length == 7 && bytes[0] == b'#';
    let start = usize::from(has_prefix);
    let correct_length = length == 6 || has_prefix;
    let valid = correct_length && bytes[start..start + 6].iter().all(|b| nibble(*b).is_some());
    let result = parse_hex_bytes(slice);
    assert!(
        result.is_some() == valid,
        "hex syntax must contain six hexadecimal digits only"
    );
    if let Some(rgb) = result {
        for channel in 0..3 {
            assert!(
                Some(rgb[channel])
                    == nibble(bytes[start + 2 * channel])
                        .zip(nibble(bytes[start + 2 * channel + 1]))
                        .map(|(hi, lo)| hi * 16 + lo),
                "hex channels must preserve both nibbles"
            );
        }
    }
    kani::cover!(valid && length == 6, "bare hex");
    kani::cover!(valid && has_prefix, "prefixed hex");
    kani::cover!(!valid && length == 6, "invalid six-byte hex");
    kani::cover!(length == 8, "overlong hex");
}

#[kani::proof]
#[kani::unwind(18)]
fn parser_preserves_all_srgb8_values() {
    let rgb: [u8; 3] = kani::any();
    let lower: bool = kani::any();
    let alphabet = if lower {
        b"0123456789abcdef"
    } else {
        b"0123456789ABCDEF"
    };
    let mut bytes = [b'#'; 7];
    for channel in 0..3 {
        bytes[1 + 2 * channel] = alphabet[usize::from(rgb[channel] >> 4)];
        bytes[2 + 2 * channel] = alphabet[usize::from(rgb[channel] & 15)];
    }
    assert!(
        parse_hex_bytes(&bytes) == Some(rgb),
        "all srgb8 values must survive hexadecimal transport"
    );
    assert!(
        parse_hex_bytes(&bytes[1..]) == Some(rgb),
        "bare transport must preserve identical colour bytes"
    );
    kani::cover!(lower, "lowercase transport");
    kani::cover!(!lower, "uppercase transport");
}
