//! Численная граница real-valued encoded-sRGB модели, не host-f64 исполнения.
//!
//! Входные binary64 RGB, effective alpha и коэффициенты Core имеют точное
//! dyadic значение. Gamma имеет точную степень 12/5 с canonical cutoff 0.04045;
//! Oklab идёт напрямую через sRGB → LMS. Одна формула исполняется через быстрые
//! наружные binary64-интервалы или уточняющую directed binary128 arithmetic.
//! Неоднозначный фильтр требует уточнения исходных входов. Совпадение binary64
//! cells не означает равенства вещественных значений и не доказывает отсутствие
//! primary drift; evidence требует отдельного доказательства целой cell.
//!
//! APFloat и его нестабильный API остаются внутри этой границы. Нормальный
//! расчёт не форматирует числа и не выделяет память; ошибки не дают verdict.

use rustc_apfloat::ieee::{Double, Quad};
use rustc_apfloat::{Float, FloatConvert, Round, Status, StatusAnd};

use crate::composition::AdmittedOpacityV1;
use crate::spaces::oklab::{LMS_TO_OKLAB, SRGB_TO_LMS};
use crate::spaces::srgb::{
    RELATIVE_LUMINANCE_BLUE_WEIGHT, RELATIVE_LUMINANCE_GREEN_WEIGHT,
    RELATIVE_LUMINANCE_RATIO_OFFSET, RELATIVE_LUMINANCE_RED_WEIGHT, SRGB_DECODE_SPLIT,
    SRGB_LINEAR_SCALE, SRGB_NONLINEAR_OFFSET, SRGB_NONLINEAR_SCALE,
};

mod model;
mod screening;

#[cfg(test)]
use model::decode;
use model::dot;

const DOWN: Round = Round::TowardNegative;
const UP: Round = Round::TowardPositive;
const NEAREST: Round = Round::NearestTiesToEven;
const ROOT_STEPS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CertifiedNumericModelV1 {
    RealEncodedSrgbDyadicCoefficientsRationalGammaV1,
}

pub(crate) const CERTIFIED_NUMERIC_MODEL_V1: CertifiedNumericModelV1 =
    CertifiedNumericModelV1::RealEncodedSrgbDyadicCoefficientsRationalGammaV1;

/// Ни один из отказов не разрешает подменить интервал ближайшим числом.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NumericUncertaintyV1 {
    NonFiniteInput,
    EncodedChannelOutsideDomain { channel: u8 },
    DomainViolation,
    DivisionByZero,
    InvalidOperation,
    Overflow,
    Underflow,
    PrecisionExhausted,
    UnresolvedRoundingCell,
    InsufficientScratchSpace,
}

/// IEEE binary128 endpoints и целая nearest-even binary64 cell.
///
/// Это приватное evidence представление модели V1: наружу не утекает APFloat,
/// а cell хранится отдельно от истинного охватывающего интервала.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CertifiedScalarEvidenceV1 {
    pub(crate) lower_bits: u128,
    pub(crate) upper_bits: u128,
    pub(crate) cell_bits: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CertifiedScalarV1 {
    lo: Quad,
    hi: Quad,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CertifiedRgbV1([CertifiedScalarV1; 3]);

/// Precision is part of memoized evidence. A refined request can never reuse
/// a screened enclosure, even when both share the same physical RGB key.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OklabPrecisionV1 {
    Screened,
    Refined,
}

#[derive(Clone, Copy)]
struct OklabCacheEntryV1 {
    rgb: CertifiedRgbV1,
    lab: [CertifiedScalarV1; 3],
    precision: OklabPrecisionV1,
}

/// Bounded memoization of the deterministic interval transform. Equal endpoint
/// bits are valid function-input keys; they are never real-expression equality
/// evidence. Two entries retain a source and its current visible appearance.
#[derive(Default)]
pub(crate) struct CertifiedOklabCacheV1 {
    entries: [Option<OklabCacheEntryV1>; 2],
    next: usize,
}

impl CertifiedOklabCacheV1 {
    /// Always returns the unchanged binary128 calculation. Upgrade a matching
    /// screened entry from its original encoded bounds, never from coarse Lab.
    pub(crate) fn evaluate(
        &mut self,
        rgb: CertifiedRgbV1,
    ) -> Result<[CertifiedScalarV1; 3], NumericUncertaintyV1> {
        self.evaluate_with_precision(rgb, OklabPrecisionV1::Refined)
    }

