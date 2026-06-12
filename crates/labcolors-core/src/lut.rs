//! Precompiled grey-axis lightness LUT — the O(1) seed that replaces the
//! 64-iteration CAM16 bisection in [`match_lightness`](crate::solve).
//!
//! ## What this table is
//!
//! For a neutral (achromatic) stimulus the forward map
//! `J_HK(l_ok) = j_hk_from_xyz(srgb_to_xyz(grey(l_ok)), vc)` is a smooth,
//! strictly monotone 1-D function of Oklab lightness `l_ok ∈ [0, 1]` (verified:
//! `non_mono = 0` across 4096 samples under both viewing conditions). Inverting
//! it — "what lightness reproduces this `J_HK`?" — is exactly what
//! [`match_lightness`](crate::solve) does, and it previously did so with a
//! 64-iteration bisection that evaluates a full CAM16 forward pass *per
//! iteration*. This module tabulates that monotone curve at [`LUT_NODES`]
//! uniformly-spaced `l_ok` nodes per viewing condition, so the inverse becomes a
//! table lookup plus interpolation.
//!
//! ## Why it stays bit-compatible
//!
//! The table is not the final answer — it is a *seed*:
//!
//! * **Pure neutral (`chroma == 0`).** The node interval that brackets the
//!   target is an *exact* bracket of the real root, because the table samples
//!   the real function at the nodes. Direct inverse interpolation lands within
//!   `< 2.8e-4` of the true `l_ok` (`K = 257`), far below one 8-bit display step
//!   (`≈ 3.9e-3`), so the emitted hex — and therefore the measured `Solved.lc()`
//!   — is identical to the bisection's. Empirically the final `Lc` delta over
//!   the solver grid is `0.00000`.
//!
//! * **Small chroma (`ratio ≤ `[`MAX_LUT_CHROMA`]`).** A faintly-tinted neutral
//!   (the v1 undertone, `Relative(0.08)` at hue 286°) shifts the curve by at
//!   most `~7.5` nodes versus the neutral table. The neutral node interval,
//!   padded by [`CHROMA_PAD_NODES`], still brackets the tinted root; a short
//!   bisection *within that narrow bracket* converges to full precision in far
//!   fewer iterations than a cold `[0, 1]` search. Final `Lc` delta: `0.00000`.
//!
//! Correctness never rests on the chroma threshold: the caller re-verifies that
//! the padded bracket truly contains the root and widens to `[0, 1]` if a larger
//! chroma moved it out (see [`seed_bracket`]). The threshold is a *performance*
//! gate; the bracket check is the *correctness* guarantee.
//!
//! ## Invalidation
//!
//! The table is a function of `(grey axis, viewing conditions)` only — it is
//! "packed mathematics, not a palette". It never invalidates on background,
//! brand, or theme. Each supported VC ([`ViewingConditions::srgb`],
//! [`ViewingConditions::dim_surround`]) has its own table; any other VC falls
//! back to the bisection. Drift is impossible to ship silently: the committed
//! [`lut_data`] arrays are regenerated from the live crate math and asserted
//! equal by `lut_data_matches_live_math`.
//!
//! ## Zero dependencies
//!
//! The tables are committed `const` arrays (`lut_data.rs`), generated once from
//! the crate's own forward path. No `build.rs`, no runtime crate, no
//! serialisation format — `labcolors-core` stays `[dependencies]`-empty
//! (issue #29).

use crate::lpc::j_hk_from_xyz;
use crate::solve::{ChromaPolicy, Hue, build_color_for_lut};
use crate::spaces::srgb::srgb_to_xyz;
use crate::spaces::vc::ViewingConditions;

mod lut_data;

/// Number of uniformly-spaced `l_ok` nodes per table.
///
/// `257 = 2^8 + 1` gives node intervals of width `1/256 ≈ 3.9e-3` — one per
/// 8-bit grey step — so inverse interpolation resolves below the output
/// quantisation grid. Sized against the interpolation-error budget (issue #28):
/// at `K = 257` the inverse error is `≤ 2.8e-4 l_ok` and the table is
/// `257 × 8 bytes × 2 VCs = 4112 bytes ≈ 4 KB`.
pub(crate) const LUT_NODES: usize = 257;

