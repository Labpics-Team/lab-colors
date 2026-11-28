//! Oklab perceptual colour-space kernels for direct XYZ(D65) and sRGB paths.
//!
//! Source: Björn Ottosson, "A perceptual color space for image processing"
//! (2020), <https://bottosson.github.io/posts/oklab/>. The matrices below are
//! the 2021-01-25 revision (the values Ottosson pinned after the original post)
//! and were checked against that reference implementation to the last printed
//! digit (audit 2026-07-03).
//!
//! The published model does not send D65 white exactly to `L = 1`: the
//! `LMS_TO_OKLAB` first-row sum gives `L = 0.9999999935 ≠ 1` for `[1, 1, 1]`.
//! That ~6.5e-9 offset is a property of the published matrices themselves, not a
//! porting error, and is absorbed by the `< 1e-6` white-point test below.

#[rustfmt::skip]
pub(crate) const SRGB_TO_LMS: [[f64; 3]; 3] = [
    [0.4122214708, 0.5363325363, 0.0514459929],
    [0.2119034982, 0.6806995451, 0.1073969566],
    [0.0883024619, 0.2817188376, 0.6299787005],
];

#[rustfmt::skip]
pub(crate) const LMS_TO_OKLAB: [[f64; 3]; 3] = [
    [ 0.2104542553,  0.7936177850, -0.0040720468],
    [ 1.9779984951, -2.4285922050,  0.4505937099],
    [ 0.0259040371,  0.7827717662, -0.8086757660],
];

// Direct XYZ(D65) -> LMS projection used by the registered F0 Oklab view.
// These are the CSS Color 4 / 2021 Oklab coefficients. Keeping this projection
// direct is important: an XYZ stimulus is not an encoded or linear-sRGB value,
// and routing it through the inverse sRGB matrix would make an output space an
// accidental part of stimulus geometry.
#[rustfmt::skip]
const XYZ_D65_TO_LMS_OKLAB_20210125: [[f64; 3]; 3] = [
    [0.819_022_437_996_703, 0.3619062600528904, -0.1288737815209879],
    [0.0329836539323885, 0.9292868615863434,  0.0361446663506424],
    [0.0481771893596242, 0.2642395317527308,  0.6335478284694309],
];

#[rustfmt::skip]
pub(crate) const OKLAB_TO_LMS: [[f64; 3]; 3] = [
    [1.0,  0.3963377774,  0.2158037573],
    [1.0, -0.1055613458, -0.0638541728],
    [1.0, -0.0894841775, -1.2914855480],
];

#[rustfmt::skip]
pub(crate) const LMS_TO_SRGB: [[f64; 3]; 3] = [
    [ 4.0767416621, -3.3077115913,  0.2309699292],
    [-1.2684380046,  2.6097574011, -0.3413193965],
    [-0.0041960863, -0.7034186147,  1.7076147010],
];

