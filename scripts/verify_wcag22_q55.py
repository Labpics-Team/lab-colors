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
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import textwrap
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
SRGB8_ROUTE_SOURCE = REPO_ROOT / "crates/labcolors-core/src/srgb8.rs"
PARSER_SOURCE = SRGB8_ROUTE_SOURCE
TERMINAL_EVIDENCE_SOURCE = REPO_ROOT / "crates/labcolors-core/src/wcag22_evidence.rs"
FACADE_SOURCE = REPO_ROOT / "crates/labcolors-core/src/wcag22.rs"
CRATE_ROOT_ROUTE_SOURCE = REPO_ROOT / "crates/labcolors-core/src/lib.rs"
CORE_MANIFEST = REPO_ROOT / "crates/labcolors-core/Cargo.toml"
EVALUATOR_SOURCE = FACADE_SOURCE
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
    "c97980c1ca2c7ea9cabff9c8d2fb7282773cca180ae15948391c29c9d6196040"
)
TERMINAL_EVIDENCE_ID = "wcag22-srgb8-terminal-evidence-v1"
EXPECTED_TERMINAL_EVIDENCE_SHA256 = (
    "3c5a75b07254c6071a64700af208a64987d0f0ea9698eadc54a9e74585ce1f72"
)
PARSER_ID = "encoded-srgb8-hex-parser-v1"
EXPECTED_PARSER_SHA256 = (
    "7729a07dcc356851a6c7e0763df96317b607e98396b20199a9575a3b625cbe18"
)
FACADE_ID = "wcag22-srgb8-public-facade-v1"
EXPECTED_NORMALIZED_FACADE_SHA256 = (
    "8cecfaf660e896c5ac7c377ed286fa0377a0201e83f2b65f858e4136348397ef"
)
DECLARED_OPERATION_LAW = (
    "final-srgb8-outward-q55-two-orientation-integer-threshold-v1"
)
SOURCE_BINDING_SCHEMA_VERSION = 1
SOURCE_BINDING_LAW = "wcag22-rust-semantic-dependency-cone-v1"
SOURCE_BINDING_DOMAIN = b"labcolors.wcag22-source-binding"
CANONICAL_CRATE_TARGET = b"crates/labcolors-core/src/lib.rs"
ROOT_ROUTE_BEGIN = b"// BEGIN WCAG22_SOURCE_ROUTES_V1"
ROOT_ROUTE_END = b"// END WCAG22_SOURCE_ROUTES_V1"
EXPECTED_ROOT_ROUTE_REGION = b"""// BEGIN WCAG22_SOURCE_ROUTES_V1
const _: () = ();
pub mod numerics;
pub(crate) mod srgb8;
pub mod wcag22;
#[doc(hidden)]
pub mod wcag22_evidence;
// END WCAG22_SOURCE_ROUTES_V1"""
PARSER_ROUTE_BEGIN = b"// BEGIN WCAG22_PARSER_CAPSULE_V1"
PARSER_ROUTE_END = b"// END WCAG22_PARSER_CAPSULE_V1"
EXPECTED_PARSER_ROUTE_REGION = b"""// BEGIN WCAG22_PARSER_CAPSULE_V1
const _: () = ();
/// Parse optional-`#` `RRGGBB` into the exact three encoded bytes.
///
/// Public APIs choose their own transport strictness before calling this SSOT.
/// ASCII is checked before byte slicing, so arbitrary public Unicode input
/// returns `Err` instead of panicking at a non-character boundary.
pub(crate) fn hex_bytes(hex: &str) -> Result<[u8; 3], String> {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 || !hex.is_ascii() {
        return Err(format!("expected #RRGGBB, got #{hex}"));
    }
    let parse = |value: &str| u8::from_str_radix(value, 16).map_err(|error| error.to_string());
    Ok([parse(&hex[0..2])?, parse(&hex[2..4])?, parse(&hex[4..6])?])
}
// END WCAG22_PARSER_CAPSULE_V1"""
PROFILE_CHECKSUM_DOMAIN = b"labcolors.wcag22-srgb8-profile.v1"
REGISTRY_ROW_BINDING_DOMAIN = b"labcolors.wcag22-registry-row.v1"
REGISTRY_ROW_BINDING_SCHEMA_VERSION = 1
EXPECTED_REGISTRY_ROW_SHA256 = (
    "c91c5e185c432ae4a9fb9ea03e9838bf2565f2aabff56019e190aae97bfaa0f1"
)
REGISTRY_ROW_SET_FIELDS = (
    "stable_outcomes",
    "compatibility_releases",
    "evidence_classes",
    "artifact_ids",
    "bound_ids",
    "proof_ids",
    "runtime_attestations",
)
EXPECTED_WCAG_REGISTRY_ROW = {
    "site_id": "wcag22-srgb8-contrast-v1",
    "stable_outcomes": ("canonical-finite-bounded",),
    "compatibility_releases": (),
    "evidence_classes": ("canonical-finite-bounded",),
    "artifact_ids": (ARTIFACT_ID,),
    "bound_ids": (BOUND_ID,),
    "proof_ids": (PROOF_ID,),
    "runtime_attestations": (),
    "bound_status": "available",
    "fallback_status": "none",
}
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


