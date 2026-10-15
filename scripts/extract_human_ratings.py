#!/usr/bin/env python3
"""Read published repeated ratings without pooling observers or word scales."""
import argparse
from collections import Counter, defaultdict
from dataclasses import asdict, dataclass
from fractions import Fraction
from functools import lru_cache
from hashlib import sha256
from itertools import combinations
from io import BytesIO
import json
from math import comb
from pathlib import Path
import re
from runpy import run_path
from statistics import StatisticsError, correlation, median
from zipfile import ZipFile

ROOT = Path(__file__).resolve().parents[1]
CAPSULE = ROOT / "proof/clean-set-srgb8/v2"
ARCHIVE = CAPSULE / "primary/colors-sato-input-identity-20261002/primary/original-sato-data.zip"
SOURCE_SHA256 = "22cdecae96e9fd2b1af1f09a4c4358cbacefb12cee82d7fa5374fcfe2ed8edad"
COHORTS = {"NTF": 10, "NTM": 13, "CVD": 13}
SCALES = {
    "cleanliness": ("kirei-kitanai", "clean", "dirty", "cleanness"),
    "freshness": ("shinsen-furukusai", "fresh", "stale", "freshness"),
    "hardness": ("yawarakai-katai", "soft", "hard", "hardness"),
    "preference": ("suki-kirai", "like", "dislike", "preference"),
    "warmth": ("tumetai-atatakai", "cool", "warm", "warmth"),
    "weight": ("karui-omoi", "light", "heavy", "weight"),
}
# Reuse the primary-byte parser. It is trusted repository code, never archive code.
parse_responses = run_path(str(CAPSULE / "audit_primary_inputs.py"))["parse_responses"]


@dataclass(frozen=True, order=True)
class Rating:
    scale: str
    cohort: str
    observer: int
    session: int
    stimulus: int
    centipoints: int


def require(condition, message):
    if not condition:
        raise ValueError(message)


def validate(ratings, cohorts=COHORTS, scales=SCALES, stimuli=range(32)):
    """The same observer key owns all scales, stimuli and repeated sessions."""
    ratings = tuple(ratings)
    actual = set()
    for row in ratings:
        require(type(row) is Rating, "rating type")
        require(type(row.scale) is str and type(row.cohort) is str, "label type")
        require(all(type(value) is int for value in
                    (row.observer, row.session, row.stimulus, row.centipoints)), "integer fields")
        require(0 <= row.centipoints <= 1000, "rating range")
        key = (row.scale, row.cohort, row.observer, row.session, row.stimulus)
        require(key not in actual, "duplicate observation")
        actual.add(key)
    expected = {(scale, cohort, observer, session, stimulus)
                for scale in scales for cohort, count in cohorts.items()
                for observer in range(1, count + 1) for session in (1, 2)
                for stimulus in stimuli}
    require(actual == expected and expected, "complete observer/session/stimulus census")
    return tuple(sorted(ratings))


def load(archive=ARCHIVE):
    payload = archive.read_bytes()
    require(sha256(payload).hexdigest() == SOURCE_SHA256, "published archive identity")
    ratings = []
    with ZipFile(BytesIO(payload)) as source:
        names = [name for name in source.namelist()
                 if name.startswith("dataset_ver2/affective/") and not name.endswith("/")]
        require(len(names) == len(set(names)) == 432, "affective file census")
        for name in sorted(names):
            match = re.fullmatch(
                r"dataset_ver2/affective/([a-z]+)/(CVD|NTF|NTM)/\2subject([0-9]{3})_([12])_([a-z]+)\.txt",
                name)
            require(match is not None, "response filename")
            scale, cohort, observer, session, suffix = match.groups()
            require(scale in SCALES and suffix == SCALES[scale][3], "word-scale identity")
            responses, _ = parse_responses(source.read(name))
            ratings.extend(Rating(scale, cohort, int(observer), int(session), sid, value)
                           for sid, value in responses.items())
    return validate(ratings)


def fraction(value):
    return {"numerator": value.numerator, "denominator": value.denominator}


def repeat_correlation(first, second):
    # A constant response has undefined correlation, not perfect agreement or zero.
    try:
        return correlation(first, second)
    except StatisticsError:
        return None


def ranks(values):
    """Average ranks retain ties; ranking happens within one observer/session."""
    positions = defaultdict(list)
    for rank, (_, index) in enumerate(sorted((v, i) for i, v in enumerate(values)), 1):
        positions[values[index]].append(rank)
    means = {value: Fraction(sum(group), len(group)) for value, group in positions.items()}
    return [means[value] for value in values]