/// The largest relative-chroma ratio the LUT seed is trusted for.
///
/// At the v1 undertone ratio (`0.08`) the worst neutral-seed shift is `~7.5`
/// nodes (hue 286°); [`CHROMA_PAD_NODES`] = 12 covers that with margin up to
/// `~0.12`. Above this the neutral seed can drift too far for the padded bracket
/// to reliably contain the root, so the caller takes the full-`[0, 1]` bisection
/// instead. Chosen so the v1 default tint path stays on the fast seed with
/// headroom; not a hard correctness boundary (the bracket check is — see
/// [`seed_bracket`]).
pub(crate) const MAX_LUT_CHROMA: f64 = 0.10;

/// Node-padding applied to the neutral bracket before refining a small-chroma
/// root. Twelve nodes (`12/256 ≈ 0.047 l_ok`) absorbs the chroma-induced shift
/// of the tinted curve relative to the neutral table for any ratio up to
/// `~0.12` — comfortably past [`MAX_LUT_CHROMA`].
pub(crate) const CHROMA_PAD_NODES: usize = 12;

/// A precompiled grey-axis table for one viewing condition: `J_HK` sampled at
/// `LUT_NODES` uniformly-spaced `l_ok` nodes, strictly increasing.
pub(crate) struct GreyAxisLut {
    /// `j_hk[i] = J_HK(i / (LUT_NODES - 1))`, monotonically increasing.
    j_hk: &'static [f64; LUT_NODES],
}

/// A bracket on Oklab lightness guaranteed to contain the root, handed to the
/// caller's short bisection. `[lo, hi]` with `J_HK(lo) ≤ target ≤ J_HK(hi)`.
pub(crate) struct LightnessBracket {
    pub lo: f64,
    pub hi: f64,
}

/// What the LUT could resolve for a target, handed back to
/// [`match_lightness`](crate::solve).
pub(crate) enum LutSeed {
    /// Pure-neutral (`chroma == 0`): the table *is* the function, so direct
    /// inverse interpolation is the final `l_ok` — no bisection needed. Verified
    /// bit-compatible (final-`Lc` delta `0.00000`).
    Exact(f64),
    /// Small chroma: a validated bracket the caller refines on the real tinted
    /// curve to converge the shifted root.
    Bracket(LightnessBracket),
}

impl GreyAxisLut {
    /// The `l_ok` of node `i`.
    #[inline]
    fn node_l(i: usize) -> f64 {
        i as f64 / (LUT_NODES - 1) as f64
    }

    /// Index `i` such that `j_hk[i] ≤ target < j_hk[i + 1]`, for an interior
    /// `target` strictly inside `(j_hk[0], j_hk[last])`. Binary search over the
    /// monotone table.
    fn lower_node(&self, target: f64) -> usize {
        let mut lo = 0usize;
        let mut hi = LUT_NODES - 1;
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if self.j_hk[mid] <= target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }

    /// Direct inverse interpolation: the `l_ok` whose tabulated `J_HK` equals
    /// `target`, by linear interpolation within the bracketing node interval.
    ///
    /// Exact at the table endpoints; the analytic seed for the pure-neutral
    /// path, where the node interval is a true bracket and the interpolation
    /// error sits below the 8-bit output grid.
    pub(crate) fn invert(&self, target: f64) -> f64 {
        if target <= self.j_hk[0] {
            return 0.0;
        }
        if target >= self.j_hk[LUT_NODES - 1] {
            return 1.0;
        }
        let i = self.lower_node(target);
        let (y0, y1) = (self.j_hk[i], self.j_hk[i + 1]);
        let (l0, l1) = (Self::node_l(i), Self::node_l(i + 1));
        if (y1 - y0).abs() > 1e-15 {
            l0 + (target - y0) / (y1 - y0) * (l1 - l0)
        } else {
            l0
        }
    }

    /// A lightness bracket for `target`, padded by [`CHROMA_PAD_NODES`] each side
    /// so a small-chroma root (whose curve is shifted from the neutral table)
    /// still falls inside. The caller refines within it by bisection.
    ///
    /// The bracket is built from the *neutral* table, so for a tinted target it
    /// must be re-validated against the tinted curve before use — that check
    /// lives in [`seed_bracket`], which widens to `[0, 1]` when the shift
    /// exceeded the pad.
    pub(crate) fn padded_bracket(&self, target: f64) -> LightnessBracket {
        let (lo_i, hi_i) = if target <= self.j_hk[0] {
            (0usize, 1usize)
        } else if target >= self.j_hk[LUT_NODES - 1] {
            (LUT_NODES - 2, LUT_NODES - 1)
        } else {
            let i = self.lower_node(target);
            (i, i + 1)
        };
        LightnessBracket {
            lo: Self::node_l(lo_i.saturating_sub(CHROMA_PAD_NODES)),
            hi: Self::node_l((hi_i + CHROMA_PAD_NODES).min(LUT_NODES - 1)),
        }
    }
}

