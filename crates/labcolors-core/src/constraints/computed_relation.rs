//! Сохранение авторских отношений силы контраста на общем фоне.
//!
//! CRstrong / CRweak сохраняет разрыв ln(CRstrong) − ln(CRweak). Решение
//! принимается непосредственно по multiplier, до диагностической проекции ln:
//! трансцендентное округление не создаёт дополнительного допуска. Это tonal
//! intent композиции, не модель salience, Cleanliness или нормативный WCAG pass.

use crate::Srgb8;
use crate::color_intent::reference_transport::{luminance, luminance_polarity};
use crate::spaces::srgb::relative_luminance_ratio;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoreComputedVisibleRelationInvocationV1 {
    PreserveTonalHierarchy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TonalHierarchyReferenceErrorV1 {
    TooFewOccurrences,
    ZeroPolarity,
    MixedPolarity,
    InvalidOrder,
    ResourceExhausted,
}

/// Положение видимого цвета относительно relative luminance фактического фона.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TonalPolarityV1 {
    /// Relative luminance видимого цвета меньше relative luminance фона.
    Darker,
    /// Relative luminance совпадают; endpoint не сохраняет tonal hierarchy.
    Equal,
    /// Relative luminance видимого цвета больше relative luminance фона.
    Lighter,
}

/// Измерение смежной пары в объявленном порядке сильнее → слабее.
/// Биты сохраняют direct-f64 multiplier, по которому принято решение.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TonalHierarchyPairMeasurementV1 {
    required_ratio_bits: u64,
    actual_ratio_bits: u64,
    stronger_polarity: TonalPolarityV1,
    weaker_polarity: TonalPolarityV1,
}

impl TonalHierarchyPairMeasurementV1 {
    /// Исходное отношение CRstrong / CRweak, строго больше единицы.
    #[must_use]
    pub const fn required_ratio(self) -> f64 {
        f64::from_bits(self.required_ratio_bits)
    }
    /// Конечное отношение CRstrong / CRweak на общем фактическом фоне.
    #[must_use]
    pub const fn actual_ratio(self) -> f64 {
        f64::from_bits(self.actual_ratio_bits)
    }
    /// Исходный разрыв силы в единицах натурального логарифма contrast ratio.
    /// Диагностическая проекция; решение сравнивает исходные multipliers.
    #[must_use]
    pub fn required_gap(self) -> f64 {
        self.required_ratio().ln()
    }
    /// Конечный разрыв силы в единицах натурального логарифма contrast ratio.
    /// Одинаковые округлённые log gaps ещё не означают равенства multipliers.
    #[must_use]
    pub fn actual_gap(self) -> f64 {
        self.actual_ratio().ln()
    }
    /// Сторона общего фона, на которой находится сильнейший endpoint пары.
    #[must_use]
    pub const fn stronger_polarity(self) -> TonalPolarityV1 {
        self.stronger_polarity
    }
    /// Сторона общего фона, на которой находится слабейший endpoint пары.
    #[must_use]
    pub const fn weaker_polarity(self) -> TonalPolarityV1 {
        self.weaker_polarity
    }
    /// Общая ненулевая polarity и сохранённый multiplier без смыслового допуска.
    #[must_use]
    pub fn satisfied(self) -> bool {
        self.stronger_polarity != TonalPolarityV1::Equal
            && self.stronger_polarity == self.weaker_polarity
            && self.actual_ratio() >= self.required_ratio()
    }
}

/// Конечные отношения силы исходной композиции, каждое строго больше единицы.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompiledTonalHierarchyV1 {
    ratios: Box<[f64]>,
}
// Srgb8 admission исключает NaN и бесконечность; равенство транзитивно.
impl Eq for CompiledTonalHierarchyV1 {}

