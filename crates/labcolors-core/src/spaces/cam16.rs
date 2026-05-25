/**
 * Core CIECAM16 nonlinear adaptation functions.
 *
 * These are the forward and inverse compressive transforms applied
 * to cone responses after chromatic adaptation.
 */

/// Forward nonlinear adaptation.
///
/// Source: CIE 170-2:2015 eq. (6.5).
pub(crate) fn adapt(c: f64, fl: f64) -> f64 {
    let x = fl * c.abs() / 100.0;
    let y = x.powf(0.42);
    c.signum() * 400.0 * y / (y + 27.13)
}

/// Inverse nonlinear adaptation.
pub(crate) fn unadapt(a: f64, fl: f64) -> f64 {
    let x = a.abs();
    let y = (27.13 * x / (400.0 - x)).max(0.0);
    a.signum() * 100.0 * y.powf(1.0 / 0.42) / fl
}
