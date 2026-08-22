#!/usr/bin/env python3
"""MPFI source-owned BUILD input and transport policy.

Это только BUILD-граница: она не создаёт receipt и не запускает evaluator.
Source-bound controller M2a обязан передать sealed input в общий transport,
а затем independently bind его к двум свежим build observations.
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from enum import StrEnum
from typing import NoReturn

import provenance
from build import input as build_input
from build import transport as build_transport


MPFI_BUILD_IMAGE_REFERENCE_V1 = (
    "silkeh/clang@sha256:f1d693e7af5ee954370e1f3605830d8cabc05f9731226fc99aa5e26127797c11"
)
MPFI_BUILD_PLATFORM_V1 = "linux/amd64"
MPFI_BUILD_OUTPUT_NAME_V1 = "mpfi-evaluator-v1"
MPFI_GENERATED_FORMULA_PATH_V1 = "generated/mpfi-formula.generated.c"
MPFI_FORMULA_SPEC_PATH_V1 = (
    "crates/labcolors-core/contracts/contextual-region-formula-v1.lcir"
)
MPFI_FORMULA_GENERATOR_PATH_V1 = "proof/region/v1/mpfi/evaluator/formula.py"
MPFI_BUILD_RECIPE_PATH_V1 = "proof/region/v1/mpfi/build.sh"
MPFI_BUILD_INNER_RECIPE_PATH_V1 = "proof/region/v1/mpfi/build-inner.sh"

MPFI_GENERATED_FORMULA_SHA256_V1 = (
    "a8df7529261ba68e8fbf591cff283ec88a35cb98958b293bc7885d9fb4dd0fb6"
)
MPFI_FORMULA_SPEC_SHA256_V1 = (
    "a6f77ac462f226453b1c27bbd8637b62780b9a640c317a6f50028dacd1de8540"
)

_MPFI_SOURCE_ID_LABEL_V1 = b"labcolors.proof-region.mpfi-build-sources.v1\0"
_MPFI_SOURCE_REPLAY_ID_LABEL_V1 = b"labcolors.proof-region.mpfi-source-replay.v1\0"
_MPFI_INPUT_ID_LABEL_V1 = b"labcolors.proof-region.mpfi-build-input.v1\0"

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
/bin/sh "$snapshot/workspace/proof/region/v1/mpfi/build.sh"
/usr/bin/cat /build/work/mpfi-evaluator-v1 >&3
"""

MPFI_BUILD_STDOUT_LIMIT_V1 = build_transport.BUILD_STDOUT_LIMIT_V1
MPFI_BUILD_STDERR_LIMIT_V1 = build_transport.BUILD_STDERR_LIMIT_V1
MPFI_BUILD_TIMEOUT_NS_V1 = build_transport.BUILD_TIMEOUT_NS_V1
MPFI_DOCKER_PROBE_OUTPUT_LIMIT_V1 = build_transport.DOCKER_PROBE_OUTPUT_LIMIT_V1
MPFI_DOCKER_PROBE_TIMEOUT_NS_V1 = build_transport.DOCKER_PROBE_TIMEOUT_NS_V1
MPFI_BUILD_TMP_LIMIT_BYTES_V1 = 512 * 1024 * 1024
MPFI_BUILD_STATE_LIMIT_BYTES_V1 = 4 * 1024 * 1024 * 1024
_MPFI_BUILD_TMPFS_SPEC_V1 = (
    f"/tmp:rw,noexec,nosuid,nodev,size={MPFI_BUILD_TMP_LIMIT_BYTES_V1},mode=1777"
)
_MPFI_BUILD_STATE_TMPFS_SPEC_V1 = (
    f"/build:rw,exec,nosuid,nodev,size={MPFI_BUILD_STATE_LIMIT_BYTES_V1},mode=0777"
)

MPFI_BUILD_TRANSPORT_POLICY_V1 = build_transport.DockerBuildPolicyV1(
    MPFI_BUILD_IMAGE_REFERENCE_V1,
    MPFI_BUILD_PLATFORM_V1,
    "labcolors-mpfi-build-v1",
    _BUILD_BOOTSTRAP_V1,
    "labcolors-mpfi-build-bootstrap-v1",
    (_MPFI_BUILD_TMPFS_SPEC_V1, _MPFI_BUILD_STATE_TMPFS_SPEC_V1),
    build_transport.DockerUserModeV1.HOST_EFFECTIVE_IDS,
    MPFI_BUILD_STDOUT_LIMIT_V1,
    MPFI_BUILD_STDERR_LIMIT_V1,
    MPFI_BUILD_TIMEOUT_NS_V1,
    MPFI_DOCKER_PROBE_OUTPUT_LIMIT_V1,
    MPFI_DOCKER_PROBE_TIMEOUT_NS_V1,
)

