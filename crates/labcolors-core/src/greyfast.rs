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
//! This module reconstructs the resolved set for a grey code from a **build-time
//! constant table** ([`grey_data`]) — the emitted colour of every role, for all
//! 256 grey codes per supported VC, committed as bytes. A grey resolve replays
//! the solver's `finish` measurement on those stored colours (a handful of CAM16
//! forwards, no bisection, no curve scan) instead of the ~1 ms, ~1000-forward
//! live `resolve_set`, and memoises the result per code. It is a **transparent**
//! fast path: [`resolve_set`](crate::resolve_set) consults it and falls back to
//! the live solver for any non-grey background, an unsupported VC, or a custom
//! table, so the public contract is unchanged.
//!
//! ## Why it stays bit-identical
//!
//! The table stores each role's *emitted* on-grid colour — the irreducible output
//! of the solver's search. Every other field of a resolved colour (the decoded
//! appearance, the perceptual `Lc`, the WCAG ratio) is a deterministic
//! *measurement* on that on-grid colour, so replaying the crate's own
//! [`finish`](crate::solve::reconstruct_solved) reproduces the exact
//! `(Role, Resolved)` sequence — including the hierarchy-compression flag — that
//! the live `resolve_set` produced. No measurement logic is duplicated. The grey
//! level is recovered by an *exact* match
//! ([`grey_code`](crate::spaces::srgb::grey_code)); an off-grid grey fails the
//! match and takes the live path, so the fast path is never an approximation.
//! Gated bit-for-bit by [`tests::greyfast_const_is_bit_identical_to_live`].
//!
//! ## Cost and scope — the deliberate size/latency trade
//!
//! There is **no 256-resolve build**: the first grey resolve reconstructs only
//! the requested code (tens of microseconds — a few forwards per role) and
//! memoises it, so it never pays the ~555 ms table build the old lazy design
//! folded into the first grey call. Every later resolve of that code is an O(1)
//! clone.
//!
//! Unlike the grey-axis LUT (which is packed mathematics, ~4 KB), this table is a
//! **precomputed palette**, and it is committed **into the WASM binary**: `256
//! codes × 20 roles × 4 bytes × 2 VCs ≈ 41 KB` raw (`+~18 KB` gzipped, ~13 % of
//! the bundle). That is a *chosen* trade — carry ~41 KB of static data to erase a
//! ~555 ms first-paint freeze on a grey surface — made under the owner's explicit
//! "speed is critical, colours must compute instantly" priority. It stays cheap
//! because only the emitted-colour bytes are stored, never the measured floats
//! (those are re-derived by the `finish` replay), so `labcolors-core` keeps its
//! empty `[dependencies]` (issue #29).
//!
//! **Domain invariant (loud, not silent).** A [`GreyEntry`] is four bytes and
//! encodes exactly two outcomes: a solved `Color` or the zero token (`None`). The
//! neutral default domain provably produces nothing else — no `Unreachable`, no
//! `Translucent` (the default table has no ladder/alpha roles, and every text
//! role is reachable on every grey). If a future policy change ever makes the
//! default table yield an `Unreachable` or `Translucent` role on a grey, the
//! generator [`tests::_emit_grey_data`] **panics** rather than emit an
//! unrepresentable record, and [`tests::greyfast_const_is_bit_identical_to_live`]
//! goes red — the assumption fails at the gate, never as a wrong colour in
//! production.
//!
//! **Memo is thread-local.** The per-code cache is a `thread_local!` of 512 slots
//! (256 codes × 2 VCs). In WASM — single-threaded — this is exactly one shared
//! cache, ideal. On a multi-threaded native host each worker warms its own copy
//! (bounded, correct, a little redundant): a deliberate choice, since the crate's
//! primary target is the browser and the const table makes a cold reconstruction
//! cheap anyway.
//!
//! Chromatic backgrounds and the `next_breakpoint` animation API are later steps
//! of the breakpoint-curve chapter; this is the neutral 1-D foundation.

use crate::semantic::{Resolved, Role, RoleSpec, RoleTable};
use crate::solve::BgInput;
use crate::spaces::srgb::grey_code;
use crate::spaces::vc::ViewingConditions;

mod grey_data;

/// One fully-resolved role set — the value `resolve_set` returns.
type GreySet = Vec<(Role, Resolved)>;

/// The number of roles in every resolved set — [`Role::ALL`] in visual-weight
/// order. The constant table is indexed `code * ROLES + role_index`.
const ROLES: usize = Role::ALL.len();

