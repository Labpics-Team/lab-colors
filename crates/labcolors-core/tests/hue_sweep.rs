//! Contract 1 — solver hue sweep (the already-fired bug class).
//!
//! BUG CLASS this guards: *an axis the test suite never swept.* The crate's
//! audit found that 6/10 mutations of key constants survived the suite, and the
//! solver was exercised only at hue 0 (and a couple of incidental chromatic
//! points). Hue and the gamut cap change which quantised colour can reproduce a
//! fixed Ys candidate score. A regression in that search could miss its numeric
//! target only on part of the hue domain.
//!
//! This sweeps hue 0..360 in 15 steps, both viewing conditions, on a light
//! (#FFFFFF) and a dark (#1C1C1E) background, at a moderate chroma
//! (`Relative(0.3)`), and asserts two contracts on every result:
//!
//! 1. **Ys candidate-score target held unless the WCAG floor overrode it.** When the WCAG
//!    floor did NOT override (`!floor_override`), the measured |Lc − target| ≤ 1
//!    (the solver's own ±1 quantization budget), independently re-measured
//!    through the public `recheck_against` on the emitted hex — not trusting
//!    the reported `lc()`. When the AA-text floor DID override (a +45 target on
//!    white cannot clear 4.5:1, so the solver legitimately pushes the colour
//!    darker), the numeric target is intentionally exceeded: there the contract is
//!    |Lc| ≥ target, not |Lc − target| ≤ 1. Asserting the tight band there would
//!    be testing a bug into existence — the override is the documented, correct
//!    behaviour (`Solved::floor_override`).
//! 2. **WCAG invariant held.** A `Contract::text` carries the AA-text floor
//!    (4.5:1); a successful solve must clear it on the quantised colour, in
//!    every case, override or not.
//!
//! And it pins the error model: an `Err` is acceptable only for *principled*
//! reasons (out of range, dead zone, bounded search exhausted, floor unreachable), never
//! a malformed-input panic — and a moderate +45 target on pure white, which the
//! background can clearly host, must NOT come back ExceedsRange.

use labcolors_core::{
    BgInput, ChromaPolicy, Contract, Floor, Gamut, Hue, SolveFailure, ViewingConditions,
    recheck_against, solve,
};

/// Независимое повторное измерение знаковой candidate-оценки `Lc` по `Ys` на
/// финальных sRGB8-байтах. Проверка через публичный `recheck_against` сохраняет
/// тест на фактически испускаемом состоянии и его полярности.
fn candidate_lc(fg_hex: &str, bg_hex: &str, vc: &ViewingConditions) -> f64 {
    recheck_against(bg_hex, &[fg_hex], vc).expect("solver and fixture emit valid sRGB8 hex")[0].0
}

/// The solver's own quantization budget (mirrors `solve::QUANT_BUDGET` / the
/// `TOL` in its inline tests): a reachable target is hit within 1 Lc.
const TOL: f64 = 1.0;

/// WCAG 2.1 AA normal-text ratio — the floor `Contract::text` enforces.
const AA_TEXT_RATIO: f64 = 4.5;

