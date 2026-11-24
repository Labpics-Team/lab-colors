//! One color model over interchangeable enclosing arithmetic. The refined
//! implementation deliberately preserves the original operation order.

use super::{
    CertifiedScalarV1, LMS_TO_OKLAB, NumericUncertaintyV1, SRGB_DECODE_SPLIT, SRGB_LINEAR_SCALE,
    SRGB_NONLINEAR_OFFSET, SRGB_NONLINEAR_SCALE, SRGB_TO_LMS, lift,
};

pub(super) enum SplitV1<T> {
    Below(T),
    Above(T),
    Crossing { below: T, above: T },
}

pub(super) trait EnclosingScalarV1: Copy {
    fn exact(value: f64) -> Result<Self, NumericUncertaintyV1>;
    fn add(self, rhs: Self) -> Result<Self, NumericUncertaintyV1>;
    fn mul(self, rhs: Self) -> Result<Self, NumericUncertaintyV1>;
    fn div(self, rhs: Self) -> Result<Self, NumericUncertaintyV1>;
    fn pow(self, power: u32) -> Result<Self, NumericUncertaintyV1>;
    fn root(self, degree: u32) -> Result<Self, NumericUncertaintyV1>;
    fn hull(self, rhs: Self) -> Self;
    fn split(self, at: f64) -> Result<SplitV1<Self>, NumericUncertaintyV1>;
}

impl EnclosingScalarV1 for CertifiedScalarV1 {
    fn exact(value: f64) -> Result<Self, NumericUncertaintyV1> {
        Self::exact(value)
    }
    fn add(self, rhs: Self) -> Result<Self, NumericUncertaintyV1> {
        self.add(rhs)
    }
    fn mul(self, rhs: Self) -> Result<Self, NumericUncertaintyV1> {
        self.mul(rhs)
    }
    fn div(self, rhs: Self) -> Result<Self, NumericUncertaintyV1> {
        self.div(rhs)
    }
    fn pow(self, power: u32) -> Result<Self, NumericUncertaintyV1> {
        self.pow(power)
    }
    fn root(self, degree: u32) -> Result<Self, NumericUncertaintyV1> {
        self.root(degree)
    }
    fn hull(self, rhs: Self) -> Self {
        self.hull(rhs)
    }
    fn split(self, at: f64) -> Result<SplitV1<Self>, NumericUncertaintyV1> {
        let at = lift(at)?;
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

pub(super) fn decode<S: EnclosingScalarV1>(channel: S) -> Result<S, NumericUncertaintyV1> {
    let x = channel.div(S::exact(255.0)?)?;
    let linear = |v: S| v.div(S::exact(SRGB_LINEAR_SCALE)?);
    let power = |v: S| {
        v.add(S::exact(SRGB_NONLINEAR_OFFSET)?)?
            .div(S::exact(SRGB_NONLINEAR_SCALE)?)?
            .pow(12)?
            .root(5)
    };
    match x.split(SRGB_DECODE_SPLIT)? {
        SplitV1::Below(value) => linear(value),
        SplitV1::Above(value) => power(value),
        // Literal coefficients have a small discontinuity. Both branches
        // participate at a crossing; continuity is not assumed.
        SplitV1::Crossing { below, above } => Ok(linear(below)?.hull(power(above)?)),
    }
}

pub(super) fn dot<S: EnclosingScalarV1>(
    coefficients: [f64; 3],
    value: [S; 3],
) -> Result<S, NumericUncertaintyV1> {
    S::exact(coefficients[0])?
        .mul(value[0])?
        .add(S::exact(coefficients[1])?.mul(value[1])?)?
        .add(S::exact(coefficients[2])?.mul(value[2])?)
}

pub(super) fn linear<S: EnclosingScalarV1>(
    encoded: [S; 3],
) -> Result<[S; 3], NumericUncertaintyV1> {
    Ok([
        decode(encoded[0])?,
        decode(encoded[1])?,
        decode(encoded[2])?,
    ])
}

pub(super) fn oklab<S: EnclosingScalarV1>(encoded: [S; 3]) -> Result<[S; 3], NumericUncertaintyV1> {
    oklab_from_linear(linear(encoded)?, |value| value.root(3))
}

pub(super) fn oklab_from_linear<S: EnclosingScalarV1>(
    linear: [S; 3],
    mut cube_root: impl FnMut(S) -> Result<S, NumericUncertaintyV1>,
) -> Result<[S; 3], NumericUncertaintyV1> {
    let lms = [
        cube_root(dot(SRGB_TO_LMS[0], linear)?)?,
        cube_root(dot(SRGB_TO_LMS[1], linear)?)?,
        cube_root(dot(SRGB_TO_LMS[2], linear)?)?,
    ];
    Ok([
        dot(LMS_TO_OKLAB[0], lms)?,
        dot(LMS_TO_OKLAB[1], lms)?,
        dot(LMS_TO_OKLAB[2], lms)?,
    ])
}
