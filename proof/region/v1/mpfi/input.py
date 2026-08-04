#!/usr/bin/env python3
"""MPFI-замыкание исходников, материализуемое в единый sealed input."""

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
        canonical = provenance.snapshot_source_closure_lock_v1(source_lock)
    except (
        provenance.ProvenanceErrorV1,
        AttributeError,
        TypeError,
        ValueError,
        OverflowError,
        UnicodeError,
    ):
        _fail(MpfiSourceInputReasonV1.FOREIGN_SOURCE_CAPABILITY, "source_lock")
    if type(canonical) is not provenance.MpfiSourceLockV1:
        _fail(MpfiSourceInputReasonV1.FOREIGN_SOURCE_CAPABILITY, "source_lock")
    return canonical


def _fresh_admitted_sources_v1(
    source_lock: provenance.MpfiSourceLockV1,
    admitted_sources: provenance.AdmittedMpfiSourcesV1,
) -> provenance.ReplayedSourceClosureV1:
    if type(admitted_sources) is not provenance.AdmittedMpfiSourcesV1:
        _fail(MpfiSourceInputReasonV1.WRONG_TYPE, "admitted_sources")
    try:
        source_lock_identity = admitted_sources.source_lock_identity
    except Exception:
        # Exact type не делает retained capability неуязвимой к post-admission
        # подмене; ordinary hostile failure обязан остаться typed rejection.
        _fail(
            MpfiSourceInputReasonV1.FOREIGN_SOURCE_CAPABILITY,
            "admitted_sources",
        )
    bound = (
        type(source_lock_identity) is bytes
        and len(source_lock_identity) == 32
        and source_lock_identity != bytes(32)
        and source_lock_identity == source_lock.identity
    )
    if not bound:
        _fail(
            MpfiSourceInputReasonV1.FOREIGN_SOURCE_CAPABILITY,
            "admitted_sources",
        )
    # Replay owns nested archive evidence; its typed provenance taxonomy must
    # survive instead of being flattened into an MPFI declaration error.
    try:
        return provenance.replay_admitted_source_closure_v1(
            source_lock,
            admitted_sources,
        )
    except provenance.ProvenanceErrorV1 as error:
        if error.artifact == "source-closure-replay-v1":
            _fail(
                MpfiSourceInputReasonV1.FOREIGN_SOURCE_CAPABILITY,
                "admitted_sources",
            )
        raise
    except TypeError:
        _fail(MpfiSourceInputReasonV1.FOREIGN_SOURCE_CAPABILITY, "admitted_sources")


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
    snapshot: provenance.ReplayedSourceClosureV1,
) -> tuple[tuple[str, int, bytes], ...]:
    entries = tuple(
        (
            f"sources/{_SOURCE_NAMESPACE_V1[lock.role]}/{relative}",
            mode,
            contents,
        )
        for lock, materialized in zip(
            snapshot.source_lock.sources,
            snapshot.sources,
            strict=True,
        )
        for relative, mode, contents in materialized.files
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
    `ProvenanceErrorV1` — failure exact archive replay, а `InputErrorV1` —
    canonical USTAR или resource-bound rejection.
    """

    canonical_lock = _canonical_lock_v1(source_lock)
    canonical_limits = _canonical_limits_v1(limits)
    # Declared totals are authenticated by the canonical release lock.  Check
    # them before archive replay so an impossible caller budget cannot force
    # archive-body allocation merely to learn it is impossible.
    _preflight_declared_resource_bounds_v1(canonical_lock, canonical_limits)
    snapshot = _fresh_admitted_sources_v1(canonical_lock, admitted_sources)
    entries = _source_entries_v1(snapshot)
    contents = build_input.canonical_ustar_v1(entries, canonical_limits)
    return build_input.seal_input_v1(
        _binding_identity_v1(
            snapshot.source_lock,
            snapshot.admitted_sources,
            contents,
        ),
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
