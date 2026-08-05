#!/usr/bin/env python3
"""One-shot source → MPFI BUILD → RUN provenance boundary.

The receipt is provenance-only.  It does not certify interval semantics,
scientific correctness, or Arb/MPFI agreement; those remain separate protocol
admissions.  All source, build, runtime and executor coordinates are replayed
before sealing so a self-consistent forged observation cannot pass.
"""

from __future__ import annotations

import hashlib
import os
import threading
from dataclasses import dataclass, fields
from enum import StrEnum
from pathlib import Path
from typing import TypeAlias

from build import transport as build_transport
from build import input as build_input

import executor
import provenance
import region_proof_protocol as protocol
from mpfi import build as mpfi_build
from mpfi import runtime as mpfi_runtime


_BUILD_OBSERVATION_TOKEN = object()
_EVIDENCE_TOKEN = object()
_RECEIPT_TOKEN = object()
_NATIVE_BUILD_BACKEND_TYPE = build_transport.NativeDockerBuildBackendV1
_NATIVE_RUN_BACKEND_TYPE = executor.NativeLinuxBackendV1

_REQUEST_ID_LABEL_V1 = b"labcolors.proof-region.mpfi-request.v1\0"
_BUILD_ID_LABEL_V1 = b"labcolors.proof-region.mpfi-build-replay.v1\0"
_RUN_ID_LABEL_V1 = b"labcolors.proof-region.mpfi-run-replay.v1\0"
_EVIDENCE_ID_LABEL_V1 = b"labcolors.proof-region.mpfi-evaluator-replay.v1\0"
_POLICY_ID_LABEL_V1 = b"labcolors.proof-region.mpfi-source-bound-policy.v1\0"
_PREIMAGE_LABEL = b"labcolors.proof-region.mpfi-comparator-preimage.v1\0"
_MPFI_FAILURE_DETAIL_LIMIT_V1 = 4096
_MPFI_FAILURE_DETAIL_FALLBACK_V1 = "MPFI source-bound operation failed"


def _identity(label: bytes, chunks: tuple[bytes, ...]) -> bytes:
    payload = b"".join(
        len(chunk).to_bytes(8, "big") + chunk
        for chunk in chunks
    )
    return hashlib.sha256(label + len(payload).to_bytes(8, "big") + payload).digest()


def _preimage(role: str, chunks: tuple[bytes, ...]) -> bytes:
    return _identity(_PREIMAGE_LABEL + role.encode("ascii") + b"\0", chunks)


def _digest(value: object, field_name: str) -> bytes:
    if type(value) is not bytes or len(value) != 32 or value == bytes(32):
        raise TypeError(f"invalid {field_name}")
    return value


def _failure_detail_v1(value: object, *, diagnostic_repr: bool = False) -> str:
    """Keep rejection diagnostics bounded even when an adapter raises badly."""

    try:
        detail = repr(value) if diagnostic_repr else str(value)
    except Exception:
        return _MPFI_FAILURE_DETAIL_FALLBACK_V1
    if not detail or len(detail) > _MPFI_FAILURE_DETAIL_LIMIT_V1:
        return _MPFI_FAILURE_DETAIL_FALLBACK_V1
    return detail


class MpfiRequestErrorReasonV1(StrEnum):
    WRONG_TYPE = "wrong_type"
    FOREIGN_SOURCE_CAPABILITY = "foreign_source_capability"
    FORMULA_MISMATCH = "formula_mismatch"
    LIMIT_MISMATCH = "limit_mismatch"
    GENERATED_FORMULA_DRIFT = "generated_formula_drift"


@dataclass(frozen=True)
class MpfiRequestErrorV1(ValueError):
    reason: MpfiRequestErrorReasonV1
    field: str

    def __str__(self) -> str:
        return f"{self.reason.value}: {self.field}"


class MpfiPipelineRequestV1(tuple):
    """Exact detached inputs for one source-bound MPFI operation."""

    __slots__ = ()

    def __new__(
        cls,
        source_lock: provenance.MpfiSourceLockV1,
        admitted_sources: provenance.AdmittedMpfiSourcesV1,
        build_sources: mpfi_build.AdmittedMpfiBuildSourcesV1,
        generated_formula: bytes,
        build_limits: object,
        job: protocol.ProofJobV1,
        runtime_binding: mpfi_runtime.MpfiRuntimeBindingV1,
    ) -> MpfiPipelineRequestV1:
        if type(source_lock) is not provenance.MpfiSourceLockV1:
            raise MpfiRequestErrorV1(
                MpfiRequestErrorReasonV1.WRONG_TYPE,
                "source_lock",
            )
        if type(admitted_sources) is not provenance.AdmittedMpfiSourcesV1:
            raise MpfiRequestErrorV1(
                MpfiRequestErrorReasonV1.WRONG_TYPE,
                "admitted_sources",
            )
        if type(build_sources) is not mpfi_build.AdmittedMpfiBuildSourcesV1:
            raise MpfiRequestErrorV1(
                MpfiRequestErrorReasonV1.WRONG_TYPE,
                "build_sources",
            )
        if type(generated_formula) is not bytes:
            raise MpfiRequestErrorV1(
                MpfiRequestErrorReasonV1.WRONG_TYPE,
                "generated_formula",
            )
        if type(build_limits) is not build_input.CanonicalInputLimitsV1:
            raise MpfiRequestErrorV1(
                MpfiRequestErrorReasonV1.WRONG_TYPE,
                "build_limits",
            )
        if type(job) is not protocol.ProofJobV1:
            raise MpfiRequestErrorV1(MpfiRequestErrorReasonV1.WRONG_TYPE, "job")
        if type(runtime_binding) is not mpfi_runtime.MpfiRuntimeBindingV1:
            raise MpfiRequestErrorV1(
                MpfiRequestErrorReasonV1.WRONG_TYPE,
                "runtime_binding",
            )
        return tuple.__new__(
            cls,
            (
                source_lock,
                admitted_sources,
                build_sources,
                generated_formula,
                build_limits,
                job,
                runtime_binding,
            ),
        )

    source_lock = property(lambda self: self[0])
    admitted_sources = property(lambda self: self[1])
    build_sources = property(lambda self: self[2])
    generated_formula = property(lambda self: self[3])
    build_limits = property(lambda self: self[4])
    job = property(lambda self: self[5])
    runtime_binding = property(lambda self: self[6])


