#!/usr/bin/env python3
"""Operator contract for the semantic verifier's rigorous interval math.

Every enclosure must contain the mathematical truth; the checks below pin
each transcendental inside a narrow rational window, so an enclosure that
misses the truth (or a remainder bound that stops too early) fails without
needing any external numeric library.
"""

from __future__ import annotations

import sys
import unittest
from fractions import Fraction
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from semantic import intervalmath  # noqa: E402

GUARD = 64
CAP = 64
SCALE = 10**20


def call(name: str, *arguments) -> intervalmath.Interval:
    return getattr(intervalmath, name)(
        *arguments, guard_bits=GUARD, cap_bits=CAP
    )


class EnclosureSoundnessTests(unittest.TestCase):
    def assert_encloses_truth(
        self,
        interval: intervalmath.Interval,
        truncated_20dp: int,
        *,
        negative: bool = False,
    ) -> None:
        # The window is the first 20 decimals of the mathematical truth, so
        # the truth lies strictly inside it.  A correct guard-64 enclosure is
        # at most a few ulp of 2^-64 wide, so it must cover the whole window;
        # a missed truth or an unsound remainder bound leaves an endpoint
        # inside the window and fails.
        lo = Fraction(truncated_20dp, SCALE)
        hi = Fraction(truncated_20dp + 1, SCALE)
        if negative:
            lo, hi = -hi, -lo
        self.assertLess(interval.lo, lo)
        self.assertGreater(interval.hi, hi)
        self.assertLess(interval.hi - interval.lo, Fraction(1, 1 << 48))

    def test_exp_one_contains_e(self) -> None:
        # e = 2.71828182845904523536...
        self.assert_encloses_truth(
            call("exp", intervalmath.exact(Fraction(1))),
            271828182845904523536,
        )

    def test_exp_negative_one(self) -> None:
        # exp(-1) = 0.36787944117144232159...
        self.assert_encloses_truth(
            call("exp", intervalmath.exact(Fraction(-1))),
            36787944117144232159,
        )

    def test_log_two(self) -> None:
        # ln 2 = 0.69314718055994530941...
        self.assert_encloses_truth(
            call("log", intervalmath.exact(Fraction(2))),
            69314718055994530941,
        )

    def test_sin_one(self) -> None:
        # sin 1 = 0.84147098480789650665...
        self.assert_encloses_truth(
            call("sin", intervalmath.exact(Fraction(1))),
            84147098480789650665,
        )

    def test_cos_one(self) -> None:
        # cos 1 = 0.54030230586813971740...
        self.assert_encloses_truth(
            call("cos", intervalmath.exact(Fraction(1))),
            54030230586813971740,
        )

    def test_sin_big_argument_reduces(self) -> None:
        # sin(1000/7) = -0.99636221069974350838...: exercises quadrant
        # reduction against the rational pi enclosure with a large argument.
        self.assert_encloses_truth(
            call("sin", intervalmath.exact(Fraction(1000, 7))),
            99636221069974350838,
            negative=True,
        )

    def test_sqrt_two(self) -> None:
        # sqrt 2 = 1.41421356237309504880...
        self.assert_encloses_truth(
            call("sqrt", intervalmath.exact(Fraction(2))),
            141421356237309504880,
        )

    def test_root3_five(self) -> None:
        # 5^(1/3) = 1.70997594667669698935...
        self.assert_encloses_truth(
            call("root3", intervalmath.exact(Fraction(5))),
            170997594667669698935,
        )

    def test_pow_pos(self) -> None:
        # (3/2)^(5/4) = 1.66002287955048238861...
        self.assert_encloses_truth(
            call(
                "pow_pos",
                intervalmath.exact(Fraction(3, 2)),
                intervalmath.exact(Fraction(5, 4)),
            ),
            166002287955048238861,
        )


