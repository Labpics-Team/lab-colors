//! Current point representation used while **Labpics Colors Space (LCS)** is
//! being reduced to one context-bound coordinate contract. The stored
//! CAM16-UCS/Oklab views are implementation inputs, not independent editable
//! definitions of LCS and not a claim of uniform perceptual attributes.

use crate::Srgb8;
use crate::spaces::srgb::{srgb_linear_from_srgb8, srgb_to_xyz, xyz_to_srgb};
use crate::spaces::{cam16, cat16, oklab, vc::ViewingConditions};

/// A physical submanifold that must survive coordinate transforms until output.
///
/// This is private because callers describe stimuli, not implementation flags.
/// The locus prevents matrix round-off from inventing a chromatic direction
/// where the exact encoded source has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PhysicalLocus {
    General,
    SrgbGrayAxis,
}

/// All hue fields (`h_ok`, `h_cam`) are stored in **degrees** `[0, 360)`.
/// Convert to radians only at trigonometric call sites — never store radians.
///
/// Coordinates are read-only outside this module because `locus` records a
/// physical representation invariant. Mutating one coordinate independently
/// would make conversion and measurement observe two different colours.
///
/// ```compile_fail
/// use labcolors_core::LcsColor;
/// let mut color = LcsColor::from_hex("#808080").unwrap();
/// color.s = 1.0;
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LcsColor {
    jp: f64,
    h_ok: f64,
    /// Internal reparameterisation of CAM16-UCS colourfulness `M′`:
    /// `s = M′ / (J′ + 1)`. The `+ 1` is a regulariser against division by zero
    /// as `J′ → 0`; `LcsColor::mp` applies the analytical inverse
    /// `s · (J′ + 1)`, subject to ordinary binary64 round-off. This is NOT the
    /// CAM16 saturation correlate.
    s: f64,
    h_cam: f64,
    locus: PhysicalLocus,
}

impl LcsColor {
    /// CAM16-UCS `J′` coordinate under the construction viewing conditions.
    pub fn jp(&self) -> f64 {
        self.jp
    }

    /// Oklab hue angle in degrees, canonicalised to zero on exact sRGB grays.
    pub fn h_ok(&self) -> f64 {
        self.h_ok
    }

    /// Internal chroma reparameterisation; not a client-owned intent axis.
    pub fn s(&self) -> f64 {
        self.s
    }

    /// Parse from hex using standard sRGB viewing conditions (average surround).
    pub fn from_hex(hex: &str) -> Result<Self, String> {
        Self::from_hex_with_vc(hex, &ViewingConditions::srgb())
    }

    /// Parse from hex using the given viewing conditions.
    ///
    /// The stored CAM16-UCS/Oklab coordinates are evaluated under the provided
    /// VC (e.g. [`ViewingConditions::dim_surround`] for dark themes). They are
    /// implementation inputs, not universal perceptual-attribute scales.
    pub fn from_hex_with_vc(hex: &str, vc: &ViewingConditions) -> Result<Self, String> {
        let encoded = Srgb8::new(crate::srgb8::hex_bytes(hex)?);
        Ok(Self::from_srgb8_with_vc(encoded, vc))
    }

    /// Build the appearance view of one exact emitted sRGB8 stimulus.
    ///
    /// Parsing and solver output both enter here so representation facts such
    /// as the exact grey axis cannot diverge between two views of the same
    /// bytes.
    pub(crate) fn from_srgb8_with_vc(encoded: Srgb8, vc: &ViewingConditions) -> Self {
        let rgb = srgb_linear_from_srgb8(encoded);
        let xyz = srgb_to_xyz(rgb);
        let h_ok = oklab::oklab_hue(rgb);
        let mut color = Self::from_xyz_with_hok(xyz, h_ok, vc);
        if encoded.is_achromatic() {
            // Hue is undefined on the exact encoded gray axis.  Oklab matrix
            // round-off otherwise turns that absence into a discontinuous
            // numeric angle at authored byte anchors, while interpolated gray
            // points carry the canonical zero representation.
            color.h_ok = 0.0;
            color.locus = PhysicalLocus::SrgbGrayAxis;
        }
        color
    }

