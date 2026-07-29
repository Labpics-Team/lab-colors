#!/usr/bin/env python3
"""Controlled offline BUILD/RUN observations for the Arb evaluator.

The Docker daemon and its persistent Linux host are explicitly inside this
V1 trust boundary.  This module neither claims a fresh VM nor emits SLSA or
source-bound receipts. It observes two fresh-container builds, owns the exact
post-exit output bytes, and can feed that same bytes object to an explicitly
diagnostic, unsealed RUN observation.
"""

from __future__ import annotations

import hashlib
import json
import os
import platform
import selectors
import signal
import stat
import subprocess
import tempfile
import time
from dataclasses import dataclass, fields
from enum import StrEnum
from functools import cached_property
from pathlib import Path, PurePosixPath
from typing import NoReturn, Protocol, TypeAlias

import executor
import provenance
import region_proof_protocol as protocol
import snapshot


OCI_IMAGE_MANIFEST_SHA256_V1 = (
    "c74b2d34b775e6a1b14b13b1d41dc7233f62a18f7a6a4e139e0cf59eeab2e070"
)
OCI_IMAGE_REFERENCE_V1 = f"gcc@sha256:{OCI_IMAGE_MANIFEST_SHA256_V1}"
OCI_PLATFORM_V1 = "linux/amd64"
EVALUATOR_OUTPUT_NAME_V1 = "arb-evaluator-v1"
GENERATED_FORMULA_PATH_V1 = "generated/formula.generated.c"
FORMULA_SPEC_PATH_V1 = "crates/labcolors-core/contracts/contextual-region-formula-v1.lcir"
FORMULA_GENERATOR_PATH_V1 = "proof/region/v1/arb/evaluator/formula.py"
BUILD_RECIPE_PATH_V1 = "proof/region/v1/arb/build.sh"

FORMULA_SPEC_SHA256_V1 = "a6f77ac462f226453b1c27bbd8637b62780b9a640c317a6f50028dacd1de8540"
GENERATED_FORMULA_SHA256_V1 = "9958f20c8ca598625db0593a45f8f8bc79e4b2f22b53263b6c32d78a5e1d2693"

# This is a drift gate, not documentation copied from memory.  Admission below
# hashes every exact input and rejects a local source edit until this manifest
# is deliberately updated together with its causal tests.
_PINNED_BUILD_SOURCE_SHA256_V1 = {
    FORMULA_SPEC_PATH_V1: FORMULA_SPEC_SHA256_V1,
    GENERATED_FORMULA_PATH_V1: GENERATED_FORMULA_SHA256_V1,
    BUILD_RECIPE_PATH_V1: "cf25dc9f3754bb34c74fb0bf44ffe1eae3552dc83ed05936b65e2f48f491342d",
    FORMULA_GENERATOR_PATH_V1: "16629cc3a2ef745ae244ae4762f8946a6546972886f96beeb9ee4920b043040c",
    "proof/region/v1/arb/evaluator/formula.h": "b118f31b0f11ceb04b8239e0762385ac47aeb06b7be0f3b5e29e8e7fcadf20c7",
    "proof/region/v1/arb/evaluator/hash.c": "c28e6281208f09ca15fa74aea0091f27726ed68efc3480c34a7db33b8ca3567e",
    "proof/region/v1/arb/evaluator/hash.h": "a62c07f2eca9294b4c1c802e2a9e6cff6ad9f8fd696a74b54a21489d56fab6c4",
    "proof/region/v1/arb/evaluator/interval.c": "93f206258b83fc0f373ae865787ebf266c9d011f2578567ed913a7cb6c0ed899",
    "proof/region/v1/arb/evaluator/interval.h": "f9d7416059d4b09979c22e6823a747f252c576558c750fe3e2ff92509894c7b3",
    "proof/region/v1/arb/evaluator/main.c": "e9a3fa6b70b3a25eb6d6cf7eaba9a98d2fbe5cb7fdd3c1790219efb7fe20918d",
    "proof/region/v1/arb/evaluator/region.c": "c665de00b3226912112c2d75bf85ce078ef1461bfebdb7e42453b639343c566f",
    "proof/region/v1/arb/evaluator/region.h": "95da5117bb162c707b441242637d5e0e1bbeef2532ac1f10248f2b93ab16dcc8",
    "proof/region/v1/arb/evaluator/wire.c": "5f5eb984f953cc3b49cf5b3b31ee44efe70a89f74f736e9a2cf1cbc865ed58b7",
    "proof/region/v1/arb/evaluator/wire.h": "bdf2ce9be9fce95a38c61e923b45038efb7bfab78842e38296114f0e83266c98",
}

REQUIRED_BUILD_SOURCE_MODES_V1 = tuple(
    (path, 0o755 if path == BUILD_RECIPE_PATH_V1 else 0o644)
    for path in sorted(_PINNED_BUILD_SOURCE_SHA256_V1)
)

BUILD_STDOUT_LIMIT_V1 = 16 * 1024 * 1024
BUILD_STDERR_LIMIT_V1 = 16 * 1024 * 1024
BUILD_TIMEOUT_NS_V1 = 2 * 60 * 60 * 1_000_000_000
DOCKER_PROBE_OUTPUT_LIMIT_V1 = 1024 * 1024
DOCKER_PROBE_TIMEOUT_NS_V1 = 30 * 1_000_000_000
MAX_BUILD_SOURCE_FILE_BYTES_V1 = 16 * 1024 * 1024
MAX_BUILD_SOURCE_TOTAL_BYTES_V1 = 32 * 1024 * 1024

# FLINT's exact locked qsieve path uses /tmp directly rather than TMPDIR.  A
# container-private tmpfs preserves a read-only root without a host bind,
# volume, or reusable writable-layer scratch.  POSIX sticky-directory mode is
# required because the container runs as the unprivileged host runner identity.
_BUILD_TMPFS_SPEC_V1 = "/tmp:rw,noexec,nosuid,nodev,mode=1777"

_BUILD_SOURCES_ID_LABEL_V1 = b"labcolors.proof-region.arb-build-sources.v1\0"
_BUILD_INPUT_ID_LABEL_V1 = b"labcolors.proof-region.arb-compiler-inputs.v1\0"
_FORMULA_SUPPORT_ID_LABEL_V1 = b"labcolors.proof-region.arb-formula-support.v1\0"
_FLINT_COMMIT_CONTENT_ID_LABEL_V1 = (
    b"labcolors.proof-region.flint-commit-content.v1\0"
)
_FLINT_RELEASE_ONLY_ID_LABEL_V1 = (
    b"labcolors.proof-region.flint-project-pinned-release-only.v1\0"
)
_PIPELINE_POLICY_ID_LABEL_V1 = b"labcolors.proof-region.arb-pipeline-policy.v1\0"
_INVOCATION_ID_LABEL_V1 = b"labcolors.proof-region.arb-invocation.v1\0"
_PLATFORM_ID_LABEL_V1 = b"labcolors.proof-region.arb-run-platform.v1\0"
_BUILD_SOURCES_TOKEN = object()
_COMPARATOR_TOKEN = object()
_BUILD_OBSERVATION_TOKEN = object()
_PIPELINE_OBSERVATION_TOKEN = object()


def _blob(value: bytes) -> bytes:
    return len(value).to_bytes(8, "big") + value


def _identity(label: bytes, chunks: tuple[bytes, ...]) -> bytes:
    payload = b"".join(_blob(chunk) for chunk in chunks)
    return hashlib.sha256(label + len(payload).to_bytes(8, "big") + payload).digest()


def _valid_digest(value: object) -> bool:
    return type(value) is bytes and len(value) == 32 and value != bytes(32)


class BuildSourceReasonV1(StrEnum):
    WRONG_TYPE = "wrong_type"
    INVALID_PATH = "invalid_path"
    INVALID_MODE = "invalid_mode"
    INVALID_CONTENT = "invalid_content"
    NONCANONICAL_SET = "noncanonical_set"
    CONTENT_DRIFT = "content_drift"


@dataclass(frozen=True)
class BuildSourceAdmissionErrorV1(ValueError):
    reason: BuildSourceReasonV1
    path: str

    def __str__(self) -> str:
        return f"{self.reason.value}: {self.path}"


def _source_fail(reason: BuildSourceReasonV1, path: str) -> NoReturn:
    raise BuildSourceAdmissionErrorV1(reason, path)


def _logical_path(value: object) -> str:
    if type(value) is not str or not value or value.startswith("/") or "\\" in value:
        _source_fail(BuildSourceReasonV1.INVALID_PATH, str(value))
    try:
        encoded = value.encode("ascii")
    except UnicodeEncodeError:
        _source_fail(BuildSourceReasonV1.INVALID_PATH, value)
    if (
        len(encoded) > 4096
        or any(byte < 0x20 or byte == 0x7F for byte in encoded)
        or any(part in ("", ".", "..") for part in value.split("/"))
    ):
        _source_fail(BuildSourceReasonV1.INVALID_PATH, value)
    return value


@dataclass(frozen=True)
class BuildSourceFileV1:
    path: str
    mode: int
    contents: bytes

    def __post_init__(self) -> None:
        _logical_path(self.path)
        if type(self.mode) is not int or self.mode not in (0o644, 0o755):
            _source_fail(BuildSourceReasonV1.INVALID_MODE, self.path)
        if (
            type(self.contents) is not bytes
            or not self.contents
            or len(self.contents) > MAX_BUILD_SOURCE_FILE_BYTES_V1
        ):
            _source_fail(BuildSourceReasonV1.INVALID_CONTENT, self.path)


@dataclass(frozen=True, init=False)
class AdmittedBuildSourcesV1:
    """Owned exact local build-support closure.

    ``build_input_identity`` covers the recipe, generated C and evaluator files
    named by that recipe. ``formula_support_identity`` separately covers the
    formula spec, generator and generated C. The latter is support/replay
    material; it does not claim that build.sh executed the generator.
    """

    files: tuple[BuildSourceFileV1, ...]
    identity: bytes

    def __init__(
        self,
        files_value: tuple[BuildSourceFileV1, ...],
        identity: bytes,
        *,
        _token: object,
    ) -> None:
        if _token is not _BUILD_SOURCES_TOKEN:
            raise TypeError("AdmittedBuildSourcesV1 is created only by source admission")
        if type(files_value) is not tuple or any(
            type(item) is not BuildSourceFileV1 for item in files_value
        ):
            raise TypeError("invalid build source files")
        if not _valid_digest(identity):
            raise TypeError("invalid build source identity")
        object.__setattr__(self, "files", files_value)
        object.__setattr__(self, "identity", identity)

    def contents(self, path: str) -> bytes:
        for item in self.files:
            if item.path == path:
                return item.contents
        raise KeyError(path)

    @property
    def formula_spec(self) -> bytes:
        return self.contents(FORMULA_SPEC_PATH_V1)

    @property
    def generated_formula(self) -> bytes:
        return self.contents(GENERATED_FORMULA_PATH_V1)

    @cached_property
    def build_input_identity(self) -> bytes:
        direct = tuple(
            item
            for item in self.files
            if item.path not in (FORMULA_SPEC_PATH_V1, FORMULA_GENERATOR_PATH_V1)
        )
        return _source_subset_identity(_BUILD_INPUT_ID_LABEL_V1, direct)

    @cached_property
    def formula_support_identity(self) -> bytes:
        support_paths = frozenset(
            (
                FORMULA_SPEC_PATH_V1,
                FORMULA_GENERATOR_PATH_V1,
                GENERATED_FORMULA_PATH_V1,
            )
        )
        support = tuple(item for item in self.files if item.path in support_paths)
        return _source_subset_identity(_FORMULA_SUPPORT_ID_LABEL_V1, support)


