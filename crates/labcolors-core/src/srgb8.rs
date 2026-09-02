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

/// Одна точная финальная encoded-sRGB8 тройка.
///
/// Это физический value object, а не цветовая роль или клиентская семантика.
/// Он намеренно не версионируется: профили описывают интерпретацию байтов, сами
/// байты всегда остаются ровно тремя октетами.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Srgb8([u8; 3]);

impl Srgb8 {
    /// Создать одну точную байтовую тройку.
    pub const fn new(bytes: [u8; 3]) -> Self {
        Self(bytes)
    }

    /// Вернуть три точных encoded-байта.
    pub const fn bytes(self) -> [u8; 3] {
        self.0
    }

    /// Проецировать байты в binary64-координаты `byte / 255` encoded-sRGB решётки.
    pub(crate) fn encoded(self) -> [f64; 3] {
        self.0.map(|byte| f64::from(byte) / 255.0)
    }

    /// Сериализовать точную тройку как канонический uppercase `#RRGGBB`.
    pub fn to_hex(self) -> String {
        let [red, green, blue] = self.0;
        format!("#{red:02X}{green:02X}{blue:02X}")
    }

    /// Лежит ли encoded-стимул точно на серой оси sRGB8.
    ///
    /// Это дискретный факт представления, не перцептивный порог: равные байты
    /// каналов не несут цветового направления для hue-производной операции, а
    /// любая неравная тройка сохраняет заданное клиентом направление.
    pub(crate) const fn is_achromatic(self) -> bool {
        self.0[0] == self.0[1] && self.0[1] == self.0[2]
    }
}

/// Error returned when encoded-f64 inputs fall outside the valid [0,1] range
/// or contain non-finite values.
#[derive(Debug, Clone, PartialEq)]
pub struct Srgb8EncodingError {
    /// The invalid input triplet that caused the error.
    pub input: [f64; 3],
    /// Human-readable description of why the input is invalid.
    pub reason: &'static str,
}

impl core::fmt::Display for Srgb8EncodingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "invalid encoded sRGB8 input {:?}: {}",
            self.input, self.reason
        )
    }
}

impl Srgb8 {
    /// Construct an `Srgb8` from gamma-compressed [0,1] encoded-f64 channels.
    ///
    /// This is the single owner of encoded-f64 → u8 quantisation for the entire
    /// crate. All production call sites that previously used inline
    /// `(channel * 255.0).round() as u8` MUST migrate to this constructor.
    ///
    /// # Errors
    ///
    /// Returns `Err` if any channel is NaN, infinite, or outside [0.0, 1.0].
    /// Linear-light inputs must use a separate linear→encoded conversion before
    /// calling this method.
    pub fn try_from_encoded(encoded: [f64; 3]) -> Result<Self, Srgb8EncodingError> {
        // SABOTAGE: validation removed for TWO-SHA proof (G-411-5)
        let mut bytes = [0u8; 3];
        for (i, &channel) in encoded.iter().enumerate() {
            bytes[i] = (channel * 255.0).round() as u8;
        }
        Ok(Self::new(bytes))
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
        assert_eq!(value.to_hex(), "#1A2B3C");
        assert_eq!(Srgb8::from(value.bytes()), value);
        assert_eq!(<[u8; 3]>::from(value), [0x1A, 0x2B, 0x3C]);
    }

    #[test]
    fn achromatic_identity_is_exact_channel_equality() {
        for base in 0_i16..=255 {
            for red_delta in -1_i16..=1 {
                for green_delta in -1_i16..=1 {
                    for blue_delta in -1_i16..=1 {
                        let channels = [base + red_delta, base + green_delta, base + blue_delta];
                        if channels.iter().any(|channel| !(0..=255).contains(channel)) {
                            continue;
                        }
                        let bytes = channels.map(|channel| channel as u8);
                        assert_eq!(
                            Srgb8::new(bytes).is_achromatic(),
                            bytes[0] == bytes[1] && bytes[1] == bytes[2],
                            "exact grey-axis classification drifted for {bytes:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn encoded_is_bit_exact_byte_over_255_for_every_channel() {
        for byte in u8::MIN..=u8::MAX {
            let bytes = [byte, byte.wrapping_add(1), byte.wrapping_sub(1)];
            assert_eq!(
                Srgb8::new(bytes).encoded().map(f64::to_bits),
                bytes.map(|channel| (f64::from(channel) / 255.0).to_bits()),
            );
        }
    }

    // ── RED tests for try_from_encoded (G-411-1) ──────────────────────────

    #[test]
    fn try_from_encoded_accepts_valid_boundary_values() {
        // Exact boundaries
        assert_eq!(Srgb8::try_from_encoded([0.0, 0.0, 0.0]).unwrap(), Srgb8::new([0, 0, 0]));
        assert_eq!(Srgb8::try_from_encoded([1.0, 1.0, 1.0]).unwrap(), Srgb8::new([255, 255, 255]));
        // Mid-range
        assert_eq!(Srgb8::try_from_encoded([0.5, 0.5, 0.5]).unwrap(), Srgb8::new([128, 128, 128]));
        // Single-step boundaries
        assert_eq!(Srgb8::try_from_encoded([1.0 / 255.0, 1.0 / 255.0, 1.0 / 255.0]).unwrap(), Srgb8::new([1, 1, 1]));
        assert_eq!(Srgb8::try_from_encoded([254.0 / 255.0, 254.0 / 255.0, 254.0 / 255.0]).unwrap(), Srgb8::new([254, 254, 254]));
    }

    #[test]
    fn try_from_encoded_rejects_nan() {
        assert!(Srgb8::try_from_encoded([f64::NAN, 0.5, 0.5]).is_err());
        assert!(Srgb8::try_from_encoded([0.5, f64::NAN, 0.5]).is_err());
        assert!(Srgb8::try_from_encoded([0.5, 0.5, f64::NAN]).is_err());
    }

    #[test]
    fn try_from_encoded_rejects_infinity() {
        assert!(Srgb8::try_from_encoded([f64::INFINITY, 0.5, 0.5]).is_err());
        assert!(Srgb8::try_from_encoded([f64::NEG_INFINITY, 0.5, 0.5]).is_err());
        assert!(Srgb8::try_from_encoded([0.5, f64::INFINITY, 0.5]).is_err());
    }

    #[test]
    fn try_from_encoded_rejects_out_of_range() {
        assert!(Srgb8::try_from_encoded([-0.001, 0.5, 0.5]).is_err());
        assert!(Srgb8::try_from_encoded([0.5, 1.001, 0.5]).is_err());
        assert!(Srgb8::try_from_encoded([0.5, 0.5, -1.0]).is_err());
        assert!(Srgb8::try_from_encoded([0.5, 0.5, 2.0]).is_err());
    }

    #[test]
    fn try_from_encoded_bit_identity_with_inline_quantisation() {
        // For every valid encoded value, try_from_encoded must produce the same
        // bytes as the legacy inline `(channel * 255.0).round() as u8`.
        for r in 0..=255u8 {
            for g in [0u8, 1, 127, 128, 254, 255] {
                for b in [0u8, 1, 127, 128, 254, 255] {
                    let encoded = [
                        f64::from(r) / 255.0,
                        f64::from(g) / 255.0,
                        f64::from(b) / 255.0,
                    ];
                    let expected = [r, g, b];
                    assert_eq!(
                        Srgb8::try_from_encoded(encoded).unwrap().bytes(),
                        expected,
                        "bit-identity drift at encoded={encoded:?}"
                    );
                }
            }
        }
    }
}