    /// Filtering enclosure only. A caller that cannot decide from its bounds
    /// must refine both operands, including region-pruning comparisons.
    pub(crate) fn evaluate_screened(
        &mut self,
        rgb: CertifiedRgbV1,
    ) -> Result<[CertifiedScalarV1; 3], NumericUncertaintyV1> {
        self.evaluate_with_precision(rgb, OklabPrecisionV1::Screened)
    }

    fn evaluate_with_precision(
        &mut self,
        rgb: CertifiedRgbV1,
        requested: OklabPrecisionV1,
    ) -> Result<[CertifiedScalarV1; 3], NumericUncertaintyV1> {
        let existing = self.entries.iter().position(|entry| {
            entry.as_ref().is_some_and(|entry| {
                entry
                    .rgb
                    .0
                    .iter()
                    .zip(rgb.0)
                    .all(|(&a, b)| a.bounds() == b.bounds())
            })
        });
        if let Some(index) = existing {
            let entry = self.entries[index].as_ref().unwrap();
            if requested == OklabPrecisionV1::Screened
                || entry.precision == OklabPrecisionV1::Refined
            {
                return Ok(entry.lab);
            }
        }
        let (lab, precision) = if requested == OklabPrecisionV1::Screened {
            match screening::oklab(rgb) {
                Ok(lab) => (lab, OklabPrecisionV1::Screened),
                Err(_) => (rgb.oklab()?, OklabPrecisionV1::Refined),
            }
        } else {
            (rgb.oklab()?, OklabPrecisionV1::Refined)
        };
        let index = existing.unwrap_or(self.next);
        self.entries[index] = Some(OklabCacheEntryV1 {
            rgb,
            lab,
            precision,
        });
        if existing.is_none() {
            self.next ^= 1;
        }
        Ok(lab)
    }
}

fn check_status(status: Status) -> Result<(), NumericUncertaintyV1> {
    if status.contains(Status::INVALID_OP) {
        Err(NumericUncertaintyV1::InvalidOperation)
    } else if status.contains(Status::DIV_BY_ZERO) {
        Err(NumericUncertaintyV1::DivisionByZero)
    } else if status.contains(Status::OVERFLOW) {
        Err(NumericUncertaintyV1::Overflow)
    } else if status.contains(Status::UNDERFLOW) {
        Err(NumericUncertaintyV1::Underflow)
    } else {
        // INEXACT is precisely the case covered by outward rounding.
        Ok(())
    }
}

fn checked(value: StatusAnd<Quad>) -> Result<Quad, NumericUncertaintyV1> {
    check_status(value.status)?;
    if !value.value.is_finite() {
        return Err(NumericUncertaintyV1::InvalidOperation);
    }
    // The mathematical real zero has a single identity, regardless of which
    // directed operation produced a signed machine zero.
    Ok(if value.value.is_zero() {
        Quad::ZERO
    } else {
        value.value
    })
}

fn lift(value: f64) -> Result<Quad, NumericUncertaintyV1> {
    if !value.is_finite() {
        return Err(NumericUncertaintyV1::NonFiniteInput);
    }
    let mut loses_info = false;
    let converted: StatusAnd<Quad> =
        Double::from_bits(u128::from(value.to_bits())).convert(&mut loses_info);
    let result = checked(converted)?;
    if loses_info || converted.status != Status::OK {
        return Err(NumericUncertaintyV1::PrecisionExhausted);
    }
    Ok(result)
}

fn to_f64(value: Quad, rounding: Round) -> Result<f64, NumericUncertaintyV1> {
    let converted: StatusAnd<Double> = value.convert_r(rounding, &mut false);
    check_status(converted.status)?;
    let result = f64::from_bits(converted.value.to_bits() as u64);
    if !result.is_finite() {
        return Err(NumericUncertaintyV1::Overflow);
    }
    Ok(if result == 0.0 { 0.0 } else { result })
}

impl CertifiedScalarV1 {
    pub(crate) fn exact(value: f64) -> Result<Self, NumericUncertaintyV1> {
        Ok(Self::point(lift(value)?))
    }