_PINNED_WORKSPACE_SHA256_V1 = {
    MPFI_BUILD_RECIPE_PATH_V1: "ae7ab236d323d694e0d627b7fcb07f272c351290da10f78f9f7d6cf63b6cf571",
    MPFI_BUILD_INNER_RECIPE_PATH_V1: "95d2cde6649f0bf138a3acfee774a49294f2515f683c62f4234bd75b7a558d60",
    "proof/region/v1/mpfi/operations.py": "61c977e9373788d141ac89dbdc70fba0fb853cb175052975916c21506689eaf3",
    MPFI_FORMULA_GENERATOR_PATH_V1: "961d488a2e9f539518d9a2b7223230495a617cbb50497f3482ad90a5819e4a6a",
    "proof/region/v1/mpfi/evaluator/formula.h": "84794cec2cbc73f73948f4c411c78f6495546879a95bf76a401df8dd24c3b794",
    "proof/region/v1/mpfi/evaluator/hash.c": "9adf78d50c7cbaa25befa4ab745df8f5e0b9de0d8a06cc208bd9cf30f31aa8ce",
    "proof/region/v1/mpfi/evaluator/hash.h": "605a14a0ad221a7e43c5d65c72154793d735d461c53407790dc1dcb78c7f111a",
    "proof/region/v1/mpfi/evaluator/interval.c": "9e146aba9467c0386dd40ada686727039a150afb37f65b8cbafbfa5c1fcfb017",
    "proof/region/v1/mpfi/evaluator/interval.h": "12eb0563f481898bbf6b8add3c7a52e47fd8e6d9ff4d796f2a9bea4f07238e8b",
    "proof/region/v1/mpfi/evaluator/main.c": "7cb30c89fd4b54a1b3b9fbff1b22f7242371b6ef716a176bac8b558181c0205d",
    "proof/region/v1/mpfi/evaluator/region.c": "8a0308f951b9ba681b1d537229ba5fbd1390a1ca863c082b4e24e4fa1a69345f",
    "proof/region/v1/mpfi/evaluator/region.h": "940680c5201393232bec58f4fc45d2db9a3ed3b02d237d8d6e8aa59f6168fa4d",
    "proof/region/v1/mpfi/evaluator/wire.c": "fc9cd817a64b50499f6eb822cfa013bd9557dbf633fb9619e8c30349371643ed",
    "proof/region/v1/mpfi/evaluator/wire.h": "3be98141e9e3ef67b03f5e535e523810fb10e00526fe638bfddbee9d4bbf1710",
    MPFI_FORMULA_SPEC_PATH_V1: MPFI_FORMULA_SPEC_SHA256_V1,
}

REQUIRED_WORKSPACE_MODES_V1 = tuple(
    (path, 0o755 if path == MPFI_BUILD_RECIPE_PATH_V1 else 0o644)
    for path in sorted(_PINNED_WORKSPACE_SHA256_V1)
)


def _identity(label: bytes, chunks: tuple[bytes, ...]) -> bytes:
    payload = b"".join(
        len(chunk).to_bytes(8, "big") + chunk
        for chunk in chunks
    )
    return hashlib.sha256(label + len(payload).to_bytes(8, "big") + payload).digest()


def _valid_digest(value: object) -> bool:
    return type(value) is bytes and len(value) == 32 and value != bytes(32)


class MpfiBuildSourceReasonV1(StrEnum):
    WRONG_TYPE = "wrong_type"
    NONCANONICAL_SET = "noncanonical_set"
    INVALID_PATH = "invalid_path"
    INVALID_MODE = "invalid_mode"
    INVALID_CONTENT = "invalid_content"
    CONTENT_DRIFT = "content_drift"


@dataclass(frozen=True)
class MpfiBuildSourceErrorV1(ValueError):
    reason: MpfiBuildSourceReasonV1
    path: str

    def __str__(self) -> str:
        return f"{self.reason.value}: {self.path}"


