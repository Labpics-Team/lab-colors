//! Core CIECAM16 nonlinear adaptation functions and the shared forward pass.
//!
//! These are the forward and inverse compressive transforms applied
//! to cone responses after chromatic adaptation, plus the single CIECAM16
//! forward pass `XYZ → (J, M, h)` that both [`crate::lcs`] and [`crate::lpc`]
//! build on, and the CAM16-UCS rescaling helpers (`J ↔ J'`, `M ↔ M'`).

use crate::spaces::cat16;
use crate::spaces::vc::ViewingConditions;

#[cfg(test)]
thread_local! {
    /// Test-only per-thread counter of [`forward`] invocations. Powers the
    /// deterministic `cam16_forwards_per_set_regression_guard` test, which pins
    /// the count of CIECAM16 forward passes a default `resolve_set` runs — the
    /// honest, noise-free "before/after" metric for the discrete-exactness perf
    /// work (wall-time on a loaded box is too variable to measure a few-percent
    /// delta). Thread-local, not a global atomic, so the test runner's parallel
    /// tests cannot pollute the count.
    pub(crate) static FORWARD_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Forward nonlinear adaptation.
///
/// Source: CIE 170-2:2015 eq. (6.5).
pub(crate) fn adapt(c: f64, fl: f64) -> f64 {
    let x = fl * c.abs() / 100.0;
    let y = x.powf(0.42);
    c.signum() * 400.0 * y / (y + 27.13)
}

/// Inverse nonlinear adaptation.
pub(crate) fn unadapt(a: f64, fl: f64) -> f64 {
    let x = a.abs();
    let y = (27.13 * x / (400.0 - x)).max(0.0);
    a.signum() * 100.0 * y.powf(1.0 / 0.42) / fl
}

// Per-`resolve_set` memoization of the `forward` pass, keyed on the input `XYZ`
// bit pattern. Within one `resolve_set` the viewing conditions are fixed, so the
// forward is a pure function of `XYZ` alone — and the curve refine fixed-point
// and the text-hierarchy pass re-measure the same candidate colours, making
// 25–33% of the forwards exact repeats (measured on the default table). The
// cache is live only for the span of a set (see `ForwardCacheGuard`) and cleared
// on entry and exit, so it never aliases across viewing conditions and cannot
// grow unbounded; outside that span (`active == false`) it is transparent. It
// returns the bit-identical tuple the math would have produced — pure
// memoization, no numeric movement.
thread_local! {
    static FORWARD_CACHE: std::cell::RefCell<ForwardCache> =
        std::cell::RefCell::new(ForwardCache {
            active: false,
            map: XyzMap::default(),
        });
}

type XyzMap =
    std::collections::HashMap<[u64; 3], (f64, f64, f64), std::hash::BuildHasherDefault<XyzHasher>>;

struct ForwardCache {
    active: bool,
    map: XyzMap,
}

/// A minimal multiply-xor hasher for the `[u64; 3]` `XYZ`-bits key.
///
/// The default `SipHash` on a 24-byte key costs more than the CIECAM16 forward
/// the cache is meant to save, erasing the win on native. The key is already
/// three near-random `f64` bit patterns, so a single multiply-xor round per word
/// disperses them well enough for a small per-set table (a few hundred entries,
/// no adversarial input — these are colour coordinates, not untrusted data).
#[derive(Default)]
struct XyzHasher(u64);

impl std::hash::Hasher for XyzHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        // Not on the hot path ([u64; 3] hashes via `write_u64`), but required by
        // the trait — fold any stray bytes in so the impl stays total.
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x517c_c1b7_2722_0a95);
        }
    }
    fn write_u64(&mut self, i: u64) {
        self.0 = (self.0 ^ i).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

/// RAII activation of the [`FORWARD_CACHE`] for the lifetime of one `resolve_set`.
///
/// Activating clears any prior contents and enables caching; dropping restores
/// the previous active state and clears the map. Because the cache is keyed on
/// `XYZ` alone, it is correct only while a single viewing condition is in flight
/// — clearing on both edges guarantees that, even under (today never) nesting.
pub(crate) struct ForwardCacheGuard {
    prev_active: bool,
}

impl ForwardCacheGuard {
    /// Activate caching for the enclosing scope; the returned guard deactivates
    /// and clears it on drop.
    pub(crate) fn activate() -> Self {
        let prev_active = FORWARD_CACHE.with(|c| {
            let mut c = c.borrow_mut();
            let prev = c.active;
            c.active = true;
            c.map.clear();
            prev
        });
        Self { prev_active }
    }
}

impl Drop for ForwardCacheGuard {
    fn drop(&mut self) {
        let prev_active = self.prev_active;
        FORWARD_CACHE.with(|c| {
            let mut c = c.borrow_mut();
            c.active = prev_active;
            c.map.clear();
        });
    }
}

/// CIECAM16 correlates `(J, M, h)` for an XYZ stimulus (`Y` normalised to 1).
///
/// `h` is the CAM16 hue angle in **degrees** `[0, 360)`. This is the single
/// CIECAM16 forward pass: [`crate::lcs::LcsColor::from_xyz_with_hok`] applies the
/// CAM16-UCS rescale ([`ucs_j`] / [`ucs_m`]) on top of it, and
/// [`crate::lpc::cam16_jch_from_xyz`] is a thin re-export. Keeping one copy makes
/// a CAM16 matrix or surround change land in exactly one place (issue #19).
///
/// When the [`FORWARD_CACHE`] is active (inside a `resolve_set`) a repeated
/// `XYZ` is served from the table — the same bits, not a re-derivation — so the
/// `FORWARD_CALLS` counter and the per-set forward count reflect *distinct*
/// computations, the honest measure of real CAM16 work.
pub(crate) fn forward(xyz: [f64; 3], vc: &ViewingConditions) -> (f64, f64, f64) {
    let key = [xyz[0].to_bits(), xyz[1].to_bits(), xyz[2].to_bits()];
    if let Some(hit) = FORWARD_CACHE.with(|c| {
        let c = c.borrow();
        if c.active {
            c.map.get(&key).copied()
        } else {
            None
        }
    }) {
        return hit;
    }
    #[cfg(test)]
    FORWARD_CALLS.with(|c| c.set(c.get() + 1));
    let result = forward_compute(xyz, vc);
    FORWARD_CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.active {
            c.map.insert(key, result);
        }
    });
    result
}

