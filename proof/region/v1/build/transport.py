#!/usr/bin/env python3
"""Engine-neutral, causally observed Docker BUILD transport."""

from __future__ import annotations

import hashlib
import json
import os
import platform
import re
import selectors
import signal
import stat
import subprocess
import tempfile
import time
from enum import StrEnum
from pathlib import Path
from typing import Callable, Protocol, TypeAlias

from . import input


# These ceilings preserve the already shipped Arb V1 observer contract; they
# are not physical constants or evidence that every build fits. Changing one
# requires a new transport version, a streaming/resource design review, and a
# targeted native high-water gate. A lane policy may only tighten them.
BUILD_STDOUT_LIMIT_V1 = 16 * 1024 * 1024
BUILD_STDERR_LIMIT_V1 = 16 * 1024 * 1024
BUILD_TIMEOUT_NS_V1 = 2 * 60 * 60 * 1_000_000_000
DOCKER_PROBE_OUTPUT_LIMIT_V1 = 1024 * 1024
DOCKER_PROBE_TIMEOUT_NS_V1 = 30 * 1_000_000_000

# These are observer scheduling/termination mechanics retained from Arb V1,
# not successful-build evidence coordinates. CPU, RAM and PID containment is
# owned by the declared disposable worker, outside this Docker transport.
_IO_CHUNK_BYTES_V1 = 64 * 1024
_POLL_SLICE_SECONDS_V1 = 0.1
_PROCESS_STOP_TIMEOUT_SECONDS_V1 = 30
_PATH_TYPE = type(Path("/"))

_BUILD_INPUT_PROGRESS_TOKEN = object()
_BUILD_INPUT_TRANSFER_TOKEN = object()
_DOCKER_COMMAND_EXITED_TOKEN = object()
_DOCKER_BUILD_EXITED_TOKEN = object()
_BUILD_CLEANUP_FAILURE_TOKEN = object()
_BUILD_SESSION_TOKEN = object()
_TWO_BUILD_OBSERVATION_TOKEN = object()


def _valid_digest(value: object) -> bool:
    return type(value) is bytes and len(value) == 32 and value != bytes(32)


def _blob(value: bytes) -> bytes:
    return len(value).to_bytes(8, "big") + value


def _identity(label: bytes, chunks: tuple[bytes, ...]) -> bytes:
    payload = b"".join(_blob(chunk) for chunk in chunks)
    return hashlib.sha256(label + len(payload).to_bytes(8, "big") + payload).digest()


def _pinned_image_reference(value: object) -> bool:
    if type(value) is not str or value.count("@sha256:") != 1:
        return False
    repository, digest = value.split("@sha256:", 1)
    components = repository.split("/")
    if any(not component for component in components):
        return False
    first, *path_components = components
    if ":" in first:
        domain, separator, port = first.rpartition(":")
        if (
            not separator
            or not domain
            or not port
            or any(character not in "0123456789" for character in port)
        ):
            return False
        first = domain
    repository_component = re.compile(
        r"[a-z0-9]+(?:[._-]+[a-z0-9]+)*\Z"
    )
    return (
        bool(repository)
        and repository[0].isalnum()
        and repository[-1].isalnum()
        and repository == repository.lower()
        and repository_component.fullmatch(first) is not None
        and all(
            repository_component.fullmatch(component) is not None
            for component in path_components
        )
        and len(digest) == 64
        and all(character in "0123456789abcdef" for character in digest)
    )


def _encoded_policy_text(
    value: object,
    maximum: int,
    field_name: str,
    *,
    allow_newlines: bool = False,
) -> str:
    if type(value) is not str or not value or "\0" in value:
        raise TypeError(f"invalid {field_name}")
    try:
        encoded = value.encode("utf-8")
    except UnicodeEncodeError as error:
        raise TypeError(f"invalid {field_name}") from error
    if (
        len(encoded) > maximum
        or not allow_newlines and ("\n" in value or "\r" in value)
    ):
        raise TypeError(f"invalid {field_name}")
    return value


class DockerUserModeV1(StrEnum):
    """Declare which unsealed-host user coordinates Docker observes."""

    HOST_EFFECTIVE_IDS = "host_effective_ids"


class DockerBuildPolicyV1(tuple):
    """Deeply immutable coordinates for one bounded Docker build transport."""

    __slots__ = ()

    def __new__(
        cls,
        image_reference: str,
        platform: str,
        hostname: str,
        container_name_prefix: str,
        bootstrap: str,
        bootstrap_argv0: str,
        tmpfs_specs: tuple[str, ...],
        user_mode: DockerUserModeV1,
        stdout_limit: int,
        stderr_limit: int,
        build_timeout_ns: int,
        probe_output_limit: int,
        probe_timeout_ns: int,
    ) -> DockerBuildPolicyV1:
        strings = tuple(
            _encoded_policy_text(
                value,
                maximum,
                field_name,
                allow_newlines=field_name == "bootstrap",
            )
            for field_name, value, maximum in (
                ("image_reference", image_reference, 512),
                ("platform", platform, 64),
                ("hostname", hostname, 64),
                ("container_name_prefix", container_name_prefix, 64),
                ("bootstrap", bootstrap, 64 * 1024),
                ("bootstrap_argv0", bootstrap_argv0, 128),
            )
        )
        (
            image_reference,
            platform,
            hostname,
            container_name_prefix,
            bootstrap,
            bootstrap_argv0,
        ) = strings
        if (
            platform != "linux/amd64"
            or not _pinned_image_reference(image_reference)
        ):
            raise TypeError("policy requires one pinned linux/amd64 image")
        if (
            any(
                character not in "abcdefghijklmnopqrstuvwxyz0123456789-"
                for character in hostname
            )
            or any(
                character not in "abcdefghijklmnopqrstuvwxyz0123456789-"
                for character in container_name_prefix
            )
            or not container_name_prefix.endswith("-")
        ):
            raise TypeError("invalid Docker names")
        if type(tmpfs_specs) is not tuple or not tmpfs_specs:
            raise TypeError("invalid tmpfs_specs")
        owned_tmpfs: list[str] = []
        for spec in tmpfs_specs:
            parsed = _encoded_policy_text(spec, 4096, "tmpfs_specs")
            if not parsed.startswith("/"):
                raise TypeError("invalid tmpfs_specs")
            owned_tmpfs.append(parsed)
        tmpfs_specs = tuple(owned_tmpfs)
        if len(set(tmpfs_specs)) != len(tmpfs_specs):
            raise TypeError("invalid tmpfs_specs")
        if type(user_mode) is not DockerUserModeV1:
            raise TypeError("invalid Docker user mode")
        limits = (
            (stdout_limit, BUILD_STDOUT_LIMIT_V1, "stdout_limit"),
            (stderr_limit, BUILD_STDERR_LIMIT_V1, "stderr_limit"),
            (build_timeout_ns, BUILD_TIMEOUT_NS_V1, "build_timeout_ns"),
            (probe_output_limit, DOCKER_PROBE_OUTPUT_LIMIT_V1, "probe_output_limit"),
            (probe_timeout_ns, DOCKER_PROBE_TIMEOUT_NS_V1, "probe_timeout_ns"),
        )
        if any(
            type(value) is not int or value <= 0 or value > maximum
            for value, maximum, _name in limits
        ):
            raise TypeError("invalid Docker policy limit")
        return tuple.__new__(
            cls,
            (
                image_reference,
                platform,
                hostname,
                container_name_prefix,
                bootstrap,
                bootstrap_argv0,
                tmpfs_specs,
                user_mode,
                stdout_limit,
                stderr_limit,
                build_timeout_ns,
                probe_output_limit,
                probe_timeout_ns,
            ),
        )

    @property
    def image_reference(self) -> str:
        return self[0]

    @property
    def platform(self) -> str:
        return self[1]

    @property
    def hostname(self) -> str:
        return self[2]

    @property
    def container_name_prefix(self) -> str:
        return self[3]

    @property
    def bootstrap(self) -> str:
        return self[4]

    @property
    def bootstrap_argv0(self) -> str:
        return self[5]

    @property
    def tmpfs_specs(self) -> tuple[str, ...]:
        return self[6]

    @property
    def user_mode(self) -> DockerUserModeV1:
        return self[7]

    @property
    def stdout_limit(self) -> int:
        return self[8]

    @property
    def stderr_limit(self) -> int:
        return self[9]

    @property
    def build_timeout_ns(self) -> int:
        return self[10]

    @property
    def probe_output_limit(self) -> int:
        return self[11]

    @property
    def probe_timeout_ns(self) -> int:
        return self[12]


def docker_policy_is_valid_v1(value: object) -> bool:
    if type(value) is not DockerBuildPolicyV1:
        return False
    try:
        return tuple(DockerBuildPolicyV1(*tuple(value))) == tuple(value)
    except Exception:
        return False


def transport_policy_identity_v1(policy: DockerBuildPolicyV1) -> bytes:
    """Bind every declared transport-policy coordinate in constructor order."""

    if not docker_policy_is_valid_v1(policy):
        raise TypeError("policy must be canonical DockerBuildPolicyV1")
    return _identity(
        b"labcolors.proof-region.docker-transport-policy.v1\0",
        (
            policy.image_reference.encode("utf-8"),
            policy.platform.encode("utf-8"),
            policy.hostname.encode("utf-8"),
            policy.container_name_prefix.encode("utf-8"),
            policy.bootstrap.encode("utf-8"),
            policy.bootstrap_argv0.encode("utf-8"),
            len(policy.tmpfs_specs).to_bytes(4, "big"),
            *(spec.encode("utf-8") for spec in policy.tmpfs_specs),
            policy.user_mode.value.encode("ascii"),
            policy.stdout_limit.to_bytes(8, "big"),
            policy.stderr_limit.to_bytes(8, "big"),
            policy.build_timeout_ns.to_bytes(8, "big"),
            policy.probe_output_limit.to_bytes(8, "big"),
            policy.probe_timeout_ns.to_bytes(8, "big"),
        ),
    )


class _NativeCommandSlotV1(StrEnum):
    CLI_PATH = "cli_path"
    IMAGE_REFERENCE = "image_reference"
    PLATFORM = "platform"
    ORDERED_TMPFS_SPECS = "ordered_tmpfs_specs"
    CONTAINER_NAME = "container_name"
    HOSTNAME = "hostname"
    HOST_USER = "host_user"
    CID_FILE = "cid_file"
    BOOTSTRAP = "bootstrap"
    BOOTSTRAP_ARGV0 = "bootstrap_argv0"
    INPUT_LENGTH = "input_length"
    INPUT_SHA256 = "input_sha256"
    CONTAINER_COORDINATE = "container_coordinate"
    CONTAINER_FILTER = "container_filter"