def _source_subset_identity(
    label: bytes,
    files_value: tuple[BuildSourceFileV1, ...],
) -> bytes:
    chunks: list[bytes] = [len(files_value).to_bytes(4, "big")]
    for item in files_value:
        chunks.extend(
            (
                item.path.encode("ascii"),
                item.mode.to_bytes(4, "big"),
                hashlib.sha256(item.contents).digest(),
                len(item.contents).to_bytes(8, "big"),
            )
        )
    return _identity(label, tuple(chunks))


def _build_sources_identity(files_value: tuple[BuildSourceFileV1, ...]) -> bytes:
    return _source_subset_identity(_BUILD_SOURCES_ID_LABEL_V1, files_value)


def admit_build_sources_v1(
    files_value: tuple[BuildSourceFileV1, ...],
) -> AdmittedBuildSourcesV1:
    if type(files_value) is not tuple or any(
        type(item) is not BuildSourceFileV1 for item in files_value
    ):
        _source_fail(BuildSourceReasonV1.WRONG_TYPE, "files")
    actual = tuple((item.path, item.mode) for item in files_value)
    if actual != REQUIRED_BUILD_SOURCE_MODES_V1:
        _source_fail(BuildSourceReasonV1.NONCANONICAL_SET, "files")
    if sum(len(item.contents) for item in files_value) > MAX_BUILD_SOURCE_TOTAL_BYTES_V1:
        _source_fail(BuildSourceReasonV1.INVALID_CONTENT, "files")
    for item in files_value:
        if hashlib.sha256(item.contents).hexdigest() != _PINNED_BUILD_SOURCE_SHA256_V1[item.path]:
            _source_fail(BuildSourceReasonV1.CONTENT_DRIFT, item.path)
    return AdmittedBuildSourcesV1(
        files_value,
        _build_sources_identity(files_value),
        _token=_BUILD_SOURCES_TOKEN,
    )


class HostTrustBoundaryV1(StrEnum):
    PERSISTENT_SELF_HOSTED_DOCKER = "persistent-self-hosted-linux-docker-host"


def pipeline_policy_identity_v1(
    host_trust: HostTrustBoundaryV1,
) -> bytes:
    if type(host_trust) is not HostTrustBoundaryV1:
        raise TypeError("host_trust must be HostTrustBoundaryV1")
    return _identity(
        _PIPELINE_POLICY_ID_LABEL_V1,
        (
            OCI_IMAGE_REFERENCE_V1.encode("ascii"),
            OCI_PLATFORM_V1.encode("ascii"),
            host_trust.value.encode("ascii"),
            b"build-observation=diagnostic-unsealed-v1",
            b"run-observation=diagnostic-unsealed-v1",
            b"network=none",
            b"rootfs=readonly",
            b"scratch-tmpfs=" + _BUILD_TMPFS_SPEC_V1.encode("ascii"),
            b"cap-drop=all",
            b"no-new-privileges=true",
            b"inputs=readonly-bind",
            b"workspace=readonly-bind",
            f"source-snapshot-mtime-ns={snapshot.SOURCE_SNAPSHOT_MTIME_NS_V1}".encode(
                "ascii"
            ),
            b"fresh-container-count=2",
        ),
    )


class PipelineInputReasonV1(StrEnum):
    WRONG_TYPE = "wrong_type"
    FOREIGN_SOURCE_CAPABILITY = "foreign_source_capability"
    FORMULA_MISMATCH = "formula_mismatch"
    EXECUTION_LIMIT_MISMATCH = "execution_limit_mismatch"


@dataclass(frozen=True)
class PipelineInputErrorV1(ValueError):
    reason: PipelineInputReasonV1
    field: str

    def __str__(self) -> str:
        return f"{self.reason.value}: {self.field}"


@dataclass(frozen=True)
class FlintSourceContentPartitionV1:
    """Structural FLINT archive partition, not an origin assertion.

    ``commit_content`` names the side which the project lock expects a future
    authority to relate to the exact Git tree.  ``project_pinned_release_only``
    names the separate release bytes consumed by the build.  Neither identity
    claims that those release-only bytes were generated from the commit.
    """

    commit_content_identity: bytes
    commit_content_file_count: int
    project_pinned_release_only_identity: bytes
    project_pinned_release_only_file_count: int

    def __post_init__(self) -> None:
        if not _valid_digest(self.commit_content_identity):
            raise TypeError("invalid FLINT commit-content identity")
        if not _valid_digest(self.project_pinned_release_only_identity):
            raise TypeError("invalid FLINT project-pinned release-only identity")
        if (
            type(self.commit_content_file_count) is not int
            or self.commit_content_file_count <= 0
            or type(self.project_pinned_release_only_file_count) is not int
            or self.project_pinned_release_only_file_count <= 0
        ):
            raise TypeError("FLINT source partition must be nonempty on both sides")


def _archive_file_subset_identity(
    label: bytes,
    files_value: tuple[provenance.ArchiveFileV1, ...],
) -> bytes:
    chunks: list[bytes] = [len(files_value).to_bytes(8, "big")]
    for item in files_value:
        chunks.extend(
            (
                item.path.encode("ascii"),
                item.mode.to_bytes(4, "big"),
                item.length.to_bytes(8, "big"),
                item.sha256,
            )
        )
    return _identity(label, tuple(chunks))


def _require_bound_source_capability_v1(
    source_lock: provenance.ArbSourceLockV1,
    admitted_sources: provenance.AdmittedArbSourcesV1,
) -> None:
    if source_lock.identity != admitted_sources.source_lock_identity:
        raise PipelineInputErrorV1(
            PipelineInputReasonV1.FOREIGN_SOURCE_CAPABILITY,
            "admitted_sources",
        )
    for lock, admitted in zip(
        source_lock.sources,
        admitted_sources.sources,
        strict=True,
    ):
        if lock.identity != admitted.source_lock_identity:
            raise PipelineInputErrorV1(
                PipelineInputReasonV1.FOREIGN_SOURCE_CAPABILITY,
                "admitted_sources",
            )


def flint_source_content_partition_v1(
    source_lock: provenance.ArbSourceLockV1,
    admitted_sources: provenance.AdmittedArbSourcesV1,
) -> FlintSourceContentPartitionV1:
    if type(source_lock) is not provenance.ArbSourceLockV1:
        raise PipelineInputErrorV1(PipelineInputReasonV1.WRONG_TYPE, "source_lock")
    if type(admitted_sources) is not provenance.AdmittedArbSourcesV1:
        raise PipelineInputErrorV1(
            PipelineInputReasonV1.WRONG_TYPE,
            "admitted_sources",
        )
    _require_bound_source_capability_v1(source_lock, admitted_sources)

    flint_lock = source_lock.sources[2]
    flint_source = admitted_sources.sources[2]
    if type(flint_lock.integrity) is not provenance.GitContentRelationPolicyV1:
        raise PipelineInputErrorV1(PipelineInputReasonV1.WRONG_TYPE, "source_lock")
    release_only_by_path = {
        item.path: item
        for item in flint_lock.integrity.project_pinned_release_only_files
    }
    release_only = tuple(
        item for item in flint_source.files if item.path in release_only_by_path
    )
    commit_content = tuple(
        item for item in flint_source.files if item.path not in release_only_by_path
    )
    if (
        len(commit_content) != flint_lock.integrity.common_file_count
        or len(release_only) != len(release_only_by_path)
        or any(
            item.mode != release_only_by_path[item.path].mode
            or item.length != release_only_by_path[item.path].length
            or item.sha256 != release_only_by_path[item.path].sha256
            for item in release_only
        )
        or len(commit_content) + len(release_only) != len(flint_source.files)
    ):
        raise PipelineInputErrorV1(
            PipelineInputReasonV1.FOREIGN_SOURCE_CAPABILITY,
            "flint_source_partition",
        )
    return FlintSourceContentPartitionV1(
        _archive_file_subset_identity(
            _FLINT_COMMIT_CONTENT_ID_LABEL_V1,
            commit_content,
        ),
        len(commit_content),
        _archive_file_subset_identity(
            _FLINT_RELEASE_ONLY_ID_LABEL_V1,
            release_only,
        ),
        len(release_only),
    )


def _comparator_preimage_v1(label: bytes, chunks: tuple[bytes, ...]) -> bytes:
    """Encode one independently versioned, ordered comparator preimage."""

    if (
        type(label) is not bytes
        or not label.startswith(b"labcolors.proof-region.arb-comparator.")
        or not label.endswith(b".v1\0")
        or type(chunks) is not tuple
        or not chunks
        or any(type(chunk) is not bytes for chunk in chunks)
    ):
        raise TypeError("invalid comparator preimage coordinates")
    return label + b"\x01" + len(chunks).to_bytes(4, "big") + b"".join(
        _blob(chunk) for chunk in chunks
    )


def _encoded_build_file_set_v1(
    label: bytes,
    files_value: tuple[BuildSourceFileV1, ...],
) -> bytes:
    chunks: list[bytes] = [len(files_value).to_bytes(4, "big")]
    for item in files_value:
        chunks.extend(
            (
                item.path.encode("ascii"),
                item.mode.to_bytes(4, "big"),
                len(item.contents).to_bytes(8, "big"),
                item.contents,
            )
        )
    return _comparator_preimage_v1(label, tuple(chunks))


def _operation_allowlist_preimage_v1(formula_spec: bytes) -> bytes:
    """Bind the exact ordered SSA operator contract from the admitted formula."""

    if type(formula_spec) is not bytes or not formula_spec:
        raise TypeError("formula_spec must be nonempty bytes")
    lines = formula_spec.splitlines()
    declarations: tuple[bytes, ...] | None = None
    for index, line in enumerate(lines):
        if not line.startswith(b"operators "):
            continue
        pieces = line.split(b" ")
        if len(pieces) != 2 or not pieces[1].isdigit():
            raise ValueError("invalid formula operator count")
        count = int(pieces[1])
        candidate = tuple(lines[index + 1 : index + 1 + count])
        if (
            count <= 0
            or len(candidate) != count
            or any(not item.startswith(b"operator ") for item in candidate)
            or (
                index + 1 + count < len(lines)
                and lines[index + 1 + count].startswith(b"operator ")
            )
        ):
            raise ValueError("formula operator declarations do not match their count")
        declarations = candidate
        break
    if declarations is None:
        raise ValueError("formula has no operator contract")
    return _comparator_preimage_v1(
        b"labcolors.proof-region.arb-comparator.operation-allowlist.v1\0",
        (
            b"exact-real-ssa-operator-declarations",
            len(declarations).to_bytes(4, "big"),
            *declarations,
        ),
    )