def length_prefixed(value: bytes) -> bytes:
    return struct.pack("<I", len(value)) + value


def profile_checksum() -> str:
    preimage = bytearray(length_prefixed(PROFILE_CHECKSUM_DOMAIN))
    preimage.extend(struct.pack("<I", PROFILE["schemaVersion"]))
    for key in PROFILE_CHECKSUM_FIELDS:
        value = str(PROFILE[key]).encode("utf-8")
        preimage.extend(length_prefixed(key.encode("utf-8")))
        preimage.extend(length_prefixed(value))
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


def verify_signed64_replay_envelope(max_interval_width: int) -> dict[str, int | str]:
    """Prove every cleared-denominator Q55 term fits a signed 64-bit replay."""
    outward_width_bound = len(WEIGHT_KEYS)
    assert max_interval_width <= outward_width_bound, (
        "Q55 proof exceeds the one-outward-unit-per-channel envelope"
    )
    maximum_luminance_upper = Q + outward_width_bound
    maximum_threshold_term = max(
        30 * maximum_luminance_upper + Q,
        180 * maximum_luminance_upper + 7 * Q,
    )
    signed_64_max = (1 << 63) - 1
    assert maximum_threshold_term <= signed_64_max

    next_scale = Q * 2
    next_scale_maximum_term = 180 * (
        next_scale + outward_width_bound
    ) + 7 * next_scale
    assert next_scale_maximum_term > signed_64_max, (
        "Q55 is no longer the maximal signed-64-safe binary scale"
    )
    return {
        "carrier": "signed-64",
        "observed_interval_width": max_interval_width,
        "outward_interval_width_bound": outward_width_bound,
        "maximum_luminance_upper": maximum_luminance_upper,
        "maximum_threshold_term": maximum_threshold_term,
        "carrier_maximum": signed_64_max,
        "headroom": signed_64_max - maximum_threshold_term,
        "next_scale_power": SCALE_POWER + 1,
        "next_scale_maximum_threshold_term": next_scale_maximum_term,
    }


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
) -> int:
    """Доказывает, что verifier действительно кусает digest, row и overlap."""
    controls = 0
    bad_digest_metadata = metadata.copy()
    bad_digest_metadata["artifact_sha256"] = "0" * 64
    try:
        verify_rows(bad_digest_metadata, committed_rows)
    except AssertionError:
        pass
    else:
        raise AssertionError("negative control: digest tampering was accepted")
    controls += 1

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
    controls += 1

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
    controls += 1
    return controls


