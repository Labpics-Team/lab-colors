#!/usr/bin/env python3
"""Controlled offline BUILD observations for the Arb evaluator.

The unsealed Linux x64 host and its Docker daemon are explicitly inside this
V1 trust boundary. Provider identity and host freshness are not observable
here. This module emits neither SLSA nor source-bound receipts: it observes two
fresh-container builds and owns their exact output bytes.  The one-shot
source-bound controller owns RUN and receipt sealing in ``receipt.py``.
"""

from __future__ import annotations

import hashlib
import io
import json
import os
import platform
import selectors
import signal
import stat
import subprocess
import tarfile
import tempfile
import time
from dataclasses import dataclass, field, fields
from enum import StrEnum
from functools import cached_property
from pathlib import Path
from typing import NoReturn, Protocol, TypeAlias

import executor
import provenance
import region_proof_protocol as protocol


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
# is deliberately updated together with its binding tests.
_PINNED_BUILD_SOURCE_SHA256_V1 = {
    FORMULA_SPEC_PATH_V1: FORMULA_SPEC_SHA256_V1,
    GENERATED_FORMULA_PATH_V1: GENERATED_FORMULA_SHA256_V1,
    BUILD_RECIPE_PATH_V1: "92d6de1a321d5e097e122eeda68111d75283089b0c75adc0d359d46494a65390",
    FORMULA_GENERATOR_PATH_V1: "16629cc3a2ef745ae244ae4762f8946a6546972886f96beeb9ee4920b043040c",
    "proof/region/v1/arb/evaluator/formula.h": "46fd5ad1b68b728efcd990a71d1dcc273b75e3391d8c06ef2fd0ac6a4d7dfdbd",
    "proof/region/v1/arb/evaluator/hash.c": "c28e6281208f09ca15fa74aea0091f27726ed68efc3480c34a7db33b8ca3567e",
    "proof/region/v1/arb/evaluator/hash.h": "a62c07f2eca9294b4c1c802e2a9e6cff6ad9f8fd696a74b54a21489d56fab6c4",
    "proof/region/v1/arb/evaluator/interval.c": "93f206258b83fc0f373ae865787ebf266c9d011f2578567ed913a7cb6c0ed899",
    "proof/region/v1/arb/evaluator/interval.h": "f9d7416059d4b09979c22e6823a747f252c576558c750fe3e2ff92509894c7b3",
    "proof/region/v1/arb/evaluator/main.c": "e9a3fa6b70b3a25eb6d6cf7eaba9a98d2fbe5cb7fdd3c1790219efb7fe20918d",
    "proof/region/v1/arb/evaluator/region.c": "0026d501077911eae58933487a4cac0a83003cd70d1dbf0966890c29bfff8f99",
    "proof/region/v1/arb/evaluator/region.h": "95da5117bb162c707b441242637d5e0e1bbeef2532ac1f10248f2b93ab16dcc8",
    "proof/region/v1/arb/evaluator/wire.c": "4edb1120a8274774b8790eceea877c664f599bb9e039b0aa6e6ba8dafe124d47",
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

# FLINT's exact locked qsieve path uses /tmp directly rather than TMPDIR.  This
# independent operational cap is part of the build policy; overflow rejects.
BUILD_TMP_LIMIT_BYTES_V1 = 512 * 1024 * 1024
_BUILD_TMPFS_SPEC_V1 = (
    f"/tmp:rw,noexec,nosuid,nodev,size={BUILD_TMP_LIMIT_BYTES_V1},mode=1777"
)

# This is a versioned resource policy, not a mathematical constant.  Four GiB
# is the first shipping cap for one serial GMP/MPFR/FLINT build plus their
# upstream test artifacts.  The no-skip native gate is the authority for
# lowering it; exhaustion rejects the build instead of falling back to a host
# directory or an unbounded Docker volume.
BUILD_STATE_LIMIT_BYTES_V1 = 4 * 1024 * 1024 * 1024
_BUILD_STATE_TMPFS_SPEC_V1 = (
    f"/build:rw,exec,nosuid,nodev,size={BUILD_STATE_LIMIT_BYTES_V1},mode=0777"
)

_BUILD_BOOTSTRAP_V1 = r"""set -eu
exec 3>&1
exec 1>&2
umask 077
readonly bundle=/build/input.bundle
readonly snapshot=/build/snapshot
/usr/bin/cat > "$bundle"
actual_length=$(/usr/bin/wc -c < "$bundle")
if [ "$actual_length" != "$1" ]; then
    printf '%s\n' 'build input bundle length mismatch' >&2
    exit 65
fi
printf '%s  %s\n' "$2" "$bundle" | /usr/bin/sha256sum --check --strict -
/usr/bin/mkdir "$snapshot" /build/work
umask 022
/usr/bin/tar --extract --file "$bundle" --directory "$snapshot" --no-same-owner
/usr/bin/rm "$bundle"
umask 077
/bin/sh "$snapshot/workspace/proof/region/v1/arb/build.sh"
/usr/bin/cat /build/work/arb-evaluator-v1 >&3
"""

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
_BUILD_INPUT_BUNDLE_ID_LABEL_V1 = (
    b"labcolors.proof-region.arb-build-input-bundle.v1\0"
)
_BUILD_SOURCES_TOKEN = object()
_COMPARATOR_TOKEN = object()
_BUILD_OBSERVATION_TOKEN = object()
_BUILD_INPUT_BUNDLE_TOKEN = object()
_BUILD_INPUT_PROGRESS_TOKEN = object()
_BUILD_INPUT_TRANSFER_TOKEN = object()
_DOCKER_COMMAND_EXITED_TOKEN = object()
_DOCKER_BUILD_EXITED_TOKEN = object()


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


def build_source_manifest_bytes_v1(sources: AdmittedBuildSourcesV1) -> bytes:
    """Replay and encode the canonical retained build-source manifest."""

    if type(sources) is not AdmittedBuildSourcesV1:
        raise TypeError("sources must be AdmittedBuildSourcesV1")
    replayed = admit_build_sources_v1(sources.files)
    if (
        replayed.identity != sources.identity
        or replayed.build_input_identity != sources.build_input_identity
        or replayed.formula_support_identity != sources.formula_support_identity
    ):
        raise TypeError("retained build-source coordinates changed")
    chunks: list[bytes] = [len(sources.files).to_bytes(4, "big")]
    for item in sources.files:
        chunks.extend(
            (
                item.path.encode("ascii"),
                item.mode.to_bytes(4, "big"),
                len(item.contents).to_bytes(8, "big"),
                hashlib.sha256(item.contents).digest(),
            )
        )
    return b"".join(_blob(chunk) for chunk in chunks)


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


@dataclass(frozen=True, init=False)
class SealedBuildInputBundleV1:
    """One controller-owned immutable byte object reused by both BUILDs."""

    source_identity: bytes
    build_input_identity: bytes
    sha256: bytes
    length: int
    identity: bytes
    _contents: bytes = field(repr=False, compare=False)

    def __init__(
        self,
        source_identity: bytes,
        build_input_identity: bytes,
        contents: bytes,
        *,
        _token: object,
    ) -> None:
        if _token is not _BUILD_INPUT_BUNDLE_TOKEN:
            raise TypeError(
                "SealedBuildInputBundleV1 is created only by the build controller"
            )
        if not _valid_digest(source_identity) or not _valid_digest(
            build_input_identity
        ):
            raise TypeError("invalid build input coordinates")
        if type(contents) is not bytes or not contents:
            raise TypeError("build input bundle must be owned nonempty bytes")
        digest = hashlib.sha256(contents).digest()
        identity = _identity(
            _BUILD_INPUT_BUNDLE_ID_LABEL_V1,
            (
                source_identity,
                build_input_identity,
                len(contents).to_bytes(8, "big"),
                digest,
                hashlib.sha256(_BUILD_BOOTSTRAP_V1.encode("utf-8")).digest(),
            ),
        )
        for name, value in (
            ("source_identity", source_identity),
            ("build_input_identity", build_input_identity),
            ("sha256", digest),
            ("length", len(contents)),
            ("identity", identity),
            ("_contents", contents),
        ):
            object.__setattr__(self, name, value)


def sealed_build_input_bundle_is_well_bound_v1(value: object) -> bool:
    if type(value) is not SealedBuildInputBundleV1:
        return False
    try:
        digest = hashlib.sha256(value._contents).digest()
        identity = _identity(
            _BUILD_INPUT_BUNDLE_ID_LABEL_V1,
            (
                value.source_identity,
                value.build_input_identity,
                len(value._contents).to_bytes(8, "big"),
                digest,
                hashlib.sha256(_BUILD_BOOTSTRAP_V1.encode("utf-8")).digest(),
            ),
        )
        return (
            _valid_digest(value.source_identity)
            and _valid_digest(value.build_input_identity)
            and type(value._contents) is bytes
            and bool(value._contents)
            and value.length == len(value._contents)
            and value.sha256 == digest
            and value.identity == identity
        )
    except Exception:
        return False


def _canonical_tar_v1(
    entries: tuple[tuple[str, int, bytes], ...],
) -> bytes:
    if type(entries) is not tuple or not entries:
        raise TypeError("build bundle entries must be a canonical nonempty set")
    paths = tuple(path for path, _mode, _contents in entries)
    if paths != tuple(sorted(paths)) or len(set(paths)) != len(entries):
        raise TypeError("build bundle entries must be a canonical nonempty set")
    folded_paths: set[str] = set()
    directories: set[str] = set()
    for path, _mode, _contents in entries:
        _logical_path(path)
        folded = path.lower()
        if folded in folded_paths:
            raise TypeError("build bundle paths must be case-distinct")
        folded_paths.add(folded)
        parts = path.split("/")[:-1]
        for length in range(1, len(parts) + 1):
            directories.add("/".join(parts[:length]))
    if directories.intersection(paths):
        raise TypeError("build bundle file cannot also be a directory")
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        for path in sorted(directories, key=lambda value: (value.count("/"), value)):
            member = tarfile.TarInfo(path)
            member.type = tarfile.DIRTYPE
            member.mode = 0o755
            member.uid = 0
            member.gid = 0
            member.uname = ""
            member.gname = ""
            member.mtime = 0
            member.size = 0
            archive.addfile(member)
        for path, mode, contents in entries:
            _logical_path(path)
            if (
                type(mode) is not int
                or mode not in (0o644, 0o755)
                or type(contents) is not bytes
            ):
                raise TypeError("invalid build bundle entry")
            member = tarfile.TarInfo(path)
            member.type = tarfile.REGTYPE
            member.mode = mode
            member.uid = 0
            member.gid = 0
            member.uname = ""
            member.gname = ""
            member.mtime = 0
            member.size = len(contents)
            archive.addfile(member, io.BytesIO(contents))
    return output.getvalue()


def _normalized_source_entries_v1(
    lock: provenance.SourceReleaseLockV1,
    admitted: provenance.SafeSourceArchiveV1,
) -> tuple[tuple[str, int, bytes], ...]:
    replayed, raw_tar = provenance.replay_admitted_source_archive_v1(
        lock,
        admitted,
    )
    if (
        replayed.source_lock_identity != admitted.source_lock_identity
        or replayed.archive_sha256 != admitted.archive_sha256
        or replayed.tree_identity != admitted.tree_identity
        or replayed.regular_file_count != admitted.regular_file_count
        or replayed.regular_file_bytes != admitted.regular_file_bytes
        or replayed.files != admitted.files
    ):
        raise TypeError("admitted source coordinates changed before bundle sealing")
    expected = {item.path: item for item in replayed.files}
    values: list[tuple[str, int, bytes]] = []
    seen: set[str] = set()
    with tarfile.open(fileobj=io.BytesIO(raw_tar), mode="r:") as archive:
        for member in archive:
            if member.isdir():
                continue
            if not member.isreg() or not member.name.startswith(lock.root_prefix):
                raise TypeError("admitted source replay contains a foreign member")
            relative = member.name[len(lock.root_prefix) :]
            coordinate = expected.get(relative)
            if coordinate is None or relative in seen:
                raise TypeError("admitted source replay changed its file set")
            stream = archive.extractfile(member)
            if stream is None:
                raise TypeError("admitted source replay lost a regular file")
            chunks: list[bytes] = []
            length = 0
            hasher = hashlib.sha256()
            while True:
                chunk = stream.read(provenance.READ_CHUNK_BYTES)
                if not chunk:
                    break
                length += len(chunk)
                if length > coordinate.length:
                    raise TypeError("admitted source replay exceeded locked length")
                chunks.append(chunk)
                hasher.update(chunk)
            if length != coordinate.length or hasher.digest() != coordinate.sha256:
                raise TypeError("admitted source replay changed locked contents")
            values.append(
                (
                    f"inputs/{lock.root_prefix[:-1]}/{relative}",
                    coordinate.mode,
                    b"".join(chunks),
                )
            )
            seen.add(relative)
    if seen != set(expected):
        raise TypeError("admitted source replay is incomplete")
    return tuple(sorted(values))


def _seal_build_input_bundle_v1(
    request: "PipelineRequestV1",
) -> SealedBuildInputBundleV1:
    if type(request) is not PipelineRequestV1:
        raise TypeError("request must be PipelineRequestV1")
    source_entries = tuple(
        entry
        for lock, admitted in zip(
            request.source_lock.sources,
            request.admitted_sources.sources,
            strict=True,
        )
        for entry in _normalized_source_entries_v1(lock, admitted)
    )
    workspace_entries = tuple(
        (
            "inputs/formula.generated.c"
            if item.path == GENERATED_FORMULA_PATH_V1
            else f"workspace/{item.path}",
            item.mode,
            item.contents,
        )
        for item in request.build_sources.files
        if item.path not in (FORMULA_SPEC_PATH_V1, FORMULA_GENERATOR_PATH_V1)
    )
    contents = _canonical_tar_v1(tuple(sorted(source_entries + workspace_entries)))
    return SealedBuildInputBundleV1(
        request.admitted_sources.identity,
        request.build_sources.build_input_identity,
        contents,
        _token=_BUILD_INPUT_BUNDLE_TOKEN,
    )


class HostTrustBoundaryV1(StrEnum):
    UNSEALED_LINUX_X64_DOCKER_HOST = "unsealed-linux-x64-docker-host"


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
            b"network=none",
            b"rootfs=readonly",
            b"scratch-tmpfs=" + _BUILD_TMPFS_SPEC_V1.encode("ascii"),
            b"build-state-tmpfs=" + _BUILD_STATE_TMPFS_SPEC_V1.encode("ascii"),
            b"cap-drop=all",
            b"no-new-privileges=true",
            b"inputs=one-controller-sealed-normalized-tree-ustar",
            b"transport=bounded-docker-stdin-v1",
            b"container-admission=exact-length-and-sha256-before-extraction",
            b"output=bounded-docker-stdout-v1",
            hashlib.sha256(_BUILD_BOOTSTRAP_V1.encode("utf-8")).digest(),
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
    manifest: protocol.ContentResolvedComparatorManifestV2
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
        manifest: protocol.ContentResolvedComparatorManifestV2,
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
            type(manifest) is not protocol.ContentResolvedComparatorManifestV2
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
        replayed = protocol.ContentResolvedComparatorManifestV2.admit(
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
        for field_name, field_value in (
            ("preimages", preimages),
            ("manifest", manifest),
            ("structural_source_identity", structural_source_identity),
            ("build_input_identity", build_input_identity),
            ("pipeline_policy_identity", pipeline_policy_identity),
            ("binary_sha256", binary_sha256),
            ("rebuild_sha256s", rebuild_sha256s),
        ):
            object.__setattr__(self, field_name, field_value)

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
            self.execution_limits.max_executable_bytes > BUILD_STDOUT_LIMIT_V1
            or len(job_bytes) > self.execution_limits.max_stdin_bytes
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
    input_bundle: SealedBuildInputBundleV1
    max_executable_bytes: int
    cid_file: Path
    container_name: str

    def __post_init__(self) -> None:
        if type(self.attempt) is not int or self.attempt not in (1, 2):
            raise TypeError("attempt must be 1 or 2")
        if not sealed_build_input_bundle_is_well_bound_v1(self.input_bundle):
            raise TypeError("input_bundle must be controller sealed and well bound")
        if (
            type(self.max_executable_bytes) is not int
            or self.max_executable_bytes <= 0
            or self.max_executable_bytes > BUILD_STDOUT_LIMIT_V1
        ):
            raise TypeError("invalid executable output limit")
        _absolute_path(self.cid_file, "cid_file")
        _container_name(self.container_name)


def _bounded_bytes(value: object, maximum: int, field_name: str) -> bytes:
    if type(value) is not bytes or len(value) > maximum:
        raise TypeError(f"invalid {field_name}")
    return value


@dataclass(frozen=True, init=False)
class BuildInputTransferProgressV1:
    bundle_identity: bytes
    expected_length: int
    expected_sha256: bytes
    written_length: int
    written_sha256: bytes

    def __init__(
        self,
        bundle_identity: bytes,
        expected_length: int,
        expected_sha256: bytes,
        written_length: int,
        written_sha256: bytes,
        *,
        _token: object,
    ) -> None:
        if _token is not _BUILD_INPUT_PROGRESS_TOKEN:
            raise TypeError("build input progress is controller-observed")
        if not _valid_digest(bundle_identity) or not _valid_digest(expected_sha256):
            raise TypeError("invalid build input progress coordinates")
        if (
            type(expected_length) is not int
            or expected_length <= 0
            or type(written_length) is not int
            or written_length < 0
            or written_length > expected_length
            or type(written_sha256) is not bytes
            or len(written_sha256) != 32
        ):
            raise TypeError("invalid build input progress")
        for name, value in (
            ("bundle_identity", bundle_identity),
            ("expected_length", expected_length),
            ("expected_sha256", expected_sha256),
            ("written_length", written_length),
            ("written_sha256", written_sha256),
        ):
            object.__setattr__(self, name, value)


def _build_input_progress_v1(
    bundle: SealedBuildInputBundleV1,
    written_length: int,
    written_sha256: bytes,
) -> BuildInputTransferProgressV1:
    if not sealed_build_input_bundle_is_well_bound_v1(bundle):
        raise TypeError("build input bundle is not well bound")
    if (
        type(written_length) is not int
        or written_length < 0
        or written_length > bundle.length
        or type(written_sha256) is not bytes
        or written_sha256
        != hashlib.sha256(bundle._contents[:written_length]).digest()
    ):
        raise TypeError("build input progress does not match the sealed bytes")
    return BuildInputTransferProgressV1(
        bundle.identity,
        bundle.length,
        bundle.sha256,
        written_length,
        written_sha256,
        _token=_BUILD_INPUT_PROGRESS_TOKEN,
    )


@dataclass(frozen=True, init=False)
class BuildInputTransferV1:
    bundle_identity: bytes
    expected_length: int
    expected_sha256: bytes
    written_length: int
    written_sha256: bytes

    def __init__(
        self,
        progress: BuildInputTransferProgressV1,
        *,
        _token: object,
    ) -> None:
        if _token is not _BUILD_INPUT_TRANSFER_TOKEN:
            raise TypeError("build input transfer is controller-observed")
        if (
            type(progress) is not BuildInputTransferProgressV1
            or progress.written_length != progress.expected_length
            or progress.written_sha256 != progress.expected_sha256
        ):
            raise TypeError("completed build input transfer must be exact")
        for name in (
            "bundle_identity",
            "expected_length",
            "expected_sha256",
            "written_length",
            "written_sha256",
        ):
            object.__setattr__(self, name, getattr(progress, name))


def _completed_build_input_transfer_v1(
    bundle: SealedBuildInputBundleV1,
    written_length: int,
    written_sha256: bytes,
) -> BuildInputTransferV1:
    progress = _build_input_progress_v1(bundle, written_length, written_sha256)
    return BuildInputTransferV1(
        progress,
        _token=_BUILD_INPUT_TRANSFER_TOKEN,
    )


@dataclass(frozen=True, init=False)
class _DockerCommandExitedV1:
    returncode: int
    stdout: bytes
    stderr: bytes

    def __init__(
        self,
        returncode: int,
        stdout: bytes,
        stderr: bytes,
        *,
        _token: object,
    ) -> None:
        if _token is not _DOCKER_COMMAND_EXITED_TOKEN:
            raise TypeError("Docker command exit is controller-observed")
        if type(returncode) is not int:
            raise TypeError("invalid Docker returncode")
        _bounded_bytes(stdout, BUILD_STDOUT_LIMIT_V1, "stdout")
        _bounded_bytes(stderr, BUILD_STDERR_LIMIT_V1, "stderr")
        object.__setattr__(self, "returncode", returncode)
        object.__setattr__(self, "stdout", stdout)
        object.__setattr__(self, "stderr", stderr)


def _docker_command_exited_v1(
    returncode: int,
    stdout: bytes,
    stderr: bytes,
) -> _DockerCommandExitedV1:
    return _DockerCommandExitedV1(
        returncode,
        stdout,
        stderr,
        _token=_DOCKER_COMMAND_EXITED_TOKEN,
    )


@dataclass(frozen=True, init=False)
class DockerBuildExitedV1:
    returncode: int
    stdout: bytes
    stderr: bytes
    input_transfer: BuildInputTransferV1

    def __init__(
        self,
        returncode: int,
        stdout: bytes,
        stderr: bytes,
        input_transfer: BuildInputTransferV1,
        *,
        _token: object,
    ) -> None:
        if _token is not _DOCKER_BUILD_EXITED_TOKEN:
            raise TypeError("Docker build exit is controller-observed")
        if type(returncode) is not int:
            raise TypeError("invalid Docker returncode")
        _bounded_bytes(stdout, BUILD_STDOUT_LIMIT_V1, "stdout")
        _bounded_bytes(stderr, BUILD_STDERR_LIMIT_V1, "stderr")
        if type(input_transfer) is not BuildInputTransferV1:
            raise TypeError("invalid Docker build input transfer")
        object.__setattr__(self, "returncode", returncode)
        object.__setattr__(self, "stdout", stdout)
        object.__setattr__(self, "stderr", stderr)
        object.__setattr__(self, "input_transfer", input_transfer)


def _docker_build_exited_v1(
    returncode: int,
    stdout: bytes,
    stderr: bytes,
    input_transfer: BuildInputTransferV1,
) -> DockerBuildExitedV1:
    return DockerBuildExitedV1(
        returncode,
        stdout,
        stderr,
        input_transfer,
        _token=_DOCKER_BUILD_EXITED_TOKEN,
    )


@dataclass(frozen=True)
class DockerBuildTimedOutV1:
    stdout: bytes
    stderr: bytes
    input_progress: BuildInputTransferProgressV1 | None = None

    def __post_init__(self) -> None:
        _bounded_bytes(self.stdout, BUILD_STDOUT_LIMIT_V1, "stdout")
        _bounded_bytes(self.stderr, BUILD_STDERR_LIMIT_V1, "stderr")
        if self.input_progress is not None and type(
            self.input_progress
        ) is not BuildInputTransferProgressV1:
            raise TypeError("invalid timed-out build input progress")


class DockerOutputStreamV1(StrEnum):
    STDOUT = "stdout"
    STDERR = "stderr"


@dataclass(frozen=True)
class DockerBuildOutputLimitV1:
    stream: DockerOutputStreamV1
    stdout: bytes
    stderr: bytes
    input_progress: BuildInputTransferProgressV1 | None = None

    def __post_init__(self) -> None:
        if type(self.stream) is not DockerOutputStreamV1:
            raise TypeError("invalid Docker output stream")
        _bounded_bytes(self.stdout, BUILD_STDOUT_LIMIT_V1, "stdout")
        _bounded_bytes(self.stderr, BUILD_STDERR_LIMIT_V1, "stderr")
        if self.input_progress is not None and type(
            self.input_progress
        ) is not BuildInputTransferProgressV1:
            raise TypeError("invalid output-limited build input progress")


@dataclass(frozen=True)
class DockerBuildObserverFailureV1:
    detail: str

    def __post_init__(self) -> None:
        if type(self.detail) is not str or not self.detail or len(self.detail) > 4096:
            raise TypeError("invalid Docker observer failure")


@dataclass(frozen=True)
class DockerBuildInputRejectedV1:
    input_progress: BuildInputTransferProgressV1
    stdout: bytes
    stderr: bytes

    def __post_init__(self) -> None:
        if type(self.input_progress) is not BuildInputTransferProgressV1:
            raise TypeError("invalid partial build input progress")
        _bounded_bytes(self.stdout, BUILD_STDOUT_LIMIT_V1, "stdout")
        _bounded_bytes(self.stderr, BUILD_STDERR_LIMIT_V1, "stderr")

    @property
    def written_length(self) -> int:
        return self.input_progress.written_length

    @property
    def written_sha256(self) -> bytes:
        return self.input_progress.written_sha256


class DockerCleanupTriggerV1(StrEnum):
    PROCESS_EXIT = "process_exit"
    INPUT_TRANSFER = "input_transfer"
    TIMEOUT = "timeout"
    OUTPUT_LIMIT = "output_limit"
    OBSERVER_FAILURE = "observer_failure"


@dataclass(frozen=True)
class DockerBuildCleanupFailureV1:
    trigger: DockerCleanupTriggerV1
    detail: str
    stdout: bytes
    stderr: bytes
    input_progress: BuildInputTransferProgressV1 | None = None

    def __post_init__(self) -> None:
        if type(self.trigger) is not DockerCleanupTriggerV1:
            raise TypeError("invalid Docker cleanup trigger")
        if type(self.detail) is not str or not self.detail or len(self.detail) > 4096:
            raise TypeError("invalid Docker cleanup failure")
        _bounded_bytes(self.stdout, BUILD_STDOUT_LIMIT_V1, "stdout")
        _bounded_bytes(self.stderr, BUILD_STDERR_LIMIT_V1, "stderr")
        if self.input_progress is not None and type(
            self.input_progress
        ) is not BuildInputTransferProgressV1:
            raise TypeError("invalid cleanup build input progress")


DockerBuildProcessObservationV1: TypeAlias = (
    DockerBuildExitedV1
    | DockerBuildTimedOutV1
    | DockerBuildOutputLimitV1
    | DockerBuildObserverFailureV1
    | DockerBuildInputRejectedV1
    | DockerBuildCleanupFailureV1
)

_DockerCommandObservationV1: TypeAlias = (
    _DockerCommandExitedV1 | DockerBuildProcessObservationV1
)


class DockerBuildBackendV1(Protocol):
    def probe(self) -> DockerCapabilityReportV1: ...

    def run_build(
        self,
        request: DockerBuildRequestV1,
    ) -> DockerBuildProcessObservationV1: ...


def build_process_bytes_v1(process: DockerBuildExitedV1) -> bytes:
    if (
        type(process) is not DockerBuildExitedV1
        or type(process.input_transfer) is not BuildInputTransferV1
    ):
        raise TypeError("only successful typed build observations are encodable")
    return b"".join(
        (
            process.returncode.to_bytes(4, "big", signed=True),
            len(process.stdout).to_bytes(8, "big"),
            hashlib.sha256(process.stdout).digest(),
            len(process.stderr).to_bytes(8, "big"),
            hashlib.sha256(process.stderr).digest(),
            process.input_transfer.bundle_identity,
            process.input_transfer.expected_length.to_bytes(8, "big"),
            process.input_transfer.expected_sha256,
            process.input_transfer.written_length.to_bytes(8, "big"),
            process.input_transfer.written_sha256,
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
            b"gap:host-and-docker-daemon-not-source-bound",
            b"gap:unsealed-diagnostic-build-observer",
            b"gap:libc-libm-libpthread-libgcc-and-build-utility-source",
            b"gap:no-per-test-result-records",
            b"gap:no-git-derivation-for-project-pinned-release-only-files",
            b"gap:no-origin-authority-reverification",
            request.host_trust.value.encode("ascii"),
            b"build-observation=diagnostic-unsealed-v1",
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
        upstream_chunks.extend(
            provenance.source_archive_replay_coordinates_v1(lock, source)
        )
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

    process_bytes = tuple(build_process_bytes_v1(item) for item in build_processes)
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
    manifest_value = protocol.ComparatorManifestV2(
        protocol.ComparatorKindV1.ARB,
        *coordinates,
    )
    by_digest = {
        coordinate: getattr(preimages, item.name)
        for coordinate, item in zip(coordinates, fields(preimages), strict=True)
    }
    resolved = protocol.ContentResolvedComparatorManifestV2.admit(
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
    """Docker adapter whose probe observes only Linux x64 and its daemon."""

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
        self._platform_name = (
            platform.system().lower() if platform_name is None else platform_name
        )
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
                type(result) is not _DockerCommandExitedV1
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
        command = [
            str(self._docker_path),
            "run",
            "--rm",
            "--interactive",
            "--pull",
            "never",
            "--platform",
            OCI_PLATFORM_V1,
            "--network",
            "none",
            "--read-only",
            "--tmpfs",
            _BUILD_TMPFS_SPEC_V1,
            "--tmpfs",
            _BUILD_STATE_TMPFS_SPEC_V1,
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
        command.extend(
            (
            "--entrypoint",
                "/usr/bin/env",
                OCI_IMAGE_REFERENCE_V1,
                "-i",
                "PATH=/usr/local/bin:/usr/bin:/bin",
                "LC_ALL=C",
                "LANG=C",
                "TZ=UTC",
                "HOME=/nonexistent",
                "/bin/sh",
                "-c",
                _BUILD_BOOTSTRAP_V1,
                "labcolors-arb-build-bootstrap-v1",
                str(request.input_bundle.length),
                request.input_bundle.sha256.hex(),
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
            stdout_limit=request.max_executable_bytes,
            stderr_limit=BUILD_STDERR_LIMIT_V1,
            timeout_ns=BUILD_TIMEOUT_NS_V1,
            cid_file=request.cid_file,
            container_name=request.container_name,
            input_bundle=request.input_bundle,
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
        input_bundle: SealedBuildInputBundleV1 | None = None,
    ) -> _DockerCommandObservationV1:
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
        if input_bundle is not None and type(input_bundle) is not SealedBuildInputBundleV1:
            raise TypeError("input_bundle must be controller sealed")
        if input_bundle is not None and not sealed_build_input_bundle_is_well_bound_v1(
            input_bundle
        ):
            return DockerBuildObserverFailureV1("build input bundle is not well bound")
        try:
            process = subprocess.Popen(
                command,
                stdin=subprocess.PIPE if input_bundle is not None else subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                cwd="/",
                env=self._environment(),
                close_fds=True,
                start_new_session=True,
            )
        except OSError:
            return DockerBuildObserverFailureV1("cannot start Docker CLI")
        if (
            process.stdout is None
            or process.stderr is None
            or (input_bundle is not None and process.stdin is None)
        ):
            input_progress = (
                _build_input_progress_v1(
                    input_bundle,
                    0,
                    hashlib.sha256(b"").digest(),
                )
                if input_bundle is not None
                else None
            )
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
                    input_progress,
                )
            return DockerBuildObserverFailureV1("Docker pipes unavailable")

        stdout = bytearray()
        stderr = bytearray()
        selector = selectors.DefaultSelector()
        terminal: DockerOutputStreamV1 | None = None
        timed_out = False
        observer_failed = False
        input_failed = False
        written = 0
        input_hasher = hashlib.sha256()
        bundle_view = (
            memoryview(input_bundle._contents) if input_bundle is not None else None
        )
        try:
            streams = (
                (
                    process.stdout.fileno(),
                    DockerOutputStreamV1.STDOUT,
                    stdout,
                    stdout_limit,
                ),
                (
                    process.stderr.fileno(),
                    DockerOutputStreamV1.STDERR,
                    stderr,
                    stderr_limit,
                ),
            )
            for descriptor, stream, target, maximum in streams:
                os.set_blocking(descriptor, False)
                selector.register(
                    descriptor,
                    selectors.EVENT_READ,
                    ("read", stream, target, maximum),
                )
            if process.stdin is not None:
                input_descriptor = process.stdin.fileno()
                os.set_blocking(input_descriptor, False)
                selector.register(
                    input_descriptor,
                    selectors.EVENT_WRITE,
                    ("write",),
                )
            start = self._clock()
            deadline = start + timeout_ns
            while selector.get_map() or process.poll() is None:
                now = self._clock()
                if now >= deadline:
                    timed_out = True
                    break
                timeout = min((deadline - now) / 1_000_000_000, 0.1)
                for key, _events in selector.select(timeout):
                    if key.data[0] == "read":
                        _kind, stream, target, maximum = key.data
                        try:
                            chunk = os.read(
                                key.fd,
                                min(64 * 1024, maximum + 1 - len(target)),
                            )
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
                        continue
                    if input_bundle is None or bundle_view is None:
                        observer_failed = True
                        break
                    try:
                        count = os.write(
                            key.fd,
                            bundle_view[written : written + 64 * 1024],
                        )
                    except BlockingIOError:
                        continue
                    except BrokenPipeError:
                        input_failed = True
                        break
                    if count <= 0:
                        input_failed = True
                        break
                    input_hasher.update(bundle_view[written : written + count])
                    written += count
                    if written == input_bundle.length:
                        selector.unregister(key.fd)
                        if process.stdin is not None:
                            process.stdin.close()
                if terminal is not None or input_failed or observer_failed:
                    break
        except Exception:
            observer_failed = True
        finally:
            try:
                try:
                    selector.close()
                except OSError:
                    observer_failed = True
            finally:
                try:
                    if process.stdin is not None and not process.stdin.closed:
                        try:
                            process.stdin.close()
                        except OSError:
                            observer_failed = True
                finally:
                    if bundle_view is not None:
                        bundle_view.release()

        input_progress: BuildInputTransferProgressV1 | None = None
        if input_bundle is not None:
            try:
                input_progress = _build_input_progress_v1(
                    input_bundle,
                    written,
                    input_hasher.digest(),
                )
            except Exception:
                observer_failed = True

        stop_detail: str | None = None
        if (
            timed_out
            or terminal is not None
            or observer_failed
            or input_failed
        ):
            stop_detail = self._stop_process(process)
        elif process.poll() is None:
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
            elif input_failed:
                trigger = DockerCleanupTriggerV1.INPUT_TRANSFER
            return DockerBuildCleanupFailureV1(
                trigger,
                stop_detail or cleanup_detail or "Docker cleanup failed",
                bytes(stdout),
                bytes(stderr),
                input_progress,
            )
        if input_failed:
            if input_progress is None:
                return DockerBuildObserverFailureV1(
                    "build input progress could not be retained"
                )
            return DockerBuildInputRejectedV1(
                input_progress,
                bytes(stdout),
                bytes(stderr),
            )
        if observer_failed:
            return DockerBuildObserverFailureV1("Docker output observation failed")
        if terminal is not None:
            return DockerBuildOutputLimitV1(
                terminal,
                bytes(stdout),
                bytes(stderr),
                input_progress,
            )
        if timed_out:
            return DockerBuildTimedOutV1(
                bytes(stdout),
                bytes(stderr),
                input_progress,
            )
        if type(process.returncode) is not int:
            return DockerBuildObserverFailureV1("Docker returncode unavailable")
        if input_bundle is not None:
            if (
                input_progress is None
                or written != input_bundle.length
                or input_hasher.digest() != input_bundle.sha256
            ):
                return DockerBuildObserverFailureV1(
                    "completed build input transfer invariant failed"
                )
            input_transfer = _completed_build_input_transfer_v1(
                input_bundle,
                written,
                input_hasher.digest(),
            )
            return _docker_build_exited_v1(
                process.returncode,
                bytes(stdout),
                bytes(stderr),
                input_transfer,
            )
        return _docker_command_exited_v1(
            process.returncode,
            bytes(stdout),
            bytes(stderr),
        )

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
    ) -> _DockerCommandObservationV1:
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
                    type(observation) is not _DockerCommandExitedV1
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
    INPUT_TRANSFER_FAILED = "input_transfer_failed"
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
    input_bundle_identity: bytes
    input_bundle_sha256: bytes
    input_bundle_length: int
    build_processes: tuple[DockerBuildExitedV1, DockerBuildExitedV1]
    comparator: DiagnosticArbComparatorV1
    _binary: bytes
    _rebuild_binaries: tuple[bytes, bytes]
    _input_bundle: SealedBuildInputBundleV1

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
        input_bundle_identity: bytes,
        input_bundle_sha256: bytes,
        input_bundle_length: int,
        build_processes: tuple[DockerBuildExitedV1, DockerBuildExitedV1],
        comparator: DiagnosticArbComparatorV1,
        rebuild_binaries: tuple[bytes, bytes],
        input_bundle: SealedBuildInputBundleV1,
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
            ("input_bundle_identity", input_bundle_identity),
            ("input_bundle_sha256", input_bundle_sha256),
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
        if type(input_bundle_length) is not int or input_bundle_length <= 0:
            raise TypeError("invalid build input bundle length")
        if (
            not sealed_build_input_bundle_is_well_bound_v1(input_bundle)
            or input_bundle.identity != input_bundle_identity
            or input_bundle.sha256 != input_bundle_sha256
            or input_bundle.length != input_bundle_length
            or input_bundle.build_input_identity != build_input_identity
            or input_bundle.source_identity != structural_source_identity
        ):
            raise TypeError("diagnostic build lost its sealed input bundle")
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
            any(
                item.input_transfer.bundle_identity != input_bundle_identity
                or item.input_transfer.expected_length != input_bundle_length
                or item.input_transfer.expected_sha256 != input_bundle_sha256
                or item.input_transfer.written_length != input_bundle_length
                or item.input_transfer.written_sha256 != input_bundle_sha256
                for item in build_processes
            )
        ):
            raise TypeError("builds did not consume the exact sealed input bundle")
        if (
            type(comparator) is not DiagnosticArbComparatorV1
            or comparator.structural_source_identity != structural_source_identity
            or comparator.build_input_identity != build_input_identity
            or comparator.pipeline_policy_identity != pipeline_policy_identity
            or comparator.binary_sha256 != binary_sha256
            or comparator.rebuild_sha256s != rebuild_sha256s
        ):
            raise TypeError("comparator does not bind this diagnostic build")
        if (
            type(rebuild_binaries) is not tuple
            or len(rebuild_binaries) != 2
            or any(type(item) is not bytes for item in rebuild_binaries)
            or rebuild_binaries[0] != rebuild_binaries[1]
            or tuple(hashlib.sha256(item).digest() for item in rebuild_binaries)
            != rebuild_sha256s
        ):
            raise TypeError("invalid owned rebuild binaries")
        for field_name, field_value in (
            ("structural_source_identity", structural_source_identity),
            ("flint_commit_content_identity", flint_commit_content_identity),
            ("flint_commit_content_file_count", flint_commit_content_file_count),
            (
                "flint_project_pinned_release_only_identity",
                flint_project_pinned_release_only_identity,
            ),
            (
                "flint_project_pinned_release_only_file_count",
                flint_project_pinned_release_only_file_count,
            ),
            ("build_input_identity", build_input_identity),
            ("formula_support_identity", formula_support_identity),
            ("pipeline_policy_identity", pipeline_policy_identity),
            (
                "docker_daemon_observation_sha256",
                docker_daemon_observation_sha256,
            ),
            ("oci_image_reference", oci_image_reference),
            ("oci_platform", oci_platform),
            ("binary_sha256", binary_sha256),
            ("rebuild_sha256s", rebuild_sha256s),
            ("host_trust", host_trust),
            ("input_bundle_identity", input_bundle_identity),
            ("input_bundle_sha256", input_bundle_sha256),
            ("input_bundle_length", input_bundle_length),
            ("build_processes", build_processes),
            ("comparator", comparator),
        ):
            object.__setattr__(self, field_name, field_value)
        object.__setattr__(self, "_binary", rebuild_binaries[0])
        object.__setattr__(self, "_rebuild_binaries", rebuild_binaries)
        object.__setattr__(self, "_input_bundle", input_bundle)

    @property
    def binary(self) -> bytes:
        return self._binary

    @property
    def rebuild_binaries(self) -> tuple[bytes, bytes]:
        return self._rebuild_binaries

    @property
    def input_transfers(self) -> tuple[BuildInputTransferV1, BuildInputTransferV1]:
        first = self.build_processes[0].input_transfer
        second = self.build_processes[1].input_transfer
        if type(first) is not BuildInputTransferV1 or type(second) is not BuildInputTransferV1:
            raise RuntimeError("sealed build observation lost its input transfer")
        return first, second

    @property
    def input_bundle(self) -> SealedBuildInputBundleV1:
        return self._input_bundle


BuildResultV1: TypeAlias = (
    DiagnosticBuildObservationV1
    | PipelineBlockedV1
    | BuildRejectedV1
    | NonReproducibleBuildV1
)


class ControlledPipelineV1:
    def __init__(
        self,
        *,
        build_backend: DockerBuildBackendV1,
    ) -> None:
        self._build_backend = build_backend

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

        try:
            input_bundle = _seal_build_input_bundle_v1(request)
        except (
            OSError,
            TypeError,
            ValueError,
            tarfile.TarError,
            BuildSourceAdmissionErrorV1,
            provenance.ProvenanceErrorV1,
        ):
            return BuildRejectedV1(
                1,
                BuildFailureReasonV1.BACKEND_CONTRACT,
            )
        builds: list[tuple[bytes, DockerBuildExitedV1]] = []
        for attempt in (1, 2):
            built = self._build_once(request, attempt, input_bundle)
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
            input_bundle.identity,
            input_bundle.sha256,
            input_bundle.length,
            build_processes,
            comparator,
            (first[0], second[0]),
            input_bundle,
            _token=_BUILD_OBSERVATION_TOKEN,
        )

    def _build_once(
        self,
        request: PipelineRequestV1,
        attempt: int,
        input_bundle: SealedBuildInputBundleV1,
    ) -> tuple[bytes, DockerBuildExitedV1] | BuildRejectedV1:
        if (
            not sealed_build_input_bundle_is_well_bound_v1(input_bundle)
            or input_bundle.source_identity != request.admitted_sources.identity
            or input_bundle.build_input_identity
            != request.build_sources.build_input_identity
        ):
            return BuildRejectedV1(
                attempt,
                BuildFailureReasonV1.BACKEND_CONTRACT,
            )
        try:
            with tempfile.TemporaryDirectory(prefix=f"labcolors-arb-build-v1-{attempt}-") as temporary:
                root = Path(temporary).resolve()
                build_request = DockerBuildRequestV1(
                    attempt,
                    input_bundle,
                    request.execution_limits.max_executable_bytes,
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
                    DockerBuildInputRejectedV1,
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
                if type(process) is DockerBuildInputRejectedV1:
                    return BuildRejectedV1(
                        attempt,
                        BuildFailureReasonV1.INPUT_TRANSFER_FAILED,
                        process,
                    )
                if type(process) is not DockerBuildExitedV1 or process.returncode != 0:
                    return BuildRejectedV1(
                        attempt,
                        BuildFailureReasonV1.PROCESS_FAILED,
                        process,
                    )
                transfer = process.input_transfer
                if (
                    type(transfer) is not BuildInputTransferV1
                    or transfer.bundle_identity != input_bundle.identity
                    or transfer.expected_length != input_bundle.length
                    or transfer.expected_sha256 != input_bundle.sha256
                    or transfer.written_length != input_bundle.length
                    or transfer.written_sha256 != input_bundle.sha256
                ):
                    return BuildRejectedV1(
                        attempt,
                        BuildFailureReasonV1.BACKEND_CONTRACT,
                        process,
                    )
                binary = process.stdout
                try:
                    executor.require_static_x86_64_elf_v1(binary)
                except executor.ExecutionRequestErrorV1:
                    return BuildRejectedV1(
                        attempt,
                        BuildFailureReasonV1.INVALID_OUTPUT,
                        process,
                    )
                return binary, process
        except OSError:
            return BuildRejectedV1(
                attempt,
                BuildFailureReasonV1.BACKEND_CONTRACT,
            )