@dataclass(frozen=True)
class ArbComparatorPreimagesV1:
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
            raise TypeError("comparator preimages must be nonempty exact bytes")
        if len(set(values)) != len(values):
            raise TypeError("comparator preimages must be independently domain-separated")


@dataclass(frozen=True, init=False)
class DiagnosticArbComparatorV1:
    """Manifest declaration derived from admitted inputs and diagnostic BUILD."""

    preimages: ArbComparatorPreimagesV1
    manifest: protocol.ContentResolvedComparatorManifestV1
    structural_source_identity: bytes
    build_input_identity: bytes
    pipeline_policy_identity: bytes
    binary_sha256: bytes
    rebuild_sha256s: tuple[bytes, bytes]

    def __new__(cls, *args: object, **kwargs: object) -> "DiagnosticArbComparatorV1":
        if kwargs.get("_token") is not _COMPARATOR_TOKEN:
            raise TypeError("DiagnosticArbComparatorV1 is controller-derived")
        return object.__new__(cls)

    def __init__(
        self,
        preimages: ArbComparatorPreimagesV1,
        manifest: protocol.ContentResolvedComparatorManifestV1,
        structural_source_identity: bytes,
        build_input_identity: bytes,
        pipeline_policy_identity: bytes,
        binary_sha256: bytes,
        rebuild_sha256s: tuple[bytes, bytes],
        *,
        _token: object,
    ) -> None:
        if _token is not _COMPARATOR_TOKEN:
            raise TypeError("DiagnosticArbComparatorV1 is controller-derived")
        if type(preimages) is not ArbComparatorPreimagesV1:
            raise TypeError("invalid comparator preimages")
        if (
            type(manifest) is not protocol.ContentResolvedComparatorManifestV1
            or manifest.manifest.kind is not protocol.ComparatorKindV1.ARB
        ):
            raise TypeError("invalid Arb comparator manifest")
        manifest_names = tuple(
            item.name for item in fields(manifest.manifest) if item.name != "kind"
        )
        preimage_names = tuple(item.name for item in fields(preimages))
        if manifest_names != preimage_names:
            raise TypeError("comparator manifest/preimage schema drift")
        by_digest = {
            hashlib.sha256(getattr(preimages, name)).digest(): getattr(preimages, name)
            for name in preimage_names
        }
        replayed = protocol.ContentResolvedComparatorManifestV1.admit(
            manifest.manifest,
            by_digest.get,
        )
        if replayed.identity != manifest.identity:
            raise TypeError("comparator manifest replay drift")
        for name, value in (
            ("structural_source_identity", structural_source_identity),
            ("build_input_identity", build_input_identity),
            ("pipeline_policy_identity", pipeline_policy_identity),
            ("binary_sha256", binary_sha256),
        ):
            if not _valid_digest(value):
                raise TypeError(f"invalid {name}")
        if (
            type(rebuild_sha256s) is not tuple
            or rebuild_sha256s != (binary_sha256, binary_sha256)
        ):
            raise TypeError("invalid comparator rebuild binding")
        for name, value in locals().items():
            if name in self.__dataclass_fields__:
                object.__setattr__(self, name, value)

    @property
    def identity(self) -> bytes:
        return self.manifest.identity


@dataclass(frozen=True)
class PipelineRequestV1:
    source_lock: provenance.ArbSourceLockV1
    admitted_sources: provenance.AdmittedArbSourcesV1
    build_sources: AdmittedBuildSourcesV1
    job: protocol.ProofJobV1
    execution_limits: executor.ExecutionLimitsV1
    host_trust: HostTrustBoundaryV1

    def __post_init__(self) -> None:
        expected_types = (
            ("source_lock", self.source_lock, provenance.ArbSourceLockV1),
            ("admitted_sources", self.admitted_sources, provenance.AdmittedArbSourcesV1),
            ("build_sources", self.build_sources, AdmittedBuildSourcesV1),
            ("job", self.job, protocol.ProofJobV1),
            ("execution_limits", self.execution_limits, executor.ExecutionLimitsV1),
            ("host_trust", self.host_trust, HostTrustBoundaryV1),
        )
        for field_name, value, expected_type in expected_types:
            if type(value) is not expected_type:
                raise PipelineInputErrorV1(PipelineInputReasonV1.WRONG_TYPE, field_name)
        _require_bound_source_capability_v1(self.source_lock, self.admitted_sources)
        flint_source_content_partition_v1(self.source_lock, self.admitted_sources)
        if self.build_sources.formula_spec != self.job.formula_spec:
            raise PipelineInputErrorV1(PipelineInputReasonV1.FORMULA_MISMATCH, "job")
        job_bytes = self.job.encode()
        invocation_bytes = sum(
            len(value) + 1
            for value in (
                b"arb-evaluator",
                b"--manifest-identity",
                bytes(32).hex().encode("ascii"),
                b"--job",
                b"/dev/stdin",
            )
        ) + sum(
            len(key) + len(value) + 2
            for key, value in ((b"LC_ALL", b"C"), (b"TZ", b"UTC"))
        )
        if (
            len(job_bytes) > self.execution_limits.max_stdin_bytes
            or invocation_bytes > self.execution_limits.max_argument_bytes
        ):
            raise PipelineInputErrorV1(
                PipelineInputReasonV1.EXECUTION_LIMIT_MISMATCH,
                "execution_limits",
            )


class DockerBlockerReasonV1(StrEnum):
    HOST_NOT_LINUX_AMD64 = "host_not_linux_amd64"
    DOCKER_UNAVAILABLE = "docker_unavailable"
    IMAGE_UNAVAILABLE = "image_unavailable"
    IMAGE_IDENTITY_MISMATCH = "image_identity_mismatch"
    ISOLATION_UNAVAILABLE = "isolation_unavailable"
    SAME_OBJECT_OUTPUT_UNAVAILABLE = "same_object_output_unavailable"
    BACKEND_CONTRACT = "backend_contract"


@dataclass(frozen=True)
class DockerUnsupportedV1:
    reason: DockerBlockerReasonV1
    detail: str

    def __post_init__(self) -> None:
        if type(self.reason) is not DockerBlockerReasonV1:
            raise TypeError("invalid Docker blocker reason")
        if type(self.detail) is not str or not self.detail or len(self.detail) > 4096:
            raise TypeError("invalid Docker blocker detail")


@dataclass(frozen=True)
class DockerSupportedV1:
    image_reference: str
    platform: str
    daemon_observation_sha256: bytes

    def __post_init__(self) -> None:
        if self.image_reference != OCI_IMAGE_REFERENCE_V1:
            raise TypeError("wrong OCI image reference")
        if self.platform != OCI_PLATFORM_V1:
            raise TypeError("wrong OCI platform")
        if not _valid_digest(self.daemon_observation_sha256):
            raise TypeError("invalid Docker daemon observation digest")


DockerCapabilityReportV1: TypeAlias = DockerSupportedV1 | DockerUnsupportedV1


def _absolute_path(value: object, field_name: str) -> Path:
    if not isinstance(value, Path) or not value.is_absolute():
        raise TypeError(f"{field_name} must be an absolute Path")
    if any(character in str(value) for character in (",", "\n", "\r", "\0")):
        raise TypeError(f"{field_name} is not Docker-mount-safe")
    return value


_CONTAINER_NAME_PREFIX_V1 = "labcolors-arb-build-v1-"


def _container_name(value: object) -> str:
    if (
        type(value) is not str
        or not value.startswith(_CONTAINER_NAME_PREFIX_V1)
        or len(value) > 128
        or any(character not in "abcdefghijklmnopqrstuvwxyz0123456789-" for character in value)
    ):
        raise TypeError("invalid controller-owned Docker container name")
    return value


@dataclass(frozen=True)
class DockerBuildRequestV1:
    attempt: int
    root_directory: Path
    inputs_directory: Path
    workspace_directory: Path
    build_directory: Path
    output_directory: Path
    cid_file: Path
    container_name: str

    def __post_init__(self) -> None:
        if type(self.attempt) is not int or self.attempt not in (1, 2):
            raise TypeError("attempt must be 1 or 2")
        paths = tuple(
            _absolute_path(getattr(self, field_name), field_name)
            for field_name in (
                "root_directory",
                "inputs_directory",
                "workspace_directory",
                "build_directory",
                "output_directory",
                "cid_file",
            )
        )
        _container_name(self.container_name)
        root = self.root_directory
        if len(set(paths)) != len(paths):
            raise TypeError("build paths must be distinct")
        for path in paths[1:]:
            try:
                path.relative_to(root)
            except ValueError:
                raise TypeError("build path escapes controller root") from None


def _bounded_bytes(value: object, maximum: int, field_name: str) -> bytes:
    if type(value) is not bytes or len(value) > maximum:
        raise TypeError(f"invalid {field_name}")
    return value


@dataclass(frozen=True)
class DockerBuildExitedV1:
    returncode: int
    stdout: bytes
    stderr: bytes

    def __post_init__(self) -> None:
        if type(self.returncode) is not int:
            raise TypeError("invalid Docker returncode")
        _bounded_bytes(self.stdout, BUILD_STDOUT_LIMIT_V1, "stdout")
        _bounded_bytes(self.stderr, BUILD_STDERR_LIMIT_V1, "stderr")


@dataclass(frozen=True)
class DockerBuildTimedOutV1:
    stdout: bytes
    stderr: bytes

    def __post_init__(self) -> None:
        _bounded_bytes(self.stdout, BUILD_STDOUT_LIMIT_V1, "stdout")
        _bounded_bytes(self.stderr, BUILD_STDERR_LIMIT_V1, "stderr")


class DockerOutputStreamV1(StrEnum):
    STDOUT = "stdout"
    STDERR = "stderr"


@dataclass(frozen=True)
class DockerBuildOutputLimitV1:
    stream: DockerOutputStreamV1
    stdout: bytes
    stderr: bytes

    def __post_init__(self) -> None:
        if type(self.stream) is not DockerOutputStreamV1:
            raise TypeError("invalid Docker output stream")
        _bounded_bytes(self.stdout, BUILD_STDOUT_LIMIT_V1, "stdout")
        _bounded_bytes(self.stderr, BUILD_STDERR_LIMIT_V1, "stderr")


@dataclass(frozen=True)
class DockerBuildObserverFailureV1:
    detail: str

    def __post_init__(self) -> None:
        if type(self.detail) is not str or not self.detail or len(self.detail) > 4096:
            raise TypeError("invalid Docker observer failure")


class DockerCleanupTriggerV1(StrEnum):
    PROCESS_EXIT = "process_exit"
    TIMEOUT = "timeout"
    OUTPUT_LIMIT = "output_limit"
    OBSERVER_FAILURE = "observer_failure"


