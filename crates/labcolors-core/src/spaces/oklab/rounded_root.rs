//! Exact machine cube-root rounding; independent of color policy.

const SIGNIFICAND_BIT: u64 = 1 << (f64::MANTISSA_DIGITS - 1);
const FRACTION_MASK: u64 = SIGNIFICAND_BIT - 1;

/// Compare the exact cube of the midpoint between `lower` and its successor
/// to the positive normal binary64 `input`. The midpoint has an odd 54-bit
/// significand M; M^3 fits in 162 bits. We need only the high 128 bits:
/// when exponents agree, the input significand is shifted left by >64 bits,
/// and the omitted low limb of the odd cube is strictly nonzero.
fn midpoint_cube_below(lower: u64, input: u64) -> bool {
    let m = (((lower & FRACTION_MASK) | SIGNIFICAND_BIT) * 2 + 1) as u128;
    let square = m * m;
    let carry = ((square as u64 as u128) * m) >> 64;
    let high = (square >> 64) * m + carry;
    let high_bits = u128::BITS - high.leading_zeros();
    let cube_exponent = (64 + high_bits) as i32 - 1 + 3 * ((lower >> 52) as i32 - 1023 - 53);
    let input_exponent = (input >> 52) as i32 - 1023;
    if cube_exponent != input_exponent {
        return cube_exponent < input_exponent;
    }
    let aligned = (((input & FRACTION_MASK) | SIGNIFICAND_BIT) as u128) << (high_bits - 53);
    high < aligned
}

/// Correctly rounded cube root of a positive normal binary64. The native
/// cbrt is only a hint (Rust does not specify its accuracy). Exact midpoint
/// comparisons certify the result; a bad hint can only add work. The root
/// is in the binade derived from the input exponent, including its upper
/// endpoint after rounding. No midpoint tie exists for a binary64 input.
pub(super) fn rounded_cbrt_bits(input: u64, hint: u64) -> u64 {
    let exponent = ((input >> 52) as i32 - 1023).div_euclid(3);
    let mut lo = ((exponent + 1023) as u64) << 52;
    let mut hi = lo + SIGNIFICAND_BIT;
    let hint = hint.clamp(lo, hi);
    if midpoint_cube_below(hint, input) {
        if !midpoint_cube_below(hint + 1, input) {
            return hint + 1;
        }
        lo = hint + 2;
    } else {
        if midpoint_cube_below(hint - 1, input) {
            return hint;
        }
        if midpoint_cube_below(hint - 2, input) {
            return hint - 1;
        }
        hi = hint - 1;
    }
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if midpoint_cube_below(mid, input) {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_cube_roots_are_rounded_even_with_arbitrary_hints() {
        // Independent exact rational midpoint oracle (integer arithmetic on
        // binary64 as_integer_ratio). Include binade boundaries and extrema.
        let cases = [
            (0x3ff0_0000_0000_0000, 0x3ff0_0000_0000_0000),
            (0x3fef_ffff_ffff_ffff, 0x3ff0_0000_0000_0000),
            (0x3ff0_0000_0000_0001, 0x3ff0_0000_0000_0000),
            (0x3fc0_0000_0000_0000, 0x3fe0_0000_0000_0000),
            (0x3fbf_ffff_ffff_ffff, 0x3fe0_0000_0000_0000),
            (0x3fc0_0000_0000_0001, 0x3fe0_0000_0000_0000),
            (0x3ef0_0000_0000_0000, 0x3f99_65fe_a53d_6e3d),
            (0x3ef0_0000_0000_0001, 0x3f99_65fe_a53d_6e3d),
            (0x3ef0_5faf_9b11_65e1, 0x3f99_983c_7490_a95e),
            (0x3fe0_0000_0000_0000, 0x3fe9_65fe_a53d_6e3d),
            (0x3fdf_ffff_ffff_ffff, 0x3fe9_65fe_a53d_6e3c),
            (0x3fe0_0000_0000_0001, 0x3fe9_65fe_a53d_6e3d),
            (0x4008_0000_0000_0000, 0x3ff7_1374_4912_3ef6),
            (0x0010_0000_0000_0000, 0x2aa4_28a2_f98d_728b),
            (0x7fef_ffff_ffff_ffff, 0x5544_28a2_f98d_728b),
        ];
        for (input, expected) in cases {
            for hint in [
                0,
                u64::MAX,
                expected - 2,
                expected - 1,
                expected,
                expected + 1,
                expected + 2,
                f64::from_bits(input).cbrt().to_bits(),
            ] {
                assert_eq!(
                    rounded_cbrt_bits(input, hint),
                    expected,
                    "{input:x} {hint:x}"
                );
            }
        }
    }
}