    /// Convert to hex using standard sRGB viewing conditions.
    pub fn to_hex(&self) -> String {
        self.to_hex_with_vc(&ViewingConditions::srgb())
    }

    /// Convert to hex using the given viewing conditions.
    ///
    /// Must use the same VC that was used to construct this colour, otherwise
    /// the round-trip will introduce drift.
    pub fn to_hex_with_vc(&self, vc: &ViewingConditions) -> String {
        self.to_srgb8_with_vc(vc).to_hex()
    }

    /// Quantise through the same typed final-output boundary used by curves.
    pub(crate) fn to_srgb8_with_vc(self, vc: &ViewingConditions) -> crate::Srgb8 {
        crate::spaces::srgb::srgb8_from_linear(self.to_linear_srgb_with_vc(vc))
    }

    /// Raw constructor from already-valid coordinates (curves, solver).
    /// Inputs come from internal maths; a non-finite value is therefore an
    /// invariant bug and must never be converted into a plausible display byte.
    pub(crate) fn new(jp: f64, h_ok: f64, s: f64, h_cam: f64) -> Self {
        assert!(
            [jp, h_ok, s, h_cam].into_iter().all(f64::is_finite),
            "internal LCS coordinates must be finite"
        );
        Self {
            jp,
            h_ok,
            s,
            h_cam,
            locus: PhysicalLocus::General,
        }
    }

    /// Construct the continuous physical point on the sRGB gray axis whose
    /// CAM16-UCS lightness is `jp` under `vc`.
    ///
    /// The point is never snapped to a byte. Its locus only constrains the final
    /// output conversion to one shared channel before ordinary sRGB rounding.
    pub(crate) fn from_srgb_gray_axis_jp(jp: f64, vc: &ViewingConditions) -> Self {
        let y = srgb_gray_linear_at_jp(jp, vc);
        let mut color = Self::from_xyz_with_hok(srgb_to_xyz([y; 3]), 0.0, vc);
        // `y` is the analytic inverse of this requested coordinate. Preserve
        // that coordinate exactly instead of feeding Newton/forward round-off
        // back into the continuous curve skeleton.
        color.jp = jp;
        color.locus = PhysicalLocus::SrgbGrayAxis;
        color
    }

    /// CAM16-UCS colourfulness correlate `M'`, recovered through the analytical
    /// inverse of the stored reparameterisation (see the `s` field doc).
    pub(crate) fn mp(&self) -> f64 {
        self.s * (self.jp + 1.0)
    }

    /// The CAM16-UCS colourfulness `M'` of an in-gamut **linear** sRGB colour,
    /// computed straight through the forward CAM16 path with no hex round-trip.
    ///
    /// `M'` does not depend on the Oklab hue carried alongside it, so this skips
    /// the `oklab_hue` step too: it is purely `rgb → XYZ → CAM16 → M'`. It is the
    /// allocation-free equivalent of `from_hex_with_vc(hex_from_srgb(rgb))?.mp()`
    /// for callers that have already quantised `rgb` to the display grid.
    pub(crate) fn mp_of_linear_srgb(rgb: [f64; 3], vc: &ViewingConditions) -> f64 {
        let xyz = srgb_to_xyz(rgb);
        // h_ok is irrelevant to M'; pass 0.0 to avoid the oklab_hue computation.
        Self::from_xyz_with_hok(xyz, 0.0, vc).mp()
    }

    /// CAM16 hue in degrees. Field is private (accessor-only) so the two hue
    /// spaces can't be mixed up: `h_ok` (Oklab) is the geometric hue, `h_cam`
    /// feeds only CAM16 inverse/appearance maths.
    pub(crate) fn h_cam(&self) -> f64 {
        self.h_cam
    }

    pub(crate) fn from_xyz_with_hok(xyz: [f64; 3], h_ok: f64, vc: &ViewingConditions) -> Self {
        // Single shared CIECAM16 forward pass (issue #19); the UCS rescale is the
        // only step `lcs` adds on top of it.
        let (j, m, h) = cam16::forward(xyz, vc);
        Self::from_cam16(j, m, h, h_ok)
    }