def exact_source_region(
    path: Path,
    begin: bytes,
    end: bytes,
    expected: bytes,
) -> bytes:
    """Extract one sentinel capsule and require its reviewed exact bytes."""
    source = path.read_bytes()
    begins = list(re.finditer(rb"(?m)^" + re.escape(begin) + rb"$", source))
    ends = list(re.finditer(rb"(?m)^" + re.escape(end) + rb"$", source))
    assert len(begins) == 1 and len(ends) == 1, (
        f"source-binding markers must occur exactly once in {path}"
    )
    start = begins[0].start()
    assert start == 0, f"source-binding region must be the first source item in {path}"
    stop = ends[0].end()
    assert (start == 0 or source[start - 1 : start] == b"\n") and (
        stop == len(source) or source[stop : stop + 1] == b"\n"
    ), f"source-binding markers must occupy complete lines in {path}"
    region = source[start:stop]
    assert region == expected, f"canonical source-binding region drifted: {path}"
    return region


def verify_source_routes() -> str:
    """Bind Cargo's lib target and the two exact WCAG route capsules."""
    regions = (
        (
            b"crates/labcolors-core/Cargo.toml",
            b"cargo-lib-target-v1",
            verify_canonical_crate_target(),
        ),
        (
            CANONICAL_CRATE_TARGET,
            b"wcag22-source-routes-v1",
            exact_source_region(
                CRATE_ROOT_ROUTE_SOURCE,
                ROOT_ROUTE_BEGIN,
                ROOT_ROUTE_END,
                EXPECTED_ROOT_ROUTE_REGION,
            ),
        ),
        (
            b"crates/labcolors-core/src/srgb8.rs",
            b"wcag22-parser-capsule-v1",
            exact_source_region(
                SRGB8_ROUTE_SOURCE,
                PARSER_ROUTE_BEGIN,
                PARSER_ROUTE_END,
                EXPECTED_PARSER_ROUTE_REGION,
            ),
        ),
    )
    preimage = bytearray(length_prefixed(SOURCE_BINDING_DOMAIN))
    preimage.extend(struct.pack("<I", SOURCE_BINDING_SCHEMA_VERSION))
    preimage.extend(length_prefixed(SOURCE_BINDING_LAW.encode("utf-8")))
    preimage.extend(struct.pack("<I", len(regions)))
    for path, region_id, region in regions:
        preimage.extend(length_prefixed(path))
        preimage.extend(length_prefixed(region_id))
        preimage.extend(length_prefixed(region))
    return hashlib.sha256(bytes(preimage)).hexdigest()


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
    """Bind the exact production-only byte-parser capsule."""
    assert not PARSER_SOURCE.is_symlink(), "encoded sRGB8 parser source must not be a symlink"
    source_bytes = exact_source_region(
        PARSER_SOURCE,
        PARSER_ROUTE_BEGIN,
        PARSER_ROUTE_END,
        EXPECTED_PARSER_ROUTE_REGION,
    )
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
    assert compact.count("fn hex_bytes(") == 1 and "mod tests" not in compact, (
        "encoded sRGB8 parser capsule must contain only production parsing code"
    )
    return digest


def verify_public_facade() -> str:
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

    return digest


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