@dataclass(frozen=True)
class DockerBuildCleanupFailureV1:
    trigger: DockerCleanupTriggerV1
    detail: str
    stdout: bytes
    stderr: bytes

    def __post_init__(self) -> None:
        if type(self.trigger) is not DockerCleanupTriggerV1:
            raise TypeError("invalid Docker cleanup trigger")
        if type(self.detail) is not str or not self.detail or len(self.detail) > 4096:
            raise TypeError("invalid Docker cleanup failure")
        _bounded_bytes(self.stdout, BUILD_STDOUT_LIMIT_V1, "stdout")
        _bounded_bytes(self.stderr, BUILD_STDERR_LIMIT_V1, "stderr")


DockerBuildProcessObservationV1: TypeAlias = (
    DockerBuildExitedV1
    | DockerBuildTimedOutV1
    | DockerBuildOutputLimitV1
    | DockerBuildObserverFailureV1
    | DockerBuildCleanupFailureV1
)


class DockerBuildBackendV1(Protocol):
    def probe(self) -> DockerCapabilityReportV1: ...

    def run_build(
        self,
        request: DockerBuildRequestV1,
    ) -> DockerBuildProcessObservationV1: ...


def _archive_file_manifest_bytes_v1(
    files_value: tuple[provenance.ArchiveFileV1, ...],
) -> bytes:
    chunks: list[bytes] = [len(files_value).to_bytes(8, "big")]
    for item in files_value:
        chunks.extend(
            (
                item.path.encode("ascii"),
                item.mode.to_bytes(4, "big"),
                item.length.to_bytes(8, "big"),
                item.sha256,
            )
        )
    return b"".join(_blob(chunk) for chunk in chunks)


def _source_snapshot_chunks_v1(
    lock: provenance.SourceReleaseLockV1,
    source: provenance.SafeSourceArchiveV1,
) -> tuple[bytes, ...]:
    return (
        bytes((int(lock.role),)),
        lock.encode(),
        source.source_lock_identity,
        source.archive_sha256,
        source.tree_identity,
        source.regular_file_count.to_bytes(8, "big"),
        source.regular_file_bytes.to_bytes(8, "big"),
        _archive_file_manifest_bytes_v1(source.files),
        len(source.archive_bytes).to_bytes(8, "big"),
        hashlib.sha256(source.archive_bytes).digest(),
    )


def _build_process_bytes_v1(process: DockerBuildExitedV1) -> bytes:
    if type(process) is not DockerBuildExitedV1:
        raise TypeError("only successful typed build observations are encodable")
    return b"".join(
        (
            process.returncode.to_bytes(4, "big", signed=True),
            len(process.stdout).to_bytes(8, "big"),
            hashlib.sha256(process.stdout).digest(),
            len(process.stderr).to_bytes(8, "big"),
            hashlib.sha256(process.stderr).digest(),
        )
    )


def _derive_arb_comparator_for_build_v1(
    request: PipelineRequestV1,
    docker_report: DockerSupportedV1,
    binary: bytes,
    rebuild_sha256s: tuple[bytes, bytes],
    build_processes: tuple[DockerBuildExitedV1, DockerBuildExitedV1],
) -> DiagnosticArbComparatorV1:
    """Derive all ten coordinates without accepting a caller digest/resolver."""

    if type(request) is not PipelineRequestV1:
        raise TypeError("request must be PipelineRequestV1")
    if type(docker_report) is not DockerSupportedV1:
        raise TypeError("docker_report must be DockerSupportedV1")
    if type(binary) is not bytes or not binary:
        raise TypeError("binary must be exact nonempty bytes")
    binary_sha256 = hashlib.sha256(binary).digest()
    if (
        type(build_processes) is not tuple
        or len(build_processes) != 2
        or any(type(item) is not DockerBuildExitedV1 for item in build_processes)
        or any(item.returncode != 0 for item in build_processes)
        or rebuild_sha256s != (binary_sha256, binary_sha256)
    ):
        raise TypeError("comparator derivation requires two equal successful builds")
    pipeline_policy_identity = pipeline_policy_identity_v1(request.host_trust)
    flint_lock = request.source_lock.sources[2]
    flint_source = request.admitted_sources.sources[2]
    if type(flint_lock.integrity) is not provenance.GitContentRelationPolicyV1:
        raise TypeError("FLINT requires the exact content-relation policy")

    exclusions = _comparator_preimage_v1(
        b"labcolors.proof-region.arb-comparator.exclusions.v1\0",
        (
            b"gap:trusted-persistent-docker-host-and-daemon",
            b"gap:unsealed-diagnostic-build-observer",
            b"gap:unsealed-diagnostic-run-observer",
            b"gap:libc-libm-libpthread-libgcc-and-build-utility-source",
            b"gap:no-per-test-result-records",
            b"gap:no-git-derivation-for-project-pinned-release-only-files",
            b"gap:no-origin-authority-reverification",
            request.host_trust.value.encode("ascii"),
            b"build-observation=diagnostic-unsealed-v1",
            b"run-observation=diagnostic-unsealed-v1",
            len(flint_lock.integrity.omitted_paths).to_bytes(4, "big"),
            *(
                path.encode("ascii")
                for path in flint_lock.integrity.omitted_paths
            ),
            len(
                flint_lock.integrity.project_pinned_release_only_files
            ).to_bytes(4, "big"),
            *(
                item.encode()
                for item in flint_lock.integrity.project_pinned_release_only_files
            ),
        ),
    )

    upstream_chunks: list[bytes] = [
        request.source_lock.encode(),
        request.admitted_sources.source_lock_identity,
        len(request.source_lock.sources).to_bytes(4, "big"),
    ]
    for lock, source in zip(
        request.source_lock.sources,
        request.admitted_sources.sources,
        strict=True,
    ):
        upstream_chunks.extend(_source_snapshot_chunks_v1(lock, source))
    upstream_source = _comparator_preimage_v1(
        b"labcolors.proof-region.arb-comparator.upstream-source.v1\0",
        tuple(upstream_chunks),
    )

    operation_allowlist = _operation_allowlist_preimage_v1(
        request.build_sources.formula_spec
    )
    arithmetic_chunks: list[bytes] = [
        b"exact admitted GMP MPFR FLINT source snapshots and pinned static-build boundary",
        len(request.source_lock.sources).to_bytes(4, "big"),
    ]
    for lock, source in zip(
        request.source_lock.sources,
        request.admitted_sources.sources,
        strict=True,
    ):
        arithmetic_chunks.extend(
            (
                bytes((int(lock.role),)),
                lock.identity,
                source.archive_sha256,
                source.tree_identity,
            )
        )
    arithmetic_chunks.extend(
        (
            OCI_IMAGE_REFERENCE_V1.encode("ascii"),
            OCI_PLATFORM_V1.encode("ascii"),
            hashlib.sha256(operation_allowlist).digest(),
            hashlib.sha256(exclusions).digest(),
        )
    )
    arithmetic_input_set = _comparator_preimage_v1(
        b"labcolors.proof-region.arb-comparator.arithmetic-input-set.v1\0",
        tuple(arithmetic_chunks),
    )

    wrapper_paths = frozenset(
        (
            "proof/region/v1/arb/evaluator/formula.h",
            "proof/region/v1/arb/evaluator/interval.c",
            "proof/region/v1/arb/evaluator/interval.h",
        )
    )
    wrapper_files = tuple(
        item for item in request.build_sources.files if item.path in wrapper_paths
    )
    evaluator_files = tuple(
        item
        for item in request.build_sources.files
        if item.path not in (
            FORMULA_SPEC_PATH_V1,
            FORMULA_GENERATOR_PATH_V1,
            BUILD_RECIPE_PATH_V1,
        )
        and item.path not in wrapper_paths
    )
    wrapper_source = _encoded_build_file_set_v1(
        b"labcolors.proof-region.arb-comparator.wrapper-source.v1\0",
        wrapper_files,
    )
    evaluator_source = _encoded_build_file_set_v1(
        b"labcolors.proof-region.arb-comparator.evaluator-source.v1\0",
        evaluator_files,
    )

    process_bytes = tuple(_build_process_bytes_v1(item) for item in build_processes)
    build_identity = _comparator_preimage_v1(
        b"labcolors.proof-region.arb-comparator.build-identity.v1\0",
        (
            request.build_sources.contents(BUILD_RECIPE_PATH_V1),
            request.build_sources.build_input_identity,
            request.build_sources.formula_support_identity,
            OCI_IMAGE_REFERENCE_V1.encode("ascii"),
            OCI_PLATFORM_V1.encode("ascii"),
            docker_report.daemon_observation_sha256,
            pipeline_policy_identity,
            b"build-observation=diagnostic-unsealed-v1",
            len(build_processes).to_bytes(4, "big"),
            *process_bytes,
            binary_sha256,
            rebuild_sha256s[0],
            rebuild_sha256s[1],
            len(binary).to_bytes(8, "big"),
            binary_sha256,
        ),
    )
    test_observation = _comparator_preimage_v1(
        b"labcolors.proof-region.arb-comparator.test-observation.v1\0",
        (
            b"kind:aggregate-outer-process-observation-no-per-test-records",
            request.build_sources.contents(BUILD_RECIPE_PATH_V1),
            len(build_processes).to_bytes(4, "big"),
            *process_bytes,
        ),
    )

    legal_chunks: list[bytes] = [
        b"ordered admitted legal-file set; no legal-compliance claim",
        len(request.source_lock.sources).to_bytes(4, "big"),
    ]
    for lock, source in zip(
        request.source_lock.sources,
        request.admitted_sources.sources,
        strict=True,
    ):
        actual_by_path = {item.path: item for item in source.files}
        legal_chunks.extend(
            (
                bytes((int(lock.role),)),
                lock.identity,
                source.archive_sha256,
                source.tree_identity,
                len(lock.legal_files).to_bytes(4, "big"),
            )
        )
        for declaration in lock.legal_files:
            actual = actual_by_path.get(declaration.path)
            if (
                actual is None
                or actual.length != declaration.length
                or actual.sha256 != declaration.sha256
            ):
                raise TypeError("admitted legal-file set drift")
            legal_chunks.extend(
                (
                    declaration.encode(),
                    actual.path.encode("ascii"),
                    actual.mode.to_bytes(4, "big"),
                    actual.length.to_bytes(8, "big"),
                    actual.sha256,
                )
            )
    legal_file_set = _comparator_preimage_v1(
        b"labcolors.proof-region.arb-comparator.legal-file-set.v1\0",
        tuple(legal_chunks),
    )

    engine_release = _comparator_preimage_v1(
        b"labcolors.proof-region.arb-comparator.engine-release.v1\0",
        (
            b"FLINT release lock declaration",
            flint_lock.encode(),
            flint_source.source_lock_identity,
        ),
    )
    preimages = ArbComparatorPreimagesV1(
        engine_release,
        upstream_source,
        arithmetic_input_set,
        wrapper_source,
        evaluator_source,
        build_identity,
        operation_allowlist,
        test_observation,
        legal_file_set,
        exclusions,
    )
    coordinates = tuple(
        hashlib.sha256(getattr(preimages, item.name)).digest()
        for item in fields(preimages)
    )
    manifest_value = protocol.ComparatorManifestV1(
        protocol.ComparatorKindV1.ARB,
        *coordinates,
    )
    by_digest = {
        coordinate: getattr(preimages, item.name)
        for coordinate, item in zip(coordinates, fields(preimages), strict=True)
    }
    resolved = protocol.ContentResolvedComparatorManifestV1.admit(
        manifest_value,
        by_digest.get,
    )
    return DiagnosticArbComparatorV1(
        preimages,
        resolved,
        request.admitted_sources.identity,
        request.build_sources.build_input_identity,
        pipeline_policy_identity,
        binary_sha256,
        rebuild_sha256s,
        _token=_COMPARATOR_TOKEN,
    )


