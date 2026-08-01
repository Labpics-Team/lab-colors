#!/usr/bin/env python3
"""MPFI-owned source closure, materialized как один sealed generic input."""

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from enum import StrEnum
from typing import NoReturn

import provenance
from build import input as build_input


_MPFI_SOURCE_INPUT_ID_LABEL_V1 = b"labcolors.proof-region.mpfi-source-input.v1\0"
# Это release layout, не MPFI build recipe: source role — единственная
# инъективная namespace coordinate, гарантированная locked closure.
_MPFI_SOURCE_INPUT_LAYOUT_V1 = b"sources/<role>/<relative>"
_SOURCE_NAMESPACE_V1 = {
    provenance.SourceRoleV1.GMP: "gmp",
    provenance.SourceRoleV1.MPFR: "mpfr",
    provenance.SourceRoleV1.MPFI: "mpfi",
}


class MpfiSourceInputReasonV1(StrEnum):
    WRONG_TYPE = "wrong_type"
    FOREIGN_SOURCE_CAPABILITY = "foreign_source_capability"


@dataclass(frozen=True)
class MpfiSourceInputErrorV1(ValueError):
    reason: MpfiSourceInputReasonV1
    field: str

    def __str__(self) -> str:
        return f"{self.reason.value}: {self.field}"


def _fail(reason: MpfiSourceInputReasonV1, field_name: str) -> NoReturn:
    raise MpfiSourceInputErrorV1(reason, field_name)


def _canonical_lock_v1(
    source_lock: provenance.MpfiSourceLockV1,
) -> provenance.MpfiSourceLockV1:
    """Отбрасывает cached hostile state до именования input capability."""

    if type(source_lock) is not provenance.MpfiSourceLockV1:
        _fail(MpfiSourceInputReasonV1.WRONG_TYPE, "source_lock")
    try:
        return provenance.MpfiSourceLockV1.parse(source_lock.encode())
    except provenance.ProvenanceErrorV1:
        _fail(MpfiSourceInputReasonV1.FOREIGN_SOURCE_CAPABILITY, "source_lock")
    except (AttributeError, TypeError, ValueError, OverflowError):
        _fail(MpfiSourceInputReasonV1.FOREIGN_SOURCE_CAPABILITY, "source_lock")


def _fresh_admitted_sources_v1(
    source_lock: provenance.MpfiSourceLockV1,
    admitted_sources: provenance.AdmittedMpfiSourcesV1,
) -> provenance.AdmittedMpfiSourcesV1:
    if type(admitted_sources) is not provenance.AdmittedMpfiSourcesV1:
        _fail(MpfiSourceInputReasonV1.WRONG_TYPE, "admitted_sources")
    try:
        source_lock_identity = admitted_sources.source_lock_identity
        if (
            type(source_lock_identity) is not bytes
            or len(source_lock_identity) != 32
            or source_lock_identity == bytes(32)
            or source_lock_identity != source_lock.identity
        ):
            _fail(
                MpfiSourceInputReasonV1.FOREIGN_SOURCE_CAPABILITY,
                "admitted_sources",
            )
        # Re-admission сохраняет semantic slot order: сортировка выдала бы
        # forged GMP/MPFR exchange за легитимный closure.
        return provenance.admit_mpfi_sources(source_lock, admitted_sources.sources)
    except provenance.ProvenanceErrorV1 as error:
        if error.reason is provenance.ProvenanceReasonV1.FOREIGN_BINDING:
            _fail(
                MpfiSourceInputReasonV1.FOREIGN_SOURCE_CAPABILITY,
                "admitted_sources",
            )
        raise
    except (AttributeError, TypeError, ValueError, OverflowError):
        _fail(
            MpfiSourceInputReasonV1.FOREIGN_SOURCE_CAPABILITY,
            "admitted_sources",
        )


def _canonical_limits_v1(
    limits: build_input.CanonicalInputLimitsV1,
) -> build_input.CanonicalInputLimitsV1:
    if type(limits) is not build_input.CanonicalInputLimitsV1:
        _fail(MpfiSourceInputReasonV1.WRONG_TYPE, "limits")
    try:
        canonical = build_input.CanonicalInputLimitsV1(*tuple(limits))
    except build_input.InputErrorV1:
        raise
    except (AttributeError, TypeError, ValueError, OverflowError):
        raise build_input.InputErrorV1(
            build_input.InputReasonV1.NONCANONICAL_SET,
            "limits",
        )
    if tuple(canonical) != tuple(limits):
        raise build_input.InputErrorV1(
            build_input.InputReasonV1.NONCANONICAL_SET,
            "limits",
        )
    return canonical