    fn point(value: Quad) -> Self {
        Self {
            lo: value,
            hi: value,
        }
    }

    pub(crate) fn add(self, rhs: Self) -> Result<Self, NumericUncertaintyV1> {
        Ok(Self {
            lo: checked(self.lo.add_r(rhs.lo, DOWN))?,
            hi: checked(self.hi.add_r(rhs.hi, UP))?,
        })
    }

    pub(crate) fn sub(self, rhs: Self) -> Result<Self, NumericUncertaintyV1> {
        Ok(Self {
            lo: checked(self.lo.sub_r(rhs.hi, DOWN))?,
            hi: checked(self.hi.sub_r(rhs.lo, UP))?,
        })
    }

    pub(crate) fn mul(self, rhs: Self) -> Result<Self, NumericUncertaintyV1> {
        if self.lo >= Quad::ZERO && rhs.lo >= Quad::ZERO {
            return Ok(Self {
                lo: checked(self.lo.mul_r(rhs.lo, DOWN))?,
                hi: checked(self.hi.mul_r(rhs.hi, UP))?,
            });
        }
        let mut lo = checked(self.lo.mul_r(rhs.lo, DOWN))?;
        let mut hi = checked(self.lo.mul_r(rhs.lo, UP))?;
        for (a, b) in [(self.lo, rhs.hi), (self.hi, rhs.lo), (self.hi, rhs.hi)] {
            lo = lo.min(checked(a.mul_r(b, DOWN))?);
            hi = hi.max(checked(a.mul_r(b, UP))?);
        }
        Ok(Self { lo, hi })
    }

    pub(crate) fn div(self, rhs: Self) -> Result<Self, NumericUncertaintyV1> {
        if rhs.lo <= Quad::ZERO && rhs.hi >= Quad::ZERO {
            return Err(NumericUncertaintyV1::DivisionByZero);
        }
        if self.lo >= Quad::ZERO && rhs.lo > Quad::ZERO {
            return Ok(Self {
                lo: checked(self.lo.div_r(rhs.hi, DOWN))?,
                hi: checked(self.hi.div_r(rhs.lo, UP))?,
            });
        }
        let mut lo = checked(self.lo.div_r(rhs.lo, DOWN))?;
        let mut hi = checked(self.lo.div_r(rhs.lo, UP))?;
        for (a, b) in [(self.lo, rhs.hi), (self.hi, rhs.lo), (self.hi, rhs.hi)] {
            lo = lo.min(checked(a.div_r(b, DOWN))?);
            hi = hi.max(checked(a.div_r(b, UP))?);
        }
        Ok(Self { lo, hi })
    }

    pub(crate) fn max(self, rhs: Self) -> Result<Self, NumericUncertaintyV1> {
        Ok(Self {
            lo: self.lo.max(rhs.lo),
            hi: self.hi.max(rhs.hi),
        })
    }

    fn min(self, rhs: Self) -> Self {
        Self {
            lo: self.lo.min(rhs.lo),
            hi: self.hi.min(rhs.hi),
        }
    }

    fn hull(self, rhs: Self) -> Self {
        Self {
            lo: self.lo.min(rhs.lo),
            hi: self.hi.max(rhs.hi),
        }
    }

    /// Integer powers preserve signed input and the dependence of x*x.
    pub(crate) fn pow(self, mut power: u32) -> Result<Self, NumericUncertaintyV1> {
        let mut value = Self::exact(1.0)?;
        let mut base = self;
        while power != 0 {
            if power & 1 != 0 {
                value = value.mul(base)?;
            }
            power >>= 1;
            if power != 0 {
                base = base.square()?;
            }
        }
        Ok(value)
    }

    fn square(self) -> Result<Self, NumericUncertaintyV1> {
        let a = checked(self.lo.mul_r(self.lo, DOWN))?;
        let b = checked(self.hi.mul_r(self.hi, DOWN))?;
        let lo = if self.lo <= Quad::ZERO && self.hi >= Quad::ZERO {
            Quad::ZERO
        } else {
            a.min(b)
        };
        let hi = checked(self.lo.mul_r(self.lo, UP))?.max(checked(self.hi.mul_r(self.hi, UP))?);
        Ok(Self { lo, hi })
    }

