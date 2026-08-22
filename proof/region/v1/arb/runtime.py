#!/usr/bin/env python3
"""Каноническая связь Arb runtime-профиля с executor limits."""

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from enum import StrEnum
from typing import TypeAlias

import executor


ARB_RUNTIME_PROFILE_ID_V1 = "LC-ARB-RUNTIME-V1"

# Это versioned operational boundary прямого evaluator, а не математическая
# граница definition/domain. Те же координаты обязаны жить в wire.h; native
# conformance-тест связывает обе реализации exact-значениями.
ARB_MAX_JOB_BYTES_V1 = 16 * 1024 * 1024
ARB_MAX_OUTPUT_BYTES_V1 = 16 * 1024 * 1024
ARB_MAX_PRECISION_BITS_V1 = 4096
ARB_MAX_POLICY_RUNGS_V1 = 32
ARB_MAX_KNOTS_V1 = 1024
ARB_EXIT_USAGE_V1 = 64
ARB_EXIT_INPUT_REJECTED_V1 = 65
ARB_EXIT_INPUT_LIMIT_V1 = 66
ARB_EXIT_OUTPUT_LIMIT_V1 = 67
ARB_EXIT_RESOURCE_LIMIT_V1 = 68
ARB_EXIT_INTERNAL_V1 = 70
ARB_EXIT_IO_V1 = 74

_PROFILE_ID_LABEL_V1 = b"labcolors.proof-region.arb-runtime-profile.v1\0"
_BINDING_ID_LABEL_V1 = b"labcolors.proof-region.arb-runtime-binding.v1\0"
_EXECUTION_LIMITS_ID_LABEL_V1 = b"labcolors.proof-region.execution-limits.v1\0"
_PROFILE_VALUES_V1 = (
    ARB_MAX_JOB_BYTES_V1,
    ARB_MAX_OUTPUT_BYTES_V1,
    ARB_MAX_PRECISION_BITS_V1,
    ARB_MAX_POLICY_RUNGS_V1,
    ARB_MAX_KNOTS_V1,
)


def _identity(label: bytes, chunks: tuple[bytes, ...]) -> bytes:
    payload = b"".join(
        len(chunk).to_bytes(8, "big") + chunk
        for chunk in chunks
    )
    return hashlib.sha256(label + len(payload).to_bytes(8, "big") + payload).digest()


class ArbRuntimeProfileReasonV1(StrEnum):
    WRONG_TYPE = "wrong_type"
    NONCANONICAL = "noncanonical"
    LIMIT_MISMATCH = "limit_mismatch"


@dataclass(frozen=True)
class ArbRuntimeIdentityRejectedV1:
    reason: ArbRuntimeProfileReasonV1

    def __post_init__(self) -> None:
        if type(self.reason) is not ArbRuntimeProfileReasonV1:
            raise TypeError("reason must be ArbRuntimeProfileReasonV1")


class ArbRuntimeProfileV1(tuple):
    """Один immutable exact-профиль прямого Arb evaluator V1."""

    __slots__ = ()

    def __new__(
        cls,
        max_job_bytes: int,
        max_output_bytes: int,
        max_precision_bits: int,
        max_policy_rungs: int,
        max_knots: int,
    ) -> ArbRuntimeProfileV1:
        values = (
            max_job_bytes,
            max_output_bytes,
            max_precision_bits,
            max_policy_rungs,
            max_knots,
        )
        if any(type(value) is not int for value in values):
            raise TypeError("Arb runtime profile coordinates must be exact ints")
        if values != _PROFILE_VALUES_V1:
            raise ValueError("unknown or noncanonical Arb runtime profile")
        return tuple.__new__(cls, values)

    max_job_bytes = property(lambda self: self[0])
    max_output_bytes = property(lambda self: self[1])
    max_precision_bits = property(lambda self: self[2])
    max_policy_rungs = property(lambda self: self[3])
    max_knots = property(lambda self: self[4])


def arb_runtime_profile_v1() -> ArbRuntimeProfileV1:
    return ArbRuntimeProfileV1(*_PROFILE_VALUES_V1)


class ArbRuntimeBindingV1(tuple):
    """Профиль и exact immutable executor limits одного RUN."""

    __slots__ = ()

    def __new__(
        cls,
        profile: ArbRuntimeProfileV1,
        limits: executor.ExecutionLimitsV1,
    ) -> ArbRuntimeBindingV1:
        if type(profile) is not ArbRuntimeProfileV1:
            raise TypeError("profile must be ArbRuntimeProfileV1")
        if type(limits) is not executor.ExecutionLimitsV1:
            raise TypeError("limits must be ExecutionLimitsV1")
        canonical_profile = ArbRuntimeProfileV1(*tuple(profile))
        canonical_limits = executor.ExecutionLimitsV1(*tuple(limits))
        if tuple(canonical_profile) != tuple(profile) or tuple(canonical_limits) != tuple(limits):
            raise ValueError("runtime binding coordinates are not canonical")
        if (
            canonical_limits.max_stdin_bytes != canonical_profile.max_job_bytes
            or canonical_limits.max_stdout_bytes != canonical_profile.max_output_bytes
        ):
            raise ValueError("executor limits do not implement Arb profile")
        return tuple.__new__(cls, (canonical_profile, canonical_limits))

    profile = property(lambda self: self[0])
    limits = property(lambda self: self[1])


ArbRuntimeIdentityResultV1: TypeAlias = bytes | ArbRuntimeIdentityRejectedV1


def runtime_profile_identity_v1(value: object) -> ArbRuntimeIdentityResultV1:
    if type(value) is not ArbRuntimeProfileV1:
        return ArbRuntimeIdentityRejectedV1(ArbRuntimeProfileReasonV1.WRONG_TYPE)
    try:
        profile = ArbRuntimeProfileV1(*tuple(value))
    except (TypeError, ValueError, OverflowError):
        return ArbRuntimeIdentityRejectedV1(ArbRuntimeProfileReasonV1.NONCANONICAL)
    return _identity(
        _PROFILE_ID_LABEL_V1,
        (
            ARB_RUNTIME_PROFILE_ID_V1.encode("ascii"),
            *(item.to_bytes(8, "big") for item in profile),
        ),
    )


def runtime_binding_identity_v1(value: object) -> ArbRuntimeIdentityResultV1:
    if type(value) is not ArbRuntimeBindingV1:
        return ArbRuntimeIdentityRejectedV1(ArbRuntimeProfileReasonV1.WRONG_TYPE)
    try:
        binding = ArbRuntimeBindingV1(*tuple(value))
        profile_identity = runtime_profile_identity_v1(binding.profile)
        limits_identity = _identity(
            _EXECUTION_LIMITS_ID_LABEL_V1,
            tuple(item.to_bytes(8, "big") for item in binding.limits),
        )
    except (TypeError, ValueError, OverflowError):
        return ArbRuntimeIdentityRejectedV1(ArbRuntimeProfileReasonV1.LIMIT_MISMATCH)
    if type(profile_identity) is not bytes:
        return ArbRuntimeIdentityRejectedV1(ArbRuntimeProfileReasonV1.LIMIT_MISMATCH)
    return _identity(
        _BINDING_ID_LABEL_V1,
        (
            profile_identity,
            limits_identity,
            executor.SANDBOX_POLICY_RELEASE_V1.encode("ascii"),
        ),
    )
