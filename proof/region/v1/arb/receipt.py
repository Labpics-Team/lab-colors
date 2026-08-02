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
_SOURCE_BOUND_POLICY_ID_LABEL_V2 = (
    b"labcolors.proof-region.arb-source-bound-policy.v2\0"
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


def _source_identity_v1(request: pipeline.PipelineRequestV1) -> bytes:
    if type(request) is not pipeline.PipelineRequestV1:
        raise TypeError("source replay requires PipelineRequestV1")
    chunks: list[bytes] = [
        request.source_lock.encode(),
        request.source_lock.identity,
        request.admitted_sources.identity,
    ]
    replayed_sources = provenance.admit_arb_sources(
        request.source_lock,
        request.admitted_sources.sources,
    )
    for lock, source in zip(
        request.source_lock.sources,
        request.admitted_sources.sources,
        strict=True,
    ):
        chunks.extend(
            provenance.source_archive_replay_coordinates_v1(lock, source)
        )
    if replayed_sources.identity != request.admitted_sources.identity:
        raise TypeError("source closure did not replay")
    chunks.extend(
        (
            request.build_sources.identity,
            request.build_sources.build_input_identity,
            request.build_sources.formula_support_identity,
            pipeline.build_source_manifest_bytes_v1(request.build_sources),
        )
    )
    return _identity(_SOURCE_ID_LABEL_V1, tuple(chunks))


def _comparator_replays_v1(
    request: pipeline.PipelineRequestV1,
    build: pipeline.DiagnosticBuildObservationV1,
) -> bool:
    try:
        comparator = build.comparator
        capability_identity = build_transport.docker_capability_identity_v1(
            build.docker_capability
        )
        expected_pipeline_policy = pipeline.pipeline_policy_identity_v2(
            request.host_trust,
            build.docker_capability.policy,
        )
        expected_build_preimage = pipeline.comparator_build_preimage_v2(
            request.build_sources,
            capability_identity,
            expected_pipeline_policy,
            build.build_processes,
            build.binary_sha256,
            build.rebuild_sha256s,
            len(build.binary),
        )
        # Re-derive every named comparator coordinate from the retained
        # request and BUILD/RUN observation. Re-hashing a supplied comparator
        # only proves internal consistency; it cannot prove that its wrapper,
        # evaluator, or test-observation coordinates describe this DAG.
        expected_comparator = pipeline.derive_arb_comparator_for_build_v1(
            request,
            build.docker_capability,
            build.binary,
            build.rebuild_sha256s,
            build.build_processes,
        )
        if (
            type(comparator) is not pipeline.DiagnosticArbComparatorV1
            or comparator.structural_source_identity
            != request.admitted_sources.identity
            or comparator.build_input_identity
            != request.build_sources.build_input_identity
            or comparator.pipeline_policy_identity != build.pipeline_policy_identity
            or comparator.pipeline_policy_identity != expected_pipeline_policy
            or comparator.preimages.build_identity != expected_build_preimage
            or comparator.preimages != expected_comparator.preimages
            or comparator.manifest.manifest != expected_comparator.manifest.manifest
            or comparator.manifest.identity != expected_comparator.manifest.identity
            or comparator.identity != expected_comparator.identity
            or comparator.binary_sha256 != build.binary_sha256
            or comparator.rebuild_sha256s != build.rebuild_sha256s
        ):
            return False
        names = tuple(item.name for item in fields(comparator.preimages))
        manifest_names = tuple(
            item.name
            for item in fields(comparator.manifest.manifest)
            if item.name != "kind"
        )
        if names != manifest_names:
            return False
        coordinates = tuple(
            hashlib.sha256(getattr(comparator.preimages, name)).digest()
            for name in names
        )
        fresh_manifest = protocol.ComparatorManifestV2(
            comparator.manifest.manifest.kind,
            *coordinates,
        )
        by_digest = {
            coordinate: getattr(comparator.preimages, name)
            for name, coordinate in zip(names, coordinates, strict=True)
        }
        replayed = protocol.ContentResolvedComparatorManifestV2.admit(
            fresh_manifest,
            by_digest.get,
        )
        return (
            comparator.manifest.manifest == fresh_manifest
            and replayed.manifest == fresh_manifest
            and replayed.identity == fresh_manifest.identity
            and comparator.manifest.identity == fresh_manifest.identity
            and comparator.identity == fresh_manifest.identity
        )
    except Exception:
        return False


def _build_identity_v2(
    request: pipeline.PipelineRequestV1,
    source_identity: bytes,
    build: pipeline.DiagnosticBuildObservationV1,
) -> bytes:
    if type(build) is not pipeline.DiagnosticBuildObservationV1:
        raise TypeError("build replay requires DiagnosticBuildObservationV1")
    bundle = build.input_bundle
    processes = build.build_processes
    binaries = build.rebuild_binaries
    capability_identity = build_transport.docker_capability_identity_v1(
        build.docker_capability
    )
    flint_partition = pipeline.flint_source_content_partition_v1(
        request.source_lock,
        request.admitted_sources,
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
        or not pipeline.arb_input_is_bound_v1(
            request,
            build.docker_capability.policy,
            bundle,
        )
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
        or not _comparator_replays_v1(request, build)
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
            build.host_trust.value.encode("ascii"),
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
        _token: object,
    ) -> None:
        if _token is not _EVIDENCE_TOKEN:
            raise TypeError("ContentResolvedEvaluatorReplayV1 is controller-derived")
        source_identity = _source_identity_v1(request)
        build_identity = _build_identity_v2(request, source_identity, build)
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


def replay_evidence_is_well_bound_v1(value: object) -> bool:
    try:
        if type(value) is not ContentResolvedEvaluatorReplayV1:
            return False
        source_identity = _source_identity_v1(value.request)
        build_identity = _build_identity_v2(
            value.request,
            source_identity,
            value.build,
        )
        run_identity = _run_identity_v1(
            value.request,
            value.build,
            build_identity,
            value.invocation,
            value.platform,
            value.process,
            value.transcript,
            value.run_claim,
        )
        return (
            value.source_identity == source_identity
            and value.build_identity == build_identity
            and value.run_identity == run_identity
            and value._identity
            == _identity(
                _EVIDENCE_ID_LABEL_V1,
                (source_identity, build_identity, run_identity),
            )
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


def _limits_copy_v1(value: executor.ExecutionLimitsV1) -> executor.ExecutionLimitsV1:
    return executor.ExecutionLimitsV1(*value)


def _resolve_request_v1(
    request: pipeline.PipelineRequestV1,
) -> pipeline.PipelineRequestV1:
    lock = provenance.ArbSourceLockV1.parse(request.source_lock.encode())
    if lock.identity != request.source_lock.identity:
        raise TypeError("source lock did not replay")
    return pipeline.PipelineRequestV1(
        lock,
        request.admitted_sources,
        pipeline.admit_build_sources_v1(request.build_sources.files),
        protocol.ProofJobV1.parse(request.job.encode()),
        _limits_copy_v1(request.execution_limits),
        request.host_trust,
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
            replay_request = _resolve_request_v1(request)
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
        built = pipeline.ControlledPipelineV1(build_backend=build_backend).build(
            replay_request
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