@dataclass(frozen=True)
class _MpfiOperationSnapshotV1:
    request: MpfiPipelineRequestV1
    source_closure: provenance.ReplayedSourceClosureV1


def _snapshot_request_v1(
    request: object,
) -> _MpfiOperationSnapshotV1:
    if type(request) is not MpfiPipelineRequestV1:
        raise MpfiRequestErrorV1(MpfiRequestErrorReasonV1.WRONG_TYPE, "request")
    try:
        source_lock = provenance.snapshot_source_closure_lock_v1(request.source_lock)
        admitted_sources = provenance.snapshot_admitted_source_closure_v1(
            source_lock,
            request.admitted_sources,
        )
        source_closure = provenance.replay_admitted_source_closure_v1(
            source_lock,
            admitted_sources,
        )
        if (
            type(source_closure.source_lock) is not provenance.MpfiSourceLockV1
            or type(source_closure.admitted_sources)
            is not provenance.AdmittedMpfiSourcesV1
        ):
            raise MpfiRequestErrorV1(
                MpfiRequestErrorReasonV1.FOREIGN_SOURCE_CAPABILITY,
                "admitted_sources",
            )
        build_sources = mpfi_build.canonical_build_sources_v1(request.build_sources)
        job = protocol.snapshot_proof_job_v1(request.job)
        build_limits = build_input.CanonicalInputLimitsV1(
            *tuple(request.build_limits)
        )
        runtime_binding = mpfi_runtime.MpfiRuntimeBindingV1(
            *tuple(request.runtime_binding)
        )
        if (
            build_sources.contents(mpfi_build.MPFI_FORMULA_SPEC_PATH_V1)
            != job.formula_spec
        ):
            raise MpfiRequestErrorV1(
                MpfiRequestErrorReasonV1.FORMULA_MISMATCH,
                "job",
            )
        if (
            hashlib.sha256(request.generated_formula).hexdigest()
            != mpfi_build.MPFI_GENERATED_FORMULA_SHA256_V1
        ):
            raise MpfiRequestErrorV1(
                MpfiRequestErrorReasonV1.GENERATED_FORMULA_DRIFT,
                "generated_formula",
            )
        if len(job.encode()) > runtime_binding.profile.max_job_bytes:
            raise MpfiRequestErrorV1(
                MpfiRequestErrorReasonV1.LIMIT_MISMATCH,
                "runtime_binding",
            )
        canonical_request = MpfiPipelineRequestV1(
            source_closure.source_lock,
            source_closure.admitted_sources,
            build_sources,
            request.generated_formula,
            build_limits,
            job,
            runtime_binding,
        )
        return _MpfiOperationSnapshotV1(canonical_request, source_closure)
    except MpfiRequestErrorV1:
        raise
    except (
        provenance.ProvenanceErrorV1,
        mpfi_build.MpfiBuildSourceErrorV1,
        protocol.ProtocolErrorV1,
        AttributeError,
        TypeError,
        ValueError,
        OverflowError,
    ) as error:
        raise MpfiRequestErrorV1(
            MpfiRequestErrorReasonV1.FOREIGN_SOURCE_CAPABILITY,
            "request",
        ) from error


@dataclass(frozen=True)
class MpfiComparatorPreimagesV1:
    engine_release: bytes
    upstream_source: bytes
    arithmetic_input_set: bytes
    wrapper_source: bytes
    evaluator_source: bytes
    build_identity: bytes
    operation_allowlist: bytes
    test_observation: bytes
    legal_file_set: bytes
    exclusions: bytes

    def __post_init__(self) -> None:
        values = tuple(getattr(self, item.name) for item in fields(self))
        if any(type(value) is not bytes or not value for value in values):
            raise TypeError("MPFI comparator preimages must be nonempty bytes")
        if len(set(values)) != len(values):
            raise TypeError("MPFI comparator preimages must be distinct")


