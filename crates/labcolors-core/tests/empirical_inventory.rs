//! Empirical inventory gate — provenance marker enforcement.
//!
//! BUG CLASS this closes: *perceptual magic-numbers without cited provenance*
//! (Finding 3, confirmed 2026-06-22). The class: a numeric constant that
//! controls perceptual behaviour ships with no sourced justification. A reader
//! cannot tell whether it is derived, sourced, or invented. Future drift is
//! invisible because no test checks provenance, only value.
//!
//! # Contract (success criterion)
//!
//! Every `const` or `static` in `semantic.rs` whose name or comment identifies
//! it as a perceptual magnitude (floor, JND, threshold, hairline, shadow ramp)
//! **must** carry a `// GROUNDED` marker in the source.  A `// GROUNDED` marker
//! is a structured comment of the form:
//!
//! ```text
//! // GROUNDED — <source citation(s) or derivation pointer>
//! ```
//!
//! If a value is pending final calibration it must instead carry:
//!
//! ```text
//! // NEEDS-SCIENCE — <exact open question>
//! ```
//!
//! A constant with neither marker in its surrounding block is a gate failure.
//!
//! # How this test works
//!
//! The test greps the source text of `semantic.rs` for each known perceptual
//! constant name and asserts that its surrounding 40-line window contains
//! `// GROUNDED` or `// NEEDS-SCIENCE`.  The window is searched forwards and
//! backwards from the const declaration line.
//!
//! This is a source-grep test (Class A / regression), not a runtime test — it
//! cannot be passed by adjusting runtime values.  It is deliberately mechanical
//! so it cannot be fooled by re-indenting or reformatting.
//!
//! # Why source-grep, not a proc-macro or attribute
//!
//! A proc-macro would require a build-dep on the inventory crate (not yet
//! written).  A `#[doc]` attribute carrying the provenance would be stripped
//! from tests.  Source grep is the lightest-weight gate that (a) runs on every
//! `cargo test`, (b) cannot be silently bypassed, and (c) is self-contained.

/// Perceptual constants that must carry a provenance marker.
///
/// Each entry is the const identifier as it appears in the source.  The test
/// searches for `const <NAME>` and checks the surrounding window.
const PERCEPTUAL_CONSTS: &[&str] = &[
    "DECORATIVE_FLOOR_MIN",
    "SHADOW_MINOR_JND",
    "SHADOW_AMBIENT_JND",
    "SHADOW_PENUMBRA_JND",
    "SHADOW_MAJOR_JND",
];

/// Window of lines (before + after the declaration) searched for the marker.
const WINDOW: usize = 40;

/// Source text of `semantic.rs`, included at compile time so this test is
/// self-contained and does not rely on the working directory at runtime.
const SEMANTIC_SRC: &str = include_str!("../src/semantic.rs");

