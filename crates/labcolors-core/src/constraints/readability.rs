//! Frozen display-domain readability classifier (F5).
//!
//! This is the runtime recheck predicate for the full-support bridge. Unlike
//! [`ExactSrgb8IdentityV1`](crate::constraints::ExactSrgb8IdentityV1) — which
//! measures backdrop-independent *byte identity* and would let every opaque
//! role pass trivially — this classifier measures the *readability* of a final
//! visible occurrence against its role floor over the frozen legacy WCAG 2.1
//! display-domain curve (`relative_luminance` -> `contrast_core` / WCAG ratio,
//! ADR-0003, viewing-condition-independent). It introduces no new metric: the
//! exact same `crate::wcag` / `crate::lpc` SSOT that `recheck_against` uses is
//! read here, so a solid occurrence's forward is byte-identical to the
//! colour-only path by construction.
//!
//! The classifier plugs into the same sealed `Evaluator` + `HardClassifier`
//! seam every other predicate uses, so the recheck bridge iterates one typed
//! decision per case per occurrence and the finite / shape / polarity of every
//! output is core-owned rather than re-derived by host float guards.

use crate::Srgb8;
use crate::appearance::ModeledSrgb8PointOccurrence;
use crate::constraints::{Evaluator, HardClassifier, HardDecision, private};
use crate::solve::Floor;
use core::convert::Infallible;

/// Structural identity of the frozen display-domain readability law. It carries
/// no client ID, no target bytes and no chosen floor: those belong to a
/// concrete invocation, not to the law of the check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadabilityCurveIdentityV1 {
    FrozenDisplayLuminanceContrastV1,
}

/// Release of the frozen readability formula — the legacy WCAG 2.1 (2018)
/// display-domain profile shared with `recheck_against`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadabilityCurveReleaseV1 {
    LegacyWcag21DisplayV1,
}

/// Narrow capability: only the final modeled point occurrence in encoded-sRGB8,
/// never the source Paint and never an arbitrary renderer's pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadabilityCurveCapabilityV1 {
    FinalOccurrenceDisplayReadabilityV1,
}

/// Typed polarity of a final occurrence over the signed candidate curve. It
/// replaces host-side `Math.abs` / sign inspection: the direction is decided
/// once, in Core, from the sign of `Lc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadabilityPolarityV1 {
    /// `Lc > 0`: darker foreground over a lighter backdrop (normal polarity).
    DarkOnLight,
    /// `Lc < 0`: lighter foreground over a darker backdrop (reverse polarity).
    LightOnDark,
    /// `Lc == 0`: the two luminances are within the curve's Δ floor, so no
    /// readable direction is claimed.
    Indistinct,
}

/// One readability measurement of a final occurrence: the signed candidate
/// `lc` and the symmetric `[1, 21]` WCAG `wcag` ratio, both drawn from the same
/// frozen display-domain SSOT `recheck_against` uses. Both are finite by
/// construction — the only inputs are three encoded-sRGB8 bytes over `[0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DisplayReadabilityMeasurementV1 {
    lc: f64,
    wcag: f64,
}

impl DisplayReadabilityMeasurementV1 {
    #[cfg(test)]
    pub(crate) const fn lc(&self) -> f64 {
        self.lc
    }

    pub(crate) const fn wcag(&self) -> f64 {
        self.wcag
    }

    /// Typed polarity from the sign of the signed candidate curve.
    pub(crate) fn polarity(&self) -> ReadabilityPolarityV1 {
        if self.lc > 0.0 {
            ReadabilityPolarityV1::DarkOnLight
        } else if self.lc < 0.0 {
            ReadabilityPolarityV1::LightOnDark
        } else {
            ReadabilityPolarityV1::Indistinct
        }
    }
}

/// Passing readability payload: a non-negative continuous `surplus` above the
/// role floor (for the controller's `dropFraction` hysteresis) and the typed
/// polarity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ReadabilityPassV1 {
    surplus: f64,
    polarity: ReadabilityPolarityV1,
}

impl ReadabilityPassV1 {
    /// Achieved WCAG ratio minus the role floor (`>= 0` on a pass).
    pub(crate) const fn surplus(&self) -> f64 {
        self.surplus
    }

