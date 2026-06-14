//! Chromatic-background memo for [`resolve_set`](crate::resolve_set) — the
//! sibling of [`greyfast`](crate::greyfast) for backgrounds that are *not* solid
//! grey.
//!
//! ## Why a memo, not a precomputed table
//!
//! For a solid **grey** background the resolved set is a deterministic function
//! of one 8-bit level, so [`greyfast`](crate::greyfast) precomputes all 256
//! answers and serves any grey in O(1). A **chromatic** background has no such
//! finite axis: the resolved set depends on the background through three derived
//! quantities — `Y_hk` (H-K perceptual luminance), `Y_wcag` (WCAG relative
//! luminance, which fixes polarity and the legal floor) and, since the HIG role
//! taxonomy landed (#59), `J'_bg` (the CAM16-UCS lightness the `dJ'` fill/border
//! roles step away from). For a chromatic colour `Y_hk` and `J'_bg` decouple
//! (`Y_hk = J + f(h)·C^0.587` folds in the hue/chroma the grey axis lacks), so
//! the domain is a 2-D-plus continuum, not 256 codes. Precomputing it
//! exhaustively is infeasible and a coarse grid cannot be proven bit-identical
//! against the 8-bit quantisation cliff (that is the breakpoint-surface chapter,
//! deferred — see the scope note below).
//!
//! So this module **memoises** instead: it caches the resolved set keyed on the
//! background's exact 8-bit display value (the precise thing the output is a
//! function of). The common product pattern — one themed surface colour resolved
//! for many elements, or a surface re-resolved as an *unrelated* setting is
//! tweaked — then costs an O(1) lookup instead of the ~1 ms / ~hundreds-of-CAM16-
//! forwards live solve (`resolve_set_live`).
//!
//! ## Why it stays bit-identical
//!
//! A miss runs the live solver and stores its exact `Vec<(Role, Resolved)>`; a hit
//! returns a clone of that. The key is the exact quantised display triple the
//! whole solver reads the background through, so two inputs that share a key
//! genuinely produce the same output. There is no interpolation and no
//! approximation — the memo can only ever return a value the live solver itself
//! produced. Gated by [`tests::chromafast_matches_live_solver_on_a_chromatic_sweep`].
//!
//! ## Scope (honest limits)
//!
//! The **cold** first resolve of a *new* background still pays the full live
//! cost; only repeats are O(1). Crushing the cold path for a continuously
//! changing (animated/blurred) chromatic background needs a proven `Y_hk`
//! breakpoint surface with the `dJ'` roles recomputed from the free `J'_bg` and
//! the WCAG floor replayed analytically — the next chapter, not this one. Memory
//! is bounded ([`CHROMA_MEMO_CAP`], runtime heap, never the WASM bundle) and
//! cleared wholesale at the cap (a cold rebuild, never incorrectness).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::semantic::{Resolved, Role, RoleTable, resolve_set_live};
use crate::solve::BgInput;
use crate::spaces::srgb::srgb_gamma;
use crate::spaces::vc::ViewingConditions;

/// One fully-resolved role set — the value [`resolve_set`](crate::resolve_set)
/// returns.
type ChromaSet = Vec<(Role, Resolved)>;

/// Memo key: the background's 8-bit display triple plus its [`vc_index`] slot.
type MemoKey = ([u8; 3], usize);

/// The per-thread memo map (aliased to keep the `thread_local` type legible).
type Memo = HashMap<MemoKey, Rc<ChromaSet>>;

/// Upper bound on live memo entries before a wholesale clear. One entry is a full
/// role set (~hundreds of bytes); a few thousand distinct chromatic surfaces is a
/// generous working set for any one theme session, and the cap turns an otherwise
/// unbounded thread-local into a fixed footprint (bounded memory is a correctness
/// property here, mirroring `semantic::CURVE_PLAN_CACHE_CAP`).
const CHROMA_MEMO_CAP: usize = 4096;

thread_local! {
    /// Resolved sets keyed on `(display triple, vc slot)`. `Rc` so a hit hands
    /// back a cheap clone of the shared set, not a re-solve.
    static CHROMA_MEMO: RefCell<Memo> = RefCell::new(HashMap::new());
}

/// Map a viewing condition to its memo slot, or `None` for an unsupported VC
/// (which then takes the live solver). Matches [`greyfast`](crate::greyfast)'s
/// full-[`fingerprint`](ViewingConditions::fingerprint) convention so the two
/// fast paths agree on what is supported and neither aliases a caller-built VC
/// that merely shares the surround pair `(c, nc)`.
fn vc_index(vc: &ViewingConditions) -> Option<usize> {
    let fp = vc.fingerprint();
    if fp == ViewingConditions::srgb().fingerprint() {
        Some(0)
    } else if fp == ViewingConditions::dim_surround().fingerprint() {
        Some(1)
    } else {
        None
    }
}

/// The background's exact 8-bit display triple — the key the resolved set is a
/// function of. This is the same quantisation the solver applies internally
/// (`solve::governing_display` / `semantic::bg_display`): gamma-encode the linear
/// background, clamp, and round to the display grid.
fn display_key(bg: &BgInput) -> [u8; 3] {
    // Exhaustive in-crate (like `greyfast::neutral_code`): a future interval /
    // composite `BgInput` variant fails to compile here, forcing a deliberate
    // "not on the chromatic memo" decision rather than silently aliasing.
    match bg {
        BgInput::Solid(rgb) => {
            let q = |c: f64| (srgb_gamma(c).clamp(0.0, 1.0) * 255.0).round() as u8;
            [q(rgb[0]), q(rgb[1]), q(rgb[2])]
        }
    }
}

