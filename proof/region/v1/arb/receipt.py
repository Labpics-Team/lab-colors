#!/usr/bin/env python3
"""One controller-owned source → BUILD → RUN evidence boundary for Arb.

The receipt certifies only the causal observation assembled here.  It does not
classify colors, validate interval semantics, or mint a dual proof.  The Linux
host and Docker daemon remain declared V1 trust inputs.
"""

from __future__ import annotations

import hashlib
import os
import stat
import threading
from dataclasses import dataclass, fields
from enum import StrEnum
from pathlib import Path
from typing import TypeAlias

from build import transport as build_transport

import executor
import pipeline
import provenance
import region_proof_protocol as protocol


_EVIDENCE_TOKEN = object()
_RECEIPT_TOKEN = object()
_NATIVE_BUILD_BACKEND_TYPE = build_transport.NativeDockerBuildBackendV1
_NATIVE_RUN_BACKEND_TYPE = executor.NativeLinuxBackendV1

_SOURCE_ID_LABEL_V1 = b"labcolors.proof-region.arb-source-replay.v1\0"
_BUILD_ID_LABEL_V2 = b"labcolors.proof-region.arb-build-replay.v2\0"
_RUN_ID_LABEL_V1 = b"labcolors.proof-region.arb-run-replay.v1\0"
_EVIDENCE_ID_LABEL_V1 = b"labcolors.proof-region.arb-evaluator-replay.v1\0"
_EVIDENCE_STABILITY_LABEL_V1 = (
    b"labcolors.proof-region.arb-evaluator-stability.v1\0"
)
_SOURCE_BOUND_POLICY_ID_LABEL_V2 = (
    b"labcolors.proof-region.arb-source-bound-policy.v2\0"
)

_DIAGNOSTIC_BUILD_FIELDS_V1 = (
    "structural_source_identity",
    "flint_commit_content_identity",
    "flint_commit_content_file_count",
    "flint_project_pinned_release_only_identity",
    "flint_project_pinned_release_only_file_count",
    "build_input_identity",
    "formula_support_identity",
    "pipeline_policy_identity",
    "docker_capability",
    "binary_sha256",
    "rebuild_sha256s",
    "host_trust",
    "input_bundle_identity",
    "input_bundle_sha256",
    "input_bundle_length",
    "build_processes",
    "comparator",
    "_binary",
    "_rebuild_binaries",
    "_input_bundle",
)
_DIAGNOSTIC_COMPARATOR_FIELDS_V1 = (
    "preimages",
    "manifest",
    "structural_source_identity",
    "build_input_identity",
    "pipeline_policy_identity",
    "binary_sha256",
    "rebuild_sha256s",
)


def _blob(value: bytes) -> bytes:
    return len(value).to_bytes(8, "big") + value


def _identity(label: bytes, chunks: tuple[bytes, ...]) -> bytes:
    payload = b"".join(_blob(chunk) for chunk in chunks)
    return hashlib.sha256(label + len(payload).to_bytes(8, "big") + payload).digest()


def source_bound_policy_identity_v2(
    capability: build_transport.DockerSupportedV1,
    host_trust: pipeline.HostTrustBoundaryV1,
) -> bytes:
    """Identity of the exact observation rules and observed BUILD capability."""

    capability_identity = build_transport.docker_capability_identity_v1(capability)

    return _identity(
        _SOURCE_BOUND_POLICY_ID_LABEL_V2,
        (
            pipeline.pipeline_policy_identity_v2(
                host_trust,
                capability.policy,
            ),
            capability_identity,
            executor.SANDBOX_POLICY_RELEASE_V1.encode("ascii"),
            b"authority=one-shot-native-controller",
            b"source=lock-plus-owned-archive-and-build-input-replay",
            b"build=one-sealed-bundle-two-fresh-byte-equal-attempts",
            b"run=retained-executable-object-one-contained-process",
            b"identity=immutable-coordinates-total-rejection-v2",
            b"claim=provenance-only-no-numerical-semantics",
            b"trust=unsealed-linux-x64-host-native-docker-cli-and-daemon",
        ),
    )


def _source_identity_from_operation_v1(
    snapshot: pipeline._PipelineOperationSnapshotV1,
) -> bytes:
    """Derive source identity from one operation-owned materialization."""

    if type(snapshot) is not pipeline._PipelineOperationSnapshotV1:
        raise TypeError("source identity requires a pipeline operation snapshot")
    request = snapshot.request
    chunks: list[bytes] = [
        request.source_lock.encode(),
        request.source_lock.identity,
        request.admitted_sources.identity,
    ]
    for materialized in snapshot.source_closure.sources:
        chunks.extend(provenance._materialized_source_coordinates_v1(materialized))
    chunks.extend(
        (
            request.build_sources.identity,
            request.build_sources.build_input_identity,
            request.build_sources.formula_support_identity,
            pipeline.build_source_manifest_bytes_v1(request.build_sources),
        )
    )
    return _identity(_SOURCE_ID_LABEL_V1, tuple(chunks))


def _source_identity_v1(request: pipeline.PipelineRequestV1) -> bytes:
    """Independently derive source identity from a public request."""

    return _source_identity_from_operation_v1(
        pipeline._snapshot_pipeline_operation_v1(request)
    )


def _comparator_replays_from_operation_v1(
    snapshot: pipeline._PipelineOperationSnapshotV1,
    build: pipeline.DiagnosticBuildObservationV1,
) -> bool:
    try:
        if (
            type(snapshot) is not pipeline._PipelineOperationSnapshotV1
            or type(build) is not pipeline.DiagnosticBuildObservationV1
        ):
            return False
        expected = pipeline._derive_arb_comparator_for_build_v1(
            snapshot,
            build.docker_capability,
            build.binary,
            build.rebuild_sha256s,
            build.build_processes,
        )
        return build.comparator == expected
    except Exception:
        return False