    pub(crate) const fn polarity(&self) -> ReadabilityPolarityV1 {
        self.polarity
    }
}

/// Violating readability payload: the same continuous `surplus` (now negative —
/// the signed distance from the floor) and the typed polarity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ReadabilityViolationV1 {
    surplus: f64,
    polarity: ReadabilityPolarityV1,
}

impl ReadabilityViolationV1 {
    /// Achieved WCAG ratio minus the role floor (`< 0` on a violation).
    pub(crate) const fn surplus(&self) -> f64 {
        self.surplus
    }

    pub(crate) const fn polarity(&self) -> ReadabilityPolarityV1 {
        self.polarity
    }
}

/// The frozen display-domain readability evaluator + hard classifier. A unit
/// type: the law it applies is fixed, the only per-invocation datum is the role
/// [`Floor`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DisplayReadabilityCurveV1;

impl DisplayReadabilityCurveV1 {
    pub(crate) const IDENTITY: ReadabilityCurveIdentityV1 =
        ReadabilityCurveIdentityV1::FrozenDisplayLuminanceContrastV1;
}

impl private::EvaluatorSealed for DisplayReadabilityCurveV1 {}
impl private::HardClassifierSealed for DisplayReadabilityCurveV1 {}

impl Evaluator<ModeledSrgb8PointOccurrence> for DisplayReadabilityCurveV1 {
    type Invocation = Floor;
    type Identity = ReadabilityCurveIdentityV1;
    type Release = ReadabilityCurveReleaseV1;
    type Capability = ReadabilityCurveCapabilityV1;
    type Measurement = DisplayReadabilityMeasurementV1;
    type Error = Infallible;

    fn identity(&self) -> Self::Identity {
        Self::IDENTITY
    }

    fn release(&self) -> Self::Release {
        ReadabilityCurveReleaseV1::LegacyWcag21DisplayV1
    }

    fn capability(&self) -> Self::Capability {
        ReadabilityCurveCapabilityV1::FinalOccurrenceDisplayReadabilityV1
    }

    fn evaluate(
        &self,
        occurrence: &ModeledSrgb8PointOccurrence,
        _invocation: &Self::Invocation,
    ) -> Result<Self::Measurement, Self::Error> {
        // The final visible composite is the readability foreground; the
        // occurrence's backdrop is the readability background. Both are lifted
        // through the SAME `Srgb8::encoded` projection and read by the SAME
        // `crate::wcag::relative_luminance` SSOT the colour-only path uses, so a
        // solid (opaque) occurrence is byte-identical to `recheck_against`.
        let rl_fg = crate::wcag::relative_luminance(Srgb8::new(occurrence.visible()).encoded());
        let rl_bg = crate::wcag::relative_luminance(Srgb8::new(occurrence.backdrop()).encoded());
        Ok(DisplayReadabilityMeasurementV1 {
            lc: crate::lpc::contrast_core(rl_fg, rl_bg),
            wcag: crate::wcag::ratio_from_luminances(rl_fg, rl_bg),
        })
    }
}

impl HardClassifier<Floor, DisplayReadabilityMeasurementV1> for DisplayReadabilityCurveV1 {
    type Pass = ReadabilityPassV1;
    type Violation = ReadabilityViolationV1;

    fn classify(
        &self,
        invocation: &Floor,
        measurement: &DisplayReadabilityMeasurementV1,
    ) -> HardDecision<Self::Pass, Self::Violation> {
        let polarity = measurement.polarity();
        // A decorative role (`Floor::None`) has no normative minimum: the
        // physical floor is the identity ratio `1.0`, which every occurrence
        // clears, so surplus is measured against it and the decision is always
        // a pass.
        let min_ratio = invocation.min_ratio().unwrap_or(1.0);
        let surplus = measurement.wcag - min_ratio;
        if measurement.wcag >= min_ratio {
            HardDecision::Pass(ReadabilityPassV1 { surplus, polarity })
        } else {
            HardDecision::Violation(ReadabilityViolationV1 { surplus, polarity })
        }
    }
}