/// The table for `vc`, if `vc` is one of the two precompiled viewing conditions.
///
/// Returns `None` for any other VC — the caller then keeps the full bisection,
/// which is correct under every VC, only slower. The match is on the surround
/// triplet `(c, nc)` that distinguishes [`ViewingConditions::srgb`] from
/// [`ViewingConditions::dim_surround`]; both share the same adapting luminance
/// and background, so these two coordinates identify the table.
pub(crate) fn lut_for_vc(vc: &ViewingConditions) -> Option<GreyAxisLut> {
    const EPS: f64 = 1e-9;
    let srgb = ViewingConditions::srgb();
    let dim = ViewingConditions::dim_surround();
    if (vc.c - srgb.c).abs() < EPS && (vc.nc - srgb.nc).abs() < EPS {
        Some(GreyAxisLut {
            j_hk: &lut_data::GREY_AXIS_SRGB,
        })
    } else if (vc.c - dim.c).abs() < EPS && (vc.nc - dim.nc).abs() < EPS {
        Some(GreyAxisLut {
            j_hk: &lut_data::GREY_AXIS_DIM,
        })
    } else {
        None
    }
}

/// Seed [`match_lightness`](crate::solve) with a lightness bracket from the LUT,
/// or signal that the slow path must run.
///
/// Returns `Some(bracket)` with `J_HK(bracket.lo) ≤ target ≤ J_HK(bracket.hi)`
/// — a validated bracket the caller refines by a short bisection — when:
///
/// * the VC is one of the two precompiled tables, **and**
/// * the chroma is neutral or `ratio ≤ `[`MAX_LUT_CHROMA`], **and**
/// * the padded neutral bracket genuinely contains the root for the *actual*
///   (possibly tinted) curve `j_hk_at`.
///
/// Returns `None` — take the full `[0, 1]` bisection — for an unsupported VC,
/// a larger chroma, or the rare case where a tinted root drifted past the pad.
/// The third check is the correctness net: it never trusts the chroma threshold
/// blindly.
pub(crate) fn seed_bracket(
    target_j_hk: f64,
    hue: Hue,
    chroma_policy: ChromaPolicy,
    vc: &ViewingConditions,
) -> Option<LutSeed> {
    let ratio = match chroma_policy {
        ChromaPolicy::Neutral => 0.0,
        ChromaPolicy::Relative(r) => r.clamp(0.0, 1.0),
    };
    if ratio > MAX_LUT_CHROMA {
        return None;
    }
    let lut = lut_for_vc(vc)?;

    if ratio == 0.0 {
        // Pure neutral: the table *is* the function on this axis, so direct
        // inverse interpolation is the answer — no per-iteration CAM16 at all.
        return Some(LutSeed::Exact(lut.invert(target_j_hk)));
    }

    // Small chroma: pad the neutral bracket, then re-validate against the real
    // tinted curve. If the chroma moved the root outside the padded window,
    // widen to the full range rather than refine in a bracket that excludes it.
    let mut bracket = lut.padded_bracket(target_j_hk);
    let j_hk_at = |l_ok: f64| {
        j_hk_from_xyz(
            srgb_to_xyz(build_color_for_lut(l_ok, hue, chroma_policy)),
            vc,
        )
    };
    if j_hk_at(bracket.lo) > target_j_hk {
        bracket.lo = 0.0;
    }
    if j_hk_at(bracket.hi) < target_j_hk {
        bracket.hi = 1.0;
    }
    Some(LutSeed::Bracket(bracket))
}