    fn root(self, degree: u32) -> Result<Self, NumericUncertaintyV1> {
        if self.lo < Quad::ZERO {
            return Err(NumericUncertaintyV1::DomainViolation);
        }
        let lower = root_point(self.lo, degree)?;
        if self.lo == self.hi {
            return Ok(lower);
        }
        let upper = root_point(self.hi, degree)?;
        Ok(Self {
            lo: lower.lo,
            hi: upper.hi,
        })
    }

    /// CR of nonnegative real relative-luminance coordinates.
    pub(crate) fn contrast(self, rhs: Self) -> Result<Self, NumericUncertaintyV1> {
        if self.lo < Quad::ZERO || rhs.lo < Quad::ZERO {
            return Err(NumericUncertaintyV1::DomainViolation);
        }
        let offset = Self::exact(RELATIVE_LUMINANCE_RATIO_OFFSET)?;
        self.max(rhs)?.add(offset)?.div(self.min(rhs).add(offset)?)
    }

    /// Symmetric multiplicative loss max(actual/target, target/actual).
    #[cfg(test)]
    pub(crate) fn ratio_loss(self, target: Self) -> Result<Self, NumericUncertaintyV1> {
        if self.lo <= Quad::ZERO || target.lo <= Quad::ZERO {
            return Err(NumericUncertaintyV1::DomainViolation);
        }
        let one = Self::exact(1.0)?;
        let ratio = self.div(target)?;
        let mut loss = ratio.max(one.div(ratio)?)?;
        if ratio.lo <= one.lo && ratio.hi >= one.hi {
            // max(r,1/r) >= 1 for every positive r, even when the interval
            // expression loses that shared dependence.
            loss.lo = one.lo;
        }
        Ok(loss)
    }

    pub(crate) fn strictly_below(self, rhs: Self) -> bool {
        self.hi < rhs.lo
    }

    /// Matching non-singleton enclosures do not prove real equality. This
    /// predicate permits a lexicographic comparison to skip only exact ties.
    pub(crate) fn proves_equal(self, rhs: Self) -> bool {
        self.lo == self.hi && rhs.lo == rhs.hi && self.lo == rhs.lo
    }

    /// Monotonic nearest-even conversion of both endpoints certifies that
    /// every enclosed real value belongs to this one cell, including ties.
    pub(crate) fn nearest_even_cell(self) -> Result<u64, NumericUncertaintyV1> {
        let lower = to_f64(self.lo, NEAREST)?.to_bits();
        let upper = to_f64(self.hi, NEAREST)?.to_bits();
        if lower == upper {
            Ok(lower)
        } else {
            Err(NumericUncertaintyV1::UnresolvedRoundingCell)
        }
    }

    pub(crate) fn evidence(self) -> Result<CertifiedScalarEvidenceV1, NumericUncertaintyV1> {
        let [lower_bits, upper_bits] = self.bounds();
        Ok(CertifiedScalarEvidenceV1 {
            lower_bits,
            upper_bits,
            cell_bits: self.nearest_even_cell()?,
        })
    }

    /// Raw enclosing bounds remain meaningful when no single cell is proven.
    pub(crate) fn bounds(self) -> [u128; 2] {
        [self.lo.to_bits(), self.hi.to_bits()]
    }

    /// Directed subtraction and directed storage keep this an upper bound
    /// after materialization as binary64. It may be negative for a proven gain.
    pub(crate) fn upper_difference(self, baseline: Self) -> Result<f64, NumericUncertaintyV1> {
        to_f64(checked(self.hi.sub_r(baseline.lo, UP))?, UP)
    }
}

/// One interval-Newton contraction for f(x) = x^degree - target. By the mean
/// value theorem, a root in `bracket` also lies in mid - f(mid)/f'(bracket).
/// All residual, derivative and correction bounds round outwards. Failure or
/// no contraction leaves the caller's proven bracket available for bisection.
/// See <https://octave.sourceforge.io/interval/package_doc/Root-Finding.html>.
fn root_newton_bracket(
    bracket: CertifiedScalarV1,
    mid: Quad,
    power: CertifiedScalarV1,
    target: Quad,
    degree: u32,
) -> Option<CertifiedScalarV1> {
    let derivative = bracket
        .pow(degree - 1)
        .ok()?
        .mul(CertifiedScalarV1::exact(f64::from(degree)).ok()?)
        .ok()?;
    let residual = power.sub(CertifiedScalarV1::point(target)).ok()?;
    let correction = CertifiedScalarV1::point(mid)
        .sub(residual.div(derivative).ok()?)
        .ok()?;
    let next = CertifiedScalarV1 {
        lo: bracket.lo.max(correction.lo),
        hi: bracket.hi.min(correction.hi),
    };
    (next.lo <= next.hi && (next.lo > bracket.lo || next.hi < bracket.hi)).then_some(next)
}