impl CompiledTonalHierarchyV1 {
    pub(crate) fn compile(
        reference_visible: &[Srgb8],
        backdrop: Srgb8,
    ) -> Result<Self, TonalHierarchyReferenceErrorV1> {
        use TonalHierarchyReferenceErrorV1 as Error;
        if reference_visible.len() < 2 {
            return Err(Error::TooFewOccurrences);
        }
        let backdrop = luminance(backdrop);
        let first_polarity = luminance_polarity(luminance(reference_visible[0]), backdrop);
        let mut ratios = Vec::new();
        ratios
            .try_reserve_exact(reference_visible.len() - 1)
            .map_err(|_| Error::ResourceExhausted)?;
        let mut previous_visible = None;
        for &visible in reference_visible {
            let visible = luminance(visible);
            let current_polarity = luminance_polarity(visible, backdrop);
            if current_polarity == TonalPolarityV1::Equal {
                return Err(Error::ZeroPolarity);
            }
            if current_polarity != first_polarity {
                return Err(Error::MixedPolarity);
            }
            if let Some(previous) = previous_visible {
                let ratio = contrast_multiplier(previous, visible, backdrop);
                if ratio <= 1.0 {
                    return Err(Error::InvalidOrder);
                }
                ratios.push(ratio);
            }
            previous_visible = Some(visible);
        }
        Ok(Self {
            ratios: ratios.into_boxed_slice(),
        })
    }

    pub(crate) fn ratios(&self) -> &[f64] {
        &self.ratios
    }

    /// Смежные пары разделяют endpoints. Истинность всех пар доказывает общую
    /// ненулевую polarity всей группы, включая полный разворот относительно ref.
    pub(crate) fn measure_pair(
        &self,
        index: usize,
        stronger: Srgb8,
        weaker: Srgb8,
        backdrop: Srgb8,
    ) -> TonalHierarchyPairMeasurementV1 {
        let [stronger, weaker, backdrop] = [stronger, weaker, backdrop].map(luminance);
        let actual_ratio = contrast_multiplier(stronger, weaker, backdrop);
        TonalHierarchyPairMeasurementV1 {
            required_ratio_bits: self.ratios[index].to_bits(),
            actual_ratio_bits: actual_ratio.to_bits(),
            stronger_polarity: luminance_polarity(stronger, backdrop),
            weaker_polarity: luminance_polarity(weaker, backdrop),
        }
    }

    /// `false` исключает каждую точку входных relative-luminance интервалов.
    /// Независимые интервалы могут дать ложное `true`; финальное решение всегда
    /// измеряет encoded RGB тем же point kernel, включая общий backdrop.
    #[cfg(test)]
    fn pair_possible(
        &self,
        index: usize,
        stronger_y: [f64; 2],
        weaker_y: [f64; 2],
        backdrop_y: [f64; 2],
    ) -> bool {
        Self::pair_possible_ratio(self.ratios[index], stronger_y, weaker_y, backdrop_y)
    }

    pub(crate) fn pair_possible_ratio(
        required_ratio: f64,
        stronger_y: [f64; 2],
        weaker_y: [f64; 2],
        backdrop_y: [f64; 2],
    ) -> bool {
        if !required_ratio.is_finite()
            || required_ratio <= 1.0
            || [stronger_y, weaker_y, backdrop_y].iter().any(|range| {
                !range[0].is_finite()
                    || !range[1].is_finite()
                    || range[0] < 0.0
                    || range[1] > 1.0
                    || range[0] > range[1]
            })
        {
            // Неопределённая оценка никогда не доказывает отсутствие.
            return true;
        }
        let lighter_possible = stronger_y[1].min(weaker_y[1]) > backdrop_y[0]
            && multiplier_upper(stronger_y[1], weaker_y[0].max(backdrop_y[0])) >= required_ratio;
        let darker_possible = stronger_y[0].max(weaker_y[0]) < backdrop_y[1]
            && multiplier_upper(weaker_y[1].min(backdrop_y[1]), stronger_y[0]) >= required_ratio;
        lighter_possible || darker_possible
    }
}

/// A common backdrop cancels on either shared nonzero side. Preserve that
/// invariant before floating-point evaluation: two separate contrasts followed
/// by division can invent a one-ulp loss when only the background changes.
fn contrast_multiplier(stronger: f64, weaker: f64, backdrop: f64) -> f64 {
    let direction = luminance_polarity(stronger, backdrop);
    if direction == luminance_polarity(weaker, backdrop) {
        match direction {
            TonalPolarityV1::Darker => {
                return multiplier_upper(weaker, stronger);
            }
            TonalPolarityV1::Lighter => {
                return multiplier_upper(stronger, weaker);
            }
            TonalPolarityV1::Equal => return 1.0,
        }
    }
    // Mixed-side ratios remain useful diagnostic data, never a hierarchy pass.
    relative_luminance_ratio(stronger, backdrop) / relative_luminance_ratio(weaker, backdrop)
}

