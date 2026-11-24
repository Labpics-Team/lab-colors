//! Cheap outward binary64 enclosures for filtering. IEEE basic operations are
//! widened by one adjacent value; no hardware rounding-mode change is needed
//! on native or WASM. Transcendentals only seed independently verified brackets.
//! Failure is a request for the unchanged binary128 evaluator, never a verdict.

use rustc_apfloat::Float;

use super::model::{self, EnclosingScalarV1, SplitV1};
use super::{CertifiedRgbV1, CertifiedScalarV1, DOWN, NumericUncertaintyV1, UP, lift, to_f64};

const ROOT_BRACKET_STEPS: usize = 16;

#[derive(Clone, Copy, Debug)]
struct ScreenedScalarV1 {
    lo: f64,
    hi: f64,
}

fn next_down(value: f64) -> f64 {
    if value == 0.0 {
        return -f64::from_bits(1);
    }
    f64::from_bits(if value > 0.0 {
        value.to_bits() - 1
    } else {
        value.to_bits() + 1
    })
}

fn next_up(value: f64) -> f64 {
    if value == 0.0 {
        return f64::from_bits(1);
    }
    f64::from_bits(if value > 0.0 {
        value.to_bits() + 1
    } else {
        value.to_bits() - 1
    })
}

impl ScreenedScalarV1 {
    fn range(lo: f64, hi: f64) -> Result<Self, NumericUncertaintyV1> {
        if lo.is_finite() && hi.is_finite() && lo <= hi {
            Ok(Self { lo, hi })
        } else {
            Err(NumericUncertaintyV1::PrecisionExhausted)
        }
    }

    fn rounded(lo: f64, hi: f64) -> Result<Self, NumericUncertaintyV1> {
        let rounded = Self::range(lo, hi)?;
        Self::range(next_down(rounded.lo), next_up(rounded.hi))
    }

    fn is_zero(self) -> bool {
        self.lo == 0.0 && self.hi == 0.0
    }

    fn square(self) -> Result<Self, NumericUncertaintyV1> {
        if self.is_zero() {
            return Ok(self);
        }
        let a = self.lo * self.lo;
        let b = self.hi * self.hi;
        let lo = if self.lo <= 0.0 && self.hi >= 0.0 {
            0.0
        } else {
            next_down(a.min(b)).max(0.0)
        };
        Self::range(lo, next_up(a.max(b)))
    }

    fn certified(self) -> Result<CertifiedScalarV1, NumericUncertaintyV1> {
        Ok(CertifiedScalarV1 {
            lo: lift(self.lo)?,
            hi: lift(self.hi)?,
        })
    }
}

impl EnclosingScalarV1 for ScreenedScalarV1 {
    fn exact(value: f64) -> Result<Self, NumericUncertaintyV1> {
        Self::range(value, value)
    }

    fn add(self, rhs: Self) -> Result<Self, NumericUncertaintyV1> {
        if self.is_zero() {
            return Ok(rhs);
        }
        if rhs.is_zero() {
            return Ok(self);
        }
        let mut result = Self::rounded(self.lo + rhs.lo, self.hi + rhs.hi)?;
        if self.lo >= 0.0 && rhs.lo >= 0.0 {
            result.lo = result.lo.max(0.0);
        }
        Ok(result)
    }

    fn mul(self, rhs: Self) -> Result<Self, NumericUncertaintyV1> {
        if self.is_zero() || rhs.is_zero() {
            return Self::exact(0.0);
        }
        let products = [
            self.lo * rhs.lo,
            self.lo * rhs.hi,
            self.hi * rhs.lo,
            self.hi * rhs.hi,
        ];
        let lo = products.into_iter().fold(f64::INFINITY, f64::min);
        let hi = products.into_iter().fold(f64::NEG_INFINITY, f64::max);
        let mut result = Self::rounded(lo, hi)?;
        if self.lo >= 0.0 && rhs.lo >= 0.0 {
            result.lo = result.lo.max(0.0);
        }
        Ok(result)
    }

    fn div(self, rhs: Self) -> Result<Self, NumericUncertaintyV1> {
        if rhs.lo <= 0.0 && rhs.hi >= 0.0 {
            return Err(NumericUncertaintyV1::DivisionByZero);
        }
        if self.is_zero() {
            return Self::exact(0.0);
        }
        let quotients = [
            self.lo / rhs.lo,
            self.lo / rhs.hi,
            self.hi / rhs.lo,
            self.hi / rhs.hi,
        ];
        let mut result = Self::rounded(
            quotients.into_iter().fold(f64::INFINITY, f64::min),
            quotients.into_iter().fold(f64::NEG_INFINITY, f64::max),
        )?;
        if self.lo >= 0.0 && rhs.lo > 0.0 {
            result.lo = result.lo.max(0.0);
        }
        Ok(result)
    }

