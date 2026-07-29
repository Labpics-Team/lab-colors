#!/usr/bin/env python3
"""Canonical offline protocol; intentionally contains no colour evaluator."""

from __future__ import annotations

import hashlib
from dataclasses import dataclass, field, fields
from enum import IntEnum, StrEnum
from fractions import Fraction
from functools import cached_property
from itertools import pairwise
from typing import Callable, Iterable, Iterator, NoReturn, TypeAlias


FORMULA_RELEASE_DOMAIN_V1 = b"labcolors.nominal-exact-real-lift.ascii-ssa.v1\0"
FORMULA_RELEASE_V1 = bytes.fromhex(
    "2c626d8ee60eeb62ae4db53660d61bbc25e0efd4e557f0dc1e77565c130b6e52"
)
DEFINITION_DOMAIN_V1 = b"labcolors.contextual-region-family-provider.v1\0"
OUTPUT_CARDINALITY_V1 = 1 << 24
FORMULA_SPEC_BYTES_V1 = 24_434
DEFINITION_PREFIX_LENGTHS_V1 = (
    len(DEFINITION_DOMAIN_V1),
    1,
    1,
    1,
    1,
    1,
    1,
    4,
    1,
    1,
    4,
    8,
    8,
    1,
    1,
    1,
    32,
    1,
    8,
    8,
    8,
    8,
)

DOMAIN_MAGIC_V1 = b"LCDOM1\0\0"
POLICY_MAGIC_V1 = b"LCPOL1\0\0"
JOB_MAGIC_V1 = b"LCJOB1\0\0"
MANIFEST_MAGIC_V1 = b"LCMAN1\0\0"
TRANSCRIPT_MAGIC_V1 = b"LCTRN1\0\0"
RUN_CLAIM_MAGIC_V1 = b"LCRUN1\0\0"
PROVENANCE_CLAIM_MAGIC_V1 = b"LCPRV1\0\0"
COMPARISON_MAGIC_V1 = b"LCCMP1\0\0"

DOMAIN_ID_LABEL_V1 = b"labcolors.proof-region.domain.v1\0"
POLICY_ID_LABEL_V1 = b"labcolors.proof-region.policy.v1\0"
JOB_ID_LABEL_V1 = b"labcolors.proof-region.job.v1\0"
MANIFEST_ID_LABEL_V1 = b"labcolors.proof-region.comparator-manifest.v1\0"
TRANSCRIPT_ID_LABEL_V1 = b"labcolors.proof-region.transcript.v1\0"
RUN_CLAIM_ID_LABEL_V1 = b"labcolors.proof-region.run-claim.v1\0"
PROVENANCE_CLAIM_ID_LABEL_V1 = b"labcolors.proof-region.evaluator-provenance-claim.v1\0"
COMPARISON_ID_LABEL_V1 = b"labcolors.proof-region.dual-comparison.v1\0"


class ProtocolReasonV1(StrEnum):
    BAD_MAGIC = "bad_magic"
    TRUNCATED = "truncated"
    TRAILING_BYTES = "trailing_bytes"
    LENGTH_OUT_OF_BOUNDS = "length_out_of_bounds"
    COUNT_MISMATCH = "count_mismatch"
    UNKNOWN_RELEASE = "unknown_release"
    INVALID_DEFINITION = "invalid_definition"
    DIGEST_MISMATCH = "digest_mismatch"
    EMPTY_DOMAIN = "empty_domain"
    NONCANONICAL_ORDER = "noncanonical_order"
    OVERLAPPING_RANGE = "overlapping_range"
    ADJACENT_RANGE = "adjacent_range"
    INVALID_RANGE = "invalid_range"
    INVALID_POLICY = "invalid_policy"
    INVALID_MANIFEST = "invalid_manifest"
    INVALID_DIGEST = "invalid_digest"
    INVALID_TRANSCRIPT = "invalid_transcript"
    MISSING_EQUALITY_WITNESS = "missing_equality_witness"
    FOREIGN_BINDING = "foreign_binding"
    SHARED_DIVERSITY_COORDINATE = "shared_diversity_coordinate"
    UNRESOLVED_TRANSCRIPT = "unresolved_transcript"
    DISAGREEMENT = "disagreement"


@dataclass(frozen=True)
class ProtocolErrorV1(ValueError):
    artifact: str
    offset: int
    reason: ProtocolReasonV1
    detail: str

    def __str__(self) -> str:
        return f"{self.artifact}@{self.offset}: {self.reason}: {self.detail}"


def _fail(
    artifact: str,
    offset: int,
    reason: ProtocolReasonV1,
    detail: str,
) -> NoReturn:
    raise ProtocolErrorV1(artifact, offset, reason, detail)


def _formula_release_v1(specification: bytes) -> bytes:
    return hashlib.sha256(
        FORMULA_RELEASE_DOMAIN_V1
        + len(specification).to_bytes(8, "big")
        + specification
    ).digest()


def _identity(label: bytes, encoded: bytes) -> bytes:
    hasher = hashlib.sha256()
    hasher.update(label)
    hasher.update(len(encoded).to_bytes(8, "big"))
    hasher.update(encoded)
    return hasher.digest()


def _identity_from_chunks(
    label: bytes,
    encoded_length: int,
    chunks: Iterable[bytes | memoryview],
) -> bytes:
    hasher = hashlib.sha256()
    hasher.update(label)
    hasher.update(encoded_length.to_bytes(8, "big"))
    consumed = 0
    for chunk in chunks:
        hasher.update(chunk)
        consumed += len(chunk)
    if consumed != encoded_length:
        _fail("identity-v1", 0, ProtocolReasonV1.COUNT_MISMATCH, "identity chunk length drift")
    return hasher.digest()


def _require_digest(value: bytes, artifact: str, field: str) -> bytes:
    if type(value) is not bytes or len(value) != 32 or value == bytes(32):
        _fail(artifact, 0, ProtocolReasonV1.INVALID_DIGEST, f"invalid {field} digest")
    return value


class _Reader:
    def __init__(self, data: bytes, artifact: str):
        if type(data) is not bytes:
            raise TypeError("canonical wire input must be owned bytes")
        self.data = data
        self.artifact = artifact
        self.offset = 0

    @property
    def remaining(self) -> int:
        return len(self.data) - self.offset

    def exact(self, length: int) -> bytes:
        if length < 0 or length > self.remaining:
            _fail(
                self.artifact,
                self.offset,
                ProtocolReasonV1.TRUNCATED,
                f"need {length} bytes, have {self.remaining}",
            )
        start = self.offset
        self.offset += length
        return self.data[start : self.offset]

    def span_exact(self, length: int) -> tuple[int, int]:
        if length < 0 or length > self.remaining:
            _fail(
                self.artifact,
                self.offset,
                ProtocolReasonV1.TRUNCATED,
                f"need {length} bytes, have {self.remaining}",
            )
        start = self.offset
        self.offset += length
        return start, self.offset

    def magic(self, expected: bytes) -> None:
        start = self.offset
        actual = self.exact(len(expected))
        if actual != expected:
            _fail(self.artifact, start, ProtocolReasonV1.BAD_MAGIC, "wire magic mismatch")

    def u8(self) -> int:
        return self.exact(1)[0]

    def u32(self) -> int:
        return int.from_bytes(self.exact(4), "big")

    def u64(self) -> int:
        return int.from_bytes(self.exact(8), "big")

    def blob(
        self,
        *,
        exact_length: int | None = None,
        maximum_length: int | None = None,
    ) -> bytes:
        length_offset = self.offset
        length = self.u64()
        if exact_length is not None and length != exact_length:
            _fail(
                self.artifact,
                length_offset,
                ProtocolReasonV1.LENGTH_OUT_OF_BOUNDS,
                f"declared {length}, expected {exact_length}",
            )
        if maximum_length is not None and length > maximum_length:
            _fail(
                self.artifact,
                length_offset,
                ProtocolReasonV1.LENGTH_OUT_OF_BOUNDS,
                f"declared {length}, V1 cap {maximum_length}",
            )
        if length > self.remaining:
            _fail(
                self.artifact,
                length_offset,
                ProtocolReasonV1.LENGTH_OUT_OF_BOUNDS,
                f"declared {length}, remaining {self.remaining}",
            )
        return self.exact(length)

    def finish(self) -> None:
        if self.remaining:
            _fail(
                self.artifact,
                self.offset,
                ProtocolReasonV1.TRAILING_BYTES,
                f"{self.remaining} bytes after canonical object",
            )


def _blob(value: bytes) -> bytes:
    return len(value).to_bytes(8, "big") + value