class OperatorDomainContractTests(unittest.TestCase):
    def test_pow_nn_zero_base_needs_strictly_positive_exponent(self) -> None:
        zero = intervalmath.exact(0)
        crossing = intervalmath.Interval(Fraction(-1), Fraction(2))
        with self.assertRaises(intervalmath.UnresolvedError):
            call("pow_nn", zero, crossing)
        result = call("pow_nn", zero, intervalmath.exact(Fraction(2)))
        self.assertTrue(result.is_exact)
        self.assertEqual(result.lo, 0)

    def test_root3_rejects_negative_arguments(self) -> None:
        with self.assertRaises(intervalmath.UnresolvedError):
            call("root3", intervalmath.exact(Fraction(-8)))
        with self.assertRaises(intervalmath.UnresolvedError):
            call("root3", intervalmath.Interval(Fraction(-1), Fraction(1)))

    def test_log_rejects_nonpositive_arguments(self) -> None:
        with self.assertRaises(intervalmath.UnresolvedError):
            call("log", intervalmath.exact(Fraction(0)))
        with self.assertRaises(intervalmath.UnresolvedError):
            call("log", intervalmath.Interval(Fraction(-1), Fraction(1)))

    def test_sqrt_rejects_negative_arguments(self) -> None:
        with self.assertRaises(intervalmath.UnresolvedError):
            call("sqrt", intervalmath.exact(Fraction(-1)))

    def test_exp_rejects_arguments_beyond_reduction_range(self) -> None:
        huge = intervalmath.exact(Fraction(10) ** 400)
        with self.assertRaises(intervalmath.UnresolvedError):
            call("exp", huge)

    def test_sin_cos_reject_arguments_beyond_reduction_range(self) -> None:
        # A hostile binary64 argument must stay unresolved instead of blowing
        # up the quadrant sweep in proportion to its magnitude.
        huge = intervalmath.exact(Fraction(10) ** 400)
        with self.assertRaises(intervalmath.UnresolvedError):
            call("sin", huge)
        with self.assertRaises(intervalmath.UnresolvedError):
            call("cos", huge)
        negative_huge = intervalmath.exact(Fraction(-(10) ** 400))
        with self.assertRaises(intervalmath.UnresolvedError):
            call("sin", negative_huge)

    def test_div_rejects_zero_divisor(self) -> None:
        with self.assertRaises(intervalmath.UnresolvedError):
            intervalmath.div(
                intervalmath.exact(Fraction(1)),
                intervalmath.exact(Fraction(0)),
                cap_bits=CAP,
            )

    def test_sign_needs_a_strict_sign(self) -> None:
        with self.assertRaises(intervalmath.UnresolvedError):
            intervalmath.sign(intervalmath.Interval(Fraction(-1), Fraction(1)))
        positive = intervalmath.sign(intervalmath.exact(Fraction(3)))
        self.assertTrue(positive.is_exact)
        self.assertEqual(positive.lo, 1)

    def test_ratio0_zero_over_zero_is_exact_zero(self) -> None:
        result = intervalmath.ratio0(
            intervalmath.exact(0), intervalmath.exact(0), cap_bits=CAP
        )
        self.assertTrue(result.is_exact)
        self.assertEqual(result.lo, 0)

    def test_ratio0_needs_a_strictly_positive_divisor(self) -> None:
        with self.assertRaises(intervalmath.UnresolvedError):
            intervalmath.ratio0(
                intervalmath.exact(1), intervalmath.exact(0), cap_bits=CAP
            )
        with self.assertRaises(intervalmath.UnresolvedError):
            intervalmath.ratio0(
                intervalmath.exact(1),
                intervalmath.Interval(Fraction(-1), Fraction(1)),
                cap_bits=CAP,
            )
        half = intervalmath.ratio0(
            intervalmath.exact(1), intervalmath.exact(2), cap_bits=CAP
        )
        # The dyadic grid keeps the exact quotient unrounded.
        self.assertEqual(half.lo, Fraction(1, 2))
        self.assertEqual(half.hi, Fraction(1, 2))


if __name__ == "__main__":
    unittest.main()
