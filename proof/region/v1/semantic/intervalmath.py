"""Rigorous interval arithmetic over dyadic-friendly Fractions.

Every enclosure keeps exact rational endpoints.  Widths arise only from
declared truncation and from Taylor remainder bounds with rational constants;
nothing in this module samples floats into results.  Transcendental
enclosures use argument reduction against rational enclosures of ln(2) and
pi, so every returned interval provably contains the mathematical value.
"""

from __future__ import annotations

from dataclasses import dataclass
from fractions import Fraction
from functools import lru_cache
from math import ceil, floor, isqrt


class UnresolvedError(Exception):
    """The requested conclusion needs more guard precision."""


@dataclass(frozen=True)
class Interval:
    lo: Fraction
    hi: Fraction

    def __post_init__(self) -> None:
        if type(self.lo) is not Fraction or type(self.hi) is not Fraction:
            raise TypeError("interval bounds must be exact Fractions")
        if self.lo > self.hi:
            raise ValueError("interval lower bound exceeds upper bound")

    @property
    def is_exact(self) -> bool:
        return self.lo == self.hi

    def contains_zero(self) -> bool:
        return self.lo <= 0 <= self.hi

    def strict_sign(self) -> int | None:
        """Return -1/0/+1 only when the interval proves that sign."""

        if self.lo > 0:
            return 1
        if self.hi < 0:
            return -1
        if self.lo == 0 and self.hi == 0:
            return 0
        return None


def exact(value: Fraction | int) -> Interval:
    if type(value) is int:
        value = Fraction(value)
    return Interval(value, value)


def _floor_to(value: Fraction, cap_bits: int) -> Fraction:
    scale = 1 << cap_bits
    numerator = value.numerator * scale
    floored = numerator // value.denominator
    return Fraction(floored, scale)


