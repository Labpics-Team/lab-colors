#!/usr/bin/env python3
"""Независимо проверяет WCAG 2.2 sRGB8 Q55 artifact из Issue #284.

Verifier не импортирует production generator. Primary row oracle использует
adaptive-precision Decimal с directed rounding и stability across precisions;
отдельный integer proof проверяет tightness. Полный sRGB8 domain перечисляется
как 256^3 интервалов, но пары 256^6 не перебираются: монотонный scan
рассматривает только boundary band, где threshold law может быть unresolved.
"""

from __future__ import annotations

import hashlib
import heapq
import json
import re
import struct
import sys
import time
from array import array
from dataclasses import dataclass
from decimal import Decimal, ROUND_CEILING, ROUND_FLOOR, localcontext
from fractions import Fraction
from math import gcd
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
PROFILE_PATH = REPO_ROOT / "crates/labcolors-core/contracts/wcag22-srgb8-v1.json"
GENERATOR_PATH = REPO_ROOT / "scripts/generate_wcag22_q55.py"
VERIFIER_PATH = Path(__file__).resolve()
RUST_ARTIFACT = REPO_ROOT / "crates/labcolors-core/src/wcag22/q55_data.rs"
KERNEL_SOURCE = REPO_ROOT / "crates/labcolors-core/src/wcag22/kernel.rs"
PARSER_SOURCE = REPO_ROOT / "crates/labcolors-core/src/srgb8.rs"
TERMINAL_EVIDENCE_SOURCE = REPO_ROOT / "crates/labcolors-core/src/wcag22_evidence.rs"
FACADE_SOURCE = REPO_ROOT / "crates/labcolors-core/src/wcag22.rs"
CRATE_LIB_SOURCE = REPO_ROOT / "crates/labcolors-core/src/lib.rs"
EVALUATOR_SOURCE = FACADE_SOURCE
NUMERICS_SOURCE = REPO_ROOT / "crates/labcolors-core/src/numerics.rs"
CANONICAL_BINARY_ARTIFACT = (
    REPO_ROOT / "crates/labcolors-core/contracts/wcag22-srgb8-q55-v1.bin"
)
PROOF_PATH = REPO_ROOT / "crates/labcolors-core/contracts/wcag22-srgb8-q55-proof-v1.json"
PROFILE_BYTES = PROFILE_PATH.read_bytes()
PROFILE = json.loads(PROFILE_BYTES)
# Independent immutable expectation for profile V1. The JSON is the production
# SSOT; this second copy is deliberately a test oracle that makes a normative
# value or engineering-scale mutation fail instead of silently defining V1 anew.
NORMATIVE_PROFILE_V1 = {
    "schemaVersion": 1,
    "profileId": "wcag22-srgb8-contrast-v1",
    "recommendation": "https://www.w3.org/TR/2024/REC-WCAG22-20241212/",
    "channelSplit": "0.04045",
    "linearDivisor": "12.92",
    "encodedOffset": "0.055",
    "encodedScale": "1.055",
    "encodedExponent": "2.4",
    "redWeight": "0.2126",
    "greenWeight": "0.7152",
    "blueWeight": "0.0722",
    "contrastOffset": "0.05",
    "normalTextRatio": "4.5",
    "largeTextRatio": "3.0",
    "requiredNonTextRatio": "3.0",
    "fixedPointScalePower": 55,
}
if PROFILE != NORMATIVE_PROFILE_V1:
    raise ValueError("canonical WCAG22 sRGB8 profile V1 drifted")

ARTIFACT_ID = "wcag22-srgb8-luminance-q55-v1"
BOUND_ID = "wcag22-srgb8-outward-q55-v1"
PROOF_ID = "wcag22-srgb8-full-domain-q55-v1"
KERNEL_ID = "wcag22-srgb8-evaluation-kernel-v1"
EXPECTED_KERNEL_SHA256 = (
    "daa10163830e2f15f13ab3ca26c5bae561397b39e641d35c68cae3fd5f1cb601"
)
TERMINAL_EVIDENCE_ID = "wcag22-srgb8-terminal-evidence-v1"
EXPECTED_TERMINAL_EVIDENCE_SHA256 = (
    "3c5a75b07254c6071a64700af208a64987d0f0ea9698eadc54a9e74585ce1f72"
)
PARSER_ID = "encoded-srgb8-hex-parser-v1"
EXPECTED_PARSER_SHA256 = (
    "57cd2605e040a4d206a83c86cf01c5d6935e5bff9c45e556db0e4c6eaede7280"
)
FACADE_ID = "wcag22-srgb8-public-facade-v1"
EXPECTED_NORMALIZED_FACADE_SHA256 = (
    "07abc87d428e26d793c306babdddb4fb1746a5ef7fff3698feee5a786ebc6b51"
)
EXPECTED_CRATE_LIB_SHA256 = (
    "40d926da94547201242ef3aaf01db4c7e3912e8034998ab9a11671882057a726"
)
DECLARED_OPERATION_LAW = (
    "final-srgb8-outward-q55-two-orientation-integer-threshold-v1"
)
PROFILE_CHECKSUM_DOMAIN = b"labcolors.wcag22-srgb8-profile.v1"
PROFILE_CHECKSUM_FIELDS = (
    "profileId",
    "recommendation",
    "channelSplit",
    "linearDivisor",
    "encodedOffset",
    "encodedScale",
    "encodedExponent",
    "redWeight",
    "greenWeight",
    "blueWeight",
    "contrastOffset",
    "normalTextRatio",
    "largeTextRatio",
    "requiredNonTextRatio",
    "fixedPointScalePower",
)


