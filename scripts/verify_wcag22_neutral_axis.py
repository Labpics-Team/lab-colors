#!/usr/bin/env python3
"""Независимый exact oracle для WCAG 2.2 neutral-axis fixtures из #295.

Oracle читает только канонический нормативный профиль WCAG 2.2. Он не
импортирует production generator, Q55-таблицы и не вызывает Rust evaluator.
Вся арифметика выполняется над ``Fraction``: нелинейная ветвь ``12/5``
доказывается адаптивными рациональными границами корня пятой степени.

Без аргументов verifier проверяет каноничность, SHA-256 provenance,
anti-vacuum self-tests и заново вычисляет каждый fixture. ``--emit`` нужен
только при осознанном создании новой версии committed artifact.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from fractions import Fraction
from pathlib import Path
from typing import Any, Iterable


REPO_ROOT = Path(__file__).resolve().parents[1]
PROFILE_PATH = REPO_ROOT / "crates/labcolors-core/contracts/wcag22-srgb8-v1.json"
FIXTURE_PATH = (
    REPO_ROOT
    / "crates/labcolors-core/contracts/wcag22-neutral-axis-oracle-v1.json"
)
ORACLE_PATH = Path(__file__).resolve()

ORACLE_ID = "wcag22-srgb8-neutral-axis-fraction-oracle-v1"
DOMAIN_ID = "srgb8-neutral-axis-complete-v1"
FIXTURE_ID = "wcag22-srgb8-neutral-axis-fixtures-v1"
CANDIDATE_DIGEST_DOMAIN = b"labcolors.wcag22-neutral-axis-candidates.v1\0"
PAYLOAD_DIGEST_LAW = "canonical-json-lf-without-payload-digest-v1"

CRITERION_RATIO_FIELDS = {
    "sc-1.4.3-text-default": "normalTextRatio",
    "sc-1.4.3-text-large-scale": "largeTextRatio",
    "sc-1.4.11-ui-component-or-state": "requiredNonTextRatio",
    "sc-1.4.11-graphical-object": "requiredNonTextRatio",
}
# Это fail-closed budget, а не численный допуск и не часть verdict. Его
# происхождение структурно: один refinement на каждую пару полного 8-bit
# neutral domain и каждого зарегистрированного criterion. Committed fixture
# отдельно фиксирует реально потребовавшийся максимум, поэтому приближение к
# budget видно задолго до отказа.
MAX_ROOT_REFINEMENTS = (1 << 8) * len(CRITERION_RATIO_FIELDS)
THREE_TO_ONE_CRITERIA = (
    "sc-1.4.3-text-large-scale",
    "sc-1.4.11-ui-component-or-state",
    "sc-1.4.11-graphical-object",
)

# Эти пять множеств — независимые committed expectations. ``--emit`` не может
# молча принять иной ответ алгоритма: сначала exact evaluation обязана совпасть
# с указанными диапазонами.
FIXTURE_SPECS = (
    {
        "id": "normal-text-vs-767676",
        "criteria": ("sc-1.4.3-text-default",),
        "adjacent_codes": (0x76,),
        "expected_ranges": ((0x00, 0x04), (0xFE, 0xFF)),
    },
    {
        "id": "normal-text-vs-black-white",
        "criteria": ("sc-1.4.3-text-default",),
        "adjacent_codes": (0x00, 0xFF),
        "expected_ranges": ((0x75, 0x76),),
    },
    {
        "id": "normal-text-vs-black-white-767676",
        "criteria": ("sc-1.4.3-text-default",),
        "adjacent_codes": (0x00, 0x76, 0xFF),
        "expected_ranges": (),
    },
    {
        "id": "three-to-one-vs-767676",
        "criteria": THREE_TO_ONE_CRITERIA,
        "adjacent_codes": (0x76,),
        "expected_ranges": ((0x00, 0x2D), (0xD2, 0xFF)),
    },
    {
        "id": "three-to-one-vs-black-white",
        "criteria": THREE_TO_ONE_CRITERIA,
        "adjacent_codes": (0x00, 0xFF),
        "expected_ranges": ((0x5A, 0x94),),
    },
)


def reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def reject_non_json_constant(value: str) -> None:
    raise ValueError(f"non-JSON numeric constant: {value}")


def parse_json(raw: bytes, label: str) -> dict[str, Any]:
    try:
        value = json.loads(
            raw,
            object_pairs_hook=reject_duplicate_keys,
            parse_constant=reject_non_json_constant,
        )
    except (UnicodeDecodeError, json.JSONDecodeError, ValueError) as error:
        raise ValueError(f"{label} is not strict JSON: {error}") from error
    if not isinstance(value, dict):
        raise TypeError(f"{label} root must be an object")
    return value


def canonical_json_bytes(value: dict[str, Any]) -> bytes:
    return (
        json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
        + "\n"
    ).encode("utf-8")


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def exact_decimal(profile: dict[str, Any], key: str) -> Fraction:
    value = profile.get(key)
    if not isinstance(value, str):
        raise TypeError(f"profile field {key} must be an exact decimal string")
    try:
        return Fraction(value)
    except (ValueError, ZeroDivisionError) as error:
        raise ValueError(f"profile field {key} is not an exact decimal") from error


def integer_root_floor(value: int, degree: int) -> int:
    if value < 0 or degree <= 0:
        raise ValueError("integer root requires a non-negative value and positive degree")
    if value < 2:
        return value
    low = 0
    high = 1 << ((value.bit_length() + degree - 1) // degree)
    while low <= high:
        midpoint = (low + high) // 2
        power = midpoint**degree
        if power <= value:
            low = midpoint + 1
        else:
            high = midpoint - 1
    return high


def exact_fifth_root(value: Fraction) -> Fraction | None:
    numerator = integer_root_floor(value.numerator, 5)
    denominator = integer_root_floor(value.denominator, 5)
    if numerator**5 == value.numerator and denominator**5 == value.denominator:
        return Fraction(numerator, denominator)
    return None


class FifthRootInterval:
    """Tightening exact rational enclosure of one non-negative fifth root."""

    def __init__(self, radicand: Fraction) -> None:
        if not 0 <= radicand <= 1:
            raise ValueError("sRGB transfer radicand must be in [0, 1]")
        exact = exact_fifth_root(radicand)
        self.radicand = radicand
        self.lower = exact if exact is not None else Fraction(0)
        self.upper = exact if exact is not None else Fraction(1)
        self.depth = 0

    @property
    def resolved(self) -> bool:
        return self.lower == self.upper

    def refine(self) -> None:
        if self.resolved:
            return
        midpoint = (self.lower + self.upper) / 2
        midpoint_power = midpoint**5
        if midpoint_power == self.radicand:
            self.lower = midpoint
            self.upper = midpoint
        elif midpoint_power < self.radicand:
            self.lower = midpoint
        else:
            self.upper = midpoint
        self.depth += 1
        if not self.lower**5 <= self.radicand <= self.upper**5:
            raise AssertionError("fifth-root enclosure lost its exact radicand")


class NeutralAxisOracle:
    def __init__(self, profile: dict[str, Any]) -> None:
        if profile.get("schemaVersion") != 1:
            raise ValueError("unsupported WCAG profile schema")
        if profile.get("profileId") != "wcag22-srgb8-contrast-v1":
            raise ValueError("unsupported WCAG profile identity")

        self.split = exact_decimal(profile, "channelSplit")
        self.divisor = exact_decimal(profile, "linearDivisor")
        self.encoded_offset = exact_decimal(profile, "encodedOffset")
        self.encoded_scale = exact_decimal(profile, "encodedScale")
        self.exponent = exact_decimal(profile, "encodedExponent")
        self.contrast_offset = exact_decimal(profile, "contrastOffset")
        self.ratios = {
            criterion: exact_decimal(profile, field)
            for criterion, field in CRITERION_RATIO_FIELDS.items()
        }
        weights = tuple(
            exact_decimal(profile, key)
            for key in ("redWeight", "greenWeight", "blueWeight")
        )
        if self.exponent != Fraction(12, 5):
            raise ValueError("neutral-axis exact oracle requires exponent 12/5")
        if sum(weights, Fraction()) != 1:
            raise ValueError("neutral luminance weights must sum exactly to one")
        if self.divisor <= 0 or self.encoded_scale <= 0 or self.contrast_offset < 0:
            raise ValueError("WCAG profile contains a non-positive transfer parameter")
        if any(ratio <= 1 for ratio in self.ratios.values()):
            raise ValueError("WCAG contrast ratios must be greater than one")

        self._luminances: dict[int, FifthRootInterval] = {}

    def luminance(self, code: int) -> FifthRootInterval:
        if not isinstance(code, int) or not 0 <= code <= 255:
            raise ValueError("sRGB8 neutral code must be an integer in [0, 255]")
        cached = self._luminances.get(code)
        if cached is not None:
            return cached

        encoded = Fraction(code, 255)
        if encoded <= self.split:
            exact = encoded / self.divisor
            interval = FifthRootInterval(exact**5)
            if not interval.resolved or interval.lower != exact:
                raise AssertionError("linear transfer branch must remain exact")
        else:
            base = (encoded + self.encoded_offset) / self.encoded_scale
            interval = FifthRootInterval(base**12)
        self._luminances[code] = interval
        return interval

    def contrast_passes(self, first: int, second: int, criterion: str) -> bool:
        try:
            ratio = self.ratios[criterion]
        except KeyError as error:
            raise ValueError(f"unsupported WCAG criterion: {criterion}") from error

        lighter_code, darker_code = max(first, second), min(first, second)
        lighter = self.luminance(lighter_code)
        darker = self.luminance(darker_code)
        return self._contrast_passes_intervals(
            lighter, darker, ratio, MAX_ROOT_REFINEMENTS
        )

    def _contrast_passes_intervals(
        self,
        lighter: FifthRootInterval,
        darker: FifthRootInterval,
        ratio: Fraction,
        refinement_limit: int,
    ) -> bool:
        if refinement_limit < 0:
            raise ValueError("refinement limit must be non-negative")
        refinements = 0
        while True:
            # The actual margin is enclosed by these expressions because its
            # lighter term is positive and its darker term has coefficient -r.
            minimum_margin = (
                lighter.lower
                + self.contrast_offset
                - ratio * (darker.upper + self.contrast_offset)
            )
            maximum_margin = (
                lighter.upper
                + self.contrast_offset
                - ratio * (darker.lower + self.contrast_offset)
            )
            if minimum_margin >= 0:
                return True
            if maximum_margin < 0:
                return False
            if refinements == refinement_limit:
                break
            if darker is lighter:
                lighter.refine()
            elif lighter.upper - lighter.lower >= ratio * (
                darker.upper - darker.lower
            ):
                # Refine only the larger contribution to the undecided margin.
                # This keeps a frequently reused adjacency from accumulating
                # irrelevant precision while preserving exact enclosure.
                lighter.refine()
            else:
                darker.refine()
            refinements += 1
        raise ArithmeticError(
            f"contrast decision did not separate after {refinement_limit} refinements"
        )

    def candidate_set(self, adjacent_codes: Iterable[int], criterion: str) -> tuple[int, ...]:
        adjacent = tuple(adjacent_codes)
        if not adjacent:
            raise ValueError("fixture must declare a non-empty adjacent set")
        if len(set(adjacent)) != len(adjacent):
            raise ValueError("fixture adjacent set contains duplicates")
        for code in adjacent:
            self.luminance(code)
        return tuple(
            candidate
            for candidate in range(256)
            if all(self.contrast_passes(candidate, other, criterion) for other in adjacent)
        )

    def maximum_refinement_depth(self) -> int:
        return max((interval.depth for interval in self._luminances.values()), default=0)


def neutral_hex(code: int) -> str:
    return f"#{code:02X}{code:02X}{code:02X}"


def expand_ranges(ranges: Iterable[tuple[int, int]]) -> tuple[int, ...]:
    result: list[int] = []
    previous_end = -1
    for start, end in ranges:
        if not 0 <= start <= end <= 255 or start <= previous_end:
            raise ValueError("candidate ranges must be ordered, disjoint sRGB8 intervals")
        result.extend(range(start, end + 1))
        previous_end = end
    return tuple(result)


def compress_ranges(codes: tuple[int, ...]) -> list[list[str]]:
    if tuple(sorted(set(codes))) != codes:
        raise ValueError("candidate set must be strictly ascending and unique")
    if not codes:
        return []
    result: list[list[str]] = []
    start = previous = codes[0]
    for code in codes[1:]:
        if code == previous + 1:
            previous = code
            continue
        result.append([neutral_hex(start), neutral_hex(previous)])
        start = previous = code
    result.append([neutral_hex(start), neutral_hex(previous)])
    return result


def candidate_set_sha256(codes: tuple[int, ...]) -> str:
    encoded = b"".join(neutral_hex(code).encode("ascii") + b"\n" for code in codes)
    return sha256(CANDIDATE_DIGEST_DOMAIN + encoded)


def payload_sha256(document: dict[str, Any]) -> str:
    payload = dict(document)
    digest = payload.pop("fixture_payload_sha256", None)
    if digest is None:
        raise ValueError("fixture payload digest is absent")
    return sha256(canonical_json_bytes(payload))


def seal_fixture(document: dict[str, Any]) -> dict[str, Any]:
    if "fixture_payload_sha256" in document:
        raise ValueError("unsealed fixture unexpectedly contains a payload digest")
    sealed = dict(document)
    placeholder = dict(document)
    placeholder["fixture_payload_sha256"] = "pending"
    sealed["fixture_payload_sha256"] = payload_sha256(placeholder)
    return sealed


def build_fixture(profile: dict[str, Any], profile_bytes: bytes) -> dict[str, Any]:
    oracle = NeutralAxisOracle(profile)
    fixtures: list[dict[str, Any]] = []
    for spec in FIXTURE_SPECS:
        expected = expand_ranges(spec["expected_ranges"])
        criteria = spec["criteria"]
        if not criteria:
            raise AssertionError("fixture criterion set must not be empty")
        computed_by_criterion = {
            criterion: oracle.candidate_set(spec["adjacent_codes"], criterion)
            for criterion in criteria
        }
        for criterion, computed in computed_by_criterion.items():
            if computed != expected:
                raise AssertionError(
                    f"{spec['id']} / {criterion}: expected {expected}, got {computed}"
                )
        if len(set(computed_by_criterion.values())) != 1:
            raise AssertionError(f"{spec['id']}: named criteria disagree")

        fixtures.append(
            {
                "adjacent": [neutral_hex(code) for code in spec["adjacent_codes"]],
                "candidate_count": len(expected),
                "candidate_ranges": compress_ranges(expected),
                "candidate_set_sha256": candidate_set_sha256(expected),
                "criteria": list(criteria),
                "id": spec["id"],
            }
        )

    document = {
        "algorithm": "fraction-adaptive-exact-fifth-root-interval-v1",
        "candidate_set_digest_domain": CANDIDATE_DIGEST_DOMAIN[:-1].decode("ascii"),
        "candidate_set_digest_law": "domain-nul-then-uppercase-neutral-hex-lf-v1",
        "criterion_ratio_fields": CRITERION_RATIO_FIELDS,
        "domain": {
            "candidate_count": 256,
            "candidate_order": "ascending-encoded-byte",
            "id": DOMAIN_ID,
            "member_encoding": "uppercase-hex-with-one-byte-repeated-three-times-v1",
            "member_range": ["#000000", "#FFFFFF"],
        },
        "fixture_id": FIXTURE_ID,
        "fixture_payload_digest_law": PAYLOAD_DIGEST_LAW,
        "fixtures": fixtures,
        "maximum_root_refinement_depth_observed": oracle.maximum_refinement_depth(),
        "oracle_id": ORACLE_ID,
        "oracle_source": "scripts/verify_wcag22_neutral_axis.py",
        "oracle_source_sha256": sha256(ORACLE_PATH.read_bytes()),
        "profile_id": profile["profileId"],
        "profile_source": "crates/labcolors-core/contracts/wcag22-srgb8-v1.json",
        "profile_source_sha256": sha256(profile_bytes),
        "root_refinement_fail_closed_limit": MAX_ROOT_REFINEMENTS,
        "schema_version": 1,
    }
    return seal_fixture(document)


def verify_payload_integrity(document: dict[str, Any]) -> None:
    declared = document.get("fixture_payload_sha256")
    if not isinstance(declared, str) or len(declared) != 64:
        raise ValueError("fixture payload digest must be a 64-digit SHA-256")
    if payload_sha256(document) != declared:
        raise ValueError("fixture payload SHA-256 mismatch")


def run_self_tests(profile: dict[str, Any]) -> int:
    tests = 0

    for constant in (b"NaN", b"Infinity", b"-Infinity"):
        try:
            parse_json(b'{"value":' + constant + b"}", "non-finite probe")
        except ValueError as error:
            if "is not strict JSON" not in str(error):
                raise AssertionError("strict-JSON error lost its context") from error
            tests += 1
        else:
            raise AssertionError(f"non-JSON constant {constant!r} was accepted")

    if integer_root_floor(0, 5) != 0 or integer_root_floor(33, 5) != 2:
        raise AssertionError("integer fifth-root floor self-test failed")
    tests += 1
    if exact_fifth_root(Fraction(32, 243)) != Fraction(2, 3):
        raise AssertionError("exact rational fifth-root self-test failed")
    tests += 1

    interval = FifthRootInterval(Fraction(1, 2))
    original_width = interval.upper - interval.lower
    interval.refine()
    if not interval.lower**5 <= Fraction(1, 2) <= interval.upper**5:
        raise AssertionError("adaptive fifth-root enclosure self-test failed")
    if not interval.upper - interval.lower < original_width:
        raise AssertionError("adaptive fifth-root interval did not contract")
    tests += 1

    oracle = NeutralAxisOracle(profile)
    normal = "sc-1.4.3-text-default"
    large = "sc-1.4.3-text-large-scale"
    if not oracle.contrast_passes(0x04, 0x76, normal):
        raise AssertionError("known lower passing boundary disappeared")
    if oracle.contrast_passes(0x05, 0x76, normal):
        raise AssertionError("known lower failing neighbour was admitted")
    if oracle.contrast_passes(0xFD, 0x76, normal):
        raise AssertionError("known upper failing neighbour was admitted")
    if not oracle.contrast_passes(0xFE, 0x76, normal):
        raise AssertionError("known upper passing boundary disappeared")
    tests += 4

    normal_bw = oracle.candidate_set((0x00, 0xFF), normal)
    if normal_bw != (0x75, 0x76):
        raise AssertionError("normal-text two-anchor anti-vacuum witness drifted")
    tests += 1
    if oracle.candidate_set((0x00, 0x76, 0xFF), normal):
        raise AssertionError("third adjacency failed to empty the candidate set")
    tests += 1
    if oracle.candidate_set((0x00, 0xFF), large) == normal_bw:
        raise AssertionError("ratio mutation did not change the candidate set")
    tests += 1

    class NonSeparatingInterval:
        lower = Fraction(0)
        upper = Fraction(1)

        def __init__(self) -> None:
            self.refinements = 0

        def refine(self) -> None:
            self.refinements += 1

    nonseparating = NonSeparatingInterval()
    try:
        oracle._contrast_passes_intervals(
            nonseparating,  # type: ignore[arg-type]
            nonseparating,  # type: ignore[arg-type]
            Fraction(9, 2),
            2,
        )
    except ArithmeticError:
        if nonseparating.refinements != 2:
            raise AssertionError("refinement cap performed work past its exact limit")
        tests += 1
    else:
        raise AssertionError("non-separating interval did not fail closed")

    try:
        oracle.candidate_set((), normal)
    except ValueError:
        tests += 1
    else:
        raise AssertionError("empty-adjacency vacuum was accepted")

    probe = seal_fixture({"schema_version": 1, "witness": "intact"})
    verify_payload_integrity(probe)
    probe["witness"] = "tampered"
    try:
        verify_payload_integrity(probe)
    except ValueError:
        tests += 1
    else:
        raise AssertionError("fixture tamper self-test did not fail")
    return tests


def load_profile() -> tuple[dict[str, Any], bytes]:
    raw = PROFILE_PATH.read_bytes()
    profile = parse_json(raw, "WCAG profile")
    if canonical_json_bytes(profile) != raw:
        raise ValueError("WCAG profile bytes are not canonical JSON plus LF")
    return profile, raw


def verify() -> tuple[int, dict[str, Any], bytes]:
    profile, profile_bytes = load_profile()
    self_test_count = run_self_tests(profile)
    fixture_bytes = FIXTURE_PATH.read_bytes()
    fixture = parse_json(fixture_bytes, "neutral-axis fixture")
    if canonical_json_bytes(fixture) != fixture_bytes:
        raise ValueError("neutral-axis fixture is not canonical JSON plus LF")
    verify_payload_integrity(fixture)
    if fixture.get("profile_source_sha256") != sha256(profile_bytes):
        raise ValueError("fixture does not bind the exact normative profile bytes")
    if fixture.get("oracle_source_sha256") != sha256(ORACLE_PATH.read_bytes()):
        raise ValueError("fixture does not bind the exact oracle source bytes")
    expected = build_fixture(profile, profile_bytes)
    if fixture != expected:
        raise ValueError("neutral-axis fixture differs from exact recomputation")
    return self_test_count, fixture, fixture_bytes


def emit() -> None:
    profile, profile_bytes = load_profile()
    run_self_tests(profile)
    fixture = build_fixture(profile, profile_bytes)
    FIXTURE_PATH.parent.mkdir(parents=True, exist_ok=True)
    FIXTURE_PATH.write_bytes(canonical_json_bytes(fixture))


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--emit",
        action="store_true",
        help="write the canonical fixture after all independent expectations pass",
    )
    args = parser.parse_args(argv)
    if args.emit:
        emit()
    tests, fixture, fixture_bytes = verify()
    counts = [entry["candidate_count"] for entry in fixture["fixtures"]]
    print(
        "WCAG22 neutral-axis exact oracle: PASS; "
        f"counts={counts}; self_tests={tests}; "
        f"fixture_sha256={sha256(fixture_bytes)}"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except (AssertionError, ArithmeticError, OSError, TypeError, ValueError) as error:
        print(f"WCAG22 neutral-axis exact oracle: FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
