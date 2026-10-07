#!/usr/bin/env python3
"""Independent exact lexical/IEEE-754 audit. No producer code imported."""
import hashlib
import json
import re
import sys
from fractions import Fraction as Q
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def rational_bits(bits):
    sign = -1 if bits >> 63 else 1
    exponent = (bits >> 52) & 2047
    mantissa = bits & ((1 << 52) - 1)
    require(exponent != 2047, "nonfinite bit pattern")
    if exponent == 0:
        return sign * Q(mantissa, 1 << 1074)
    mantissa += 1 << 52
    power = exponent - 1075
    return sign * (Q(mantissa << power) if power >= 0 else Q(mantissa, 1 << -power))


def nearest_even(decimal, bits):
    candidate = rational_bits(bits)
    lower = rational_bits(bits - 1) if bits else -Q(1, 1 << 1074)
    upper = rational_bits(bits + 1)
    midpoint_lo = (lower + candidate) / 2
    midpoint_hi = (candidate + upper) / 2
    return (midpoint_lo < decimal < midpoint_hi or
            bits % 2 == 0 and decimal in (midpoint_lo, midpoint_hi))


bridge = json.loads((ROOT / "input/bridge.json").read_text(encoding='utf-8'))
sources = {"decode": ("gamma_data.rs", "DECODE_8BIT", 256),
           "matrix": ("srgb.rs", "SRGB_TO_XYZ_D65", 9)}
observed = []
rejected_mutants = 0
for group, (filename, declaration, expected_count) in sources.items():
    path = ROOT / "input" / filename
    data = path.read_bytes()
    binding, = (item for item in bridge["bindings"] if item["path"].endswith("/" + filename))
    require(hashlib.sha256(data).hexdigest() == binding["sha256"], "source SHA256")
    blob = b"blob " + str(len(data)).encode() + b"\0" + data
    require(hashlib.sha1(blob).hexdigest() == binding["git_blob_sha1"], "source Git blob")
    source = data.decode(encoding='utf-8')
    initializer, = re.findall(r"\b" + declaration + r"\s*:[^=]+?=\s*\[(.*?)\];", source, re.S)
    tokens = re.findall(r"\d[\d_]*(?:\.[\d_]*)?(?:[eE][+-]?\d+)?", initializer)
    require(len(tokens) == expected_count, "complete source lexical census")
    rows = sorted((row for row in bridge["rows"] if row["group"] == group), key=lambda row: row["index"])
    require([row["index"] for row in rows] == list(range(expected_count)), "bridge index census")
    for token, row in zip(tokens, rows):
        require(token.replace("_", "") == row["token"].replace("_", ""), "source token identity")
        exact = Q(token.replace("_", ""))
        bits = int(row["bits_hex"], 16)
        require(0 <= bits < 1 << 63, "nonnegative finite source")
        require(nearest_even(exact, bits), "exact nearest-even rounding")
        require(rational_bits(bits) == Q(int(row["dyadic"]["numerator"]), int(row["dyadic"]["denominator"])), "declared exact dyadic")
        require(exact == Q(int(row["decimal"]["numerator"]), int(row["decimal"]["denominator"])), "declared exact decimal")
        for wrong in ([bits + 1] if bits == 0 else [bits - 1, bits + 1]):
            require(not nearest_even(exact, wrong), "adjacent bit mutation survived")
            rejected_mutants += 1
    observed.append({"group": group, "source_sha256": binding["sha256"], "literals": expected_count})

require(len(bridge["rows"]) == sum(item["literals"] for item in observed), "no unreviewed bridge rows")
result = {"verdict": "PASS", "claim": "The pinned current Rust source literals round exactly to all 265 bridge dyadics under IEEE-754 binary64 nearest-even", "arithmetic": "integer bit decomposition and fractions.Fraction; no floating conversion", "groups": observed, "adjacent_wrong_bit_patterns_rejected": rejected_mutants, "non_claims": ["Compiler artifact execution", "Exact algebraic IEC transfer", "Historical ac6 identity", "Sato primary-source reconstruction", "Physical or human validation"]}
(ROOT / sys.argv[1] / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding='utf-8', newline='\n')
print(json.dumps(result))