@dataclass(frozen=True)
class MpfiDiagnosticComparatorV1:
    preimages: MpfiComparatorPreimagesV1
    manifest: protocol.ContentResolvedComparatorManifestV2
    source_identity: bytes
    build_source_identity: bytes
    runtime_binding_identity: bytes
    binary_sha256: bytes
    rebuild_sha256s: tuple[bytes, bytes]

    def __post_init__(self) -> None:
        if (
            type(self.manifest) is not protocol.ContentResolvedComparatorManifestV2
            or self.manifest.manifest.kind is not protocol.ComparatorKindV1.MPFI
            or tuple(item.name for item in fields(self.manifest.manifest) if item.name != "kind")
            != tuple(item.name for item in fields(self.preimages))
        ):
            raise TypeError("MPFI comparator manifest/preimages drift")
        _digest(self.source_identity, "source_identity")
        _digest(self.build_source_identity, "build_source_identity")
        _digest(self.runtime_binding_identity, "runtime_binding_identity")
        _digest(self.binary_sha256, "binary_sha256")
        if self.rebuild_sha256s != (self.binary_sha256, self.binary_sha256):
            raise TypeError("MPFI comparator rebuild binding drift")

    @property
    def identity(self) -> bytes:
        return self.manifest.identity


@dataclass(frozen=True)
class _MpfiBuildCoordinatesV1:
    """Private BUILD coordinates captured before comparator derivation."""

    source_identity: bytes
    build_source_identity: bytes
    generated_formula_sha256: bytes
    runtime_binding_identity: bytes
    docker_capability: build_transport.DockerSupportedV1
    input_bundle: build_input.SealedInputV1
    binary_sha256: bytes
    rebuild_sha256s: tuple[bytes, bytes]
    processes: tuple[
        build_transport.DockerBuildExitedV1,
        build_transport.DockerBuildExitedV1,
    ]
    binaries: tuple[bytes, bytes]


@dataclass(frozen=True, init=False)
class MpfiDiagnosticBuildObservationV1:
    source_identity: bytes
    build_source_identity: bytes
    generated_formula_sha256: bytes
    runtime_binding_identity: bytes
    docker_capability: build_transport.DockerSupportedV1
    input_bundle: build_input.SealedInputV1
    binary_sha256: bytes
    rebuild_sha256s: tuple[bytes, bytes]
    processes: tuple[
        build_transport.DockerBuildExitedV1,
        build_transport.DockerBuildExitedV1,
    ]
    binaries: tuple[bytes, bytes]
    comparator: MpfiDiagnosticComparatorV1

    def __new__(cls, *args: object, **kwargs: object) -> "MpfiDiagnosticBuildObservationV1":
        if kwargs.get("_token") is not _BUILD_OBSERVATION_TOKEN:
            raise TypeError("MpfiDiagnosticBuildObservationV1 is controller-derived")
        return object.__new__(cls)

    def __init__(
        self,
        source_identity: bytes,
        build_source_identity: bytes,
        generated_formula_sha256: bytes,
        runtime_binding_identity: bytes,
        docker_capability: build_transport.DockerSupportedV1,
        input_bundle: build_input.SealedInputV1,
        binary_sha256: bytes,
        rebuild_sha256s: tuple[bytes, bytes],
        processes: tuple[
            build_transport.DockerBuildExitedV1,
            build_transport.DockerBuildExitedV1,
        ],
        binaries: tuple[bytes, bytes],
        comparator: MpfiDiagnosticComparatorV1,
        *,
        _token: object,
    ) -> None:
        if _token is not _BUILD_OBSERVATION_TOKEN:
            raise TypeError("MpfiDiagnosticBuildObservationV1 is controller-derived")
        for name, value in (
            ("source_identity", source_identity),
            ("build_source_identity", build_source_identity),
            ("generated_formula_sha256", generated_formula_sha256),
            ("runtime_binding_identity", runtime_binding_identity),
            ("binary_sha256", binary_sha256),
        ):
            _digest(value, name)
        if type(docker_capability) is not build_transport.DockerSupportedV1:
            raise TypeError("invalid MPFI Docker capability")
        canonical_capability = build_transport.DockerSupportedV1(
            *tuple(docker_capability)
        )
        if tuple(canonical_capability) != tuple(docker_capability):
            raise TypeError("MPFI Docker capability did not replay")
        if not build_input.sealed_input_is_intact_v1(input_bundle):
            raise TypeError("invalid MPFI sealed build bundle")
        if (
            type(rebuild_sha256s) is not tuple
            or rebuild_sha256s != (binary_sha256, binary_sha256)
            or type(processes) is not tuple
            or len(processes) != 2
            or any(
                type(item) is not build_transport.DockerBuildExitedV1
                or item.returncode != 0
                for item in processes
            )
            or type(binaries) is not tuple
            or len(binaries) != 2
            or binaries[0] != binaries[1]
            or tuple(hashlib.sha256(item).digest() for item in binaries)
            != rebuild_sha256s
            or type(comparator) is not MpfiDiagnosticComparatorV1
            or comparator.source_identity != source_identity
            or comparator.build_source_identity != build_source_identity
            or comparator.runtime_binding_identity != runtime_binding_identity
            or comparator.binary_sha256 != binary_sha256
        ):
            raise TypeError("MPFI diagnostic BUILD observation drift")
        for process in processes:
            transfer = process.input_transfer
            if (
                type(transfer) is not build_transport.BuildInputTransferV1
                or transfer.bundle_identity != input_bundle.binding_identity
                or transfer.expected_length != input_bundle.length
                or transfer.expected_sha256 != input_bundle.sha256
                or transfer.written_length != input_bundle.length
                or transfer.written_sha256 != input_bundle.sha256
            ):
                raise TypeError("MPFI BUILD transfer did not consume sealed input")
        for name, value in (
            ("source_identity", source_identity),
            ("build_source_identity", build_source_identity),
            ("generated_formula_sha256", generated_formula_sha256),
            ("runtime_binding_identity", runtime_binding_identity),
            ("docker_capability", docker_capability),
            ("input_bundle", input_bundle),
            ("binary_sha256", binary_sha256),
            ("rebuild_sha256s", rebuild_sha256s),
            ("processes", processes),
            ("binaries", binaries),
            ("comparator", comparator),
        ):
            object.__setattr__(self, name, value)