RUST_REGISTRY_PROBE = r"""
use std::fmt::Write;

use labcolors_core::{NumericalDecisionEvidenceV1, NumericalSiteIdV2, numerical_registry_v2};
use labcolors_core::wcag22::{
    Wcag22ApplicableDecisionV1, Wcag22AssessmentV1, Wcag22CriterionV1,
    Wcag22EvaluationErrorV1, evaluate_wcag22_hex, evaluate_wcag22_srgb8,
};

fn hex_key(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.bytes() {
        write!(&mut encoded, "{byte:02x}").expect("String writes are infallible");
    }
    encoded
}

macro_rules! emit_keys {
    ($name:literal, $values:expr) => {
        print!(concat!($name, "\t{}"), $values.len());
        for value in $values {
            print!("\t{}", hex_key(value.key()));
        }
        println!();
    };
}

fn decision_and_evidence(
    assessment: &Wcag22AssessmentV1,
) -> (Wcag22ApplicableDecisionV1, NumericalDecisionEvidenceV1) {
    let Wcag22AssessmentV1::Evaluated {
        decision, evidence, ..
    } = assessment else {
        panic!("public WCAG route returned NotEvaluated");
    };
    (*decision, *evidence)
}

fn verify_public_route() {
    let white = [255_u8; 3];
    let grey_118 = [118_u8; 3];
    let byte_path = evaluate_wcag22_srgb8(
        grey_118,
        white,
        Wcag22CriterionV1::Sc143TextDefault,
    ).expect("public byte route");
    let hex_path = evaluate_wcag22_hex(
        "#767676",
        "#FFFFFF",
        Wcag22CriterionV1::Sc143TextDefault,
    ).expect("public hex route");
    assert_eq!(byte_path, hex_path);
    let (decision, evidence) = decision_and_evidence(&hex_path);
    assert_eq!(decision, Wcag22ApplicableDecisionV1::Pass);
    let NumericalDecisionEvidenceV1::CanonicalFiniteBounded(payload) = evidence else {
        panic!("public WCAG route returned the wrong evidence class");
    };
    assert_eq!(payload.artifact_id().key(), "wcag22-srgb8-luminance-q55-v1");
    assert_eq!(payload.bound_id().key(), "wcag22-srgb8-outward-q55-v1");
    assert_eq!(payload.proof_id().key(), "wcag22-srgb8-full-domain-q55-v1");

    let default_119 = evaluate_wcag22_hex(
        "#777777",
        "#FFFFFF",
        Wcag22CriterionV1::Sc143TextDefault,
    ).expect("4.5 public boundary");
    assert_eq!(
        decision_and_evidence(&default_119).0,
        Wcag22ApplicableDecisionV1::Fail,
    );
    let large_119 = evaluate_wcag22_srgb8(
        [119_u8; 3],
        white,
        Wcag22CriterionV1::Sc143TextLargeScale,
    ).expect("3.0 criterion discriminator");
    assert_eq!(
        decision_and_evidence(&large_119).0,
        Wcag22ApplicableDecisionV1::Pass,
    );

    for invalid in ["#GGGGGG", "#€€", "##000000"] {
        assert!(matches!(
            evaluate_wcag22_hex(
                invalid,
                "#FFFFFF",
                Wcag22CriterionV1::Sc143TextDefault,
            ),
            Err(Wcag22EvaluationErrorV1::InvalidSrgb8 {
                field: "foreground",
                ..
            })
        ));
    }
}

fn main() {
    verify_public_route();
    let mut matches = numerical_registry_v2()
        .iter()
        .filter(|row| row.site_id == NumericalSiteIdV2::Wcag22Srgb8ContrastV1);
    let row = matches.next().expect("WCAG22 registry row");
    assert!(matches.next().is_none(), "duplicate WCAG22 registry row");

    println!("site_id\t{}", hex_key(row.site_id.key()));
    emit_keys!("stable_outcomes", row.stable_outcomes);
    emit_keys!("compatibility_releases", row.compatibility_releases);
    emit_keys!("evidence_classes", row.evidence_classes);
    emit_keys!("artifact_ids", row.artifact_ids);
    emit_keys!("bound_ids", row.bound_ids);
    emit_keys!("proof_ids", row.proof_ids);
    emit_keys!("runtime_attestations", row.runtime_attestations);
    println!("bound_status\t{}", hex_key(row.bound_status.key()));
    println!("fallback_status\t{}", hex_key(row.fallback_status.key()));
}
"""


def cargo_executable() -> str:
    configured = os.environ.get("CARGO")
    if configured:
        return configured
    discovered = shutil.which("cargo")
    if discovered:
        return discovered
    candidates = [Path.home() / ".cargo/bin/cargo"]
    candidates.extend(
        sorted(
            Path("/opt/homebrew/Cellar/rustup").glob("*/bin/cargo"),
            reverse=True,
        )
    )
    for candidate in candidates:
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return str(candidate)
    raise AssertionError(
        "typed WCAG registry proof requires Cargo; set CARGO to its executable"
    )


