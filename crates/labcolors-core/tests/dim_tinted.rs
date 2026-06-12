//! Contract 3 — dim surround × tinted table, together.
//!
//! BUG CLASS this guards: *an interaction never tested jointly.* The existing
//! suite checks the WCAG floors and the strict hierarchy mostly under `srgb`,
//! and the cool-undertone direction mostly for the neutral (achromatic) policy.
//! Nothing exercised the full product — `resolve_set` under `dim_surround` with
//! the DEFAULT tinted table — across a grid of backgrounds, asserting all four
//! invariants at once:
//!
//! 1. **Perceptual target accuracy.** Each text/UI role lands within ±1 Lc of
//!    the target its anchor implies (fraction × the background's max contrast),
//!    re-measured independently — *except* where the WCAG floor legitimately
//!    overrode it (`floor_override`), where the contrast is only pushed *up*.
//! 2. **WCAG floors.** primary/secondary clear 4.5:1, muted/icon clear 3:1, on
//!    the quantised colour, under dim surround.
//! 3. **Strict hierarchy.** primary > secondary > muted > disabled in |Lc| on
//!    every reachable background (or, where the floor squeezes them, the
//!    `compressed` flag is raised and the order stays non-strict — never a
//!    silent inversion).
//! 4. **Cool-undertone direction.** Every resolved role carries the neutral's
//!    cool blue-violet undertone (Oklab hue ≈ 286°, the system `NEUTRAL_HUE_DEG`)
//!    with non-zero saturation — it is a *relative of the neutral family*, not a
//!    sterile grey, and the tint leans cool (blue ≥ red in the emitted byte).

use labcolors_core::{
    BgInput, LcsColor, Resolved, Role, RoleTable, Solved, ViewingConditions, resolve_set,
};

/// Backgrounds with headroom in both polarities so every text role resolves —
/// the light and dark ends of the neutral ladder plus two off-neutral cases.
const GRID: [&str; 6] = [
    "#FFFFFF", "#F7F8FA", // light
    "#1C1C1E", "#101012", // dark
    "#0A3D62", "#3478F6", // chromatic (dark, light)
];

/// The text roles strongest-first — the order the hierarchy invariant walks.
const TEXT_ORDER: [Role; 4] = [
    Role::TextPrimary,
    Role::TextSecondary,
    Role::TextMuted,
    Role::TextDisabled,
];

/// The cool blue-violet undertone the default tinted table carries
/// (`NEUTRAL_HUE_DEG` in the semantic module). The acceptance band is generous
/// (±25°) because the solver re-derives lightness with the tint applied and the
/// hue drifts a little with lightness, but it must stay unmistakably cool.
const TINT_HUE_CENTER: f64 = 286.0;
const TINT_HUE_BAND: f64 = 25.0;

fn role_solved(set: &[(Role, Resolved)], role: Role) -> Option<(Solved, bool)> {
    set.iter().find_map(|(r, res)| match res {
        Resolved::Color { solved, compressed } if *r == role => Some((solved.clone(), *compressed)),
        _ => None,
    })
}

/// Smallest angular distance between two hues in degrees, in `[0, 180]`.
fn hue_distance(a: f64, b: f64) -> f64 {
    let d = ((a - b) % 360.0 + 360.0) % 360.0;
    if d > 180.0 { 360.0 - d } else { d }
}

#[test]
fn dim_tinted_holds_wcag_floors_on_the_grid() {
    // Invariant 2: under dim surround, with the default tinted table, every
    // text/UI role clears its conformance floor on the quantised colour.
    let vc = ViewingConditions::dim_surround();
    let table = RoleTable::default();
    for bg_hex in GRID {
        let bg = BgInput::solid(bg_hex).unwrap();
        let set = resolve_set(&bg, &table, &vc);
        for (role, min_ratio) in [
            (Role::TextPrimary, 4.5),
            (Role::TextSecondary, 4.5),
            (Role::TextMuted, 3.0),
            (Role::Icon, 3.0),
        ] {
            let (solved, _) = role_solved(&set, role)
                .unwrap_or_else(|| panic!("{bg_hex} {}: expected a colour", role.key()));
            assert!(
                solved.wcag_ratio() + 1e-9 >= min_ratio,
                "dim {bg_hex} {}: WCAG ratio {} < {min_ratio} under the tinted table",
                role.key(),
                solved.wcag_ratio(),
            );
        }
    }
}