def _build_identity_from_operation_v2(
    snapshot: pipeline._PipelineOperationSnapshotV1,
    source_identity: bytes,
    build: pipeline.DiagnosticBuildObservationV1,
) -> bytes:
    if type(snapshot) is not pipeline._PipelineOperationSnapshotV1:
        raise TypeError("build identity requires a pipeline operation snapshot")
    if type(build) is not pipeline.DiagnosticBuildObservationV1:
        raise TypeError("build replay requires DiagnosticBuildObservationV1")
    request = snapshot.request
    bundle = build.input_bundle
    processes = build.build_processes
    binaries = build.rebuild_binaries
    capability_identity = build_transport.docker_capability_identity_v1(
        build.docker_capability
    )
    flint_partition = pipeline._owned_flint_source_content_partition_v1(
        request.source_lock,
        request.admitted_sources,
    )
    expected_bundle = pipeline._seal_build_input_from_snapshot_v1(
        snapshot,
        build.docker_capability.policy,
    )
    if (
        build.structural_source_identity != request.admitted_sources.identity
        or build.flint_commit_content_identity
        != flint_partition.commit_content_identity
        or build.flint_commit_content_file_count
        != flint_partition.commit_content_file_count
        or build.flint_project_pinned_release_only_identity
        != flint_partition.project_pinned_release_only_identity
        or build.flint_project_pinned_release_only_file_count
        != flint_partition.project_pinned_release_only_file_count
        or build.build_input_identity != request.build_sources.build_input_identity
        or build.formula_support_identity
        != request.build_sources.formula_support_identity
        or build.pipeline_policy_identity
        != pipeline.pipeline_policy_identity_v2(
            request.host_trust,
            build.docker_capability.policy,
        )
        or build.host_trust is not request.host_trust
        or not pipeline._owned_arb_input_is_bound_v1(bundle, expected_bundle)
        or build.input_bundle_identity != bundle.binding_identity
        or build.input_bundle_sha256 != bundle.sha256
        or build.input_bundle_length != bundle.length
        or type(processes) is not tuple
        or len(processes) != 2
        or any(
            type(item) is not build_transport.DockerBuildExitedV1
            for item in processes
        )
        or type(binaries) is not tuple
        or len(binaries) != 2
        or binaries[0] is not processes[0].stdout
        or binaries[1] is not processes[1].stdout
        or binaries[0] != binaries[1]
        or build.binary is not binaries[0]
        or build.binary_sha256 != hashlib.sha256(build.binary).digest()
        or build.rebuild_sha256s != (build.binary_sha256, build.binary_sha256)
        or not _comparator_replays_from_operation_v1(snapshot, build)
    ):
        raise TypeError("controller-observed BUILD did not replay")
    for process in processes:
        transfer = process.input_transfer
        if (
            process.returncode != 0
            or type(transfer) is not build_transport.BuildInputTransferV1
            or transfer.bundle_identity != bundle.binding_identity
            or transfer.expected_length != bundle.length
            or transfer.expected_sha256 != bundle.sha256
            or transfer.written_length != bundle.length
            or transfer.written_sha256 != bundle.sha256
        ):
            raise TypeError("BUILD transfer did not consume the sealed bundle")
    return _identity(
        _BUILD_ID_LABEL_V2,
        (
            source_identity,
            build.pipeline_policy_identity,
            pipeline._host_trust_wire_v1(build.host_trust),
            capability_identity,
            bundle.binding_identity,
            bundle.sha256,
            bundle.length.to_bytes(8, "big"),
            build_transport.build_process_bytes_v1(processes[0]),
            build_transport.build_process_bytes_v1(processes[1]),
            build.binary_sha256,
            len(build.binary).to_bytes(8, "big"),
            build.comparator.identity,
        ),
    )


def _build_identity_v2(
    request: pipeline.PipelineRequestV1,
    source_identity: bytes,
    build: pipeline.DiagnosticBuildObservationV1,
) -> bytes:
    """Independently derive BUILD identity from a public request."""

    return _build_identity_from_operation_v2(
        pipeline._snapshot_pipeline_operation_v1(request),
        source_identity,
        build,
    )