class NativeDockerBuildBackendV1:
    """Docker adapter for one explicitly trusted persistent Linux host."""

    def __init__(
        self,
        docker_path: Path,
        *,
        platform_name: str | None = None,
        machine_name: str | None = None,
        monotonic_ns: object = time.monotonic_ns,
    ) -> None:
        if not isinstance(docker_path, Path) or not docker_path.is_absolute():
            raise TypeError("docker_path must be an absolute Path")
        self._docker_path = docker_path
        self._platform_name = sys_platform = (
            platform.system().lower() if platform_name is None else platform_name
        )
        self._platform_name = "linux" if sys_platform == "linux" else sys_platform
        self._machine_name = platform.machine() if machine_name is None else machine_name
        self._monotonic_ns = monotonic_ns

    @staticmethod
    def _environment() -> dict[str, str]:
        return {
            "HOME": "/nonexistent",
            "PATH": "/usr/bin:/bin",
            "DOCKER_CONFIG": "/nonexistent",
        }

    def probe(self) -> DockerCapabilityReportV1:
        if self._platform_name != "linux" or self._machine_name.lower() not in (
            "x86_64",
            "amd64",
        ):
            return DockerUnsupportedV1(
                DockerBlockerReasonV1.HOST_NOT_LINUX_AMD64,
                "controlled build requires a Linux amd64 Docker host",
            )
        try:
            metadata = self._docker_path.lstat()
        except OSError:
            return DockerUnsupportedV1(
                DockerBlockerReasonV1.DOCKER_UNAVAILABLE,
                "exact Docker CLI path is unavailable",
            )
        if not stat.S_ISREG(metadata.st_mode) or stat.S_ISLNK(metadata.st_mode):
            return DockerUnsupportedV1(
                DockerBlockerReasonV1.DOCKER_UNAVAILABLE,
                "Docker CLI must be one regular non-symlink path",
            )
        commands = (
            (
                str(self._docker_path),
                "version",
                "--format",
                "{{json .Server}}",
            ),
            (
                str(self._docker_path),
                "image",
                "inspect",
                OCI_IMAGE_REFERENCE_V1,
            ),
        )
        outputs: list[bytes] = []
        for index, command in enumerate(commands):
            result = self._observe_command(
                command,
                stdout_limit=DOCKER_PROBE_OUTPUT_LIMIT_V1,
                stderr_limit=DOCKER_PROBE_OUTPUT_LIMIT_V1,
                timeout_ns=DOCKER_PROBE_TIMEOUT_NS_V1,
                cid_file=None,
            )
            if (
                type(result) is not DockerBuildExitedV1
                or result.returncode != 0
                or not result.stdout
                or result.stderr
            ):
                return DockerUnsupportedV1(
                    DockerBlockerReasonV1.DOCKER_UNAVAILABLE
                    if index == 0
                    else DockerBlockerReasonV1.IMAGE_UNAVAILABLE,
                    "Docker daemon probe failed"
                    if index == 0
                    else "pinned image is not locally inspectable",
                )
            outputs.append(result.stdout)
        try:
            inspected = json.loads(outputs[1])
            if type(inspected) is not list or len(inspected) != 1:
                raise ValueError("wrong image inspection cardinality")
            image = inspected[0]
            if type(image) is not dict:
                raise ValueError("wrong image inspection shape")
            repo_digests = image.get("RepoDigests")
            if (
                image.get("Os") != "linux"
                or image.get("Architecture") not in ("amd64", "x86_64")
                or type(repo_digests) is not list
                or OCI_IMAGE_REFERENCE_V1 not in repo_digests
            ):
                raise ValueError("foreign image coordinate")
        except (ValueError, TypeError, json.JSONDecodeError):
            return DockerUnsupportedV1(
                DockerBlockerReasonV1.IMAGE_IDENTITY_MISMATCH,
                "local image does not match pinned linux/amd64 manifest",
            )
        daemon_digest = _identity(
            b"labcolors.proof-region.docker-daemon-observation.v1\0",
            tuple(outputs),
        )
        return DockerSupportedV1(
            OCI_IMAGE_REFERENCE_V1,
            OCI_PLATFORM_V1,
            daemon_digest,
        )

    def command_for(self, request: DockerBuildRequestV1) -> tuple[str, ...]:
        if type(request) is not DockerBuildRequestV1:
            raise TypeError("request must be DockerBuildRequestV1")
        mounts = (
            f"type=bind,src={request.inputs_directory},dst=/inputs,readonly,bind-propagation=private",
            f"type=bind,src={request.workspace_directory},dst=/workspace,readonly,bind-propagation=private",
            f"type=bind,src={request.build_directory},dst=/build,bind-propagation=private",
            f"type=bind,src={request.output_directory},dst=/out,bind-propagation=private",
        )
        command = [
            str(self._docker_path),
            "run",
            "--rm",
            "--pull",
            "never",
            "--platform",
            OCI_PLATFORM_V1,
            "--network",
            "none",
            "--read-only",
            "--tmpfs",
            _BUILD_TMPFS_SPEC_V1,
            "--cap-drop",
            "ALL",
            "--security-opt",
            "no-new-privileges:true",
            "--name",
            request.container_name,
            "--hostname",
            "labcolors-arb-build-v1",
            "--user",
            f"{os.getuid()}:{os.getgid()}",
            "--workdir",
            "/",
            "--cidfile",
            str(request.cid_file),
        ]
        for mount in mounts:
            command.extend(("--mount", mount))
        command.extend(
            (
                "--entrypoint",
                "/bin/sh",
                OCI_IMAGE_REFERENCE_V1,
                f"/workspace/{BUILD_RECIPE_PATH_V1}",
            )
        )
        return tuple(command)

    def run_build(
        self,
        request: DockerBuildRequestV1,
    ) -> DockerBuildProcessObservationV1:
        if type(request) is not DockerBuildRequestV1:
            raise TypeError("request must be DockerBuildRequestV1")
        return self._observe_command(
            self.command_for(request),
            stdout_limit=BUILD_STDOUT_LIMIT_V1,
            stderr_limit=BUILD_STDERR_LIMIT_V1,
            timeout_ns=BUILD_TIMEOUT_NS_V1,
            cid_file=request.cid_file,
            container_name=request.container_name,
        )

    def _observe_command(
        self,
        command: tuple[str, ...],
        *,
        stdout_limit: int,
        stderr_limit: int,
        timeout_ns: int,
        cid_file: Path | None,
        container_name: str | None = None,
    ) -> DockerBuildProcessObservationV1:
        if (
            type(command) is not tuple
            or not command
            or any(type(item) is not str or not item or "\0" in item for item in command)
        ):
            raise TypeError("command must be a nonempty string tuple")
        if (
            type(stdout_limit) is not int
            or stdout_limit <= 0
            or stdout_limit > BUILD_STDOUT_LIMIT_V1
            or type(stderr_limit) is not int
            or stderr_limit <= 0
            or stderr_limit > BUILD_STDERR_LIMIT_V1
            or type(timeout_ns) is not int
            or timeout_ns <= 0
            or timeout_ns > BUILD_TIMEOUT_NS_V1
        ):
            raise TypeError("invalid Docker observation limits")
        if (cid_file is None) != (container_name is None):
            raise TypeError("Docker cleanup requires both CID file and exact name")
        if cid_file is not None:
            _absolute_path(cid_file, "cid_file")
            _container_name(container_name)
        try:
            process = subprocess.Popen(
                command,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                cwd="/",
                env=self._environment(),
                close_fds=True,
                start_new_session=True,
            )
        except OSError:
            return DockerBuildObserverFailureV1("cannot start Docker CLI")
        if process.stdout is None or process.stderr is None:
            stop_detail = self._stop_process(process)
            cleanup_detail = (
                self._cleanup_container(cid_file, container_name)
                if cid_file is not None and container_name is not None
                else None
            )
            if stop_detail is not None or cleanup_detail is not None:
                return DockerBuildCleanupFailureV1(
                    DockerCleanupTriggerV1.OBSERVER_FAILURE,
                    stop_detail or cleanup_detail or "Docker cleanup failed",
                    b"",
                    b"",
                )
            return DockerBuildObserverFailureV1("Docker pipes unavailable")

        stdout = bytearray()
        stderr = bytearray()
        streams = {
            process.stdout.fileno(): (DockerOutputStreamV1.STDOUT, stdout, stdout_limit),
            process.stderr.fileno(): (DockerOutputStreamV1.STDERR, stderr, stderr_limit),
        }
        selector = selectors.DefaultSelector()
        terminal: DockerOutputStreamV1 | None = None
        timed_out = False
        observer_failed = False
        try:
            for descriptor in streams:
                os.set_blocking(descriptor, False)
                selector.register(descriptor, selectors.EVENT_READ)
            start = self._clock()
            deadline = start + timeout_ns
            while selector.get_map() or process.poll() is None:
                now = self._clock()
                if now >= deadline:
                    timed_out = True
                    break
                timeout = min((deadline - now) / 1_000_000_000, 0.1)
                for key, _events in selector.select(timeout):
                    stream, target, maximum = streams[key.fd]
                    try:
                        chunk = os.read(key.fd, min(64 * 1024, maximum + 1 - len(target)))
                    except BlockingIOError:
                        continue
                    if not chunk:
                        selector.unregister(key.fd)
                        continue
                    target.extend(chunk)
                    if len(target) > maximum:
                        del target[maximum:]
                        terminal = stream
                        break
                if terminal is not None:
                    break
        except Exception:
            observer_failed = True
        finally:
            selector.close()

        stop_detail: str | None = None
        if timed_out or terminal is not None or observer_failed:
            stop_detail = self._stop_process(process)
        else:
            try:
                process.wait(timeout=30)
            except subprocess.TimeoutExpired:
                timed_out = True
                stop_detail = self._stop_process(process)
        process.stdout.close()
        process.stderr.close()
        cleanup_detail = (
            self._cleanup_container(cid_file, container_name)
            if cid_file is not None and container_name is not None
            else None
        )
        if stop_detail is not None or cleanup_detail is not None:
            trigger = DockerCleanupTriggerV1.PROCESS_EXIT
            if observer_failed:
                trigger = DockerCleanupTriggerV1.OBSERVER_FAILURE
            elif terminal is not None:
                trigger = DockerCleanupTriggerV1.OUTPUT_LIMIT
            elif timed_out:
                trigger = DockerCleanupTriggerV1.TIMEOUT
            return DockerBuildCleanupFailureV1(
                trigger,
                stop_detail or cleanup_detail or "Docker cleanup failed",
                bytes(stdout),
                bytes(stderr),
            )
        if observer_failed:
            return DockerBuildObserverFailureV1("Docker output observation failed")
        if terminal is not None:
            return DockerBuildOutputLimitV1(terminal, bytes(stdout), bytes(stderr))
        if timed_out:
            return DockerBuildTimedOutV1(bytes(stdout), bytes(stderr))
        if type(process.returncode) is not int:
            return DockerBuildObserverFailureV1("Docker returncode unavailable")
        return DockerBuildExitedV1(process.returncode, bytes(stdout), bytes(stderr))

    def _clock(self) -> int:
        value = self._monotonic_ns()
        if type(value) is not int or value < 0:
            raise RuntimeError("invalid monotonic clock")
        return value

    def _stop_process(
        self,
        process: subprocess.Popen[bytes],
    ) -> str | None:
        failed = False
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        except OSError:
            try:
                process.kill()
            except ProcessLookupError:
                pass
            except OSError:
                failed = True
        try:
            process.wait(timeout=30)
        except subprocess.TimeoutExpired:
            failed = True
        if process.poll() is None:
            failed = True
        return "Docker CLI process could not be terminated" if failed else None

    @staticmethod
    def _admitted_container_id(cid_file: Path) -> str | None:
        try:
            descriptor = os.open(
                cid_file,
                os.O_RDONLY
                | getattr(os, "O_CLOEXEC", 0)
                | getattr(os, "O_NOFOLLOW", 0),
            )
        except OSError:
            return None
        try:
            metadata = os.fstat(descriptor)
            if (
                not stat.S_ISREG(metadata.st_mode)
                or metadata.st_nlink != 1
                or metadata.st_size not in (64, 65)
            ):
                return None
            raw = os.read(descriptor, 66)
        except OSError:
            return None
        finally:
            os.close(descriptor)
        if len(raw) == 65 and raw.endswith(b"\n"):
            raw = raw[:-1]
        if len(raw) != 64 or any(
            byte not in b"0123456789abcdef" for byte in raw
        ):
            return None
        return raw.decode("ascii")

    def _observe_cleanup_command(
        self,
        command: tuple[str, ...],
    ) -> DockerBuildProcessObservationV1:
        return self._observe_command(
            command,
            stdout_limit=DOCKER_PROBE_OUTPUT_LIMIT_V1,
            stderr_limit=DOCKER_PROBE_OUTPUT_LIMIT_V1,
            timeout_ns=DOCKER_PROBE_TIMEOUT_NS_V1,
            cid_file=None,
        )

    def _cleanup_container(self, cid_file: Path, container_name: str) -> str | None:
        _absolute_path(cid_file, "cid_file")
        _container_name(container_name)
        container_id = self._admitted_container_id(cid_file)
        removal_coordinates = (
            (container_id, container_name)
            if container_id is not None
            else (container_name,)
        )
        try:
            for coordinate in removal_coordinates:
                self._observe_cleanup_command(
                    (
                        str(self._docker_path),
                        "container",
                        "rm",
                        "--force",
                        coordinate,
                    )
                )
            filters = [f"name=^/{container_name}$"]
            if container_id is not None:
                filters.append(f"id={container_id}")
            for filter_value in filters:
                observation = self._observe_cleanup_command(
                    (
                        str(self._docker_path),
                        "container",
                        "ls",
                        "--all",
                        "--quiet",
                        "--no-trunc",
                        "--filter",
                        filter_value,
                    )
                )
                if (
                    type(observation) is not DockerBuildExitedV1
                    or observation.returncode != 0
                    or observation.stdout
                    or observation.stderr
                ):
                    return "Docker container absence could not be verified"
        except Exception:
            return "Docker container cleanup observer raised"
        return None