class _NativeCommandTokenV1(tuple):
    """One tagged literal or named slot in the native command grammar."""

    __slots__ = ()

    def __new__(
        cls,
        literal: str | None = None,
        slot: _NativeCommandSlotV1 | None = None,
    ) -> _NativeCommandTokenV1:
        if (literal is None) == (slot is None):
            raise TypeError("command token must be exactly one literal or slot")
        if literal is not None:
            if type(literal) is not str or not literal or "\0" in literal:
                raise TypeError("invalid native command literal")
            return tuple.__new__(cls, (b"literal", literal))
        if type(slot) is not _NativeCommandSlotV1:
            raise TypeError("invalid native command slot")
        return tuple.__new__(cls, (b"slot", slot))

    @property
    def tag(self) -> bytes:
        return self[0]

    @property
    def value(self) -> str | _NativeCommandSlotV1:
        return self[1]


def _literal_v1(value: str) -> _NativeCommandTokenV1:
    return _NativeCommandTokenV1(literal=value)


def _slot_v1(value: _NativeCommandSlotV1) -> _NativeCommandTokenV1:
    return _NativeCommandTokenV1(slot=value)


_NATIVE_COMMAND_TEMPLATES_V1: tuple[
    tuple[str, tuple[_NativeCommandTokenV1, ...]], ...
] = (
    (
        "version_probe",
        (
            _slot_v1(_NativeCommandSlotV1.CLI_PATH),
            _literal_v1("version"),
            _literal_v1("--format"),
            _literal_v1("{{json .Server}}"),
        ),
    ),
    (
        "image_inspect",
        (
            _slot_v1(_NativeCommandSlotV1.CLI_PATH),
            _literal_v1("image"),
            _literal_v1("inspect"),
            _slot_v1(_NativeCommandSlotV1.IMAGE_REFERENCE),
        ),
    ),
    (
        "build",
        (
            _slot_v1(_NativeCommandSlotV1.CLI_PATH),
            _literal_v1("run"),
            _literal_v1("--rm"),
            _literal_v1("--interactive"),
            _literal_v1("--pull"),
            _literal_v1("never"),
            _literal_v1("--platform"),
            _slot_v1(_NativeCommandSlotV1.PLATFORM),
            _literal_v1("--network"),
            _literal_v1("none"),
            _literal_v1("--read-only"),
            _literal_v1("--tmpfs"),
            _slot_v1(_NativeCommandSlotV1.ORDERED_TMPFS_SPECS),
            _literal_v1("--cap-drop"),
            _literal_v1("ALL"),
            _literal_v1("--security-opt"),
            _literal_v1("no-new-privileges:true"),
            _literal_v1("--name"),
            _slot_v1(_NativeCommandSlotV1.CONTAINER_NAME),
            _literal_v1("--hostname"),
            _slot_v1(_NativeCommandSlotV1.HOSTNAME),
            _literal_v1("--user"),
            _slot_v1(_NativeCommandSlotV1.HOST_USER),
            _literal_v1("--workdir"),
            _literal_v1("/"),
            _literal_v1("--cidfile"),
            _slot_v1(_NativeCommandSlotV1.CID_FILE),
            _literal_v1("--entrypoint"),
            _literal_v1("/usr/bin/env"),
            _slot_v1(_NativeCommandSlotV1.IMAGE_REFERENCE),
            _literal_v1("-i"),
            _literal_v1("PATH=/usr/local/bin:/usr/bin:/bin"),
            _literal_v1("LC_ALL=C"),
            _literal_v1("LANG=C"),
            _literal_v1("TZ=UTC"),
            _literal_v1("HOME=/nonexistent"),
            _literal_v1("/bin/sh"),
            _literal_v1("-c"),
            _slot_v1(_NativeCommandSlotV1.BOOTSTRAP),
            _slot_v1(_NativeCommandSlotV1.BOOTSTRAP_ARGV0),
            _slot_v1(_NativeCommandSlotV1.INPUT_LENGTH),
            _slot_v1(_NativeCommandSlotV1.INPUT_SHA256),
        ),
    ),
    (
        "cleanup_rm",
        (
            _slot_v1(_NativeCommandSlotV1.CLI_PATH),
            _literal_v1("container"),
            _literal_v1("rm"),
            _literal_v1("--force"),
            _slot_v1(_NativeCommandSlotV1.CONTAINER_COORDINATE),
        ),
    ),
    (
        "cleanup_ls",
        (
            _slot_v1(_NativeCommandSlotV1.CLI_PATH),
            _literal_v1("container"),
            _literal_v1("ls"),
            _literal_v1("--all"),
            _literal_v1("--quiet"),
            _literal_v1("--no-trunc"),
            _literal_v1("--filter"),
            _slot_v1(_NativeCommandSlotV1.CONTAINER_FILTER),
        ),
    ),
)


def native_command_contract_identity_v1() -> bytes:
    chunks: list[bytes] = [len(_NATIVE_COMMAND_TEMPLATES_V1).to_bytes(4, "big")]
    for name, tokens in _NATIVE_COMMAND_TEMPLATES_V1:
        chunks.extend((name.encode("ascii"), len(tokens).to_bytes(4, "big")))
        for token in tokens:
            value = token.value
            chunks.extend(
                (
                    token.tag,
                    (
                        value.value.encode("ascii")
                        if type(value) is _NativeCommandSlotV1
                        else value.encode("utf-8")
                    ),
                )
            )
    return _identity(
        b"labcolors.proof-region.native-command-contract.v1\0",
        tuple(chunks),
    )


def _native_command_path_v1(value: object) -> tuple[Path, bytes]:
    if type(value) is not _PATH_TYPE or not value.is_absolute():
        raise TypeError("native command path must be an absolute Path")
    try:
        encoded = os.fsencode(value)
    except (TypeError, UnicodeEncodeError) as error:
        raise TypeError("native command path is not filesystem-encodable") from error
    if not encoded or b"\0" in encoded:
        raise TypeError("invalid native command path bytes")
    return value, encoded


class NativeCommandCoordinateV1(tuple):
    """Exact filesystem command coordinate bound to the native argv grammar."""

    __slots__ = ()

    def __new__(
        cls,
        path_bytes: bytes,
        command_contract_identity: bytes,
    ) -> NativeCommandCoordinateV1:
        if type(path_bytes) is not bytes or not path_bytes or b"\0" in path_bytes:
            raise TypeError("invalid native command path bytes")
        try:
            path = Path(os.fsdecode(path_bytes))
        except (TypeError, UnicodeDecodeError) as error:
            raise TypeError("invalid native command path bytes") from error
        _owned_path, encoded = _native_command_path_v1(path)
        if encoded != path_bytes:
            raise TypeError("native command path bytes are not canonical")
        if command_contract_identity != native_command_contract_identity_v1():
            raise TypeError("foreign native command contract")
        return tuple.__new__(cls, (path_bytes, command_contract_identity))

    @property
    def path_bytes(self) -> bytes:
        return self[0]

    @property
    def path(self) -> Path:
        return Path(os.fsdecode(self.path_bytes))

    @property
    def command_contract_identity(self) -> bytes:
        return self[1]

    @property
    def identity(self) -> bytes:
        return _native_command_coordinate_identity_v1(self)


def native_command_coordinate_v1(path: Path) -> NativeCommandCoordinateV1:
    _owned, encoded = _native_command_path_v1(path)
    return NativeCommandCoordinateV1(
        encoded,
        native_command_contract_identity_v1(),
    )


def _native_command_coordinate_identity_v1(
    coordinate: NativeCommandCoordinateV1,
) -> bytes:
    if type(coordinate) is not NativeCommandCoordinateV1:
        raise TypeError("coordinate must be NativeCommandCoordinateV1")
    canonical = NativeCommandCoordinateV1(*tuple(coordinate))
    if tuple(canonical) != tuple(coordinate):
        raise TypeError("coordinate is not canonical")
    return _identity(
        b"labcolors.proof-region.native-command-coordinate.v1\0",
        (
            coordinate.command_contract_identity,
            coordinate.path_bytes,
        ),
    )


def _render_native_command_v1(
    template_name: str,
    command_coordinate: NativeCommandCoordinateV1,
    values: dict[_NativeCommandSlotV1, tuple[str, ...]],
) -> tuple[str, ...]:
    if type(template_name) is not str or type(values) is not dict:
        raise TypeError("invalid native command expansion")
    canonical_coordinate = NativeCommandCoordinateV1(*tuple(command_coordinate))
    templates = dict(_NATIVE_COMMAND_TEMPLATES_V1)
    try:
        tokens = templates[template_name]
    except KeyError as error:
        raise TypeError("unknown native command template") from error
    owned_values = dict(values)
    if _NativeCommandSlotV1.CLI_PATH in owned_values:
        raise TypeError("native command path is owned by its coordinate")
    owned_values[_NativeCommandSlotV1.CLI_PATH] = (
        os.fsdecode(canonical_coordinate.path_bytes),
    )
    expected_slots = {
        token.value
        for token in tokens
        if token.tag == b"slot"
    }
    if set(owned_values) != expected_slots:
        raise TypeError("native command slots do not match its template")
    command: list[str] = []
    for token in tokens:
        if token.tag == b"literal":
            command.append(token.value)
            continue
        slot = token.value
        expanded = owned_values[slot]
        if (
            type(expanded) is not tuple
            or (
                slot is not _NativeCommandSlotV1.ORDERED_TMPFS_SPECS
                and len(expanded) != 1
            )
            or any(
                type(value) is not str or not value or "\0" in value
                for value in expanded
            )
        ):
            raise TypeError("invalid native command slot expansion")
        if slot is _NativeCommandSlotV1.ORDERED_TMPFS_SPECS:
            if not expanded or not command:
                raise TypeError("invalid ordered tmpfs template")
            repeated_literal = command.pop()
            for value in expanded:
                command.extend((repeated_literal, value))
            continue
        command.extend(expanded)
    try:
        tuple(os.fsencode(value) for value in command)
    except (TypeError, UnicodeEncodeError) as error:
        raise TypeError("native command contains an unencodable coordinate") from error
    return tuple(command)


class DockerBlockerReasonV1(StrEnum):
    HOST_NOT_LINUX_AMD64 = "host_not_linux_amd64"
    DOCKER_UNAVAILABLE = "docker_unavailable"
    IMAGE_UNAVAILABLE = "image_unavailable"
    IMAGE_IDENTITY_MISMATCH = "image_identity_mismatch"
    BACKEND_CONTRACT = "backend_contract"


class DockerUnsupportedV1(tuple):
    __slots__ = ()

    def __new__(
        cls,
        reason: DockerBlockerReasonV1,
        detail: str,
    ) -> DockerUnsupportedV1:
        if type(reason) is not DockerBlockerReasonV1:
            raise TypeError("invalid Docker blocker reason")
        if type(detail) is not str or not detail or len(detail) > 4096:
            raise TypeError("invalid Docker blocker detail")
        return tuple.__new__(cls, (reason, detail))

    @property
    def reason(self) -> DockerBlockerReasonV1:
        return self[0]

    @property
    def detail(self) -> str:
        return self[1]