def _dyadic(bits: bytes, artifact: str, field: str) -> Fraction:
    if len(bits) != 8:
        _fail(artifact, 0, ProtocolReasonV1.INVALID_DEFINITION, f"{field} is not binary64")
    raw = int.from_bytes(bits, "big")
    sign = -1 if raw >> 63 else 1
    exponent = (raw >> 52) & 0x7FF
    fraction = raw & ((1 << 52) - 1)
    if exponent == 0x7FF or raw == (1 << 63):
        _fail(artifact, 0, ProtocolReasonV1.INVALID_DEFINITION, f"invalid {field} binary64")
    if exponent == 0:
        significand = fraction
        power = -1074
    else:
        significand = (1 << 52) | fraction
        power = exponent - 1075
    numerator = sign * significand
    return Fraction(numerator << power, 1) if power >= 0 else Fraction(numerator, 1 << -power)


def encode_contextual_definition_fields_v1(fields_: tuple[bytes, ...]) -> bytes:
    return b"".join(_blob(value) for value in fields_)


@dataclass(frozen=True)
class ContextualRegionDefinitionV1:
    fields: tuple[bytes, ...]
    knot_count: int

    def __post_init__(self) -> None:
        self._validate()

    @classmethod
    def parse(cls, data: bytes) -> "ContextualRegionDefinitionV1":
        reader = _Reader(data, "contextual-definition-v1")
        parsed = [
            reader.blob(exact_length=length)
            for length in DEFINITION_PREFIX_LENGTHS_V1
        ]
        knot_count = int.from_bytes(parsed[21], "big")
        required = knot_count * 4 * 16
        if reader.remaining < required:
            _fail(
                reader.artifact,
                reader.offset,
                ProtocolReasonV1.COUNT_MISMATCH,
                "knot count exceeds available canonical fields",
            )
        if reader.remaining > required:
            _fail(
                reader.artifact,
                reader.offset + required,
                ProtocolReasonV1.TRAILING_BYTES,
                "bytes remain after declared knots",
            )
        for _ in range(knot_count * 4):
            parsed.append(reader.blob(exact_length=8))
        reader.finish()
        result = cls(tuple(parsed), knot_count)
        if result.encode() != data:
            _fail(reader.artifact, 0, ProtocolReasonV1.INVALID_DEFINITION, "noncanonical re-encode")
        return result

    def _validate(self) -> None:
        artifact = "contextual-definition-v1"
        if (
            type(self.knot_count) is not int
            or self.knot_count <= 0
            or type(self.fields) is not tuple
            or len(self.fields) != 22 + 4 * self.knot_count
            or any(type(field) is not bytes for field in self.fields)
        ):
            _fail(artifact, 0, ProtocolReasonV1.COUNT_MISMATCH, "invalid knot cardinality")
        if (
            tuple(len(field) for field in self.fields[:22])
            != DEFINITION_PREFIX_LENGTHS_V1
            or any(
                len(self.fields[index]) != 8
                for index in range(22, len(self.fields))
            )
            or int.from_bytes(self.fields[21], "big") != self.knot_count
        ):
            _fail(artifact, 0, ProtocolReasonV1.COUNT_MISMATCH, "definition field lengths or knot count drifted")
        if self.fields[0] != DEFINITION_DOMAIN_V1:
            _fail(artifact, 0, ProtocolReasonV1.UNKNOWN_RELEASE, "foreign definition domain")
        for index in (1, 2, 3, 4, 5, 6, 8, 9, 14, 15, 17):
            if self.fields[index] != b"\x01":
                _fail(artifact, 0, ProtocolReasonV1.UNKNOWN_RELEASE, f"foreign tag {index}")
        if self.fields[7] != b"\x01\x01\x01\x01" or self.fields[10] != b"\x01\x01\x01\x01":
            _fail(artifact, 0, ProtocolReasonV1.UNKNOWN_RELEASE, "foreign colorimetric frame")
        if self.fields[13] not in (b"\x01", b"\x02", b"\x03"):
            _fail(artifact, 0, ProtocolReasonV1.UNKNOWN_RELEASE, "foreign surround profile")
        if self.fields[16] != FORMULA_RELEASE_V1:
            _fail(artifact, 0, ProtocolReasonV1.UNKNOWN_RELEASE, "foreign formula release")
        adapting_luminance = _dyadic(self.fields[11], artifact, "adapting_luminance")
        background_ratio = _dyadic(self.fields[12], artifact, "background_ratio")
        if adapting_luminance <= 0:
            _fail(artifact, 0, ProtocolReasonV1.INVALID_DEFINITION, "adapting luminance must be positive")
        if background_ratio <= 0 or background_ratio > 1:
            _fail(artifact, 0, ProtocolReasonV1.INVALID_DEFINITION, "background ratio must be in (0,1]")
        g00 = _dyadic(self.fields[18], artifact, "g00")
        g01 = _dyadic(self.fields[19], artifact, "g01")
        g11 = _dyadic(self.fields[20], artifact, "g11")
        if g00 <= 0 or g00 * g11 - g01 * g01 <= 0:
            _fail(artifact, 0, ProtocolReasonV1.INVALID_DEFINITION, "shape is not exact SPD")
        previous_tone: Fraction | None = None
        for index in range(self.knot_count):
            base = 22 + 4 * index
            tone = _dyadic(self.fields[base], artifact, f"tone[{index}]")
            _dyadic(self.fields[base + 1], artifact, f"center_a[{index}]")
            _dyadic(self.fields[base + 2], artifact, f"center_b[{index}]")
            radius = _dyadic(self.fields[base + 3], artifact, f"radius_squared[{index}]")
            if previous_tone is not None and tone <= previous_tone:
                _fail(artifact, 0, ProtocolReasonV1.INVALID_DEFINITION, "tones are not strict")
            if radius < 0:
                _fail(artifact, 0, ProtocolReasonV1.INVALID_DEFINITION, "negative radius squared")
            previous_tone = tone

    @property
    def formula_release(self) -> bytes:
        return self.fields[16]

    @cached_property
    def definition_digest(self) -> bytes:
        return hashlib.sha256(self.encode()).digest()

    def encode(self) -> bytes:
        return encode_contextual_definition_fields_v1(self.fields)


def _validate_range_record(
    artifact: str,
    start: int,
    end: int,
    previous: tuple[int, int] | None,
) -> None:
    if start < 0 or start >= end or end > OUTPUT_CARDINALITY_V1:
        _fail(artifact, 0, ProtocolReasonV1.INVALID_RANGE, f"invalid [{start},{end})")
    if previous is not None:
        if start < previous[0]:
            _fail(artifact, 0, ProtocolReasonV1.NONCANONICAL_ORDER, "ranges reordered")
        if start < previous[1]:
            _fail(artifact, 0, ProtocolReasonV1.OVERLAPPING_RANGE, "ranges overlap")
        if start == previous[1]:
            _fail(artifact, 0, ProtocolReasonV1.ADJACENT_RANGE, "adjacent ranges must merge")


def _validate_ranges(ranges: tuple[tuple[int, int], ...], point_count: int) -> None:
    artifact = "reduced-domain-v1"
    if not ranges or point_count == 0:
        _fail(artifact, 0, ProtocolReasonV1.EMPTY_DOMAIN, "domain must be nonempty")
    if type(point_count) is not int or point_count > OUTPUT_CARDINALITY_V1:
        _fail(artifact, 0, ProtocolReasonV1.LENGTH_OUT_OF_BOUNDS, "domain exceeds sRGB8 cardinality")
    total = 0
    previous: tuple[int, int] | None = None
    for start, end in ranges:
        _validate_range_record(artifact, start, end, previous)
        total += end - start
        previous = (start, end)
    if total != point_count:
        _fail(artifact, 0, ProtocolReasonV1.COUNT_MISMATCH, f"ranges cover {total}, declared {point_count}")