def verify_canonical_crate_target(
    *,
    manifest_path: Path = CORE_MANIFEST,
    expected_source: Path = CRATE_ROOT_ROUTE_SOURCE,
    logical_source: bytes = CANONICAL_CRATE_TARGET,
) -> bytes:
    """Use Cargo's own metadata model to reject a redirected library root."""
    cargo = cargo_executable()
    environment = os.environ.copy()
    environment["PATH"] = (
        str(Path(cargo).parent)
        + os.pathsep
        + environment.get("PATH", "")
    )
    completed = subprocess.run(
        [
            cargo,
            "metadata",
            "--format-version=1",
            "--no-deps",
            "--offline",
            "--manifest-path",
            str(manifest_path),
        ],
        cwd=manifest_path.parent,
        env=environment,
        check=True,
        capture_output=True,
        text=True,
    )
    metadata = json.loads(completed.stdout)
    canonical_manifest = manifest_path.resolve()
    packages = [
        package
        for package in metadata["packages"]
        if Path(package["manifest_path"]).resolve() == canonical_manifest
    ]
    assert len(packages) == 1, "Cargo metadata did not identify one core package"
    library_targets = [
        target
        for target in packages[0]["targets"]
        if "lib" in target["kind"]
    ]
    assert len(library_targets) == 1, "Cargo metadata did not identify one lib target"
    actual_source = Path(library_targets[0]["src_path"])
    assert not expected_source.is_symlink(), "canonical crate root must not be a symlink"
    assert actual_source.resolve() == expected_source.resolve(), (
        "crate root redirect: "
        f"Cargo compiles {actual_source}, expected {expected_source}"
    )
    return logical_source


def decode_registry_probe_key(encoded: str) -> str:
    assert re.fullmatch(r"(?:[0-9a-f]{2})*", encoded), (
        f"non-canonical typed registry key encoding: {encoded!r}"
    )
    try:
        return bytes.fromhex(encoded).decode("utf-8")
    except UnicodeDecodeError as error:
        raise AssertionError("typed registry key is not UTF-8") from error


def parse_registry_probe_output(
    output: str,
) -> dict[str, str | tuple[str, ...]]:
    expected_fields = (
        "site_id",
        *REGISTRY_ROW_SET_FIELDS,
        "bound_status",
        "fallback_status",
    )
    lines = output.splitlines()
    assert len(lines) == len(expected_fields), (
        f"typed registry probe line-count drifted: {len(lines)}"
    )
    row: dict[str, str | tuple[str, ...]] = {}
    for expected_field, line in zip(expected_fields, lines):
        parts = line.split("\t")
        assert parts[0] == expected_field, (
            f"typed registry probe field-order drift: {line!r}"
        )
        if expected_field in REGISTRY_ROW_SET_FIELDS:
            assert len(parts) >= 2 and parts[1].isdigit(), (
                f"malformed typed registry set line: {line!r}"
            )
            count = int(parts[1])
            assert parts[1] == str(count) and len(parts) == count + 2, (
                f"typed registry set count drift: {line!r}"
            )
            values = tuple(decode_registry_probe_key(value) for value in parts[2:])
            assert len(values) == len(set(values)), (
                f"duplicate typed registry key in {expected_field}: {values!r}"
            )
            row[expected_field] = values
        else:
            assert len(parts) == 2, f"malformed typed registry scalar line: {line!r}"
            row[expected_field] = decode_registry_probe_key(parts[1])
    return row


