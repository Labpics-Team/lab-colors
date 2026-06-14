//! Neutral-background O(1) fast path for [`resolve_set`](crate::resolve_set).
//!
//! For a **solid grey** background the whole role set is a deterministic function
//! of the 8-bit grey level alone (given the default [`RoleTable`] and a supported
//! viewing condition): the perceptual luminance `Y_hk` and the WCAG relative
//! luminance both collapse onto that one level, so there is no chromatic degree
//! of freedom for the legal floor to depend on. That makes the grey axis a clean
//! 1-D function with a finite, exact domain of 256 codes — the simplest slice of
//! the breakpoint-curve idea (precompute the answer, look it up).
//!
//! This module memoises the resolved set for all 256 grey codes per supported VC,
//! built lazily on first use from the *live* solver. A subsequent grey resolve is
//! then an array index — no CIECAM16 forwards, no bisection — instead of the
//! ~1 ms, ~1000-forward `resolve_set`. It is a **transparent** fast path:
//! [`resolve_set`](crate::resolve_set) consults it and falls back to the live
//! solver for any non-grey background, an unsupported VC, or a custom table, so
//! the public contract is unchanged.
//!
//! ## Why it stays bit-identical
//!
//! The table is filled by the live solver itself, so a lookup returns the exact
//! `(Role, Resolved)` sequence the live `resolve_set` would have produced —
//! including the hierarchy-compression flags. The grey level is recovered by an
//! *exact* match ([`grey_code`](crate::spaces::srgb::grey_code)); an off-grid
//! grey (e.g. a blurred average) fails the match and takes the live path, so the
//! fast path is never an approximation. Gated by
//! `greyfast_matches_live_solver_on_every_grey`.
//!
//! ## Cost and scope
//!
//! The first grey resolve under a VC builds the 256-entry table (256 live
//! resolves, a one-time cost a consumer can warm at idle); every later grey
//! resolve under that VC is O(1). Memory is bounded (256 sets × 2 VCs, runtime
//! heap, never the WASM bundle — nothing is `const`). Chromatic backgrounds (the
//! `Y_hk`/`Y_wcag` 2-D case) and the `next_breakpoint` animation API are later
//! steps of the breakpoint-curve chapter; this is the neutral 1-D foundation.

use std::rc::Rc;

use crate::semantic::{Resolved, Role, RoleTable, resolve_set_live};
use crate::solve::BgInput;
use crate::spaces::srgb::grey_code;
use crate::spaces::vc::ViewingConditions;

/// One fully-resolved role set — the value `resolve_set` returns.
type GreySet = Vec<(Role, Resolved)>;

thread_local! {
    /// Lazily-built grey tables, indexed by [`vc_index`] (`0` = sRGB, `1` = dim).
    /// `None` until the first grey resolve under that VC fills it. `Rc` so a
    /// lookup hands back a cheap clone of the shared table, not a re-build.
    static GREY_SETS: std::cell::RefCell<[Option<Rc<Vec<GreySet>>>; 2]> =
        const { std::cell::RefCell::new([None, None]) };
}

/// Map a viewing condition to its grey-table slot, or `None` for an unsupported
/// VC (which then takes the live solver). Matches on the FULL VC
/// [`fingerprint`](ViewingConditions::fingerprint), not just the surround pair
/// `(c, nc)`: a caller-built VC that aliases `(c, nc)` but differs in adaptation
/// must fall through to the live solver, never be served the wrong precompiled
/// set.
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

/// The 8-bit grey code of `bg`, or `None` if it is not an on-grid solid grey.
fn neutral_code(bg: &BgInput) -> Option<u8> {
    // Exhaustive in-crate: a future interval/composite `BgInput` variant will
    // fail to compile here, forcing a deliberate "not on the neutral fast path"
    // decision rather than silently aliasing onto a grey code.
    match bg {
        BgInput::Solid(rgb) => {
            if rgb[0] == rgb[1] && rgb[1] == rgb[2] {
                grey_code(rgb[0])
            } else {
                None
            }
        }
    }
}