class DockerDaemonObservationV1(tuple):
    """Exact stdout bytes observed from the two admitted Docker probes."""

    __slots__ = ()

    def __new__(
        cls,
        server_stdout: bytes,
        image_inspect_stdout: bytes,
    ) -> DockerDaemonObservationV1:
        for value, field_name in (
            (server_stdout, "server_stdout"),
            (image_inspect_stdout, "image_inspect_stdout"),
        ):
            if (
                type(value) is not bytes
                or not value
                or len(value) > DOCKER_PROBE_OUTPUT_LIMIT_V1
            ):
                raise TypeError(f"invalid Docker daemon {field_name}")
        return tuple.__new__(cls, (server_stdout, image_inspect_stdout))

    @property
    def server_stdout(self) -> bytes:
        return self[0]

    @property
    def image_inspect_stdout(self) -> bytes:
        return self[1]

    @property
    def identity(self) -> bytes:
        return _docker_daemon_observation_identity_v1(self)


def _docker_daemon_observation_identity_v1(
    observation: DockerDaemonObservationV1,
) -> bytes:
    if type(observation) is not DockerDaemonObservationV1:
        raise TypeError("observation must be DockerDaemonObservationV1")
    canonical = DockerDaemonObservationV1(*tuple(observation))
    if tuple(canonical) != tuple(observation):
        raise TypeError("daemon observation is not canonical")
    return _identity(
        b"labcolors.proof-region.docker-daemon-observation.v1\0",
        (
            observation.server_stdout,
            observation.image_inspect_stdout,
        ),
    )


def _host_user_identity_v1(host_user: tuple[int, int]) -> bytes:
    owned = _host_user_coordinates(host_user)
    return _identity(
        b"labcolors.proof-region.host-user.v1\0",
        (
            owned[0].to_bytes(4, "big"),
            owned[1].to_bytes(4, "big"),
        ),
    )


class DockerSupportedV1(tuple):
    """Canonical capability observed for one exact native Docker coordinate."""

    __slots__ = ()

    def __new__(
        cls,
        policy: DockerBuildPolicyV1,
        daemon_observation: DockerDaemonObservationV1,
        command_coordinate: NativeCommandCoordinateV1,
        host_user: tuple[int, int],
    ) -> DockerSupportedV1:
        if not docker_policy_is_valid_v1(policy):
            raise TypeError("invalid Docker policy capability")
        if type(daemon_observation) is not DockerDaemonObservationV1:
            raise TypeError("invalid Docker daemon observation")
        canonical_daemon = DockerDaemonObservationV1(*tuple(daemon_observation))
        if (
            tuple(canonical_daemon) != tuple(daemon_observation)
            or len(canonical_daemon.server_stdout) > policy.probe_output_limit
            or len(canonical_daemon.image_inspect_stdout)
            > policy.probe_output_limit
        ):
            raise TypeError("Docker daemon observation is not canonical")
        if type(command_coordinate) is not NativeCommandCoordinateV1:
            raise TypeError("invalid native Docker command coordinate")
        canonical_command = NativeCommandCoordinateV1(*tuple(command_coordinate))
        if tuple(canonical_command) != tuple(command_coordinate):
            raise TypeError("native Docker command coordinate is not canonical")
        owned_user = _host_user_coordinates(host_user)
        return tuple.__new__(
            cls,
            (policy, daemon_observation, command_coordinate, owned_user),
        )

    @property
    def policy(self) -> DockerBuildPolicyV1:
        return self[0]

    @property
    def daemon_observation(self) -> DockerDaemonObservationV1:
        return self[1]

    @property
    def command_coordinate(self) -> NativeCommandCoordinateV1:
        return self[2]

    @property
    def host_user(self) -> tuple[int, int]:
        return self[3]

    @property
    def policy_identity(self) -> bytes:
        return transport_policy_identity_v1(self.policy)

    @property
    def daemon_observation_identity(self) -> bytes:
        return _docker_daemon_observation_identity_v1(self.daemon_observation)

    @property
    def command_coordinate_identity(self) -> bytes:
        return _native_command_coordinate_identity_v1(self.command_coordinate)

    @property
    def host_user_identity(self) -> bytes:
        return _host_user_identity_v1(self.host_user)

    @property
    def identity(self) -> bytes:
        return docker_capability_identity_v1(self)


def _docker_supported_is_valid_v1(value: object) -> bool:
    if type(value) is not DockerSupportedV1:
        return False
    try:
        return tuple(DockerSupportedV1(*tuple(value))) == tuple(value)
    except Exception:
        return False


def docker_capability_identity_v1(capability: DockerSupportedV1) -> bytes:
    if not _docker_supported_is_valid_v1(capability):
        raise TypeError("capability must be canonical DockerSupportedV1")
    return _identity(
        b"labcolors.proof-region.docker-capability.v1\0",
        (
            capability.policy_identity,
            capability.command_coordinate_identity,
            capability.daemon_observation_identity,
            capability.host_user[0].to_bytes(4, "big"),
            capability.host_user[1].to_bytes(4, "big"),
        ),
    )


DockerCapabilityReportV1: TypeAlias = DockerSupportedV1 | DockerUnsupportedV1


def _docker_unsupported_is_valid_v1(value: object) -> bool:
    if type(value) is not DockerUnsupportedV1:
        return False
    try:
        return tuple(DockerUnsupportedV1(*tuple(value))) == tuple(value)
    except Exception:
        return False


def _absolute_path(value: object, field_name: str) -> Path:
    if type(value) is not _PATH_TYPE or not value.is_absolute():
        raise TypeError(f"{field_name} must be an absolute Path")
    try:
        encoded = os.fsencode(value)
    except (TypeError, UnicodeEncodeError) as error:
        raise TypeError(f"{field_name} is not filesystem-encodable") from error
    if (
        not encoded
        or b"\0" in encoded
        or any(character in str(value) for character in (",", "\n", "\r"))
    ):
        raise TypeError(f"{field_name} is not Docker-mount-safe")
    return value


def _host_user_coordinates(value: object) -> tuple[int, int]:
    if (
        type(value) is not tuple
        or len(value) != 2
        or any(type(item) is not int or item < 0 or item >= 1 << 32 for item in value)
    ):
        raise TypeError("host_user must be one exact Linux uid/gid pair")
    return value


def _container_name(value: object, prefix: str) -> str:
    if (
        type(value) is not str
        or type(prefix) is not str
        or not value.startswith(prefix)
        or len(value) > 128
        or any(character not in "abcdefghijklmnopqrstuvwxyz0123456789-" for character in value)
    ):
        raise TypeError("invalid controller-owned Docker container name")
    return value


class DockerBuildRequestV1(tuple):
    __slots__ = ()

    def __new__(
        cls,
        attempt: int,
        capability: DockerSupportedV1,
        input_bundle: input.SealedInputV1,
        max_output_bytes: int,
        cid_file: Path,
        container_name: str,
    ) -> DockerBuildRequestV1:
        if type(attempt) is not int or attempt not in (1, 2):
            raise TypeError("attempt must be 1 or 2")
        if not _docker_supported_is_valid_v1(capability):
            raise TypeError("capability must be canonical DockerSupportedV1")
        if not input.sealed_input_is_intact_v1(input_bundle):
            raise TypeError("input_bundle must preserve exact sealed bytes")
        if (
            type(max_output_bytes) is not int
            or max_output_bytes <= 0
            or max_output_bytes > capability.policy.stdout_limit
        ):
            raise TypeError("invalid executable output limit")
        _absolute_path(cid_file, "cid_file")
        _container_name(container_name, capability.policy.container_name_prefix)
        return tuple.__new__(
            cls,
            (
                attempt,
                capability,
                input_bundle,
                max_output_bytes,
                cid_file,
                container_name,
            ),
        )

    @property
    def attempt(self) -> int:
        return self[0]

    @property
    def capability(self) -> DockerSupportedV1:
        return self[1]

    @property
    def input_bundle(self) -> input.SealedInputV1:
        return self[2]

    @property
    def max_output_bytes(self) -> int:
        return self[3]

    @property
    def cid_file(self) -> Path:
        return self[4]

    @property
    def container_name(self) -> str:
        return self[5]


def _docker_build_request_is_valid_v1(
    value: object,
    capability: DockerSupportedV1,
) -> bool:
    if (
        type(value) is not DockerBuildRequestV1
        or not _docker_supported_is_valid_v1(capability)
    ):
        return False
    try:
        canonical = DockerBuildRequestV1(*tuple(value))
        return (
            tuple(canonical) == tuple(value)
            and canonical.capability == capability
            and input.sealed_input_is_intact_v1(canonical.input_bundle)
        )
    except Exception:
        return False


def _bounded_bytes(value: object, maximum: int, field_name: str) -> bytes:
    if type(value) is not bytes or len(value) > maximum:
        raise TypeError(f"invalid {field_name}")
    return value


class BuildInputTransferProgressV1(tuple):
    __slots__ = ()

    def __new__(
        cls,
        bundle_identity: bytes,
        expected_length: int,
        expected_sha256: bytes,
        written_length: int,
        written_sha256: bytes,
        *,
        _token: object,
    ) -> BuildInputTransferProgressV1:
        if _token is not _BUILD_INPUT_PROGRESS_TOKEN:
            raise TypeError("build input progress is controller-observed")
        if not _valid_digest(bundle_identity) or not _valid_digest(expected_sha256):
            raise TypeError("invalid build input progress coordinates")
        if (
            type(expected_length) is not int
            or expected_length <= 0
            or expected_length >= 1 << 64
            or type(written_length) is not int
            or written_length < 0
            or written_length >= 1 << 64
            or written_length > expected_length
            or type(written_sha256) is not bytes
            or len(written_sha256) != 32
        ):
            raise TypeError("invalid build input progress")
        return tuple.__new__(
            cls,
            (
                bundle_identity,
                expected_length,
                expected_sha256,
                written_length,
                written_sha256,
            ),
        )

    @property
    def bundle_identity(self) -> bytes:
        return self[0]

    @property
    def expected_length(self) -> int:
        return self[1]

    @property
    def expected_sha256(self) -> bytes:
        return self[2]

    @property
    def written_length(self) -> int:
        return self[3]

    @property
    def written_sha256(self) -> bytes:
        return self[4]


def _build_input_progress_v1(
    bundle: input.SealedInputV1,
    written_length: int,
    written_sha256: bytes,
) -> BuildInputTransferProgressV1:
    if not input.sealed_input_is_intact_v1(bundle):
        raise TypeError("build input bytes are not intact")
    if (
        type(written_length) is not int
        or written_length < 0
        or written_length > bundle.length
        or type(written_sha256) is not bytes
        or written_sha256
        != hashlib.sha256(bundle.contents[:written_length]).digest()
    ):
        raise TypeError("build input progress does not match the sealed bytes")
    return BuildInputTransferProgressV1(
        bundle.binding_identity,
        bundle.length,
        bundle.sha256,
        written_length,
        written_sha256,
        _token=_BUILD_INPUT_PROGRESS_TOKEN,
    )


