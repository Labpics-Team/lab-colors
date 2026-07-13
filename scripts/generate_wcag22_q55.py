#!/usr/bin/env python3
"""Generate the canonical WCAG 2.2 sRGB8 Q55 contribution artifact.

The generator uses only Python integers.  It never rounds a floating-point
transfer function: low-branch rows are rational division; high-branch rows use
the exact fifth-power comparison from issue #284.  Output is Rust source on
stdout so regeneration can be diffed before replacing the committed artifact.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
from fractions import Fraction
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
PROFILE_PATH = REPO_ROOT / "crates/labcolors-core/contracts/wcag22-srgb8-v1.json"
PROFILE_BYTES = PROFILE_PATH.read_bytes()
PROFILE = json.loads(PROFILE_BYTES)


def exact(key: str) -> Fraction:
    value = PROFILE[key]
    if not isinstance(value, str):
        raise TypeError(f"profile field {key} must be an exact decimal string")
    return Fraction(value)


Q = 1 << int(PROFILE["fixedPointScalePower"])
SPLIT = exact("channelSplit")
DIVISOR = exact("linearDivisor")
OFFSET = exact("encodedOffset")
ENCODED_SCALE = exact("encodedScale")
EXPONENT = exact("encodedExponent")
if EXPONENT != Fraction(12, 5):
    raise ValueError(f"unsupported exact exponent: {EXPONENT}")
WEIGHTS = tuple(exact(key) for key in ("redWeight", "greenWeight", "blueWeight"))
if sum(WEIGHTS, Fraction()) != 1:
    raise ValueError("WCAG luminance weights must sum exactly to one")

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


def profile_checksum() -> str:
    def framed(value: bytes) -> bytes:
        return struct.pack("<I", len(value)) + value

    preimage = bytearray(framed(PROFILE_CHECKSUM_DOMAIN))
    preimage.extend(struct.pack("<I", PROFILE["schemaVersion"]))
    for key in PROFILE_CHECKSUM_FIELDS:
        value = str(PROFILE[key]).encode("utf-8")
        preimage.extend(framed(key.encode("utf-8")))
        preimage.extend(framed(value))
    value = 0x811C9DC5
    for byte in preimage:
        value = ((value ^ byte) * 0x01000193) & 0xFFFFFFFF
    return f"{value:08x}"


def weighted_bounds(weight: Fraction, code: int) -> tuple[int, int]:
    """Return tight integer floor/ceil of Q * weight * linearize(code)."""
    encoded = Fraction(code, 255)
    if encoded <= SPLIT:
        contribution = Q * weight * encoded / DIVISOR
        lower, remainder = divmod(contribution.numerator, contribution.denominator)
        upper = lower + (remainder != 0)
    else:
        # For exponent 12/5, compare fifth powers entirely with integers.
        base = (encoded + OFFSET) / ENCODED_SCALE
        right = Q**5 * weight.numerator**5 * base.numerator**12
        left_factor = weight.denominator**5 * base.denominator**12
        lo, hi = 0, Q
        while lo < hi:
            midpoint = (lo + hi + 1) // 2
            if midpoint**5 * left_factor <= right:
                lo = midpoint
            else:
                hi = midpoint - 1
        lower = lo
        is_exact = lower**5 * left_factor == right
        upper = lower if is_exact else lower + 1
        assert lower**5 * left_factor <= right
        assert (lower + 1) ** 5 * left_factor > right

    assert 0 <= lower <= upper <= Q
    assert upper - lower <= 1
    return lower, upper


def generate() -> tuple[list[list[tuple[int, int]]], bytes, str]:
    tables = [[weighted_bounds(weight, code) for code in range(256)] for weight in WEIGHTS]
    canonical = b"".join(
        struct.pack("<QQ", lower, upper)
        for table in tables
        for lower, upper in table
    )
    return tables, canonical, hashlib.sha256(canonical).hexdigest()


def emit(artifact_path: Path | None) -> None:
    tables, canonical, digest = generate()
    if artifact_path is not None:
        artifact_path.parent.mkdir(parents=True, exist_ok=True)
        artifact_path.write_bytes(canonical)
    profile_digest = hashlib.sha256(PROFILE_BYTES).hexdigest()
    generator_digest = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    print("//! Generated WCAG 2.2 sRGB8 Q55 weighted contribution bounds.")
    print("//!")
    print("//! DO NOT EDIT: regenerate with `python3 scripts/generate_wcag22_q55.py`.")
    print("//! Canonical digest covers each lower/upper u64 in channel/code order,")
    print("//! little-endian, without Rust formatting.")
    print()
    print(f"pub(crate) const Q55_SCALE: u64 = {Q};")
    print("pub(crate) const PROFILE_CHECKSUM: &str =")
    print(f'    "{profile_checksum()}";')
    print("pub(crate) const PROFILE_SOURCE_SHA256: &str =")
    print(f'    "{profile_digest}";')
    print("pub(crate) const GENERATOR_SHA256: &str =")
    print(f'    "{generator_digest}";')
    print("pub(crate) const ARTIFACT_SHA256: &str =")
    print(f'    "{digest}";')
    print("#[rustfmt::skip]")
    print("pub(crate) static WEIGHTED_CONTRIBUTION_BOUNDS: [[[u64; 2]; 256]; 3] = [")
    for weight, table in zip(WEIGHTS, tables):
        print(f"    [ // exact weight {weight.numerator}/{weight.denominator}")
        for offset in range(0, 256, 4):
            cells = ", ".join(f"[{lo}, {hi}]" for lo, hi in table[offset : offset + 4])
            print(f"        {cells},")
        print("    ],")
    print("];")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--artifact",
        type=Path,
        help="also write the canonical 1536-word little-endian artifact",
    )
    emit(parser.parse_args().artifact)