def grouped(ratings):
    groups = defaultdict(dict)
    for row in ratings:
        key = (row.observer, row.session, row.stimulus)
        require(key not in groups[(row.scale, row.cohort)], "duplicate observation")
        groups[(row.scale, row.cohort)][key] = row.centipoints
    return sorted(groups.items())


def paired_counts(first, second):
    """Counts for matching observers. Positive difference means left is lower."""
    require(len(first) == len(second) and first, "paired observer census")
    averaged = Counter("left_lower" if a + b > 0 else "right_lower" if a + b < 0 else "tie"
                       for a, b in zip(first, second))
    repeats = Counter("opposite" if a * b < 0 else
                      "same_order" if a * b > 0 else
                      "both_tied" if a == b == 0 else "one_tied"
                      for a, b in zip(first, second))
    return {
        "observers": len(first),
        "two_session_mean_order": {key: averaged[key] for key in ("left_lower", "tie", "right_lower")},
        "repeat_order": {key: repeats[key] for key in ("same_order", "opposite", "both_tied", "one_tied")},
        "mean_right_minus_left": fraction(Fraction(sum(first) + sum(second), 200 * len(first))),
    }


def comparisons(ratings):
    rows = []
    for (scale, cohort), values in grouped(ratings):
        observers = sorted({key[0] for key in values})
        stimuli = sorted({key[2] for key in values})
        require(set(values) == {(p, s, c) for p in observers for s in (1, 2) for c in stimuli},
                "paired session completeness")
        for left, right in combinations(stimuli, 2):
            differences = [[values[p, session, right] - values[p, session, left]
                            for p in observers] for session in (1, 2)]
            rows.append({"scale": scale, "cohort": cohort, "left_stimulus": left,
                         "right_stimulus": right, **paired_counts(*differences)})
    return rows


def summarize(ratings):
    result = []
    for (scale, cohort), values in grouped(ratings):
        observers = sorted({key[0] for key in values})
        stimuli = sorted({key[2] for key in values})
        require(set(values) == {(p, s, c) for p in observers for s in (1, 2) for c in stimuli},
                "paired session completeness")
        individual = []
        for observer in observers:
            sessions = [[values[observer, s, c] for c in stimuli] for s in (1, 2)]
            individual.append({"observer": observer,
                               "spearman": repeat_correlation(*[ranks(s) for s in sessions]),
                               "mean_absolute_repeat_difference": fraction(Fraction(
                                   sum(abs(a - b) for a, b in zip(*sessions)), 100 * len(stimuli)))})
        correlations = [row["spearman"] for row in individual if row["spearman"] is not None]
        averages = [[Fraction(sum(values[p, s, c] for p in observers), len(observers))
                     for c in stimuli] for s in (1, 2)]
        result.append({
            "scale": scale, "cohort": cohort, "observers": len(observers),
            "stimuli": len(stimuli), "sessions_per_observer": 2, "ratings": len(values),
            "cohort_mean_repeat_spearman": repeat_correlation(*[ranks(s) for s in averages]),
            "individual_repeat_spearman": {
                "defined_observers": len(correlations),
                "minimum": min(correlations) if correlations else None,
                "median": median(correlations) if correlations else None,
                "maximum": max(correlations) if correlations else None,
            },
            "individuals": individual,
            "stimulus_means": [{"stimulus": c, "mean": fraction(Fraction(
                sum(values[p, s, c] for p in observers for s in (1, 2)), 200 * len(observers)))}
                for c in stimuli],
        })
    return result


# Шаг сетки ограничивает только наружное округление концов интервала.
# Он не задаёт порог человеческой оценки или статистический уровень доверия.
PROPORTION_GRID = 1 << 32


@lru_cache(maxsize=256)
def _binomial_lower(successes, trials, tail):
    if successes == 0:
        return Fraction(0)
    lo, hi = 0, PROPORTION_GRID
    denominator = PROPORTION_GRID ** trials
    while hi - lo > 1:
        midpoint = (lo + hi) // 2
        numerator = sum(comb(trials, k) * midpoint ** k *
                        (PROPORTION_GRID - midpoint) ** (trials - k)
                        for k in range(successes, trials + 1))
        if numerator * tail.denominator <= tail.numerator * denominator:
            lo = midpoint
        else:
            hi = midpoint
    return Fraction(lo, PROPORTION_GRID)