@dataclass(frozen=True)
class ReducedDomainManifestV1:
    ranges: tuple[tuple[int, int], ...]
    point_count: int

    def __post_init__(self) -> None:
        if (
            type(self.point_count) is not int
            or type(self.ranges) is not tuple
            or any(
                type(item) is not tuple
                or len(item) != 2
                or any(type(value) is not int for value in item)
                for item in self.ranges
            )
        ):
            _fail("reduced-domain-v1", 0, ProtocolReasonV1.INVALID_RANGE, "domain must use immutable integer ranges")
        _validate_ranges(self.ranges, self.point_count)

    @classmethod
    def from_ordinals(cls, ordinals: Iterable[int]) -> "ReducedDomainManifestV1":
        iterator = iter(ordinals)
        try:
            first = next(iterator)
        except StopIteration:
            _fail(
                "reduced-domain-v1",
                0,
                ProtocolReasonV1.EMPTY_DOMAIN,
                "domain must be nonempty",
            )
        if type(first) is not int or first < 0 or first >= OUTPUT_CARDINALITY_V1:
            _fail("reduced-domain-v1", 0, ProtocolReasonV1.INVALID_RANGE, "ordinal outside sRGB8")
        ranges: list[tuple[int, int]] = []
        start = first
        end = start + 1
        count = 1
        previous = first
        for value in iterator:
            if type(value) is not int or value < 0 or value >= OUTPUT_CARDINALITY_V1:
                _fail("reduced-domain-v1", 0, ProtocolReasonV1.INVALID_RANGE, "ordinal outside sRGB8")
            if value <= previous:
                _fail("reduced-domain-v1", 0, ProtocolReasonV1.NONCANONICAL_ORDER, "ordinals not strict")
            if value == end:
                end += 1
            else:
                ranges.append((start, end))
                start, end = value, value + 1
            previous = value
            count += 1
        ranges.append((start, end))
        return cls(tuple(ranges), count)

    @classmethod
    def parse(cls, data: bytes) -> "ReducedDomainManifestV1":
        reader = _Reader(data, "reduced-domain-v1")
        reader.magic(DOMAIN_MAGIC_V1)
        if reader.u8() != 1:
            _fail(reader.artifact, 8, ProtocolReasonV1.UNKNOWN_RELEASE, "foreign ordinal law")
        point_count = reader.u64()
        if point_count == 0:
            _fail(reader.artifact, reader.offset - 8, ProtocolReasonV1.EMPTY_DOMAIN, "domain must be nonempty")
        if point_count > OUTPUT_CARDINALITY_V1:
            _fail(reader.artifact, reader.offset - 8, ProtocolReasonV1.LENGTH_OUT_OF_BOUNDS, "point count exceeds sRGB8")
        range_count = reader.u64()
        maximum_range_count = min(
            point_count,
            OUTPUT_CARDINALITY_V1 - point_count + 1,
        )
        if range_count == 0 or range_count > maximum_range_count:
            _fail(
                reader.artifact,
                reader.offset - 8,
                ProtocolReasonV1.LENGTH_OUT_OF_BOUNDS,
                "range count cannot represent the declared non-adjacent coverage",
            )
        required = range_count * 8
        if required != reader.remaining:
            reason = ProtocolReasonV1.TRUNCATED if required > reader.remaining else ProtocolReasonV1.TRAILING_BYTES
            _fail(reader.artifact, reader.offset, reason, "range count does not consume exact body")
        ranges_list: list[tuple[int, int]] = []
        total = 0
        previous: tuple[int, int] | None = None
        for _ in range(range_count):
            start = reader.u32()
            end = reader.u32()
            _validate_range_record(reader.artifact, start, end, previous)
            ranges_list.append((start, end))
            total += end - start
            previous = (start, end)
        if total != point_count:
            _fail(
                reader.artifact,
                reader.offset,
                ProtocolReasonV1.COUNT_MISMATCH,
                f"ranges cover {total}, declared {point_count}",
            )
        reader.finish()
        result = cls(tuple(ranges_list), point_count)
        if result.encode() != data:
            _fail(reader.artifact, 0, ProtocolReasonV1.INVALID_RANGE, "noncanonical domain")
        return result

    def encode(self) -> bytes:
        body = bytearray(DOMAIN_MAGIC_V1)
        body.append(1)
        body.extend(self.point_count.to_bytes(8, "big"))
        body.extend(len(self.ranges).to_bytes(8, "big"))
        for start, end in self.ranges:
            body.extend(start.to_bytes(4, "big"))
            body.extend(end.to_bytes(4, "big"))
        return bytes(body)

    @cached_property
    def identity(self) -> bytes:
        return _identity(DOMAIN_ID_LABEL_V1, self.encode())

    def iter_ordinals(self) -> Iterator[int]:
        for start, end in self.ranges:
            yield from range(start, end)

    def index_of(self, ordinal: int) -> int | None:
        index = 0
        for start, end in self.ranges:
            if start <= ordinal < end:
                return index + ordinal - start
            index += end - start
        return None


class ComparatorKindV1(IntEnum):
    ARB = 1
    MPFI = 2


@dataclass(frozen=True)
class ComparatorBudgetV1:
    kind: ComparatorKindV1
    precision_ladder: tuple[int, ...]
    per_point_work: int
    global_pregrant: int

    def __post_init__(self) -> None:
        if (
            type(self.kind) is not ComparatorKindV1
            or type(self.precision_ladder) is not tuple
            or not self.precision_ladder
            or any(
                type(value) is not int or value <= 0 or value > 0xFFFF_FFFF
                for value in self.precision_ladder
            )
        ):
            _fail("proof-policy-v1", 0, ProtocolReasonV1.INVALID_POLICY, "invalid precision ladder")
        if any(left >= right for left, right in pairwise(self.precision_ladder)):
            _fail("proof-policy-v1", 0, ProtocolReasonV1.NONCANONICAL_ORDER, "precision ladder not strict")
        if (
            type(self.per_point_work) is not int
            or self.per_point_work < 0
            or self.per_point_work > 0xFFFF_FFFF_FFFF_FFFF
        ):
            _fail("proof-policy-v1", 0, ProtocolReasonV1.INVALID_POLICY, "invalid point work")
        if (
            type(self.global_pregrant) is not int
            or self.global_pregrant < 0
            or self.global_pregrant > 0xFFFF_FFFF_FFFF_FFFF
        ):
            _fail("proof-policy-v1", 0, ProtocolReasonV1.INVALID_POLICY, "invalid global grant")