/// Build the 256-entry grey table for `vc` from the live solver.
fn build_table(vc: &ViewingConditions) -> Vec<GreySet> {
    let table = RoleTable::default();
    (0u32..=255)
        .map(|code| {
            let hex = format!("#{code:02X}{code:02X}{code:02X}");
            let bg = BgInput::solid(&hex).expect("a grey hex is always valid");
            resolve_set_live(&bg, &table, vc)
        })
        .collect()
}

/// The resolved set for `bg` from the neutral fast path, or `None` to fall back
/// to the live solver.
///
/// Returns `Some` only when every precondition for an *exact* lookup holds: the
/// default role table, a supported VC, and an on-grid solid grey background. The
/// table for that VC is built on first use and reused thereafter.
pub(crate) fn try_resolve_set(
    bg: &BgInput,
    table: &RoleTable,
    vc: &ViewingConditions,
) -> Option<GreySet> {
    // Only the default table is precomputed; a custom table takes the live path.
    if table != &RoleTable::default() {
        return None;
    }
    let idx = vc_index(vc)?;
    let code = neutral_code(bg)? as usize;

    // Reuse the built table if present; otherwise build it outside the borrow
    // (the build runs the live solver, which must not see GREY_SETS borrowed).
    let existing = GREY_SETS.with(|c| c.borrow()[idx].clone());
    let built = match existing {
        Some(t) => t,
        None => {
            let t = Rc::new(build_table(vc));
            GREY_SETS.with(|c| c.borrow_mut()[idx] = Some(t.clone()));
            t
        }
    };
    Some(built[code].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vcs() -> [(ViewingConditions, &'static str); 2] {
        [
            (ViewingConditions::srgb(), "srgb"),
            (ViewingConditions::dim_surround(), "dim"),
        ]
    }

    #[test]
    fn greyfast_matches_live_solver_on_every_grey() {
        // BIT-IDENTITY GATE: the fast path must reproduce the live solver's exact
        // (Role, Resolved) sequence — hex, contrasts, and compression flags — for
        // all 256 grey codes under both VCs. `Resolved`'s `PartialEq` compares the
        // whole set, so any drift (wrong code, wrong table slot, stale entry) fails
        // here. Independent of the build: it re-runs the live solver fresh.
        let table = RoleTable::default();
        for (vc, name) in vcs() {
            for code in 0u32..=255 {
                let hex = format!("#{code:02X}{code:02X}{code:02X}");
                let bg = BgInput::solid(&hex).unwrap();
                let fast = try_resolve_set(&bg, &table, &vc)
                    .expect("an on-grid grey under a supported VC is on the fast path");
                let live = resolve_set_live(&bg, &table, &vc);
                assert_eq!(
                    fast, live,
                    "{name}/{hex}: fast path diverged from live solver"
                );
            }
        }
    }

    #[test]
    fn fast_path_declines_outside_its_exact_domain() {
        // The fast path must say `None` (fall back) whenever an exact lookup is not
        // provable: a chromatic background or a custom table. (Both public VCs are
        // supported, so the unsupported-VC arm has no constructor to exercise it.)
        let table = RoleTable::default();
        let srgb = ViewingConditions::srgb();

        // Chromatic background — not grey.
        let chromatic = BgInput::solid("#007AFF").unwrap();
        assert!(try_resolve_set(&chromatic, &table, &srgb).is_none());

        // Custom role table differs from the default, so it is not precomputed.
        let custom = RoleTable::default().with_chroma(crate::RoleChroma::Neutral);
        let grey = BgInput::solid("#808080").unwrap();
        assert!(try_resolve_set(&grey, &custom, &srgb).is_none());

        // A caller-built VC that ALIASES sRGB's surround pair (c, nc) but differs
        // in adaptation (aw): the old (c, nc)-only match would have served it
        // sRGB's precompiled grey set (a silent wrong-colour memo collision). The
        // full-fingerprint match must decline, so it takes the live solver. This
        // is the unsupported-VC arm the comment above used to call unexercisable.
        let mut aliasing = ViewingConditions::srgb();
        aliasing.aw += 1.0;
        assert_eq!(aliasing.c, srgb.c, "alias keeps c");
        assert_eq!(aliasing.nc, srgb.nc, "alias keeps nc");
        assert!(
            try_resolve_set(&grey, &table, &aliasing).is_none(),
            "a VC aliasing (c, nc) but differing in adaptation must decline the fast path"
        );
    }
}