def _input_progress_matches_v1(
    value: object,
    bundle: input.SealedInputV1,
) -> bool:
    if (
        type(value) is not BuildInputTransferProgressV1
        or not input.sealed_input_is_intact_v1(bundle)
    ):
        return False
    try:
        canonical = _build_input_progress_v1(
            bundle,
            value.written_length,
            value.written_sha256,
        )
        return tuple(canonical) == tuple(value)
    except Exception:
        return False


class BuildInputTransferV1(tuple):
    __slots__ = ()

    def __new__(
        cls,
        progress: BuildInputTransferProgressV1,
        *,
        _token: object,
    ) -> BuildInputTransferV1:
        if _token is not _BUILD_INPUT_TRANSFER_TOKEN:
            raise TypeError("build input transfer is controller-observed")
        if (
            type(progress) is not BuildInputTransferProgressV1
            or progress.written_length != progress.expected_length
            or progress.written_sha256 != progress.expected_sha256
        ):
            raise TypeError("completed build input transfer must be exact")
        return tuple.__new__(cls, tuple(progress))

    @property
    def bundle_identity(self) -> bytes:
        return self[0]

    @property
    def expected_length(self) -> int:
        return self[1]

    @property
    def expected_sha256(self) -> bytes:
        return self[2]

    @property
    def written_length(self) -> int:
        return self[3]

    @property
    def written_sha256(self) -> bytes:
        return self[4]


def _input_transfer_is_structurally_valid_v1(value: object) -> bool:
    if type(value) is not BuildInputTransferV1:
        return False
    try:
        return (
            len(value) == 5
            and _valid_digest(value.bundle_identity)
            and type(value.expected_length) is int
            and 0 < value.expected_length < 1 << 64
            and _valid_digest(value.expected_sha256)
            and value.written_length == value.expected_length
            and value.written_sha256 == value.expected_sha256
        )
    except Exception:
        return False


def _completed_build_input_transfer_v1(
    bundle: input.SealedInputV1,
    written_length: int,
    written_sha256: bytes,
) -> BuildInputTransferV1:
    progress = _build_input_progress_v1(bundle, written_length, written_sha256)
    return BuildInputTransferV1(
        progress,
        _token=_BUILD_INPUT_TRANSFER_TOKEN,
    )


class _DockerCommandExitedV1(tuple):
    __slots__ = ()

    def __new__(
        cls,
        returncode: int,
        stdout: bytes,
        stderr: bytes,
        *,
        _token: object,
    ) -> _DockerCommandExitedV1:
        if _token is not _DOCKER_COMMAND_EXITED_TOKEN:
            raise TypeError("Docker command exit is controller-observed")
        if type(returncode) is not int or not -(1 << 31) <= returncode < 1 << 31:
            raise TypeError("invalid Docker returncode")
        _bounded_bytes(stdout, BUILD_STDOUT_LIMIT_V1, "stdout")
        _bounded_bytes(stderr, BUILD_STDERR_LIMIT_V1, "stderr")
        return tuple.__new__(cls, (returncode, stdout, stderr))

    @property
    def returncode(self) -> int:
        return self[0]

    @property
    def stdout(self) -> bytes:
        return self[1]

    @property
    def stderr(self) -> bytes:
        return self[2]


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


class DockerBuildExitedV1(tuple):
    __slots__ = ()

    def __new__(
        cls,
        returncode: int,
        stdout: bytes,
        stderr: bytes,
        input_transfer: BuildInputTransferV1,
        *,
        _token: object,
    ) -> DockerBuildExitedV1:
        if _token is not _DOCKER_BUILD_EXITED_TOKEN:
            raise TypeError("Docker build exit is controller-observed")
        if type(returncode) is not int or not -(1 << 31) <= returncode < 1 << 31:
            raise TypeError("invalid Docker returncode")
        _bounded_bytes(stdout, BUILD_STDOUT_LIMIT_V1, "stdout")
        _bounded_bytes(stderr, BUILD_STDERR_LIMIT_V1, "stderr")
        if not _input_transfer_is_structurally_valid_v1(input_transfer):
            raise TypeError("invalid Docker build input transfer")
        return tuple.__new__(
            cls,
            (returncode, stdout, stderr, input_transfer),
        )

    @property
    def returncode(self) -> int:
        return self[0]

    @property
    def stdout(self) -> bytes:
        return self[1]

    @property
    def stderr(self) -> bytes:
        return self[2]

    @property
    def input_transfer(self) -> BuildInputTransferV1:
        return self[3]


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


def docker_build_exited_is_valid_v1(
    value: object,
    input_value: input.SealedInputV1,
    max_output_bytes: int,
    max_stderr_bytes: int,
) -> bool:
    if (
        type(value) is not DockerBuildExitedV1
        or not input.sealed_input_is_intact_v1(input_value)
        or type(max_output_bytes) is not int
        or max_output_bytes <= 0
        or type(max_stderr_bytes) is not int
        or max_stderr_bytes <= 0
    ):
        return False
    try:
        return (
            len(value) == 4
            and type(value.returncode) is int
            and -(1 << 31) <= value.returncode < 1 << 31
            and type(value.stdout) is bytes
            and len(value.stdout) <= max_output_bytes
            and type(value.stderr) is bytes
            and len(value.stderr) <= max_stderr_bytes
            and _input_transfer_is_structurally_valid_v1(value.input_transfer)
            and value.input_transfer.bundle_identity
            == input_value.binding_identity
            and value.input_transfer.expected_length == input_value.length
            and value.input_transfer.expected_sha256 == input_value.sha256
            and value.input_transfer.written_length == input_value.length
            and value.input_transfer.written_sha256 == input_value.sha256
        )
    except Exception:
        return False


class DockerBuildTimedOutV1(tuple):
    __slots__ = ()

    def __new__(
        cls,
        stdout: bytes,
        stderr: bytes,
        input_progress: BuildInputTransferProgressV1 | None = None,
    ) -> DockerBuildTimedOutV1:
        _bounded_bytes(stdout, BUILD_STDOUT_LIMIT_V1, "stdout")
        _bounded_bytes(stderr, BUILD_STDERR_LIMIT_V1, "stderr")
        if input_progress is not None and type(
            input_progress
        ) is not BuildInputTransferProgressV1:
            raise TypeError("invalid timed-out build input progress")
        return tuple.__new__(cls, (stdout, stderr, input_progress))

    @property
    def stdout(self) -> bytes:
        return self[0]

    @property
    def stderr(self) -> bytes:
        return self[1]

    @property
    def input_progress(self) -> BuildInputTransferProgressV1 | None:
        return self[2]


class DockerOutputStreamV1(StrEnum):
    STDOUT = "stdout"
    STDERR = "stderr"


class DockerBuildOutputLimitV1(tuple):
    __slots__ = ()

    def __new__(
        cls,
        stream: DockerOutputStreamV1,
        stdout: bytes,
        stderr: bytes,
        input_progress: BuildInputTransferProgressV1 | None = None,
    ) -> DockerBuildOutputLimitV1:
        if type(stream) is not DockerOutputStreamV1:
            raise TypeError("invalid Docker output stream")
        _bounded_bytes(stdout, BUILD_STDOUT_LIMIT_V1, "stdout")
        _bounded_bytes(stderr, BUILD_STDERR_LIMIT_V1, "stderr")
        if input_progress is not None and type(
            input_progress
        ) is not BuildInputTransferProgressV1:
            raise TypeError("invalid output-limited build input progress")
        return tuple.__new__(cls, (stream, stdout, stderr, input_progress))

    @property
    def stream(self) -> DockerOutputStreamV1:
        return self[0]

    @property
    def stdout(self) -> bytes:
        return self[1]

    @property
    def stderr(self) -> bytes:
        return self[2]

    @property
    def input_progress(self) -> BuildInputTransferProgressV1 | None:
        return self[3]


class DockerBuildObserverFailureV1(tuple):
    __slots__ = ()

    def __new__(
        cls,
        detail: str,
        stdout: bytes,
        stderr: bytes,
        input_progress: BuildInputTransferProgressV1 | None = None,
    ) -> DockerBuildObserverFailureV1:
        if type(detail) is not str or not detail or len(detail) > 4096:
            raise TypeError("invalid Docker observer failure")
        _bounded_bytes(stdout, BUILD_STDOUT_LIMIT_V1, "stdout")
        _bounded_bytes(stderr, BUILD_STDERR_LIMIT_V1, "stderr")
        if input_progress is not None and type(
            input_progress
        ) is not BuildInputTransferProgressV1:
            raise TypeError("invalid observer-failure build input progress")
        return tuple.__new__(cls, (detail, stdout, stderr, input_progress))

    @property
    def detail(self) -> str:
        return self[0]

    @property
    def stdout(self) -> bytes:
        return self[1]

    @property
    def stderr(self) -> bytes:
        return self[2]

    @property
    def input_progress(self) -> BuildInputTransferProgressV1 | None:
        return self[3]


class DockerBuildInputRejectedV1(tuple):
    __slots__ = ()

    def __new__(
        cls,
        input_progress: BuildInputTransferProgressV1,
        stdout: bytes,
        stderr: bytes,
    ) -> DockerBuildInputRejectedV1:
        if type(input_progress) is not BuildInputTransferProgressV1:
            raise TypeError("invalid partial build input progress")
        _bounded_bytes(stdout, BUILD_STDOUT_LIMIT_V1, "stdout")
        _bounded_bytes(stderr, BUILD_STDERR_LIMIT_V1, "stderr")
        return tuple.__new__(cls, (input_progress, stdout, stderr))

    @property
    def input_progress(self) -> BuildInputTransferProgressV1:
        return self[0]

    @property
    def stdout(self) -> bytes:
        return self[1]

    @property
    def stderr(self) -> bytes:
        return self[2]

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


class CleanupResourceV1(StrEnum):
    DOCKER_CLI_PROCESS = "docker_cli_process"
    DOCKER_CONTAINER = "docker_container"
    TEMPORARY_ROOT = "temporary_root"


class CleanupFailureRecordV1(tuple):
    __slots__ = ()

    def __new__(
        cls,
        resource: CleanupResourceV1,
        detail: str,
    ) -> CleanupFailureRecordV1:
        if type(resource) is not CleanupResourceV1:
            raise TypeError("invalid cleanup resource")
        if type(detail) is not str or not detail or len(detail) > 4096:
            raise TypeError("invalid cleanup failure detail")
        return tuple.__new__(cls, (resource, detail))

    @property
    def resource(self) -> CleanupResourceV1:
        return self[0]

    @property
    def detail(self) -> str:
        return self[1]