/// Positive root maintains lo^degree <= target <= hi^degree. Host powf/cbrt
/// only propose a tighter bracket; directed powers must independently admit it.
fn root_point(target: Quad, degree: u32) -> Result<CertifiedScalarV1, NumericUncertaintyV1> {
    if !matches!(degree, 3 | 5) || target < Quad::ZERO || !target.is_finite() {
        return Err(NumericUncertaintyV1::DomainViolation);
    }
    let one = lift(1.0)?;
    if target == Quad::ZERO || target == one {
        return Ok(CertifiedScalarV1::point(target));
    }
    let exponent = target.ilogb().div_euclid(degree as i32);
    // A finite Quad exponent divided by 3 or 5 keeps these exact powers of two
    // strictly within Quad's normal range. No unchecked numerical scaling is
    // performed on the user value itself.
    let mut lo = one.scalbn(exponent);
    let mut hi = one.scalbn(exponent + 1);

    // Subnormal binary64 conversion could report underflow. Since this is only
    // an optional hint, the exact exponent bracket handles that domain instead.
    if target >= lift(f64::MIN_POSITIVE)? && target <= lift(f64::MAX)? {
        let value = to_f64(target, NEAREST)?;
        let seed = if degree == 3 {
            value.cbrt()
        } else {
            value.powf(0.2)
        };
        if seed.is_finite() && seed > 0.0 {
            let guessed_lo = lift(f64::from_bits(seed.to_bits() - 1))?;
            let guessed_hi = lift(f64::from_bits(seed.to_bits() + 1))?;
            if CertifiedScalarV1::point(guessed_lo).pow(degree)?.hi <= target
                && CertifiedScalarV1::point(guessed_hi).pow(degree)?.lo >= target
            {
                lo = guessed_lo;
                hi = guessed_hi;
            }
        }
    }

    let half = lift(0.5)?;
    for _ in 0..ROOT_STEPS {
        let mid = checked(checked(lo.add_r(hi, NEAREST))?.mul_r(half, NEAREST))?;
        if mid == lo || mid == hi {
            return Ok(CertifiedScalarV1 { lo, hi });
        }
        let power = CertifiedScalarV1::point(mid).pow(degree)?;
        if power.lo == target && power.hi == target {
            return Ok(CertifiedScalarV1::point(mid));
        }
        if let Some(next) =
            root_newton_bracket(CertifiedScalarV1 { lo, hi }, mid, power, target, degree)
        {
            lo = next.lo;
            hi = next.hi;
        } else if power.hi < target {
            lo = mid;
        } else if power.lo > target {
            hi = mid;
        } else {
            // Ambiguous powers retain the enclosure, never imply equality.
            // The consumer can still prove a cell or strict interval ordering.
            return Ok(CertifiedScalarV1 { lo, hi });
        }
    }
    Err(NumericUncertaintyV1::PrecisionExhausted)
}

impl CertifiedRgbV1 {
    /// Exact equality of the encoded real colors, never equality of rounded
    /// output cells or matching non-singleton enclosures.
    pub(crate) fn proves_equal(self, rhs: Self) -> bool {
        self.0.iter().zip(rhs.0).all(|(&a, b)| a.proves_equal(b))
    }

    /// Admitted encoded channel box in byte units. The endpoints are exact
    /// binary64 inputs; this is an enclosure, never clamping or gamut mapping.
    pub(crate) fn from_encoded_bounds(
        lower: [f64; 3],
        upper: [f64; 3],
    ) -> Result<Self, NumericUncertaintyV1> {
        let lower = Self::from_encoded(lower)?;
        let upper = Self::from_encoded(upper)?;
        if lower.0.iter().zip(upper.0).any(|(lo, hi)| lo.lo > hi.hi) {
            return Err(NumericUncertaintyV1::DomainViolation);
        }
        Ok(Self(std::array::from_fn(|i| lower.0[i].hull(upper.0[i]))))
    }