def _source_identity_v1(snapshot: _MpfiOperationSnapshotV1) -> bytes:
    return mpfi_build.source_identity_v1(snapshot.source_closure)


def _build_identity_v1(
    snapshot: _MpfiOperationSnapshotV1,
    build: _MpfiBuildCoordinatesV1 | MpfiDiagnosticBuildObservationV1,
) -> bytes:
    policy_identity = build_transport.transport_policy_identity_v1(
        build.docker_capability.policy
    )
    process_bytes = tuple(
        build_transport.build_process_bytes_v1(item) for item in build.processes
    )
    runtime_identity = mpfi_runtime.runtime_binding_identity_v1(
        snapshot.request.runtime_binding
    )
    if type(runtime_identity) is not bytes:
        raise TypeError("runtime binding identity did not replay")
    return _identity(
        _BUILD_ID_LABEL_V1,
        (
            _source_identity_v1(snapshot),
            snapshot.request.build_sources.identity,
            hashlib.sha256(snapshot.request.generated_formula).digest(),
            runtime_identity,
            policy_identity,
            build_transport.docker_capability_identity_v1(build.docker_capability),
            build.input_bundle.binding_identity,
            build.input_bundle.sha256,
            build.input_bundle.length.to_bytes(8, "big"),
            *process_bytes,
            build.binary_sha256,
        ),
    )


def _derive_comparator_v1(
    snapshot: _MpfiOperationSnapshotV1,
    build: _MpfiBuildCoordinatesV1 | MpfiDiagnosticBuildObservationV1,
    build_identity: bytes,
) -> MpfiDiagnosticComparatorV1:
    files = snapshot.request.build_sources.files
    wrapper_paths = frozenset(
        f"proof/region/v1/mpfi/evaluator/{name}"
        for name in ("formula.h", "wire.h", "interval.h", "region.h", "hash.h")
    )
    wrapper = tuple(
        item
        for item in files
        if item.path in wrapper_paths
    )
    evaluator = tuple(
        item
        for item in files
        if item.path.startswith("proof/region/v1/mpfi/evaluator/")
        and item.path not in wrapper_paths
    )
    operation = snapshot.request.build_sources.contents(
        "proof/region/v1/mpfi/operations.py"
    )
    source_identity = _source_identity_v1(snapshot)
    runtime_identity = mpfi_runtime.runtime_binding_identity_v1(
        snapshot.request.runtime_binding
    )
    if type(runtime_identity) is not bytes:
        raise TypeError("runtime binding identity did not replay")
    preimages = MpfiComparatorPreimagesV1(
        _preimage("engine-release", (snapshot.request.source_lock.encode(),)),
        _preimage("upstream-source", (source_identity,)),
        _preimage(
            "arithmetic-input-set",
            (
                snapshot.request.job.formula_spec,
                snapshot.request.generated_formula,
                runtime_identity,
            ),
        ),
        _preimage(
            "wrapper-source",
            tuple(item.contents for item in wrapper),
        ),
        _preimage(
            "evaluator-source",
            tuple(item.contents for item in evaluator),
        ),
        _preimage("build-identity", (build_identity,)),
        _preimage("operation-allowlist", (operation,)),
        _preimage(
            "test-observation",
            (
                snapshot.request.build_sources.contents(mpfi_build.MPFI_BUILD_RECIPE_PATH_V1),
                snapshot.request.build_sources.contents(mpfi_build.MPFI_BUILD_INNER_RECIPE_PATH_V1),
                *(build_transport.build_process_bytes_v1(item) for item in build.processes),
            ),
        ),
        _preimage(
            "legal-file-set",
            tuple(lock.encode() for lock in snapshot.request.source_lock.sources),
        ),
        _preimage(
            "exclusions",
            (
                b"provenance-only",
                b"no-semantic-verifier",
                b"no-publisher-origin-claim",
            ),
        ),
    )
    coordinates = tuple(
        hashlib.sha256(getattr(preimages, field.name)).digest()
        for field in fields(preimages)
    )
    manifest = protocol.ContentResolvedComparatorManifestV2.admit(
        protocol.ComparatorManifestV2(protocol.ComparatorKindV1.MPFI, *coordinates),
        {
            coordinate: getattr(preimages, field.name)
            for coordinate, field in zip(coordinates, fields(preimages), strict=True)
        }.get,
    )
    return MpfiDiagnosticComparatorV1(
        preimages,
        manifest,
        source_identity,
        snapshot.request.build_sources.identity,
        runtime_identity,
        build.binary_sha256,
        build.rebuild_sha256s,
    )