def proportion_interval(successes, trials, alpha=Fraction(1, 20)):
    """Наружная рациональная оболочка двустороннего интервала Clopper–Pearson."""
    require(type(successes) is int and type(trials) is int and
            0 <= successes <= trials and trials > 0, "binomial counts")
    require(type(alpha) is Fraction and 0 < alpha < 1, "binomial tail probability")
    # Симметрия бернуллиевских исходов задаёт верхний конец через нижний.
    lower = _binomial_lower(successes, trials, alpha / 2)
    upper = 1 - _binomial_lower(trials - successes, trials, alpha / 2)
    return lower, upper


def participant_support(ratings, cohorts=COHORTS, stimuli=range(32)):
    """Согласие людей по clean–dirty с сохранением двухсессионной единицы выборки."""
    ratings = tuple(ratings)
    require(all(type(row) is Rating for row in ratings), "rating type")
    clean = tuple(row for row in ratings if row.scale == "cleanliness")
    clean = validate(clean, cohorts, ("cleanliness",), stimuli)
    groups = grouped(clean)
    family_size = sum(len({key[2] for key in values}) for _, values in groups)
    alpha = Fraction(1, 20 * family_size)
    rows = []
    for (_, cohort), values in groups:
        observers = sorted({key[0] for key in values})
        for stimulus in sorted({key[2] for key in values}):
            paired = [(values[p, 1, stimulus], values[p, 2, stimulus]) for p in observers]
            totals = [first + second for first, second in paired]
            # 500 сотых балла — середина исходной шкалы. Равенство хранится
            # отдельно; порог является описательной политикой, а не выводом авторов.
            below = sum(total < 1000 for total in totals)
            equal = sum(total == 1000 for total in totals)
            lower, upper = proportion_interval(below, len(observers), alpha)
            rows.append({
                "cohort": cohort, "stimulus": stimulus, "observers": len(observers),
                "mean": fraction(Fraction(sum(totals), 200 * len(observers))),
                "below_midpoint": below, "at_midpoint": equal,
                "above_midpoint": len(observers) - below - equal,
                "both_sessions_below": sum(a < 500 and b < 500 for a, b in paired),
                "changed_midpoint_side": sum((a - 500) * (b - 500) < 0 for a, b in paired),
                "below_fraction": fraction(Fraction(below, len(observers))),
                "simultaneous_interval": {"lower": fraction(lower), "upper": fraction(upper)},
            })
    return {
        "policy": "two-session-mean-strictly-below-scale-midpoint",
        "scale": "cleanliness", "midpoint_centipoints": 500,
        "participants": len({(row.cohort, row.observer) for row in clean}),
        "responses": len(clean),
        "method": "clopper-pearson-bonferroni",
        "family_size": family_size, "familywise_alpha": fraction(Fraction(1, 20)),
        "per_cell_alpha": fraction(alpha),
        "endpoint_outward_error_at_most": fraction(Fraction(1, PROPORTION_GRID)),
        "sampling_assumption": "independent-identically-distributed-participants-within-cohort",
        "population_transfer": "not-established",
        "rows": rows,
    }


def document(mode="summary"):
    ratings = load()
    require(mode in ("summary", "ratings", "comparisons", "support"), "report mode")
    support = participant_support(ratings) if mode == "support" else None
    payload = (support["rows"] if support is not None else summarize(ratings) if mode == "summary"
               else comparisons(ratings) if mode == "comparisons" else [asdict(row) for row in ratings])
    return {
        **({"support_method": {key: value for key, value in support.items() if key != "rows"}}
           if support is not None else {}),
        "schema_version": 1,
        "source": {"doi": "10.7717/peerj.2751", "archive_sha256": SOURCE_SHA256,
                   "license": "CC-BY-4.0", "unit": "observer-within-cohort",
                   "raw_column_roles": "source-supported-assumption",
                   "stimulus_identity": "raw-study-id"},
        "coverage": {"studies": 1, "observers": len({(r.cohort, r.observer) for r in ratings}),
                     "observer_sessions": len({(r.cohort, r.observer, r.session) for r in ratings}),
                     "ratings": len(ratings)},
        "scales": {key: {"original_pair": row[0], "zero": row[1], "ten": row[2]}
                   for key, row in SCALES.items()},
        "interpretation": "descriptive-development-data",
        "mode": mode, "rows": payload,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=("summary", "ratings", "comparisons", "support"), default="summary")
    args = parser.parse_args()
    print(json.dumps(document(args.mode), ensure_ascii=False, allow_nan=False,
                     sort_keys=True, separators=(",", ":")))


if __name__ == "__main__":
    main()