    /// Build from already-computed CIECAM16 correlates `(J, M, h_cam)` plus the
    /// Oklab hue. The UCS rescale is the only work here — no forward pass — so a
    /// caller that already ran [`cam16::forward`] (e.g. [`crate::solve`]'s
    /// `finish`) reuses that result instead of recomputing it.
    pub(crate) fn from_cam16(j: f64, m: f64, h_cam: f64, h_ok: f64) -> Self {
        // CAM16-UCS rescaling (Li et al. 2017, DOI 10.1002/col.22131). This is
        // an analytically invertible coordinate transform used for
        // colour-difference work; binary64 round-off is covered by the shared
        // tolerance tests. No individual J'/M' value is assigned a universal
        // attribute meaning here. The inverse path uses the same helpers.
        let jp = cam16::ucs_j(j);
        let mp = cam16::ucs_m(m);
        let s = mp / (jp + 1.0);

        Self::new(jp, h_ok, s, h_cam)
    }

    fn to_linear_srgb_with_vc(self, vc: &ViewingConditions) -> [f64; 3] {
        match self.locus {
            PhysicalLocus::General => xyz_to_srgb(self.to_xyz_general(vc)),
            PhysicalLocus::SrgbGrayAxis => [srgb_gray_linear_at_jp(self.jp, vc); 3],
        }
    }

    fn to_xyz_general(self, vc: &ViewingConditions) -> [f64; 3] {
        // Inverse CAM16-UCS rescaling (Li et al. 2017, DOI 10.1002/col.22131),
        // single source of truth in `cam16`.
        let j = cam16::ucs_j_inv(self.jp);
        let m = cam16::ucs_m_inv(self.mp());
        let hr = self.h_cam.to_radians();
        // `hr.cos()` / `hr.sin()` ниже вычислялись дважды каждый; считаем один раз
        // и переиспользуем — байт-идентичный CSE (тот же аргумент, тот же
        // libm-вызов). `e_hue` берёт другой аргумент (`hr + 2.0`) и не трогается.
        let cos_hr = hr.cos();
        let sin_hr = hr.sin();

        let e_hue = 0.25 * ((hr + 2.0).cos() + 3.8);
        // `vc.t_inner` == `(1.64 - 0.29^n)^0.73`, `vc.fl_pow_025` == `fl^0.25`: те
        // же пер-VC константы, что и в прямом ходе, вынесенные из пер-цветовой
        // инверсии. Порядок умножения сохранён → байт-идентично прежнему инлайну
        // `t_inner * vc.fl.powf(0.25)`.
        let t = (m / ((j / 100.0).sqrt() * vc.t_inner * vc.fl_pow_025)).powf(1.0 / 0.9);

        let p1 = e_hue * (50000.0 / 13.0) * vc.nc * vc.nbb;
        let p2 = (vc.aw * (j / 100.0).powf(1.0 / (vc.c * vc.z))) / vc.nbb;
        let gamma = 23.0 * (p2 + 0.305) * t / (23.0 * p1 + 11.0 * t * cos_hr + 108.0 * t * sin_hr);

        let a = gamma * cos_hr;
        let b = gamma * sin_hr;

        let r_a = (460.0 * p2 + 451.0 * a + 288.0 * b) / 1403.0;
        let g_a = (460.0 * p2 - 891.0 * a - 261.0 * b) / 1403.0;
        let b_a = (460.0 * p2 - 220.0 * a - 6300.0 * b) / 1403.0;

        let r_c = cam16::unadapt(r_a, vc.fl);
        let g_c = cam16::unadapt(g_a, vc.fl);
        let b_c = cam16::unadapt(b_a, vc.fl);

        let lms = [r_c / vc.rgb_d[0], g_c / vc.rgb_d[1], b_c / vc.rgb_d[2]];
        let xyz = cat16::cone_to_xyz(lms);

        [xyz[0] / 100.0, xyz[1] / 100.0, xyz[2] / 100.0]
    }
}