def _run_identity_v1(
    request: pipeline.PipelineRequestV1,
    build: pipeline.DiagnosticBuildObservationV1,
    build_identity: bytes,
    invocation: executor.ExecutionRequestV1,
    platform_value: executor.SupportedV1,
    process: executor.CompletedV1,
    transcript: protocol.DecisionTranscriptV1,
    run_claim: protocol.RunClaimV1,
) -> bytes:
    expected_invocation = executor.ExecutionRequestV1(
        executable=build.binary,
        argv=(
            b"arb-evaluator",
            b"--manifest-identity",
            build.comparator.identity.hex().encode("ascii"),
            b"--job",
            b"/dev/stdin",
        ),
        environment=((b"LC_ALL", b"C"), (b"TZ", b"UTC")),
        cwd=b"/",
        stdin=request.job.encode(),
        umask=0o077,
        limits=request.execution_limits,
    )
    if (
        type(invocation) is not executor.ExecutionRequestV1
        or type(platform_value) is not executor.SupportedV1
        or platform_value.platform != executor.EXECUTION_PLATFORM_V1
        or platform_value.sandbox_policy_release
        != executor.SANDBOX_POLICY_RELEASE_V1
        or type(process) is not executor.CompletedV1
        or type(transcript) is not protocol.DecisionTranscriptV1
        or type(run_claim) is not protocol.RunClaimV1
        or invocation != expected_invocation
        or invocation.executable is not build.binary
        or process.binary_sha256 != build.binary_sha256
        or not executor.result_matches_request_v1(process, invocation)
        or process.stderr
        or transcript.encode() != process.stdout
        or transcript.job_identity != request.job.identity
        or transcript.domain_identity != request.job.domain.identity
        or transcript.comparator_identity != build.comparator.identity
        or transcript.point_count != request.job.domain.point_count
    ):
        raise TypeError("controller-observed RUN did not replay")
    parsed = protocol.DecisionTranscriptV1.parse(process.stdout)
    if parsed.encode() != process.stdout or parsed.identity != transcript.identity:
        raise TypeError("RUN stdout is not the retained canonical transcript")
    protocol.validate_witness_alignment_v1(
        request.job.domain,
        transcript.decision_bits,
        transcript.point_count,
        transcript.counters,
        transcript.witness_store,
    )
    invocation_identity = executor.invocation_identity_v1(invocation)
    platform_identity = executor.platform_identity_v1(platform_value)
    if (
        type(invocation_identity) is not bytes
        or type(platform_identity) is not bytes
    ):
        raise TypeError("execution identity replay was rejected")
    expected_claim = protocol.RunClaimV1.for_transcript(
        request.job,
        build.comparator.manifest,
        transcript,
        build.binary_sha256,
        invocation_identity,
        platform_identity,
    )
    if expected_claim != run_claim:
        raise TypeError("RunClaimV1 did not replay")
    process_identity = _identity(
        b"labcolors.proof-region.arb-run-process.v1\0",
        (
            process.binary_sha256,
            process.stdout,
            process.stderr,
        ),
    )
    return _identity(
        _RUN_ID_LABEL_V1,
        (
            build_identity,
            build.comparator.identity,
            request.job.identity,
            invocation_identity,
            platform_identity,
            process_identity,
            transcript.identity,
            run_claim.identity,
        ),
    )


@dataclass(frozen=True, init=False)
class ContentResolvedEvaluatorReplayV1:
    """Immutable DAG whose three identities commit source, BUILD and RUN edges."""

    request: pipeline.PipelineRequestV1
    build: pipeline.DiagnosticBuildObservationV1
    invocation: executor.ExecutionRequestV1
    platform: executor.SupportedV1
    process: executor.CompletedV1
    transcript: protocol.DecisionTranscriptV1
    run_claim: protocol.RunClaimV1
    source_identity: bytes
    build_identity: bytes
    run_identity: bytes
    _identity: bytes

    def __new__(cls, *args: object, **kwargs: object) -> "ContentResolvedEvaluatorReplayV1":
        if kwargs.get("_token") is not _EVIDENCE_TOKEN:
            raise TypeError("ContentResolvedEvaluatorReplayV1 is controller-derived")
        return object.__new__(cls)

    def __init__(
        self,
        request: pipeline.PipelineRequestV1,
        build: pipeline.DiagnosticBuildObservationV1,
        invocation: executor.ExecutionRequestV1,
        platform_value: executor.SupportedV1,
        process: executor.CompletedV1,
        transcript: protocol.DecisionTranscriptV1,
        run_claim: protocol.RunClaimV1,
        *,
        _operation: pipeline._PipelineOperationSnapshotV1,
        _token: object,
    ) -> None:
        if (
            _token is not _EVIDENCE_TOKEN
            or type(_operation) is not pipeline._PipelineOperationSnapshotV1
            or _operation.request is not request
        ):
            raise TypeError("ContentResolvedEvaluatorReplayV1 is controller-derived")
        source_identity = _source_identity_from_operation_v1(_operation)
        build_identity = _build_identity_from_operation_v2(
            _operation,
            source_identity,
            build,
        )
        run_identity = _run_identity_v1(
            request,
            build,
            build_identity,
            invocation,
            platform_value,
            process,
            transcript,
            run_claim,
        )
        identity = _identity(
            _EVIDENCE_ID_LABEL_V1,
            (source_identity, build_identity, run_identity),
        )
        for name, value in (
            ("request", request),
            ("build", build),
            ("invocation", invocation),
            ("platform", platform_value),
            ("process", process),
            ("transcript", transcript),
            ("run_claim", run_claim),
            ("source_identity", source_identity),
            ("build_identity", build_identity),
            ("run_identity", run_identity),
            ("_identity", identity),
        ):
            object.__setattr__(self, name, value)

    @property
    def executable(self) -> bytes:
        return self.build.binary

    @property
    def identity(self) -> bytes:
        return self._identity


@dataclass(frozen=True)
class _EvidenceFieldsV1:
    """One non-reentrant observation of every public evidence coordinate."""

    request: pipeline.PipelineRequestV1
    build: pipeline.DiagnosticBuildObservationV1
    invocation: executor.ExecutionRequestV1
    platform: executor.SupportedV1
    process: executor.CompletedV1
    transcript: protocol.DecisionTranscriptV1
    run_claim: protocol.RunClaimV1
    source_identity: bytes
    build_identity: bytes
    run_identity: bytes
    identity: bytes


def _capture_evidence_fields_v1(value: object) -> _EvidenceFieldsV1:
    """Read all public fields before replay can re-enter a hostile fixture."""

    if type(value) is not ContentResolvedEvaluatorReplayV1:
        raise TypeError("evidence must be ContentResolvedEvaluatorReplayV1")
    return _EvidenceFieldsV1(
        value.request,
        value.build,
        value.invocation,
        value.platform,
        value.process,
        value.transcript,
        value.run_claim,
        value.source_identity,
        value.build_identity,
        value.run_identity,
        value._identity,
    )