/// `flags` bit: the WCAG legal floor overrode the perceptual target on this role.
const FLAG_FLOOR_OVERRIDE: u8 = 0b001;
/// `flags` bit: this role's place in the text hierarchy was compressed onto its
/// senior by the legal floor ([`Resolved::compressed`]).
const FLAG_COMPRESSED: u8 = 0b010;
/// `flags` bit: this role resolved to the honest zero token ([`Resolved::None`]),
/// not a colour; `rgb` is unused.
const FLAG_NONE: u8 = 0b100;

/// One resolved role in the constant grey table: the emitted 8-bit colour plus
/// the two search flags that a `finish` replay cannot re-derive. Four bytes, no
/// padding. A [`FLAG_NONE`] entry is the zero token and ignores `rgb`.
#[derive(Clone, Copy)]
pub(crate) struct GreyEntry {
    /// The emitted `#RRGGBB` as three bytes — the solver's on-grid output.
    pub rgb: [u8; 3],
    /// `FLAG_*` bitset: floor-override, compression, and the zero-token marker.
    pub flags: u8,
}

thread_local! {
    /// Per-code reconstruction memo, indexed `[vc_index][grey_code]`. `None`
    /// until a code is first resolved, then holds that code's set so later
    /// resolves clone instead of re-running the `finish` replay. Bounded by
    /// construction (512 slots) and filled lazily — the first grey resolve costs
    /// one code's reconstruction, never the whole table.
    static GREY_CACHE: std::cell::RefCell<[[Option<GreySet>; 256]; 2]> =
        const { std::cell::RefCell::new([[const { None }; 256], [const { None }; 256]]) };
}

/// The committed constant tables, one per preset VC (`0` = sRGB, `1` = dim),
/// sharing the canonical slot assignment with the LUT and chroma fast paths.
const TABLES: [&[GreyEntry; 256 * ROLES]; 2] =
    [&grey_data::GREY_SETS_SRGB, &grey_data::GREY_SETS_DIM];