def _cleanup_failure_records_v1(
    value: object,
    allowed_order: tuple[CleanupResourceV1, ...],
) -> tuple[CleanupFailureRecordV1, ...]:
    if type(value) is not tuple or not value:
        raise TypeError("cleanup failures must be one nonempty tuple")
    order = {resource: index for index, resource in enumerate(allowed_order)}
    owned: list[CleanupFailureRecordV1] = []
    indexes: list[int] = []
    for record in value:
        if type(record) is not CleanupFailureRecordV1:
            raise TypeError("cleanup failure record is not canonical")
        canonical = CleanupFailureRecordV1(*tuple(record))
        if tuple(canonical) != tuple(record) or canonical.resource not in order:
            raise TypeError("cleanup failure record is not canonical")
        owned.append(canonical)
        indexes.append(order[canonical.resource])
    if indexes != sorted(set(indexes)):
        raise TypeError("cleanup failure records are not in stable resource order")
    return tuple(owned)


class DockerBuildCleanupFailureV1(tuple):
    __slots__ = ()

    def __new__(
        cls,
        trigger: DockerCleanupTriggerV1,
        failures: tuple[CleanupFailureRecordV1, ...],
        stdout: bytes,
        stderr: bytes,
        input_progress: BuildInputTransferProgressV1 | None = None,
    ) -> DockerBuildCleanupFailureV1:
        if type(trigger) is not DockerCleanupTriggerV1:
            raise TypeError("invalid Docker cleanup trigger")
        owned_failures = _cleanup_failure_records_v1(
            failures,
            (
                CleanupResourceV1.DOCKER_CLI_PROCESS,
                CleanupResourceV1.DOCKER_CONTAINER,
            ),
        )
        _bounded_bytes(stdout, BUILD_STDOUT_LIMIT_V1, "stdout")
        _bounded_bytes(stderr, BUILD_STDERR_LIMIT_V1, "stderr")
        if input_progress is not None and type(
            input_progress
        ) is not BuildInputTransferProgressV1:
            raise TypeError("invalid cleanup build input progress")
        return tuple.__new__(
            cls,
            (trigger, owned_failures, stdout, stderr, input_progress),
        )

    @property
    def trigger(self) -> DockerCleanupTriggerV1:
        return self[0]

    @property
    def failures(self) -> tuple[CleanupFailureRecordV1, ...]:
        return self[1]

    @property
    def detail(self) -> str:
        """Render all typed records for diagnostic-only consumers."""

        return "; ".join(record.detail for record in self.failures)

    @property
    def stdout(self) -> bytes:
        return self[2]

    @property
    def stderr(self) -> bytes:
        return self[3]

    @property
    def input_progress(self) -> BuildInputTransferProgressV1 | None:
        return self[4]


DockerBuildProcessObservationV1: TypeAlias = (
    DockerBuildExitedV1
    | DockerBuildTimedOutV1
    | DockerBuildOutputLimitV1
    | DockerBuildObserverFailureV1
    | DockerBuildInputRejectedV1
    | DockerBuildCleanupFailureV1
)


class BuildCleanupFailureV1(tuple):
    """Controller-owned cleanup failure with any preceding backend observation."""

    __slots__ = ()

    def __new__(
        cls,
        failures: tuple[CleanupFailureRecordV1, ...],
        current_process: DockerBuildProcessObservationV1 | None,
        *,
        _token: object,
    ) -> BuildCleanupFailureV1:
        if _token is not _BUILD_CLEANUP_FAILURE_TOKEN:
            raise TypeError("build cleanup failure is controller-observed")
        owned_failures = _cleanup_failure_records_v1(
            failures,
            (CleanupResourceV1.TEMPORARY_ROOT,),
        )
        if current_process is not None and type(current_process) not in (
            DockerBuildExitedV1,
            DockerBuildTimedOutV1,
            DockerBuildOutputLimitV1,
            DockerBuildObserverFailureV1,
            DockerBuildInputRejectedV1,
            DockerBuildCleanupFailureV1,
        ):
            raise TypeError("cleanup failure lost its current process observation")
        return tuple.__new__(cls, (owned_failures, current_process))

    @property
    def failures(self) -> tuple[CleanupFailureRecordV1, ...]:
        return self[0]

    @property
    def current_process(self) -> DockerBuildProcessObservationV1 | None:
        return self[1]


_DockerCommandObservationV1: TypeAlias = (
    _DockerCommandExitedV1 | DockerBuildProcessObservationV1
)


def _canonical_progress_v1(
    value: object,
    input_value: input.SealedInputV1,
) -> BuildInputTransferProgressV1 | None:
    if value is None:
        return None
    if not _input_progress_matches_v1(value, input_value):
        raise TypeError("build input progress is not canonical")
    return _build_input_progress_v1(
        input_value,
        value.written_length,
        value.written_sha256,
    )


def _canonical_process_observation_v1(
    value: object,
    input_value: input.SealedInputV1,
    max_output_bytes: int,
    max_stderr_bytes: int,
) -> DockerBuildProcessObservationV1:
    """Own a backend observation before classification or retention."""

    if (
        not input.sealed_input_is_intact_v1(input_value)
        or type(max_output_bytes) is not int
        or max_output_bytes <= 0
        or type(max_stderr_bytes) is not int
        or max_stderr_bytes <= 0
    ):
        raise TypeError("invalid process-observation boundary")
    try:
        if type(value) is DockerBuildExitedV1:
            if not docker_build_exited_is_valid_v1(
                value,
                input_value,
                max_output_bytes,
                max_stderr_bytes,
            ):
                raise TypeError("invalid exited build observation")
            transfer = _completed_build_input_transfer_v1(
                input_value,
                value.input_transfer.written_length,
                value.input_transfer.written_sha256,
            )
            canonical = _docker_build_exited_v1(
                value.returncode,
                bytes(value.stdout),
                bytes(value.stderr),
                transfer,
            )
            if tuple(canonical) != tuple(value):
                raise TypeError("exited build observation is not canonical")
            return value
        if type(value) is DockerBuildTimedOutV1:
            progress = _canonical_progress_v1(value.input_progress, input_value)
            if progress is None:
                raise TypeError("build timeout did not retain input progress")
            canonical = DockerBuildTimedOutV1(
                _bounded_bytes(value.stdout, max_output_bytes, "stdout"),
                _bounded_bytes(value.stderr, max_stderr_bytes, "stderr"),
                progress,
            )
            if tuple(canonical) != tuple(value):
                raise TypeError("timeout observation is not canonical")
            return value
        if type(value) is DockerBuildOutputLimitV1:
            progress = _canonical_progress_v1(value.input_progress, input_value)
            if progress is None:
                raise TypeError("build output limit did not retain input progress")
            stdout = _bounded_bytes(value.stdout, max_output_bytes, "stdout")
            stderr = _bounded_bytes(value.stderr, max_stderr_bytes, "stderr")
            if (
                (
                    value.stream is DockerOutputStreamV1.STDOUT
                    and len(stdout) != max_output_bytes
                )
                or (
                    value.stream is DockerOutputStreamV1.STDERR
                    and len(stderr) != max_stderr_bytes
                )
            ):
                raise TypeError("output-limit observation did not reach its cap")
            canonical = DockerBuildOutputLimitV1(
                value.stream,
                stdout,
                stderr,
                progress,
            )
            if tuple(canonical) != tuple(value):
                raise TypeError("output-limit observation is not canonical")
            return value
        if type(value) is DockerBuildObserverFailureV1:
            canonical = DockerBuildObserverFailureV1(
                value.detail,
                _bounded_bytes(value.stdout, max_output_bytes, "stdout"),
                _bounded_bytes(value.stderr, max_stderr_bytes, "stderr"),
                _canonical_progress_v1(value.input_progress, input_value),
            )
            if tuple(canonical) != tuple(value):
                raise TypeError("observer failure is not canonical")
            return value
        if type(value) is DockerBuildInputRejectedV1:
            progress = _canonical_progress_v1(value.input_progress, input_value)
            if progress is None or progress.written_length >= progress.expected_length:
                raise TypeError("input rejection did not retain partial progress")
            canonical = DockerBuildInputRejectedV1(
                progress,
                _bounded_bytes(value.stdout, max_output_bytes, "stdout"),
                _bounded_bytes(value.stderr, max_stderr_bytes, "stderr"),
            )
            if tuple(canonical) != tuple(value):
                raise TypeError("input rejection is not canonical")
            return value
        if type(value) is DockerBuildCleanupFailureV1:
            progress = _canonical_progress_v1(value.input_progress, input_value)
            if progress is None:
                raise TypeError("build cleanup failure did not retain input progress")
            canonical = DockerBuildCleanupFailureV1(
                value.trigger,
                value.failures,
                _bounded_bytes(value.stdout, max_output_bytes, "stdout"),
                _bounded_bytes(value.stderr, max_stderr_bytes, "stderr"),
                progress,
            )
            if tuple(canonical) != tuple(value):
                raise TypeError("cleanup failure is not canonical")
            return value
    except (AttributeError, IndexError, TypeError, ValueError) as error:
        raise TypeError("backend process observation is not canonical") from error
    raise TypeError("backend returned an unknown process observation")


def _build_cleanup_failure_v1(
    current_process: DockerBuildProcessObservationV1 | None,
    detail: str,
) -> BuildCleanupFailureV1:
    return BuildCleanupFailureV1(
        (
            CleanupFailureRecordV1(
                CleanupResourceV1.TEMPORARY_ROOT,
                detail,
            ),
        ),
        current_process,
        _token=_BUILD_CLEANUP_FAILURE_TOKEN,
    )


def _canonical_build_cleanup_failure_v1(
    value: object,
    input_value: input.SealedInputV1,
    max_output_bytes: int,
    max_stderr_bytes: int,
) -> BuildCleanupFailureV1:
    if type(value) is not BuildCleanupFailureV1:
        raise TypeError("unknown build cleanup observation")
    try:
        canonical_process = (
            None
            if value.current_process is None
            else _canonical_process_observation_v1(
                value.current_process,
                input_value,
                max_output_bytes,
                max_stderr_bytes,
            )
        )
        canonical = BuildCleanupFailureV1(
            value.failures,
            canonical_process,
            _token=_BUILD_CLEANUP_FAILURE_TOKEN,
        )
        if tuple(canonical) != tuple(value):
            raise TypeError("build cleanup observation is not canonical")
        return canonical
    except (AttributeError, IndexError, TypeError, ValueError) as error:
        raise TypeError("build cleanup observation is not canonical") from error


class DockerBuildBackendV1(Protocol):
    def probe(self) -> DockerCapabilityReportV1: ...

    def run_build(
        self,
        request: DockerBuildRequestV1,
    ) -> DockerBuildProcessObservationV1: ...