fn mat_vec_mul(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

pub(crate) fn srgb_linear_to_oklab(rgb: [f64; 3]) -> [f64; 3] {
    mat_vec_mul(LMS_TO_OKLAB, srgb_linear_to_lms_root(rgb))
}

pub(crate) fn srgb_linear_to_lms_root(rgb: [f64; 3]) -> [f64; 3] {
    let lms = mat_vec_mul(SRGB_TO_LMS, rgb);
    [lms[0].cbrt(), lms[1].cbrt(), lms[2].cbrt()]
}

// Every encoded channel is in [0, 1], and the smallest positive product
// is at least 2^-17. Thus each LMS sum is zero or a positive normal f64.
// Positive matrix entries and monotone binary64 operations preserve order.
const _: () = {
    let mut row = 0;
    while row < 3 {
        let mut column = 0;
        while column < 3 {
            let coefficient = SRGB_TO_LMS[row][column];
            assert!(coefficient >= 1.0 / 32.0 && coefficient <= 1.0);
            column += 1;
        }
        row += 1;
    }
};

mod rounded_root;

/// Identity of the operational encoded-color projection shared by all
/// assignment execution profiles. Authored content keeps its own identity.
pub(crate) const SRGB8_OKLAB_PROJECTION_IDENTITY: &[u8] =
    b"labcolors.srgb8-oklab-projection.v1.exact-rounded-lms-roots\0";

fn srgb8_lms_root_uncached(rgb: crate::Srgb8) -> [f64; 3] {
    // The independently replayable nominal proof binds the exact sRGB
    // implementation. Admit its actual decoder once before any cache entry
    // can be produced; the matrix coefficients are checked at compile time.
    static DECODE: std::sync::Once = std::sync::Once::new();
    DECODE.call_once(|| {
        use crate::spaces::srgb::decode_8bit;
        assert!(decode_8bit(0) == 0.0 && decode_8bit(1) >= 1.0 / 4096.0);
        for i in 1..=255_u8 {
            let decoded = decode_8bit(i);
            assert!(decoded > decode_8bit(i - 1) && decoded <= 1.0);
        }
    });
    let lms = mat_vec_mul(
        SRGB_TO_LMS,
        crate::spaces::srgb::srgb_linear_from_srgb8(rgb),
    );
    lms.map(|value| {
        if value == 0.0 {
            0.0
        } else {
            f64::from_bits(rounded_root::rounded_cbrt_bits(
                value.to_bits(),
                value.cbrt().to_bits(),
            ))
        }
    })
}

pub(crate) fn srgb8_lms_root(rgb: crate::Srgb8) -> [f64; 3] {
    use core::cell::Cell;

    // Point colors and interval corners share the exact same three roots.
    // A bounded, per-thread working set retains only this pure projection.
    type Slot = Cell<Option<(crate::Srgb8, [f64; 3])>>;
    // This power of two caps the per-thread cache at 8 KiB with 32-byte slots.
    const LMS_ROOT_CACHE_SLOTS: usize = 1 << 8;
    thread_local! {
        static ROOTS: [Slot; LMS_ROOT_CACHE_SLOTS] = const { [const { Cell::new(None) }; LMS_ROOT_CACHE_SLOTS] };
    }
    let [r, g, b] = rgb.bytes();
    let key = u32::from_be_bytes([0, r, g, b]);
    // The high bits of a product with xxHash's PRIME32_1 spread RGB keys over
    // the slots. Only the full RGB equality below can reuse a projection.
    let index =
        (key.wrapping_mul(0x9e37_79b1) >> (u32::BITS - LMS_ROOT_CACHE_SLOTS.ilog2())) as usize;
    ROOTS.with(|slots| {
        let slot = &slots[index];
        if let Some((color, roots)) = slot.get() {
            if color == rgb {
                return roots;
            }
        }
        let roots = srgb8_lms_root_uncached(rgb);
        slot.set(Some((rgb, roots)));
        roots
    })
}

pub(crate) fn srgb8_to_oklab(rgb: crate::Srgb8) -> [f64; 3] {
    mat_vec_mul(LMS_TO_OKLAB, srgb8_lms_root(rgb))
}

/// Exact finite gray-axis projection, shared by point and region admission.
/// No claim about monotonicity of the second (signed) Oklab matrix is needed.
pub(crate) fn srgb8_neutral_lightness() -> &'static [f64; 256] {
    static LIGHTNESS: std::sync::OnceLock<[f64; 256]> = std::sync::OnceLock::new();
    LIGHTNESS.get_or_init(|| {
        std::array::from_fn(|gray| srgb8_to_oklab(crate::Srgb8::new([gray as u8; 3]))[0])
    })
}