def _same_evidence_references_v1(
    first: _EvidenceFieldsV1,
    second: _EvidenceFieldsV1,
) -> bool:
    """Reject replacement even when an attacker chooses equal-looking values."""

    return (
        first.request is second.request
        and first.build is second.build
        and first.invocation is second.invocation
        and first.platform is second.platform
        and first.process is second.process
        and first.transcript is second.transcript
        and first.run_claim is second.run_claim
        and first.source_identity is second.source_identity
        and first.build_identity is second.build_identity
        and first.run_identity is second.run_identity
        and first.identity is second.identity
    )


def _request_replay_coordinates_v1(
    request: pipeline.PipelineRequestV1,
) -> tuple[bytes, ...]:
    """Project a request without reopening a second materialized source closure."""

    if type(request) is not pipeline.PipelineRequestV1:
        raise TypeError("request must be PipelineRequestV1")
    source_lock = request.source_lock
    admitted_sources = request.admitted_sources
    build_sources = request.build_sources
    job = request.job
    execution_limits = request.execution_limits
    if (
        type(source_lock) is not provenance.ArbSourceLockV1
        or type(admitted_sources) is not provenance.AdmittedArbSourcesV1
        or type(build_sources) is not pipeline.AdmittedBuildSourcesV1
        or type(job) is not protocol.ProofJobV1
        or type(execution_limits) is not executor.ExecutionLimitsV1
        or admitted_sources.source_lock_identity != source_lock.identity
        or type(source_lock.sources) is not tuple
        or type(admitted_sources.sources) is not tuple
        or len(source_lock.sources) != provenance.SOURCE_CLOSURE_COUNT_V1
        or len(admitted_sources.sources) != provenance.SOURCE_CLOSURE_COUNT_V1
    ):
        raise TypeError("request coordinates are not canonical")
    source_coordinates: list[bytes] = []
    for lock, source in zip(
        source_lock.sources,
        admitted_sources.sources,
        strict=True,
    ):
        if (
            type(lock) is not provenance.SourceReleaseLockV1
            or type(source) is not provenance.SafeSourceArchiveV1
        ):
            raise TypeError("request source coordinates are not canonical")
        archive = source.archive_bytes
        if (
            type(archive) is not bytes
            or type(source.source_lock_identity) is not bytes
            or type(source.archive_sha256) is not bytes
            or type(source.tree_identity) is not bytes
            or type(source.regular_file_count) is not int
            or type(source.regular_file_bytes) is not int
            or source.source_lock_identity != lock.identity
            or source.archive_sha256 != lock.archive_sha256
            or source.regular_file_count != lock.regular_file_count
            or source.regular_file_bytes != lock.regular_file_bytes
            or source.archive_sha256 != hashlib.sha256(archive).digest()
        ):
            raise TypeError("retained source coordinates changed")
        source_coordinates.extend(
            provenance._source_archive_coordinates_from_replayed_v1(lock, source)
        )
    canonical_job = _canonical_proof_job_with_coherent_identities_v1(job)
    canonical_limits = executor.ExecutionLimitsV1(*tuple(execution_limits))
    return (
        source_lock.encode(),
        source_lock.identity,
        admitted_sources.identity,
        *source_coordinates,
        build_sources.identity,
        build_sources.build_input_identity,
        build_sources.formula_support_identity,
        pipeline.build_source_manifest_bytes_v1(build_sources),
        canonical_job.encode(),
        canonical_job.identity,
        *(
            value.to_bytes(8, "big")
            for value in canonical_limits
        ),
        pipeline._host_trust_wire_v1(request.host_trust),
    )


def _exact_digest_v1(value: object, field_name: str) -> bytes:
    if type(value) is not bytes or len(value) != 32 or value == bytes(32):
        raise TypeError(f"invalid {field_name}")
    return value


def _require_canonical_digest_v1(
    retained: object,
    canonical: object,
    field_name: str,
) -> None:
    """Reject an observable identity cache that disagrees with fresh wire state."""

    if _exact_digest_v1(retained, field_name) != _exact_digest_v1(
        canonical,
        f"canonical {field_name}",
    ):
        raise TypeError(f"{field_name} does not match canonical wire state")


def _canonical_proof_job_with_coherent_identities_v1(
    value: object,
) -> protocol.ProofJobV1:
    """Detach a job and require every identity cache used by its wire to agree.

    ``cached_property`` is a performance detail, not an authority boundary:
    frozen public protocol values still expose a writable ``__dict__`` to
    hostile callers.  The detached snapshot supplies the cache-free oracle.
    """

    if type(value) is not protocol.ProofJobV1:
        raise TypeError("request job must be ProofJobV1")
    canonical = protocol.snapshot_proof_job_v1(value)
    _require_canonical_digest_v1(
        value.definition.definition_digest,
        canonical.definition.definition_digest,
        "request definition digest",
    )
    _require_canonical_digest_v1(
        value.domain.identity,
        canonical.domain.identity,
        "request domain identity",
    )
    _require_canonical_digest_v1(
        value.policy.identity,
        canonical.policy.identity,
        "request policy identity",
    )
    _require_canonical_digest_v1(
        value.identity,
        canonical.identity,
        "request job identity",
    )
    return canonical


def _bytes_stability_coordinate_v1(value: object, field_name: str) -> bytes:
    if type(value) is not bytes:
        raise TypeError(f"invalid {field_name}")
    return len(value).to_bytes(8, "big") + hashlib.sha256(value).digest()


