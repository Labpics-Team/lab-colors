//! Contract 2 — resolve_set continuity ("плавность").
//!
//! BUG CLASS this guards: *silent quantization cliffs and undocumented polarity
//! teleports.* The crate's history carries three post-factum bugs of exactly
//! this family — the #777-#999 false-unreachable stripe, the quantization cliff
//! (#44), and the Bracket-path LUT seam (#50/#53). All three share a signature:
//! a role's resolved colour jumps discontinuously as the background moves by a
//! single 8-bit step, while every point-wise test still passes. This file walks
//! the background one hex quantum at a time and asserts that, *outside the one
//! documented polarity-break step*, no role's signed Lc teleports.
//!
//! Empirically discovered constants (probed through the public API, then frozen):
//!
//! * **Continuity bound.** Off the polarity break, the largest per-quantum
//!   |ΔLc| any role exhibits on the grey axis is ~1.05 Lc (the WCAG-AA floor
//!   lifting mid-grey text roles near the flip). Frozen here at **2.0 Lc**, ~2×
//!   the measured worst case — a real cliff (the historical bugs jumped tens of
//!   Lc) breaks it, ordinary floor nudges do not. A *tightening* of this bound
//!   over time would be a welcome signal; a *breach* is a regression.
//!
//! * **Polarity-break zone.** TextPrimary's contrast sign flips across exactly
//!   **#757575 → #767676** — one single 8-bit step — and the step is identical
//!   under both viewing conditions (the semantic module's VC-independence
//!   promise). The test pins both the location and the width: if the zone widens,
//!   shifts, or becomes VC-dependent, that is the signal to investigate.

use labcolors_core::{BgInput, Resolved, Role, RoleTable, ViewingConditions, resolve_set};

/// Off-break per-quantum continuity bound, ~2× the measured worst case (~1.05).
const CONTINUITY_BOUND: f64 = 2.0;

/// The roles whose continuity is asserted — every text/UI role that resolves to
/// a colour on a neutral background.
const TRACKED: [Role; 5] = [
    Role::TextPrimary,
    Role::TextSecondary,
    Role::TextMuted,
    Role::TextDisabled,
    Role::Icon,
];

fn gray(g: u8) -> String {
    format!("#{g:02X}{g:02X}{g:02X}")
}