/// Границы машинного Oklab для RGB8-box. Таблица decode допускается один раз,
/// положительные коэффициенты LMS проверяются при компиляции, корень корректно округлён,
/// поэтому все три координаты LMS монотонны на всей RGB8-решётке.
/// Знаки коэффициентов и порядок операций совпадают с point-проекцией.
pub(crate) fn srgb8_box_oklab_bounds(lower: [u8; 3], upper: [u8; 3]) -> ([f64; 3], [f64; 3]) {
    use crate::Srgb8;
    let lo = srgb8_lms_root(Srgb8::new(lower));
    let hi = srgb8_lms_root(Srgb8::new(upper));
    let mut minimum = [0.0; 3];
    let mut maximum = [0.0; 3];
    for (index, row) in LMS_TO_OKLAB.iter().enumerate() {
        let low: [f64; 3] = std::array::from_fn(|i| if row[i] < 0.0 { hi[i] } else { lo[i] });
        let high: [f64; 3] = std::array::from_fn(|i| if row[i] < 0.0 { lo[i] } else { hi[i] });
        minimum[index] = row[0] * low[0] + row[1] * low[1] + row[2] * low[2];
        maximum[index] = row[0] * high[0] + row[1] * high[1] + row[2] * high[2];
    }
    (minimum, maximum)
}

fn srgb8_gamut_oklab_bounds() -> ([f64; 3], [f64; 3]) {
    static BOUNDS: std::sync::OnceLock<([f64; 3], [f64; 3])> = std::sync::OnceLock::new();
    *BOUNDS.get_or_init(|| {
        // Cover every RGB8 point by 512 disjoint 32×32×32 cells. Each cell uses
        // the same machine-operation enclosure as a query box. Their union
        // bounds the whole gamut without pretending the three LMS roots are
        // independent over the entire cube. This one-time refinement changes
        // no color policy and requires no stored platform-dependent constants.
        let mut minimum = [f64::INFINITY; 3];
        let mut maximum = [f64::NEG_INFINITY; 3];
        for r in 0_u8..8 {
            for g in 0_u8..8 {
                for b in 0_u8..8 {
                    let lower = [r, g, b].map(|cell| cell * 32);
                    let upper = lower.map(|channel| channel + 31);
                    let (lo, hi) = srgb8_box_oklab_bounds(lower, upper);
                    for axis in 0..3 {
                        minimum[axis] = minimum[axis].min(lo[axis]);
                        maximum[axis] = maximum[axis].max(hi[axis]);
                    }
                }
            }
        }
        (minimum, maximum)
    })
}