def _build_stability_coordinates_v1(
    build: pipeline.DiagnosticBuildObservationV1,
) -> tuple[bytes, ...]:
    """Capture every mutable BUILD observation coordinate without source replay."""

    if (
        type(build) is not pipeline.DiagnosticBuildObservationV1
        or tuple(field.name for field in fields(build))
        != _DIAGNOSTIC_BUILD_FIELDS_V1
    ):
        raise TypeError("diagnostic BUILD schema is not canonical V1")
    scalar_digests = tuple(
        _exact_digest_v1(getattr(build, name), name)
        for name in (
            "structural_source_identity",
            "flint_commit_content_identity",
            "flint_project_pinned_release_only_identity",
            "build_input_identity",
            "formula_support_identity",
            "pipeline_policy_identity",
            "binary_sha256",
            "input_bundle_identity",
            "input_bundle_sha256",
        )
    )
    counts = (
        build.flint_commit_content_file_count,
        build.flint_project_pinned_release_only_file_count,
        build.input_bundle_length,
    )
    if any(type(value) is not int or value <= 0 for value in counts):
        raise TypeError("invalid diagnostic BUILD count")
    rebuild_sha256s = build.rebuild_sha256s
    binary = build.binary
    input_bundle = build.input_bundle
    processes = build.build_processes
    comparator = build.comparator
    if (
        type(binary) is not bytes
        or type(rebuild_sha256s) is not tuple
        or len(rebuild_sha256s) != 2
        or any(
            _exact_digest_v1(value, "rebuild_sha256") != build.binary_sha256
            for value in rebuild_sha256s
        )
        or type(processes) is not tuple
        or len(processes) != 2
        or any(
            type(process) is not build_transport.DockerBuildExitedV1
            for process in processes
        )
        or type(comparator) is not pipeline.DiagnosticArbComparatorV1
        or tuple(field.name for field in fields(comparator))
        != _DIAGNOSTIC_COMPARATOR_FIELDS_V1
    ):
        raise TypeError("invalid diagnostic BUILD observation")
    rebuild_binaries = build.rebuild_binaries
    if (
        type(rebuild_binaries) is not tuple
        or len(rebuild_binaries) != 2
        or any(type(value) is not bytes for value in rebuild_binaries)
    ):
        raise TypeError("diagnostic BUILD executable binding changed")
    if (
        input_bundle.binding_identity != build.input_bundle_identity
        or input_bundle.sha256 != build.input_bundle_sha256
        or input_bundle.length != build.input_bundle_length
        or type(input_bundle.contents) is not bytes
        or hashlib.sha256(input_bundle.contents).digest() != input_bundle.sha256
    ):
        raise TypeError("diagnostic BUILD input bundle changed")
    preimages = comparator.preimages
    manifest = comparator.manifest
    if (
        type(preimages) is not pipeline.ArbComparatorPreimagesV1
        or type(manifest) is not protocol.ContentResolvedComparatorManifestV2
        or type(manifest.manifest) is not protocol.ComparatorManifestV2
        or comparator.structural_source_identity != build.structural_source_identity
        or comparator.build_input_identity != build.build_input_identity
        or comparator.pipeline_policy_identity != build.pipeline_policy_identity
        or comparator.binary_sha256 != build.binary_sha256
        or comparator.rebuild_sha256s != rebuild_sha256s
    ):
        raise TypeError("diagnostic BUILD comparator binding changed")
    manifest_bytes = manifest.manifest.encode()
    parsed_manifest = protocol.ComparatorManifestV2.parse(manifest_bytes)
    preimage_coordinates = tuple(
        _bytes_stability_coordinate_v1(
            getattr(preimages, field.name),
            f"comparator preimage {field.name}",
        )
        for field in fields(preimages)
    )
    resolved_manifest = protocol.ContentResolvedComparatorManifestV2.admit(
        parsed_manifest,
        {
            hashlib.sha256(getattr(preimages, field.name)).digest(): getattr(
                preimages,
                field.name,
            )
            for field in fields(preimages)
        }.get,
    )
    if resolved_manifest.manifest.encode() != manifest_bytes:
        raise TypeError("diagnostic BUILD manifest changed")
    _require_canonical_digest_v1(
        manifest.manifest.identity,
        parsed_manifest.identity,
        "diagnostic BUILD manifest identity",
    )
    _require_canonical_digest_v1(
        manifest.identity,
        resolved_manifest.identity,
        "diagnostic BUILD resolved manifest identity",
    )
    _require_canonical_digest_v1(
        comparator.identity,
        resolved_manifest.identity,
        "diagnostic BUILD comparator identity",
    )
    return (
        *scalar_digests,
        *(value.to_bytes(8, "big") for value in counts),
        build_transport.docker_capability_identity_v1(build.docker_capability),
        pipeline._host_trust_wire_v1(build.host_trust),
        _bytes_stability_coordinate_v1(binary, "diagnostic BUILD binary"),
        bytes(
            (
                binary is rebuild_binaries[0],
                binary is processes[0].stdout,
                rebuild_binaries[1] is processes[1].stdout,
                hashlib.sha256(binary).digest() == build.binary_sha256,
            )
        ),
        *(
            _bytes_stability_coordinate_v1(value, "diagnostic rebuild binary")
            for value in rebuild_binaries
        ),
        input_bundle.binding_identity,
        input_bundle.sha256,
        input_bundle.length.to_bytes(8, "big"),
        _bytes_stability_coordinate_v1(
            input_bundle.contents,
            "diagnostic BUILD input bytes",
        ),
        build_transport.build_process_bytes_v1(processes[0]),
        build_transport.build_process_bytes_v1(processes[1]),
        manifest_bytes,
        *preimage_coordinates,
        comparator.structural_source_identity,
        comparator.build_input_identity,
        comparator.pipeline_policy_identity,
        comparator.binary_sha256,
        *comparator.rebuild_sha256s,
    )