def _fail(reason: MpfiBuildSourceReasonV1, path: str) -> NoReturn:
    raise MpfiBuildSourceErrorV1(reason, path)


@dataclass(frozen=True)
class MpfiBuildSourceFileV1:
    path: str
    mode: int
    contents: bytes

    def __post_init__(self) -> None:
        if (
            type(self.path) is not str
            or self.path not in _PINNED_WORKSPACE_SHA256_V1
        ):
            _fail(MpfiBuildSourceReasonV1.INVALID_PATH, str(self.path))
        if type(self.mode) is not int or self.mode not in (0o644, 0o755):
            _fail(MpfiBuildSourceReasonV1.INVALID_MODE, self.path)
        if type(self.contents) is not bytes or not self.contents:
            _fail(MpfiBuildSourceReasonV1.INVALID_CONTENT, self.path)


@dataclass(frozen=True)
class AdmittedMpfiBuildSourcesV1:
    files: tuple[MpfiBuildSourceFileV1, ...]
    identity: bytes

    def __post_init__(self) -> None:
        if (
            type(self.files) is not tuple
            or any(type(item) is not MpfiBuildSourceFileV1 for item in self.files)
            or tuple((item.path, item.mode) for item in self.files)
            != REQUIRED_WORKSPACE_MODES_V1
            or not _valid_digest(self.identity)
        ):
            raise TypeError("invalid admitted MPFI build sources")

    def contents(self, path: str) -> bytes:
        for item in self.files:
            if item.path == path:
                return item.contents
        raise KeyError(path)


def _workspace_identity(files: tuple[MpfiBuildSourceFileV1, ...]) -> bytes:
    chunks: list[bytes] = [len(files).to_bytes(4, "big")]
    for item in files:
        chunks.extend(
            (
                item.path.encode("ascii"),
                item.mode.to_bytes(4, "big"),
                len(item.contents).to_bytes(8, "big"),
                hashlib.sha256(item.contents).digest(),
            )
        )
    return _identity(_MPFI_SOURCE_ID_LABEL_V1, tuple(chunks))


def admit_mpfi_build_sources_v1(
    files: tuple[MpfiBuildSourceFileV1, ...],
) -> AdmittedMpfiBuildSourcesV1:
    if type(files) is not tuple or any(
        type(item) is not MpfiBuildSourceFileV1 for item in files
    ):
        _fail(MpfiBuildSourceReasonV1.WRONG_TYPE, "files")
    try:
        owned = tuple(
            MpfiBuildSourceFileV1(item.path, item.mode, item.contents)
            for item in files
        )
    except MpfiBuildSourceErrorV1:
        raise
    except Exception:
        _fail(MpfiBuildSourceReasonV1.WRONG_TYPE, "files")
    actual = tuple((item.path, item.mode) for item in owned)
    if actual != REQUIRED_WORKSPACE_MODES_V1:
        _fail(MpfiBuildSourceReasonV1.NONCANONICAL_SET, "files")
    for item in owned:
        if hashlib.sha256(item.contents).hexdigest() != _PINNED_WORKSPACE_SHA256_V1[item.path]:
            _fail(MpfiBuildSourceReasonV1.CONTENT_DRIFT, item.path)
    return AdmittedMpfiBuildSourcesV1(owned, _workspace_identity(owned))


def canonical_build_sources_v1(
    value: object,
) -> AdmittedMpfiBuildSourcesV1:
    if type(value) is not AdmittedMpfiBuildSourcesV1:
        raise TypeError("build_sources must be AdmittedMpfiBuildSourcesV1")
    canonical = admit_mpfi_build_sources_v1(value.files)
    if value.identity != canonical.identity:
        raise ValueError("retained MPFI build-source identity drift")
    return canonical