/// Conservative distance range shared by search pruning and material admission.
pub(crate) fn srgb8_boxes_distance_bounds(
    left: ([u8; 3], [u8; 3]),
    right: ([u8; 3], [u8; 3]),
) -> [f64; 2] {
    let (gamut_lo, gamut_hi) = srgb8_gamut_oklab_bounds();
    let bounded = |rgb: ([u8; 3], [u8; 3])| {
        let (lo, hi) = srgb8_box_oklab_bounds(rgb.0, rgb.1);
        (
            std::array::from_fn::<_, 3, _>(|axis| lo[axis].max(gamut_lo[axis])),
            std::array::from_fn::<_, 3, _>(|axis| hi[axis].min(gamut_hi[axis])),
        )
    };
    let (left_lo, left_hi) = bounded(left);
    let (right_lo, right_hi) = bounded(right);
    let lower = std::array::from_fn::<_, 3, _>(|i| {
        (left_lo[i] - right_hi[i])
            .max(right_lo[i] - left_hi[i])
            .max(0.0)
    });
    let upper = std::array::from_fn::<_, 3, _>(|i| {
        (left_hi[i] - right_lo[i])
            .abs()
            .max((right_hi[i] - left_lo[i]).abs())
    });
    let norm = |v: [f64; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [norm(lower), norm(upper)]
}

/// Direct 2021 Oklab projection of one relative XYZ(D65) stimulus.
///
/// The caller owns frame admission. This kernel does not perform chromatic
/// adaptation, infer an output profile, clamp coordinates or provide an
/// inverse/editing operation.
#[inline]
pub(crate) fn xyz_d65_to_oklab_v1(xyz: [f64; 3]) -> [f64; 3] {
    let lms = mat_vec_mul(XYZ_D65_TO_LMS_OKLAB_20210125, xyz);
    let lms_root = [lms[0].cbrt(), lms[1].cbrt(), lms[2].cbrt()];
    mat_vec_mul(LMS_TO_OKLAB, lms_root)
}

pub(crate) fn oklab_to_srgb_linear(lab: [f64; 3]) -> [f64; 3] {
    let lms_ = mat_vec_mul(OKLAB_TO_LMS, lab);
    let lms = [lms_[0].powi(3), lms_[1].powi(3), lms_[2].powi(3)];
    mat_vec_mul(LMS_TO_SRGB, lms)
}

/// Exact linear-sRGB grey at an Oklab lightness coordinate.
///
/// On the neutral Oklab ray `a = b = 0`, the first column of `OKLAB_TO_LMS` is
/// exactly one, so all three cube-root LMS channels equal `L`; cubing gives
/// `L³`. Every row of `LMS_TO_SRGB` sums to one, hence every output channel is
/// the same `L³`. Sharing that scalar directly avoids row-specific round-off.
pub(crate) fn neutral_srgb_linear(l_ok: f64) -> [f64; 3] {
    let channel = (l_ok * l_ok * l_ok).clamp(0.0, 1.0);
    [channel; 3]
}

pub(crate) fn oklab_hue(rgb: [f64; 3]) -> f64 {
    // Хью зависит только от Oklab-компонент `a`, `b` (строки 1, 2 матрицы
    // `LMS_TO_OKLAB`) — светлота `L` (строка 0) не участвует. Считаем ровно эти
    // две проекции нелинейной `lms_`, не гоняя полный `srgb_linear_to_oklab` с
    // выбрасываемой строкой `L`. Значения `a`, `b` — те же независимые скалярные
    // произведения (те же операнды, тот же порядок слева-направо), что даёт полный
    // matmul, поэтому результат байт-идентичен; пинится golden/reference-векторами
    // (`h_ok`). Пропуск строки `L` убирает одно скалярное произведение и, главное,
    // одну ветку зависимостей, которую LLVM не всегда вычищает через границу
    // `mat_vec_mul` (массив `[f64; 3]` целиком).
    let lms = mat_vec_mul(SRGB_TO_LMS, rgb);
    let lms_ = [lms[0].cbrt(), lms[1].cbrt(), lms[2].cbrt()];
    let a =
        LMS_TO_OKLAB[1][0] * lms_[0] + LMS_TO_OKLAB[1][1] * lms_[1] + LMS_TO_OKLAB[1][2] * lms_[2];
    let b =
        LMS_TO_OKLAB[2][0] * lms_[0] + LMS_TO_OKLAB[2][1] * lms_[1] + LMS_TO_OKLAB[2][2] * lms_[2];
    // `atan2().to_degrees()` лежит в (−180, 180], поэтому `rem_euclid(360)` здесь
    // тождественно равен одной условной прибавке 360 (для отрицательного угла) —
    // байт-идентично, но без floating-modulo. Та же замена, что в `cam16::
    // forward_compute`; байт-гейтится golden/reference-векторами (`h_ok`).
    let deg = b.atan2(a).to_degrees();
    if deg < 0.0 { deg + 360.0 } else { deg }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spaces::srgb::srgb_from_hex;

    #[test]
    fn repeatedly_projected_and_evicted_bytes_match_uncached_projection_to_the_bit() {
        for index in (0..4096_u32).chain((0..4096_u32).rev()) {
            let bytes = index.wrapping_mul(0x7f4a_7c15).to_be_bytes();
            let rgb = crate::Srgb8::new([bytes[1], bytes[2], bytes[3]]);
            let expected = mat_vec_mul(LMS_TO_OKLAB, srgb8_lms_root_uncached(rgb));
            for _ in 0..2 {
                assert_eq!(
                    srgb8_to_oklab(rgb).map(f64::to_bits),
                    expected.map(f64::to_bits)
                );
            }
        }
    }

    #[test]
    fn gamut_bound_contains_cell_boundaries_and_rgb_axis_crossings() {
        let (lower, upper) = srgb8_gamut_oklab_bounds();
        let endpoints = [
            0, 31, 32, 63, 64, 95, 96, 127, 128, 159, 160, 191, 192, 223, 224, 255,
        ];
        for first in endpoints {
            for second in endpoints {
                for varying in 0..=255_u8 {
                    for rgb in [
                        [first, second, varying],
                        [first, varying, second],
                        [varying, first, second],
                    ] {
                        let point = srgb8_to_oklab(crate::Srgb8::new(rgb));
                        assert!(
                            (0..3).all(
                                |axis| lower[axis] <= point[axis] && point[axis] <= upper[axis]
                            ),
                            "{rgb:?}: {point:?}"
                        );
                    }
                }
            }
        }
        let distance = srgb8_boxes_distance_bounds(([27; 3], [255; 3]), ([28; 3], [255; 3]));
        assert_eq!(distance[0], 0.0);
        assert!(
            distance[1] < 2.0,
            "RGB uncertainty cannot invent an out-of-gamut distance"
        );
    }

    #[test]
    fn white_gives_l1_a0_b0() {
        let lab = srgb_linear_to_oklab([1.0, 1.0, 1.0]);
        assert!((lab[0] - 1.0).abs() < 1e-6, "L={}", lab[0]);
        assert!(lab[1].abs() < 1e-6, "a={}", lab[1]);
        assert!(lab[2].abs() < 1e-6, "b={}", lab[2]);
    }

    #[test]
    fn roundtrip_five_colors() {
        let hexes = ["#FF0000", "#00FF00", "#0000FF", "#787880", "#FFD700"];
        for hex in hexes {
            let lin = srgb_from_hex(hex).unwrap();
            let lab = srgb_linear_to_oklab(lin);
            let back = oklab_to_srgb_linear(lab);
            for i in 0..3 {
                assert!(
                    (lin[i] - back[i]).abs() < 1e-6,
                    "{hex} channel {i}: expected {}, got {}",
                    lin[i],
                    back[i]
                );
            }
        }
    }

    #[test]
    fn pure_red_has_positive_a() {
        let lin = srgb_from_hex("#FF0000").unwrap();
        let lab = srgb_linear_to_oklab(lin);
        assert!(lab[1] > 0.0, "a={} should be positive for red", lab[1]);
    }

    #[test]
    fn pure_blue_has_negative_b() {
        let lin = srgb_from_hex("#0000FF").unwrap();
        let lab = srgb_linear_to_oklab(lin);
        assert!(lab[2] < 0.0, "b={} should be negative for blue", lab[2]);
    }

    #[test]
    fn hue_returns_degrees_0_360() {
        // Red ≈ 24.5°, Green ≈ 142°, Blue ≈ 264° — Oklab canonical values
        let lin_r = srgb_from_hex("#FF0000").unwrap();
        let lin_g = srgb_from_hex("#00FF00").unwrap();
        let lin_b = srgb_from_hex("#0000FF").unwrap();

        let h_r = oklab_hue(lin_r);
        let h_g = oklab_hue(lin_g);
        let h_b = oklab_hue(lin_b);

        // All hues in [0, 360)
        for &h in &[h_r, h_g, h_b] {
            assert!((0.0..360.0).contains(&h), "hue {} not in [0, 360)", h);
        }

        // Red quadrant (≈29°)
        assert!(
            (h_r - 29.2).abs() < 1.0,
            "red hue = {}°, expected ≈29.2°",
            h_r
        );
        // Green quadrant (≈142°)
        assert!(
            (h_g - 142.0).abs() < 3.0,
            "green hue = {}°, expected ≈142°",
            h_g
        );
        // Blue quadrant (≈264°)
        assert!(
            (h_b - 264.0).abs() < 3.0,
            "blue hue = {}°, expected ≈264°",
            h_b
        );
    }
}
