//! Reproduces the RMS sweep that pins `TINT_TARGET_MP` (semantic.rs) using the
//! REAL engine curve. Run:
//!
//! ```sh
//! cargo +1.96.0 run -p labcolors-core --example tint_target_sweep
//! ```
//!
//! Value-preserving provenance: this changes NO shipped value; it only measures
//! and prints. The metric is identical to the in-crate `#[cfg(test)]` test
//! `curve_fits_reference_plateau_colorfulness`: the residual is the RMS over the
//! reference plateau (Oklab L in `[0.45, 0.90]`) of
//! `|realised_curve_M'(l, target) − reference_M'|`, where the curve `M'` is the
//! GAMUT-CLAMPED engine curve built to `target` (NOT the raw target). All engine
//! work runs inside `labcolors_core::semantic::tint_target_sweep_repro`, so these
//! numbers cannot drift from the test.

fn main() {
    // Oklab-L plateau window (matches the in-crate test).
    let (l_min, l_max) = (0.45_f64, 0.90_f64);
    // Candidate CAM16-UCS M' targets, 0.01 resolution over [4.0, 8.0].
    let targets: Vec<f64> = (400..=800).map(|i| f64::from(i) / 100.0).collect();

    let (plateau, sweep) =
        labcolors_core::semantic::tint_target_sweep_repro(&targets, l_min, l_max);

    println!(
        "plateau nodes (Oklab L in [{l_min}, {l_max}]): {}",
        plateau.len()
    );
    println!("  Oklab-L   reference M'");
    for (l, mp) in &plateau {
        println!("  {l:>7.4}   {mp:>6.3}");
    }

    println!("\ntarget   RMS      MAX  (|curve M' - ref M'|)");
    for (t, rms, max) in &sweep {
        // Print every 0.2 M' to keep the table short.
        if ((t * 100.0).round() as i64) % 20 == 0 {
            println!("  {t:>5.2}   {rms:>6.3}   {max:>6.3}");
        }
    }

    let (best_t, best_rms) =
        sweep
            .iter()
            .copied()
            .fold((f64::NAN, f64::INFINITY), |acc, (t, r, _)| {
                if r < acc.1 { (t, r) } else { acc }
            });
    let (rms_declared, max_declared) = sweep
        .iter()
        .find(|(t, _, _)| (t - 6.1).abs() < 1e-9)
        .map_or((f64::NAN, f64::NAN), |&(_, r, m)| (r, m));

    println!("\nplateau nodes: {}", plateau.len());
    println!("swept RMS-argmin target t* = {best_t:.3}  (RMS {best_rms:.3})");
    println!(
        "at declared TINT_TARGET_MP = 6.1 : RMS {rms_declared:.3}, MAX per-node {max_declared:.3}"
    );
}