/// The CIECAM16 forward math itself (cache-free); see [`forward`].
fn forward_compute(xyz: [f64; 3], vc: &ViewingConditions) -> (f64, f64, f64) {
    let xyz = [xyz[0] * 100.0, xyz[1] * 100.0, xyz[2] * 100.0];

    let lms = cat16::xyz_to_cone(xyz);
    let lms_a = [
        lms[0] * vc.rgb_d[0],
        lms[1] * vc.rgb_d[1],
        lms[2] * vc.rgb_d[2],
    ];
    let lms_aa = [
        adapt(lms_a[0], vc.fl),
        adapt(lms_a[1], vc.fl),
        adapt(lms_a[2], vc.fl),
    ];

    let a = lms_aa[0] - 12.0 * lms_aa[1] / 11.0 + lms_aa[2] / 11.0;
    let b = (lms_aa[0] + lms_aa[1] - 2.0 * lms_aa[2]) / 9.0;
    let h = b.atan2(a).to_degrees().rem_euclid(360.0);
    let hr = h.to_radians();

    let e_hue = 0.25 * ((hr + 2.0).cos() + 3.8);
    let a_achrom = (2.0 * lms_aa[0] + lms_aa[1] + lms_aa[2] / 20.0) * vc.nbb;
    let j = 100.0 * (a_achrom / vc.aw).powf(vc.c * vc.z);

    let u = (a * a + b * b).sqrt();
    let t = (50000.0 / 13.0) * e_hue * vc.nc * vc.nbb * u
        / (lms_aa[0] + lms_aa[1] + 1.05 * lms_aa[2] + 0.305);
    let m = t.powf(0.9)
        * (j / 100.0).sqrt()
        * (1.64 - 0.29_f64.powf(vc.n)).powf(0.73)
        * vc.fl.powf(0.25);

    (j, m, h)
}

// ------------------------------------------------------------------
//  CAM16-UCS rescaling — Li et al. 2017, DOI 10.1002/col.22131.
// ------------------------------------------------------------------
//
//   J' = 1.7·J / (1 + 0.007·J),   M' = ln(1 + 0.0228·M) / 0.0228.
//
// Maps raw CIECAM16 J/M onto perceptually uniform J'/M' (J'=50 reads as
// half-lightness). These four helpers are the SINGLE SOURCE OF TRUTH for the
// rescale: `lcs` stores J'/M', `lpc` decompresses back to raw J/M, and the
// constants (`1.7`, `0.007`, `0.0228`) must never be re-typed inline anywhere
// else (previously duplicated across `lcs::from_xyz_with_hok`, `lcs::to_xyz`,
// and `lpc::y_hk_from_lcs`).

/// CAM16-UCS lightness rescale `J → J'`.
pub(crate) fn ucs_j(j: f64) -> f64 {
    1.7 * j / (1.0 + 0.007 * j)
}

/// Inverse CAM16-UCS lightness rescale `J' → J`.
pub(crate) fn ucs_j_inv(jp: f64) -> f64 {
    jp / (1.7 - 0.007 * jp)
}