fn vcs() -> [(ViewingConditions, &'static str); 2] {
    [
        (ViewingConditions::srgb(), "srgb"),
        (ViewingConditions::dim_surround(), "dim"),
    ]
}

#[test]
fn solver_holds_ys_candidate_score_target_across_the_full_hue_circle() {
    // Инверсия Ys candidate-score без юридического floor обязана попадать в
    // ±1 Lc на каждом оттенке. Регрессия в hue/gamut search проявится здесь как
    // промах по численной цели на части окружности.
    //
    // bg, signed target the background can host in its natural polarity:
    // dark-on-light (+) on white, light-on-dark (−) on near-black.
    let cases = [("#FFFFFF", 45.0_f64), ("#1C1C1E", -45.0_f64)];

    let mut reachable = 0_usize;
    let mut max_err = 0.0_f64;

    for (vc, vc_name) in vcs() {
        for (bg_hex, target) in cases {
            let mut hue_deg = 0.0_f64;
            while hue_deg < 360.0 {
                let bg = BgInput::solid(bg_hex).unwrap();
                let result = solve(
                    bg,
                    Contract::text(target).with_conformance(Floor::None),
                    Hue::deg(hue_deg),
                    ChromaPolicy::Relative(0.3),
                    &vc,
                    Gamut::Srgb,
                );
                match result {
                    Ok(solved) => {
                        reachable += 1;
                        // Re-measure independently on the emitted hex; never
                        // trust the reported lc(). Sign and magnitude come from
                        // the same production recheck path.
                        let measured = candidate_lc(solved.hex(), bg_hex, &vc);
                        assert_eq!(
                            measured > 0.0,
                            target > 0.0,
                            "{vc_name} {bg_hex} hue {hue_deg}: polarity sign mismatch, \
                             measured {measured} for target {target}",
                        );
                        // With Floor::None there is no override, so the tight ±1
                        // candidate-score budget must hold at every hue.
                        assert!(
                            !solved.floor_override(),
                            "{vc_name} {bg_hex} hue {hue_deg}: Floor::None must never override",
                        );
                        let err = (measured - target).abs();
                        max_err = max_err.max(err);
                        assert!(
                            err <= TOL,
                            "{vc_name} {bg_hex} hue {hue_deg}: target {target}, measured \
                             {measured} (hex {}), err {err:.4} > {TOL}",
                            solved.hex(),
                        );
                    }
                    Err(SolveFailure::InvalidInput(msg)) => {
                        panic!(
                            "{vc_name} {bg_hex} hue {hue_deg}: malformed-input error on a \
                             well-formed solve: {msg}"
                        );
                    }
                    // A principled refusal (e.g. a hue whose tiny in-gamut chroma
                    // exhausts the bounded neighbour search) is legal; the white-specific
                    // guard below rejects the one variant that would be a bug.
                    Err(_) => {}
                }
                hue_deg += 15.0;
            }
        }
    }

    // The sweep must actually solve the bulk of the circle — a suite that
    // silently refused everything would be a false green.
    assert!(
        reachable >= 90,
        "hue sweep solved too few combinations ({reachable}); the solver may be \
         refusing reachable targets across the hue circle"
    );
    eprintln!("hue candidate-score sweep: {reachable} reachable, max |Lc - target| = {max_err:.4}");
}

#[test]
fn solver_holds_wcag_floor_across_the_full_hue_circle() {
    // The second half of contract 1's invariant: a default Contract::text carries
    // the AA-text floor (4.5:1), so EVERY successful solve — at every hue, both
    // VCs, both backgrounds — must clear it on the quantised colour, whether or
    // not the floor had to override the candidate-score target. A regression that let the floor
    // be reported satisfied while the emitted hex falls short would surface here.
    let cases = [("#FFFFFF", 45.0_f64), ("#1C1C1E", -45.0_f64)];

    for (vc, vc_name) in vcs() {
        for (bg_hex, target) in cases {
            let mut hue_deg = 0.0_f64;
            while hue_deg < 360.0 {
                let bg = BgInput::solid(bg_hex).unwrap();
                let result = solve(
                    bg,
                    Contract::text(target), // default AA-text floor (4.5:1)
                    Hue::deg(hue_deg),
                    ChromaPolicy::Relative(0.3),
                    &vc,
                    Gamut::Srgb,
                );
                if let Ok(solved) = result {
                    assert!(
                        solved.wcag_ratio() + 1e-9 >= AA_TEXT_RATIO,
                        "{vc_name} {bg_hex} hue {hue_deg}: WCAG ratio {} < {AA_TEXT_RATIO} \
                         despite an AA-text contract (hex {})",
                        solved.wcag_ratio(),
                        solved.hex(),
                    );
                } else if let Err(SolveFailure::InvalidInput(msg)) = result {
                    panic!(
                        "{vc_name} {bg_hex} hue {hue_deg}: malformed-input error on a \
                         well-formed solve: {msg}"
                    );
                }
                hue_deg += 15.0;
            }
        }
    }
}

#[test]
fn moderate_positive_target_on_white_is_never_out_of_range_at_any_hue() {
    // The sharp end of contract 1: a +45 dark-on-light target on PURE WHITE is
    // comfortably reachable (white hosts up to ~106 Lc), so at NO hue may the
    // solver report ExceedsRange — that would mean it believes white cannot
    // supply a mid contrast, the exact false-unreachable failure mode. A
    // BoundedSearchExhausted is the only tolerated near-miss, and even that must
    // not occur here in practice; ExceedsRange never may.
    for (vc, vc_name) in vcs() {
        let mut hue_deg = 0.0_f64;
        while hue_deg < 360.0 {
            let bg = BgInput::solid("#FFFFFF").unwrap();
            let result = solve(
                bg,
                Contract::text(45.0),
                Hue::deg(hue_deg),
                ChromaPolicy::Relative(0.3),
                &vc,
                Gamut::Srgb,
            );
            if let Err(err) = result {
                assert!(
                    !matches!(err, SolveFailure::ExceedsRange { .. }),
                    "{vc_name} hue {hue_deg}: white reported ExceedsRange for +45 Lc \
                     — false-unreachable regression: {err:?}"
                );
                // Any other Err here would also be surprising on white; surface it.
                assert!(
                    matches!(err, SolveFailure::BoundedSearchExhausted { .. }),
                    "{vc_name} hue {hue_deg}: unexpected refusal of +45 on white: {err:?}"
                );
            }
            hue_deg += 15.0;
        }
    }
}
