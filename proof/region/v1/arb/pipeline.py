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
from dataclasses import dataclass, fields
from enum import StrEnum
from functools import cached_property
from typing import NoReturn, TypeAlias

from build import input as build_input
from build import transport as build_transport

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

# The generic transport owns universal observer ceilings.  This lane binds to
# those coordinates rather than recreating a coincident copy of the policy.
BUILD_STDOUT_LIMIT_V1 = build_transport.BUILD_STDOUT_LIMIT_V1
BUILD_STDERR_LIMIT_V1 = build_transport.BUILD_STDERR_LIMIT_V1
BUILD_TIMEOUT_NS_V1 = build_transport.BUILD_TIMEOUT_NS_V1
DOCKER_PROBE_OUTPUT_LIMIT_V1 = build_transport.DOCKER_PROBE_OUTPUT_LIMIT_V1
DOCKER_PROBE_TIMEOUT_NS_V1 = build_transport.DOCKER_PROBE_TIMEOUT_NS_V1
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
_PIPELINE_POLICY_ID_LABEL_V2 = b"labcolors.proof-region.arb-pipeline-policy.v2\0"
_BUILD_INPUT_BUNDLE_ID_LABEL_V2 = (
    b"labcolors.proof-region.arb-build-input-bundle.v2\0"
)
_BUILD_SOURCES_TOKEN = object()
_COMPARATOR_TOKEN = object()
_BUILD_OBSERVATION_TOKEN = object()


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


ARB_BUILD_TRANSPORT_POLICY_V1 = build_transport.DockerBuildPolicyV1(
    OCI_IMAGE_REFERENCE_V1,
    OCI_PLATFORM_V1,
    "labcolors-arb-build-v1",
    _BUILD_BOOTSTRAP_V1,
    "labcolors-arb-build-bootstrap-v1",
    (_BUILD_TMPFS_SPEC_V1, _BUILD_STATE_TMPFS_SPEC_V1),
    build_transport.DockerUserModeV1.HOST_EFFECTIVE_IDS,
    BUILD_STDOUT_LIMIT_V1,
    BUILD_STDERR_LIMIT_V1,
    BUILD_TIMEOUT_NS_V1,
    DOCKER_PROBE_OUTPUT_LIMIT_V1,
    DOCKER_PROBE_TIMEOUT_NS_V1,
)


def _arb_input_binding_identity_v2(
    source_identity: bytes,
    build_input_identity: bytes,
    contents: bytes,
    exact_policy: build_transport.DockerBuildPolicyV1,
) -> bytes:
    if (
        not _valid_digest(source_identity)
        or not _valid_digest(build_input_identity)
        or type(contents) is not bytes
        or not contents
        or not build_transport.docker_policy_is_valid_v1(exact_policy)
    ):
        raise TypeError("invalid Arb build input binding coordinates")
    digest = hashlib.sha256(contents).digest()
    return _identity(
        _BUILD_INPUT_BUNDLE_ID_LABEL_V2,
        (
            source_identity,
            build_input_identity,
            len(contents).to_bytes(8, "big"),
            digest,
            # This inner identity fixes only the stream-to-tree program.  V1
            # never reads shell $0; argv0 instead remains in the outer
            # transport identity.  A bootstrap that consumes $0 needs a new
            # binding schema rather than silently widening this preimage.
            hashlib.sha256(
                exact_policy.bootstrap.encode("utf-8")
            ).digest(),
        ),
    )


def arb_input_is_bound_v1(
    request: object,
    exact_policy: object,
    value: object,
) -> bool:
    """Recompute Arb semantics independently of generic byte integrity."""

    if (
        type(request) is not PipelineRequestV1
        or type(value) is not build_input.SealedInputV1
        or not build_input.sealed_input_is_intact_v1(value)
    ):
        return False
    try:
        return value.binding_identity == _arb_input_binding_identity_v2(
            request.admitted_sources.identity,
            request.build_sources.build_input_identity,
            value.contents,
            exact_policy,
        )
    except Exception:
        return False