def _evidence_stability_coordinates_v1(fields_value: _EvidenceFieldsV1) -> bytes:
    """Bind the entry-to-exit value state; this is not a receipt identity."""

    invocation_identity = executor.invocation_identity_v1(fields_value.invocation)
    platform_identity = executor.platform_identity_v1(fields_value.platform)
    if type(invocation_identity) is not bytes or type(platform_identity) is not bytes:
        raise TypeError("execution coordinates did not replay")
    process = fields_value.process
    if (
        type(process) is not executor.CompletedV1
        or type(process.stdout) is not bytes
        or type(process.stderr) is not bytes
    ):
        raise TypeError("RUN observation is not structurally bound")
    transcript = fields_value.transcript
    run_claim = fields_value.run_claim
    if (
        type(transcript) is not protocol.DecisionTranscriptV1
        or type(run_claim) is not protocol.RunClaimV1
    ):
        raise TypeError("RUN protocol observations are not canonical")
    transcript_bytes = transcript.encode()
    canonical_transcript = protocol.DecisionTranscriptV1.parse(transcript_bytes)
    run_claim_bytes = run_claim.encode()
    canonical_claim = protocol.RunClaimV1.parse(run_claim_bytes)
    if (
        canonical_transcript.encode() != transcript_bytes
        or canonical_claim.encode() != run_claim_bytes
    ):
        raise TypeError("RUN protocol bindings changed")
    _require_canonical_digest_v1(
        transcript.identity,
        canonical_transcript.identity,
        "RUN transcript identity",
    )
    _require_canonical_digest_v1(
        run_claim.identity,
        canonical_claim.identity,
        "RUN claim identity",
    )
    return _identity(
        _EVIDENCE_STABILITY_LABEL_V1,
        (
            _identity(
                b"labcolors.proof-region.arb-request-stability.v1\0",
                _request_replay_coordinates_v1(fields_value.request),
            ),
            _identity(
                b"labcolors.proof-region.arb-build-stability.v1\0",
                _build_stability_coordinates_v1(fields_value.build),
            ),
            invocation_identity,
            platform_identity,
            process.binary_sha256,
            _bytes_stability_coordinate_v1(process.stdout, "RUN stdout"),
            _bytes_stability_coordinate_v1(process.stderr, "RUN stderr"),
            _bytes_stability_coordinate_v1(transcript_bytes, "RUN transcript"),
            _bytes_stability_coordinate_v1(run_claim_bytes, "RUN claim"),
            bytes(
                (
                    fields_value.invocation.executable is fields_value.build.binary,
                    process.binary_sha256 == fields_value.build.binary_sha256,
                    transcript_bytes == process.stdout,
                    canonical_claim.binary_identity
                    == fields_value.build.binary_sha256,
                    canonical_claim.invocation_identity == invocation_identity,
                    canonical_claim.platform_identity == platform_identity,
                    canonical_claim.transcript_identity
                    == canonical_transcript.identity,
                )
            ),
            _exact_digest_v1(fields_value.source_identity, "source identity"),
            _exact_digest_v1(fields_value.build_identity, "build identity"),
            _exact_digest_v1(fields_value.run_identity, "run identity"),
            _exact_digest_v1(fields_value.identity, "evidence identity"),
        ),
    )


def _replay_evidence_fields_v1(
    operation: pipeline._PipelineOperationSnapshotV1,
    fields: _EvidenceFieldsV1,
) -> tuple[bytes, bytes, bytes, bytes]:
    """Re-derive the three evidence edges from one owned source operation."""

    if any(
        type(value) is not bytes
        for value in (
            fields.source_identity,
            fields.build_identity,
            fields.run_identity,
            fields.identity,
        )
    ):
        raise TypeError("evidence identities must be exact bytes")
    request = operation.request
    source_identity = _source_identity_from_operation_v1(operation)
    build_identity = _build_identity_from_operation_v2(
        operation,
        source_identity,
        fields.build,
    )
    run_identity = _run_identity_v1(
        request,
        fields.build,
        build_identity,
        fields.invocation,
        fields.platform,
        fields.process,
        fields.transcript,
        fields.run_claim,
    )
    identity = _identity(
        _EVIDENCE_ID_LABEL_V1,
        (source_identity, build_identity, run_identity),
    )
    if (
        fields.source_identity != source_identity
        or fields.build_identity != build_identity
        or fields.run_identity != run_identity
        or fields.identity != identity
    ):
        raise TypeError("evidence identity did not replay")
    return source_identity, build_identity, run_identity, identity


def replay_evidence_is_well_bound_v1(value: object) -> bool:
    try:
        before = _capture_evidence_fields_v1(value)
        before_stability = _evidence_stability_coordinates_v1(before)
        operation = pipeline._snapshot_pipeline_operation_v1(before.request)
        expected_request = _request_replay_coordinates_v1(operation.request)
        first = _replay_evidence_fields_v1(operation, before)

        middle = _capture_evidence_fields_v1(value)
        if (
            not _same_evidence_references_v1(before, middle)
            or _evidence_stability_coordinates_v1(middle) != before_stability
            or _request_replay_coordinates_v1(before.request) != expected_request
        ):
            return False

        # The source operation has one retained materialization.  A second
        # edge replay and a second structural projection close the interval in
        # which a mutable public object could otherwise change mid-check.
        second = _replay_evidence_fields_v1(operation, middle)
        after = _capture_evidence_fields_v1(value)
        return (
            first == second
            and _same_evidence_references_v1(middle, after)
            and _evidence_stability_coordinates_v1(after) == before_stability
            and _request_replay_coordinates_v1(before.request) == expected_request
        )
    except Exception:
        return False