def _source_entries_v1(
    snapshot: provenance.ReplayedSourceClosureV1,
) -> tuple[tuple[str, int, bytes], ...]:
    entries = tuple(
        (
            f"inputs/sources/{lock.role.name.lower()}/{relative}",
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
        raise ValueError("MPFI source closure is empty")
    return tuple(sorted(entries))


def source_identity_v1(
    snapshot: provenance.ReplayedSourceClosureV1,
) -> bytes:
    chunks: list[bytes] = [
        snapshot.source_lock.identity,
        snapshot.admitted_sources.identity,
    ]
    for path, mode, contents in _source_entries_v1(snapshot):
        chunks.extend(
            (
                path.encode("ascii"),
                mode.to_bytes(4, "big"),
                len(contents).to_bytes(8, "big"),
                hashlib.sha256(contents).digest(),
            )
        )
    return _identity(_MPFI_SOURCE_REPLAY_ID_LABEL_V1, tuple(chunks))


def _canonical_input_entries_from_owned_v1(
    snapshot: provenance.ReplayedSourceClosureV1,
    build_sources: AdmittedMpfiBuildSourcesV1,
    generated_formula: bytes,
) -> tuple[tuple[str, int, bytes], ...]:
    source_entries = _source_entries_v1(snapshot)
    workspace_entries = tuple(
        (f"workspace/{item.path}", item.mode, item.contents)
        for item in build_sources.files
    )
    return tuple(
        sorted(
            source_entries
            + (("inputs/formula.generated.c", 0o644, generated_formula),)
            + workspace_entries
        )
    )


def canonical_input_entries_v1(
    snapshot: provenance.ReplayedSourceClosureV1,
    build_sources: AdmittedMpfiBuildSourcesV1,
    generated_formula: bytes,
) -> tuple[tuple[str, int, bytes], ...]:
    """Return the canonical source, formula and workspace file set."""

    if type(snapshot) is not provenance.ReplayedSourceClosureV1:
        raise TypeError("snapshot must be ReplayedSourceClosureV1")
    if (
        type(snapshot.source_lock) is not provenance.MpfiSourceLockV1
        or type(snapshot.admitted_sources) is not provenance.AdmittedMpfiSourcesV1
        or snapshot.admitted_sources.source_lock_identity
        != snapshot.source_lock.identity
    ):
        raise TypeError("snapshot must retain MPFI source lock")
    canonical = canonical_build_sources_v1(build_sources)
    if type(generated_formula) is not bytes or not generated_formula:
        raise TypeError("generated_formula must be nonempty bytes")
    if hashlib.sha256(generated_formula).hexdigest() != MPFI_GENERATED_FORMULA_SHA256_V1:
        raise ValueError("generated MPFI formula drift")
    return _canonical_input_entries_from_owned_v1(
        snapshot,
        canonical,
        generated_formula,
    )


def _input_binding_identity_v1(
    source_identity: bytes,
    build_sources: AdmittedMpfiBuildSourcesV1,
    generated_formula: bytes,
    policy: build_transport.DockerBuildPolicyV1,
    contents: bytes,
) -> bytes:
    return _identity(
        _MPFI_INPUT_ID_LABEL_V1,
        (
            source_identity,
            build_sources.identity,
            hashlib.sha256(generated_formula).digest(),
            build_transport.transport_policy_identity_v1(policy),
            len(contents).to_bytes(8, "big"),
            hashlib.sha256(contents).digest(),
        ),
    )


def seal_mpfi_build_input_v1(
    source_lock: provenance.MpfiSourceLockV1,
    admitted_sources: provenance.AdmittedMpfiSourcesV1,
    build_sources: AdmittedMpfiBuildSourcesV1,
    generated_formula: bytes,
    limits: build_input.CanonicalInputLimitsV1,
    policy: build_transport.DockerBuildPolicyV1 = MPFI_BUILD_TRANSPORT_POLICY_V1,
) -> build_input.SealedInputV1:
    if type(source_lock) is not provenance.MpfiSourceLockV1:
        raise TypeError("source_lock must be MpfiSourceLockV1")
    if type(admitted_sources) is not provenance.AdmittedMpfiSourcesV1:
        raise TypeError("admitted_sources must be AdmittedMpfiSourcesV1")
    if type(limits) is not build_input.CanonicalInputLimitsV1:
        raise TypeError("limits must be CanonicalInputLimitsV1")
    build_sources = canonical_build_sources_v1(build_sources)
    if type(generated_formula) is not bytes or not generated_formula:
        raise TypeError("generated_formula must be nonempty bytes")
    if hashlib.sha256(generated_formula).hexdigest() != MPFI_GENERATED_FORMULA_SHA256_V1:
        raise ValueError("generated MPFI formula drift")
    if not build_transport.docker_policy_is_valid_v1(policy):
        raise TypeError("policy must be canonical DockerBuildPolicyV1")
    snapshot = provenance.replay_admitted_source_closure_v1(
        source_lock,
        admitted_sources,
    )
    return seal_mpfi_build_input_from_snapshot_v1(
        snapshot,
        build_sources,
        generated_formula,
        limits,
        policy,
    )


def seal_mpfi_build_input_from_snapshot_v1(
    snapshot: provenance.ReplayedSourceClosureV1,
    build_sources: AdmittedMpfiBuildSourcesV1,
    generated_formula: bytes,
    limits: build_input.CanonicalInputLimitsV1,
    policy: build_transport.DockerBuildPolicyV1 = MPFI_BUILD_TRANSPORT_POLICY_V1,
) -> build_input.SealedInputV1:
    """Seal from one already-owned source replay without a second materialization."""

    if type(snapshot) is not provenance.ReplayedSourceClosureV1:
        raise TypeError("snapshot must be ReplayedSourceClosureV1")
    if (
        type(snapshot.source_lock) is not provenance.MpfiSourceLockV1
        or type(snapshot.admitted_sources)
        is not provenance.AdmittedMpfiSourcesV1
        or snapshot.admitted_sources.source_lock_identity
        != snapshot.source_lock.identity
    ):
        raise TypeError("snapshot must retain MPFI source lock")
    build_sources = canonical_build_sources_v1(build_sources)
    if type(generated_formula) is not bytes or not generated_formula:
        raise TypeError("generated_formula must be nonempty bytes")
    if hashlib.sha256(generated_formula).hexdigest() != MPFI_GENERATED_FORMULA_SHA256_V1:
        raise ValueError("generated MPFI formula drift")
    if not build_transport.docker_policy_is_valid_v1(policy):
        raise TypeError("policy must be canonical DockerBuildPolicyV1")
    if type(limits) is not build_input.CanonicalInputLimitsV1:
        raise TypeError("limits must be CanonicalInputLimitsV1")
    entries = _canonical_input_entries_from_owned_v1(
        snapshot,
        build_sources,
        generated_formula,
    )
    canonical = build_input.canonical_ustar_v1(entries, limits)
    return build_input.seal_input_v1(
        _input_binding_identity_v1(
            source_identity_v1(snapshot),
            build_sources,
            generated_formula,
            policy,
            canonical,
        ),
        canonical,
    )


def mpfi_build_input_is_bound_v1(
    source_lock: object,
    admitted_sources: object,
    build_sources: object,
    generated_formula: object,
    limits: object,
    value: object,
    policy: object = MPFI_BUILD_TRANSPORT_POLICY_V1,
) -> bool:
    if (
        type(value) is not build_input.SealedInputV1
        or not build_input.sealed_input_is_intact_v1(value)
        or type(source_lock) is not provenance.MpfiSourceLockV1
        or type(admitted_sources) is not provenance.AdmittedMpfiSourcesV1
        or type(build_sources) is not AdmittedMpfiBuildSourcesV1
        or type(generated_formula) is not bytes
        or type(limits) is not build_input.CanonicalInputLimitsV1
        or type(policy) is not build_transport.DockerBuildPolicyV1
    ):
        return False
    try:
        snapshot = provenance.replay_admitted_source_closure_v1(
            source_lock,
            admitted_sources,
        )
        return mpfi_build_input_is_bound_from_snapshot_v1(
            snapshot,
            build_sources,
            generated_formula,
            limits,
            value,
            policy,
        )
    except Exception:
        return False


def mpfi_build_input_is_bound_from_snapshot_v1(
    snapshot: object,
    build_sources: object,
    generated_formula: object,
    limits: object,
    value: object,
    policy: object = MPFI_BUILD_TRANSPORT_POLICY_V1,
) -> bool:
    if (
        type(snapshot) is not provenance.ReplayedSourceClosureV1
        or type(value) is not build_input.SealedInputV1
        or not build_input.sealed_input_is_intact_v1(value)
        or type(build_sources) is not AdmittedMpfiBuildSourcesV1
        or type(generated_formula) is not bytes
        or type(limits) is not build_input.CanonicalInputLimitsV1
        or type(policy) is not build_transport.DockerBuildPolicyV1
    ):
        return False
    try:
        canonical_build_sources = canonical_build_sources_v1(build_sources)
        expected = seal_mpfi_build_input_from_snapshot_v1(
            snapshot,
            canonical_build_sources,
            generated_formula,
            limits,
            policy,
        )
    except Exception:
        return False
    return (
        value.binding_identity == expected.binding_identity
        and value.contents == expected.contents
    )