class BuildFailureReasonV1(StrEnum):
    BACKEND_CONTRACT = "backend_contract"
    PROCESS_FAILED = "process_failed"
    CLEANUP_FAILED = "cleanup_failed"
    INPUT_CHANGED = "input_changed"
    INVALID_OUTPUT = "invalid_output"


@dataclass(frozen=True)
class PipelineBlockedV1:
    reason: DockerBlockerReasonV1
    detail: str


@dataclass(frozen=True)
class BuildRejectedV1:
    attempt: int
    reason: BuildFailureReasonV1
    process: DockerBuildProcessObservationV1 | None = None


@dataclass(frozen=True)
class NonReproducibleBuildV1:
    first_sha256: bytes
    second_sha256: bytes


class ExecutionFailureReasonV1(StrEnum):
    UNSUPPORTED = "unsupported"
    PROCESS_FAILED = "process_failed"
    STDERR_NOT_EMPTY = "stderr_not_empty"
    BINARY_MISMATCH = "binary_mismatch"
    BACKEND_CONTRACT = "backend_contract"


@dataclass(frozen=True)
class ExecutionRejectedV1:
    reason: ExecutionFailureReasonV1
    observation: object


class TranscriptFailureReasonV1(StrEnum):
    INVALID_WIRE = "invalid_wire"
    FOREIGN_BINDING = "foreign_binding"


@dataclass(frozen=True)
class TranscriptRejectedV1:
    reason: TranscriptFailureReasonV1
    detail: str


@dataclass(frozen=True, init=False)
class DiagnosticBuildObservationV1:
    """Controller-owned two-build observation with no native-evidence claim."""

    structural_source_identity: bytes
    flint_commit_content_identity: bytes
    flint_commit_content_file_count: int
    flint_project_pinned_release_only_identity: bytes
    flint_project_pinned_release_only_file_count: int
    build_input_identity: bytes
    formula_support_identity: bytes
    pipeline_policy_identity: bytes
    docker_daemon_observation_sha256: bytes
    oci_image_reference: str
    oci_platform: str
    binary_sha256: bytes
    rebuild_sha256s: tuple[bytes, bytes]
    host_trust: HostTrustBoundaryV1
    build_processes: tuple[DockerBuildExitedV1, DockerBuildExitedV1]
    comparator: DiagnosticArbComparatorV1
    _binary: bytes

    def __init__(
        self,
        structural_source_identity: bytes,
        flint_commit_content_identity: bytes,
        flint_commit_content_file_count: int,
        flint_project_pinned_release_only_identity: bytes,
        flint_project_pinned_release_only_file_count: int,
        build_input_identity: bytes,
        formula_support_identity: bytes,
        pipeline_policy_identity: bytes,
        docker_daemon_observation_sha256: bytes,
        oci_image_reference: str,
        oci_platform: str,
        binary_sha256: bytes,
        rebuild_sha256s: tuple[bytes, bytes],
        host_trust: HostTrustBoundaryV1,
        build_processes: tuple[DockerBuildExitedV1, DockerBuildExitedV1],
        comparator: DiagnosticArbComparatorV1,
        binary: bytes,
        *,
        _token: object,
    ) -> None:
        if _token is not _BUILD_OBSERVATION_TOKEN:
            raise TypeError("DiagnosticBuildObservationV1 is controller-only")
        for name, value in (
            ("structural_source_identity", structural_source_identity),
            ("flint_commit_content_identity", flint_commit_content_identity),
            (
                "flint_project_pinned_release_only_identity",
                flint_project_pinned_release_only_identity,
            ),
            ("build_input_identity", build_input_identity),
            ("formula_support_identity", formula_support_identity),
            ("pipeline_policy_identity", pipeline_policy_identity),
            ("docker_daemon_observation_sha256", docker_daemon_observation_sha256),
            ("binary_sha256", binary_sha256),
        ):
            if not _valid_digest(value):
                raise TypeError(f"invalid {name}")
        if (
            type(flint_commit_content_file_count) is not int
            or flint_commit_content_file_count <= 0
            or type(flint_project_pinned_release_only_file_count) is not int
            or flint_project_pinned_release_only_file_count <= 0
        ):
            raise TypeError("FLINT source partition must be nonempty")
        if oci_image_reference != OCI_IMAGE_REFERENCE_V1 or oci_platform != OCI_PLATFORM_V1:
            raise TypeError("diagnostic build does not bind the pinned OCI manifest/platform")
        if (
            type(rebuild_sha256s) is not tuple
            or len(rebuild_sha256s) != 2
            or any(not _valid_digest(item) for item in rebuild_sha256s)
            or rebuild_sha256s != (binary_sha256, binary_sha256)
        ):
            raise TypeError("invalid reproducible-build digests")
        if type(host_trust) is not HostTrustBoundaryV1:
            raise TypeError("invalid host trust boundary")
        if pipeline_policy_identity != pipeline_policy_identity_v1(host_trust):
            raise TypeError("pipeline policy is not the fixed diagnostic policy")
        if (
            type(build_processes) is not tuple
            or len(build_processes) != 2
            or any(type(item) is not DockerBuildExitedV1 for item in build_processes)
            or any(item.returncode != 0 for item in build_processes)
        ):
            raise TypeError("invalid build process observations")
        if (
            type(comparator) is not DiagnosticArbComparatorV1
            or comparator.structural_source_identity != structural_source_identity
            or comparator.build_input_identity != build_input_identity
            or comparator.pipeline_policy_identity != pipeline_policy_identity
            or comparator.binary_sha256 != binary_sha256
            or comparator.rebuild_sha256s != rebuild_sha256s
        ):
            raise TypeError("comparator does not bind this diagnostic build")
        if type(binary) is not bytes or hashlib.sha256(binary).digest() != binary_sha256:
            raise TypeError("invalid owned binary")
        for name, value in locals().items():
            if name in self.__dataclass_fields__ and not name.startswith("_"):
                object.__setattr__(self, name, value)
        object.__setattr__(self, "_binary", binary)

    @property
    def binary(self) -> bytes:
        return self._binary