/// Map a viewing condition to its grey-table slot, or `None` for an unsupported
/// VC. Delegates to [`ViewingConditions::preset_index`], which is the canonical
/// slot assignment shared across the grey/chroma fast paths and the LUT.
fn vc_index(vc: &ViewingConditions) -> Option<usize> {
    vc.preset_index()
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

/// Reconstruct the resolved set for `code` under `vc` from the constant table by
/// replaying the solver's `finish` on each stored colour.
///
/// `bg` is the grey background itself (the code's colour); it drives the
/// once-per-role `finish` replay (its luminance and display colour). Returns
/// `None` — so the caller falls back to the live solver — only on the impossible
/// case of a stored colour failing to reconstruct, never for a domain miss (those
/// are declined earlier by [`try_resolve_set`]).
fn reconstruct_set(
    bg: &BgInput,
    idx: usize,
    code: usize,
    vc: &ViewingConditions,
) -> Option<GreySet> {
    let table = TABLES[idx];
    let base = code * ROLES;
    // The fast path is only entered for the default table (guarded in
    // `try_resolve_set`), so the role specs are the default ones — used here to
    // re-attach `achieved_dj`, which the live path measures for dJ' roles only.
    let specs = RoleTable::default();
    let mut out: GreySet = Vec::with_capacity(ROLES);
    for (i, &role) in Role::ALL.iter().enumerate() {
        let entry = table[base + i];
        let resolved = if entry.flags & FLAG_NONE != 0 {
            Resolved::None
        } else {
            let solved = crate::solve::reconstruct_solved(
                entry.rgb,
                bg,
                entry.flags & FLAG_FLOOR_OVERRIDE != 0,
                vc,
            )
            .ok()?;
            // `achieved_dj` is `Some` iff the live solver produced it — that is,
            // for decorative dJ' roles (`RoleSpec::DecorativeDj`), the only path
            // that records a measured `|dJ'|`. It is a pure re-measurement of the
            // emitted on-grid colour, so replaying it reproduces the live value
            // bit-for-bit; every other role carries `None`, matching the live set.
            let achieved_dj = matches!(specs.spec(role), RoleSpec::DecorativeDj { .. })
                .then(|| crate::solve::reconstruct_achieved_dj(entry.rgb, bg, vc));
            Resolved::Color {
                solved,
                compressed: entry.flags & FLAG_COMPRESSED != 0,
                achieved_dj,
            }
        };
        out.push((role, resolved));
    }
    Some(out)
}

/// The resolved set for `bg` from the neutral fast path, or `None` to fall back
/// to the live solver.
///
/// Returns `Some` only when every precondition for an *exact* lookup holds: the
/// default role table, a supported VC, and an on-grid solid grey background. The
/// code's set is reconstructed from the constant table on first use and memoised
/// thereafter.
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

    // Memoised? Hand back a clone. Otherwise reconstruct this one code (never the
    // whole table) outside the borrow, then record it.
    if let Some(cached) = GREY_CACHE.with(|c| c.borrow()[idx][code].clone()) {
        return Some(cached);
    }
    let set = reconstruct_set(bg, idx, code, vc)?;
    GREY_CACHE.with(|c| c.borrow_mut()[idx][code] = Some(set.clone()));
    Some(set)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lcs::LcsColor;
    use crate::semantic::{reset_live_solve_count, resolve_set_live};

    #[test]
    #[ignore]
    fn _emit_grey_data() {
        // GENERATOR (run once with --ignored): writes src/greyfast/grey_data.rs
        // from the live solver. The committed file is the artifact; this only
        // (re)produces it. `greyfast_const_is_bit_identical_to_live` guards it
        // thereafter — a policy change that moves the emitted colours fails that
        // gate until this is re-run.
        use std::fmt::Write as _;

        let table = RoleTable::default();
        let collect = |vc: &ViewingConditions| -> Vec<(u8, u8, u8, u8)> {
            let mut rows = Vec::with_capacity(256 * ROLES);
            for code in 0u32..=255 {
                let hex = format!("#{code:02X}{code:02X}{code:02X}");
                let bg = BgInput::solid(&hex).expect("a grey hex is always valid");
                for (role, res) in resolve_set_live(&bg, &table, vc) {
                    let row = match res {
                        Resolved::None => (0u8, 0u8, 0u8, FLAG_NONE),
                        // `achieved_dj` is not stored — it is a pure re-measurement
                        // of the emitted colour, re-derived on reconstruction (see
                        // `reconstruct_set`); only the search flags a `finish` replay
                        // cannot re-derive are committed.
                        Resolved::Color {
                            solved, compressed, ..
                        } => {
                            let h = solved.hex();
                            let byte = |a, b| u8::from_str_radix(&h[a..b], 16).unwrap();
                            let mut flags = 0u8;
                            if solved.floor_override() {
                                flags |= FLAG_FLOOR_OVERRIDE;
                            }
                            if compressed {
                                flags |= FLAG_COMPRESSED;
                            }
                            (byte(1, 3), byte(3, 5), byte(5, 7), flags)
                        }
                        other => panic!(
                            "grey code {code}, role {role:?} resolved to {other:?} — the \
                             constant grey table only represents Color and the zero token. \
                             An unreachable or translucent role in the neutral default \
                             domain means the record must be redesigned before regenerating."
                        ),
                    };
                    rows.push(row);
                }
            }
            rows
        };

        let mut out = String::new();
        out.push_str("//! Precompiled neutral (grey) resolved-set table — DO NOT EDIT BY HAND.\n");
        out.push_str("//!\n");
        out.push_str("//! One [`GreyEntry`] per `(grey code, role)` in [`Role::ALL`] order, for\n");
        out.push_str(
            "//! all 256 grey codes, one array per supported viewing condition. Each entry\n",
        );
        out.push_str(
            "//! is the role's emitted on-grid colour plus its search flags; the measured\n",
        );
        out.push_str(
            "//! appearance/contrasts are re-derived by replaying `finish`. Generated from\n",
        );
        out.push_str(
            "//! the crate's own solver by `greyfast::tests::_emit_grey_data`; regenerate\n",
        );
        out.push_str("//! with `cargo test -p labcolors-core _emit_grey_data -- --ignored`. The\n");
        out.push_str("//! `greyfast_const_is_bit_identical_to_live` test fails if this drifts.\n");
        out.push_str("use super::{GreyEntry, ROLES};\n\n");

        let emit = |out: &mut String, decl: &str, rows: &[(u8, u8, u8, u8)]| {
            writeln!(out, "#[rustfmt::skip]").ok();
            writeln!(
                out,
                "pub(crate) static {decl}: [GreyEntry; 256 * ROLES] = ["
            )
            .ok();
            for chunk in rows.chunks(4) {
                out.push_str("    ");
                for &(r, g, b, f) in chunk {
                    write!(out, "GreyEntry {{ rgb: [{r}, {g}, {b}], flags: {f} }}, ").ok();
                }
                out.push('\n');
            }
            out.push_str("];\n\n");
        };
        emit(
            &mut out,
            "GREY_SETS_SRGB",
            &collect(&ViewingConditions::srgb()),
        );
        emit(
            &mut out,
            "GREY_SETS_DIM",
            &collect(&ViewingConditions::dim_surround()),
        );
        while out.ends_with("\n\n") {
            out.pop();
        }
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/greyfast/grey_data.rs");
        std::fs::write(path, out).expect("write grey_data.rs");
        eprintln!("wrote {path}");
    }

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
        // provable: a chromatic background, a custom table, or a VC that is not a
        // precompiled preset (the aliasing case below).
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

    /// Bit-for-bit equality of two [`LcsColor`]s (every field via `to_bits`, so a
    /// one-ULP drift or a `-0.0`/`+0.0` swap fails — stricter than `PartialEq`).
    fn lcs_bits_eq(a: LcsColor, b: LcsColor) -> bool {
        a.jp.to_bits() == b.jp.to_bits()
            && a.h_ok.to_bits() == b.h_ok.to_bits()
            && a.s.to_bits() == b.s.to_bits()
            && a.h_cam().to_bits() == b.h_cam().to_bits()
    }

    /// Bit-for-bit equality of two resolved roles. On the neutral default domain
    /// every entry is `Color` or the zero token, so those two arms are exhaustive
    /// in practice; any other pairing is a divergence and fails.
    fn resolved_bits_eq(a: &Resolved, b: &Resolved) -> bool {
        match (a, b) {
            (Resolved::None, Resolved::None) => true,
            (
                Resolved::Color {
                    solved: sa,
                    compressed: ca,
                    achieved_dj: da,
                },
                Resolved::Color {
                    solved: sb,
                    compressed: cb,
                    achieved_dj: db,
                },
            ) => {
                ca == cb
                    && sa.hex() == sb.hex()
                    && sa.lc().to_bits() == sb.lc().to_bits()
                    && sa.wcag_ratio().to_bits() == sb.wcag_ratio().to_bits()
                    && sa.floor_override() == sb.floor_override()
                    && lcs_bits_eq(sa.color(), sb.color())
                    && da.map(f64::to_bits) == db.map(f64::to_bits)
            }
            _ => false,
        }
    }

    #[test]
    fn greyfast_const_is_bit_identical_to_live() {
        // CHARACTERIZATION LOCK: the constant table, reconstructed through the
        // `finish` replay, must reproduce the live solver's exact (Role, Resolved)
        // sequence for all 256 grey codes under both VCs — compared field-by-field
        // via `to_bits`, so even a one-ULP drift fails. This is also the anti-rot
        // gate: if a policy change moves the emitted colours, the committed table
        // is stale and this fails, demanding regeneration.
        let table = RoleTable::default();
        for (vc, name) in vcs() {
            for code in 0u32..=255 {
                let hex = format!("#{code:02X}{code:02X}{code:02X}");
                let bg = BgInput::solid(&hex).unwrap();
                let fast = try_resolve_set(&bg, &table, &vc)
                    .expect("an on-grid grey under a supported VC is on the fast path");
                let live = resolve_set_live(&bg, &table, &vc);
                assert_eq!(fast.len(), live.len(), "{name}/{hex}: role count diverged");
                for ((rf, resf), (rl, resl)) in fast.iter().zip(live.iter()) {
                    assert_eq!(rf, rl, "{name}/{hex}: role order diverged");
                    assert!(
                        resolved_bits_eq(resf, resl),
                        "{name}/{hex} role {rf:?}: constant grey table diverged from the live \
                         solver bit-for-bit — regenerate with \
                         `cargo test -p labcolors-core _emit_grey_data -- --ignored`"
                    );
                }
            }
        }
    }

    #[test]
    fn default_grey_path_never_calls_live_solver() {
        // The whole point of the constant table: the first resolve of a grey code
        // reconstructs it from committed bytes, never running the live solver — so
        // there is no lazy 256-resolve build to fold ~566 ms into the first grey
        // call. Clear the memo (force a real reconstruction, not a cache hit),
        // reset the live-solve counter, resolve, and assert the live solver was
        // never touched.
        let table = RoleTable::default();
        let vc = ViewingConditions::srgb();
        GREY_CACHE.with(|c| {
            for slot in c.borrow_mut()[0].iter_mut() {
                *slot = None;
            }
        });
        let _ = reset_live_solve_count();

        let bg = BgInput::solid("#7F7F7F").unwrap();
        let set = try_resolve_set(&bg, &table, &vc).expect("grey is on the fast path");
        assert_eq!(set.len(), ROLES);
        assert_eq!(
            reset_live_solve_count(),
            0,
            "the constant grey fast path must reconstruct without any live solve"
        );
    }
}