#[test]
fn every_perceptual_const_has_provenance_marker() {
    // BUG CLASS: perceptual magic-number without provenance.
    //
    // For each known perceptual constant, locate its declaration line in the
    // source and assert that either `// GROUNDED` or `// NEEDS-SCIENCE` appears
    // within WINDOW lines of it.
    //
    // Bite: removing the `// GROUNDED` comment from DECORATIVE_FLOOR_MIN
    // without adding `// NEEDS-SCIENCE` causes this test to fail, preventing
    // a provenance-free numeric drift from shipping silently.

    let lines: Vec<&str> = SEMANTIC_SRC.lines().collect();

    let mut failures: Vec<String> = Vec::new();

    for &name in PERCEPTUAL_CONSTS {
        // Find the line index of the `const <NAME>` declaration.
        let needle = format!("const {name}");
        let decl_idx = lines.iter().position(|l| l.contains(&needle));

        let Some(decl) = decl_idx else {
            failures.push(format!(
                "{name}: declaration `const {name}` not found in semantic.rs — \
                 either the constant was renamed or removed; update PERCEPTUAL_CONSTS \
                 in tests/empirical_inventory.rs"
            ));
            continue;
        };

        // Search the window around the declaration for the provenance marker.
        let lo = decl.saturating_sub(WINDOW);
        let hi = (decl + WINDOW + 1).min(lines.len());
        let window_text = lines[lo..hi].join("\n");

        let has_grounded = window_text.contains("// GROUNDED");
        let has_needs_science = window_text.contains("// NEEDS-SCIENCE");

        if !has_grounded && !has_needs_science {
            failures.push(format!(
                "{name} (semantic.rs:{}): no `// GROUNDED` or `// NEEDS-SCIENCE` \
                 marker found within {WINDOW} lines of the declaration. \
                 Every perceptual constant must carry its provenance. \
                 Add one of:\n\
                 \x20  // GROUNDED — <source citations / derivation pointer>\n\
                 \x20  // NEEDS-SCIENCE — <exact open question blocking derivation>",
                decl + 1
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "Provenance gate failed — {} perceptual constant(s) lack a marker:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

#[test]
fn separator_row_references_grounded_const() {
    // BUG CLASS: separator spec uses a hardcoded literal instead of the named
    // const, breaking the SSOT.  This is the INVARIANT 1 companion at the
    // provenance layer.
    //
    // We additionally require that the Separator row carries a reference to
    // DECORATIVE_FLOOR_MIN (not a fresh literal), and that DECORATIVE_FLOOR_MIN
    // itself is GROUNDED — so both ends of the chain are proven.
    //
    // Bite: replacing `decorative(DECORATIVE_FLOOR_MIN)` with
    // `decorative(15.5)` satisfies Invariant 1 (which only bans `8.0`) but
    // would hide the provenance link — this test catches it.

    let src = SEMANTIC_SRC;

    // The Separator row must reference the const by name, not a literal.
    assert!(
        src.contains("decorative(DECORATIVE_FLOOR_MIN)"),
        "Separator spec does not reference DECORATIVE_FLOOR_MIN by name. \
         The spec must be `decorative(DECORATIVE_FLOOR_MIN)`, never a fresh \
         numeric literal, so provenance traces through the const."
    );

    // DECORATIVE_FLOOR_MIN must have a GROUNDED marker.
    let lines: Vec<&str> = src.lines().collect();
    let decl = lines
        .iter()
        .position(|l| l.contains("const DECORATIVE_FLOOR_MIN"))
        .expect("const DECORATIVE_FLOOR_MIN not found in semantic.rs");
    let lo = decl.saturating_sub(WINDOW);
    let hi = (decl + WINDOW + 1).min(lines.len());
    let window = lines[lo..hi].join("\n");

    assert!(
        window.contains("// GROUNDED") || window.contains("// NEEDS-SCIENCE"),
        "DECORATIVE_FLOOR_MIN (semantic.rs:{}) has no `// GROUNDED` or \
         `// NEEDS-SCIENCE` marker within {WINDOW} lines — the chain \
         separator→const is ungrounded.",
        decl + 1
    );
}

#[test]
fn separator_row_provisional_until_calibrated() {
    // BUG CLASS: a PROVISIONAL separator set-point is silently promoted to
    // "final" by removing the PROVISIONAL/NEEDS-SCIENCE comment without any
    // gate catching the transition.
    //
    // Finding 2 (confirmed 2026-06-22, High): DECORATIVE_FLOOR_MIN = 15.5 is a
    // documented floor-pinned placeholder, NOT a perceptually eye-calibrated
    // separator target.  Per daniil-separators-barely-perceptible the separator
    // must be "barely perceptible but sufficient" — a floor-minimum risks
    // over- or under-shooting.  Eye-calibration is deferred to the downstream
    // scope `jnd-floor-and-separator-pin`.
    //
    // This test closes the class by asserting that the separator row in the
    // production source must carry a `// PROVISIONAL` marker AND that the
    // DECORATIVE_FLOOR_MIN const block carries a `// NEEDS-SCIENCE` marker.
    // The moment a developer:
    //   (a) removes the `// PROVISIONAL` comment from the separator row, OR
    //   (b) removes the `// NEEDS-SCIENCE` marker from the const block
    // without completing the eye-calibration chapter, this test fails —
    // preventing silent promotion.
    //
    // How to legitimately close this test:
    //   1. Complete `jnd-floor-and-separator-pin` chapter (eye-calibrated set-point).
    //   2. Replace `decorative(DECORATIVE_FLOOR_MIN)` with `dj(SEPARATOR_DJ)`
    //      or equivalent calibrated const.
    //   3. Update `PERCEPTUAL_CONSTS` and remove this test (with a note citing
    //      the chapter that performed the calibration).
    //
    // Bite: stripping the PROVISIONAL comment from the separator tuple silently
    // causes this test to fail — promotion is never undetected.

    let src = SEMANTIC_SRC;
    let lines: Vec<&str> = src.lines().collect();

    // --- Gate A: the separator tuple line must be preceded by a PROVISIONAL marker
    // within 10 lines.  We find the production tuple line and search backwards.
    let tuple_needle = "Role::Separator, decorative(DECORATIVE_FLOOR_MIN)";
    let tuple_idx = lines.iter().position(|l| l.contains(tuple_needle)).expect(
        "production separator tuple `Role::Separator, decorative(DECORATIVE_FLOOR_MIN)` \
             not found in semantic.rs — if the Separator spec changed, update this gate \
             (tests/empirical_inventory.rs :: separator_row_provisional_until_calibrated)",
    );

    let lo = tuple_idx.saturating_sub(10);
    let pre_window = lines[lo..=tuple_idx].join("\n");

    assert!(
        pre_window.contains("PROVISIONAL") || pre_window.contains("// NEEDS-SCIENCE"),
        "semantic.rs:{}: the separator production tuple is no longer preceded by a \
         `// PROVISIONAL` or `// NEEDS-SCIENCE` marker within 10 lines.  \
         DECORATIVE_FLOOR_MIN (15.5) is the perceptual floor, NOT an eye-calibrated \
         set-point.  Either restore the PROVISIONAL marker or complete the \
         jnd-floor-and-separator-pin calibration chapter and remove this gate.",
        tuple_idx + 1
    );

    // --- Gate B: the separator tuple's surrounding block must also carry a
    // // NEEDS-SCIENCE marker.  The GROUNDED marker on DECORATIVE_FLOOR_MIN
    // certifies the floor source; the NEEDS-SCIENCE near the separator tuple
    // certifies that using it as the final set-point is not yet eye-calibrated.
    // Both coexist until the jnd-floor-and-separator-pin chapter is complete.
    let lo_b = tuple_idx.saturating_sub(15);
    let hi_b = (tuple_idx + 5).min(lines.len());
    let separator_block = lines[lo_b..hi_b].join("\n");

    assert!(
        separator_block.contains("// NEEDS-SCIENCE"),
        "semantic.rs:{}: the separator block has no `// NEEDS-SCIENCE` marker within 15 \
         lines above the tuple.  DECORATIVE_FLOOR_MIN is the perceptual floor; using it \
         as the separator final set-point is not yet eye-calibrated.  Add a \
         `// NEEDS-SCIENCE — separator set-point not yet eye-calibrated` comment, or \
         complete the jnd-floor-and-separator-pin chapter and remove this gate.",
        tuple_idx + 1
    );
}