    pub(crate) fn from_encoded(value: [f64; 3]) -> Result<Self, NumericUncertaintyV1> {
        for (channel, value) in value.iter().enumerate() {
            if !value.is_finite() {
                return Err(NumericUncertaintyV1::NonFiniteInput);
            }
            if !(0.0..=255.0).contains(value) {
                return Err(NumericUncertaintyV1::EncodedChannelOutsideDomain {
                    channel: channel as u8,
                });
            }
        }
        Ok(Self([
            CertifiedScalarV1::exact(value[0])?,
            CertifiedScalarV1::exact(value[1])?,
            CertifiedScalarV1::exact(value[2])?,
        ]))
    }

    pub(crate) fn source_over(
        self,
        effective_alpha: AdmittedOpacityV1,
        backdrop: Self,
    ) -> Result<Self, NumericUncertaintyV1> {
        let alpha = effective_alpha.value();
        if alpha == 0.0 {
            return Ok(backdrop);
        }
        if alpha == 1.0 {
            return Ok(self);
        }
        let alpha = CertifiedScalarV1::exact(alpha)?;
        let blend = |source: CertifiedScalarV1, backdrop: CertifiedScalarV1| {
            let result = backdrop.add(alpha.mul(source.sub(backdrop)?)?)?;
            // Admitted alpha proves convexity. Intersecting that known domain
            // tightens arithmetic enclosure; it never clamps an input point.
            let physical = source.hull(backdrop);
            Ok(CertifiedScalarV1 {
                lo: result.lo.max(physical.lo),
                hi: result.hi.min(physical.hi),
            })
        };
        Ok(Self([
            blend(self.0[0], backdrop.0[0])?,
            blend(self.0[1], backdrop.0[1])?,
            blend(self.0[2], backdrop.0[2])?,
        ]))
    }

    fn linear(self) -> Result<[CertifiedScalarV1; 3], NumericUncertaintyV1> {
        model::linear(self.0)
    }

    pub(crate) fn relative_luminance(self) -> Result<CertifiedScalarV1, NumericUncertaintyV1> {
        dot(
            [
                RELATIVE_LUMINANCE_RED_WEIGHT,
                RELATIVE_LUMINANCE_GREEN_WEIGHT,
                RELATIVE_LUMINANCE_BLUE_WEIGHT,
            ],
            self.linear()?,
        )
    }

    pub(crate) fn contrast(
        self,
        backdrop: Self,
    ) -> Result<CertifiedScalarV1, NumericUncertaintyV1> {
        self.relative_luminance()?
            .contrast(backdrop.relative_luminance()?)
    }

    pub(crate) fn oklab(self) -> Result<[CertifiedScalarV1; 3], NumericUncertaintyV1> {
        model::oklab(self.0)
    }
}

pub(crate) fn source_distance_squared(
    a: [CertifiedScalarV1; 3],
    b: [CertifiedScalarV1; 3],
) -> Result<CertifiedScalarV1, NumericUncertaintyV1> {
    a[0].sub(b[0])?
        .square()?
        .add(a[1].sub(b[1])?.square()?)?
        .add(a[2].sub(b[2])?.square()?)
}

/// The kth-largest real value lies between the kth-largest lower endpoint and
/// kth-largest upper endpoint. Sorting whole intervals by a rounded midpoint
/// would not establish those order-statistic bounds when intervals overlap.
pub(crate) fn sort_descending_profile(
    values: &mut [CertifiedScalarV1],
    scratch: &mut [CertifiedScalarV1],
) -> Result<(), NumericUncertaintyV1> {
    use core::cmp::Ordering;

    let Some(scratch) = scratch.get_mut(..values.len()) else {
        return Err(NumericUncertaintyV1::InsufficientScratchSpace);
    };
    let descending = |a: Quad, b: Quad| {
        if a > b {
            Ordering::Less
        } else if a < b {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    };
    scratch.copy_from_slice(values);
    scratch.sort_unstable_by(|a, b| descending(a.lo, b.lo));
    values.sort_unstable_by(|a, b| descending(a.hi, b.hi));
    for (value, lower) in values.iter_mut().zip(scratch) {
        value.lo = lower.lo;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