    fn pow(self, mut power: u32) -> Result<Self, NumericUncertaintyV1> {
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

    fn root(self, degree: u32) -> Result<Self, NumericUncertaintyV1> {
        if self.lo < 0.0 {
            return Err(NumericUncertaintyV1::DomainViolation);
        }
        let lower = root_point(self.lo, degree)?;
        if self.lo == self.hi {
            return Ok(lower);
        }
        let upper = root_point(self.hi, degree)?;
        Self::range(lower.lo, upper.hi)
    }

    fn hull(self, rhs: Self) -> Self {
        Self {
            lo: self.lo.min(rhs.lo),
            hi: self.hi.max(rhs.hi),
        }
    }

    fn split(self, at: f64) -> Result<SplitV1<Self>, NumericUncertaintyV1> {
        Ok(if self.hi <= at {
            SplitV1::Below(self)
        } else if self.lo > at {
            SplitV1::Above(self)
        } else {
            SplitV1::Crossing {
                below: Self {
                    lo: self.lo,
                    hi: at,
                },
                above: Self {
                    lo: at,
                    hi: self.hi,
                },
            }
        })
    }
}

fn root_point(target: f64, degree: u32) -> Result<ScreenedScalarV1, NumericUncertaintyV1> {
    if !matches!(degree, 3 | 5) || target < 0.0 || !target.is_finite() {
        return Err(NumericUncertaintyV1::DomainViolation);
    }
    if target == 0.0 || target == 1.0 {
        return ScreenedScalarV1::exact(target);
    }
    let seed = if degree == 3 {
        target.cbrt()
    } else {
        target.powf(0.2)
    };
    root_from_seed(target, degree, seed)
}

fn root_from_seed(
    target: f64,
    degree: u32,
    seed: f64,
) -> Result<ScreenedScalarV1, NumericUncertaintyV1> {
    if !seed.is_finite() || seed <= 0.0 {
        return Err(NumericUncertaintyV1::PrecisionExhausted);
    }
    let mut lo = next_down(seed);
    let mut hi = next_up(seed);
    for _ in 0..ROOT_BRACKET_STEPS {
        // Monotonic integer powers prove the bracket independently of libm.
        // An inaccurate seed can only cause more work or a Quad fallback.
        if ScreenedScalarV1::exact(lo)?.pow(degree)?.hi <= target
            && ScreenedScalarV1::exact(hi)?.pow(degree)?.lo >= target
        {
            return ScreenedScalarV1::range(lo, hi);
        }
        lo = next_down(lo);
        hi = next_up(hi);
        if lo < 0.0 {
            break;
        }
    }
    Err(NumericUncertaintyV1::PrecisionExhausted)
}

pub(super) fn oklab(rgb: CertifiedRgbV1) -> Result<[CertifiedScalarV1; 3], NumericUncertaintyV1> {
    let mut encoded = [ScreenedScalarV1::exact(0.0)?; 3];
    for (value, source) in encoded.iter_mut().zip(rgb.0) {
        *value = ScreenedScalarV1::range(to_f64(source.lo, DOWN)?, to_f64(source.hi, UP)?)?;
    }
    let lab = model::oklab(encoded)?;
    Ok([
        lab[0].certified()?,
        lab[1].certified()?,
        lab[2].certified()?,
    ])
}

/// Show that the natural interval extension used by Refined cannot have a
/// large lower loss. Each rooted LMS interval includes the values at both
/// encoded corners. The final interval dot product admits any independent
/// combination of these three coordinates, even a physically unrealizable one.
/// This witness may only disprove pruning; it never selects a paint tuple.
pub(super) fn region_loss_witness(
    rgb: CertifiedRgbV1,
    preferred: [CertifiedScalarV1; 3],
) -> Result<CertifiedScalarV1, NumericUncertaintyV1> {
    if rgb.proves_equal(rgb) {
        // Both corners are the same exact input.
        return super::source_distance_squared(oklab(rgb)?, preferred);
    }
    let lms_corner = |upper| {
        let mut encoded = [ScreenedScalarV1::exact(0.0)?; 3];
        for (channel, range) in encoded.iter_mut().zip(rgb.0) {
            let end = if upper { range.hi } else { range.lo };
            *channel = ScreenedScalarV1::range(to_f64(end, DOWN)?, to_f64(end, UP)?)?;
        }
        model::rooted_lms(model::linear(encoded)?, |value| value.root(3))
    };
    let lower = lms_corner(false)?;
    let upper = lms_corner(true)?;
    let mut witness = preferred;
    for (axis, coefficients) in super::LMS_TO_OKLAB.into_iter().enumerate() {
        let corner = |minimize| {
            let coordinates = std::array::from_fn(|i| {
                if (coefficients[i] >= 0.0) == minimize {
                    lower[i]
                } else {
                    upper[i]
                }
            });
            model::dot(coefficients, coordinates)
        };
        let low = corner(true)?;
        let high = corner(false)?;
        if low.hi <= high.lo {
            let nearest = preferred[axis].lo.max(lift(low.hi)?).min(lift(high.lo)?);
            witness[axis] = CertifiedScalarV1::point(nearest);
        } else {
            // The two corner enclosures overlap (including singleton axes).
            // low still encloses one actual value of the refined coordinate.
            witness[axis] = low.certified()?;
        }
    }
    super::source_distance_squared(witness, preferred)
}

#[cfg(test)]
mod tests;