def _seal_build_input_bundle_v1(
    request: "PipelineRequestV1",
    exact_policy: build_transport.DockerBuildPolicyV1,
) -> build_input.SealedInputV1:
    if type(request) is not PipelineRequestV1:
        raise TypeError("request must be PipelineRequestV1")
    if not build_transport.docker_policy_is_valid_v1(exact_policy):
        raise TypeError("exact_policy must be canonical DockerBuildPolicyV1")
    source_entries = tuple(
        (
            f"inputs/{lock.root_prefix[:-1]}/{relative}",
            mode,
            contents,
        )
        for lock, admitted in zip(
            request.source_lock.sources,
            request.admitted_sources.sources,
            strict=True,
        )
        for relative, mode, contents in provenance.materialize_admitted_source_files_v1(
            lock,
            admitted,
        )
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
    contents = build_input.canonical_ustar_v1(
        tuple(sorted(source_entries + workspace_entries)),
        build_input.CanonicalInputLimitsV1(
            len(source_entries) + len(workspace_entries)
            + sum(
                path.count("/")
                for path, _mode, _contents in source_entries + workspace_entries
            ),
            max(
                MAX_BUILD_SOURCE_FILE_BYTES_V1,
                *(
                    lock.regular_file_bytes
                    for lock in request.source_lock.sources
                ),
            ),
            MAX_BUILD_SOURCE_TOTAL_BYTES_V1
            + sum(lock.regular_file_bytes for lock in request.source_lock.sources),
        ),
    )
    return build_input.seal_input_v1(
        _arb_input_binding_identity_v2(
            request.admitted_sources.identity,
            request.build_sources.build_input_identity,
            contents,
            exact_policy,
        ),
        contents,
    )


class HostTrustBoundaryV1(StrEnum):
    UNSEALED_LINUX_X64_DOCKER_HOST = "unsealed-linux-x64-docker-host"


def pipeline_policy_identity_v2(
    host_trust: HostTrustBoundaryV1,
    exact_policy: build_transport.DockerBuildPolicyV1,
) -> bytes:
    if type(host_trust) is not HostTrustBoundaryV1:
        raise TypeError("host_trust must be HostTrustBoundaryV1")
    if not build_transport.docker_policy_is_valid_v1(exact_policy):
        raise TypeError("exact_policy must be canonical DockerBuildPolicyV1")
    return _identity(
        _PIPELINE_POLICY_ID_LABEL_V2,
        (
            build_transport.transport_policy_identity_v1(exact_policy),
            build_transport.native_command_contract_identity_v1(),
            host_trust.value.encode("ascii"),
            b"build-observation=diagnostic-unsealed-v1",
            b"inputs=one-controller-sealed-normalized-tree-ustar",
            b"container-admission=exact-length-and-sha256-before-extraction",
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


def _comparator_preimage_v2(label: bytes, chunks: tuple[bytes, ...]) -> bytes:
    """Encode one V2 comparator preimage without accepting a V1 label."""

    if (
        type(label) is not bytes
        or not label.startswith(b"labcolors.proof-region.arb-comparator.")
        or not label.endswith(b".v2\0")
        or type(chunks) is not tuple
        or not chunks
        or any(type(chunk) is not bytes for chunk in chunks)
    ):
        raise TypeError("invalid V2 comparator preimage coordinates")
    return label + b"\x02" + len(chunks).to_bytes(4, "big") + b"".join(
        _blob(chunk) for chunk in chunks
    )


def comparator_build_preimage_v2(
    build_sources: AdmittedBuildSourcesV1,
    docker_capability_identity: bytes,
    pipeline_policy_identity: bytes,
    build_processes: tuple[
        build_transport.DockerBuildExitedV1,
        build_transport.DockerBuildExitedV1,
    ],
    binary_sha256: bytes,
    rebuild_sha256s: tuple[bytes, bytes],
    binary_length: int,
) -> bytes:
    """Single replay schema for the BUILD coordinate in the comparator."""

    if type(build_sources) is not AdmittedBuildSourcesV1:
        raise TypeError("build_sources must be AdmittedBuildSourcesV1")
    for name, value in (
        ("docker_capability_identity", docker_capability_identity),
        ("pipeline_policy_identity", pipeline_policy_identity),
        ("binary_sha256", binary_sha256),
    ):
        if not _valid_digest(value):
            raise TypeError(f"invalid {name}")
    if (
        type(build_processes) is not tuple
        or len(build_processes) != 2
        or any(
            type(item) is not build_transport.DockerBuildExitedV1
            for item in build_processes
        )
        or type(rebuild_sha256s) is not tuple
        or rebuild_sha256s != (binary_sha256, binary_sha256)
        or type(binary_length) is not int
        or binary_length <= 0
    ):
        raise TypeError("invalid comparator BUILD observation")
    process_bytes = tuple(
        build_transport.build_process_bytes_v1(item) for item in build_processes
    )
    return _comparator_preimage_v2(
        b"labcolors.proof-region.arb-comparator.build-identity.v2\0",
        (
            build_sources.contents(BUILD_RECIPE_PATH_V1),
            build_sources.build_input_identity,
            build_sources.formula_support_identity,
            docker_capability_identity,
            pipeline_policy_identity,
            b"build-observation=diagnostic-unsealed-v1",
            len(build_processes).to_bytes(4, "big"),
            *process_bytes,
            binary_sha256,
            rebuild_sha256s[0],
            rebuild_sha256s[1],
            binary_length.to_bytes(8, "big"),
            binary_sha256,
        ),
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


def _derive_arb_comparator_for_build_v1(
    request: PipelineRequestV1,
    docker_capability: build_transport.DockerSupportedV1,
    binary: bytes,
    rebuild_sha256s: tuple[bytes, bytes],
    build_processes: tuple[
        build_transport.DockerBuildExitedV1,
        build_transport.DockerBuildExitedV1,
    ],
) -> DiagnosticArbComparatorV1:
    """Derive all ten coordinates without accepting a caller digest/resolver."""

    if type(request) is not PipelineRequestV1:
        raise TypeError("request must be PipelineRequestV1")
    if type(docker_capability) is not build_transport.DockerSupportedV1:
        raise TypeError("docker_capability must be DockerSupportedV1")
    if type(binary) is not bytes or not binary:
        raise TypeError("binary must be exact nonempty bytes")
    binary_sha256 = hashlib.sha256(binary).digest()
    if (
        type(build_processes) is not tuple
        or len(build_processes) != 2
        or any(
            type(item) is not build_transport.DockerBuildExitedV1
            for item in build_processes
        )
        or any(item.returncode != 0 for item in build_processes)
        or rebuild_sha256s != (binary_sha256, binary_sha256)
    ):
        raise TypeError("comparator derivation requires two equal successful builds")
    docker_capability_identity = build_transport.docker_capability_identity_v1(
        docker_capability
    )
    pipeline_policy_identity = pipeline_policy_identity_v2(
        request.host_trust,
        docker_capability.policy,
    )
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

    process_bytes = tuple(
        build_transport.build_process_bytes_v1(item) for item in build_processes
    )
    build_identity = comparator_build_preimage_v2(
        request.build_sources,
        docker_capability_identity,
        pipeline_policy_identity,
        build_processes,
        binary_sha256,
        rebuild_sha256s,
        len(binary),
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


@dataclass(frozen=True)
class PipelineBlockedV1:
    reason: build_transport.DockerBlockerReasonV1
    detail: str


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
    docker_capability: build_transport.DockerSupportedV1
    binary_sha256: bytes
    rebuild_sha256s: tuple[bytes, bytes]
    host_trust: HostTrustBoundaryV1
    input_bundle_identity: bytes
    input_bundle_sha256: bytes
    input_bundle_length: int
    build_processes: tuple[
        build_transport.DockerBuildExitedV1,
        build_transport.DockerBuildExitedV1,
    ]
    comparator: DiagnosticArbComparatorV1
    _binary: bytes
    _rebuild_binaries: tuple[bytes, bytes]
    _input_bundle: build_input.SealedInputV1

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
        docker_capability: build_transport.DockerSupportedV1,
        binary_sha256: bytes,
        rebuild_sha256s: tuple[bytes, bytes],
        host_trust: HostTrustBoundaryV1,
        input_bundle_identity: bytes,
        input_bundle_sha256: bytes,
        input_bundle_length: int,
        build_processes: tuple[
            build_transport.DockerBuildExitedV1,
            build_transport.DockerBuildExitedV1,
        ],
        comparator: DiagnosticArbComparatorV1,
        rebuild_binaries: tuple[bytes, bytes],
        input_bundle: build_input.SealedInputV1,
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
        if type(docker_capability) is not build_transport.DockerSupportedV1:
            raise TypeError("diagnostic build requires DockerSupportedV1")
        canonical_capability = build_transport.DockerSupportedV1(
            *tuple(docker_capability)
        )
        if tuple(canonical_capability) != tuple(docker_capability):
            raise TypeError("diagnostic build does not bind the exact Arb capability")
        if (
            type(rebuild_sha256s) is not tuple
            or len(rebuild_sha256s) != 2
            or any(not _valid_digest(item) for item in rebuild_sha256s)
            or rebuild_sha256s != (binary_sha256, binary_sha256)
        ):
            raise TypeError("invalid observed two-build digests")
        if type(host_trust) is not HostTrustBoundaryV1:
            raise TypeError("invalid host trust boundary")
        if type(input_bundle_length) is not int or input_bundle_length <= 0:
            raise TypeError("invalid build input bundle length")
        if (
            not build_input.sealed_input_is_intact_v1(input_bundle)
            or input_bundle.binding_identity != input_bundle_identity
            or input_bundle.sha256 != input_bundle_sha256
            or input_bundle.length != input_bundle_length
            or input_bundle.binding_identity
            != _arb_input_binding_identity_v2(
                structural_source_identity,
                build_input_identity,
                input_bundle.contents,
                canonical_capability.policy,
            )
        ):
            raise TypeError("diagnostic build lost its sealed input bundle")
        if pipeline_policy_identity != pipeline_policy_identity_v2(
            host_trust,
            canonical_capability.policy,
        ):
            raise TypeError("pipeline policy is not the fixed diagnostic policy")
        if (
            type(build_processes) is not tuple
            or len(build_processes) != 2
            or any(
                type(item) is not build_transport.DockerBuildExitedV1
                for item in build_processes
            )
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
            ("docker_capability", docker_capability),
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
    def input_transfers(
        self,
    ) -> tuple[
        build_transport.BuildInputTransferV1,
        build_transport.BuildInputTransferV1,
    ]:
        first = self.build_processes[0].input_transfer
        second = self.build_processes[1].input_transfer
        if (
            type(first) is not build_transport.BuildInputTransferV1
            or type(second) is not build_transport.BuildInputTransferV1
        ):
            raise RuntimeError("sealed build observation lost its input transfer")
        return first, second

    @property
    def input_bundle(self) -> build_input.SealedInputV1:
        if (
            not build_input.sealed_input_is_intact_v1(self._input_bundle)
            or self._input_bundle.binding_identity != self.input_bundle_identity
            or self._input_bundle.sha256 != self.input_bundle_sha256
            or self._input_bundle.length != self.input_bundle_length
        ):
            raise RuntimeError("diagnostic build lost its exact input bytes")
        return self._input_bundle


BuildResultV1: TypeAlias = (
    DiagnosticBuildObservationV1
    | PipelineBlockedV1
    | build_transport.BuildRejectedV1
    | build_transport.TwoBuildObservationV1
)


class ControlledPipelineV1:
    def __init__(
        self,
        *,
        build_backend: build_transport.DockerBuildBackendV1,
    ) -> None:
        self._transport = build_transport.ControlledBuildTransportV1(
            policy=ARB_BUILD_TRANSPORT_POLICY_V1,
            backend=build_backend,
        )

    @staticmethod
    def _admit_arb_output_v1(binary: bytes) -> bool:
        try:
            executor.require_static_x86_64_elf_v1(binary)
        except executor.ExecutionRequestErrorV1:
            return False
        return True

    def build(self, request: PipelineRequestV1) -> BuildResultV1:
        """Observe two fresh equal builds without requiring a RUN capability."""

        if type(request) is not PipelineRequestV1:
            raise PipelineInputErrorV1(PipelineInputReasonV1.WRONG_TYPE, "request")
        probe_result = self._transport.probe()
        if type(probe_result) is build_transport.DockerUnsupportedV1:
            return PipelineBlockedV1(probe_result.reason, probe_result.detail)
        if type(probe_result) is not build_transport.DockerSupportedV1:
            return PipelineBlockedV1(
                build_transport.DockerBlockerReasonV1.BACKEND_CONTRACT,
                "Docker capability report is not typed",
            )
        docker_capability = probe_result
        try:
            input_bundle = _seal_build_input_bundle_v1(
                request,
                docker_capability.policy,
            )
        except (
            OSError,
            TypeError,
            ValueError,
            BuildSourceAdmissionErrorV1,
            provenance.ProvenanceErrorV1,
            build_input.InputErrorV1,
        ):
            return build_transport.BuildRejectedV1(
                1,
                build_transport.BuildFailureReasonV1.CONTRACT_VIOLATION,
            )
        built = self._transport.build(
            docker_capability,
            input_bundle,
            request.execution_limits.max_executable_bytes,
            input_admission=lambda value: arb_input_is_bound_v1(
                request,
                docker_capability.policy,
                value,
            ),
            output_admission=self._admit_arb_output_v1,
        )
        if type(built) is build_transport.BuildRejectedV1:
            return built
        if type(built) is not build_transport.TwoBuildObservationV1:
            return build_transport.BuildRejectedV1(
                1,
                build_transport.BuildFailureReasonV1.CONTRACT_VIOLATION,
            )
        if not build_transport.two_build_observation_matches_v1(
            built,
            built.session,
        ):
            return build_transport.BuildRejectedV1(
                1,
                build_transport.BuildFailureReasonV1.CONTRACT_VIOLATION,
            )
        if built.relation is build_transport.BuildByteRelationV1.DIFFERENT:
            return built
        if built.relation is not build_transport.BuildByteRelationV1.IDENTICAL:
            return build_transport.BuildRejectedV1(
                1,
                build_transport.BuildFailureReasonV1.CONTRACT_VIOLATION,
            )

        binary = built.outputs[0]
        rebuild_sha256s = tuple(
            hashlib.sha256(item).digest() for item in built.outputs
        )
        build_processes = built.processes
        comparator = _derive_arb_comparator_for_build_v1(
            request,
            docker_capability,
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
            pipeline_policy_identity_v2(
                request.host_trust,
                docker_capability.policy,
            ),
            docker_capability,
            rebuild_sha256s[0],
            rebuild_sha256s,
            request.host_trust,
            input_bundle.binding_identity,
            input_bundle.sha256,
            input_bundle.length,
            build_processes,
            comparator,
            built.outputs,
            input_bundle,
            _token=_BUILD_OBSERVATION_TOKEN,
        )