def exact_decimal(key: str) -> Decimal:
    value = PROFILE.get(key)
    if not isinstance(value, str):
        raise TypeError(f"profile field {key} must be an exact decimal string")
    return Decimal(value)


def exact_fraction(key: str) -> Fraction:
    value = PROFILE.get(key)
    if not isinstance(value, str):
        raise TypeError(f"profile field {key} must be an exact decimal string")
    return Fraction(value)


SCALE_POWER = PROFILE.get("fixedPointScalePower")
if not isinstance(SCALE_POWER, int):
    raise TypeError("profile fixedPointScalePower must be an integer")
Q = 1 << SCALE_POWER
SPLIT_DECIMAL = exact_decimal("channelSplit")
DIVISOR_DECIMAL = exact_decimal("linearDivisor")
OFFSET_DECIMAL = exact_decimal("encodedOffset")
ENCODED_SCALE_DECIMAL = exact_decimal("encodedScale")
EXPONENT_DECIMAL = exact_decimal("encodedExponent")
SPLIT_FRACTION = exact_fraction("channelSplit")
DIVISOR_FRACTION = exact_fraction("linearDivisor")
OFFSET_FRACTION = exact_fraction("encodedOffset")
ENCODED_SCALE_FRACTION = exact_fraction("encodedScale")
EXPONENT_FRACTION = exact_fraction("encodedExponent")
CONTRAST_OFFSET_FRACTION = exact_fraction("contrastOffset")
WEIGHT_KEYS = ("redWeight", "greenWeight", "blueWeight")
WEIGHT_DECIMALS = tuple(exact_decimal(key) for key in WEIGHT_KEYS)
WEIGHT_FRACTIONS = tuple(exact_fraction(key) for key in WEIGHT_KEYS)
if EXPONENT_FRACTION != Fraction(12, 5):
    raise ValueError(f"unsupported exact exponent: {EXPONENT_FRACTION}")
if sum(WEIGHT_FRACTIONS, Fraction()) != 1:
    raise ValueError("WCAG luminance weights must sum exactly to one")

CHANNEL_CODES = 256
COLOR_COUNT = CHANNEL_CODES**3
ROW_COUNT = len(WEIGHT_KEYS) * CHANNEL_CODES
PACK_WIDTH_BITS = 2
PACK_WIDTH_MASK = (1 << PACK_WIDTH_BITS) - 1
DECIMAL_PRECISIONS = (48, 72, 108, 162)


def fnv1a32(data: bytes) -> str:
    value = 0x811C9DC5
    for byte in data:
        value = ((value ^ byte) * 0x01000193) & 0xFFFFFFFF
    return f"{value:08x}"


def profile_checksum() -> str:
    def framed(value: bytes) -> bytes:
        return struct.pack("<I", len(value)) + value

    preimage = bytearray(framed(PROFILE_CHECKSUM_DOMAIN))
    preimage.extend(struct.pack("<I", PROFILE["schemaVersion"]))
    for key in PROFILE_CHECKSUM_FIELDS:
        value = str(PROFILE[key]).encode("utf-8")
        preimage.extend(framed(key.encode("utf-8")))
        preimage.extend(framed(value))
    return fnv1a32(bytes(preimage))


@dataclass(frozen=True)
class Threshold:
    name: str
    light_factor: int
    dark_factor: int
    offset_factor: int


def threshold_from_profile(key: str) -> Threshold:
    ratio = exact_fraction(key)
    common_denominator = CONTRAST_OFFSET_FRACTION.denominator
    light_factor = ratio.denominator * common_denominator
    dark_factor = ratio.numerator * common_denominator
    offset_factor = (
        ratio.numerator - ratio.denominator
    ) * CONTRAST_OFFSET_FRACTION.numerator
    common = gcd(gcd(light_factor, dark_factor), offset_factor)
    return Threshold(
        str(PROFILE[key]),
        light_factor // common,
        dark_factor // common,
        offset_factor // common,
    )


if exact_fraction("largeTextRatio") != exact_fraction("requiredNonTextRatio"):
    raise ValueError("profile 3:1 criteria no longer share one threshold law")
THRESHOLDS = (
    threshold_from_profile("largeTextRatio"),
    threshold_from_profile("normalTextRatio"),
)