def verify_registry_transport_negative_controls(output: str) -> int:
    """The probe transport must preserve punctuation and reject extra items."""
    controls = 0
    lines = output.splitlines()
    stable_index = 1

    punctuation = lines.copy()
    punctuation[stable_index] += "2c"
    punctuation_row = parse_registry_probe_output("\n".join(punctuation) + "\n")
    assert punctuation_row["stable_outcomes"] == (
        "canonical-finite-bounded,",
    )
    controls += 1

    trailing_empty = lines.copy()
    trailing_empty[stable_index] += "\t"
    try:
        parse_registry_probe_output("\n".join(trailing_empty) + "\n")
    except AssertionError as error:
        if not str(error).startswith("typed registry set count drift:"):
            raise AssertionError(
                "registry transport mutation missed the count guard"
            ) from error
    else:
        raise AssertionError("registry transport accepted a trailing empty item")
    controls += 1
    return controls


def load_live_registry_row() -> tuple[
    dict[str, str | tuple[str, ...]], int
]:
    """Read the runtime-expanded typed row; Python owns the proof encoding."""
    with tempfile.TemporaryDirectory(prefix="labcolors-wcag22-registry-") as temp:
        root = Path(temp)
        source = root / "src"
        source.mkdir()
        core_path = REPO_ROOT / "crates/labcolors-core"
        (root / "Cargo.toml").write_text(
            textwrap.dedent(
                f"""
                [package]
                name = "labcolors-wcag22-registry-probe"
                version = "0.0.0"
                edition = "2024"
                publish = false

                [workspace]

                [dependencies]
                labcolors-core = {{ path = {json.dumps(str(core_path))} }}
                """
            ).lstrip(),
            encoding="utf-8",
        )
        (source / "main.rs").write_text(RUST_REGISTRY_PROBE, encoding="utf-8")
        environment = os.environ.copy()
        environment.setdefault(
            "CARGO_TARGET_DIR",
            str(REPO_ROOT / "target/wcag22-registry-probe"),
        )
        cargo = cargo_executable()
        environment["PATH"] = (
            str(Path(cargo).parent)
            + os.pathsep
            + environment.get("PATH", "")
        )
        command = [
            cargo,
            "run",
            "--quiet",
            "--offline",
            "--manifest-path",
            str(root / "Cargo.toml"),
        ]
        completed = subprocess.run(
            command,
            cwd=REPO_ROOT,
            env=environment,
            check=False,
            capture_output=True,
            text=True,
            timeout=120,
        )
        assert completed.returncode == 0, (
            "typed WCAG registry probe failed: " + completed.stderr[-2000:]
        )

    row = parse_registry_probe_output(completed.stdout)
    transport_controls = verify_registry_transport_negative_controls(
        completed.stdout
    )
    return row, transport_controls


def canonical_registry_row_preimage(
    row: dict[str, str | tuple[str, ...]],
) -> bytes:
    preimage = bytearray(length_prefixed(REGISTRY_ROW_BINDING_DOMAIN))
    preimage.extend(struct.pack("<I", REGISTRY_ROW_BINDING_SCHEMA_VERSION))
    site_id = row["site_id"]
    assert isinstance(site_id, str)
    preimage.extend(length_prefixed(site_id.encode("utf-8")))
    for field in REGISTRY_ROW_SET_FIELDS:
        values = row[field]
        assert isinstance(values, tuple)
        assert len(values) == len(set(values)), f"duplicate registry key in {field}"
        ordered = sorted(values, key=lambda value: value.encode("utf-8"))
        preimage.extend(struct.pack("<I", len(ordered)))
        for value in ordered:
            preimage.extend(length_prefixed(value.encode("utf-8")))
    for field in ("bound_status", "fallback_status"):
        value = row[field]
        assert isinstance(value, str)
        preimage.extend(length_prefixed(value.encode("utf-8")))
    return bytes(preimage)