def build_process_bytes_v1(process: DockerBuildExitedV1) -> bytes:
    if type(process) is not DockerBuildExitedV1:
        raise TypeError("only successful typed build observations are encodable")
    try:
        transfer = process.input_transfer
        if (
            process.returncode != 0
            or not -(1 << 31) <= process.returncode < 1 << 31
            or type(process.stdout) is not bytes
            or len(process.stdout) > BUILD_STDOUT_LIMIT_V1
            or type(process.stderr) is not bytes
            or len(process.stderr) > BUILD_STDERR_LIMIT_V1
            or not _input_transfer_is_structurally_valid_v1(transfer)
        ):
            raise TypeError("successful build observation is not canonical")
    except (AttributeError, IndexError, OverflowError, TypeError) as error:
        raise TypeError(
            "only successful canonical build observations are encodable"
        ) from error
    return b"".join(
        (
            process.returncode.to_bytes(4, "big", signed=True),
            len(process.stdout).to_bytes(8, "big"),
            hashlib.sha256(process.stdout).digest(),
            len(process.stderr).to_bytes(8, "big"),
            hashlib.sha256(process.stderr).digest(),
            transfer.bundle_identity,
            transfer.expected_length.to_bytes(8, "big"),
            transfer.expected_sha256,
            transfer.written_length.to_bytes(8, "big"),
            transfer.written_sha256,
        )
    )