@dataclass(frozen=True)
class ProofPolicyV1:
    equality_release: int
    comparators: tuple[ComparatorBudgetV1, ComparatorBudgetV1]

    def __post_init__(self) -> None:
        if (
            type(self.comparators) is not tuple
            or len(self.comparators) != 2
            or any(type(item) is not ComparatorBudgetV1 for item in self.comparators)
        ):
            _fail("proof-policy-v1", 0, ProtocolReasonV1.INVALID_POLICY, "V1 requires two typed comparator budgets")
        if type(self.equality_release) is not int or self.equality_release != 1:
            _fail("proof-policy-v1", 0, ProtocolReasonV1.UNKNOWN_RELEASE, "foreign equality release")
        if tuple(item.kind for item in self.comparators) != (ComparatorKindV1.ARB, ComparatorKindV1.MPFI):
            _fail("proof-policy-v1", 0, ProtocolReasonV1.NONCANONICAL_ORDER, "comparator order is fixed")

    @classmethod
    def parse(cls, data: bytes) -> "ProofPolicyV1":
        reader = _Reader(data, "proof-policy-v1")
        reader.magic(POLICY_MAGIC_V1)
        equality_release = reader.u8()
        if reader.u8() != 2:
            _fail(reader.artifact, 9, ProtocolReasonV1.COUNT_MISMATCH, "V1 requires two comparators")
        budgets: list[ComparatorBudgetV1] = []
        for expected_kind in (ComparatorKindV1.ARB, ComparatorKindV1.MPFI):
            kind_offset = reader.offset
            try:
                kind = ComparatorKindV1(reader.u8())
            except ValueError:
                _fail(reader.artifact, kind_offset, ProtocolReasonV1.UNKNOWN_RELEASE, "unknown comparator")
            if kind != expected_kind:
                _fail(reader.artifact, kind_offset, ProtocolReasonV1.NONCANONICAL_ORDER, "comparator reordered")
            count_offset = reader.offset
            rung_count = reader.u32()
            # The first record must leave its own two u64 budgets plus the
            # complete minimum MPFI record. Otherwise a hostile count could
            # reinterpret the second comparator as precision rungs.
            minimum_tail = 41 if expected_kind == ComparatorKindV1.ARB else 16
            if (
                rung_count == 0
                or rung_count > max(0, (reader.remaining - minimum_tail) // 4)
            ):
                _fail(reader.artifact, count_offset, ProtocolReasonV1.LENGTH_OUT_OF_BOUNDS, "precision count exceeds body")
            ladder = tuple(reader.u32() for _ in range(rung_count))
            budgets.append(ComparatorBudgetV1(kind, ladder, reader.u64(), reader.u64()))
        reader.finish()
        result = cls(equality_release, (budgets[0], budgets[1]))
        if result.encode() != data:
            _fail(reader.artifact, 0, ProtocolReasonV1.INVALID_POLICY, "noncanonical policy")
        return result

    def encode(self) -> bytes:
        body = bytearray(POLICY_MAGIC_V1)
        body.extend((self.equality_release, 2))
        for item in self.comparators:
            body.append(int(item.kind))
            body.extend(len(item.precision_ladder).to_bytes(4, "big"))
            for precision in item.precision_ladder:
                body.extend(precision.to_bytes(4, "big"))
            body.extend(item.per_point_work.to_bytes(8, "big"))
            body.extend(item.global_pregrant.to_bytes(8, "big"))
        return bytes(body)

    @cached_property
    def identity(self) -> bytes:
        return _identity(POLICY_ID_LABEL_V1, self.encode())


@dataclass(frozen=True)
class ProofJobV1:
    definition: ContextualRegionDefinitionV1
    formula_spec: bytes
    domain: ReducedDomainManifestV1
    policy: ProofPolicyV1

    def __post_init__(self) -> None:
        if (
            type(self.definition) is not ContextualRegionDefinitionV1
            or type(self.formula_spec) is not bytes
            or len(self.formula_spec) != FORMULA_SPEC_BYTES_V1
            or type(self.domain) is not ReducedDomainManifestV1
            or type(self.policy) is not ProofPolicyV1
        ):
            _fail("proof-job-v1", 0, ProtocolReasonV1.INVALID_DEFINITION, "job coordinates are not canonical V1 types")
        release = _formula_release_v1(self.formula_spec)
        if release != self.definition.formula_release:
            _fail("proof-job-v1", 0, ProtocolReasonV1.DIGEST_MISMATCH, "formula does not bind definition")

    @classmethod
    def parse(cls, data: bytes) -> "ProofJobV1":
        reader = _Reader(data, "proof-job-v1")
        reader.magic(JOB_MAGIC_V1)
        definition_digest = reader.exact(32)
        definition_bytes = reader.blob()
        definition = ContextualRegionDefinitionV1.parse(definition_bytes)
        if definition.definition_digest != definition_digest:
            _fail(reader.artifact, 8, ProtocolReasonV1.DIGEST_MISMATCH, "definition digest mismatch")
        formula_release = reader.exact(32)
        formula_spec = reader.blob(exact_length=FORMULA_SPEC_BYTES_V1)
        if formula_release != definition.formula_release:
            _fail(reader.artifact, reader.offset, ProtocolReasonV1.DIGEST_MISMATCH, "formula release mismatch")
        domain_identity = reader.exact(32)
        domain = ReducedDomainManifestV1.parse(reader.blob())
        if domain.identity != domain_identity:
            _fail(reader.artifact, reader.offset, ProtocolReasonV1.DIGEST_MISMATCH, "domain identity mismatch")
        policy_identity = reader.exact(32)
        policy = ProofPolicyV1.parse(reader.blob())
        if policy.identity != policy_identity:
            _fail(reader.artifact, reader.offset, ProtocolReasonV1.DIGEST_MISMATCH, "policy identity mismatch")
        reader.finish()
        result = cls(definition, formula_spec, domain, policy)
        if result.encode() != data:
            _fail(reader.artifact, 0, ProtocolReasonV1.DIGEST_MISMATCH, "job re-encode drift")
        return result

    def encode(self) -> bytes:
        definition = self.definition.encode()
        domain = self.domain.encode()
        policy = self.policy.encode()
        return b"".join(
            (
                JOB_MAGIC_V1,
                self.definition.definition_digest,
                _blob(definition),
                self.definition.formula_release,
                _blob(self.formula_spec),
                self.domain.identity,
                _blob(domain),
                self.policy.identity,
                _blob(policy),
            )
        )

    @cached_property
    def identity(self) -> bytes:
        return _identity(JOB_ID_LABEL_V1, self.encode())


@dataclass(frozen=True)
class ComparatorManifestV1:
    kind: ComparatorKindV1
    engine_release: bytes
    upstream_source: bytes
    arithmetic_closure: bytes
    wrapper_source: bytes
    evaluator_source: bytes
    build_identity: bytes
    operation_allowlist: bytes
    test_receipt: bytes
    license_closure: bytes
    exclusions: bytes

    def __post_init__(self) -> None:
        if type(self.kind) is not ComparatorKindV1:
            _fail("comparator-manifest-v1", 0, ProtocolReasonV1.UNKNOWN_RELEASE, "unknown comparator kind")
        for field in fields(self):
            if field.name != "kind":
                _require_digest(getattr(self, field.name), "comparator-manifest-v1", field.name)

    @classmethod
    def parse(cls, data: bytes) -> "ComparatorManifestV1":
        reader = _Reader(data, "comparator-manifest-v1")
        reader.magic(MANIFEST_MAGIC_V1)
        kind_offset = reader.offset
        try:
            kind = ComparatorKindV1(reader.u8())
        except ValueError:
            _fail(reader.artifact, kind_offset, ProtocolReasonV1.UNKNOWN_RELEASE, "unknown engine family")
        values = tuple(reader.exact(32) for _ in range(10))
        reader.finish()
        result = cls(kind, *values)
        if result.encode() != data:
            _fail(reader.artifact, 0, ProtocolReasonV1.INVALID_MANIFEST, "manifest re-encode drift")
        return result

    def encode(self) -> bytes:
        return MANIFEST_MAGIC_V1 + bytes((int(self.kind),)) + b"".join(
            getattr(self, field.name) for field in fields(self) if field.name != "kind"
        )

    @cached_property
    def identity(self) -> bytes:
        return _identity(MANIFEST_ID_LABEL_V1, self.encode())

@dataclass(frozen=True, init=False)
class ContentResolvedComparatorManifestV1:
    manifest: ComparatorManifestV1

    def __new__(cls):
        raise TypeError("use ContentResolvedComparatorManifestV1.admit")

    @classmethod
    def admit(
        cls,
        manifest: ComparatorManifestV1,
        resolve_content_address: Callable[[bytes], bytes | Iterable[bytes] | None],
    ) -> "ContentResolvedComparatorManifestV1":
        # A digest declaration alone is not source binding. This structural
        # transition only re-hashes caller-provided bytes; a future controlled
        # replay must establish where those bytes came from.
        if type(manifest) is not ComparatorManifestV1:
            _fail(
                "comparator-manifest-v1",
                0,
                ProtocolReasonV1.INVALID_MANIFEST,
                "content resolution requires a canonical manifest",
            )
        for field in fields(manifest):
            if field.name == "kind":
                continue
            coordinate = getattr(manifest, field.name)
            content = resolve_content_address(coordinate)
            if content is None:
                _fail(
                    "comparator-manifest-v1",
                    0,
                    ProtocolReasonV1.INVALID_MANIFEST,
                    f"unresolved content address: {field.name}",
                )
            if type(content) is bytes:
                chunks: Iterator[bytes] = iter((content,))
            else:
                try:
                    chunks = iter(content)
                except TypeError:
                    _fail(
                        "comparator-manifest-v1",
                        0,
                        ProtocolReasonV1.INVALID_MANIFEST,
                        f"content resolver did not return bytes: {field.name}",
                    )
            replay = hashlib.sha256()
            for chunk in chunks:
                if type(chunk) is not bytes:
                    _fail(
                        "comparator-manifest-v1",
                        0,
                        ProtocolReasonV1.INVALID_MANIFEST,
                        f"non-byte content chunk: {field.name}",
                    )
                replay.update(chunk)
            if replay.digest() != coordinate:
                _fail(
                    "comparator-manifest-v1",
                    0,
                    ProtocolReasonV1.DIGEST_MISMATCH,
                    f"content digest mismatch: {field.name}",
                )
        result = object.__new__(cls)
        object.__setattr__(result, "manifest", manifest)
        return result

    @cached_property
    def identity(self) -> bytes:
        return self.manifest.identity


class DecisionV1(IntEnum):
    INSIDE = 0
    OUTSIDE = 1
    BOUNDARY_UNPROVEN = 2
    RESOURCE_LIMIT_REACHED = 3


@dataclass(frozen=True)
class ExactZeroSignalTraceV1:
    ordinal: int
    trace_digest: bytes

    def __post_init__(self) -> None:
        if type(self.ordinal) is not int or self.ordinal < 0 or self.ordinal >= OUTPUT_CARDINALITY_V1:
            _fail("transcript-v1", 0, ProtocolReasonV1.INVALID_TRANSCRIPT, "equality ordinal outside sRGB8")
        _require_digest(self.trace_digest, "transcript-v1", "exact zero trace")


@dataclass(frozen=True)
class BoundaryUnprovenWitnessV1:
    ordinal: int
    enclosure_digest: bytes

    def __post_init__(self) -> None:
        if type(self.ordinal) is not int or self.ordinal < 0 or self.ordinal >= OUTPUT_CARDINALITY_V1:
            _fail("transcript-v1", 0, ProtocolReasonV1.INVALID_TRANSCRIPT, "boundary ordinal outside sRGB8")
        _require_digest(self.enclosure_digest, "transcript-v1", "boundary enclosure")


@dataclass(frozen=True)
class ResourceLimitWitnessV1:
    ordinal: int
    scope: int
    granted: int
    consumed: int

    def __post_init__(self) -> None:
        if (
            type(self.ordinal) is not int
            or self.ordinal < 0
            or self.ordinal >= OUTPUT_CARDINALITY_V1
            or type(self.scope) is not int
            or self.scope not in (1, 2)
            or type(self.granted) is not int
            or type(self.consumed) is not int
            or not (0 <= self.granted <= 0xFFFF_FFFF_FFFF_FFFF)
            or self.consumed != self.granted
        ):
            _fail("transcript-v1", 0, ProtocolReasonV1.INVALID_TRANSCRIPT, "invalid resource witness")


WitnessV1: TypeAlias = ExactZeroSignalTraceV1 | BoundaryUnprovenWitnessV1 | ResourceLimitWitnessV1


def _decision_payload_length(point_count: int) -> int:
    if point_count <= 0 or point_count > OUTPUT_CARDINALITY_V1:
        _fail("transcript-v1", 0, ProtocolReasonV1.LENGTH_OUT_OF_BOUNDS, "point count outside V1 domain")
    return (point_count * 2 + 7) // 8


def _validate_decision_payload(payload: bytes, point_count: int) -> None:
    expected = _decision_payload_length(point_count)
    if len(payload) != expected:
        _fail("transcript-v1", 0, ProtocolReasonV1.COUNT_MISMATCH, "decision payload length mismatch")
    unused = expected * 8 - point_count * 2
    if payload and unused and payload[-1] & ((1 << unused) - 1):
        _fail("transcript-v1", 0, ProtocolReasonV1.INVALID_TRANSCRIPT, "nonzero padding bits")


def _decision_at(payload: bytes, index: int) -> DecisionV1:
    return DecisionV1((payload[index // 4] >> (6 - 2 * (index % 4))) & 0b11)


def _decision_counters(
    payload: bytes,
    point_count: int,
) -> tuple[int, int, int, int]:
    _validate_decision_payload(payload, point_count)
    counters = [0, 0, 0, 0]
    full_bytes = point_count // 4
    for byte_index in range(full_bytes):
        value = payload[byte_index]
        counters[(value >> 6) & 0b11] += 1
        counters[(value >> 4) & 0b11] += 1
        counters[(value >> 2) & 0b11] += 1
        counters[value & 0b11] += 1
    for index in range(full_bytes * 4, point_count):
        counters[int(_decision_at(payload, index))] += 1
    return tuple(counters)  # type: ignore[return-value]


def _iter_decisions(payload: bytes, point_count: int) -> Iterator[DecisionV1]:
    _validate_decision_payload(payload, point_count)
    for index in range(point_count):
        yield _decision_at(payload, index)


def _pack_decisions(
    decisions: Iterable[DecisionV1],
    point_count: int,
) -> tuple[bytes, tuple[int, int, int, int]]:
    output = bytearray(_decision_payload_length(point_count))
    counters = [0, 0, 0, 0]
    iterator = iter(decisions)
    for index in range(point_count):
        try:
            raw = next(iterator)
        except StopIteration:
            _fail("transcript-v1", 0, ProtocolReasonV1.COUNT_MISMATCH, "decision stream is truncated")
        try:
            decision = DecisionV1(raw)
        except (TypeError, ValueError):
            _fail("transcript-v1", 0, ProtocolReasonV1.UNKNOWN_RELEASE, "unknown decision tag")
        output[index // 4] |= int(decision) << (6 - 2 * (index % 4))
        counters[int(decision)] += 1
    try:
        next(iterator)
    except StopIteration:
        return bytes(output), tuple(counters)  # type: ignore[return-value]
    _fail("transcript-v1", 0, ProtocolReasonV1.COUNT_MISMATCH, "decision stream has trailing outcomes")


def _append_u32(output: bytearray, value: int) -> None:
    output.append((value >> 24) & 0xFF)
    output.append((value >> 16) & 0xFF)
    output.append((value >> 8) & 0xFF)
    output.append(value & 0xFF)


def _append_u64(output: bytearray, value: int) -> None:
    for shift in range(56, -1, -8):
        output.append((value >> shift) & 0xFF)


def _append_witness(output: bytearray, witness: WitnessV1) -> None:
    if type(witness) is ExactZeroSignalTraceV1:
        output.append(1)
    elif type(witness) is BoundaryUnprovenWitnessV1:
        output.append(2)
    else:
        output.append(3)
    _append_u32(output, witness.ordinal)
    if type(witness) is ExactZeroSignalTraceV1:
        output.extend(witness.trace_digest)
    elif type(witness) is BoundaryUnprovenWitnessV1:
        output.extend(witness.enclosure_digest)
    else:
        output.append(witness.scope)
        _append_u64(output, witness.granted)
        _append_u64(output, witness.consumed)


def _u32_at(source: bytes, offset: int) -> int:
    return (
        (source[offset] << 24)
        | (source[offset + 1] << 16)
        | (source[offset + 2] << 8)
        | source[offset + 3]
    )


def _u64_at(source: bytes, offset: int) -> int:
    value = 0
    for index in range(offset, offset + 8):
        value = (value << 8) | source[index]
    return value


def _span_has_nonzero(source: bytes, start: int, end: int) -> bool:
    for index in range(start, end):
        if source[index]:
            return True
    return False


def _scan_witness_body_v1(
    source: bytes,
    start: int,
    wire_length: int,
    count: int,
    *,
    artifact: str,
    reported_base: int,
) -> tuple[int, int, int]:
    cursor = start
    limit = start + wire_length
    equality_count = 0
    boundary_count = 0
    resource_count = 0
    previous_ordinal: int | None = None
    for _ in range(count):
        relative = cursor - start
        if limit - cursor < 5:
            _fail(artifact, reported_base + relative, ProtocolReasonV1.TRUNCATED, "truncated witness prefix")
        kind = source[cursor]
        ordinal = _u32_at(source, cursor + 1)
        if ordinal >= OUTPUT_CARDINALITY_V1:
            _fail(artifact, reported_base + relative, ProtocolReasonV1.INVALID_TRANSCRIPT, "witness ordinal outside sRGB8")
        if previous_ordinal is not None and ordinal <= previous_ordinal:
            _fail(artifact, reported_base + relative, ProtocolReasonV1.NONCANONICAL_ORDER, "witnesses not strict")
        previous_ordinal = ordinal

        if kind in (1, 2):
            end = cursor + 37
            if end > limit:
                _fail(artifact, reported_base + relative, ProtocolReasonV1.TRUNCATED, "truncated digest witness")
            if not _span_has_nonzero(source, cursor + 5, end):
                _fail(artifact, reported_base + relative, ProtocolReasonV1.INVALID_DIGEST, "zero witness digest")
            if kind == 1:
                equality_count += 1
            else:
                boundary_count += 1
        elif kind == 3:
            end = cursor + 22
            if end > limit:
                _fail(artifact, reported_base + relative, ProtocolReasonV1.TRUNCATED, "truncated resource witness")
            scope = source[cursor + 5]
            same_accounting = True
            for index in range(8):
                if source[cursor + 6 + index] != source[cursor + 14 + index]:
                    same_accounting = False
                    break
            if scope not in (1, 2) or not same_accounting:
                _fail(artifact, reported_base + relative, ProtocolReasonV1.INVALID_TRANSCRIPT, "invalid resource witness")
            resource_count += 1
        else:
            _fail(artifact, reported_base + relative, ProtocolReasonV1.UNKNOWN_RELEASE, "unknown witness kind")
        cursor = end
    if cursor != limit:
        _fail(artifact, reported_base + cursor - start, ProtocolReasonV1.TRAILING_BYTES, "bytes after witness store")
    return equality_count, boundary_count, resource_count


@dataclass(frozen=True, init=False, eq=False)
class WitnessStoreV1:
    _source: bytes
    _start: int
    wire_length: int
    count: int
    counts: tuple[int, int, int]
    _hash: int | None = field(init=False, repr=False, compare=False, default=None)

    def __init__(self, body: bytes, count: int):
        if type(body) is not bytes:
            _fail(
                "witness-store-v1",
                0,
                ProtocolReasonV1.INVALID_TRANSCRIPT,
                "witness body is not immutable owned bytes",
            )
        self._admit_source(
            body,
            0,
            len(body),
            count,
            artifact="witness-store-v1",
            reported_base=0,
        )

    @classmethod
    def _from_source(
        cls,
        source: bytes,
        start: int,
        wire_length: int,
        count: int,
        *,
        artifact: str,
        reported_base: int,
    ) -> "WitnessStoreV1":
        result = object.__new__(cls)
        result._admit_source(
            source,
            start,
            wire_length,
            count,
            artifact=artifact,
            reported_base=reported_base,
        )
        return result

    def _admit_source(
        self,
        source: bytes,
        start: int,
        wire_length: int,
        count: int,
        *,
        artifact: str,
        reported_base: int,
    ) -> None:
        if type(count) is not int or count < 0 or count > OUTPUT_CARDINALITY_V1:
            _fail("witness-store-v1", 0, ProtocolReasonV1.LENGTH_OUT_OF_BOUNDS, "invalid witness count")
        if (
            type(source) is not bytes
            or type(start) is not int
            or type(wire_length) is not int
            or start < 0
            or wire_length < 0
            or start > len(source)
            or wire_length > len(source) - start
        ):
            _fail("witness-store-v1", 0, ProtocolReasonV1.INVALID_TRANSCRIPT, "witness body is not immutable bytes")
        counts = _scan_witness_body_v1(
            source,
            start,
            wire_length,
            count,
            artifact=artifact,
            reported_base=reported_base,
        )
        object.__setattr__(self, "_source", source)
        object.__setattr__(self, "_start", start)
        object.__setattr__(self, "wire_length", wire_length)
        object.__setattr__(self, "count", count)
        object.__setattr__(self, "counts", counts)
        object.__setattr__(self, "_hash", None)

    @classmethod
    def from_witnesses(cls, witnesses: Iterable[WitnessV1]) -> "WitnessStoreV1":
        body = bytearray()
        count = 0
        counts = [0, 0, 0]
        previous_ordinal: int | None = None
        for witness in witnesses:
            witness_type = type(witness)
            if witness_type is ExactZeroSignalTraceV1:
                counts[0] += 1
            elif witness_type is BoundaryUnprovenWitnessV1:
                counts[1] += 1
            elif witness_type is ResourceLimitWitnessV1:
                counts[2] += 1
            else:
                _fail("witness-store-v1", 0, ProtocolReasonV1.UNKNOWN_RELEASE, "unknown witness type")
            if previous_ordinal is not None and witness.ordinal <= previous_ordinal:
                _fail("witness-store-v1", 0, ProtocolReasonV1.NONCANONICAL_ORDER, "witnesses not strict")
            _append_witness(body, witness)
            previous_ordinal = witness.ordinal
            count += 1
            if count > OUTPUT_CARDINALITY_V1:
                _fail("witness-store-v1", 0, ProtocolReasonV1.LENGTH_OUT_OF_BOUNDS, "too many witnesses")
        source = bytes(body)
        result = object.__new__(cls)
        object.__setattr__(result, "_source", source)
        object.__setattr__(result, "_start", 0)
        object.__setattr__(result, "wire_length", len(source))
        object.__setattr__(result, "count", count)
        object.__setattr__(result, "counts", tuple(counts))
        object.__setattr__(result, "_hash", None)
        return result

    def __eq__(self, other: object) -> bool:
        return (
            type(other) is WitnessStoreV1
            and self.count == other.count
            and self.counts == other.counts
            and self.wire_length == other.wire_length
            and self.body_view() == other.body_view()
        )

    def __repr__(self) -> str:
        return (
            f"WitnessStoreV1(count={self.count}, counts={self.counts}, "
            f"wire_length={self.wire_length})"
        )

    def __hash__(self) -> int:
        cached = self._hash
        if cached is None:
            # A view preserves value hashing without copying a potentially
            # full-domain body; the cache prevents every mapping lookup from
            # rescanning it while leaving the observable value immutable.
            cached = hash(
                (self.count, self.counts, self.wire_length, self.body_view())
            )
            object.__setattr__(self, "_hash", cached)
        return cached

    def body_view(self) -> memoryview:
        return memoryview(self._source)[
            self._start : self._start + self.wire_length
        ]

    @property
    def equality_count(self) -> int:
        return self.counts[0]

    @property
    def boundary_count(self) -> int:
        return self.counts[1]

    @property
    def resource_count(self) -> int:
        return self.counts[2]

    def iter_witnesses(self) -> Iterator[WitnessV1]:
        cursor = self._start
        for _ in range(self.count):
            kind = self._source[cursor]
            ordinal = _u32_at(self._source, cursor + 1)
            if kind == 1:
                end = cursor + 37
                yield ExactZeroSignalTraceV1(ordinal, self._source[cursor + 5 : end])
            elif kind == 2:
                end = cursor + 37
                yield BoundaryUnprovenWitnessV1(ordinal, self._source[cursor + 5 : end])
            else:
                end = cursor + 22
                yield ResourceLimitWitnessV1(
                    ordinal,
                    self._source[cursor + 5],
                    _u64_at(self._source, cursor + 6),
                    _u64_at(self._source, cursor + 14),
                )
            cursor = end


def _validate_witness_alignment(
    domain: ReducedDomainManifestV1,
    decision_bits: bytes,
    point_count: int,
    counters: tuple[int, int, int, int],
    witnesses: WitnessStoreV1,
) -> None:
    if point_count != domain.point_count:
        _fail("transcript-v1", 0, ProtocolReasonV1.COUNT_MISMATCH, "domain and decision counts differ")
    if witnesses.boundary_count != counters[2] or witnesses.resource_count != counters[3]:
        _fail("transcript-v1", 0, ProtocolReasonV1.COUNT_MISMATCH, "unresolved witness kinds disagree with counters")

    range_iterator = iter(domain.ranges)
    current_range = next(range_iterator, None)
    points_before_range = 0
    cursor = witnesses._start
    for _ in range(witnesses.count):
        kind = witnesses._source[cursor]
        ordinal = _u32_at(witnesses._source, cursor + 1)
        while current_range is not None and ordinal >= current_range[1]:
            points_before_range += current_range[1] - current_range[0]
            current_range = next(range_iterator, None)
        if current_range is None or ordinal < current_range[0]:
            _fail("transcript-v1", 0, ProtocolReasonV1.FOREIGN_BINDING, "witness outside domain")
        index = points_before_range + ordinal - current_range[0]
        decision = (decision_bits[index // 4] >> (6 - 2 * (index % 4))) & 0b11
        expected = 0 if kind == 1 else 2 if kind == 2 else 3
        if decision != expected:
            _fail("transcript-v1", 0, ProtocolReasonV1.INVALID_TRANSCRIPT, "witness kind does not match decision")
        cursor += 37 if kind in (1, 2) else 22


@dataclass(frozen=True)
class DecisionTranscriptV1:
    job_identity: bytes
    domain_identity: bytes
    comparator_identity: bytes
    point_count: int
    decision_bits: bytes
    counters: tuple[int, int, int, int]
    exact_equality_count: int
    accounting_digest: bytes
    witness_store: WitnessStoreV1

    def __post_init__(self) -> None:
        if (
            type(self.counters) is not tuple
            or len(self.counters) != 4
            or any(
                type(count) is not int
                or count < 0
                or count > 0xFFFF_FFFF_FFFF_FFFF
                for count in self.counters
            )
            or type(self.exact_equality_count) is not int
            or self.exact_equality_count < 0
            or self.exact_equality_count > 0xFFFF_FFFF_FFFF_FFFF
            or type(self.point_count) is not int
            or type(self.witness_store) is not WitnessStoreV1
            or type(self.decision_bits) is not bytes
        ):
            _fail("transcript-v1", 0, ProtocolReasonV1.INVALID_TRANSCRIPT, "noncanonical transcript field type")
        for name in ("job_identity", "domain_identity", "comparator_identity", "accounting_digest"):
            _require_digest(getattr(self, name), "transcript-v1", name)
        actual = _decision_counters(self.decision_bits, self.point_count)
        if actual != self.counters or sum(self.counters) != self.point_count:
            _fail("transcript-v1", 0, ProtocolReasonV1.COUNT_MISMATCH, "decision counters mismatch")
        if self.witness_store.count > self.point_count:
            _fail("transcript-v1", 0, ProtocolReasonV1.LENGTH_OUT_OF_BOUNDS, "more witnesses than points")
        if (
            self.witness_store.equality_count != self.exact_equality_count
            or self.witness_store.equality_count > self.counters[0]
        ):
            _fail("transcript-v1", 0, ProtocolReasonV1.MISSING_EQUALITY_WITNESS, "equality count mismatch")
        if (
            self.witness_store.boundary_count != self.counters[2]
            or self.witness_store.resource_count != self.counters[3]
        ):
            _fail("transcript-v1", 0, ProtocolReasonV1.COUNT_MISMATCH, "counter and witness counts disagree")

    @classmethod
    def from_decisions(
        cls,
        job: ProofJobV1,
        comparator: ContentResolvedComparatorManifestV1,
        decisions: Iterable[DecisionV1],
        witnesses: Iterable[WitnessV1],
        accounting_digest: bytes,
        *,
        exact_equality_count: int | None = None,
    ) -> "DecisionTranscriptV1":
        decision_bits, counters = _pack_decisions(decisions, job.domain.point_count)
        witness_store = WitnessStoreV1.from_witnesses(witnesses)
        result = cls(
            job.identity,
            job.domain.identity,
            comparator.identity,
            job.domain.point_count,
            decision_bits,
            counters,
            (
                witness_store.equality_count
                if exact_equality_count is None
                else exact_equality_count
            ),
            accounting_digest,
            witness_store,
        )
        _validate_witness_alignment(
            job.domain,
            result.decision_bits,
            result.point_count,
            result.counters,
            result.witness_store,
        )
        return result

    @classmethod
    def parse(cls, data: bytes) -> "DecisionTranscriptV1":
        reader = _Reader(data, "transcript-v1")
        reader.magic(TRANSCRIPT_MAGIC_V1)
        job = reader.exact(32)
        domain = reader.exact(32)
        comparator = reader.exact(32)
        point_count = reader.u64()
        decision_bits = reader.blob(exact_length=_decision_payload_length(point_count))
        counters = tuple(reader.u64() for _ in range(4))
        equality_count = reader.u64()
        accounting = reader.exact(32)
        witness_count = reader.u64()
        # Every witness needs at least kind+ordinal+scope+two u64 for the
        # shortest resource record. This rejects hostile counts before looping.
        expected_witness_count = counters[2] + counters[3] + equality_count
        actual_counters = _decision_counters(decision_bits, point_count)
        if (
            counters != actual_counters
            or sum(counters) != point_count
            or equality_count > point_count
            or equality_count > counters[0]
            or witness_count != expected_witness_count
        ):
            _fail(reader.artifact, reader.offset, ProtocolReasonV1.COUNT_MISMATCH, "counter and witness counts disagree")
        if (
            witness_count > point_count
            or witness_count > reader.remaining // 22
        ):
            _fail(reader.artifact, reader.offset, ProtocolReasonV1.LENGTH_OUT_OF_BOUNDS, "witness count exceeds body")
        expected_witness_bytes = 37 * (counters[2] + equality_count) + 22 * counters[3]
        if reader.remaining != expected_witness_bytes:
            reason = (
                ProtocolReasonV1.TRUNCATED
                if reader.remaining < expected_witness_bytes
                else ProtocolReasonV1.TRAILING_BYTES
            )
            _fail(reader.artifact, reader.offset, reason, "witness body length disagrees with counters")
        witness_start, witness_end = reader.span_exact(expected_witness_bytes)
        reader.finish()
        witness_store = WitnessStoreV1._from_source(
            data,
            witness_start,
            witness_end - witness_start,
            witness_count,
            artifact=reader.artifact,
            reported_base=witness_start,
        )
        result = cls(
            job,
            domain,
            comparator,
            point_count,
            decision_bits,
            counters,
            equality_count,
            accounting,
            witness_store,
        )
        if not result._matches_encoded(data):
            _fail(reader.artifact, 0, ProtocolReasonV1.INVALID_TRANSCRIPT, "transcript re-encode drift")
        return result

    def _header_chunks(self) -> Iterator[bytes]:
        yield TRANSCRIPT_MAGIC_V1
        yield self.job_identity
        yield self.domain_identity
        yield self.comparator_identity
        yield self.point_count.to_bytes(8, "big")
        yield len(self.decision_bits).to_bytes(8, "big")
        yield self.decision_bits
        for count in self.counters:
            yield count.to_bytes(8, "big")
        yield self.exact_equality_count.to_bytes(8, "big")
        yield self.accounting_digest
        yield self.witness_store.count.to_bytes(8, "big")

    def _encoded_chunks(self) -> Iterator[bytes | memoryview]:
        yield from self._header_chunks()
        yield self.witness_store.body_view()

    def _matches_encoded(self, encoded: bytes) -> bool:
        offset = 0
        encoded_view = memoryview(encoded)
        for chunk in self._header_chunks():
            end = offset + len(chunk)
            if end > len(encoded_view) or encoded_view[offset:end] != chunk:
                return False
            offset = end
        return (
            self.witness_store._source is encoded
            and self.witness_store._start == offset
            and offset + self.witness_store.wire_length == len(encoded_view)
        )

    def encode(self) -> bytes:
        return b"".join(self._encoded_chunks())

    @cached_property
    def identity(self) -> bytes:
        return _identity_from_chunks(
            TRANSCRIPT_ID_LABEL_V1,
            200 + len(self.decision_bits) + self.witness_store.wire_length,
            self._encoded_chunks(),
        )

    def iter_decisions(self) -> Iterator[DecisionV1]:
        return _iter_decisions(self.decision_bits, self.point_count)

    def iter_witnesses(self) -> Iterator[WitnessV1]:
        return self.witness_store.iter_witnesses()


@dataclass(frozen=True)
class RunClaimV1:
    job_identity: bytes
    comparator_identity: bytes
    binary_identity: bytes
    invocation_identity: bytes
    platform_identity: bytes
    transcript_identity: bytes

    def __post_init__(self) -> None:
        for field in fields(self):
            _require_digest(getattr(self, field.name), "run-claim-v1", field.name)

    @classmethod
    def for_transcript(
        cls,
        job: ProofJobV1,
        comparator: ContentResolvedComparatorManifestV1,
        transcript: DecisionTranscriptV1,
        binary_identity: bytes,
        invocation_identity: bytes,
        platform_identity: bytes,
    ) -> "RunClaimV1":
        if transcript.job_identity != job.identity or transcript.comparator_identity != comparator.identity:
            _fail("run-claim-v1", 0, ProtocolReasonV1.FOREIGN_BINDING, "transcript binding mismatch")
        return cls(job.identity, comparator.identity, binary_identity, invocation_identity, platform_identity, transcript.identity)

    @classmethod
    def parse(cls, data: bytes) -> "RunClaimV1":
        reader = _Reader(data, "run-claim-v1")
        reader.magic(RUN_CLAIM_MAGIC_V1)
        result = cls(*(reader.exact(32) for _ in range(6)))
        reader.finish()
        if result.encode() != data:
            _fail(reader.artifact, 0, ProtocolReasonV1.FOREIGN_BINDING, "run claim re-encode drift")
        return result

    def encode(self) -> bytes:
        return RUN_CLAIM_MAGIC_V1 + b"".join(getattr(self, field.name) for field in fields(self))

    @cached_property
    def identity(self) -> bytes:
        return _identity(RUN_CLAIM_ID_LABEL_V1, self.encode())


@dataclass(frozen=True)
class EvaluatorProvenanceClaimV1:
    provenance_policy_identity: bytes
    run_claim_identity: bytes
    replay_evidence_identity: bytes

    def __post_init__(self) -> None:
        for field in fields(self):
            _require_digest(
                getattr(self, field.name),
                "evaluator-provenance-claim-v1",
                field.name,
            )

    @classmethod
    def parse(cls, data: bytes) -> "EvaluatorProvenanceClaimV1":
        reader = _Reader(data, "evaluator-provenance-claim-v1")
        reader.magic(PROVENANCE_CLAIM_MAGIC_V1)
        result = cls(*(reader.exact(32) for _ in range(3)))
        reader.finish()
        if result.encode() != data:
            _fail(
                reader.artifact,
                0,
                ProtocolReasonV1.FOREIGN_BINDING,
                "provenance claim re-encode drift",
            )
        return result

    def encode(self) -> bytes:
        return PROVENANCE_CLAIM_MAGIC_V1 + b"".join(
            getattr(self, field.name) for field in fields(self)
        )

    @cached_property
    def identity(self) -> bytes:
        return _identity(PROVENANCE_CLAIM_ID_LABEL_V1, self.encode())


@dataclass(frozen=True)
class DualComparisonClaimV1:
    job_identity: bytes
    definition_digest: bytes
    domain_identity: bytes
    policy_identity: bytes
    domain_point_count: int
    comparator_identities: tuple[bytes, bytes]
    run_claim_identities: tuple[bytes, bytes]
    transcript_identities: tuple[bytes, bytes]
    decision_digest: bytes

    def __post_init__(self) -> None:
        if (
            type(self.domain_point_count) is not int
            or self.domain_point_count <= 0
            or self.domain_point_count > OUTPUT_CARDINALITY_V1
        ):
            _fail("dual-comparison-v1", 0, ProtocolReasonV1.LENGTH_OUT_OF_BOUNDS, "point count outside V1 domain")
        if any(
            type(pair) is not tuple or len(pair) != 2
            for pair in (
                self.comparator_identities,
                self.run_claim_identities,
                self.transcript_identities,
            )
        ):
            _fail("dual-comparison-v1", 0, ProtocolReasonV1.COUNT_MISMATCH, "dual coordinate pairs must have length two")
        for value in (
            self.job_identity,
            self.definition_digest,
            self.domain_identity,
            self.policy_identity,
            *self.comparator_identities,
            *self.run_claim_identities,
            *self.transcript_identities,
            self.decision_digest,
        ):
            _require_digest(value, "dual-comparison-v1", "comparison coordinate")

    @classmethod
    def parse(cls, data: bytes) -> "DualComparisonClaimV1":
        reader = _Reader(data, "dual-comparison-v1")
        reader.magic(COMPARISON_MAGIC_V1)
        prefix = tuple(reader.exact(32) for _ in range(4))
        point_count = reader.u64()
        suffix = tuple(reader.exact(32) for _ in range(7))
        values = prefix + suffix
        reader.finish()
        result = cls(values[0], values[1], values[2], values[3], point_count, (values[4], values[5]), (values[6], values[7]), (values[8], values[9]), values[10])
        if result.encode() != data:
            _fail(reader.artifact, 0, ProtocolReasonV1.FOREIGN_BINDING, "dual comparison re-encode drift")
        return result

    def encode(self) -> bytes:
        return COMPARISON_MAGIC_V1 + b"".join(
            (
                self.job_identity,
                self.definition_digest,
                self.domain_identity,
                self.policy_identity,
                self.domain_point_count.to_bytes(8, "big"),
                *self.comparator_identities,
                *self.run_claim_identities,
                *self.transcript_identities,
                self.decision_digest,
            )
        )

    @cached_property
    def identity(self) -> bytes:
        return _identity(COMPARISON_ID_LABEL_V1, self.encode())


@dataclass(frozen=True, init=False)
class DualComparisonCandidateV1:
    claim: DualComparisonClaimV1

    def __new__(cls):
        raise TypeError("DualComparisonCandidateV1 is created by dual admission")

    @classmethod
    def _admit(cls, claim: DualComparisonClaimV1) -> "DualComparisonCandidateV1":
        if type(claim) is not DualComparisonClaimV1:
            _fail(
                "dual-comparison-v1",
                0,
                ProtocolReasonV1.FOREIGN_BINDING,
                "comparison claim is not canonical",
            )
        result = object.__new__(cls)
        object.__setattr__(result, "claim", claim)
        return result

    def encode(self) -> bytes:
        return self.claim.encode()

    @property
    def identity(self) -> bytes:
        return self.claim.identity


def _admit_transcript(
    job: ProofJobV1,
    comparator: ContentResolvedComparatorManifestV1,
    transcript: DecisionTranscriptV1,
    run: RunClaimV1,
    *,
    job_identity: bytes,
    domain_identity: bytes,
    comparator_identity: bytes,
    transcript_identity: bytes,
) -> None:
    if (
        transcript.job_identity != job_identity
        or transcript.domain_identity != domain_identity
        or transcript.comparator_identity != comparator_identity
        or transcript.point_count != job.domain.point_count
        or run.job_identity != job_identity
        or run.comparator_identity != comparator_identity
        or run.transcript_identity != transcript_identity
    ):
        _fail("dual-admission-v1", 0, ProtocolReasonV1.FOREIGN_BINDING, "foreign transcript/run coordinate")
    _validate_witness_alignment(
        job.domain,
        transcript.decision_bits,
        transcript.point_count,
        transcript.counters,
        transcript.witness_store,
    )


def compare_dual_transcripts(
    job: ProofJobV1,
    first_manifest: ContentResolvedComparatorManifestV1,
    first_transcript: DecisionTranscriptV1,
    first_run: RunClaimV1,
    second_manifest: ContentResolvedComparatorManifestV1,
    second_transcript: DecisionTranscriptV1,
    second_run: RunClaimV1,
) -> DualComparisonCandidateV1:
    if (
        type(job) is not ProofJobV1
        or type(first_transcript) is not DecisionTranscriptV1
        or type(first_run) is not RunClaimV1
        or type(second_transcript) is not DecisionTranscriptV1
        or type(second_run) is not RunClaimV1
    ):
        _fail(
            "dual-admission-v1",
            0,
            ProtocolReasonV1.FOREIGN_BINDING,
            "dual admission requires canonical job, transcripts and runs",
        )
    if (
        type(first_manifest) is not ContentResolvedComparatorManifestV1
        or type(second_manifest) is not ContentResolvedComparatorManifestV1
        or type(first_manifest.manifest) is not ComparatorManifestV1
        or type(second_manifest.manifest) is not ComparatorManifestV1
    ):
        _fail(
            "dual-admission-v1",
            0,
            ProtocolReasonV1.INVALID_MANIFEST,
            "dual admission requires content-resolved canonical manifests",
        )
    first = first_manifest.manifest
    second = second_manifest.manifest
    if (
        first.kind == second.kind
        or first.engine_release == second.engine_release
        or first.upstream_source == second.upstream_source
        or first.wrapper_source == second.wrapper_source
        or first.evaluator_source == second.evaluator_source
    ):
        _fail(
            "dual-admission-v1",
            0,
            ProtocolReasonV1.SHARED_DIVERSITY_COORDINATE,
            "comparators share a required-distinct coordinate",
        )
    if (first.kind, second.kind) != (ComparatorKindV1.ARB, ComparatorKindV1.MPFI):
        _fail("dual-admission-v1", 0, ProtocolReasonV1.NONCANONICAL_ORDER, "dual order is Arb then MPFI")
    if first_run.binary_identity == second_run.binary_identity:
        _fail(
            "dual-comparison-v1",
            0,
            ProtocolReasonV1.SHARED_DIVERSITY_COORDINATE,
            "comparators share one binary identity",
        )
    job_identity = job.identity
    domain_identity = job.domain.identity
    policy_identity = job.policy.identity
    first_comparator_identity = first_manifest.identity
    second_comparator_identity = second_manifest.identity
    first_transcript_identity = first_transcript.identity
    second_transcript_identity = second_transcript.identity
    _admit_transcript(
        job,
        first_manifest,
        first_transcript,
        first_run,
        job_identity=job_identity,
        domain_identity=domain_identity,
        comparator_identity=first_comparator_identity,
        transcript_identity=first_transcript_identity,
    )
    _admit_transcript(
        job,
        second_manifest,
        second_transcript,
        second_run,
        job_identity=job_identity,
        domain_identity=domain_identity,
        comparator_identity=second_comparator_identity,
        transcript_identity=second_transcript_identity,
    )
    if first_transcript.counters[2] or first_transcript.counters[3] or second_transcript.counters[2] or second_transcript.counters[3]:
        _fail("dual-comparison-v1", 0, ProtocolReasonV1.UNRESOLVED_TRANSCRIPT, "unresolved outcome has no resolved comparison")
    if first_transcript.decision_bits != second_transcript.decision_bits:
        _fail("dual-admission-v1", 0, ProtocolReasonV1.DISAGREEMENT, "point decisions disagree")
    if (
        first_transcript.witness_store.body_view()
        != second_transcript.witness_store.body_view()
    ):
        _fail("dual-admission-v1", 0, ProtocolReasonV1.DISAGREEMENT, "exact equality traces disagree")
    decision_hasher = hashlib.sha256()
    decision_hasher.update(b"labcolors.proof-region.resolved-decisions.v1\0")
    decision_hasher.update(domain_identity)
    decision_hasher.update(len(first_transcript.decision_bits).to_bytes(8, "big"))
    decision_hasher.update(first_transcript.decision_bits)
    decision_digest = decision_hasher.digest()
    return DualComparisonCandidateV1._admit(
        DualComparisonClaimV1(
            job_identity,
            job.definition.definition_digest,
            domain_identity,
            policy_identity,
            job.domain.point_count,
            (first_comparator_identity, second_comparator_identity),
            (first_run.identity, second_run.identity),
            (first_transcript_identity, second_transcript_identity),
            decision_digest,
        )
    )
