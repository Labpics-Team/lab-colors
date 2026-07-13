// BEGIN WCAG22_PARSER_CAPSULE_V1
const _: () = (); // First-item parser proof anchor; moving it fails verify_wcag22_q55.py.
/// Parse optional-`#` `RRGGBB` into exact encoded-sRGB8 bytes shared by colour math and proofs.
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
// END WCAG22_PARSER_CAPSULE_V1

/// One exact final encoded-sRGB8 triplet.
///
/// This is a physical value object, not a colour-role or client-semantic type.
/// It is intentionally unversioned: versioned profiles describe how bytes are
/// interpreted, while the bytes themselves remain exactly three octets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Srgb8([u8; 3]);

impl Srgb8 {
    /// Construct one exact byte triplet.
    pub const fn new(bytes: [u8; 3]) -> Self {
        Self(bytes)
    }

    /// Return the exact three encoded bytes.
    pub const fn bytes(self) -> [u8; 3] {
        self.0
    }
}

impl From<[u8; 3]> for Srgb8 {
    fn from(value: [u8; 3]) -> Self {
        Self::new(value)
    }
}

impl From<Srgb8> for [u8; 3] {
    fn from(value: Srgb8) -> Self {
        value.bytes()
    }
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

    #[test]
    fn typed_public_value_round_trips_exact_bytes() {
        let value = Srgb8::new([0x1A, 0x2B, 0x3C]);
        assert_eq!(value.bytes(), [0x1A, 0x2B, 0x3C]);
        assert_eq!(Srgb8::from(value.bytes()), value);
        assert_eq!(<[u8; 3]>::from(value), [0x1A, 0x2B, 0x3C]);
    }
}