def verify_registry_binding(
    row: dict[str, str | tuple[str, ...]],
) -> str:
    for field, expected in EXPECTED_WCAG_REGISTRY_ROW.items():
        actual = row.get(field)
        assert actual == expected, (
            f"WCAG registry admission drift at {field}: "
            f"actual={actual!r}, expected={expected!r}"
        )
    preimage = canonical_registry_row_preimage(row)

    # Independent byte-law guards: set order is irrelevant and duplicates fail.
    synthetic = dict(row)
    synthetic["stable_outcomes"] = ("z", "a")
    reversed_synthetic = dict(synthetic)
    reversed_synthetic["stable_outcomes"] = ("a", "z")
    assert canonical_registry_row_preimage(synthetic) == (
        canonical_registry_row_preimage(reversed_synthetic)
    )
    duplicate = dict(row)
    duplicate["stable_outcomes"] = ("duplicate", "duplicate")
    try:
        canonical_registry_row_preimage(duplicate)
    except AssertionError:
        pass
    else:
        raise AssertionError("registry admission preimage accepted a duplicate key")

    digest = hashlib.sha256(preimage).hexdigest()
    assert digest == EXPECTED_REGISTRY_ROW_SHA256, (
        f"WCAG registry admission preimage drifted: "
        f"{digest} != {EXPECTED_REGISTRY_ROW_SHA256}"
    )
    return digest


def verify_registry_negative_controls(
    row: dict[str, str | tuple[str, ...]],
) -> int:
    """Every mint-relevant field must independently invalidate admission."""
    controls = 0
    for field, value in EXPECTED_WCAG_REGISTRY_ROW.items():
        mutated = dict(row)
        if isinstance(value, tuple):
            mutated[field] = (*value, "negative-control")
        else:
            mutated[field] = f"{value}-negative-control"
        try:
            verify_registry_binding(mutated)
        except AssertionError as error:
            expected = f"WCAG registry admission drift at {field}:"
            if not str(error).startswith(expected):
                raise AssertionError(
                    f"negative control for {field} missed admission comparison"
                ) from error
        else:
            raise AssertionError(
                f"negative control: WCAG registry {field} drift was accepted"
            )
        controls += 1
    return controls


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
    facade_digest = verify_public_facade()
    source_route_digest = verify_source_routes()
    registry_row, registry_transport_controls = load_live_registry_row()
    registry_row_digest = verify_registry_binding(registry_row)
    registry_negative_controls = verify_registry_negative_controls(registry_row)
    assert metadata["profile_source_sha256"] == profile_digest, (
        "profile digest drift: "
        f"artifact={metadata['profile_source_sha256']}, source={profile_digest}"
    )
    assert metadata["generator_sha256"] == generator_digest, (
        "generator digest drift: "
        f"artifact={metadata['generator_sha256']}, source={generator_digest}"
    )

    tables, decimal_report = verify_rows(metadata, committed_rows)
    numerical_negative_controls = verify_negative_controls(metadata, committed_rows)
    rows_elapsed = time.perf_counter() - started

    intervals, generated = build_unique_color_intervals(tables)
    domain_elapsed = time.perf_counter() - started
    max_width = sum(max(upper - lower for lower, upper in table) for table in tables)
    assert max_width <= PACK_WIDTH_MASK
    integer_replay_envelope = verify_signed64_replay_envelope(max_width)

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
        "schema_version": 2,
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
        "source_binding_schema_version": SOURCE_BINDING_SCHEMA_VERSION,
        "source_binding_law": SOURCE_BINDING_LAW,
        "source_route_sha256": source_route_digest,
        "registry_row_id": EXPECTED_WCAG_REGISTRY_ROW["site_id"],
        "registry_row_sha256": registry_row_digest,
        "registry_row_negative_controls": registry_negative_controls,
        "declared_operation_law": DECLARED_OPERATION_LAW,
        "generator_sha256": generator_digest,
        "verifier_sha256": verifier_digest,
        "q55_scale": Q,
        "rows": len(committed_rows),
        "row_oracle": decimal_report,
        "colors": generated,
        "unique_intervals": len(intervals),
        "max_color_interval_width": max_width,
        "integer_replay_envelope": integer_replay_envelope,
        "negative_controls": (
            numerical_negative_controls
            + registry_negative_controls
            + registry_transport_controls
        ),
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