// Monotone positive arithmetic is identical in point and interval evaluation.
// The supplied numerator maximum / denominator minimum encloses the actual
// machine multiplier without a numerical or semantic threshold allowance.
fn multiplier_upper(numerator_y: f64, denominator_y: f64) -> f64 {
    (numerator_y + 0.05) / (denominator_y + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color_intent::reference_transport::luminance;
    fn gray(value: u8) -> Srgb8 {
        Srgb8::new([value; 3])
    }

    #[test]
    fn reference_requires_order_and_common_nonzero_polarity() {
        use TonalHierarchyReferenceErrorV1 as Error;
        for (reference, backdrop, expected) in [
            (vec![gray(32)], gray(255), Error::TooFewOccurrences),
            (vec![gray(32), gray(255)], gray(255), Error::ZeroPolarity),
            (vec![gray(0), gray(192)], gray(128), Error::MixedPolarity),
            (vec![gray(128), gray(64)], gray(255), Error::InvalidOrder),
            (vec![gray(128), gray(128)], gray(255), Error::InvalidOrder),
        ] {
            assert_eq!(
                CompiledTonalHierarchyV1::compile(&reference, backdrop),
                Err(expected)
            );
        }
    }

    #[test]
    fn hierarchy_uses_luminance_polarity_and_reference_contrast() {
        let blue = Srgb8::new([0, 0, 255]);
        let weaker = gray(78);
        let backdrop = gray(80);
        assert!(crate::color_intent::oklab(blue)[0] > crate::color_intent::oklab(backdrop)[0]);
        let hierarchy = CompiledTonalHierarchyV1::compile(&[blue, weaker], backdrop).expect(
            "blue and gray fixture colors share a darker polarity and valid strength order",
        );
        let pair = hierarchy.measure_pair(0, blue, weaker, backdrop);
        assert!(pair.satisfied());
        assert_eq!(pair.stronger_polarity(), TonalPolarityV1::Darker);
        assert_eq!(pair.weaker_polarity(), TonalPolarityV1::Darker);
        assert_eq!(
            pair.required_ratio(),
            (luminance(weaker) + 0.05) / (luminance(blue) + 0.05)
        );
        assert_eq!(pair.actual_gap(), pair.required_ratio().ln());
        assert_eq!(pair.actual_gap(), pair.required_gap());
    }

    #[test]
    fn authored_strength_survives_complete_polarity_flip() {
        let reference = [gray(0), gray(64), gray(128)];
        let hierarchy = CompiledTonalHierarchyV1::compile(&reference, gray(255))
            .expect("ordered gray reference hierarchy on white should compile");
        let reversed = [gray(255), gray(160), gray(80)];
        for index in 0..2 {
            let exact =
                hierarchy.measure_pair(index, reference[index], reference[index + 1], gray(255));
            assert_eq!(exact.actual_ratio(), exact.required_ratio());
            assert!(exact.satisfied());
            let flipped =
                hierarchy.measure_pair(index, reversed[index], reversed[index + 1], gray(0));
            assert!(flipped.satisfied());
            assert_eq!(flipped.stronger_polarity(), TonalPolarityV1::Lighter);
            assert_eq!(flipped.weaker_polarity(), TonalPolarityV1::Lighter);
        }
        assert!(
            !hierarchy
                .measure_pair(0, gray(0), gray(192), gray(128))
                .satisfied()
        );
        assert!(
            !hierarchy
                .measure_pair(0, gray(128), gray(0), gray(0))
                .satisfied()
        );
        assert!(
            !hierarchy
                .measure_pair(0, gray(192), gray(160), gray(0))
                .satisfied()
        );
    }

    #[test]
    fn quantized_point_decision_has_no_epsilon_at_inclusive_threshold() {
        let hierarchy = CompiledTonalHierarchyV1::compile(&[gray(255), gray(128)], gray(0))
            .expect("ordered white and gray reference hierarchy on black should compile");
        let exact = hierarchy.measure_pair(0, gray(255), gray(128), gray(0));
        let below = hierarchy.measure_pair(0, gray(254), gray(128), gray(0));
        assert!(exact.satisfied());
        assert_eq!(exact.required_ratio(), exact.actual_ratio());
        assert!(below.actual_ratio() < exact.required_ratio());
        assert!(!below.satisfied());
    }

    #[test]
    fn unchanged_same_side_pairs_are_invariant_under_every_gray_backdrop() {
        for (stronger, weaker) in [(0, 128), (16, 64), (32, 192), (127, 128)] {
            let hierarchy =
                CompiledTonalHierarchyV1::compile(&[gray(stronger), gray(weaker)], gray(255))
                    .expect("ordered darker-side gray reference pair should compile");
            for backdrop in weaker + 1..=255 {
                let pair = hierarchy.measure_pair(0, gray(stronger), gray(weaker), gray(backdrop));
                assert_eq!(pair.actual_ratio(), pair.required_ratio());
                assert!(pair.satisfied());
            }
            let hierarchy = CompiledTonalHierarchyV1::compile(
                &[gray(255 - stronger), gray(255 - weaker)],
                gray(0),
            )
            .expect("mirrored lighter-side gray reference pair should compile");
            for backdrop in 0..255 - weaker {
                let pair = hierarchy.measure_pair(
                    0,
                    gray(255 - stronger),
                    gray(255 - weaker),
                    gray(backdrop),
                );
                assert_eq!(pair.actual_ratio(), pair.required_ratio());
                assert!(pair.satisfied());
            }
        }
    }

    #[test]
    fn bounds_prune_impossible_multipliers_and_use_common_backdrop() {
        let hierarchy = CompiledTonalHierarchyV1::compile(&[gray(0), gray(128)], gray(255))
            .expect("black and mid-gray reference pair on white should compile");
        assert!(!hierarchy.pair_possible(0, [0.4, 0.5], [0.45, 0.55], [0.0, 1.0]));
        assert!(!hierarchy.pair_possible(0, [0.0, 0.1], [0.8, 0.9], [0.4, 0.6]));
        assert!(hierarchy.pair_possible(0, [0.0, 1.0], [0.0, 1.0], [0.0, 1.0]));
        assert!(hierarchy.pair_possible(0, [f64::NAN, 1.0], [0.0, 1.0], [0.0, 1.0]));
    }

    #[test]
    fn bounds_never_prune_a_valid_small_encoded_domain_tuple() {
        let values = [0, 64, 128, 192, 255].map(gray);
        let ys = values.map(luminance);
        let hierarchy = CompiledTonalHierarchyV1::compile(&[gray(0), gray(32)], gray(255))
            .expect("black and dark-gray reference pair on white should compile");
        let mut ranges = Vec::new();
        for lower in 0..values.len() {
            for upper in lower..values.len() {
                ranges.push((lower, upper, [ys[lower], ys[upper]]));
            }
        }
        for &(stronger_lo, stronger_hi, stronger) in &ranges {
            for &(weaker_lo, weaker_hi, weaker) in &ranges {
                for &(backdrop_lo, backdrop_hi, backdrop) in &ranges {
                    if hierarchy.pair_possible(0, stronger, weaker, backdrop) {
                        continue;
                    }
                    for &strong in &values[stronger_lo..=stronger_hi] {
                        for &weak in &values[weaker_lo..=weaker_hi] {
                            for &background in &values[backdrop_lo..=backdrop_hi] {
                                assert!(
                                    !hierarchy
                                        .measure_pair(0, strong, weak, background)
                                        .satisfied(),
                                    "pruned valid {strong:?}, {weak:?}, {background:?} in {stronger:?}, {weaker:?}, {backdrop:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn bounds_include_direct_f64_divisions_at_exact_reference_threshold() {
        let samples = [
            gray(0),
            gray(1),
            gray(32),
            gray(64),
            gray(128),
            gray(192),
            gray(254),
            gray(255),
            Srgb8::new([0, 0, 255]),
            Srgb8::new([255, 208, 0]),
            Srgb8::new([255, 128, 0]),
        ];
        for &backdrop in &samples {
            for &stronger in &samples {
                for &weaker in &samples {
                    let Ok(hierarchy) =
                        CompiledTonalHierarchyV1::compile(&[stronger, weaker], backdrop)
                    else {
                        continue;
                    };
                    for &runtime_backdrop in &samples {
                        if hierarchy
                            .measure_pair(0, stronger, weaker, runtime_backdrop)
                            .satisfied()
                        {
                            assert!(hierarchy.pair_possible(
                                0,
                                [luminance(stronger); 2],
                                [luminance(weaker); 2],
                                [luminance(runtime_backdrop); 2]
                            ));
                        }
                    }
                }
            }
        }
    }
}
