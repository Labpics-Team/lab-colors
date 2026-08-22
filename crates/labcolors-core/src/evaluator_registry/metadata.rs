//! Domain-bounds and uncertainty metadata for evaluator releases.
//! These types are versioned independently of the evaluator protocol to allow
//! metadata schema evolution without breaking evaluator registration.
//!
//! WIRE NOTE: These types have no serde derives. Serialization/deserialization
//! is performed by mirror types in labcolors-wasm via manual From/Into conversions.
//! This preserves core's zero-dependency invariant.

/// Domain bounds within which an evaluator's verdict is valid.
/// An invocation outside these bounds is a protocol error, not a violation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EvaluatorApplicabilityV1 {
    /// Human-readable domain description (e.g., "sRGB8 foreground/background pairs").
    pub(crate) domain_description: &'static str,
    /// Minimum sample size used to validate this release's classifier thresholds.
    /// `None` when the evaluator is analytically exact (e.g., ExactSrgb8Identity).
    pub(crate) validation_sample_size: Option<u64>,
    /// Confidence interval width at 95% coverage, in the evaluator's native
    /// measurement unit. `None` for deterministic evaluators.
    pub(crate) confidence_interval_95: Option<f64>,
}

impl EvaluatorApplicabilityV1 {
    /// Default applicability for evaluators that have not yet declared their domain.
    /// Production evaluators MUST override before F-03 exit.
    pub(crate) const fn undeclared() -> Self {
        Self {
            domain_description: "undeclared",
            validation_sample_size: None,
            confidence_interval_95: None,
        }
    }
}

/// Classification of uncertainty sources that apply to a specific evaluator release.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum EvaluatorUncertaintyV1 {
    /// Instrument or quantization noise in the measurement stage.
    MeasurementError { max_absolute: f64 },
    /// Bias introduced by non-representative validation corpus.
    SamplingBias { description: &'static str },
    /// Model parameters drifted from validation conditions.
    ModelDrift { reference_release: &'static str },
    /// Symmetric confidence interval expressed as binary64 (IEEE 754 double).
    /// Used for statistical/readability models where uncertainty is best
    /// represented as a numeric interval rather than a categorical label.
    /// `half_width` is the half-width of the 95% CI in the evaluator's native unit.
    ///
    /// INVARIANT: `half_width` MUST be finite and non-NaN. Construction through
    /// `new_confidence_interval_binary64` enforces this at runtime. Direct field
    /// access is `pub(crate)` for test convenience; production code MUST use the
    /// constructor.
    ConfidenceIntervalBinary64 { half_width: f64 },
    /// No applicable uncertainty — evaluator is analytically exact.
    None,
}

/// Error returned when constructing an invalid `ConfidenceIntervalBinary64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UncertaintyConstructionErrorV1 {
    /// The half-width value is NaN.
    NaN,
    /// The half-width value is positive or negative infinity.
    Infinite,
}

impl EvaluatorUncertaintyV1 {
    /// Constructs a `ConfidenceIntervalBinary64` variant with IEEE 754 binary64
    /// validation. Rejects NaN and Inf per the wire contract (§1.2).
    /// Zero is permitted (represents exact measurement with declared statistical model).
    pub(crate) fn new_confidence_interval_binary64(
        half_width: f64,
    ) -> Result<Self, UncertaintyConstructionErrorV1> {
        if half_width.is_nan() {
            return Err(UncertaintyConstructionErrorV1::NaN);
        }
        if half_width.is_infinite() {
            return Err(UncertaintyConstructionErrorV1::Infinite);
        }
        Ok(Self::ConfidenceIntervalBinary64 { half_width })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_interval_rejects_nan() {
        let result = EvaluatorUncertaintyV1::new_confidence_interval_binary64(f64::NAN);
        assert_eq!(result, Err(UncertaintyConstructionErrorV1::NaN));
    }

    #[test]
    fn confidence_interval_rejects_positive_infinity() {
        let result = EvaluatorUncertaintyV1::new_confidence_interval_binary64(f64::INFINITY);
        assert_eq!(result, Err(UncertaintyConstructionErrorV1::Infinite));
    }

    #[test]
    fn confidence_interval_rejects_negative_infinity() {
        let result = EvaluatorUncertaintyV1::new_confidence_interval_binary64(f64::NEG_INFINITY);
        assert_eq!(result, Err(UncertaintyConstructionErrorV1::Infinite));
    }

    #[test]
    fn confidence_interval_accepts_zero() {
        let result = EvaluatorUncertaintyV1::new_confidence_interval_binary64(0.0);
        assert_eq!(
            result,
            Ok(EvaluatorUncertaintyV1::ConfidenceIntervalBinary64 { half_width: 0.0 })
        );
    }

    #[test]
    fn confidence_interval_accepts_finite_positive() {
        let result = EvaluatorUncertaintyV1::new_confidence_interval_binary64(0.05);
        assert_eq!(
            result,
            Ok(EvaluatorUncertaintyV1::ConfidenceIntervalBinary64 { half_width: 0.05 })
        );
    }

    #[test]
    fn applicability_undeclared_has_expected_defaults() {
        let app = EvaluatorApplicabilityV1::undeclared();
        assert_eq!(app.domain_description, "undeclared");
        assert_eq!(app.validation_sample_size, None);
        assert_eq!(app.confidence_interval_95, None);
    }
}