/// The resolved set for `bg` from the chromatic memo, or `None` to fall back to
/// the live solver.
///
/// Returns `Some` for any solid background under a supported VC with the default
/// role table: a hit serves the cached set, a miss runs the live solver once and
/// caches it (so the *next* resolve of the same surface is O(1)). It declines
/// (`None`) for a custom table or an unsupported VC, exactly like
/// [`greyfast`](crate::greyfast). Solid greys never reach here — `resolve_set`
/// consults greyfast first, which owns every on-grid grey.
pub(crate) fn try_resolve_set(
    bg: &BgInput,
    table: &RoleTable,
    vc: &ViewingConditions,
) -> Option<ChromaSet> {
    // Only the default table is memoised; a custom table takes the live path.
    if table != &RoleTable::default() {
        return None;
    }
    let idx = vc_index(vc)?;
    let key = (display_key(bg), idx);

    if let Some(hit) = CHROMA_MEMO.with(|c| c.borrow().get(&key).cloned()) {
        return Some((*hit).clone());
    }

    // Miss: run the live solver (the oracle) and cache its exact output. The live
    // solve must not see CHROMA_MEMO borrowed, so it runs outside the borrow.
    let set = Rc::new(resolve_set_live(bg, table, vc));
    CHROMA_MEMO.with(|c| {
        let mut m = c.borrow_mut();
        if m.len() >= CHROMA_MEMO_CAP {
            m.clear(); // bounded footprint: wholesale cold rebuild, never wrong
        }
        m.insert(key, set.clone());
    });
    Some((*set).clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Clear the memo so a test starts from a known cold state regardless of
    /// order.
    fn reset() {
        CHROMA_MEMO.with(|c| c.borrow_mut().clear());
    }

    fn vcs() -> [(ViewingConditions, &'static str); 2] {
        [
            (ViewingConditions::srgb(), "srgb"),
            (ViewingConditions::dim_surround(), "dim"),
        ]
    }

    /// A spread of chromatic backgrounds across the hue wheel and saturation,
    /// plus a couple of near-neutral low-chroma tints (which are still off the
    /// grey axis, so greyfast declines them and they belong here).
    const CHROMATIC: [&str; 12] = [
        "#2E6FB7", "#B5482E", "#3A8F5C", "#A23E8C", "#6E6E7A", "#0A7E8C", "#F2C14E", "#7B2D8E",
        "#C0FFEE", "#102A44", "#FF6F61", "#1B1F3B",
    ];

    #[test]
    fn chromafast_matches_live_solver_on_a_chromatic_sweep() {
        // BIT-IDENTITY GATE: for every chromatic background under both supported
        // VCs the memo must reproduce the live solver's exact set — on the cold
        // MISS (first call), against an independent fresh live solve, AND on the
        // warm HIT (second call). `Resolved`'s `PartialEq` compares the whole set
        // (hex, contrasts, compression flags), so any drift fails here.
        let table = RoleTable::default();
        for (vc, name) in vcs() {
            for hex in CHROMATIC {
                reset();
                let bg = BgInput::solid(hex).unwrap();
                let cold = try_resolve_set(&bg, &table, &vc)
                    .expect("a solid bg under a supported VC is on the memo path");
                let live = resolve_set_live(&bg, &table, &vc);
                assert_eq!(
                    cold, live,
                    "{name}/{hex}: cold memo diverged from live solver"
                );
                let warm =
                    try_resolve_set(&bg, &table, &vc).expect("the second resolve must be a hit");
                assert_eq!(
                    warm, live,
                    "{name}/{hex}: warm memo hit diverged from live solver"
                );
            }
        }
    }

    #[test]
    fn declines_outside_its_exact_domain() {
        // A custom role table is not memoised; an unsupported VC falls back. (Both
        // public VCs are supported, so the unsupported-VC arm has no constructor
        // to exercise it — same as greyfast.)
        let srgb = ViewingConditions::srgb();
        let bg = BgInput::solid("#2E6FB7").unwrap();

        let custom = RoleTable::default().with_chroma(crate::RoleChroma::Neutral);
        assert!(try_resolve_set(&bg, &custom, &srgb).is_none());

        // A VC that aliases sRGB's surround pair (c, nc) but differs in adaptation
        // must decline (full-fingerprint match), not be served sRGB's memo slot.
        let table = RoleTable::default();
        let mut aliasing = ViewingConditions::srgb();
        aliasing.aw += 1.0;
        assert!(
            try_resolve_set(&bg, &table, &aliasing).is_none(),
            "a VC aliasing (c, nc) but differing in adaptation must decline the chroma fast path"
        );
    }

    #[test]
    fn cap_clears_without_corrupting_results() {
        // Filling past the cap triggers a wholesale clear; the very next resolve
        // must still be bit-identical to the live solver (a cold rebuild, never a
        // wrong answer).
        reset();
        let table = RoleTable::default();
        let srgb = ViewingConditions::srgb();
        // Push more than the cap of distinct chromatic keys through the memo.
        for i in 0..(CHROMA_MEMO_CAP as u32 + 16) {
            let r = 0x20 + (i & 0x7F) as u8;
            let g = 0x40 + ((i >> 1) & 0x7F) as u8;
            let b = 0x80 + ((i >> 2) & 0x3F) as u8;
            let hex = format!("#{r:02X}{g:02X}{b:02X}");
            let bg = BgInput::solid(&hex).unwrap();
            let got = try_resolve_set(&bg, &table, &srgb).unwrap();
            assert_eq!(
                got,
                resolve_set_live(&bg, &table, &srgb),
                "{hex}: drift under cap churn"
            );
        }
        assert!(
            CHROMA_MEMO.with(|c| c.borrow().len()) <= CHROMA_MEMO_CAP,
            "memo must stay bounded by the cap"
        );
    }
}
