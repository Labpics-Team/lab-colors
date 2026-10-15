"""Published rows and hostile records exercise the same descriptive readout."""
from dataclasses import asdict, replace
from fractions import Fraction
from itertools import permutations, product
import json
from math import prod
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from extract_human_ratings import (
    ARCHIVE, CAPSULE, Rating, comparisons, document, load,
    paired_counts, ranks, repeat_correlation, summarize, validate,
    participant_support, proportion_interval, PROPORTION_GRID,
)


def fixture():
    return tuple(Rating("cleanliness", "group", p, s, c, value)
                 for p in (1, 2) for s in (1, 2)
                 for c, value in enumerate((100, 300, 800)))


def checked(rows):
    return validate(rows, {"group": 2}, ("cleanliness",), range(3))


class HumanRatingsTests(unittest.TestCase):
    def test_complete_observers_are_order_independent(self):
        rows = fixture()
        self.assertEqual(checked(rows), checked(reversed(rows)))
        baseline = summarize(rows)
        for ordering in permutations(range(3)):
            shuffled = sorted(rows, key=lambda row: (ordering[row.stimulus], -row.session, -row.observer))
            self.assertEqual(summarize(shuffled), baseline)
            self.assertEqual(comparisons(shuffled), comparisons(rows))
        self.assertEqual(baseline[0]["individual_repeat_spearman"]["median"], 1)
        nonlinear = tuple(
            replace(row, centipoints=(100, 200, 950)[row.stimulus])
            if row.session == 2 else row for row in rows
        )
        nonlinear_report = summarize(checked(nonlinear))[0]
        self.assertEqual(nonlinear_report["cohort_mean_repeat_spearman"], 1)
        self.assertEqual(nonlinear_report["individual_repeat_spearman"]["median"], 1)
        self.assertTrue(all(person["spearman"] == 1
                            for person in nonlinear_report["individuals"]))
        forwards = (100, 300, 800)
        backwards = tuple(reversed(forwards))
        contradictory = tuple(
            Rating("cleanliness", "group", observer, session, stimulus,
                   (forwards if observer == session else backwards)[stimulus])
            for observer in (1, 2) for session in (1, 2)
            for stimulus in range(3)
        )
        contradiction = summarize(checked(contradictory))[0]
        self.assertEqual(contradiction["cohort_mean_repeat_spearman"], 1)
        self.assertEqual(contradiction["individual_repeat_spearman"]["median"], -1)
        self.assertTrue(all(person["spearman"] == -1
                            for person in contradiction["individuals"]))

    def test_missing_repeated_or_foreign_records_refuse(self):
        rows = fixture()
        for invalid in (rows[:-1], rows + rows, (),
                        rows[:-1] + (replace(rows[-1], observer=3),),
                        rows[:-1] + (replace(rows[-1], scale="preference"),),
                        rows[:-1] + (replace(rows[-1], session=3),)):
            with self.subTest(invalid=invalid[-1:]), self.assertRaises(ValueError):
                checked(invalid)
        for name in ("observer", "session", "stimulus", "centipoints"):
            for value in (True, False, 1.0, "1", None):
                with self.subTest(name=name, value=value), self.assertRaises(ValueError):
                    checked(rows[:-1] + (replace(rows[-1], **{name: value}),))
        for value in (-1, 1001):
            with self.assertRaises(ValueError):
                checked(rows[:-1] + (replace(rows[-1], centipoints=value),))

    def test_average_hides_reversal_but_readout_retains_it(self):
        reversal = paired_counts([200, -200], [-200, 200])
        stable_tie = paired_counts([0, 0], [0, 0])
        self.assertEqual(reversal["mean_right_minus_left"], stable_tie["mean_right_minus_left"])
        self.assertEqual(reversal["two_session_mean_order"], stable_tie["two_session_mean_order"])
        self.assertEqual(reversal["repeat_order"]["opposite"], 2)
        self.assertEqual(stable_tie["repeat_order"]["both_tied"], 2)
        with self.assertRaises(ValueError):
            paired_counts([1, 2], [1])
        with self.assertRaises(ValueError):
            paired_counts([], [])

    def test_every_paired_observer_is_counted_once(self):
        result = paired_counts([300, -200, 0, 0], [100, -400, 0, 200])
        self.assertEqual(result["observers"], 4)
        self.assertEqual(result["two_session_mean_order"], {"left_lower": 2, "tie": 1, "right_lower": 1})
        self.assertEqual(result["repeat_order"], {"same_order": 2, "opposite": 0, "both_tied": 1, "one_tied": 1})
        self.assertEqual(result["mean_right_minus_left"], {"numerator": 0, "denominator": 1})
        reverse = paired_counts([-300, 200, 0, 0], [-100, 400, 0, -200])
        self.assertEqual(reverse["repeat_order"], result["repeat_order"])
        self.assertEqual(reverse["two_session_mean_order"]["right_lower"], 2)
        self.assertEqual(paired_counts([200, -100], [100, 200])["mean_right_minus_left"],
                         {"numerator": 1, "denominator": 1})

    def test_tied_ranks_and_constant_observers_stay_distinct(self):
        self.assertEqual(ranks([7, 1, 7, 3]), [Fraction(7, 2), 1, Fraction(7, 2), 2])
        self.assertIsNone(repeat_correlation([1, 1, 1], [1, 2, 3]))
        rows = tuple(replace(row, centipoints=500) for row in fixture())
        report = summarize(rows)[0]
        self.assertIsNone(report["cohort_mean_repeat_spearman"])
        self.assertEqual(report["individual_repeat_spearman"],
                         {"defined_observers": 0, "minimum": None, "median": None, "maximum": None})
        for reader in (summarize, comparisons):
            with self.assertRaises(ValueError):
                reader(fixture()[:-1])
            with self.assertRaises(ValueError):
                reader(fixture() * 2)

    def test_archive_substitution_refuses_before_decompression(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "ratings.zip"
            path.write_bytes(ARCHIVE.read_bytes() + b"\0")
            with self.assertRaisesRegex(ValueError, "published archive identity"):
                load(path)

    def test_all_published_scales_retain_their_observers_and_native_endpoints(self):
        report = document()
        self.assertEqual(report["coverage"], {"studies": 1, "observers": 36,
                                             "observer_sessions": 72, "ratings": 13824})
        self.assertEqual(len(report["rows"]), 18)
        self.assertEqual(report["scales"]["cleanliness"]["ten"], "dirty")
        self.assertEqual(report["scales"]["warmth"]["ten"], "warm")
        self.assertEqual(report["scales"]["preference"]["ten"], "dislike")
        reference = json.loads((CAPSULE / "primary/colors-sato-exact-witnesses-20261002/results/COHORT-MEANS.json").read_bytes())
        expected = {(row["CohortId"], row["StimulusId"]): row["mean"] for row in reference["rows"]}
        observed = {(group["cohort"], row["stimulus"]): row["mean"]
                    for group in report["rows"] if group["scale"] == "cleanliness"
                    for row in group["stimulus_means"]}
        self.assertEqual(observed, expected)
        pairs = comparisons(load())
        self.assertEqual(len(pairs), 6 * 3 * 496)
        for row in pairs:
            self.assertEqual(sum(row["two_session_mean_order"].values()), row["observers"])
            self.assertEqual(sum(row["repeat_order"].values()), row["observers"])
            if row["scale"] == "cleanliness":
                left, right = (expected[(row["cohort"], row[key])] for key in
                               ("left_stimulus", "right_stimulus"))
                difference = (Fraction(right["numerator"], right["denominator"]) -
                              Fraction(left["numerator"], left["denominator"]))
                self.assertEqual(row["mean_right_minus_left"],
                                 {"numerator": difference.numerator, "denominator": difference.denominator})
        compact = [{key: row[key] for key in ("cohort", "cohort_mean_repeat_spearman", "individual_repeat_spearman")}
                   for row in report["rows"] if row["scale"] == "cleanliness"]
        print("PUBLISHED_CLEANLINESS_REPEATABILITY " + json.dumps(compact, allow_nan=False))

    def test_cli_reproduces_the_same_source_and_native_rows_under_optimization(self):
        script = Path(__file__).with_name("extract_human_ratings.py")
        result = subprocess.run([sys.executable, "-I", "-OO", str(script), "--mode", "ratings"],
                                capture_output=True, text=True, check=True, timeout=30)
        report = json.loads(result.stdout)
        self.assertEqual(len(report["rows"]), 13824)
        self.assertEqual(report["coverage"]["observers"], 36)
        self.assertEqual(len({(r["scale"], r["cohort"], r["observer"], r["session"], r["stimulus"])
                              for r in report["rows"]}), len(report["rows"]))
        self.assertTrue(all(type(row["centipoints"]) is int
                            for row in report["rows"]))
        self.assertEqual(report["rows"], [asdict(row) for row in load()])
        self.assertEqual(report["source"], document()["source"])
        self.assertEqual(result.stderr, "")

        pair_process = subprocess.run(
            [sys.executable, "-I", "-OO", str(script), "--mode", "comparisons"],
            capture_output=True, text=True, check=True, timeout=30,
        )
        self.assertEqual(json.loads(pair_process.stdout), document("comparisons"))
        self.assertEqual(pair_process.stderr, "")

    def test_support_distinguishes_equal_means_and_preserves_each_person(self):
        unanimous = tuple(replace(row, centipoints=400) for row in fixture())
        divided = tuple(replace(row, centipoints=0 if row.observer == 1 else 800)
                        for row in fixture())
        self.assertEqual(summarize(unanimous), summarize(divided))
        good = participant_support(unanimous, {"group": 2}, range(3))
        split = participant_support(divided, {"group": 2}, range(3))
        self.assertEqual([row["below_midpoint"] for row in good["rows"]], [2, 2, 2])
        self.assertEqual([row["below_midpoint"] for row in split["rows"]], [1, 1, 1])
        self.assertEqual(split, participant_support(reversed(divided), {"group": 2}, range(3)))
        self.assertEqual(split["family_size"], 3)
        self.assertEqual((split["participants"], split["responses"]), (2, 12))
        self.assertEqual(split["per_cell_alpha"], {"numerator": 1, "denominator": 60})
        row = split["rows"][0]
        self.assertEqual(row["observers"], 2)
        self.assertEqual(row["below_fraction"], {"numerator": 1, "denominator": 2})
        self.assertEqual(row["simultaneous_interval"], {
            name: {"numerator": value.numerator, "denominator": value.denominator}
            for name, value in zip(("lower", "upper"), proportion_interval(1, 2, Fraction(1, 60)))
        })
        no_adjustment = proportion_interval(1, 2)
        adjusted = proportion_interval(1, 2, Fraction(1, 60))
        self.assertLess(adjusted[0], no_adjustment[0])
        self.assertGreater(adjusted[1], no_adjustment[1])

    def test_support_midpoint_equality_and_session_disagreement_stay_visible(self):
        rows = tuple(replace(row, centipoints=400 if row.observer == 1
                             else 0 if row.session == 1 else 1000) for row in fixture())
        for row in participant_support(rows, {"group": 2}, range(3))["rows"]:
            self.assertEqual(row["observers"], 2)
            self.assertEqual((row["below_midpoint"], row["at_midpoint"], row["above_midpoint"]), (1, 1, 0))
            self.assertEqual(row["both_sessions_below"], 1)
            self.assertEqual(row["changed_midpoint_side"], 1)
            self.assertEqual(row["mean"], {"numerator": 9, "denominator": 2})
        for partial in (rows[:-1], rows + rows, (), (None,),
                        tuple(row for row in rows if row.stimulus != 2),
                        tuple(row for row in rows if row.observer != 2)):
            with self.assertRaises(ValueError):
                participant_support(partial, {"group": 2}, range(3))
        with self.assertRaises(ValueError):
            participant_support(rows[:-1] + (replace(rows[-1], centipoints=True),), {"group": 2}, range(3))

    def test_binomial_intervals_enclose_analytic_and_published_references(self):
        step = Fraction(1, PROPORTION_GRID)
        lower, upper = proportion_interval(1, 1)
        self.assertLessEqual(lower, Fraction(1, 40))
        self.assertLess(Fraction(1, 40) - lower, step)
        self.assertEqual(upper, 1)
        self.assertEqual(proportion_interval(0, 1), (0, 1 - lower))
        # Независимые многочлены хвоста: n=2,k=1 → 2p-p²; n=3,k=2 → 3p²-2p³.
        for trials, successes, polynomial in [
            (2, 1, lambda p: 2 * p - p * p),
            (3, 2, lambda p: 3 * p * p - 2 * p * p * p),
        ]:
            lower, _ = proportion_interval(successes, trials)
            self.assertLessEqual(polynomial(lower), Fraction(1, 40))
            self.assertGreater(polynomial(lower + step), Fraction(1, 40))
        lower, upper = proportion_interval(13, 13)
        self.assertLessEqual(lower ** 13, Fraction(1, 40))
        self.assertGreater((lower + step) ** 13, Fraction(1, 40))
        self.assertLess(lower, Fraction(9, 10))
        self.assertEqual(upper, 1)
        # Опубликованный пример SciPy1.14.1, BinomTestResult.proportion_ci.
        lower, upper = proportion_interval(7, 50)
        self.assertAlmostEqual(float(lower), 0.05819170033997342, places=8)
        self.assertAlmostEqual(float(upper), 0.26739600249700846, places=8)

    def test_binomial_coverage_by_enumerating_individual_bernoulli_histories(self):
        for n in range(1, 7):
            for alpha in (Fraction(1, 20), Fraction(1, 100)):
                intervals = [proportion_interval(k, n, alpha) for k in range(n + 1)]
                for numerator in range(11):
                    p = Fraction(numerator, 10)
                    coverage = Fraction(0)
                    for history in product((0, 1), repeat=n):
                        lower, upper = intervals[sum(history)]
                        if lower <= p <= upper:
                            coverage += prod(p if bit else 1 - p for bit in history)
                    self.assertGreaterEqual(coverage, 1 - alpha)
                for k in range(n + 1):
                    lower, upper = intervals[k]
                    self.assertLessEqual(lower, Fraction(k, n))
                    self.assertGreaterEqual(upper, Fraction(k, n))
                    self.assertEqual((lower, upper), (1 - intervals[n-k][1], 1 - intervals[n-k][0]))
                    if k:
                        self.assertLessEqual(intervals[k-1][0], lower)
                        self.assertLessEqual(intervals[k-1][1], upper)
        for k, n, alpha in [(True, 2, Fraction(1, 20)), (1, False, Fraction(1, 20)),
                            (0, 0, Fraction(1, 20)), (-1, 3, Fraction(1, 20)),
                            (4, 3, Fraction(1, 20)), (1, 2, 0.05), (1, 2, Fraction(0)),
                            (1, 2, Fraction(1)), (1.0, 2, Fraction(1, 20))]:
            with self.assertRaises(ValueError):
                proportion_interval(k, n, alpha)

    def test_source_support_keeps_all_cells_and_matches_raw_participant_sums(self):
        ratings = load()
        report = document("support")
        self.assertEqual(report["support_method"]["family_size"], 96)
        self.assertEqual(report["support_method"]["participants"], 36)
        self.assertEqual(report["support_method"]["responses"], 2304)
        self.assertEqual(report["support_method"]["per_cell_alpha"], {"numerator": 1, "denominator": 1920})
        self.assertEqual(report["support_method"]["population_transfer"], "not-established")
        self.assertEqual(len(report["rows"]), 96)
        expected = {}
        for row in ratings:
            if row.scale == "cleanliness":
                key = (row.cohort, row.stimulus, row.observer)
                expected[key] = expected.get(key, 0) + Fraction(row.centipoints, 200)
        for row in report["rows"]:
            individuals = [value for (cohort, stimulus, _), value in expected.items()
                           if (cohort, stimulus) == (row["cohort"], row["stimulus"])]
            self.assertEqual(len(individuals), row["observers"])
            self.assertEqual(row["below_midpoint"], sum(value < 5 for value in individuals))
            self.assertEqual(row["at_midpoint"], sum(value == 5 for value in individuals))
            self.assertEqual(row["above_midpoint"], sum(value > 5 for value in individuals))
            mean = sum(individuals) / len(individuals)
            self.assertEqual(row["mean"], {"numerator": mean.numerator, "denominator": mean.denominator})
        summaries = []
        for cohort in ("NTF", "NTM", "CVD"):
            rows = [row for row in report["rows"] if row["cohort"] == cohort]
            below = [row for row in rows if Fraction(**row["mean"]) < 5]
            maximum = max(Fraction(row["observers"] - row["below_midpoint"], row["observers"]) for row in below)
            summaries.append({"cohort": cohort, "cells": len(rows), "mean_below_midpoint": len(below),
                              "unanimous_among_below_means": sum(row["below_midpoint"] == row["observers"] for row in below),
                              "max_not_below_share_among_below_means": str(maximum)})
        print("PUBLISHED_CLEANLINESS_SUPPORT " + json.dumps(summaries, sort_keys=True))

    def test_support_cli_under_optimization_exports_the_entire_family(self):
        script = Path(__file__).with_name("extract_human_ratings.py")
        command = [sys.executable, "-I", "-OO", str(script), "--mode", "support"]
        result = subprocess.run(command, capture_output=True, text=True, check=True, timeout=30)
        self.assertEqual(json.loads(result.stdout), document("support"))
        self.assertEqual(result.stderr, "")


if __name__ == "__main__":
    unittest.main()
