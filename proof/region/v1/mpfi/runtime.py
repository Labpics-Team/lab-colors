#!/usr/bin/env python3
"""Каноническая связь MPFI runtime-профиля с executor limits.

Профиль описывает границу MPFI wire/runtime.  Executor остаётся общим leaf и
не знает о MPFI; эта lane-specific binding не даёт контроллеру случайно
запустить тот же бинарь с другими limits и назвать его тем же профилем.
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from enum import StrEnum
from typing import TypeAlias

import executor


MPFI_RUNTIME_PROFILE_ID_V1 = "LC-MPFI-RUNTIME-V1"

# Эти значения уже являются опубликованной M1.5 operational boundary в
# PROTOCOL.md и wire.h.  Здесь они собраны в одном типизированном источнике,
# чтобы build/run receipt связывал их с executor, а не копировал литералы.
MPFI_MAX_JOB_BYTES_V1 = 16 * 1024 * 1024
MPFI_MAX_OUTPUT_BYTES_V1 = 16 * 1024 * 1024
MPFI_MAX_PRECISION_BITS_V1 = 4096
MPFI_MAX_POLICY_RUNGS_V1 = 32
MPFI_MAX_KNOTS_V1 = 1024

_PROFILE_ID_LABEL_V1 = b"labcolors.proof-region.mpfi-runtime-profile.v1\0"
_BINDING_ID_LABEL_V1 = b"labcolors.proof-region.mpfi-runtime-binding.v1\0"
_PROFILE_VALUES_V1 = (
    MPFI_MAX_JOB_BYTES_V1,
    MPFI_MAX_OUTPUT_BYTES_V1,
    MPFI_MAX_PRECISION_BITS_V1,
    MPFI_MAX_POLICY_RUNGS_V1,
    MPFI_MAX_KNOTS_V1,
)


def _identity(label: bytes, chunks: tuple[bytes, ...]) -> bytes:
    payload = b"".join(
        len(chunk).to_bytes(8, "big") + chunk
        for chunk in chunks
    )
    return hashlib.sha256(label + len(payload).to_bytes(8, "big") + payload).digest()


class MpfiRuntimeProfileReasonV1(StrEnum):
    WRONG_TYPE = "wrong_type"
    NONCANONICAL = "noncanonical"
    LIMIT_MISMATCH = "limit_mismatch"


@dataclass(frozen=True)
class MpfiRuntimeIdentityRejectedV1:
    reason: MpfiRuntimeProfileReasonV1

    def __post_init__(self) -> None:
        if type(self.reason) is not MpfiRuntimeProfileReasonV1:
            raise TypeError("reason must be MpfiRuntimeProfileReasonV1")


class MpfiRuntimeProfileV1(tuple):
    """One immutable, exact V1 MPFI runtime profile."""

    __slots__ = ()

    def __new__(
        cls,
        max_job_bytes: int,
        max_output_bytes: int,
        max_precision_bits: int,
        max_policy_rungs: int,
        max_knots: int,
    ) -> MpfiRuntimeProfileV1:
        values = (
            max_job_bytes,
            max_output_bytes,
            max_precision_bits,
            max_policy_rungs,
            max_knots,
        )
        if any(type(value) is not int for value in values):
            raise TypeError("MPFI runtime profile coordinates must be exact ints")
        if values != _PROFILE_VALUES_V1:
            raise ValueError("unknown or noncanonical MPFI runtime profile")
        return tuple.__new__(cls, values)

    max_job_bytes = property(lambda self: self[0])
    max_output_bytes = property(lambda self: self[1])
    max_precision_bits = property(lambda self: self[2])
    max_policy_rungs = property(lambda self: self[3])
    max_knots = property(lambda self: self[4])


def mpfi_runtime_profile_v1() -> MpfiRuntimeProfileV1:
    return MpfiRuntimeProfileV1(*_PROFILE_VALUES_V1)


class MpfiRuntimeBindingV1(tuple):
    """Profile plus the exact immutable executor limits used by one RUN."""

    __slots__ = ()

    def __new__(
        cls,
        profile: MpfiRuntimeProfileV1,
        limits: executor.ExecutionLimitsV1,
    ) -> MpfiRuntimeBindingV1:
        if type(profile) is not MpfiRuntimeProfileV1:
            raise TypeError("profile must be MpfiRuntimeProfileV1")
        if type(limits) is not executor.ExecutionLimitsV1:
            raise TypeError("limits must be ExecutionLimitsV1")
        canonical_profile = MpfiRuntimeProfileV1(*tuple(profile))
        canonical_limits = executor.ExecutionLimitsV1(*tuple(limits))
        if tuple(canonical_profile) != tuple(profile) or tuple(canonical_limits) != tuple(limits):
            raise ValueError("runtime binding coordinates are not canonical")
        # Job and transcript ceilings are the two profile coordinates exposed
        # to the process.  Other executor limits stay explicit coordinates of
        # the same binding; they are not silently invented from MPFI semantics.
        if (
            canonical_limits.max_stdin_bytes != canonical_profile.max_job_bytes
            or canonical_limits.max_stdout_bytes != canonical_profile.max_output_bytes
        ):
            raise ValueError("executor limits do not implement MPFI profile")
        return tuple.__new__(cls, (canonical_profile, canonical_limits))

    profile = property(lambda self: self[0])
    limits = property(lambda self: self[1])


MpfiRuntimeIdentityResultV1: TypeAlias = bytes | MpfiRuntimeIdentityRejectedV1


def runtime_profile_identity_v1(
    value: object,
) -> MpfiRuntimeIdentityResultV1:
    if type(value) is not MpfiRuntimeProfileV1:
        return MpfiRuntimeIdentityRejectedV1(MpfiRuntimeProfileReasonV1.WRONG_TYPE)
    try:
        profile = MpfiRuntimeProfileV1(*tuple(value))
    except (TypeError, ValueError, OverflowError):
        return MpfiRuntimeIdentityRejectedV1(MpfiRuntimeProfileReasonV1.NONCANONICAL)
    return _identity(
        _PROFILE_ID_LABEL_V1,
        (
            MPFI_RUNTIME_PROFILE_ID_V1.encode("ascii"),
            *(item.to_bytes(8, "big") for item in profile),
        ),
    )


def runtime_binding_identity_v1(
    value: object,
) -> MpfiRuntimeIdentityResultV1:
    if type(value) is not MpfiRuntimeBindingV1:
        return MpfiRuntimeIdentityRejectedV1(MpfiRuntimeProfileReasonV1.WRONG_TYPE)
    try:
        binding = MpfiRuntimeBindingV1(*tuple(value))
        profile_identity = runtime_profile_identity_v1(binding.profile)
        limits_identity = _identity(
            b"labcolors.proof-region.execution-limits.v1\0",
            tuple(item.to_bytes(8, "big") for item in binding.limits),
        )
    except (TypeError, ValueError, OverflowError):
        return MpfiRuntimeIdentityRejectedV1(MpfiRuntimeProfileReasonV1.LIMIT_MISMATCH)
    if (
        type(profile_identity) is not bytes
        or type(limits_identity) is not bytes
    ):
        return MpfiRuntimeIdentityRejectedV1(MpfiRuntimeProfileReasonV1.LIMIT_MISMATCH)
    return _identity(
        _BINDING_ID_LABEL_V1,
        (
            profile_identity,
            limits_identity,
            executor.SANDBOX_POLICY_RELEASE_V1.encode("ascii"),
        ),
    )
