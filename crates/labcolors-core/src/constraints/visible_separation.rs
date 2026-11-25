//! Клиентские границы прямого f64-расстояния Oklab между final visible и backdrop.
//! Модель и коэффициенты закреплены в `spaces::oklab`; здесь нет JND,
//! readability или универсальной оценки качества цвета.

use core::convert::Infallible;

use crate::Srgb8;
use crate::constraints::{
    Evaluator, HardClassifier, HardDecision, ProgramConstraintContentV1,
    ProgramPointEvaluatorContentV1, ProgramPointTargetV1, private,
};

/// Включённый диапазон клиентской policy. Биты канонизуют ноль и позволяют
/// включить оба порога в точную identity декларации.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisibleSeparationBoundsV1 {
    minimum_bits: u64,
    maximum_bits: u64,
}

impl VisibleSeparationBoundsV1 {
    pub(crate) fn new(minimum: f64, maximum: f64) -> Option<Self> {
        if !minimum.is_finite()
            || !maximum.is_finite()
            || minimum < 0.0
            || minimum > maximum
            || maximum > 2.0
        {
            return None;
        }
        Some(Self {
            minimum_bits: if minimum == 0.0 { 0.0 } else { minimum }.to_bits(),
            maximum_bits: if maximum == 0.0 { 0.0 } else { maximum }.to_bits(),
        })
    }

    fn contains(self, [lower, upper]: [f64; 2]) -> bool {
        lower >= self.minimum() && upper <= self.maximum()
    }

    pub(crate) const fn minimum(self) -> f64 {
        f64::from_bits(self.minimum_bits)
    }

    pub(crate) const fn maximum(self) -> f64 {
        f64::from_bits(self.maximum_bits)
    }
}

/// Сохранённое измерение прямого Euclidean Oklab расстояния в масштабе L≈0..1.
/// Значения конечны и получены из двух окончательных encoded sRGB8 цветов.
/// Это геометрия объявленной модели, не порог человеческой различимости.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramVisibleSeparationMeasurementV1 {
    visible: Srgb8,
    backdrop: Srgb8,
    distance_bits: u64,
    distance_bounds_bits: [u64; 2],
    bounds: VisibleSeparationBoundsV1,
}

impl ProgramVisibleSeparationMeasurementV1 {
    pub(crate) fn satisfied(self) -> bool {
        self.bounds.contains(self.distance_bounds())
    }

    /// Два конечных цвета с независимыми допусками растеризации.
    pub(crate) fn between(
        left: Srgb8,
        right: Srgb8,
        radii: [u8; 2],
        bounds: VisibleSeparationBoundsV1,
    ) -> Self {
        let mut measurement = measure(left, right, bounds);
        if radii != [0, 0] {
            let range = crate::spaces::oklab::srgb8_boxes_distance_bounds(
                crate::emission::rgb_bounds(left.bytes(), radii[0]),
                crate::emission::rgb_bounds(right.bytes(), radii[1]),
            );
            measurement.distance_bounds_bits = range.map(f64::to_bits);
        }
        measurement
    }

    /// Конечные encoded цвета в порядке объявления сравнения.
    #[must_use]
    pub const fn colors(self) -> [Srgb8; 2] {
        [self.visible, self.backdrop]
    }

    /// Финальный encoded результат при сравнении с подложкой.
    #[must_use]
    pub const fn visible(self) -> Srgb8 {
        self.visible
    }

    /// Фактический encoded backdrop той же occurrence.
    #[must_use]
    pub const fn backdrop(self) -> Srgb8 {
        self.backdrop
    }

    /// Расстояние между reference-цветами.
    #[must_use]
    pub const fn distance(self) -> f64 {
        f64::from_bits(self.distance_bits)
    }

    /// Диапазон расстояний с учётом точности вывода материала. Для точечного
    /// reference-контракта обе границы равны `distance()`.
    #[must_use]
    pub const fn distance_bounds(self) -> [f64; 2] {
        [
            f64::from_bits(self.distance_bounds_bits[0]),
            f64::from_bits(self.distance_bounds_bits[1]),
        ]
    }

    /// Включённая нижняя граница клиента.
    #[must_use]
    pub const fn minimum(self) -> f64 {
        self.bounds.minimum()
    }