fn vcs() -> [(ViewingConditions, &'static str); 2] {
    [
        (ViewingConditions::srgb(), "srgb"),
        (ViewingConditions::dim_surround(), "dim"),
    ]
}

/// Signed Lc of `role` resolved against `bg`, or `None` if it did not resolve to
/// a colour (unreachable / zero token).
fn role_lc(bg: &BgInput, table: &RoleTable, vc: &ViewingConditions, role: Role) -> Option<f64> {
    resolve_set(bg, table, vc)
        .into_iter()
        .find_map(|(r, res)| match res {
            Resolved::Color { solved, .. } if r == role => Some(solved.lc()),
            _ => None,
        })
}

#[test]
fn grey_axis_is_continuous_outside_the_single_polarity_break() {
    // Walk the grey axis #404040..#C0C0C0 one 8-bit quantum at a time. For each
    // adjacent pair, every tracked role that resolves on both backgrounds must
    // move by no more than CONTINUITY_BOUND in signed Lc — UNLESS this is the
    // one documented polarity-break step, where TextPrimary changes sign.
    for (vc, vc_name) in vcs() {
        let table = RoleTable::default();
        for g in 0x40u8..0xC0 {
            let bg0 = BgInput::solid(&gray(g)).unwrap();
            let bg1 = BgInput::solid(&gray(g + 1)).unwrap();

            // Is this the polarity-break step? Detected from TextPrimary's sign
            // change, the canonical signal of the break (it is allowed to teleport
            // here; the dedicated test below pins its exact location).
            let p0 = role_lc(&bg0, &table, &vc, Role::TextPrimary);
            let p1 = role_lc(&bg1, &table, &vc, Role::TextPrimary);
            let is_break = matches!((p0, p1), (Some(a), Some(b)) if a.signum() != b.signum());
            if is_break {
                continue; // the documented discontinuity — pinned separately
            }

            for role in TRACKED {
                if let (Some(a), Some(b)) = (
                    role_lc(&bg0, &table, &vc, role),
                    role_lc(&bg1, &table, &vc, role),
                ) {
                    let delta = (a - b).abs();
                    assert!(
                        delta <= CONTINUITY_BOUND,
                        "{vc_name} {} {}->{}: ΔLc {delta:.4} > {CONTINUITY_BOUND} — a quantization \
                         cliff (regression of the #44 / #50 class), {a:.3} -> {b:.3}",
                        role.key(),
                        gray(g),
                        gray(g + 1),
                    );
                }
            }
        }
    }
}

#[test]
fn polarity_break_zone_is_one_step_wide_and_vc_independent() {
    // The single legitimate discontinuity on the grey axis: TextPrimary's
    // contrast sign flips across exactly #757575 -> #767676, and at no other
    // step. Pinning the EXACT boundary turns any future widening or shifting of
    // the flip zone into a hard test failure (the #777-#999 stripe was precisely
    // a misplaced flip). The step must also be identical under both viewing
    // conditions — the semantic module guarantees polarity is read from the
    // background bytes alone, never the VC.
    for (vc, vc_name) in vcs() {
        let table = RoleTable::default();
        let mut flip_steps: Vec<(u8, u8)> = Vec::new();
        for g in 0x40u8..0xC0 {
            let bg0 = BgInput::solid(&gray(g)).unwrap();
            let bg1 = BgInput::solid(&gray(g + 1)).unwrap();
            if let (Some(a), Some(b)) = (
                role_lc(&bg0, &table, &vc, Role::TextPrimary),
                role_lc(&bg1, &table, &vc, Role::TextPrimary),
            ) && a.signum() != b.signum()
            {
                flip_steps.push((g, g + 1));
            }
        }
        assert_eq!(
            flip_steps.len(),
            1,
            "{vc_name}: expected exactly one polarity-break step, found {flip_steps:?} \
             (a second flip = the false-unreachable-stripe regression)"
        );
        assert_eq!(
            flip_steps[0],
            (0x75, 0x76),
            "{vc_name}: polarity break moved off #757575->#767676 to \
             #{:02X}->#{:02X} — the WCAG flip shifted",
            flip_steps[0].0,
            flip_steps[0].1,
        );
    }
}

#[test]
fn chromatic_axis_is_continuous_near_brand_blue() {
    // The same continuity contract on a chromatic axis: sweep the red channel
    // through the #3478F6 (brand-blue / info) neighbourhood, holding G=0x78,
    // B=0xF6. A chromatic background never hits the grey-axis polarity flip in
    // this range, so the bound holds with no exclusion. Measured worst case here
    // is ~0.31 Lc; the 2.0 bound leaves wide headroom and still catches a cliff.
    for (vc, vc_name) in vcs() {
        let table = RoleTable::default();
        for r in 0x20u8..0x50 {
            let h0 = format!("#{r:02X}78F6");
            let h1 = format!("#{:02X}78F6", r + 1);
            let bg0 = BgInput::solid(&h0).unwrap();
            let bg1 = BgInput::solid(&h1).unwrap();
            for role in TRACKED {
                if let (Some(a), Some(b)) = (
                    role_lc(&bg0, &table, &vc, role),
                    role_lc(&bg1, &table, &vc, role),
                ) {
                    // No sign flip is expected on this chromatic sweep; if one
                    // appears, that itself is a discontinuity worth failing on.
                    let delta = (a - b).abs();
                    assert!(
                        a.signum() == b.signum() && delta <= CONTINUITY_BOUND,
                        "{vc_name} {} {h0}->{h1}: chromatic ΔLc {delta:.4} (sign {} -> {}) \
                         exceeds {CONTINUITY_BOUND} — a cliff near brand blue",
                        role.key(),
                        a.signum(),
                        b.signum(),
                    );
                }
            }
        }
    }
}