class NativeDockerBuildBackendV1:
    """Docker adapter whose probe observes only Linux x64 and its daemon."""

    def __init__(
        self,
        docker_path: Path,
        policy: DockerBuildPolicyV1,
        *,
        platform_name: str | None = None,
        machine_name: str | None = None,
        monotonic_ns: object = time.monotonic_ns,
        host_user: tuple[int, int] | None = None,
    ) -> None:
        _absolute_path(docker_path, "docker_path")
        if not docker_policy_is_valid_v1(policy):
            raise TypeError("policy must be DockerBuildPolicyV1")
        if policy.user_mode is not DockerUserModeV1.HOST_EFFECTIVE_IDS:
            raise TypeError("unsupported Docker user policy")
        observed_user = (
            (os.geteuid(), os.getegid()) if host_user is None else host_user
        )
        observed_platform = (
            platform.system().lower() if platform_name is None else platform_name
        )
        observed_machine = (
            platform.machine() if machine_name is None else machine_name
        )
        self._command_coordinate = native_command_coordinate_v1(docker_path)
        self._policy = DockerBuildPolicyV1(*tuple(policy))
        self._platform_name = _encoded_policy_text(
            observed_platform,
            64,
            "platform_name",
        )
        self._machine_name = _encoded_policy_text(
            observed_machine,
            64,
            "machine_name",
        )
        self._monotonic_ns = monotonic_ns
        self._host_user = _host_user_coordinates(observed_user)
        self._probed_capability: DockerSupportedV1 | None = None

    @staticmethod
    def _environment() -> dict[str, str]:
        return {
            "HOME": "/nonexistent",
            "PATH": "/usr/bin:/bin",
            "DOCKER_CONFIG": "/nonexistent",
        }

    def probe(self) -> DockerCapabilityReportV1:
        self._probed_capability = None
        if self._platform_name != "linux" or self._machine_name.lower() not in (
            "x86_64",
            "amd64",
        ):
            return DockerUnsupportedV1(
                DockerBlockerReasonV1.HOST_NOT_LINUX_AMD64,
                "controlled build requires a Linux amd64 Docker host",
            )
        try:
            metadata = self._command_coordinate.path.lstat()
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
            _render_native_command_v1(
                "version_probe",
                self._command_coordinate,
                {},
            ),
            _render_native_command_v1(
                "image_inspect",
                self._command_coordinate,
                {
                    _NativeCommandSlotV1.IMAGE_REFERENCE: (
                        self._policy.image_reference,
                    ),
                },
            ),
        )
        outputs: list[bytes] = []
        for index, command in enumerate(commands):
            result = self._observe_command(
                command,
                stdout_limit=self._policy.probe_output_limit,
                stderr_limit=self._policy.probe_output_limit,
                timeout_ns=self._policy.probe_timeout_ns,
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
                or self._policy.image_reference not in repo_digests
            ):
                raise ValueError("foreign image coordinate")
        except (ValueError, TypeError, json.JSONDecodeError):
            return DockerUnsupportedV1(
                DockerBlockerReasonV1.IMAGE_IDENTITY_MISMATCH,
                "local image does not match pinned linux/amd64 manifest",
            )
        daemon_observation = DockerDaemonObservationV1(
            outputs[0],
            outputs[1],
        )
        capability = DockerSupportedV1(
            self._policy,
            daemon_observation,
            self._command_coordinate,
            self._host_user,
        )
        self._probed_capability = capability
        return capability

    def command_for(self, request: DockerBuildRequestV1) -> tuple[str, ...]:
        if type(request) is not DockerBuildRequestV1:
            raise TypeError("request must be DockerBuildRequestV1")
        try:
            capability = request.capability
        except (AttributeError, IndexError) as error:
            raise TypeError("request lost its Docker capability") from error
        if (
            self._probed_capability is None
            or not _docker_build_request_is_valid_v1(request, capability)
            or capability is not self._probed_capability
        ):
            raise TypeError("request capability does not match this backend probe")
        policy = capability.policy
        return _render_native_command_v1(
            "build",
            capability.command_coordinate,
            {
                _NativeCommandSlotV1.PLATFORM: (policy.platform,),
                _NativeCommandSlotV1.ORDERED_TMPFS_SPECS: policy.tmpfs_specs,
                _NativeCommandSlotV1.CONTAINER_NAME: (request.container_name,),
                _NativeCommandSlotV1.HOSTNAME: (policy.hostname,),
                _NativeCommandSlotV1.HOST_USER: (
                    f"{capability.host_user[0]}:{capability.host_user[1]}",
                ),
                _NativeCommandSlotV1.CID_FILE: (str(request.cid_file),),
                _NativeCommandSlotV1.IMAGE_REFERENCE: (policy.image_reference,),
                _NativeCommandSlotV1.BOOTSTRAP: (policy.bootstrap,),
                _NativeCommandSlotV1.BOOTSTRAP_ARGV0: (policy.bootstrap_argv0,),
                _NativeCommandSlotV1.INPUT_LENGTH: (
                    str(request.input_bundle.length),
                ),
                _NativeCommandSlotV1.INPUT_SHA256: (
                    request.input_bundle.sha256.hex(),
                ),
            },
        )

    def run_build(
        self,
        request: DockerBuildRequestV1,
    ) -> DockerBuildProcessObservationV1:
        command = self.command_for(request)
        capability = request.capability
        policy = capability.policy
        return self._observe_command(
            command,
            stdout_limit=request.max_output_bytes,
            stderr_limit=policy.stderr_limit,
            timeout_ns=policy.build_timeout_ns,
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
        input_bundle: input.SealedInputV1 | None = None,
    ) -> _DockerCommandObservationV1:
        if (
            type(command) is not tuple
            or not command
            or any(type(item) is not str or not item or "\0" in item for item in command)
        ):
            raise TypeError("command must be a nonempty string tuple")
        try:
            tuple(os.fsencode(item) for item in command)
        except (TypeError, UnicodeEncodeError) as error:
            raise TypeError("command contains an unencodable coordinate") from error
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
            active_policy = (
                self._probed_capability.policy
                if self._probed_capability is not None
                else self._policy
            )
            _container_name(container_name, active_policy.container_name_prefix)
        if input_bundle is not None and type(input_bundle) is not input.SealedInputV1:
            raise TypeError("input_bundle must be controller sealed")
        if input_bundle is not None and not input.sealed_input_is_intact_v1(
            input_bundle
        ):
            return DockerBuildObserverFailureV1(
                "build input bytes are not intact",
                b"",
                b"",
            )
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
        except (OSError, UnicodeEncodeError):
            return DockerBuildObserverFailureV1(
                "cannot start Docker CLI",
                b"",
                b"",
            )
        stdout = bytearray()
        stderr = bytearray()
        selector: selectors.BaseSelector | None = None
        terminal: DockerOutputStreamV1 | None = None
        timed_out = False
        observer_failed = False
        input_failed = False
        written = 0
        input_hasher = hashlib.sha256()
        bundle_view: memoryview | None = None
        input_progress: BuildInputTransferProgressV1 | None = None
        stop_detail: str | None = None
        cleanup_detail: str | None = None
        input_descriptor: int | None = None
        stdout_descriptor: int | None = None
        stderr_descriptor: int | None = None
        try:
            if (
                process.stdout is None
                or process.stderr is None
                or (input_bundle is not None and process.stdin is None)
            ):
                observer_failed = True
                raise RuntimeError("Docker pipes unavailable")
            stdout_descriptor = process.stdout.fileno()
            stderr_descriptor = process.stderr.fileno()
            if process.stdin is not None:
                input_descriptor = process.stdin.fileno()
            selector = selectors.DefaultSelector()
            bundle_view = (
                memoryview(input_bundle.contents)
                if input_bundle is not None
                else None
            )
            streams = (
                (
                    stdout_descriptor,
                    DockerOutputStreamV1.STDOUT,
                    stdout,
                    stdout_limit,
                ),
                (
                    stderr_descriptor,
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
                timeout = min(
                    (deadline - now) / 1_000_000_000,
                    _POLL_SLICE_SECONDS_V1,
                )
                for key, _events in selector.select(timeout):
                    if key.data[0] == "read":
                        _kind, stream, target, maximum = key.data
                        try:
                            chunk = os.read(
                                key.fd,
                                min(
                                    _IO_CHUNK_BYTES_V1,
                                    maximum + 1 - len(target),
                                ),
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
                            bundle_view[written : written + _IO_CHUNK_BYTES_V1],
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
            if selector is not None:
                try:
                    selector.close()
                except Exception:
                    observer_failed = True
            if process.stdin is not None:
                if self._close_owned_stream(process.stdin, input_descriptor):
                    observer_failed = True
            if bundle_view is not None:
                try:
                    bundle_view.release()
                except Exception:
                    observer_failed = True
            if input_bundle is not None:
                try:
                    input_progress = _build_input_progress_v1(
                        input_bundle,
                        written,
                        input_hasher.digest(),
                    )
                except Exception:
                    observer_failed = True
            try:
                process_running = process.poll() is None
            except Exception:
                process_running = True
                stop_detail = "Docker CLI process state could not be observed"
            if process_running:
                if not (
                    timed_out
                    or terminal is not None
                    or observer_failed
                    or input_failed
                ):
                    timed_out = True
                try:
                    observed_stop = self._stop_process(process)
                except Exception:
                    observed_stop = "Docker CLI process termination raised"
                stop_detail = stop_detail or observed_stop
            for stream, descriptor in (
                (process.stdout, stdout_descriptor),
                (process.stderr, stderr_descriptor),
            ):
                if stream is None:
                    continue
                if self._close_owned_stream(stream, descriptor):
                    observer_failed = True
            if cid_file is not None and container_name is not None:
                try:
                    cleanup_detail = self._cleanup_container(
                        cid_file,
                        container_name,
                    )
                except Exception:
                    cleanup_detail = "Docker container cleanup observer raised"
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
            failures: list[CleanupFailureRecordV1] = []
            if stop_detail is not None:
                failures.append(
                    CleanupFailureRecordV1(
                        CleanupResourceV1.DOCKER_CLI_PROCESS,
                        stop_detail,
                    )
                )
            if cleanup_detail is not None:
                failures.append(
                    CleanupFailureRecordV1(
                        CleanupResourceV1.DOCKER_CONTAINER,
                        cleanup_detail,
                    )
                )
            return DockerBuildCleanupFailureV1(
                trigger,
                tuple(failures),
                bytes(stdout),
                bytes(stderr),
                input_progress,
            )
        if input_failed:
            if input_progress is None:
                return DockerBuildObserverFailureV1(
                    "build input progress could not be retained",
                    bytes(stdout),
                    bytes(stderr),
                    input_progress,
                )
            return DockerBuildInputRejectedV1(
                input_progress,
                bytes(stdout),
                bytes(stderr),
            )
        if observer_failed:
            return DockerBuildObserverFailureV1(
                "Docker output observation failed",
                bytes(stdout),
                bytes(stderr),
                input_progress,
            )
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
            return DockerBuildObserverFailureV1(
                "Docker returncode unavailable",
                bytes(stdout),
                bytes(stderr),
                input_progress,
            )
        if input_bundle is not None:
            if (
                input_progress is None
                or written != input_bundle.length
                or input_hasher.digest() != input_bundle.sha256
            ):
                return DockerBuildObserverFailureV1(
                    "completed build input transfer invariant failed",
                    bytes(stdout),
                    bytes(stderr),
                    input_progress,
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

    @staticmethod
    def _close_owned_stream(stream: object, descriptor: int | None) -> bool:
        """Close the file object, then its captured owned FD if close raised."""

        close_failed = False
        try:
            closed = stream.closed is True
        except Exception:
            closed = False
            close_failed = True
        if not closed:
            try:
                stream.close()
            except Exception:
                close_failed = True
        if close_failed and type(descriptor) is int and descriptor >= 0:
            try:
                os.close(descriptor)
            except Exception:
                pass
        return close_failed

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
            process.wait(timeout=_PROCESS_STOP_TIMEOUT_SECONDS_V1)
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
        if self._probed_capability is None:
            raise TypeError("Docker cleanup requires an observed capability")
        policy = self._probed_capability.policy
        return self._observe_command(
            command,
            stdout_limit=policy.probe_output_limit,
            stderr_limit=policy.probe_output_limit,
            timeout_ns=policy.probe_timeout_ns,
            cid_file=None,
        )

    def _cleanup_container(self, cid_file: Path, container_name: str) -> str | None:
        if self._probed_capability is None:
            raise TypeError("Docker cleanup requires an observed capability")
        capability = self._probed_capability
        policy = capability.policy
        _absolute_path(cid_file, "cid_file")
        _container_name(container_name, policy.container_name_prefix)
        container_id = self._admitted_container_id(cid_file)
        removal_coordinates = (
            (container_id, container_name)
            if container_id is not None
            else (container_name,)
        )
        try:
            for coordinate in removal_coordinates:
                self._observe_cleanup_command(
                    _render_native_command_v1(
                        "cleanup_rm",
                        capability.command_coordinate,
                        {
                            _NativeCommandSlotV1.CONTAINER_COORDINATE: (
                                coordinate,
                            ),
                        },
                    )
                )
            filters = [f"name=^/{container_name}$"]
            if container_id is not None:
                filters.append(f"id={container_id}")
            for filter_value in filters:
                observation = self._observe_cleanup_command(
                    _render_native_command_v1(
                        "cleanup_ls",
                        capability.command_coordinate,
                        {
                            _NativeCommandSlotV1.CONTAINER_FILTER: (
                                filter_value,
                            ),
                        },
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
    CONTRACT_VIOLATION = "contract_violation"
    PROCESS_FAILED = "process_failed"
    CLEANUP_FAILED = "cleanup_failed"
    INPUT_TRANSFER_FAILED = "input_transfer_failed"
    TIMEOUT = "timeout"
    OUTPUT_LIMIT = "output_limit"
    OBSERVER_FAILURE = "observer_failure"
    INVALID_OUTPUT = "invalid_output"


BuildAttemptObservationV1: TypeAlias = (
    DockerBuildProcessObservationV1 | BuildCleanupFailureV1
)


class BuildSessionV1(tuple):
    """Owned coordinates shared by every attempt in one two-build session."""

    __slots__ = ()

    def __new__(
        cls,
        capability: DockerSupportedV1,
        input_value: input.SealedInputV1,
        max_output_bytes: int,
        *,
        _token: object,
    ) -> BuildSessionV1:
        if (
            _token is not _BUILD_SESSION_TOKEN
            or not _docker_supported_is_valid_v1(capability)
            or not input.sealed_input_is_intact_v1(input_value)
            or type(max_output_bytes) is not int
            or max_output_bytes <= 0
            or max_output_bytes > capability.policy.stdout_limit
        ):
            raise TypeError("invalid two-build session coordinates")
        return tuple.__new__(
            cls,
            (
                capability,
                input_value,
                max_output_bytes,
            ),
        )

    @property
    def policy(self) -> DockerBuildPolicyV1:
        return self.capability.policy

    @property
    def capability(self) -> DockerSupportedV1:
        return self[0]

    @property
    def input_value(self) -> input.SealedInputV1:
        value = self[1]
        if not input.sealed_input_is_intact_v1(value):
            raise RuntimeError("build session lost exact input bytes")
        return value

    @property
    def max_output_bytes(self) -> int:
        return self[2]


def _build_session_v1(
    capability: DockerSupportedV1,
    input_value: input.SealedInputV1,
    max_output_bytes: int,
) -> BuildSessionV1:
    return BuildSessionV1(
        capability,
        input_value,
        max_output_bytes,
        _token=_BUILD_SESSION_TOKEN,
    )


def _build_session_is_valid_v1(value: object) -> bool:
    if type(value) is not BuildSessionV1:
        return False
    try:
        canonical = _build_session_v1(
            value.capability,
            value.input_value,
            value.max_output_bytes,
        )
        return tuple(canonical) == tuple(value)
    except Exception:
        return False


class BuildByteRelationV1(StrEnum):
    IDENTICAL = "identical"
    DIFFERENT = "different"


class TwoBuildObservationV1(tuple):
    """Two fresh successful attempts and their observed byte relation."""

    __slots__ = ()

    def __new__(
        cls,
        session: BuildSessionV1,
        processes: tuple[DockerBuildExitedV1, DockerBuildExitedV1],
        *,
        _token: object,
    ) -> TwoBuildObservationV1:
        if (
            _token is not _TWO_BUILD_OBSERVATION_TOKEN
            or not _build_session_is_valid_v1(session)
            or type(processes) is not tuple
            or len(processes) != 2
        ):
            raise TypeError("invalid two-build observation")
        owned: list[DockerBuildExitedV1] = []
        for process in processes:
            canonical = _canonical_process_observation_v1(
                process,
                session.input_value,
                session.max_output_bytes,
                session.policy.stderr_limit,
            )
            if type(canonical) is not DockerBuildExitedV1 or canonical.returncode != 0:
                raise TypeError("two-build observation requires successful exits")
            owned.append(canonical)
        owned_processes = (owned[0], owned[1])
        relation = (
            BuildByteRelationV1.IDENTICAL
            if owned_processes[0].stdout == owned_processes[1].stdout
            else BuildByteRelationV1.DIFFERENT
        )
        return tuple.__new__(cls, (session, relation, owned_processes))

    @property
    def session(self) -> BuildSessionV1:
        return self[0]

    @property
    def relation(self) -> BuildByteRelationV1:
        return self[1]

    @property
    def policy(self) -> DockerBuildPolicyV1:
        return self.session.policy

    @property
    def capability(self) -> DockerSupportedV1:
        return self.session.capability

    @property
    def input_value(self) -> input.SealedInputV1:
        return self.session.input_value

    @property
    def max_output_bytes(self) -> int:
        return self.session.max_output_bytes

    @property
    def processes(
        self,
    ) -> tuple[DockerBuildExitedV1, DockerBuildExitedV1]:
        return self[2]

    @property
    def outputs(self) -> tuple[bytes, bytes]:
        return self.processes[0].stdout, self.processes[1].stdout

    @property
    def first_sha256(self) -> bytes:
        return hashlib.sha256(self.outputs[0]).digest()

    @property
    def second_sha256(self) -> bytes:
        return hashlib.sha256(self.outputs[1]).digest()


class BuildRejectedV1(tuple):
    """Typed failed attempt retaining the successful causal prefix."""

    __slots__ = ()

    def __new__(
        cls,
        attempt: int,
        reason: BuildFailureReasonV1,
        process: BuildAttemptObservationV1 | None = None,
        *,
        session: BuildSessionV1 | None = None,
        completed_processes: tuple[DockerBuildExitedV1, ...] = (),
    ) -> BuildRejectedV1:
        if (
            type(attempt) is not int
            or attempt not in (1, 2)
            or type(reason) is not BuildFailureReasonV1
            or type(completed_processes) is not tuple
        ):
            raise TypeError("invalid build rejection")
        if session is None:
            if (
                reason is not BuildFailureReasonV1.CONTRACT_VIOLATION
                or process is not None
                or completed_processes
            ):
                raise TypeError("context-free rejection must be a contract violation")
            return tuple.__new__(cls, (attempt, reason, None, None, ()))
        if (
            not _build_session_is_valid_v1(session)
            or len(completed_processes) != attempt - 1
        ):
            raise TypeError("build rejection lost its causal prefix")
        owned_completed: list[DockerBuildExitedV1] = []
        for completed in completed_processes:
            canonical = _canonical_process_observation_v1(
                completed,
                session.input_value,
                session.max_output_bytes,
                session.policy.stderr_limit,
            )
            if type(canonical) is not DockerBuildExitedV1 or canonical.returncode != 0:
                raise TypeError("causal prefix contains a failed attempt")
            owned_completed.append(canonical)
        if process is None:
            owned_process: BuildAttemptObservationV1 | None = None
        elif type(process) is BuildCleanupFailureV1:
            owned_process = _canonical_build_cleanup_failure_v1(
                process,
                session.input_value,
                session.max_output_bytes,
                session.policy.stderr_limit,
            )
        else:
            owned_process = _canonical_process_observation_v1(
                process,
                session.input_value,
                session.max_output_bytes,
                session.policy.stderr_limit,
            )
        expected_process_types: dict[BuildFailureReasonV1, tuple[type, ...]] = {
            BuildFailureReasonV1.CONTRACT_VIOLATION: (),
            BuildFailureReasonV1.PROCESS_FAILED: (DockerBuildExitedV1,),
            BuildFailureReasonV1.CLEANUP_FAILED: (
                DockerBuildCleanupFailureV1,
                BuildCleanupFailureV1,
            ),
            BuildFailureReasonV1.INPUT_TRANSFER_FAILED: (
                DockerBuildInputRejectedV1,
            ),
            BuildFailureReasonV1.TIMEOUT: (DockerBuildTimedOutV1,),
            BuildFailureReasonV1.OUTPUT_LIMIT: (DockerBuildOutputLimitV1,),
            BuildFailureReasonV1.OBSERVER_FAILURE: (
                DockerBuildObserverFailureV1,
            ),
            BuildFailureReasonV1.INVALID_OUTPUT: (DockerBuildExitedV1,),
        }
        expected = expected_process_types[reason]
        if not expected:
            if owned_process is not None:
                raise TypeError("contract-violation rejection cannot retain authority")
        elif type(owned_process) not in expected:
            raise TypeError("build rejection reason and observation disagree")
        if (
            (
                reason is BuildFailureReasonV1.PROCESS_FAILED
                and owned_process.returncode == 0
            )
            or (
                reason is BuildFailureReasonV1.INVALID_OUTPUT
                and owned_process.returncode != 0
            )
        ):
            raise TypeError("build rejection exit status disagrees with reason")
        return tuple.__new__(
            cls,
            (
                attempt,
                reason,
                owned_process,
                session,
                tuple(owned_completed),
            ),
        )

    @property
    def attempt(self) -> int:
        return self[0]

    @property
    def reason(self) -> BuildFailureReasonV1:
        return self[1]

    @property
    def process(self) -> BuildAttemptObservationV1 | None:
        return self[2]

    @property
    def session(self) -> BuildSessionV1 | None:
        return self[3]

    @property
    def completed_processes(self) -> tuple[DockerBuildExitedV1, ...]:
        return self[4]


def two_build_observation_matches_v1(
    value: object,
    session: BuildSessionV1,
) -> bool:
    if (
        type(value) is not TwoBuildObservationV1
        or not _build_session_is_valid_v1(session)
    ):
        return False
    try:
        replayed = TwoBuildObservationV1(
            session,
            value.processes,
            _token=_TWO_BUILD_OBSERVATION_TOKEN,
        )
        return tuple(replayed) == tuple(value)
    except Exception:
        return False


BuildTransportResultV1: TypeAlias = (
    TwoBuildObservationV1 | BuildRejectedV1
)


class ControlledBuildTransportV1:
    """Own two fresh attempts; callers own semantic input and output admission."""

    def __init__(
        self,
        *,
        policy: DockerBuildPolicyV1,
        backend: DockerBuildBackendV1,
    ) -> None:
        if not docker_policy_is_valid_v1(policy):
            raise TypeError("policy must be DockerBuildPolicyV1")
        self._policy = DockerBuildPolicyV1(*tuple(policy))
        self._backend = backend
        self._probed_capability: DockerSupportedV1 | None = None
        self._consumed = False

    def probe(self) -> DockerCapabilityReportV1:
        if self._consumed or self._probed_capability is not None:
            return DockerUnsupportedV1(
                DockerBlockerReasonV1.BACKEND_CONTRACT,
                "build transport capability is one-shot",
            )
        try:
            report = self._backend.probe()
        except Exception:
            return DockerUnsupportedV1(
                DockerBlockerReasonV1.BACKEND_CONTRACT,
                "Docker capability probe raised",
            )
        if type(report) is DockerUnsupportedV1:
            if _docker_unsupported_is_valid_v1(report):
                return DockerUnsupportedV1(*tuple(report))
            return DockerUnsupportedV1(
                DockerBlockerReasonV1.BACKEND_CONTRACT,
                "Docker capability rejection is not canonical",
            )
        if (
            not _docker_supported_is_valid_v1(report)
            or report.policy != self._policy
        ):
            return DockerUnsupportedV1(
                DockerBlockerReasonV1.BACKEND_CONTRACT,
                "Docker capability report does not match build policy",
            )
        self._probed_capability = report
        return report

    def build(
        self,
        capability: DockerSupportedV1,
        input_value: input.SealedInputV1,
        max_output_bytes: int,
        *,
        input_admission: Callable[[input.SealedInputV1], bool],
        output_admission: Callable[[bytes], bool],
    ) -> BuildTransportResultV1:
        if (
            self._consumed
            or capability is not self._probed_capability
            or not _docker_supported_is_valid_v1(capability)
            or capability.policy != self._policy
        ):
            return BuildRejectedV1(1, BuildFailureReasonV1.CONTRACT_VIOLATION)
        self._consumed = True
        if (
            type(max_output_bytes) is not int
            or max_output_bytes <= 0
            or max_output_bytes > capability.policy.stdout_limit
            or not callable(input_admission)
            or not callable(output_admission)
            or not input.sealed_input_is_intact_v1(input_value)
        ):
            return BuildRejectedV1(1, BuildFailureReasonV1.CONTRACT_VIOLATION)
        session = _build_session_v1(
            capability,
            input_value,
            max_output_bytes,
        )
        completed: list[DockerBuildExitedV1] = []
        for attempt in (1, 2):
            if (
                not input.sealed_input_is_intact_v1(input_value)
                or not self._admitted(input_admission, input_value)
            ):
                return BuildRejectedV1(
                    attempt,
                    BuildFailureReasonV1.CONTRACT_VIOLATION,
                    session=session,
                    completed_processes=tuple(completed),
                )
            built = self._build_once(
                attempt,
                session,
                output_admission,
                tuple(completed),
            )
            if type(built) is BuildRejectedV1:
                return built
            completed.append(built)
        return TwoBuildObservationV1(
            session,
            (completed[0], completed[1]),
            _token=_TWO_BUILD_OBSERVATION_TOKEN,
        )

    @staticmethod
    def _admitted(
        admission: Callable[[object], bool],
        value: object,
    ) -> bool:
        try:
            return admission(value) is True
        except Exception:
            return False

    def _build_once(
        self,
        attempt: int,
        session: BuildSessionV1,
        output_admission: Callable[[bytes], bool],
        completed_processes: tuple[DockerBuildExitedV1, ...],
    ) -> DockerBuildExitedV1 | BuildRejectedV1:
        if not _build_session_is_valid_v1(session):
            return BuildRejectedV1(
                attempt,
                BuildFailureReasonV1.CONTRACT_VIOLATION,
            )
        contract_rejection = BuildRejectedV1(
            attempt,
            BuildFailureReasonV1.CONTRACT_VIOLATION,
            session=session,
            completed_processes=completed_processes,
        )
        try:
            temporary_root = tempfile.TemporaryDirectory(
                prefix=f"{session.policy.container_name_prefix}{attempt}-"
            )
        except Exception:
            return contract_rejection
        current_process: DockerBuildProcessObservationV1 | None = None
        try:
            result, current_process = self._observe_build_attempt_v1(
                attempt,
                session,
                output_admission,
                completed_processes,
                Path(temporary_root.name).resolve(),
            )
        except Exception:
            result = contract_rejection
        try:
            temporary_root.cleanup()
        except Exception:
            try:
                cleanup = _build_cleanup_failure_v1(
                    current_process,
                    "temporary build root cleanup failed",
                )
                return BuildRejectedV1(
                    attempt,
                    BuildFailureReasonV1.CLEANUP_FAILED,
                    cleanup,
                    session=session,
                    completed_processes=completed_processes,
                )
            except Exception:
                return contract_rejection
        return result

    def _observe_build_attempt_v1(
        self,
        attempt: int,
        session: BuildSessionV1,
        output_admission: Callable[[bytes], bool],
        completed_processes: tuple[DockerBuildExitedV1, ...],
        root: Path,
    ) -> tuple[
        DockerBuildExitedV1 | BuildRejectedV1,
        DockerBuildProcessObservationV1 | None,
    ]:
        contract_rejection = BuildRejectedV1(
            attempt,
            BuildFailureReasonV1.CONTRACT_VIOLATION,
            session=session,
            completed_processes=completed_processes,
        )
        request = DockerBuildRequestV1(
            attempt,
            session.capability,
            session.input_value,
            session.max_output_bytes,
            root / "container.cid",
            session.policy.container_name_prefix
            + hashlib.sha256(
                os.fsencode(root) + bytes((attempt,))
            ).hexdigest(),
        )
        try:
            observed = self._backend.run_build(request)
        except Exception:
            return contract_rejection, None
        try:
            process = _canonical_process_observation_v1(
                observed,
                session.input_value,
                session.max_output_bytes,
                session.policy.stderr_limit,
            )
        except TypeError:
            return contract_rejection, None
        reason_by_type: dict[type, BuildFailureReasonV1] = {
            DockerBuildCleanupFailureV1: BuildFailureReasonV1.CLEANUP_FAILED,
            DockerBuildInputRejectedV1: BuildFailureReasonV1.INPUT_TRANSFER_FAILED,
            DockerBuildTimedOutV1: BuildFailureReasonV1.TIMEOUT,
            DockerBuildOutputLimitV1: BuildFailureReasonV1.OUTPUT_LIMIT,
            DockerBuildObserverFailureV1: BuildFailureReasonV1.OBSERVER_FAILURE,
        }
        failure_reason = reason_by_type.get(type(process))
        if failure_reason is not None:
            return (
                BuildRejectedV1(
                    attempt,
                    failure_reason,
                    process,
                    session=session,
                    completed_processes=completed_processes,
                ),
                process,
            )
        if type(process) is not DockerBuildExitedV1:
            return contract_rejection, None
        if not docker_build_exited_is_valid_v1(
            process,
            session.input_value,
            session.max_output_bytes,
            session.policy.stderr_limit,
        ):
            return contract_rejection, None
        if process.returncode != 0:
            return (
                BuildRejectedV1(
                    attempt,
                    BuildFailureReasonV1.PROCESS_FAILED,
                    process,
                    session=session,
                    completed_processes=completed_processes,
                ),
                process,
            )
        transfer = process.input_transfer
        if (
            type(transfer) is not BuildInputTransferV1
            or transfer.bundle_identity != session.input_value.binding_identity
            or transfer.expected_length != session.input_value.length
            or transfer.expected_sha256 != session.input_value.sha256
            or transfer.written_length != session.input_value.length
            or transfer.written_sha256 != session.input_value.sha256
        ):
            return contract_rejection, None
        if not self._admitted(output_admission, process.stdout):
            return (
                BuildRejectedV1(
                    attempt,
                    BuildFailureReasonV1.INVALID_OUTPUT,
                    process,
                    session=session,
                    completed_processes=completed_processes,
                ),
                process,
            )
        return process, process