def _preflight_declared_resource_bounds_v1(
    source_lock: provenance.MpfiSourceLockV1,
    limits: build_input.CanonicalInputLimitsV1,
) -> None:
    """Отклоняет declared totals до allocation file bodies во время replay."""

    declared_file_count = sum(
        item.regular_file_count for item in source_lock.sources
    )
    declared_payload_bytes = sum(
        item.regular_file_bytes for item in source_lock.sources
    )
    if declared_file_count > limits.max_members:
        raise build_input.InputErrorV1(
            build_input.InputReasonV1.RESOURCE_LIMIT,
            "max_members",
        )
    if declared_payload_bytes > limits.max_payload_bytes:
        raise build_input.InputErrorV1(
            build_input.InputReasonV1.RESOURCE_LIMIT,
            "max_payload_bytes",
        )


def _source_entries_v1(
    source_lock: provenance.MpfiSourceLockV1,
    admitted_sources: provenance.AdmittedMpfiSourcesV1,
) -> tuple[tuple[str, int, bytes], ...]:
    entries = tuple(
        (
            f"sources/{_SOURCE_NAMESPACE_V1[lock.role]}/{relative}",
            mode,
            contents,
        )
        for lock, admitted in zip(
            source_lock.sources,
            admitted_sources.sources,
            strict=True,
        )
        for relative, mode, contents in provenance.materialize_admitted_source_files_v1(
            lock,
            admitted,
        )
    )
    if not entries:
        _fail(MpfiSourceInputReasonV1.FOREIGN_SOURCE_CAPABILITY, "admitted_sources")
    return tuple(sorted(entries))


def _binding_identity_v1(
    source_lock: provenance.MpfiSourceLockV1,
    admitted_sources: provenance.AdmittedMpfiSourcesV1,
    contents: bytes,
) -> bytes:
    if type(contents) is not bytes or not contents:
        raise TypeError("MPFI source input contents must be exact nonempty bytes")
    coordinates = (
        _MPFI_SOURCE_INPUT_LAYOUT_V1,
        source_lock.identity,
        admitted_sources.identity,
        len(contents).to_bytes(8, "big"),
        hashlib.sha256(contents).digest(),
    )
    preimage = b"".join(len(value).to_bytes(8, "big") + value for value in coordinates)
    return hashlib.sha256(
        _MPFI_SOURCE_INPUT_ID_LABEL_V1
        + len(preimage).to_bytes(8, "big")
        + preimage
    ).digest()


def seal_mpfi_source_input_v1(
    source_lock: provenance.MpfiSourceLockV1,
    admitted_sources: provenance.AdmittedMpfiSourcesV1,
    limits: build_input.CanonicalInputLimitsV1,
) -> build_input.SealedInputV1:
    """Запечатывает MPFI source closure в caller-owned resource bounds.

    `MpfiSourceInputErrorV1` означает invalid public capability boundary,
    `ProvenanceErrorV1` — failure exact source replay, а `InputErrorV1` —
    canonical USTAR или resource-bound rejection.
    """

    canonical_lock = _canonical_lock_v1(source_lock)
    canonical_limits = _canonical_limits_v1(limits)
    canonical_admitted = _fresh_admitted_sources_v1(canonical_lock, admitted_sources)
    _preflight_declared_resource_bounds_v1(canonical_lock, canonical_limits)
    entries = _source_entries_v1(canonical_lock, canonical_admitted)
    contents = build_input.canonical_ustar_v1(entries, canonical_limits)
    return build_input.seal_input_v1(
        _binding_identity_v1(canonical_lock, canonical_admitted, contents),
        contents,
    )


def mpfi_source_input_is_bound_v1(
    source_lock: object,
    admitted_sources: object,
    limits: object,
    value: object,
) -> bool:
    """Независимо пересобирает MPFI input, а не доверяет одному seal."""

    if (
        type(value) is not build_input.SealedInputV1
        or not build_input.sealed_input_is_intact_v1(value)
    ):
        return False
    try:
        expected = seal_mpfi_source_input_v1(source_lock, admitted_sources, limits)
    except (
        MpfiSourceInputErrorV1,
        provenance.ProvenanceErrorV1,
        build_input.InputErrorV1,
        AttributeError,
        TypeError,
        ValueError,
        OverflowError,
    ):
        return False
    return (
        value.binding_identity == expected.binding_identity
        and value.contents == expected.contents
    )
