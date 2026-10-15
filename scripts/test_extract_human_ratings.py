"""Published rows and hostile records exercise the same descriptive readout."""
from dataclasses import asdict, replace
from fractions import Fraction
from itertools import permutations
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from extract_human_ratings import (
    ARCHIVE, CAPSULE, Rating, comparisons, document, load,
    paired_counts, ranks, repeat_correlation, summarize, validate,
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


if __name__ == "__main__":
    unittest.main()