@dataclass(frozen=True, init=False)
class MpfiEvaluatorReplayV1:
    request: MpfiPipelineRequestV1
    build: MpfiDiagnosticBuildObservationV1
    invocation: executor.ExecutionRequestV1
    platform: executor.SupportedV1
    process: executor.CompletedV1
    transcript: protocol.DecisionTranscriptV1
    run_claim: protocol.RunClaimV1
    source_identity: bytes
    build_identity: bytes
    run_identity: bytes
    identity: bytes

    def __new__(cls, *args: object, **kwargs: object) -> "MpfiEvaluatorReplayV1":
        if kwargs.get("_token") is not _EVIDENCE_TOKEN:
            raise TypeError("MpfiEvaluatorReplayV1 is controller-derived")
        return object.__new__(cls)

    def __init__(
        self,
        request: MpfiPipelineRequestV1,
        build: MpfiDiagnosticBuildObservationV1,
        invocation: executor.ExecutionRequestV1,
        platform: executor.SupportedV1,
        process: executor.CompletedV1,
        transcript: protocol.DecisionTranscriptV1,
        run_claim: protocol.RunClaimV1,
        source_identity: bytes,
        build_identity: bytes,
        run_identity: bytes,
        identity: bytes,
        *,
        _token: object,
    ) -> None:
        if _token is not _EVIDENCE_TOKEN:
            raise TypeError("MpfiEvaluatorReplayV1 is controller-derived")
        for name, value in (
            ("request", request),
            ("build", build),
            ("invocation", invocation),
            ("platform", platform),
            ("process", process),
            ("transcript", transcript),
            ("run_claim", run_claim),
            ("source_identity", source_identity),
            ("build_identity", build_identity),
            ("run_identity", run_identity),
            ("identity", identity),
        ):
            object.__setattr__(self, name, value)


def _run_identity_v1(
    snapshot: _MpfiOperationSnapshotV1,
    evidence: MpfiEvaluatorReplayV1,
) -> bytes:
    invocation_identity = executor.invocation_identity_v1(evidence.invocation)
    platform_identity = executor.platform_identity_v1(evidence.platform)
    if type(invocation_identity) is not bytes or type(platform_identity) is not bytes:
        raise TypeError("MPFI execution identity replay failed")
    return _identity(
        _RUN_ID_LABEL_V1,
        (
            evidence.build_identity,
            evidence.build.comparator.identity,
            snapshot.request.job.identity,
            invocation_identity,
            platform_identity,
            evidence.process.binary_sha256,
            evidence.process.stdout,
            evidence.process.stderr,
            evidence.transcript.identity,
            evidence.run_claim.identity,
        ),
    )


def replay_mpfi_evidence_is_well_bound_v1(value: object) -> bool:
    if type(value) is not MpfiEvaluatorReplayV1:
        return False
    try:
        snapshot = _snapshot_request_v1(value.request)
        if value.build.source_identity != _source_identity_v1(snapshot):
            return False
        runtime_identity = mpfi_runtime.runtime_binding_identity_v1(
            snapshot.request.runtime_binding
        )
        if (
            type(runtime_identity) is not bytes
            or value.build.build_source_identity
            != snapshot.request.build_sources.identity
            or value.build.generated_formula_sha256
            != hashlib.sha256(snapshot.request.generated_formula).digest()
            or value.build.runtime_binding_identity != runtime_identity
        ):
            return False
        if not mpfi_build.mpfi_build_input_is_bound_from_snapshot_v1(
            snapshot.source_closure,
            snapshot.request.build_sources,
            snapshot.request.generated_formula,
            snapshot.request.build_limits,
            value.build.input_bundle,
            value.build.docker_capability.policy,
        ):
            return False
        if (
            type(value.build.processes) is not tuple
            or len(value.build.processes) != 2
            or any(
                type(process) is not build_transport.DockerBuildExitedV1
                or process.returncode != 0
                or process.input_transfer.bundle_identity
                != value.build.input_bundle.binding_identity
                or process.input_transfer.expected_length
                != value.build.input_bundle.length
                or process.input_transfer.expected_sha256
                != value.build.input_bundle.sha256
                or process.input_transfer.written_length
                != value.build.input_bundle.length
                or process.input_transfer.written_sha256
                != value.build.input_bundle.sha256
                for process in value.build.processes
            )
            or type(value.build.binaries) is not tuple
            or len(value.build.binaries) != 2
            or value.build.binaries[0] != value.build.binaries[1]
            or tuple(hashlib.sha256(item).digest() for item in value.build.binaries)
            != value.build.rebuild_sha256s
            or value.build.binary_sha256 != value.build.rebuild_sha256s[0]
        ):
            return False
        expected_build_identity = _build_identity_v1(snapshot, value.build)
        if _preimage("build-identity", (expected_build_identity,)) != (
            value.build.comparator.preimages.build_identity
        ):
            return False
        expected_comparator = _derive_comparator_v1(
            snapshot,
            value.build,
            expected_build_identity,
        )
        if expected_comparator != value.build.comparator:
            return False
        if value.invocation.executable is not value.build.binaries[0]:
            return False
        if value.process.binary_sha256 != value.build.binary_sha256:
            return False
        if value.transcript.encode() != value.process.stdout:
            return False
        if (
            value.transcript.job_identity != snapshot.request.job.identity
            or value.transcript.comparator_identity != value.build.comparator.identity
            or value.process.stderr
            or not executor.result_matches_request_v1(value.process, value.invocation)
        ):
            return False
        invocation_identity = executor.invocation_identity_v1(value.invocation)
        platform_identity = executor.platform_identity_v1(value.platform)
        if type(invocation_identity) is not bytes or type(platform_identity) is not bytes:
            return False
        expected_claim = protocol.RunClaimV1.for_transcript(
            snapshot.request.job,
            value.build.comparator.manifest,
            value.transcript,
            value.build.binary_sha256,
            invocation_identity,
            platform_identity,
        )
        if expected_claim != value.run_claim:
            return False
        expected_run = _run_identity_v1(snapshot, value)
        expected_evidence = _identity(
            _EVIDENCE_ID_LABEL_V1,
            (_source_identity_v1(snapshot), expected_build_identity, expected_run),
        )
        return (
            value.source_identity == _source_identity_v1(snapshot)
            and value.build_identity == expected_build_identity
            and value.run_identity == expected_run
            and value.identity == expected_evidence
        )
    except Exception:
        return False