@dataclass(frozen=True, init=False)
class DiagnosticPipelineObservationV1:
    """Diagnostic BUILD plus diagnostic RUN; never a receipt or native proof."""

    build_observation: DiagnosticBuildObservationV1
    invocation_identity: bytes
    platform_identity: bytes
    transcript: protocol.DecisionTranscriptV1
    run_claim: protocol.RunClaimV1
    _transcript_bytes: bytes

    def __init__(
        self,
        build_observation: DiagnosticBuildObservationV1,
        invocation_identity: bytes,
        platform_identity: bytes,
        transcript: protocol.DecisionTranscriptV1,
        run_claim: protocol.RunClaimV1,
        transcript_bytes: bytes,
        *,
        _token: object,
    ) -> None:
        if _token is not _PIPELINE_OBSERVATION_TOKEN:
            raise TypeError("DiagnosticPipelineObservationV1 is controller-only")
        if type(build_observation) is not DiagnosticBuildObservationV1:
            raise TypeError("invalid diagnostic build observation")
        if not _valid_digest(invocation_identity) or not _valid_digest(platform_identity):
            raise TypeError("invalid RUN observation identities")
        if type(transcript) is not protocol.DecisionTranscriptV1:
            raise TypeError("invalid transcript")
        if type(run_claim) is not protocol.RunClaimV1:
            raise TypeError("invalid run claim")
        if (
            transcript.comparator_identity != build_observation.comparator.identity
            or run_claim.job_identity != transcript.job_identity
            or run_claim.comparator_identity != build_observation.comparator.identity
            or run_claim.binary_identity != build_observation.binary_sha256
            or run_claim.invocation_identity != invocation_identity
            or run_claim.platform_identity != platform_identity
            or run_claim.transcript_identity != transcript.identity
        ):
            raise TypeError("run claim does not bind diagnostic observations")
        if type(transcript_bytes) is not bytes or transcript.encode() != transcript_bytes:
            raise TypeError("invalid owned transcript")
        object.__setattr__(self, "build_observation", build_observation)
        object.__setattr__(self, "invocation_identity", invocation_identity)
        object.__setattr__(self, "platform_identity", platform_identity)
        object.__setattr__(self, "transcript", transcript)
        object.__setattr__(self, "run_claim", run_claim)
        object.__setattr__(self, "_transcript_bytes", transcript_bytes)

    @property
    def comparator(self) -> DiagnosticArbComparatorV1:
        return self.build_observation.comparator

    @property
    def structural_source_identity(self) -> bytes:
        return self.build_observation.structural_source_identity

    @property
    def flint_commit_content_identity(self) -> bytes:
        return self.build_observation.flint_commit_content_identity

    @property
    def flint_commit_content_file_count(self) -> int:
        return self.build_observation.flint_commit_content_file_count

    @property
    def flint_project_pinned_release_only_identity(self) -> bytes:
        return self.build_observation.flint_project_pinned_release_only_identity

    @property
    def flint_project_pinned_release_only_file_count(self) -> int:
        return self.build_observation.flint_project_pinned_release_only_file_count

    @property
    def build_input_identity(self) -> bytes:
        return self.build_observation.build_input_identity

    @property
    def formula_support_identity(self) -> bytes:
        return self.build_observation.formula_support_identity

    @property
    def pipeline_policy_identity(self) -> bytes:
        return self.build_observation.pipeline_policy_identity

    @property
    def docker_daemon_observation_sha256(self) -> bytes:
        return self.build_observation.docker_daemon_observation_sha256

    @property
    def oci_image_reference(self) -> str:
        return self.build_observation.oci_image_reference

    @property
    def oci_platform(self) -> str:
        return self.build_observation.oci_platform

    @property
    def binary_sha256(self) -> bytes:
        return self.build_observation.binary_sha256

    @property
    def rebuild_sha256s(self) -> tuple[bytes, bytes]:
        return self.build_observation.rebuild_sha256s

    @property
    def host_trust(self) -> HostTrustBoundaryV1:
        return self.build_observation.host_trust

    @property
    def build_processes(self) -> tuple[DockerBuildExitedV1, DockerBuildExitedV1]:
        return self.build_observation.build_processes

    @property
    def binary(self) -> bytes:
        return self.build_observation.binary

    @property
    def transcript_bytes(self) -> bytes:
        return self._transcript_bytes


BuildResultV1: TypeAlias = (
    DiagnosticBuildObservationV1
    | PipelineBlockedV1
    | BuildRejectedV1
    | NonReproducibleBuildV1
)


PipelineResultV1: TypeAlias = (
    DiagnosticPipelineObservationV1
    | PipelineBlockedV1
    | BuildRejectedV1
    | NonReproducibleBuildV1
    | ExecutionRejectedV1
    | TranscriptRejectedV1
)


def invocation_identity_v1(request: executor.ExecutionRequestV1) -> bytes:
    if type(request) is not executor.ExecutionRequestV1:
        raise TypeError("request must be ExecutionRequestV1")
    chunks: list[bytes] = [hashlib.sha256(request.executable).digest()]
    chunks.append(len(request.argv).to_bytes(4, "big"))
    chunks.extend(request.argv)
    chunks.append(len(request.environment).to_bytes(4, "big"))
    for key, value in request.environment:
        chunks.extend((key, value))
    chunks.extend(
        (
            request.cwd,
            hashlib.sha256(request.stdin).digest(),
            len(request.stdin).to_bytes(8, "big"),
            request.umask.to_bytes(4, "big"),
        )
    )
    for item in fields(request.limits):
        chunks.append(getattr(request.limits, item.name).to_bytes(8, "big"))
    return _identity(_INVOCATION_ID_LABEL_V1, tuple(chunks))


def platform_identity_v1(report: executor.SupportedV1) -> bytes:
    if type(report) is not executor.SupportedV1:
        raise TypeError("report must be SupportedV1")
    return _identity(
        _PLATFORM_ID_LABEL_V1,
        (
            report.platform.encode("ascii"),
            report.sandbox_policy_release.encode("ascii"),
        ),
    )


class _TreeMismatchV1(RuntimeError):
    pass


def _write_all(descriptor: int, contents: bytes) -> None:
    cursor = 0
    while cursor < len(contents):
        written = os.write(descriptor, contents[cursor:])
        if written <= 0:
            raise OSError("short write")
        cursor += written


def _write_exact_file(root: Path, item: BuildSourceFileV1) -> None:
    target = root / item.path
    current = root
    for part in PurePosixPath(item.path).parent.parts:
        current = current / part
        try:
            current.mkdir(mode=0o755)
        except FileExistsError:
            metadata = current.lstat()
            if not stat.S_ISDIR(metadata.st_mode) or stat.S_ISLNK(metadata.st_mode):
                raise _TreeMismatchV1("parent collision")
        current.chmod(0o755)
    descriptor = os.open(
        target,
        os.O_WRONLY
        | os.O_CREAT
        | os.O_EXCL
        | getattr(os, "O_CLOEXEC", 0)
        | getattr(os, "O_NOFOLLOW", 0),
        item.mode,
    )
    try:
        _write_all(descriptor, item.contents)
        os.fchmod(descriptor, item.mode)
    finally:
        os.close(descriptor)


def _expected_directories(paths: set[str]) -> set[str]:
    result = {"."}
    for path in paths:
        parent = PurePosixPath(path).parent
        while str(parent) != ".":
            result.add(str(parent))
            parent = parent.parent
    return result


def _verify_exact_tree(
    root: Path,
    expected: dict[str, tuple[int, int, bytes]],
) -> None:
    actual_files: set[str] = set()
    actual_directories: set[str] = {"."}
    for directory, directory_names, file_names in os.walk(root, followlinks=False):
        base = Path(directory)
        relative_base = base.relative_to(root)
        metadata = base.lstat()
        if not stat.S_ISDIR(metadata.st_mode) or stat.S_ISLNK(metadata.st_mode):
            raise _TreeMismatchV1("non-directory in tree")
        if stat.S_IMODE(metadata.st_mode) != 0o755:
            raise _TreeMismatchV1("directory mode drift")
        for name in directory_names:
            target = base / name
            target_metadata = target.lstat()
            if not stat.S_ISDIR(target_metadata.st_mode) or stat.S_ISLNK(target_metadata.st_mode):
                raise _TreeMismatchV1("link or non-directory parent")
            relative = (relative_base / name).as_posix()
            actual_directories.add(relative)
        for name in file_names:
            target = base / name
            relative = (relative_base / name).as_posix()
            coordinate = expected.get(relative)
            if coordinate is None:
                raise _TreeMismatchV1("extra file")
            metadata = target.lstat()
            mode, length, digest = coordinate
            if (
                not stat.S_ISREG(metadata.st_mode)
                or stat.S_ISLNK(metadata.st_mode)
                or metadata.st_nlink != 1
                or stat.S_IMODE(metadata.st_mode) != mode
                or metadata.st_size != length
            ):
                raise _TreeMismatchV1("file metadata drift")
            hasher = hashlib.sha256()
            with target.open("rb") as stream:
                while chunk := stream.read(64 * 1024):
                    hasher.update(chunk)
            if hasher.digest() != digest:
                raise _TreeMismatchV1("file content drift")
            actual_files.add(relative)
    if actual_files != set(expected) or actual_directories != _expected_directories(set(expected)):
        raise _TreeMismatchV1("tree shape drift")


def _read_build_output(directory: Path, maximum: int) -> bytes:
    try:
        names = tuple(item.name for item in directory.iterdir())
    except OSError as error:
        raise _TreeMismatchV1("cannot list build output") from error
    if names != (EVALUATOR_OUTPUT_NAME_V1,):
        raise _TreeMismatchV1("build output must contain exactly one file")
    directory_fd = os.open(
        directory,
        os.O_RDONLY | os.O_DIRECTORY | getattr(os, "O_CLOEXEC", 0),
    )
    try:
        descriptor = os.open(
            EVALUATOR_OUTPUT_NAME_V1,
            os.O_RDONLY
            | getattr(os, "O_CLOEXEC", 0)
            | getattr(os, "O_NOFOLLOW", 0),
            dir_fd=directory_fd,
        )
    except OSError as error:
        os.close(directory_fd)
        raise _TreeMismatchV1("cannot open exact build output") from error
    try:
        before = os.fstat(descriptor)
        if (
            not stat.S_ISREG(before.st_mode)
            or before.st_nlink != 1
            or stat.S_IMODE(before.st_mode) != 0o555
            or before.st_size <= 0
            or before.st_size > maximum
        ):
            raise _TreeMismatchV1("invalid build output metadata")
        chunks: list[bytes] = []
        length = 0
        while True:
            chunk = os.read(descriptor, min(64 * 1024, maximum + 1 - length))
            if not chunk:
                break
            chunks.append(chunk)
            length += len(chunk)
            if length > maximum:
                raise _TreeMismatchV1("oversized build output")
        after = os.fstat(descriptor)
        coordinates_before = (
            before.st_dev,
            before.st_ino,
            before.st_size,
            before.st_mtime_ns,
            before.st_ctime_ns,
        )
        coordinates_after = (
            after.st_dev,
            after.st_ino,
            after.st_size,
            after.st_mtime_ns,
            after.st_ctime_ns,
        )
        if coordinates_before != coordinates_after or length != before.st_size:
            raise _TreeMismatchV1("build output changed during observation")
        return b"".join(chunks)
    except OSError as error:
        raise _TreeMismatchV1("cannot read exact build output") from error
    finally:
        os.close(descriptor)
        os.close(directory_fd)