def _ceil_to(value: Fraction, cap_bits: int) -> Fraction:
    scale = 1 << cap_bits
    numerator = value.numerator * scale
    ceiled = -((-numerator) // value.denominator)
    return Fraction(ceiled, scale)


def outward(interval: Interval, cap_bits: int) -> Interval:
    """Widen endpoints onto a bounded denominator grid; never narrows."""

    return Interval(
        _floor_to(interval.lo, cap_bits),
        _ceil_to(interval.hi, cap_bits),
    )


def add(left: Interval, right: Interval) -> Interval:
    return Interval(left.lo + right.lo, left.hi + right.hi)


def sub(left: Interval, right: Interval) -> Interval:
    return Interval(left.lo - right.hi, left.hi - right.lo)


def neg(value: Interval) -> Interval:
    return Interval(-value.hi, -value.lo)


def mul(left: Interval, right: Interval) -> Interval:
    corners = (
        left.lo * right.lo,
        left.lo * right.hi,
        left.hi * right.lo,
        left.hi * right.hi,
    )
    return Interval(min(corners), max(corners))


def div(left: Interval, right: Interval, *, cap_bits: int) -> Interval:
    if right.contains_zero():
        raise UnresolvedError("divisor interval contains zero")
    corners = (
        left.lo / right.lo,
        left.lo / right.hi,
        left.hi / right.lo,
        left.hi / right.hi,
    )
    return outward(Interval(min(corners), max(corners)), cap_bits)


def minimum(left: Interval, right: Interval) -> Interval:
    return Interval(min(left.lo, right.lo), min(left.hi, right.hi))


def maximum(left: Interval, right: Interval) -> Interval:
    return Interval(max(left.lo, right.lo), max(left.hi, right.hi))


def absolute(value: Interval) -> Interval:
    if value.lo >= 0:
        return value
    if value.hi <= 0:
        return neg(value)
    return Interval(Fraction(0), max(-value.lo, value.hi))


def sign(value: Interval) -> Interval:
    resolved = value.strict_sign()
    if resolved is None:
        raise UnresolvedError("sign cannot be decided at this precision")
    return exact(resolved)


def _sqrt_bounds(value: Fraction, guard_bits: int) -> tuple[Fraction, Fraction]:
    """Rational lower/upper bounds of sqrt(value) via scaled integer roots."""

    if value == 0:
        return Fraction(0), Fraction(0)
    scale_power = max(guard_bits, 4)
    scaled = value.numerator * value.denominator << (2 * scale_power)
    root = isqrt(scaled)
    denominator = value.denominator << scale_power
    return Fraction(root, denominator), Fraction(root + 1, denominator)


def sqrt(value: Interval, *, guard_bits: int, cap_bits: int) -> Interval:
    if value.hi < 0:
        raise UnresolvedError("sqrt domain requires a nonnegative argument")
    if value.lo < 0:
        raise UnresolvedError("sqrt domain undecided: interval crosses zero")
    lo_lo, _ = _sqrt_bounds(value.lo, guard_bits)
    _, hi_hi = _sqrt_bounds(value.hi, guard_bits)
    return outward(Interval(lo_lo, hi_hi), cap_bits)


def _icbrt(value: int) -> int:
    if value < 0:
        return -_icbrt(-value)
    if value == 0:
        return 0
    root = 1 << ((value.bit_length() + 2) // 3)
    while True:
        step = (2 * root + value // (root * root)) // 3
        if step >= root:
            break
        root = step
    while root * root * root > value:
        root -= 1
    while (root + 1) ** 3 <= value:
        root += 1
    return root


def _root3_bounds(value: Fraction, guard_bits: int) -> tuple[Fraction, Fraction]:
    if value == 0:
        return Fraction(0), Fraction(0)
    scale_power = max(guard_bits, 4)
    scaled = value.numerator * value.denominator * value.denominator
    scaled <<= 3 * scale_power
    root = _icbrt(scaled)
    denominator = value.denominator << scale_power
    return Fraction(root, denominator), Fraction(root + 1, denominator)


def root3(value: Interval, *, guard_bits: int, cap_bits: int) -> Interval:
    if value.lo == value.hi == 0:
        return exact(0)
    lo_lo, _ = _root3_bounds(value.lo, guard_bits)
    _, hi_hi = _root3_bounds(value.hi, guard_bits)
    return outward(Interval(lo_lo, hi_hi), cap_bits)


def _series_coefficients(
    magnitude: Fraction,
    terms: int,
) -> tuple[int, int, tuple[int, ...]]:
    """Integer Horner data for an odd-power series with (2k+1) divisors.

    For |z| = p/q the partial sum over the first `terms` odd powers equals
    sign * p * P(p^2) / (q^(2 terms - 1) * product of odd divisors), where
    the integer polynomial P absorbs every denominator factor.
    """

    numerator = magnitude.numerator
    denominator = magnitude.denominator
    odd_factors = tuple(2 * index + 1 for index in range(terms))
    product = 1
    for factor in odd_factors:
        product *= factor
    coefficients = tuple(
        product // odd_factors[index]
        * denominator ** (2 * (terms - 1 - index))
        for index in range(terms)
    )
    return numerator, denominator ** (2 * terms - 1) * product, coefficients


def _series_partial_sum(z: Fraction, terms: int, odd_signs: bool) -> Fraction:
    """Exact partial sum of an odd-power series via one integer Horner pass.

    `odd_signs` selects the alternating series (atan) over the all-positive
    one (atanh).  Fractions never grow beyond the final common denominator,
    so no intermediate value explodes.
    """

    if z == 0 or terms < 1:
        return Fraction(0)
    sign = -1 if z < 0 else 1
    numerator, total_denominator, coefficients = _series_coefficients(abs(z), terms)
    if odd_signs:
        coefficients = tuple(
            coefficient if index % 2 == 0 else -coefficient
            for index, coefficient in enumerate(coefficients)
        )
    base = numerator * numerator
    total = 0
    for coefficient in reversed(coefficients):
        total = total * base + coefficient
    return Fraction(sign * numerator * total, total_denominator)


def _atanh_sum(z: Fraction, terms: int) -> Fraction:
    return _series_partial_sum(z, terms, odd_signs=False)


def _atanh_tail(z: Fraction, terms: int) -> Fraction:
    magnitude = abs(z)
    if magnitude == 0:
        return Fraction(0)
    square = magnitude * magnitude
    return magnitude ** (2 * terms + 1) / ((2 * terms + 1) * (1 - square))


def atanh_enclosure(z: Fraction, guard_bits: int) -> Interval:
    """Enclose atanh(z) for |z| < 1 with an adaptively bounded tail."""

    if not -1 < z < 1:
        raise UnresolvedError("atanh argument outside the open unit interval")
    target = Fraction(1, 1 << guard_bits)
    terms = 1
    while _atanh_tail(z, terms) > target:
        terms += 1
        if terms > 4 * guard_bits + 64:
            raise UnresolvedError("atanh series converges too slowly")
    total = _atanh_sum(z, terms)
    tail = _atanh_tail(z, terms)
    return Interval(total - tail, total + tail)


def _atan_sum(z: Fraction, terms: int) -> Fraction:
    return _series_partial_sum(z, terms, odd_signs=True)


def atan_enclosure(z: Fraction, guard_bits: int) -> Interval:
    """Enclose atan(z) for |z| <= 1 via its alternating series."""

    if not -1 <= z <= 1:
        raise UnresolvedError("atan argument outside the closed unit interval")
    target = Fraction(1, 1 << guard_bits)
    terms = 1
    magnitude = abs(z)
    while magnitude ** (2 * terms + 1) / (2 * terms + 1) > target:
        terms += 1
        if terms > 4 * guard_bits + 64:
            raise UnresolvedError("atan series converges too slowly")
    total = _atan_sum(z, terms)
    tail = magnitude ** (2 * terms + 1) / (2 * terms + 1)
    return Interval(total - tail, total + tail)


@lru_cache(maxsize=8)
def ln2_enclosure(guard_bits: int) -> Interval:
    """ln(2) = 2 atanh(1/3), enclosed with a rational tail bound."""

    base = atanh_enclosure(Fraction(1, 3), guard_bits)
    return Interval(2 * base.lo, 2 * base.hi)


@lru_cache(maxsize=8)
def pi_enclosure(guard_bits: int) -> Interval:
    """Machin formula pi = 16 atan(1/5) - 4 atan(1/239)."""

    first = atan_enclosure(Fraction(1, 5), guard_bits)
    second = atan_enclosure(Fraction(1, 239), guard_bits)
    lo = 16 * first.lo - 4 * second.hi
    hi = 16 * first.hi - 4 * second.lo
    return Interval(lo, hi)


def _exp_small(value: Interval, guard_bits: int) -> Interval:
    """Taylor enclosure of exp on an interval inside [-1, 1].

    The partial sum runs through the degree-`terms` monomial, so the
    Lagrange remainder is bounded by 3 * M^(terms + 1) / (terms + 1)!
    because exp(t) < 3 for |t| <= 1.  Interval terms accumulate with
    denominators capped by the reduced argument, and the adaptive term
    count stops as soon as the remainder bound clears the guard target
    instead of always iterating 64 terms.
    """

    if value.lo < -1 or value.hi > 1:
        raise UnresolvedError("exp reduction interval outside [-1, 1]")
    magnitude = max(abs(value.lo), abs(value.hi))
    target = Fraction(1, 1 << guard_bits)

    @lru_cache(maxsize=None)
    def radius(terms: int) -> Fraction:
        return (
            Fraction(
                3 * magnitude.numerator ** (terms + 1),
                magnitude.denominator ** (terms + 1),
            )
            / _factorial(terms + 1)
        )

    terms = 2
    while radius(terms) > target:
        terms += 1
        if terms > 4 * guard_bits + 64:
            raise UnresolvedError("exp series converges too slowly")
    total = exact(1)
    power = value
    factorial = 1
    for index in range(1, terms + 1):
        factorial *= index
        total = add(total, Interval(power.lo / factorial, power.hi / factorial))
        power = mul(power, value)
    remainder = radius(terms)
    return Interval(total.lo - remainder, total.hi + remainder)


def _factorial(terms: int) -> int:
    result = 1
    for factor in range(2, terms + 1):
        result *= factor
    return result


def exp(value: Interval, *, guard_bits: int, cap_bits: int) -> Interval:
    ln2 = ln2_enclosure(guard_bits)
    midpoint = (value.lo + value.hi) / 2
    # Integer reduction against the rational ln2 enclosure; the loop below
    # verifies the remainder bounds rigorously for any branch.
    branch = round(float(midpoint) / 0.6931471805599453)

    def reduced(candidate: int) -> Interval:
        if candidate >= 0:
            return Interval(
                value.lo - Fraction(candidate) * ln2.hi,
                value.hi - Fraction(candidate) * ln2.lo,
            )
        return Interval(
            value.lo - Fraction(candidate) * ln2.lo,
            value.hi - Fraction(candidate) * ln2.hi,
        )

    if value.hi - value.lo > 1:
        raise UnresolvedError("exp input too wide for one reduction branch")
    remainder = reduced(branch)
    while remainder.lo < -1 or remainder.hi > 1:
        branch += 1 if remainder.hi > 1 else -1
        remainder = reduced(branch)
    core = _exp_small(remainder, guard_bits)
    scale = Fraction(2) ** branch
    return outward(Interval(scale * core.lo, scale * core.hi), cap_bits)


def log(value: Interval, *, guard_bits: int, cap_bits: int) -> Interval:
    if value.hi <= 0:
        raise UnresolvedError("log domain requires a strictly positive argument")
    if value.lo <= 0:
        raise UnresolvedError("log domain undecided: interval touches zero")
    # Dyadic reduction keeps the mantissa inside [1/2, 2).
    branch = 0
    probe = value
    while probe.hi >= 2:
        branch += 1
        probe = Interval(probe.lo / 2, probe.hi / 2)
    while probe.lo < Fraction(1, 2):
        branch -= 1
        probe = Interval(probe.lo * 2, probe.hi * 2)
    # log(m) = 2 atanh((m - 1)/(m + 1)); the substitution is monotone.
    u_lo = (probe.lo - 1) / (probe.lo + 1)
    u_hi = (probe.hi - 1) / (probe.hi + 1)
    core = Interval(
        atanh_enclosure(u_lo, guard_bits).lo,
        atanh_enclosure(u_hi, guard_bits).hi,
    )
    ln2 = ln2_enclosure(guard_bits)
    if branch >= 0:
        shift = Interval(Fraction(branch) * ln2.lo, Fraction(branch) * ln2.hi)
    else:
        shift = Interval(Fraction(branch) * ln2.hi, Fraction(branch) * ln2.lo)
    result = add(shift, Interval(2 * core.lo, 2 * core.hi))
    return outward(result, cap_bits)


def _sin_small(value: Interval, guard_bits: int) -> Interval:
    """Taylor enclosure of sin on an interval inside [-1, 1].

    The alternating series tail is bounded by the first dropped term,
    M^(2 terms + 1) / (2 terms + 1)!, and the term count adapts to the
    guard target instead of a fixed 64-term sweep.
    """

    if value.lo < -1 or value.hi > 1:
        raise UnresolvedError("sin reduction interval outside [-1, 1]")
    magnitude = max(abs(value.lo), abs(value.hi))
    target = Fraction(1, 1 << guard_bits)

    @lru_cache(maxsize=None)
    def radius(terms: int) -> Fraction:
        return Fraction(
            magnitude.numerator ** (2 * terms + 1),
            magnitude.denominator ** (2 * terms + 1),
        ) / _factorial(2 * terms + 1)

    terms = 1
    while radius(terms) > target:
        terms += 1
        if terms > 4 * guard_bits + 64:
            raise UnresolvedError("sin series converges too slowly")
    total = exact(0)
    square = mul(value, value)
    power = value
    factorial = 1
    for index in range(terms):
        if index:
            factorial *= (2 * index) * (2 * index + 1)
        term = Interval(power.lo / factorial, power.hi / factorial)
        total = sub(total, term) if index % 2 else add(total, term)
        power = mul(power, square)
    remainder = radius(terms)
    return Interval(total.lo - remainder, total.hi + remainder)


def _cos_small(value: Interval, guard_bits: int) -> Interval:
    """Taylor enclosure of cos on an interval inside [-1, 1]."""

    if value.lo < -1 or value.hi > 1:
        raise UnresolvedError("cos reduction interval outside [-1, 1]")
    magnitude = max(abs(value.lo), abs(value.hi))
    target = Fraction(1, 1 << guard_bits)

    @lru_cache(maxsize=None)
    def radius(terms: int) -> Fraction:
        return Fraction(
            magnitude.numerator ** (2 * terms),
            magnitude.denominator ** (2 * terms),
        ) / _factorial(2 * terms)

    terms = 1
    while radius(terms) > target:
        terms += 1
        if terms > 4 * guard_bits + 64:
            raise UnresolvedError("cos series converges too slowly")
    total = exact(0)
    square = mul(value, value)
    power = exact(1)
    factorial = 1
    for index in range(terms):
        if index:
            factorial *= (2 * index - 1) * (2 * index)
        term = Interval(power.lo / factorial, power.hi / factorial)
        total = add(total, term) if index % 2 == 0 else sub(total, term)
        power = mul(power, square)
    remainder = radius(terms)
    return Interval(total.lo - remainder, total.hi + remainder)


def _reduce_quadrant(
    value: Interval,
    pi: Interval,
) -> list[tuple[int, Interval]]:
    """Return candidate (quadrant, remainder) pairs covering the interval."""

    half_pi = Interval(pi.lo / 2, pi.hi / 2)
    low = floor(value.lo / half_pi.hi) - 1
    high = ceil(value.hi / half_pi.lo) + 1
    candidates: list[tuple[int, Interval]] = []
    for branch in range(low, high + 1):
        if branch >= 0:
            product_lo = Fraction(branch) * half_pi.lo
            product_hi = Fraction(branch) * half_pi.hi
        else:
            product_lo = Fraction(branch) * half_pi.hi
            product_hi = Fraction(branch) * half_pi.lo
        remainder = Interval(value.lo - product_hi, value.hi - product_lo)
        if remainder.lo >= -1 and remainder.hi <= 1:
            candidates.append((branch, remainder))
    return candidates


def sin(value: Interval, *, guard_bits: int, cap_bits: int) -> Interval:
    pi = pi_enclosure(guard_bits)
    candidates = _reduce_quadrant(value, pi)
    lo = Fraction(1)
    hi = Fraction(-1)
    for branch, remainder in candidates:
        if remainder.lo < -1 or remainder.hi > 1:
            return outward(Interval(-1, 1), cap_bits)
        match branch % 4:
            case 0:
                part = _sin_small(remainder, guard_bits)
            case 1:
                part = _cos_small(remainder, guard_bits)
            case 2:
                part = neg(_sin_small(remainder, guard_bits))
            case _:
                part = neg(_cos_small(remainder, guard_bits))
        lo = min(lo, part.lo)
        hi = max(hi, part.hi)
    if lo > hi:
        raise UnresolvedError("sin reduction found no covering quadrant")
    return outward(Interval(lo, hi), cap_bits)


def cos(value: Interval, *, guard_bits: int, cap_bits: int) -> Interval:
    pi = pi_enclosure(guard_bits)
    candidates = _reduce_quadrant(value, pi)
    lo = Fraction(1)
    hi = Fraction(-1)
    for branch, remainder in candidates:
        if remainder.lo < -1 or remainder.hi > 1:
            return outward(Interval(-1, 1), cap_bits)
        match branch % 4:
            case 0:
                part = _cos_small(remainder, guard_bits)
            case 1:
                part = neg(_sin_small(remainder, guard_bits))
            case 2:
                part = neg(_cos_small(remainder, guard_bits))
            case _:
                part = _sin_small(remainder, guard_bits)
        lo = min(lo, part.lo)
        hi = max(hi, part.hi)
    if lo > hi:
        raise UnresolvedError("cos reduction found no covering quadrant")
    return outward(Interval(lo, hi), cap_bits)


def pow_pos(
    base: Interval,
    power_value: Interval,
    *,
    guard_bits: int,
    cap_bits: int,
) -> Interval:
    """x^y for strictly positive x, defined as exp(y log x)."""

    return exp(
        mul(power_value, log(base, guard_bits=guard_bits, cap_bits=cap_bits)),
        guard_bits=guard_bits,
        cap_bits=cap_bits,
    )


def pow_nn(
    base: Interval,
    power_value: Interval,
    *,
    guard_bits: int,
    cap_bits: int,
) -> Interval:
    """V1 pow_nn: zero base with strictly positive exponent is exact zero."""

    if base.is_exact and base.lo == 0:
        if power_value.hi <= 0:
            raise UnresolvedError("pow_nn exponent must be strictly positive")
        return exact(0)
    if base.contains_zero():
        raise UnresolvedError("pow_nn base undecided at zero")
    return pow_pos(base, power_value, guard_bits=guard_bits, cap_bits=cap_bits)


def ratio0(
    numerator: Interval,
    denominator: Interval,
    *,
    cap_bits: int,
) -> Interval:
    """V1 ratio0: 0/0 is exact zero; otherwise a strict positive divisor."""

    if numerator.is_exact and numerator.lo == 0 and denominator.is_exact and denominator.lo == 0:
        return exact(0)
    if denominator.lo <= 0:
        raise UnresolvedError("ratio0 divisor must be strictly positive")
    return div(numerator, denominator, cap_bits=cap_bits)