/// CAM16-UCS colourfulness rescale `M → M'`.
pub(crate) fn ucs_m(m: f64) -> f64 {
    (1.0 + 0.0228 * m).ln() / 0.0228
}

/// Inverse CAM16-UCS colourfulness rescale `M' → M`.
pub(crate) fn ucs_m_inv(mp: f64) -> f64 {
    (0.0228 * mp).exp_m1() / 0.0228
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spaces::srgb::{srgb_from_hex, srgb_to_xyz};

    /// Frozen reference: the CIECAM16 forward math exactly as it stood inline in
    /// `lcs::from_xyz_with_hok` / `lpc::cam16_jch_from_xyz` before issue #19
    /// merged the two byte-identical copies into [`forward`]. This is the *old
    /// path*; the test below proves [`forward`] reproduces it bit-for-bit, so the
    /// dedup is a pure refactor with zero numeric movement.
    fn forward_reference(xyz: [f64; 3], vc: &ViewingConditions) -> (f64, f64, f64) {
        let xyz = [xyz[0] * 100.0, xyz[1] * 100.0, xyz[2] * 100.0];

        let lms = cat16::xyz_to_cone(xyz);
        let lms_a = [
            lms[0] * vc.rgb_d[0],
            lms[1] * vc.rgb_d[1],
            lms[2] * vc.rgb_d[2],
        ];
        let lms_aa = [
            adapt(lms_a[0], vc.fl),
            adapt(lms_a[1], vc.fl),
            adapt(lms_a[2], vc.fl),
        ];

        let a = lms_aa[0] - 12.0 * lms_aa[1] / 11.0 + lms_aa[2] / 11.0;
        let b = (lms_aa[0] + lms_aa[1] - 2.0 * lms_aa[2]) / 9.0;
        let h = b.atan2(a).to_degrees().rem_euclid(360.0);
        let hr = h.to_radians();

        let e_hue = 0.25 * ((hr + 2.0).cos() + 3.8);
        let a_achrom = (2.0 * lms_aa[0] + lms_aa[1] + lms_aa[2] / 20.0) * vc.nbb;
        let j = 100.0 * (a_achrom / vc.aw).powf(vc.c * vc.z);

        let u = (a * a + b * b).sqrt();
        let t = (50000.0 / 13.0) * e_hue * vc.nc * vc.nbb * u
            / (lms_aa[0] + lms_aa[1] + 1.05 * lms_aa[2] + 0.305);
        let m = t.powf(0.9)
            * (j / 100.0).sqrt()
            * (1.64 - 0.29_f64.powf(vc.n)).powf(0.73)
            * vc.fl.powf(0.25);

        (j, m, h)
    }

    #[test]
    fn forward_is_bit_identical_to_pre_dedup_path() {
        // BIT-IDENTITY GATE (issue #19): the single shared forward must equal the
        // old inline copy to the last ULP — not "within tolerance". A non-zero
        // delta means the dedup silently moved the math and every downstream
        // golden would have to be re-baselined; this catches it at the source.
        // Grid spans the hue circle plus the achromatic axis and gamut extremes.
        const GRID: [&str; 18] = [
            "#000000", "#FFFFFF", "#7F7F7F", "#787880", "#101012", "#444444", "#FF0000", "#00FF00",
            "#0000FF", "#FFFF00", "#00FFFF", "#FF00FF", "#FF9500", "#34C759", "#007AFF", "#C71585",
            "#008B8B", "#FFD700",
        ];
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            for hex in GRID {
                let xyz = srgb_to_xyz(srgb_from_hex(hex).expect("valid hex"));
                let (j, m, h) = forward(xyz, &vc);
                let (rj, rm, rh) = forward_reference(xyz, &vc);
                assert_eq!(j.to_bits(), rj.to_bits(), "{hex}: J drifted {j} vs {rj}");
                assert_eq!(m.to_bits(), rm.to_bits(), "{hex}: M drifted {m} vs {rm}");
                assert_eq!(h.to_bits(), rh.to_bits(), "{hex}: h drifted {h} vs {rh}");
            }
        }
    }

    #[test]
    fn ucs_rescale_round_trips() {
        // The four UCS helpers are exact inverses across the reachable J/M range,
        // so `lcs` (stores J'/M') and `lpc` (decompresses to J/M) never disagree.
        for j in [0.0_f64, 1.0, 12.5, 50.0, 87.6, 100.0, 103.0] {
            let back = ucs_j_inv(ucs_j(j));
            assert!((back - j).abs() < 1e-12, "ucs_j round-trip j={j}: {back}");
        }
        for m in [0.0_f64, 0.5, 5.0, 20.0, 60.0, 120.0] {
            let back = ucs_m_inv(ucs_m(m));
            assert!((back - m).abs() < 1e-12, "ucs_m round-trip m={m}: {back}");
        }
    }
}