@dataclass(frozen=True, init=False)
class MpfiSourceBoundEvaluatorReceiptV1:
    claim: protocol.EvaluatorProvenanceClaimV1
    evidence: MpfiEvaluatorReplayV1

    def __new__(cls, *args: object, **kwargs: object) -> "MpfiSourceBoundEvaluatorReceiptV1":
        if kwargs.get("_token") is not _RECEIPT_TOKEN:
            raise TypeError("MpfiSourceBoundEvaluatorReceiptV1 is controller-sealed")
        return object.__new__(cls)

    def __init__(
        self,
        claim: protocol.EvaluatorProvenanceClaimV1,
        evidence: MpfiEvaluatorReplayV1,
        *,
        _token: object,
    ) -> None:
        if (
            _token is not _RECEIPT_TOKEN
            or type(claim) is not protocol.EvaluatorProvenanceClaimV1
            or type(evidence) is not MpfiEvaluatorReplayV1
            or not replay_mpfi_evidence_is_well_bound_v1(evidence)
        ):
            raise TypeError("MPFI receipt evidence is not replayable")
        expected_policy = mpfi_source_bound_policy_identity_v1(
            evidence.build.docker_capability,
            evidence.request.runtime_binding,
        )
        if (
            claim.provenance_policy_identity != expected_policy
            or claim.run_claim_identity != evidence.run_claim.identity
            or claim.replay_evidence_identity != evidence.identity
        ):
            raise TypeError("MPFI provenance claim does not bind evidence")
        object.__setattr__(self, "claim", claim)
        object.__setattr__(self, "evidence", evidence)

    @property
    def transcript(self) -> protocol.DecisionTranscriptV1:
        return self.evidence.transcript

    @property
    def run_claim(self) -> protocol.RunClaimV1:
        return self.evidence.run_claim

    @property
    def comparator(self) -> MpfiDiagnosticComparatorV1:
        return self.evidence.build.comparator

    @property
    def job(self) -> protocol.ProofJobV1:
        return self.evidence.request.job

    @property
    def executable(self) -> bytes:
        return self.evidence.build.binaries[0]

    @property
    def identity(self) -> bytes:
        return _identity(
            b"labcolors.proof-region.mpfi-receipt.v1\0",
            (self.claim.provenance_policy_identity, self.evidence.identity),
        )


def mpfi_source_bound_policy_identity_v1(
    capability: build_transport.DockerSupportedV1,
    runtime_binding: mpfi_runtime.MpfiRuntimeBindingV1,
) -> bytes:
    docker_identity = build_transport.docker_capability_identity_v1(capability)
    binding_identity = mpfi_runtime.runtime_binding_identity_v1(runtime_binding)
    if type(binding_identity) is not bytes:
        raise TypeError("runtime binding did not replay")
    return _identity(
        _POLICY_ID_LABEL_V1,
        (
            docker_identity,
            binding_identity,
            executor.SANDBOX_POLICY_RELEASE_V1.encode("ascii"),
            b"authority=one-shot-native-mpfi-controller",
            b"claim=provenance-only-no-semantic-verifier",
            b"trust=unsealed-linux-x64-docker-host",
        ),
    )


class MpfiSourceBoundFailureReasonV1(StrEnum):
    CONTROLLER_CONSUMED = "controller_consumed"
    CONTROLLER_PROCESS_CHANGED = "controller_process_changed"
    REQUEST_REJECTED = "request_rejected"
    OBSERVER_PLACEMENT_FAILED = "observer_placement_failed"
    BUILD_FAILED = "build_failed"
    RUN_FAILED = "run_failed"
    REPLAY_BINDING_FAILED = "replay_binding_failed"


@dataclass(frozen=True)
class MpfiSourceBoundRejectedV1:
    reason: MpfiSourceBoundFailureReasonV1
    detail: str

    def __post_init__(self) -> None:
        if type(self.reason) is not MpfiSourceBoundFailureReasonV1:
            raise TypeError("invalid MPFI source-bound failure reason")
        if (
            type(self.detail) is not str
            or not self.detail
            or len(self.detail) > _MPFI_FAILURE_DETAIL_LIMIT_V1
        ):
            raise TypeError("invalid MPFI source-bound failure detail")