class ControlledPipelineV1:
    def __init__(
        self,
        *,
        build_backend: DockerBuildBackendV1,
        executor: object,
    ) -> None:
        self._build_backend = build_backend
        self._executor = executor

    def build(self, request: PipelineRequestV1) -> BuildResultV1:
        """Observe two fresh equal builds without requiring a RUN capability."""

        if type(request) is not PipelineRequestV1:
            raise PipelineInputErrorV1(PipelineInputReasonV1.WRONG_TYPE, "request")
        try:
            docker_report = self._build_backend.probe()
        except Exception:
            return PipelineBlockedV1(
                DockerBlockerReasonV1.BACKEND_CONTRACT,
                "Docker capability probe raised",
            )
        if type(docker_report) is DockerUnsupportedV1:
            return PipelineBlockedV1(docker_report.reason, docker_report.detail)
        if type(docker_report) is not DockerSupportedV1:
            return PipelineBlockedV1(
                DockerBlockerReasonV1.BACKEND_CONTRACT,
                "Docker capability report is not typed",
            )

        builds: list[tuple[bytes, DockerBuildExitedV1]] = []
        for attempt in (1, 2):
            built = self._build_once(request, attempt)
            if type(built) is BuildRejectedV1:
                return built
            builds.append(built)
        first, second = builds
        first_digest = hashlib.sha256(first[0]).digest()
        second_digest = hashlib.sha256(second[0]).digest()
        if first[0] != second[0]:
            return NonReproducibleBuildV1(first_digest, second_digest)

        binary = first[0]
        rebuild_sha256s = (first_digest, second_digest)
        build_processes = (first[1], second[1])
        comparator = _derive_arb_comparator_for_build_v1(
            request,
            docker_report,
            binary,
            rebuild_sha256s,
            build_processes,
        )
        flint_partition = flint_source_content_partition_v1(
            request.source_lock,
            request.admitted_sources,
        )
        return DiagnosticBuildObservationV1(
            request.admitted_sources.identity,
            flint_partition.commit_content_identity,
            flint_partition.commit_content_file_count,
            flint_partition.project_pinned_release_only_identity,
            flint_partition.project_pinned_release_only_file_count,
            request.build_sources.build_input_identity,
            request.build_sources.formula_support_identity,
            pipeline_policy_identity_v1(request.host_trust),
            docker_report.daemon_observation_sha256,
            docker_report.image_reference,
            docker_report.platform,
            first_digest,
            rebuild_sha256s,
            request.host_trust,
            build_processes,
            comparator,
            binary,
            _token=_BUILD_OBSERVATION_TOKEN,
        )

    def execute(self, request: PipelineRequestV1) -> PipelineResultV1:
        if type(request) is not PipelineRequestV1:
            raise PipelineInputErrorV1(PipelineInputReasonV1.WRONG_TYPE, "request")
        build_observation = self.build(request)
        if type(build_observation) is not DiagnosticBuildObservationV1:
            return build_observation
        try:
            execution_report = self._executor.probe()
        except Exception:
            return ExecutionRejectedV1(
                ExecutionFailureReasonV1.BACKEND_CONTRACT,
                "executor capability probe raised",
            )
        if type(execution_report) is executor.UnsupportedV1:
            return ExecutionRejectedV1(
                ExecutionFailureReasonV1.UNSUPPORTED,
                execution_report,
            )
        if type(execution_report) is not executor.SupportedV1:
            return ExecutionRejectedV1(
                ExecutionFailureReasonV1.BACKEND_CONTRACT,
                execution_report,
            )

        # This is the exact first post-exit bytes object retained by BUILD.
        binary = build_observation.binary
        try:
            invocation = executor.ExecutionRequestV1(
                executable=binary,
                argv=(
                    b"arb-evaluator",
                    b"--manifest-identity",
                    build_observation.comparator.identity.hex().encode("ascii"),
                    b"--job",
                    b"/dev/stdin",
                ),
                environment=((b"LC_ALL", b"C"), (b"TZ", b"UTC")),
                cwd=b"/",
                stdin=request.job.encode(),
                umask=0o077,
                limits=request.execution_limits,
            )
        except executor.ExecutionRequestErrorV1 as error:
            return ExecutionRejectedV1(
                ExecutionFailureReasonV1.BACKEND_CONTRACT,
                error,
            )
        invocation_identity = invocation_identity_v1(invocation)
        platform_identity = platform_identity_v1(execution_report)
        try:
            execution_result = self._executor.execute(invocation)
        except Exception:
            return ExecutionRejectedV1(
                ExecutionFailureReasonV1.BACKEND_CONTRACT,
                "executor raised",
            )
        if type(execution_result) is not executor.CompletedV1:
            if not executor._result_matches_request(execution_result, invocation):
                return ExecutionRejectedV1(
                    ExecutionFailureReasonV1.BACKEND_CONTRACT,
                    execution_result,
                )
            return ExecutionRejectedV1(
                ExecutionFailureReasonV1.PROCESS_FAILED,
                execution_result,
            )
        if execution_result.binary_sha256 != build_observation.binary_sha256:
            return ExecutionRejectedV1(
                ExecutionFailureReasonV1.BINARY_MISMATCH,
                execution_result,
            )
        if not executor._result_matches_request(execution_result, invocation):
            return ExecutionRejectedV1(
                ExecutionFailureReasonV1.BACKEND_CONTRACT,
                execution_result,
            )
        if execution_result.stderr:
            return ExecutionRejectedV1(
                ExecutionFailureReasonV1.STDERR_NOT_EMPTY,
                execution_result,
            )
        transcript_bytes = execution_result.stdout
        try:
            transcript = protocol.DecisionTranscriptV1.parse(transcript_bytes)
        except protocol.ProtocolErrorV1 as error:
            return TranscriptRejectedV1(
                TranscriptFailureReasonV1.INVALID_WIRE,
                str(error),
            )
        if (
            transcript.encode() != transcript_bytes
            or transcript.job_identity != request.job.identity
            or transcript.domain_identity != request.job.domain.identity
            or transcript.comparator_identity != build_observation.comparator.identity
            or transcript.point_count != request.job.domain.point_count
        ):
            return TranscriptRejectedV1(
                TranscriptFailureReasonV1.FOREIGN_BINDING,
                "transcript does not bind the exact job/domain/comparator",
            )
        try:
            protocol._validate_witness_alignment(
                request.job.domain,
                transcript.decision_bits,
                transcript.point_count,
                transcript.counters,
                transcript.witness_store,
            )
        except protocol.ProtocolErrorV1 as error:
            return TranscriptRejectedV1(
                TranscriptFailureReasonV1.FOREIGN_BINDING,
                str(error),
            )
        try:
            run_claim = protocol.RunClaimV1.for_transcript(
                request.job,
                build_observation.comparator.manifest,
                transcript,
                build_observation.binary_sha256,
                invocation_identity,
                platform_identity,
            )
        except protocol.ProtocolErrorV1 as error:
            return TranscriptRejectedV1(
                TranscriptFailureReasonV1.FOREIGN_BINDING,
                str(error),
            )
        return DiagnosticPipelineObservationV1(
            build_observation,
            invocation_identity,
            platform_identity,
            transcript,
            run_claim,
            transcript_bytes,
            _token=_PIPELINE_OBSERVATION_TOKEN,
        )

    def _build_once(
        self,
        request: PipelineRequestV1,
        attempt: int,
    ) -> tuple[bytes, DockerBuildExitedV1] | BuildRejectedV1:
        try:
            with tempfile.TemporaryDirectory(prefix=f"labcolors-arb-build-v1-{attempt}-") as temporary:
                root = Path(temporary).resolve()
                inputs = root / "inputs"
                workspace = root / "workspace"
                build = root / "build"
                output = root / "out"
                for directory in (inputs, workspace, build, output):
                    directory.mkdir(mode=0o755)
                    directory.chmod(0o755)

                for lock, admitted in zip(
                    request.source_lock.sources,
                    request.admitted_sources.sources,
                    strict=True,
                ):
                    destination = inputs / lock.root_prefix[:-1]
                    snapshot.materialize_source_archive(
                        lock,
                        admitted,
                        destination,
                    )
                    destination.chmod(0o755)
                workspace_files: list[BuildSourceFileV1] = []
                for item in request.build_sources.files:
                    if item.path == GENERATED_FORMULA_PATH_V1:
                        generated = BuildSourceFileV1(
                            "formula.generated.c",
                            item.mode,
                            item.contents,
                        )
                        _write_exact_file(inputs, generated)
                    else:
                        _write_exact_file(workspace, item)
                        workspace_files.append(item)
                build_request = DockerBuildRequestV1(
                    attempt,
                    root,
                    inputs,
                    workspace,
                    build,
                    output,
                    root / "container.cid",
                    _CONTAINER_NAME_PREFIX_V1
                    + hashlib.sha256(
                        os.fsencode(root) + bytes((attempt,))
                    ).hexdigest(),
                )
                try:
                    process = self._build_backend.run_build(build_request)
                except Exception:
                    return BuildRejectedV1(
                        attempt,
                        BuildFailureReasonV1.BACKEND_CONTRACT,
                    )
                known_process_types = (
                    DockerBuildExitedV1,
                    DockerBuildTimedOutV1,
                    DockerBuildOutputLimitV1,
                    DockerBuildObserverFailureV1,
                    DockerBuildCleanupFailureV1,
                )
                if type(process) not in known_process_types:
                    return BuildRejectedV1(
                        attempt,
                        BuildFailureReasonV1.BACKEND_CONTRACT,
                    )
                if type(process) is DockerBuildCleanupFailureV1:
                    return BuildRejectedV1(
                        attempt,
                        BuildFailureReasonV1.CLEANUP_FAILED,
                        process,
                    )
                if type(process) is not DockerBuildExitedV1 or process.returncode != 0:
                    return BuildRejectedV1(
                        attempt,
                        BuildFailureReasonV1.PROCESS_FAILED,
                        process,
                    )
                try:
                    for lock, admitted in zip(
                        request.source_lock.sources,
                        request.admitted_sources.sources,
                        strict=True,
                    ):
                        expected = {
                            item.path: (item.mode, item.length, item.sha256)
                            for item in admitted.files
                        }
                        _verify_exact_tree(inputs / lock.root_prefix[:-1], expected)
                    expected_workspace = {
                        item.path: (
                            item.mode,
                            len(item.contents),
                            hashlib.sha256(item.contents).digest(),
                        )
                        for item in workspace_files
                    }
                    _verify_exact_tree(workspace, expected_workspace)
                    generated = request.build_sources.generated_formula
                    _verify_exact_tree(
                        inputs,
                        {
                            "formula.generated.c": (
                                0o644,
                                len(generated),
                                hashlib.sha256(generated).digest(),
                            ),
                            **{
                                f"{lock.root_prefix[:-1]}/{item.path}": (
                                    item.mode,
                                    item.length,
                                    item.sha256,
                                )
                                for lock, admitted in zip(
                                    request.source_lock.sources,
                                    request.admitted_sources.sources,
                                    strict=True,
                                )
                                for item in admitted.files
                            },
                        },
                    )
                except _TreeMismatchV1:
                    return BuildRejectedV1(
                        attempt,
                        BuildFailureReasonV1.INPUT_CHANGED,
                        process,
                    )
                try:
                    binary = _read_build_output(
                        output,
                        request.execution_limits.max_executable_bytes,
                    )
                    executor._require_static_x86_64_elf(binary)
                except (OSError, _TreeMismatchV1, executor.ExecutionRequestErrorV1):
                    return BuildRejectedV1(
                        attempt,
                        BuildFailureReasonV1.INVALID_OUTPUT,
                        process,
                    )
                return binary, process
        except (OSError, snapshot.SnapshotErrorV1, BuildSourceAdmissionErrorV1):
            return BuildRejectedV1(
                attempt,
                BuildFailureReasonV1.BACKEND_CONTRACT,
            )