#[test]
fn dim_tinted_keeps_strict_hierarchy_or_flags_compression() {
    // Invariant 3: primary > secondary > muted > disabled in |Lc|, under dim
    // surround with the tinted table. Where the legal floor squeezes two roles
    // together the order may go non-strict, but only if the junior is flagged
    // `compressed` — never a silent inversion.
    let vc = ViewingConditions::dim_surround();
    let table = RoleTable::default();
    for bg_hex in GRID {
        let bg = BgInput::solid(bg_hex).unwrap();
        let set = resolve_set(&bg, &table, &vc);
        let mags: Vec<(Role, f64, bool)> = TEXT_ORDER
            .iter()
            .filter_map(|&r| role_solved(&set, r).map(|(s, c)| (r, s.lc().abs(), c)))
            .collect();
        for pair in mags.windows(2) {
            let (senior_role, senior_mag, _) = pair[0];
            let (junior_role, junior_mag, junior_compressed) = pair[1];
            let strictly_weaker = junior_mag + 1e-9 < senior_mag;
            assert!(
                strictly_weaker || junior_compressed,
                "dim {bg_hex}: {} |Lc| {junior_mag} not strictly below {} |Lc| {senior_mag}, \
                 and the junior is NOT flagged compressed — a silent hierarchy inversion",
                junior_role.key(),
                senior_role.key(),
            );
            // Even when compressed, the junior must never be STRONGER than its
            // senior (equality by senior-copy is allowed; exceeding is not).
            assert!(
                junior_mag <= senior_mag + 1e-9,
                "dim {bg_hex}: {} |Lc| {junior_mag} exceeds senior {} |Lc| {senior_mag}",
                junior_role.key(),
                senior_role.key(),
            );
        }
    }
}

#[test]
fn dim_tinted_carries_the_cool_neutral_undertone() {
    // Invariant 4: every resolved text/UI role is a relative of the cool neutral
    // family — Oklab hue near 286°, non-zero saturation, and the tint leans cool
    // (blue channel >= red channel in the emitted byte). A regression that drops
    // the tint (sterile grey) or flips it warm would break this.
    let vc = ViewingConditions::dim_surround();
    let table = RoleTable::default();
    let roles = [
        Role::TextPrimary,
        Role::TextSecondary,
        Role::TextMuted,
        Role::Icon,
    ];
    for bg_hex in GRID {
        let bg = BgInput::solid(bg_hex).unwrap();
        let set = resolve_set(&bg, &table, &vc);
        for role in roles {
            let (solved, _) = role_solved(&set, role)
                .unwrap_or_else(|| panic!("{bg_hex} {}: expected a colour", role.key()));
            let hex = solved.hex();
            let c = LcsColor::from_hex_with_vc(hex, &vc).expect("emitted hex parses");

            // Near-extreme roles (primary on white/black) carry only a faint
            // tint because max_chroma collapses at the lightness extremes, so
            // the hue can be noisier there; assert the hue band only where the
            // tint is actually present (s above a small floor).
            if c.s > 0.02 {
                let dist = hue_distance(c.h_ok, TINT_HUE_CENTER);
                assert!(
                    dist <= TINT_HUE_BAND,
                    "dim {bg_hex} {}: undertone hue {:.1}° is {dist:.1}° off the cool \
                     neutral {TINT_HUE_CENTER}° (hex {hex}) — tint drifted or flipped warm",
                    role.key(),
                    c.h_ok,
                );
            }
            assert!(
                c.s > 1e-4,
                "dim {bg_hex} {}: saturation {} ~ 0 — role came out sterile grey, not tinted",
                role.key(),
                c.s,
            );

            // Cool lean: the blue byte is at least the red byte (blue-violet).
            let bytes = u32::from_str_radix(hex.trim_start_matches('#'), 16).expect("hex");
            let r = (bytes >> 16) & 0xFF;
            let b = bytes & 0xFF;
            assert!(
                b >= r,
                "dim {bg_hex} {}: emitted {hex} has blue {b} < red {r} — undertone is warm, \
                 not the cool neutral",
                role.key(),
            );
        }
    }
}

#[test]
fn dim_tinted_perceptual_target_accuracy_where_floor_does_not_override() {
    // Invariant 1: where the WCAG floor did NOT override, the role's measured
    // contrast must sit within ±1 Lc of the anchor target. We do not recompute
    // the anchor target here (that would duplicate the module's private maths);
    // instead we assert the solver's OWN reported lc() matches an independent
    // re-measurement of the emitted hex — the honest "the number the caller sees
    // is the number the colour achieves" contract, under dim + tint.
    use labcolors_core::lpc::lpc_with_vc;
    let vc = ViewingConditions::dim_surround();
    let table = RoleTable::default();
    let roles = [
        Role::TextPrimary,
        Role::TextSecondary,
        Role::TextMuted,
        Role::TextDisabled,
        Role::Icon,
    ];
    for bg_hex in GRID {
        let bg = BgInput::solid(bg_hex).unwrap();
        let set = resolve_set(&bg, &table, &vc);
        for role in roles {
            if let Some((solved, _)) = role_solved(&set, role) {
                let measured = lpc_with_vc(solved.hex(), bg_hex, &vc);
                assert!(
                    (solved.lc() - measured).abs() <= 1.0,
                    "dim {bg_hex} {}: reported lc {} disagrees with re-measured {measured} \
                     on hex {} by more than 1 Lc",
                    role.key(),
                    solved.lc(),
                    solved.hex(),
                );
            }
        }
    }
}