MpfiSourceBoundResultV1: TypeAlias = (
    MpfiSourceBoundEvaluatorReceiptV1
    | MpfiSourceBoundRejectedV1
    | build_transport.BuildRejectedV1
    | build_transport.TwoBuildObservationV1
    | executor.ExecutionResultV1
)


class MpfiSourceBoundControllerV1:
    """One-shot authority for the MPFI source → BUILD → RUN chain."""

    def __init__(self, docker_path: Path, cgroup_parent: Path) -> None:
        if (
            not isinstance(docker_path, Path)
            or not docker_path.is_absolute()
            or not isinstance(cgroup_parent, Path)
            or not cgroup_parent.is_absolute()
        ):
            raise TypeError("controller paths must be absolute Path values")
        self._docker_path = docker_path
        self._cgroup_parent = cgroup_parent
        self._owner_pid = os.getpid()
        self._consumed = False
        self._lock = threading.Lock()

    def _consume(self) -> MpfiSourceBoundRejectedV1 | None:
        if os.getpid() != self._owner_pid:
            return MpfiSourceBoundRejectedV1(
                MpfiSourceBoundFailureReasonV1.CONTROLLER_PROCESS_CHANGED,
                "controller authority cannot cross a process boundary",
            )
        with self._lock:
            if self._consumed:
                return MpfiSourceBoundRejectedV1(
                    MpfiSourceBoundFailureReasonV1.CONTROLLER_CONSUMED,
                    "controller authority is one-shot",
                )
            self._consumed = True
        return None

    def execute(self, request: MpfiPipelineRequestV1) -> MpfiSourceBoundResultV1:
        consumed = self._consume()
        if consumed is not None:
            return consumed
        try:
            snapshot = _snapshot_request_v1(request)
            bundle = mpfi_build.seal_mpfi_build_input_from_snapshot_v1(
                snapshot.source_closure,
                snapshot.request.build_sources,
                snapshot.request.generated_formula,
                snapshot.request.build_limits,
            )
        except Exception as error:
            return MpfiSourceBoundRejectedV1(
                MpfiSourceBoundFailureReasonV1.REQUEST_REJECTED,
                _failure_detail_v1(error),
            )
        backend = build_transport.NativeDockerBuildBackendV1(
            self._docker_path,
            mpfi_build.MPFI_BUILD_TRANSPORT_POLICY_V1,
        )
        if type(backend) is not _NATIVE_BUILD_BACKEND_TYPE:
            return MpfiSourceBoundRejectedV1(
                MpfiSourceBoundFailureReasonV1.REPLAY_BINDING_FAILED,
                "native MPFI build backend authority changed",
            )
        transport = build_transport.ControlledBuildTransportV1(
            policy=mpfi_build.MPFI_BUILD_TRANSPORT_POLICY_V1,
            backend=backend,
        )
        capability = transport.probe()
        if type(capability) is not build_transport.DockerSupportedV1:
            return MpfiSourceBoundRejectedV1(
                MpfiSourceBoundFailureReasonV1.BUILD_FAILED,
                _failure_detail_v1(capability, diagnostic_repr=True),
            )
        built = transport.build(
            capability,
            bundle,
            snapshot.request.runtime_binding.limits.max_executable_bytes,
            input_admission=lambda value: mpfi_build.mpfi_build_input_is_bound_from_snapshot_v1(
                snapshot.source_closure,
                snapshot.request.build_sources,
                snapshot.request.generated_formula,
                snapshot.request.build_limits,
                value,
                capability.policy,
            ),
            output_admission=_static_binary_is_admitted_v1,
        )
        if type(built) is not build_transport.TwoBuildObservationV1:
            return built
        if built.relation is not build_transport.BuildByteRelationV1.IDENTICAL:
            return built
        binary = built.outputs[0]
        try:
            coordinates = _make_build_coordinates_v1(
                snapshot,
                capability,
                bundle,
                built,
                binary,
            )
            build_identity = _build_identity_v1(snapshot, coordinates)
            comparator = _derive_comparator_v1(snapshot, coordinates, build_identity)
            build = _make_build_observation_v1(coordinates, comparator)
        except Exception as error:
            return MpfiSourceBoundRejectedV1(
                MpfiSourceBoundFailureReasonV1.REPLAY_BINDING_FAILED,
                _failure_detail_v1(error),
            )
        try:
            executor.enter_observer_cgroup_v1(self._cgroup_parent)
        except Exception as error:
            return MpfiSourceBoundRejectedV1(
                MpfiSourceBoundFailureReasonV1.OBSERVER_PLACEMENT_FAILED,
                _failure_detail_v1(error),
            )
        run_backend = _NATIVE_RUN_BACKEND_TYPE(self._cgroup_parent)
        if type(run_backend) is not _NATIVE_RUN_BACKEND_TYPE:
            return MpfiSourceBoundRejectedV1(
                MpfiSourceBoundFailureReasonV1.REPLAY_BINDING_FAILED,
                "native MPFI run backend authority changed",
            )
        observed = executor.ControlledExecutorV1(run_backend)
        platform_value = observed.probe()
        if type(platform_value) is not executor.SupportedV1:
            return MpfiSourceBoundRejectedV1(
                MpfiSourceBoundFailureReasonV1.RUN_FAILED,
                _failure_detail_v1(platform_value, diagnostic_repr=True),
            )
        try:
            invocation = executor.ExecutionRequestV1(
                executable=binary,
                argv=(
                    b"mpfi-evaluator",
                    b"--manifest-identity",
                    build.comparator.identity.hex().encode("ascii"),
                    b"--job",
                    b"/dev/stdin",
                ),
                environment=((b"LC_ALL", b"C"), (b"TZ", b"UTC")),
                cwd=b"/",
                stdin=snapshot.request.job.encode(),
                umask=0o077,
                limits=snapshot.request.runtime_binding.limits,
            )
        except Exception as error:
            return MpfiSourceBoundRejectedV1(
                MpfiSourceBoundFailureReasonV1.RUN_FAILED,
                _failure_detail_v1(error),
            )
        process = observed.execute(invocation, platform_value)
        if (
            type(process) is not executor.CompletedV1
            or process.stderr
            or process.binary_sha256 != build.binary_sha256
            or not executor.result_matches_request_v1(process, invocation)
        ):
            return MpfiSourceBoundRejectedV1(
                MpfiSourceBoundFailureReasonV1.RUN_FAILED,
                _failure_detail_v1(process, diagnostic_repr=True),
            )
        try:
            transcript = protocol.DecisionTranscriptV1.parse(process.stdout)
            invocation_identity = executor.invocation_identity_v1(invocation)
            platform_identity = executor.platform_identity_v1(platform_value)
            if type(invocation_identity) is not bytes or type(platform_identity) is not bytes:
                raise TypeError("execution identity rejected")
            run_claim = protocol.RunClaimV1.for_transcript(
                snapshot.request.job,
                build.comparator.manifest,
                transcript,
                build.binary_sha256,
                invocation_identity,
                platform_identity,
            )
            provisional = MpfiEvaluatorReplayV1(
                snapshot.request,
                build,
                invocation,
                platform_value,
                process,
                transcript,
                run_claim,
                _source_identity_v1(snapshot),
                build_identity,
                b"\0" * 32,
                b"\0" * 32,
                _token=_EVIDENCE_TOKEN,
            )
            run_identity = _run_identity_v1(snapshot, provisional)
            evidence_identity = _identity(
                _EVIDENCE_ID_LABEL_V1,
                (_source_identity_v1(snapshot), build_identity, run_identity),
            )
            evidence = MpfiEvaluatorReplayV1(
                snapshot.request,
                build,
                invocation,
                platform_value,
                process,
                transcript,
                run_claim,
                _source_identity_v1(snapshot),
                build_identity,
                run_identity,
                evidence_identity,
                _token=_EVIDENCE_TOKEN,
            )
            if not replay_mpfi_evidence_is_well_bound_v1(evidence):
                raise TypeError("MPFI replay did not close")
            claim = protocol.EvaluatorProvenanceClaimV1(
                mpfi_source_bound_policy_identity_v1(
                    capability,
                    snapshot.request.runtime_binding,
                ),
                run_claim.identity,
                evidence.identity,
            )
            return MpfiSourceBoundEvaluatorReceiptV1(
                claim,
                evidence,
                _token=_RECEIPT_TOKEN,
            )
        except Exception as error:
            return MpfiSourceBoundRejectedV1(
                MpfiSourceBoundFailureReasonV1.REPLAY_BINDING_FAILED,
                _failure_detail_v1(error),
            )