    /// Включённая верхняя граница клиента.
    #[must_use]
    pub const fn maximum(self) -> f64 {
        self.bounds.maximum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisibleSeparationV1;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisibleSeparationIdentityV1;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisibleSeparationReleaseV1;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisibleSeparationCapabilityV1;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisibleSeparationPassV1;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisibleSeparationViolationV1;

impl private::EvaluatorSealed for VisibleSeparationV1 {}
impl private::HardClassifierSealed for VisibleSeparationV1 {}

fn measure(
    visible: Srgb8,
    backdrop: Srgb8,
    bounds: VisibleSeparationBoundsV1,
) -> ProgramVisibleSeparationMeasurementV1 {
    use crate::spaces::{oklab::srgb_linear_to_oklab, srgb::srgb_linear_from_srgb8};
    let left = srgb_linear_to_oklab(srgb_linear_from_srgb8(visible));
    let right = srgb_linear_to_oklab(srgb_linear_from_srgb8(backdrop));
    let delta: [f64; 3] = std::array::from_fn(|index| left[index] - right[index]);
    let distance = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
    ProgramVisibleSeparationMeasurementV1 {
        visible,
        backdrop,
        distance_bits: distance.to_bits(),
        distance_bounds_bits: [distance.to_bits(); 2],
        bounds,
    }
}

impl Evaluator<ProgramPointTargetV1> for VisibleSeparationV1 {
    type Invocation = VisibleSeparationBoundsV1;
    type Identity = VisibleSeparationIdentityV1;
    type Release = VisibleSeparationReleaseV1;
    type Capability = VisibleSeparationCapabilityV1;
    type Measurement = ProgramVisibleSeparationMeasurementV1;
    type Error = Infallible;

    fn identity(&self) -> Self::Identity {
        VisibleSeparationIdentityV1
    }
    fn release(&self) -> Self::Release {
        VisibleSeparationReleaseV1
    }
    fn capability(&self) -> Self::Capability {
        VisibleSeparationCapabilityV1
    }

    fn evaluate(
        &self,
        target: &ProgramPointTargetV1,
        invocation: &Self::Invocation,
    ) -> Result<Self::Measurement, Self::Error> {
        let encoded = target.encoded();
        let emission = target.emission();
        let mut measurement = ProgramVisibleSeparationMeasurementV1::between(
            Srgb8::new(encoded.visible()),
            Srgb8::new(encoded.backdrop()),
            if emission.same_as_backdrop {
                [0; 2]
            } else {
                [emission.visible_radius, emission.backdrop_radius]
            },
            *invocation,
        );
        if emission.same_as_backdrop {
            measurement.distance_bounds_bits = [0.0_f64.to_bits(); 2];
        }
        Ok(measurement)
    }
}

impl ProgramPointEvaluatorContentV1 for VisibleSeparationV1 {
    fn program_constraint_content_v1(
        &self,
        bounds: VisibleSeparationBoundsV1,
    ) -> ProgramConstraintContentV1 {
        ProgramConstraintContentV1::VisibleSeparation {
            identity: VisibleSeparationIdentityV1,
            release: VisibleSeparationReleaseV1,
            capability: VisibleSeparationCapabilityV1,
            bounds,
        }
    }
}

impl HardClassifier<VisibleSeparationBoundsV1, ProgramVisibleSeparationMeasurementV1>
    for VisibleSeparationV1
{
    type Pass = VisibleSeparationPassV1;
    type Violation = VisibleSeparationViolationV1;

    fn classify(
        &self,
        invocation: &VisibleSeparationBoundsV1,
        measurement: &ProgramVisibleSeparationMeasurementV1,
    ) -> HardDecision<Self::Pass, Self::Violation> {
        // Both authored bounds must hold over the material's emission range.
        if invocation.contains(measurement.distance_bounds()) {
            HardDecision::Pass(VisibleSeparationPassV1)
        } else {
            HardDecision::Violation(VisibleSeparationViolationV1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_are_finite_ordered_and_zero_is_canonical() {
        for (minimum, maximum) in [
            (f64::NAN, 1.0),
            (0.0, f64::INFINITY),
            (-0.01, 1.0),
            (1.0, 0.0),
            (0.0, 2.000_001),
        ] {
            assert!(VisibleSeparationBoundsV1::new(minimum, maximum).is_none());
        }
        assert_eq!(
            VisibleSeparationBoundsV1::new(-0.0, -0.0),
            VisibleSeparationBoundsV1::new(0.0, 0.0)
        );
        assert!(VisibleSeparationBoundsV1::new(0.0, 2.0).is_some());
    }

    #[test]
    fn inclusive_thresholds_use_the_reported_distance_without_epsilon() -> Result<(), String> {
        let range = VisibleSeparationBoundsV1::new(0.0, 2.0).ok_or("valid range")?;
        let measured = measure(Srgb8::new([192; 3]), Srgb8::new([255; 3]), range);
        let distance = measured.distance();
        assert!((0.18..=0.21).contains(&distance));
        for (lower, upper, pass) in [
            (distance, distance, true),
            (0.0, f64::from_bits(distance.to_bits() - 1), false),
            (f64::from_bits(distance.to_bits() + 1), 2.0, false),
        ] {
            let bounds = VisibleSeparationBoundsV1::new(lower, upper).ok_or("valid bounds")?;
            assert_eq!(
                matches!(
                    VisibleSeparationV1.classify(&bounds, &measured),
                    HardDecision::Pass(_)
                ),
                pass
            );
        }
        let same = measure(Srgb8::new([128; 3]), Srgb8::new([128; 3]), range);
        assert_eq!(same.distance(), 0.0);
        Ok(())
    }
}