@dataclass(frozen=True, init=False)
class SourceBoundEvaluatorReceiptV1:
    claim: protocol.EvaluatorProvenanceClaimV1
    evidence: ContentResolvedEvaluatorReplayV1

    def __new__(cls, *args: object, **kwargs: object) -> "SourceBoundEvaluatorReceiptV1":
        if kwargs.get("_token") is not _RECEIPT_TOKEN:
            raise TypeError("SourceBoundEvaluatorReceiptV1 is controller-sealed")
        return object.__new__(cls)

    def __init__(
        self,
        claim: protocol.EvaluatorProvenanceClaimV1,
        evidence: ContentResolvedEvaluatorReplayV1,
        *,
        _token: object,
    ) -> None:
        if (
            _token is not _RECEIPT_TOKEN
            or type(evidence) is not ContentResolvedEvaluatorReplayV1
            or evidence._identity
            != _identity(
                _EVIDENCE_ID_LABEL_V1,
                (
                    evidence.source_identity,
                    evidence.build_identity,
                    evidence.run_identity,
                ),
            )
        ):
            raise TypeError("SourceBoundEvaluatorReceiptV1 is controller-sealed")
        if (
            claim.provenance_policy_identity
            != source_bound_policy_identity_v2(
                evidence.build.docker_capability,
                evidence.request.host_trust,
            )
            or claim.run_claim_identity != evidence.run_claim.identity
            or claim.replay_evidence_identity != evidence.identity
        ):
            raise TypeError("provenance claim does not bind replay evidence")
        object.__setattr__(self, "claim", claim)
        object.__setattr__(self, "evidence", evidence)

    @property
    def comparator(self) -> pipeline.DiagnosticArbComparatorV1:
        return self.evidence.build.comparator

    @property
    def transcript(self) -> protocol.DecisionTranscriptV1:
        return self.evidence.transcript

    @property
    def run_claim(self) -> protocol.RunClaimV1:
        return self.evidence.run_claim

    @property
    def executable(self) -> bytes:
        return self.evidence.executable

    @property
    def identity(self) -> bytes:
        return self.claim.identity


class SourceBoundFailureReasonV1(StrEnum):
    WRONG_REQUEST = "wrong_request"
    CONTROLLER_CONSUMED = "controller_consumed"
    CONTROLLER_PROCESS_CHANGED = "controller_process_changed"
    SOURCE_REPLAY_FAILED = "source_replay_failed"
    OBSERVER_PLACEMENT_FAILED = "observer_placement_failed"
    REPLAY_BINDING_FAILED = "replay_binding_failed"


@dataclass(frozen=True)
class SourceBoundRejectedV1:
    reason: SourceBoundFailureReasonV1
    detail: str

    def __post_init__(self) -> None:
        if type(self.reason) is not SourceBoundFailureReasonV1:
            raise TypeError("invalid source-bound failure reason")
        if type(self.detail) is not str or not self.detail or len(self.detail) > 4096:
            raise TypeError("invalid source-bound failure detail")


SourceBoundResultV1: TypeAlias = (
    SourceBoundEvaluatorReceiptV1
    | SourceBoundRejectedV1
    | pipeline.PipelineBlockedV1
    | build_transport.BuildRejectedV1
    | build_transport.TwoBuildObservationV1
    | pipeline.ExecutionRejectedV1
    | pipeline.TranscriptRejectedV1
)

def _enter_observer_cgroup_v1(parent: Path) -> None:
    """Move this dedicated one-shot controller into the declared observer group."""

    if not isinstance(parent, Path) or not parent.is_absolute():
        raise TypeError("cgroup parent must be an absolute Path")
    directory_fd = os.open(
        os.fsencode(parent / "observer"),
        os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW,
    )
    try:
        metadata = os.fstat(directory_fd)
        if not stat.S_ISDIR(metadata.st_mode):
            raise OSError("observer cgroup is not a directory")
        procs_fd = os.open(
            b"cgroup.procs",
            os.O_WRONLY | os.O_CLOEXEC | os.O_NOFOLLOW,
            dir_fd=directory_fd,
        )
        try:
            payload = str(os.getpid()).encode("ascii")
            if os.write(procs_fd, payload) != len(payload):
                raise OSError("short cgroup placement write")
        finally:
            os.close(procs_fd)
    finally:
        os.close(directory_fd)