/// Invert CAM16-UCS lightness on the achromatic D65 ray.
///
/// This inverse is defined only by CAM16 `J` on the D65 gray ray; it does not
/// invoke the separate H-K appearance-brightness diagnostic. CAM16 can retain
/// a small residual `M` on a numerically achromatic stimulus, so no claim of
/// zero chromatic correlate is made here. This scalar is the SSOT for XYZ,
/// continuous linear sRGB and eventual sRGB8 gray emission.
fn srgb_gray_linear_at_jp(jp: f64, vc: &ViewingConditions) -> f64 {
    assert!(jp.is_finite(), "internal gray-axis J′ must be finite");
    if jp <= 0.0 {
        return 0.0;
    }
    let j = cam16::ucs_j_inv(jp);
    assert!(
        j.is_finite() && j > 0.0,
        "internal CAM16 gray-axis J must be finite and positive"
    );
    cam16::gray_y(j, vc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_neutral_base() {
        let original = "#787880";
        let lcs = LcsColor::from_hex(original).unwrap();
        let back = lcs.to_hex();
        assert!(
            back.eq_ignore_ascii_case(original),
            "roundtrip drift: expected {original}, got {back}"
        );
    }

    #[test]
    fn roundtrip_white() {
        let original = "#FFFFFF";
        let lcs = LcsColor::from_hex(original).unwrap();
        let back = lcs.to_hex();
        assert!(
            back.eq_ignore_ascii_case(original),
            "roundtrip drift: expected {original}, got {back}"
        );
    }

    #[test]
    fn roundtrip_dark() {
        let original = "#101012";
        let lcs = LcsColor::from_hex(original).unwrap();
        let back = lcs.to_hex();
        assert!(
            back.eq_ignore_ascii_case(original),
            "roundtrip drift: expected {original}, got {back}"
        );
    }

    #[test]
    fn from_hex_rejects_short_string() {
        assert!(LcsColor::from_hex("#fff").is_err());
    }

    #[test]
    fn h_ok_stable_across_roundtrip() {
        let original = "#787880";
        let lcs1 = LcsColor::from_hex(original).unwrap();
        let back = lcs1.to_hex();
        let lcs2 = LcsColor::from_hex(&back).unwrap();
        assert!(
            (lcs1.h_ok - lcs2.h_ok).abs() < 1e-6,
            "h_ok drift: {} vs {}",
            lcs1.h_ok,
            lcs2.h_ok
        );
    }

    #[test]
    fn roundtrip_dim_surround_midgrey() {
        let vc = ViewingConditions::dim_surround();
        let original = "#787880";
        let lcs = LcsColor::from_hex_with_vc(original, &vc).unwrap();
        let back = lcs.to_hex_with_vc(&vc);
        assert!(
            back.eq_ignore_ascii_case(original),
            "dim roundtrip drift: expected {original}, got {back}"
        );
    }

    #[test]
    fn dim_jp_differs_from_srgb() {
        let vc = ViewingConditions::dim_surround();
        let avg = LcsColor::from_hex("#787880").unwrap();
        let dim = LcsColor::from_hex_with_vc("#787880", &vc).unwrap();
        assert!(
            (avg.jp - dim.jp).abs() > 0.1,
            "same stimulus should produce different J' across VCs: avg={} dim={}",
            avg.jp,
            dim.jp,
        );
    }

    #[test]
    fn wrong_vc_roundtrip_drifts() {
        // Construct with dim VC, convert with srgb VC → should drift
        let dim_vc = ViewingConditions::dim_surround();
        let lcs = LcsColor::from_hex_with_vc("#787880", &dim_vc).unwrap();
        let wrong_hex = lcs.to_hex(); // uses srgb VC — mismatch!
        // The hex will still be valid sRGB, just not matching the original
        assert!(
            !wrong_hex.eq_ignore_ascii_case("#787880"),
            "VC mismatch should cause drift, got {}",
            wrong_hex,
        );
    }

    #[test]
    fn h_cam_stored_in_degrees() {
        // CAM16 hue of sRGB red is tens of degrees; a value below 2π would
        // mean radians leaked into storage.
        let red = LcsColor::from_hex("#FF0000").expect("#FF0000 is a valid hex colour");
        let h = red.h_cam();
        assert!((0.0..360.0).contains(&h), "h_cam out of range: {}", h);
        assert!(
            h > 7.0,
            "red CAM16 hue should be tens of degrees, got {} — radians leak?",
            h
        );
    }
}