def _static_binary_is_admitted_v1(value: bytes) -> bool:
    try:
        executor.require_static_x86_64_elf_v1(value)
    except executor.ExecutionRequestErrorV1:
        return False
    return True


def _make_build_coordinates_v1(
    snapshot: _MpfiOperationSnapshotV1,
    capability: build_transport.DockerSupportedV1,
    bundle: build_input.SealedInputV1,
    built: build_transport.TwoBuildObservationV1,
    binary: bytes,
) -> _MpfiBuildCoordinatesV1:
    binary_sha256 = hashlib.sha256(binary).digest()
    runtime_identity = mpfi_runtime.runtime_binding_identity_v1(
        snapshot.request.runtime_binding
    )
    if type(runtime_identity) is not bytes:
        raise TypeError("runtime binding identity did not replay")
    return _MpfiBuildCoordinatesV1(
        _source_identity_v1(snapshot),
        snapshot.request.build_sources.identity,
        hashlib.sha256(snapshot.request.generated_formula).digest(),
        runtime_identity,
        capability,
        bundle,
        binary_sha256,
        (binary_sha256, binary_sha256),
        built.processes,
        built.outputs,
    )


def _make_build_observation_v1(
    coordinates: _MpfiBuildCoordinatesV1,
    comparator: MpfiDiagnosticComparatorV1,
) -> MpfiDiagnosticBuildObservationV1:
    if type(coordinates) is not _MpfiBuildCoordinatesV1:
        raise TypeError("coordinates must be _MpfiBuildCoordinatesV1")
    return MpfiDiagnosticBuildObservationV1(
        coordinates.source_identity,
        coordinates.build_source_identity,
        coordinates.generated_formula_sha256,
        coordinates.runtime_binding_identity,
        coordinates.docker_capability,
        coordinates.input_bundle,
        coordinates.binary_sha256,
        coordinates.rebuild_sha256s,
        coordinates.processes,
        coordinates.binaries,
        comparator,
        _token=_BUILD_OBSERVATION_TOKEN,
    )