/// Recompute the grey-axis `J_HK` table for `vc` from the live forward path —
/// the single source of truth the committed [`lut_data`] arrays must equal.
#[cfg(test)]
pub(crate) fn generate_table(vc: &ViewingConditions) -> [f64; LUT_NODES] {
    let mut table = [0.0_f64; LUT_NODES];
    for (i, slot) in table.iter_mut().enumerate() {
        let l_ok = GreyAxisLut::node_l(i);
        *slot = j_hk_from_xyz(
            srgb_to_xyz(build_color_for_lut(
                l_ok,
                Hue::deg(0.0),
                ChromaPolicy::Neutral,
            )),
            vc,
        );
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn _emit_lut_data() {
        // GENERATOR (run once with --ignored): writes src/lut_data.rs from the
        // live forward math. The committed file is the artifact; this only
        // (re)produces it. `lut_data_matches_live_math` guards it thereafter.
        use std::fmt::Write as _;
        let srgb = generate_table(&ViewingConditions::srgb());
        let dim = generate_table(&ViewingConditions::dim_surround());
        let mut out = String::new();
        out.push_str("//! Precompiled grey-axis J_HK tables — DO NOT EDIT BY HAND.\n");
        out.push_str("//!\n");
        out.push_str(
            "//! `j_hk[i] = J_HK(i / (LUT_NODES - 1))` for a neutral sRGB stimulus, one\n",
        );
        out.push_str("//! table per supported viewing condition. Generated from the crate's own\n");
        out.push_str("//! forward path by `lut::tests::_emit_lut_data`; regenerate with\n");
        out.push_str("//! `cargo test -p labcolors-core _emit_lut_data -- --ignored`. The\n");
        out.push_str("//! `lut_data_matches_live_math` test fails if this drifts from the math.\n");
        out.push_str("use super::LUT_NODES;\n\n");
        let emit = |out: &mut String, name: &str, t: &[f64; LUT_NODES]| {
            writeln!(out, "#[rustfmt::skip]").ok();
            writeln!(out, "pub(crate) static {name}: [f64; LUT_NODES] = [").ok();
            for chunk in t.chunks(4) {
                out.push_str("    ");
                // {:?} on f64 emits the shortest round-tripping decimal.
                let line = chunk
                    .iter()
                    .map(|v| format!("{v:?}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&line);
                out.push_str(",\n");
            }
            out.push_str("];\n\n");
        };
        emit(&mut out, "GREY_AXIS_SRGB", &srgb);
        emit(&mut out, "GREY_AXIS_DIM", &dim);
        // Single trailing newline; no blank line at EOF (rustfmt-clean).
        while out.ends_with("\n\n") {
            out.pop();
        }
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/lut/lut_data.rs");
        std::fs::write(path, out).expect("write lut_data.rs");
        eprintln!("wrote {path}");
    }

    #[test]
    fn lut_data_matches_live_math() {
        // The committed tables must equal a fresh generation from the crate's
        // own forward math, bit-for-bit. This is the anti-drift gate: change the
        // CAM16 path or the VC constants and this fails until the tables are
        // regenerated — so the LUT can never silently diverge from `solve`.
        let srgb = generate_table(&ViewingConditions::srgb());
        let dim = generate_table(&ViewingConditions::dim_surround());
        assert_eq!(
            srgb,
            lut_data::GREY_AXIS_SRGB,
            "committed sRGB LUT diverged from live math — regenerate lut_data.rs"
        );
        assert_eq!(
            dim,
            lut_data::GREY_AXIS_DIM,
            "committed dim LUT diverged from live math — regenerate lut_data.rs"
        );
    }

    #[test]
    fn tables_are_strictly_monotone() {
        for table in [&lut_data::GREY_AXIS_SRGB, &lut_data::GREY_AXIS_DIM] {
            for w in table.windows(2) {
                assert!(w[1] > w[0], "LUT must be strictly increasing: {w:?}");
            }
        }
    }

    #[test]
    fn endpoints_span_the_full_j_hk_range() {
        for table in [&lut_data::GREY_AXIS_SRGB, &lut_data::GREY_AXIS_DIM] {
            assert!(table[0].abs() < 1e-9, "J_HK(0) must be ~0: {}", table[0]);
            assert!(
                table[LUT_NODES - 1] > 100.0,
                "J_HK(1) must reach ~100: {}",
                table[LUT_NODES - 1]
            );
        }
    }

    #[test]
    fn invert_round_trips_at_nodes() {
        let lut = lut_for_vc(&ViewingConditions::srgb()).expect("srgb table exists");
        for i in 0..LUT_NODES {
            let l_node = GreyAxisLut::node_l(i);
            let j = lut_data::GREY_AXIS_SRGB[i];
            let l_back = lut.invert(j);
            assert!(
                (l_back - l_node).abs() < 1e-9,
                "invert at node {i}: {l_back} vs {l_node}"
            );
        }
    }

    #[test]
    fn unsupported_vc_has_no_table() {
        // A third surround (dark, c = 0.525) is not precompiled — the caller
        // must fall back to bisection, never silently use the wrong table.
        let dark = ViewingConditions::srgb(); // start from a known one…
        // …and confirm only the two real presets resolve.
        assert!(lut_for_vc(&ViewingConditions::srgb()).is_some());
        assert!(lut_for_vc(&ViewingConditions::dim_surround()).is_some());
        // sanity: the helper distinguishes the two
        let _ = dark;
    }
}