class SourceBoundArbControllerV1:
    """One-shot authority that owns native BUILD, RUN, replay and sealing."""

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

    def _consume_v1(self) -> SourceBoundRejectedV1 | None:
        if os.getpid() != self._owner_pid:
            return SourceBoundRejectedV1(
                SourceBoundFailureReasonV1.CONTROLLER_PROCESS_CHANGED,
                "controller authority cannot cross a process boundary",
            )
        with self._lock:
            if self._consumed:
                return SourceBoundRejectedV1(
                    SourceBoundFailureReasonV1.CONTROLLER_CONSUMED,
                    "controller authority is one-shot",
                )
            self._consumed = True
        return None

    def execute(self, request: pipeline.PipelineRequestV1) -> SourceBoundResultV1:
        consumed = self._consume_v1()
        if consumed is not None:
            return consumed
        if (
            type(self) is not SourceBoundArbControllerV1
            or type(request) is not pipeline.PipelineRequestV1
        ):
            return SourceBoundRejectedV1(
                SourceBoundFailureReasonV1.WRONG_REQUEST,
                "exact SourceBoundArbControllerV1 and PipelineRequestV1 are required",
            )
        try:
            operation = pipeline._snapshot_pipeline_operation_v1(request)
            replay_request = operation.request
        except Exception:
            return SourceBoundRejectedV1(
                SourceBoundFailureReasonV1.SOURCE_REPLAY_FAILED,
                "exact source, build input, or job replay failed",
            )

        build_backend = _NATIVE_BUILD_BACKEND_TYPE(
            self._docker_path,
            pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,
        )
        if type(build_backend) is not _NATIVE_BUILD_BACKEND_TYPE:
            return SourceBoundRejectedV1(
                SourceBoundFailureReasonV1.REPLAY_BINDING_FAILED,
                "native build backend authority changed",
            )
        built = pipeline.ControlledPipelineV1(
            build_backend=build_backend
        )._build_snapshot_v1(
            operation
        )
        if type(built) is not pipeline.DiagnosticBuildObservationV1:
            return built
        try:
            _enter_observer_cgroup_v1(self._cgroup_parent)
        except Exception:
            return SourceBoundRejectedV1(
                SourceBoundFailureReasonV1.OBSERVER_PLACEMENT_FAILED,
                "dedicated controller could not enter the observer cgroup",
            )
        run_backend = _NATIVE_RUN_BACKEND_TYPE(self._cgroup_parent)
        if type(run_backend) is not _NATIVE_RUN_BACKEND_TYPE:
            return SourceBoundRejectedV1(
                SourceBoundFailureReasonV1.REPLAY_BINDING_FAILED,
                "native run backend authority changed",
            )
        controller = executor.ControlledExecutorV1(run_backend)
        capability = controller.probe()
        if type(capability) is executor.UnsupportedV1:
            return pipeline.ExecutionRejectedV1(
                pipeline.ExecutionFailureReasonV1.UNSUPPORTED,
                capability,
            )
        if type(capability) is not executor.SupportedV1:
            return pipeline.ExecutionRejectedV1(
                pipeline.ExecutionFailureReasonV1.BACKEND_CONTRACT,
                capability,
            )
        try:
            invocation = executor.ExecutionRequestV1(
                executable=built.binary,
                argv=(
                    b"arb-evaluator",
                    b"--manifest-identity",
                    built.comparator.identity.hex().encode("ascii"),
                    b"--job",
                    b"/dev/stdin",
                ),
                environment=((b"LC_ALL", b"C"), (b"TZ", b"UTC")),
                cwd=b"/",
                stdin=replay_request.job.encode(),
                umask=0o077,
                limits=replay_request.execution_limits,
            )
        except executor.ExecutionRequestErrorV1 as error:
            return pipeline.ExecutionRejectedV1(
                pipeline.ExecutionFailureReasonV1.BACKEND_CONTRACT,
                error,
            )
        observed = controller.execute(invocation, capability)
        if type(observed) is not executor.CompletedV1:
            if not executor.result_matches_request_v1(observed, invocation):
                return pipeline.ExecutionRejectedV1(
                    pipeline.ExecutionFailureReasonV1.BACKEND_CONTRACT,
                    observed,
                )
            return pipeline.ExecutionRejectedV1(
                pipeline.ExecutionFailureReasonV1.PROCESS_FAILED,
                observed,
            )
        if observed.binary_sha256 != built.binary_sha256:
            return pipeline.ExecutionRejectedV1(
                pipeline.ExecutionFailureReasonV1.BINARY_MISMATCH,
                observed,
            )
        if not executor.result_matches_request_v1(observed, invocation):
            return pipeline.ExecutionRejectedV1(
                pipeline.ExecutionFailureReasonV1.BACKEND_CONTRACT,
                observed,
            )
        if observed.stderr:
            return pipeline.ExecutionRejectedV1(
                pipeline.ExecutionFailureReasonV1.STDERR_NOT_EMPTY,
                observed,
            )
        try:
            transcript = protocol.DecisionTranscriptV1.parse(observed.stdout)
        except protocol.ProtocolErrorV1 as error:
            return pipeline.TranscriptRejectedV1(
                pipeline.TranscriptFailureReasonV1.INVALID_WIRE,
                str(error),
            )
        try:
            invocation_identity = executor.invocation_identity_v1(invocation)
            if type(invocation_identity) is executor.ExecutionIdentityRejectedV1:
                return pipeline.ExecutionRejectedV1(
                    pipeline.ExecutionFailureReasonV1.BACKEND_CONTRACT,
                    invocation_identity,
                )
            platform_identity = executor.platform_identity_v1(capability)
            if type(platform_identity) is executor.ExecutionIdentityRejectedV1:
                return pipeline.ExecutionRejectedV1(
                    pipeline.ExecutionFailureReasonV1.BACKEND_CONTRACT,
                    platform_identity,
                )
            if (
                type(invocation_identity) is not bytes
                or type(platform_identity) is not bytes
            ):
                return pipeline.ExecutionRejectedV1(
                    pipeline.ExecutionFailureReasonV1.BACKEND_CONTRACT,
                    (invocation_identity, platform_identity),
                )
            run_claim = protocol.RunClaimV1.for_transcript(
                replay_request.job,
                built.comparator.manifest,
                transcript,
                built.binary_sha256,
                invocation_identity,
                platform_identity,
            )
            evidence = ContentResolvedEvaluatorReplayV1(
                replay_request,
                built,
                invocation,
                capability,
                observed,
                transcript,
                run_claim,
                _operation=operation,
                _token=_EVIDENCE_TOKEN,
            )
            claim = protocol.EvaluatorProvenanceClaimV1(
                source_bound_policy_identity_v2(
                    built.docker_capability,
                    replay_request.host_trust,
                ),
                run_claim.identity,
                evidence.identity,
            )
            return SourceBoundEvaluatorReceiptV1(
                claim,
                evidence,
                _token=_RECEIPT_TOKEN,
            )
        except protocol.ProtocolErrorV1 as error:
            return pipeline.TranscriptRejectedV1(
                pipeline.TranscriptFailureReasonV1.FOREIGN_BINDING,
                str(error),
            )
        except Exception:
            return SourceBoundRejectedV1(
                SourceBoundFailureReasonV1.REPLAY_BINDING_FAILED,
                "source/build/run replay DAG did not seal",
            )
