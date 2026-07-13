//! Exact encoded-sRGB8 transport primitives shared by colour math and proofs.

/// Parse optional-`#` `RRGGBB` into the exact three encoded bytes.
///
/// Public APIs choose their own transport strictness before calling this SSOT.
/// ASCII is checked before byte slicing, so arbitrary public Unicode input
/// returns `Err` instead of panicking at a non-character boundary.
pub(crate) fn hex_bytes(hex: &str) -> Result<[u8; 3], String> {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 || !hex.is_ascii() {
        return Err(format!("expected #RRGGBB, got #{hex}"));
    }
    let parse = |value: &str| u8::from_str_radix(value, 16).map_err(|error| error.to_string());
    Ok([parse(&hex[0..2])?, parse(&hex[2..4])?, parse(&hex[4..6])?])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wcag22_transport_parser_preserves_rgb_and_rejects_unicode_without_panic() {
        assert_eq!(hex_bytes("#1A2B3C").unwrap(), [0x1A, 0x2B, 0x3C]);
        for invalid in ["€€", "#€€", "ééé", "##1A2B3C"] {
            assert!(hex_bytes(invalid).is_err());
        }
    }

    #[test]
    fn public_srgb_parser_rejects_a_repeated_hash_prefix() {
        assert!(crate::spaces::srgb::srgb_encoded_from_hex("##1A2B3C").is_err());
    }
}
