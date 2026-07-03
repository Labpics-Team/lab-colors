//! Named-path text-hierarchy compression (ADR-0001 PR-c, point 4).
//!
//! V1 found the string-keyed path ran no hierarchy-compression pass, so a general
//! config on a near-AA mid-grey could silently collapse two labels onto one
//! colour. `resolve_named_set` now runs `enforce_named_text_hierarchy`. This gate
//! proves the pass:
//!   * fires (honest `compressed` flag) where the ladder is genuinely squeezed;
//!   * is a no-op on the golden-grid backgrounds (why the fixture stays
//!     byte-identical — see `labui_emission_golden`);
//!   * reads the ladder off the config, not off names: `icon`/`border-strong`
//!     (lone anchors) are never swept in.
//!
//! RED-proof: a neutral anchor's `compressed` flag is set ONLY by the pass
//! (`Resolved::color` hard-codes `compressed: false`). Comment out the
//! `enforce_named_text_hierarchy` call in `resolve_named_set` and
//! `pass_fires_and_flags_when_ladder_is_squeezed` fails.

use labcolors_core::config::labui_reference;
use labcolors_core::{BgInput, NamedRoleTable, ViewingConditions, resolve_named_set};

fn labui() -> NamedRoleTable {
    labui_reference()
        .compile_named_role_table()
        .expect("labui fixture compiles")
}

fn abs_lc(set: &[(String, labcolors_core::Resolved)], name: &str) -> f64 {
    set.iter()
        .find(|(n, _)| n == name)
        .and_then(|(_, r)| r.lc())
        .map(f64::abs)
        .unwrap_or_else(|| panic!("role `{name}` missing/unreachable"))
}

fn compressed(set: &[(String, labcolors_core::Resolved)], name: &str) -> bool {
    set.iter()
        .find(|(n, _)| n == name)
        .map(|(_, r)| r.compressed())
        .unwrap_or_else(|| panic!("role `{name}` missing"))
}

fn hex(set: &[(String, labcolors_core::Resolved)], name: &str) -> String {
    set.iter()
        .find(|(n, _)| n == name)
        .and_then(|(_, r)| r.solved())
        .map(|s| s.hex().to_string())
        .unwrap_or_else(|| panic!("role `{name}` not a solved colour"))
}

const LABELS: [&str; 4] = [
    "label-primary",
    "label-secondary",
    "label-tertiary",
    "label-quaternary",
];

#[test]
fn pass_fires_and_flags_when_ladder_is_squeezed() {
    // `#747474` is a near-AA mid-grey where the readable window is narrower than
    // the label steps: primary and secondary are floored onto the same colour.
    let table = labui();
    let bg = BgInput::solid("#747474").unwrap();
    let set = resolve_named_set(&bg, &table, &ViewingConditions::srgb());

    // Secondary collapsed onto primary — but the pass makes it HONEST, not silent.
    assert_eq!(
        hex(&set, "label-primary"),
        hex(&set, "label-secondary"),
        "on #747474 secondary is expected to be floored onto primary"
    );
    assert!(
        compressed(&set, "label-secondary"),
        "squeezed junior MUST carry the compressed flag — the pass fired \
         (a neutral anchor is compressed ONLY by enforce_named_text_hierarchy)"
    );
    // The senior is never flagged, and the ladder never inverts.
    assert!(
        !compressed(&set, "label-primary"),
        "senior must not be flagged compressed"
    );
    let mags: Vec<f64> = LABELS.iter().map(|l| abs_lc(&set, l)).collect();
    for w in mags.windows(2) {
        assert!(
            w[0] + 1e-9 >= w[1],
            "label ladder must stay non-strict-descending, got {mags:?}"
        );
    }
}

#[test]
fn pass_does_not_sweep_in_lone_anchors() {
    // `icon` (fraction 0.461, sits above quaternary 0.276) and `border-strong`
    // (0.968) are lone anchors, not rungs of the label ladder: the grouping reads
    // strictly-descending runs off the config, so these are never compressed even
    // on the squeezing background.
    let table = labui();
    let bg = BgInput::solid("#747474").unwrap();
    let set = resolve_named_set(&bg, &table, &ViewingConditions::srgb());
    assert!(
        !compressed(&set, "icon"),
        "icon must not join the label ladder"
    );
    assert!(
        !compressed(&set, "border-strong"),
        "border-strong must not join the label ladder"
    );
}

#[test]
fn pass_is_a_noop_on_the_golden_grid() {
    // None of the fixture's byte-identity backgrounds sit in the squeeze band, so
    // no label is compressed there — this is why `labui_emission_golden` stays
    // byte-for-byte green after the port.
    let table = labui();
    for (vc, _) in [
        (ViewingConditions::srgb(), "srgb"),
        (ViewingConditions::dim_surround(), "dim"),
    ] {
        for bg_hex in [
            "#FFFFFF", "#F2F2F7", "#7F7F7F", "#1C1C1E", "#101012", "#3478F6",
        ] {
            let bg = BgInput::solid(bg_hex).unwrap();
            let set = resolve_named_set(&bg, &table, &vc);
            for l in LABELS {
                assert!(
                    !compressed(&set, l),
                    "no label may be compressed on golden bg {bg_hex}: `{l}` was"
                );
            }
        }
    }
}