def ceil_div(numerator: int, denominator: int) -> int:
    return -(-numerator // denominator)


def decimal_contribution(
    weight: Decimal, code: int, precision: int, rounding: str
) -> Decimal:
    """Считает contribution в независимом Decimal backend."""
    with localcontext() as context:
        context.prec = precision
        context.rounding = rounding
        encoded = Decimal(code) / Decimal(255)
        if encoded <= SPLIT_DECIMAL:
            return Decimal(Q) * weight * encoded / DIVISOR_DECIMAL
        base = (encoded + OFFSET_DECIMAL) / ENCODED_SCALE_DECIMAL
        return Decimal(Q) * weight * context.power(base, EXPONENT_DECIMAL)


def decimal_weighted_bounds(weight: Decimal, code: int) -> tuple[int, int, int]:
    """Требует stable floor/ceil в двух successive precision levels."""
    previous: tuple[int, int] | None = None
    for precision in DECIMAL_PRECISIONS:
        lower_value = decimal_contribution(weight, code, precision, ROUND_FLOOR)
        upper_value = decimal_contribution(weight, code, precision, ROUND_CEILING)
        assert lower_value <= upper_value
        signature = (
            int(lower_value.to_integral_value(rounding=ROUND_FLOOR)),
            int(upper_value.to_integral_value(rounding=ROUND_CEILING)),
        )
        if signature == previous:
            lower, upper = signature
            assert 0 <= lower <= upper <= Q
            assert upper - lower <= 1
            return lower, upper, precision
        previous = signature
    raise AssertionError(
        f"Decimal bounds did not stabilize: weight={weight}, code={code}, "
        f"last={previous}"
    )


def integer_tightness_cross_check(
    weight: Fraction, code: int, row: tuple[int, int]
) -> None:
    """Отдельно доказывает, что committed Decimal-confirmed row tight."""
    lower, upper = row
    encoded = Fraction(code, 255)
    if encoded <= SPLIT_FRACTION:
        contribution = Q * weight * encoded / DIVISOR_FRACTION
        expected_lower, remainder = divmod(
            contribution.numerator, contribution.denominator
        )
        expected_upper = expected_lower + int(remainder != 0)
        assert row == (expected_lower, expected_upper)
    else:
        base = (encoded + OFFSET_FRACTION) / ENCODED_SCALE_FRACTION
        numerator = Q**5 * weight.numerator**5 * base.numerator**12
        denominator = weight.denominator**5 * base.denominator**12
        assert lower**5 * denominator <= numerator
        assert (lower + 1) ** 5 * denominator > numerator
        exact = lower**5 * denominator == numerator
        expected_upper = lower if exact else lower + 1
        assert upper == expected_upper

    assert 0 <= lower <= upper <= Q
    assert upper - lower <= 1


def parse_committed_artifact(
    path: Path,
) -> tuple[dict[str, int | str], list[tuple[int, int]]]:
    source = path.read_text(encoding="utf-8")
    metadata_patterns = {
        "q55_scale": r"Q55_SCALE: u64 = (\d+);",
        "profile_checksum": r'PROFILE_CHECKSUM: &str =\s*"([0-9a-f]{8})";',
        "profile_source_sha256": (
            r'PROFILE_SOURCE_SHA256: &str =\s*"([0-9a-f]{64})";'
        ),
        "generator_sha256": r'GENERATOR_SHA256: &str =\s*"([0-9a-f]{64})";',
        "artifact_sha256": r'ARTIFACT_SHA256: &str =\s*"([0-9a-f]{64})";',
    }
    metadata: dict[str, int | str] = {}
    for key, pattern in metadata_patterns.items():
        match = re.search(pattern, source)
        if match is None:
            raise AssertionError(f"missing {key} metadata in {path}")
        metadata[key] = int(match.group(1)) if key == "q55_scale" else match.group(1)
    table_match = re.search(
        r"WEIGHTED_CONTRIBUTION_BOUNDS:.*?= \[(.*)\];\s*$",
        source,
        flags=re.DOTALL,
    )
    if table_match is None:
        raise AssertionError(f"не удалось разобрать table из {path}")
    rows = [
        (int(lower), int(upper))
        for lower, upper in re.findall(r"\[(\d+),\s*(\d+)\]", table_match.group(1))
    ]
    return metadata, rows


def canonical_digest(rows: list[tuple[int, int]]) -> str:
    digest = hashlib.sha256()
    for lower, upper in rows:
        digest.update(struct.pack("<QQ", lower, upper))
    return digest.hexdigest()


def canonical_bytes(rows: list[tuple[int, int]]) -> bytes:
    return b"".join(struct.pack("<QQ", lower, upper) for lower, upper in rows)


def verify_rows(
    metadata: dict[str, int | str],
    committed_rows: list[tuple[int, int]],
    *,
    canonical_binary_artifact: bytes | None = None,
) -> tuple[list[list[tuple[int, int]]], dict[str, int | list[int]]]:
    assert metadata["q55_scale"] == Q, (
        f"scale drift: {metadata['q55_scale']} != {Q}"
    )
    assert metadata["profile_checksum"] == profile_checksum(), (
        "typed profile checksum drift: "
        f"artifact={metadata['profile_checksum']}, verifier={profile_checksum()}"
    )
    assert len(committed_rows) == ROW_COUNT, (
        f"row-count drift: {len(committed_rows)} != {ROW_COUNT}"
    )
    committed_binary = (
        CANONICAL_BINARY_ARTIFACT.read_bytes()
        if canonical_binary_artifact is None
        else canonical_binary_artifact
    )
    expected_binary = canonical_bytes(committed_rows)
    assert committed_binary == expected_binary, (
        "canonical binary artifact differs from the production Rust table"
    )
    actual_digest = canonical_digest(committed_rows)
    assert actual_digest == metadata["artifact_sha256"], (
        "artifact digest drift: "
        f"metadata={metadata['artifact_sha256']}, bytes={actual_digest}"
    )

    expected_rows: list[tuple[int, int]] = []
    used_precisions: list[int] = []
    for weight in WEIGHT_DECIMALS:
        for code in range(CHANNEL_CODES):
            lower, upper, precision = decimal_weighted_bounds(weight, code)
            expected_rows.append((lower, upper))
            used_precisions.append(precision)
    for index, (committed, expected) in enumerate(zip(committed_rows, expected_rows)):
        assert committed == expected, (
            f"row {index} differs from adaptive Decimal oracle: "
            f"committed={committed}, expected={expected}"
        )
        channel, code = divmod(index, CHANNEL_CODES)
        integer_tightness_cross_check(WEIGHT_FRACTIONS[channel], code, committed)

    tables = [
        committed_rows[offset : offset + CHANNEL_CODES]
        for offset in range(0, ROW_COUNT, CHANNEL_CODES)
    ]
    return tables, {
        "rows_stable": len(expected_rows),
        "precision_schedule": list(DECIMAL_PRECISIONS),
        "maximum_precision_used": max(used_precisions),
        "integer_tightness_cross_checks": len(expected_rows),
    }


def pack_interval(lower: int, upper: int) -> int:
    width = upper - lower
    assert 0 <= width <= PACK_WIDTH_MASK
    return (lower << PACK_WIDTH_BITS) | width


def unpack_interval(packed: int) -> tuple[int, int]:
    lower = packed >> PACK_WIDTH_BITS
    return lower, lower + (packed & PACK_WIDTH_MASK)


def build_unique_color_intervals(
    tables: list[list[tuple[int, int]]],
) -> tuple[array, int]:
    """Перечисляет 256^3 colours и merge-сортирует их без giant Python list."""
    red, green, blue = tables
    red_green = [
        pack_interval(r_lo + g_lo, r_hi + g_hi)
        for r_lo, r_hi in red
        for g_lo, g_hi in green
    ]
    red_green.sort()

    blue_offsets = [pack_interval(lower, upper) for lower, upper in blue]
    heap = [
        (red_green[0] + offset, blue_code, 0)
        for blue_code, offset in enumerate(blue_offsets)
    ]
    heapq.heapify(heap)

    unique = array("Q")
    previous = -1
    generated = 0
    while heap:
        packed, blue_code, rg_index = heap[0]
        generated += 1
        if packed != previous:
            unique.append(packed)
            previous = packed
        next_index = rg_index + 1
        if next_index == len(red_green):
            heapq.heappop(heap)
        else:
            heapq.heapreplace(
                heap,
                (
                    red_green[next_index] + blue_offsets[blue_code],
                    blue_code,
                    next_index,
                ),
            )

    assert generated == COLOR_COUNT
    return unique, generated


def orientation(
    lighter: tuple[int, int], darker: tuple[int, int], threshold: Threshold
) -> str:
    light_lower, light_upper = lighter
    dark_lower, dark_upper = darker
    pass_rhs = threshold.dark_factor * dark_upper + threshold.offset_factor * Q
    fail_rhs = threshold.dark_factor * dark_lower + threshold.offset_factor * Q
    passes = threshold.light_factor * light_lower >= pass_rhs
    fails = threshold.light_factor * light_upper < fail_rhs
    if passes and not fails:
        return "pass"
    if fails and not passes:
        return "fail"
    return "unresolved"


def pair_decision(
    first: tuple[int, int], second: tuple[int, int], threshold: Threshold
) -> str:
    forward = orientation(first, second, threshold)
    reverse = orientation(second, first, threshold)
    if forward == "pass" or reverse == "pass":
        return "pass"
    if forward == "fail" and reverse == "fail":
        return "fail"
    return "unresolved"


def decision_margin(
    lighter: tuple[int, int], darker: tuple[int, int], threshold: Threshold
) -> tuple[str, int] | None:
    verdict = orientation(lighter, darker, threshold)
    light_lower, light_upper = lighter
    dark_lower, dark_upper = darker
    if verdict == "pass":
        margin = (
            threshold.light_factor * light_lower
            - threshold.dark_factor * dark_upper
            - threshold.offset_factor * Q
        )
        return verdict, margin
    if verdict == "fail":
        # Strict integer '<' means the smallest definite-fail margin is one.
        margin = (
            threshold.dark_factor * dark_lower
            + threshold.offset_factor * Q
            - threshold.light_factor * light_upper
        )
        return verdict, margin
    return None


def scan_threshold(
    intervals: array, max_width: int, threshold: Threshold
) -> dict[str, object]:
    """Доказывает zero unresolved через полный monotone boundary scan.

    Для darker D orientation может быть unresolved только когда lower(L) лежит
    между ceil((q*D.lower+cQ)/p)-max_width и safe upper bound, где
    D.upper <= D.lower+max_width. Обе pointer boundaries поэтому монотонны по
    D.lower; всё ниже definite Fail, всё выше definite Pass.
    """
    count = len(intervals)
    light_factor = threshold.light_factor
    dark_factor = threshold.dark_factor
    offset = threshold.offset_factor * Q
    left = 0
    right = 0
    candidate_checks = 0
    unresolved: tuple[int, int] | None = None
    best: dict[str, tuple[int, int, int] | None] = {"pass": None, "fail": None}

    for darker_packed in intervals:
        dark_lower = darker_packed >> PACK_WIDTH_BITS
        dark_upper = dark_lower + (darker_packed & PACK_WIDTH_MASK)
        candidate_lower = (
            ceil_div(dark_factor * dark_lower + offset, light_factor) - max_width
        )
        candidate_upper = (
            dark_factor * (dark_lower + max_width) + offset - 1
        ) // light_factor

        while left < count and (intervals[left] >> PACK_WIDTH_BITS) < candidate_lower:
            left += 1
        if right < left:
            right = left
        while right < count and (intervals[right] >> PACK_WIDTH_BITS) <= candidate_upper:
            right += 1

        for index in range(left, right):
            lighter_packed = intervals[index]
            candidate_checks += 1
            if pair_decision(
                unpack_interval(lighter_packed),
                (dark_lower, dark_upper),
                threshold,
            ) == "unresolved":
                unresolved = (lighter_packed, darker_packed)
                break
            result = decision_margin(
                unpack_interval(lighter_packed),
                (dark_lower, dark_upper),
                threshold,
            )
            if result is not None:
                verdict, margin = result
                current = best[verdict]
                if current is None or margin < current[0]:
                    best[verdict] = (margin, lighter_packed, darker_packed)
        if unresolved is not None:
            break

        # Boundary neighbours are sufficient for the minimum definite margins;
        # interior points move monotonically farther from the decision boundary.
        if left > 0:
            lighter_packed = intervals[left - 1]
            light_lower = lighter_packed >> PACK_WIDTH_BITS
            light_upper = light_lower + (lighter_packed & PACK_WIDTH_MASK)
            margin = dark_factor * dark_lower + offset - light_factor * light_upper
            assert margin > 0
            current = best["fail"]
            if current is None or margin < current[0]:
                best["fail"] = (margin, lighter_packed, darker_packed)
        if right < count:
            lighter_packed = intervals[right]
            light_lower = lighter_packed >> PACK_WIDTH_BITS
            margin = light_factor * light_lower - dark_factor * dark_upper - offset
            assert margin >= 0
            current = best["pass"]
            if current is None or margin < current[0]:
                best["pass"] = (margin, lighter_packed, darker_packed)

    if unresolved is not None:
        lighter, darker = unresolved
        raise AssertionError(
            f"threshold {threshold.name} unresolved: "
            f"lighter={unpack_interval(lighter)}, darker={unpack_interval(darker)}"
        )
    assert best["pass"] is not None
    assert best["fail"] is not None
    return {
        "threshold": threshold.name,
        "integer_law": {
            "light_factor": threshold.light_factor,
            "dark_factor": threshold.dark_factor,
            "offset_factor": threshold.offset_factor,
        },
        "unresolved": 0,
        "candidate_checks": candidate_checks,
        "minimum_pass_margin": best["pass"][0],
        "minimum_pass_intervals": [
            list(unpack_interval(best["pass"][1])),
            list(unpack_interval(best["pass"][2])),
        ],
        "minimum_fail_margin": best["fail"][0],
        "minimum_fail_intervals": [
            list(unpack_interval(best["fail"][1])),
            list(unpack_interval(best["fail"][2])),
        ],
        "witness_packed": [
            best["pass"][1],
            best["pass"][2],
            best["fail"][1],
            best["fail"][2],
        ],
    }


def verify_negative_controls(
    metadata: dict[str, int | str],
    committed_rows: list[tuple[int, int]],
) -> None:
    """Доказывает, что verifier действительно кусает digest, row и overlap."""
    bad_digest_metadata = metadata.copy()
    bad_digest_metadata["artifact_sha256"] = "0" * 64
    try:
        verify_rows(bad_digest_metadata, committed_rows)
    except AssertionError:
        pass
    else:
        raise AssertionError("negative control: digest tampering was accepted")

    mutated_rows = committed_rows.copy()
    lower, upper = mutated_rows[1]
    mutated_rows[1] = (lower, upper + 1)
    bad_row_metadata = metadata.copy()
    bad_row_metadata["artifact_sha256"] = canonical_digest(mutated_rows)
    try:
        verify_rows(
            bad_row_metadata,
            mutated_rows,
            canonical_binary_artifact=canonical_bytes(mutated_rows),
        )
    except AssertionError as error:
        expected = "row 1 differs from adaptive Decimal oracle"
        if not str(error).startswith(expected):
            raise AssertionError(
                "negative control: non-tight row missed the row oracle"
            ) from error
    else:
        raise AssertionError("negative control: non-tight row was accepted")

    # Q mod 10 != 0: этот synthetic L interval пересекает exact 3.0 boundary
    # для D=[0,3]. Scan обязан найти unresolved, иначе real zero неверифицируем.
    synthetic = array(
        "Q",
        sorted(
            (
                pack_interval(0, 3),
                pack_interval(Q // 10, Q // 10 + 1),
            )
        ),
    )
    try:
        scan_threshold(synthetic, 3, THRESHOLDS[0])
    except AssertionError as error:
        if "unresolved" not in str(error):
            raise
    else:
        raise AssertionError("negative control: synthetic overlap was accepted")


def verify_production_kernel() -> str:
    """Bind the proof to the exact complete Rust evaluator kernel."""
    source_bytes = KERNEL_SOURCE.read_bytes()
    digest = hashlib.sha256(source_bytes).hexdigest()
    assert digest == EXPECTED_KERNEL_SHA256, (
        f"production kernel drifted: {digest} != {EXPECTED_KERNEL_SHA256}"
    )
    source = source_bytes.decode("utf-8")
    compact = re.sub(r"\s+", " ", source)
    required = (
        "Wcag22CriterionV1::Sc143TextDefault => ThresholdV1::FourAndHalf",
        "Wcag22CriterionV1::Sc143TextLargeScale | Wcag22CriterionV1::Sc1411UiComponentOrState | Wcag22CriterionV1::Sc1411GraphicalObject => ThresholdV1::Three",
        "WEIGHTED_CONTRIBUTION_BOUNDS[0][usize::from(rgb[0])]",
        "WEIGHTED_CONTRIBUTION_BOUNDS[1][usize::from(rgb[1])]",
        "WEIGHTED_CONTRIBUTION_BOUNDS[2][usize::from(rgb[2])]",
        "lower: red[0] + green[0] + blue[0]",
        "upper: red[1] + green[1] + blue[1]",
        "10 * light_lower >= 30 * dark_upper + scale",
        "10 * light_upper < 30 * dark_lower + scale",
        "40 * light_lower >= 180 * dark_upper + 7 * scale",
        "40 * light_upper < 180 * dark_lower + 7 * scale",
        "matches!(forward, OrientedDecisionV1::Pass) || matches!(reverse, OrientedDecisionV1::Pass)",
        "matches!(forward, OrientedDecisionV1::Fail) && matches!(reverse, OrientedDecisionV1::Fail)",
        "let decision = classify_pair(foreground_luminance, background_luminance, criterion)",
        "mint_wcag22_evidence()",
        "measurement: Wcag22MeasurementV1 { foreground, background, foreground_luminance, background_luminance, }",
        "decision, evidence",
        "crate::srgb8::hex_bytes(value)",
        "evaluate_wcag22_srgb8(foreground, background, criterion)",
    )
    for fragment in required:
        assert fragment in compact, f"production kernel semantic drift: {fragment}"
    for forbidden in ("f64", "powf", "epsilon"):
        assert forbidden not in source, f"forbidden {forbidden} in production kernel"
    return digest


def verify_terminal_evidence() -> str:
    """Bind registry validation and sealed terminal evidence to exact source."""
    source_bytes = TERMINAL_EVIDENCE_SOURCE.read_bytes()
    digest = hashlib.sha256(source_bytes).hexdigest()
    assert digest == EXPECTED_TERMINAL_EVIDENCE_SHA256, (
        "terminal evidence module drifted: "
        f"{digest} != {EXPECTED_TERMINAL_EVIDENCE_SHA256}"
    )
    compact = re.sub(r"\s+", " ", source_bytes.decode("utf-8"))
    required = (
        "row.site_id != SITE_ID",
        'row.site_id.key() != "wcag22-srgb8-contrast-v1"',
        "row.stable_outcomes != [StableNumericalOutcomeV2::CanonicalFiniteBounded]",
        "!row.compatibility_releases.is_empty()",
        "row.evidence_classes != [NumericalEvidenceClassV2::CanonicalFiniteBounded]",
        "row.artifact_ids != [ARTIFACT_ID]",
        'ARTIFACT_ID.key() != "wcag22-srgb8-luminance-q55-v1"',
        "row.bound_ids != [BOUND_ID]",
        'BOUND_ID.key() != "wcag22-srgb8-outward-q55-v1"',
        "row.proof_ids != [PROOF_ID]",
        'PROOF_ID.key() != "wcag22-srgb8-full-domain-q55-v1"',
        "!row.runtime_attestations.is_empty()",
        "row.bound_status != NumericalBoundStatusV2::Available",
        "row.fallback_status != NumericalFallbackStatusV1::None",
        "NumericalDecisionEvidenceV1::CanonicalFiniteBounded( CanonicalFiniteBoundedEvidenceV1 { artifact_id: ARTIFACT_ID, bound_id: BOUND_ID, proof_id: PROOF_ID, _private: (), }, )",
    )
    for fragment in required:
        assert fragment in compact, f"terminal evidence semantic drift: {fragment}"
    return digest


def verify_srgb8_parser() -> str:
    """Bind the public hex transport to the exact shared byte parser."""
    source_bytes = PARSER_SOURCE.read_bytes()
    digest = hashlib.sha256(source_bytes).hexdigest()
    assert digest == EXPECTED_PARSER_SHA256, (
        f"encoded sRGB8 parser drifted: {digest} != {EXPECTED_PARSER_SHA256}"
    )
    compact = re.sub(r"\s+", " ", source_bytes.decode("utf-8"))
    required = (
        "hex.strip_prefix('#').unwrap_or(hex)",
        "hex.len() != 6 || !hex.is_ascii()",
        "parse(&hex[0..2])?",
        "parse(&hex[2..4])?",
        "parse(&hex[4..6])?",
    )
    for fragment in required:
        assert fragment in compact, f"encoded sRGB8 parser semantic drift: {fragment}"
    assert "trim_start_matches" not in compact, (
        "encoded sRGB8 parser must remove at most one optional hash prefix"
    )
    return digest


def verify_public_facade() -> tuple[str, str]:
    """Bind the public symbols to the kernel without a digest self-cycle."""
    source = FACADE_SOURCE.read_text(encoding="utf-8")
    normalized = source
    for name in (
        "PROOF_SOURCE_SHA256",
        "PROOF_PAYLOAD_SHA256",
        "VERIFIER_SHA256",
    ):
        pattern = rf'({name}: &str =\s*)"[0-9a-f]{{64}}"'
        normalized, count = re.subn(
            pattern,
            r'\1"<self-digest>"',
            normalized,
        )
        assert count == 1, f"public facade has {count} {name} digest literals"
    digest = hashlib.sha256(normalized.encode("utf-8")).hexdigest()
    assert digest == EXPECTED_NORMALIZED_FACADE_SHA256, (
        "normalized public facade drifted: "
        f"{digest} != {EXPECTED_NORMALIZED_FACADE_SHA256}"
    )

    compact = re.sub(r"\s+", " ", source)
    export = "pub use kernel::{evaluate_wcag22_hex, evaluate_wcag22_srgb8};"
    assert compact.count(export) == 1, "public facade no longer re-exports the kernel exactly"
    for forbidden in ("fn evaluate_wcag22_hex", "fn evaluate_wcag22_srgb8"):
        assert forbidden not in source, f"public facade shadows kernel with {forbidden}"

    crate_bytes = CRATE_LIB_SOURCE.read_bytes()
    crate_digest = hashlib.sha256(crate_bytes).hexdigest()
    assert crate_digest == EXPECTED_CRATE_LIB_SHA256, (
        f"crate root drifted: {crate_digest} != {EXPECTED_CRATE_LIB_SHA256}"
    )
    crate_source = crate_bytes.decode("utf-8")
    assert len(re.findall(r"(?m)^pub mod wcag22;$", crate_source)) == 1, (
        "crate root must export the canonical wcag22 module exactly once"
    )
    assert not re.search(r'#\[path\s*=\s*"[^"]+"\]\s*pub mod wcag22;', crate_source), (
        "crate root must not redirect the proof-bound wcag22 facade"
    )
    return digest, crate_digest


def verify_evaluator_digest_bindings(
    proof_sha256: str, proof_payload_sha256: str, verifier_sha256: str
) -> None:
    source = EVALUATOR_SOURCE.read_text(encoding="utf-8")
    expected = {
        "PROOF_SOURCE_SHA256": proof_sha256,
        "PROOF_PAYLOAD_SHA256": proof_payload_sha256,
        "VERIFIER_SHA256": verifier_sha256,
    }
    for name, digest in expected.items():
        match = re.search(rf'{name}: &str =\s*"([0-9a-f]{{64}})";', source)
        assert match is not None, f"missing evaluator binding {name}"
        assert match.group(1) == digest, (
            f"evaluator {name} drift: {match.group(1)} != {digest}"
        )


def find_rgb_witnesses(
    tables: list[list[tuple[int, int]]], targets: set[int]
) -> dict[int, str]:
    witnesses: dict[int, str] = {}
    red, green, blue = tables
    for r, (r_lo, r_hi) in enumerate(red):
        for g, (g_lo, g_hi) in enumerate(green):
            rg_lo = r_lo + g_lo
            rg_hi = r_hi + g_hi
            for b, (b_lo, b_hi) in enumerate(blue):
                packed = pack_interval(rg_lo + b_lo, rg_hi + b_hi)
                if packed in targets and packed not in witnesses:
                    witnesses[packed] = f"#{r:02X}{g:02X}{b:02X}"
                    if len(witnesses) == len(targets):
                        return witnesses
    raise AssertionError(f"не найдены RGB witnesses для {targets - witnesses.keys()}")


def main() -> int:
    emit_only = sys.argv[1:] == ["--emit"]
    if sys.argv[1:] not in ([], ["--emit"]):
        raise ValueError("usage: verify_wcag22_q55.py [--emit]")
    started = time.perf_counter()
    metadata, committed_rows = parse_committed_artifact(RUST_ARTIFACT)
    profile_digest = hashlib.sha256(PROFILE_BYTES).hexdigest()
    generator_digest = hashlib.sha256(GENERATOR_PATH.read_bytes()).hexdigest()
    verifier_digest = hashlib.sha256(VERIFIER_PATH.read_bytes()).hexdigest()
    rust_source_digest = hashlib.sha256(RUST_ARTIFACT.read_bytes()).hexdigest()
    kernel_digest = verify_production_kernel()
    terminal_evidence_digest = verify_terminal_evidence()
    parser_digest = verify_srgb8_parser()
    facade_digest, crate_lib_digest = verify_public_facade()
    registry_source = NUMERICS_SOURCE.read_text(encoding="utf-8")
    for identity in (ARTIFACT_ID, BOUND_ID, PROOF_ID):
        assert identity in registry_source, f"registry identity missing: {identity}"
    assert metadata["profile_source_sha256"] == profile_digest, (
        "profile digest drift: "
        f"artifact={metadata['profile_source_sha256']}, source={profile_digest}"
    )
    assert metadata["generator_sha256"] == generator_digest, (
        "generator digest drift: "
        f"artifact={metadata['generator_sha256']}, source={generator_digest}"
    )

    tables, decimal_report = verify_rows(metadata, committed_rows)
    verify_negative_controls(metadata, committed_rows)
    rows_elapsed = time.perf_counter() - started

    intervals, generated = build_unique_color_intervals(tables)
    domain_elapsed = time.perf_counter() - started
    max_width = sum(max(upper - lower for lower, upper in table) for table in tables)
    assert max_width <= PACK_WIDTH_MASK

    results = [scan_threshold(intervals, max_width, threshold) for threshold in THRESHOLDS]
    targets = {
        packed
        for result in results
        for packed in result.pop("witness_packed")
    }
    witnesses = find_rgb_witnesses(tables, targets)
    for result in results:
        for field in ("minimum_pass_intervals", "minimum_fail_intervals"):
            interval_pairs = result[field]
            result[field] = [
                {"bounds": bounds, "rgb": witnesses[pack_interval(*bounds)]}
                for bounds in interval_pairs
            ]

    payload = {
        "schema_version": 1,
        "profile_id": PROFILE["profileId"],
        "profile_checksum": profile_checksum(),
        "recommendation": PROFILE["recommendation"],
        "profile_source_sha256": profile_digest,
        "artifact_id": ARTIFACT_ID,
        "artifact_sha256": metadata["artifact_sha256"],
        "artifact_words": len(committed_rows) * 2,
        "artifact_rust_source_sha256": rust_source_digest,
        "bound_id": BOUND_ID,
        "proof_id": PROOF_ID,
        "kernel_id": KERNEL_ID,
        "kernel_source_sha256": kernel_digest,
        "terminal_evidence_id": TERMINAL_EVIDENCE_ID,
        "terminal_evidence_source_sha256": terminal_evidence_digest,
        "parser_id": PARSER_ID,
        "parser_source_sha256": parser_digest,
        "facade_id": FACADE_ID,
        "facade_normalized_sha256": facade_digest,
        "crate_lib_source_sha256": crate_lib_digest,
        "declared_operation_law": DECLARED_OPERATION_LAW,
        "generator_sha256": generator_digest,
        "verifier_sha256": verifier_digest,
        "q55_scale": Q,
        "rows": len(committed_rows),
        "row_oracle": decimal_report,
        "colors": generated,
        "unique_intervals": len(intervals),
        "max_color_interval_width": max_width,
        "negative_controls": 3,
        "full_domain_algorithm": "unique-q55-interval-monotone-boundary-v1",
        "thresholds": results,
    }
    canonical_payload = json.dumps(
        payload, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")
    proof = {
        **payload,
        "proof_payload_sha256": hashlib.sha256(canonical_payload).hexdigest(),
    }
    canonical_proof = json.dumps(proof, sort_keys=True, separators=(",", ":"))
    if not emit_only:
        committed = PROOF_PATH.read_text(encoding="utf-8")
        assert committed == canonical_proof + "\n", (
            "committed full-domain proof drifted; inspect and regenerate explicitly "
            "with --emit only after scientific/numerical review"
        )
        verify_evaluator_digest_bindings(
            hashlib.sha256(PROOF_PATH.read_bytes()).hexdigest(),
            proof["proof_payload_sha256"],
            verifier_digest,
        )
    elapsed = time.perf_counter() - started

    # stdout — canonical proof artifact; status/timing идут в stderr, поэтому
    # `python3 ... > proof.json` создаёт непосредственно bindable document.
    print(canonical_proof)
    print(
        "WCAG22 Q55 independent verification: PASS; timing_seconds: "
        f"rows={rows_elapsed:.3f}, domain={domain_elapsed:.3f}, "
        f"total={elapsed:.3f}",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, OSError, ValueError) as error:
        print(f"WCAG22 Q55 independent verification: FAIL: {error}", file=sys.stderr)
        raise SystemExit(1) from error
