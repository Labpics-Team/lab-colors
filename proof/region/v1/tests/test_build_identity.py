#!/usr/bin/env python3
"""RED contract for orthogonal Docker BUILD capability identities."""

from __future__ import annotations

import hashlib
import inspect
import json
import os
import subprocess
import sys
import unittest
from pathlib import Path
from unittest import mock


PROOF = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PROOF))

from build import input as build_input  # noqa: E402
from build import transport  # noqa: E402


_POLICY_FIELDS_V1 = (
    "image_reference",
    "platform",
    "hostname",
    "bootstrap",
    "bootstrap_argv0",
    "tmpfs_specs",
    "user_mode",
    "stdout_limit",
    "stderr_limit",
    "build_timeout_ns",
    "probe_output_limit",
    "probe_timeout_ns",
)

def _literal(value: str) -> tuple[str, str]:
    return "literal", value


def _slot(value: str) -> tuple[str, str]:
    return "slot", value


_NATIVE_COMMAND_TEMPLATES_V1 = (
    (
        "version_probe",
        (
            _slot("cli_path"),
            _literal("version"),
            _literal("--format"),
            _literal("{{json .Server}}"),
        ),
    ),
    (
        "image_inspect",
        (
            _slot("cli_path"),
            _literal("image"),
            _literal("inspect"),
            _slot("image_reference"),
        ),
    ),
    (
        "build",
        (
            _slot("cli_path"),
            _literal("run"),
            _literal("--rm"),
            _literal("--interactive"),
            _literal("--pull"),
            _literal("never"),
            _literal("--platform"),
            _slot("platform"),
            _literal("--network"),
            _literal("none"),
            _literal("--read-only"),
            _literal("--tmpfs"),
            _slot("ordered_tmpfs_specs"),
            _literal("--cap-drop"),
            _literal("ALL"),
            _literal("--security-opt"),
            _literal("no-new-privileges:true"),
            _literal("--hostname"),
            _slot("hostname"),
            _literal("--user"),
            _slot("host_user"),
            _literal("--workdir"),
            _literal("/"),
            _literal("--cidfile"),
            _slot("cid_file"),
            _literal("--entrypoint"),
            _literal("/usr/bin/env"),
            _slot("image_reference"),
            _literal("-i"),
            _literal("PATH=/usr/local/bin:/usr/bin:/bin"),
            _literal("LC_ALL=C"),
            _literal("LANG=C"),
            _literal("TZ=UTC"),
            _literal("HOME=/nonexistent"),
            _literal("/bin/sh"),
            _literal("-c"),
            _slot("bootstrap"),
            _slot("bootstrap_argv0"),
            _slot("input_length"),
            _slot("input_sha256"),
        ),
    ),
    (
        "cleanup_rm",
        (
            _slot("cli_path"),
            _literal("container"),
            _literal("rm"),
            _literal("--force"),
            _slot("container_coordinate"),
        ),
    ),
    (
        "cleanup_inspect",
        (
            _slot("cli_path"),
            _literal("container"),
            _literal("inspect"),
            _literal("--format"),
            _literal("{{.Id}}"),
            _slot("container_coordinate"),
        ),
    ),
    (
        "cleanup_ls",
        (
            _slot("cli_path"),
            _literal("container"),
            _literal("ls"),
            _literal("--all"),
            _literal("--quiet"),
            _literal("--no-trunc"),
            _literal("--filter"),
            _slot("container_filter"),
        ),
    ),
)

_NATIVE_PROCESS_ENVIRONMENT_V1 = (
    ("DOCKER_CONFIG", "/nonexistent"),
    ("HOME", "/nonexistent"),
    ("LANG", "C"),
    ("LC_ALL", "C"),
    ("PATH", "/usr/bin:/bin"),
    ("TZ", "UTC"),
)
_NATIVE_PROCESS_CWD_V1 = "/"
_NATIVE_PROCESS_UMASK_V1 = 0o077
_NATIVE_PROCESS_CLOSE_FDS_V1 = True
_NATIVE_PROCESS_RESTORE_SIGNALS_V1 = True
_NATIVE_PROCESS_START_NEW_SESSION_V1 = True
_NATIVE_PROCESS_STDIN_WITH_INPUT_V1 = "pipe"
_NATIVE_PROCESS_STDIN_WITHOUT_INPUT_V1 = "devnull"
_NATIVE_PROCESS_STDOUT_V1 = "pipe"
_NATIVE_PROCESS_STDERR_V1 = "pipe"


# This literal oracle intentionally does not call production encoders: changing
# a production preimage silently must turn a test failure, not rewrite its proof.
def _blob(value: bytes) -> bytes:
    return len(value).to_bytes(8, "big") + value


def _identity(label: bytes, chunks: tuple[bytes, ...]) -> bytes:
    payload = b"".join(_blob(chunk) for chunk in chunks)
    return hashlib.sha256(
        label + len(payload).to_bytes(8, "big") + payload
    ).digest()


def _policy_coordinates(policy: object) -> dict[str, object]:
    return {name: getattr(policy, name) for name in _POLICY_FIELDS_V1}


def _policy_chunks(coordinates: dict[str, object]) -> tuple[bytes, ...]:
    tmpfs_specs = coordinates["tmpfs_specs"]
    user_mode = coordinates["user_mode"]
    if type(tmpfs_specs) is not tuple:
        raise TypeError("tmpfs_specs must be an exact tuple")
    return (
        coordinates["image_reference"].encode("utf-8"),
        coordinates["platform"].encode("utf-8"),
        coordinates["hostname"].encode("utf-8"),
        coordinates["bootstrap"].encode("utf-8"),
        coordinates["bootstrap_argv0"].encode("utf-8"),
        len(tmpfs_specs).to_bytes(4, "big"),
        *(item.encode("utf-8") for item in tmpfs_specs),
        user_mode.value.encode("ascii"),
        coordinates["stdout_limit"].to_bytes(8, "big"),
        coordinates["stderr_limit"].to_bytes(8, "big"),
        coordinates["build_timeout_ns"].to_bytes(8, "big"),
        coordinates["probe_output_limit"].to_bytes(8, "big"),
        coordinates["probe_timeout_ns"].to_bytes(8, "big"),
    )


def _expected_policy_identity(coordinates: dict[str, object]) -> bytes:
    return _identity(
        b"labcolors.proof-region.docker-transport-policy.v1\0",
        _policy_chunks(coordinates),
    )


def _expected_command_contract_identity() -> bytes:
    chunks: list[bytes] = [len(_NATIVE_COMMAND_TEMPLATES_V1).to_bytes(4, "big")]
    for name, tokens in _NATIVE_COMMAND_TEMPLATES_V1:
        chunks.extend((name.encode("ascii"), len(tokens).to_bytes(4, "big")))
        for tag, value in tokens:
            chunks.extend((tag.encode("ascii"), value.encode("utf-8")))
    chunks.extend(
        (
            b"native-process-context.v1",
            len(_NATIVE_PROCESS_ENVIRONMENT_V1).to_bytes(4, "big"),
            *(
                item
                for key, value in _NATIVE_PROCESS_ENVIRONMENT_V1
                for item in (key.encode("ascii"), value.encode("utf-8"))
            ),
            _NATIVE_PROCESS_CWD_V1.encode("ascii"),
            _NATIVE_PROCESS_UMASK_V1.to_bytes(4, "big"),
            bytes((_NATIVE_PROCESS_CLOSE_FDS_V1,)),
            bytes((_NATIVE_PROCESS_RESTORE_SIGNALS_V1,)),
            bytes((_NATIVE_PROCESS_START_NEW_SESSION_V1,)),
            b"native-stdio-topology.v1",
            b"stdin-with-input",
            _NATIVE_PROCESS_STDIN_WITH_INPUT_V1.encode("ascii"),
            b"stdin-without-input",
            _NATIVE_PROCESS_STDIN_WITHOUT_INPUT_V1.encode("ascii"),
            b"stdout",
            _NATIVE_PROCESS_STDOUT_V1.encode("ascii"),
            b"stderr",
            _NATIVE_PROCESS_STDERR_V1.encode("ascii"),
        )
    )
    return _identity(
        b"labcolors.proof-region.native-command-contract.v1\0",
        tuple(chunks),
    )


def _expected_command_coordinate(docker_path: Path) -> bytes:
    return _identity(
        b"labcolors.proof-region.native-command-coordinate.v1\0",
        (_expected_command_contract_identity(), os.fsencode(docker_path)),
    )


def _expected_daemon_identity(
    server_stdout: bytes,
    image_inspect_stdout: bytes,
) -> bytes:
    return _identity(
        b"labcolors.proof-region.docker-daemon-observation.v1\0",
        (server_stdout, image_inspect_stdout),
    )


def _expected_host_user_identity(host_user: tuple[int, int]) -> bytes:
    return _identity(
        b"labcolors.proof-region.host-user.v1\0",
        (
            host_user[0].to_bytes(4, "big"),
            host_user[1].to_bytes(4, "big"),
        ),
    )


def _expected_capability_identity(
    policy_identity: bytes,
    daemon_identity: bytes,
    command_coordinate: bytes,
    host_user: tuple[int, int],
) -> bytes:
    return _identity(
        b"labcolors.proof-region.docker-capability.v1\0",
        (
            policy_identity,
            command_coordinate,
            daemon_identity,
            host_user[0].to_bytes(4, "big"),
            host_user[1].to_bytes(4, "big"),
        ),
    )


def _policy(**changes: object) -> object:
    coordinates: dict[str, object] = {
        "image_reference": (
            "registry.example/toolchain@sha256:" + "1" * 64
        ),
        "platform": "linux/amd64",
        "hostname": "lc-build",
        "bootstrap": "set -eu\ncat",
        "bootstrap_argv0": "labcolors-build-v1",
        "tmpfs_specs": (
            "/work:rw,nosuid,nodev,noexec,size=1048576",
            "/tmp:rw,nosuid,nodev,noexec,size=2097152",
        ),
        "user_mode": transport.DockerUserModeV1.HOST_EFFECTIVE_IDS,
        "stdout_limit": 4096,
        "stderr_limit": 2048,
        "build_timeout_ns": 5_000_000_000,
        "probe_output_limit": 1024,
        "probe_timeout_ns": 1_000_000_000,
    }
    coordinates.update(changes)
    return transport.DockerBuildPolicyV1(**coordinates)


def _daemon(
    *,
    server_stdout: bytes = b'{"Version":"identity-test"}\n',
    image_inspect_stdout: bytes = b'[{"Id":"sha256:identity-test"}]\n',
) -> object:
    return transport.DockerDaemonObservationV1(
        server_stdout,
        image_inspect_stdout,
    )


def _capability(
    *,
    policy: object | None = None,
    daemon: object | None = None,
    docker_path: Path = Path("/usr/bin/true"),
    host_user: tuple[int, int] = (501, 20),
) -> object:
    owned_policy = _policy() if policy is None else policy
    owned_daemon = _daemon() if daemon is None else daemon
    return transport.DockerSupportedV1(
        owned_policy,
        owned_daemon,
        transport.native_command_coordinate_v1(docker_path),
        host_user,
    )


def _sealed_input() -> object:
    return build_input.seal_input_v1(
        hashlib.sha256(b"build-identity-test-binding").digest(),
        b"identity-test-input",
    )


def _assert_deeply_immutable(
    case: unittest.TestCase,
    value: object,
) -> None:
    case.assertFalse(hasattr(value, "__dict__"), type(value).__name__)
    with case.assertRaises((AttributeError, TypeError)):
        value[0] = value[0]
    with case.assertRaises((AttributeError, TypeError)):
        object.__setattr__(value, "foreign", object())


class _AlternateUserMode:
    """Test-only value with the encoder surface of the closed production enum."""

    def __init__(self, value: str) -> None:
        self.value = value


class BuildIdentitySurfaceTests(unittest.TestCase):
    def test_identity_surface_is_exact_and_has_no_legacy_report_aliases(self) -> None:
        for name in (
            "DockerDaemonObservationV1",
            "NativeCommandCoordinateV1",
            "transport_policy_identity_v1",
            "native_command_contract_identity_v1",
            "native_command_coordinate_v1",
            "docker_capability_identity_v1",
        ):
            with self.subTest(required=name):
                self.assertTrue(hasattr(transport, name), name)

        self.assertEqual(
            tuple(inspect.signature(transport.DockerDaemonObservationV1).parameters),
            ("server_stdout", "image_inspect_stdout"),
        )
        self.assertEqual(
            tuple(inspect.signature(transport.DockerSupportedV1).parameters),
            (
                "policy",
                "daemon_observation",
                "command_coordinate",
                "host_user",
            ),
        )
        self.assertEqual(
            tuple(inspect.signature(transport.DockerBuildRequestV1).parameters),
            (
                "attempt",
                "capability",
                "input_bundle",
                "max_output_bytes",
            ),
        )
        self.assertFalse(hasattr(transport, "docker_report_matches_policy_v1"))
        self.assertNotIn(
            "docker_report",
            Path(transport.__file__).read_text(encoding="utf-8"),
        )

        capability = _capability()
        request = transport.DockerBuildRequestV1(
            1,
            capability,
            _sealed_input(),
            64,
        )
        for legacy in (
            "image_reference",
            "platform",
            "daemon_observation_sha256",
        ):
            with self.subTest(legacy_capability_property=legacy):
                self.assertFalse(hasattr(capability, legacy))
        self.assertFalse(hasattr(request, "policy"))
        self.assertFalse(hasattr(request, "cid_file"))
        self.assertFalse(hasattr(request, "container_name"))
        self.assertIs(request.capability, capability)

        with self.assertRaises(TypeError):
            transport.DockerSupportedV1(
                capability.policy.image_reference,
                capability.policy.platform,
                capability.daemon_observation.identity,
                capability.host_user,
            )

    def test_policy_identity_binds_all_twelve_coordinates(self) -> None:
        policy = _policy()
        coordinates = _policy_coordinates(policy)
        identity = transport.transport_policy_identity_v1(policy)

        self.assertEqual(identity, _expected_policy_identity(coordinates))
        self.assertIs(type(identity), bytes)
        self.assertEqual(len(identity), 32)

        # V1 has one admitted platform and one admitted user mode.  Their
        # mutation cannot be represented as a valid policy, so the independent
        # literal preimage and source guard prove that neither closed-domain
        # coordinate silently disappears from the versioned identity.
        source = inspect.getsource(transport.transport_policy_identity_v1)
        for field_name in _POLICY_FIELDS_V1:
            with self.subTest(source_coordinate=field_name):
                self.assertIn(f".{field_name}", source)

        raw_mutations: dict[str, object] = {
            "image_reference": (
                "registry.example/toolchain@sha256:" + "2" * 64
            ),
            "platform": "linux/arm64",
            "hostname": "lc-build-alt",
            "bootstrap": "set -eu\nprintf changed",
            "bootstrap_argv0": "labcolors-build-v1-alt",
            "tmpfs_specs": coordinates["tmpfs_specs"] + ("/run:rw,size=4096",),
            "user_mode": _AlternateUserMode("explicit_ids"),
            "stdout_limit": coordinates["stdout_limit"] + 1,
            "stderr_limit": coordinates["stderr_limit"] + 1,
            "build_timeout_ns": coordinates["build_timeout_ns"] + 1,
            "probe_output_limit": coordinates["probe_output_limit"] + 1,
            "probe_timeout_ns": coordinates["probe_timeout_ns"] + 1,
        }
        for field_name, changed_value in raw_mutations.items():
            with self.subTest(preimage_coordinate=field_name):
                changed = dict(coordinates)
                changed[field_name] = changed_value
                self.assertNotEqual(
                    _expected_policy_identity(changed),
                    _expected_policy_identity(coordinates),
                )

        valid_mutations = {
            key: value
            for key, value in raw_mutations.items()
            if key not in ("platform", "user_mode")
        }
        for field_name, changed_value in valid_mutations.items():
            with self.subTest(runtime_coordinate=field_name):
                changed_policy = _policy(**{field_name: changed_value})
                self.assertNotEqual(
                    transport.transport_policy_identity_v1(changed_policy),
                    identity,
                )

    def test_literal_oracle_rejects_non_tuple_without_asserts(self) -> None:
        coordinates = _policy_coordinates(_policy())
        coordinates["tmpfs_specs"] = object()

        with self.assertRaises(TypeError):
            _policy_chunks(coordinates)


class BuildCapabilityIdentityTests(unittest.TestCase):
    def test_daemon_identity_owns_only_the_two_raw_probe_outputs(self) -> None:
        server_stdout = b'{"Version":"26.1.4","Os":"linux"}\n'
        image_stdout = b'[{"Os":"linux","Architecture":"amd64"}]\n'
        daemon = transport.DockerDaemonObservationV1(
            server_stdout,
            image_stdout,
        )

        self.assertEqual(daemon.server_stdout, server_stdout)
        self.assertEqual(daemon.image_inspect_stdout, image_stdout)
        self.assertEqual(
            daemon.identity,
            _expected_daemon_identity(server_stdout, image_stdout),
        )
        self.assertEqual(
            transport.DockerDaemonObservationV1(
                server_stdout,
                image_stdout,
            ).identity,
            daemon.identity,
        )
        self.assertNotEqual(
            transport.DockerDaemonObservationV1(
                server_stdout + b" ",
                image_stdout,
            ).identity,
            daemon.identity,
        )
        self.assertNotEqual(
            transport.DockerDaemonObservationV1(
                server_stdout,
                image_stdout + b" ",
            ).identity,
            daemon.identity,
        )
        _assert_deeply_immutable(self, daemon)

    def test_native_command_coordinate_binds_path_and_literal_template(self) -> None:
        first_path = Path("/usr/bin/true")
        second_path = Path("/usr/bin/false")
        expected_contract = _expected_command_contract_identity()
        first = transport.native_command_coordinate_v1(first_path)
        second = transport.native_command_coordinate_v1(second_path)

        self.assertEqual(
            transport.native_command_contract_identity_v1(),
            expected_contract,
        )
        self.assertEqual(
            transport.native_command_contract_identity_v1(),
            transport.native_command_contract_identity_v1(),
        )
        self.assertIs(type(first), transport.NativeCommandCoordinateV1)
        self.assertEqual(first.path, first_path)
        self.assertEqual(first.path_bytes, os.fsencode(first_path))
        self.assertEqual(first.command_contract_identity, expected_contract)
        self.assertEqual(first.identity, _expected_command_coordinate(first_path))
        self.assertEqual(second.path, second_path)
        self.assertEqual(second.identity, _expected_command_coordinate(second_path))
        self.assertNotEqual(first.identity, second.identity)
        _assert_deeply_immutable(self, first)

    def test_capability_identity_keeps_policy_daemon_path_and_user_orthogonal(self) -> None:
        policy = _policy()
        daemon = _daemon()
        command_coordinate = transport.native_command_coordinate_v1(
            Path("/usr/bin/true")
        )
        host_user = (501, 20)
        capability = transport.DockerSupportedV1(
            policy,
            daemon,
            command_coordinate,
            host_user,
        )
        expected_policy_identity = transport.transport_policy_identity_v1(policy)
        expected = _expected_capability_identity(
            expected_policy_identity,
            daemon.identity,
            command_coordinate.identity,
            host_user,
        )

        self.assertIs(capability.policy, policy)
        self.assertIs(capability.daemon_observation, daemon)
        self.assertIs(capability.command_coordinate, command_coordinate)
        self.assertEqual(capability.host_user, host_user)
        self.assertEqual(capability.policy_identity, expected_policy_identity)
        self.assertEqual(
            capability.daemon_observation_identity,
            daemon.identity,
        )
        self.assertEqual(
            capability.command_coordinate_identity,
            command_coordinate.identity,
        )
        self.assertEqual(
            capability.host_user_identity,
            _expected_host_user_identity(host_user),
        )
        self.assertEqual(capability.identity, expected)
        self.assertEqual(
            transport.docker_capability_identity_v1(capability),
            expected,
        )

        variants = (
            _capability(policy=_policy(hostname="lc-build-other"), daemon=daemon),
            _capability(
                policy=policy,
                daemon=_daemon(server_stdout=b'{"Version":"other"}\n'),
            ),
            _capability(policy=policy, daemon=daemon, docker_path=Path("/bin/sh")),
            _capability(policy=policy, daemon=daemon, host_user=(502, 20)),
            _capability(policy=policy, daemon=daemon, host_user=(501, 21)),
        )
        for variant in variants:
            with self.subTest(component=variant):
                self.assertNotEqual(variant.identity, capability.identity)

        # Changing outer capability coordinates never contaminates the raw
        # daemon-observation identity.
        self.assertTrue(
            all(
                variant.daemon_observation_identity
                == variant.daemon_observation.identity
                for variant in variants
            )
        )
        self.assertEqual(
            variants[0].daemon_observation_identity,
            daemon.identity,
        )
        self.assertEqual(
            variants[2].daemon_observation_identity,
            daemon.identity,
        )
        self.assertEqual(
            variants[3].daemon_observation_identity,
            daemon.identity,
        )
        self.assertEqual(
            variants[4].daemon_observation_identity,
            daemon.identity,
        )

        for value in (policy, capability):
            with self.subTest(immutable=type(value).__name__):
                _assert_deeply_immutable(self, value)


class NativeCommandAndRequestTests(unittest.TestCase):
    def test_native_process_context_is_one_identity_bound_launch_renderer(self) -> None:
        expected_base = {
            "stdout": subprocess.PIPE,
            "stderr": subprocess.PIPE,
            "cwd": _NATIVE_PROCESS_CWD_V1,
            "env": dict(_NATIVE_PROCESS_ENVIRONMENT_V1),
            "close_fds": _NATIVE_PROCESS_CLOSE_FDS_V1,
            "restore_signals": _NATIVE_PROCESS_RESTORE_SIGNALS_V1,
            "start_new_session": _NATIVE_PROCESS_START_NEW_SESSION_V1,
            "umask": _NATIVE_PROCESS_UMASK_V1,
        }
        context = transport._NATIVE_PROCESS_CONTEXT_V1
        for receives_stdin, stdin in (
            (False, subprocess.DEVNULL),
            (True, subprocess.PIPE),
        ):
            with self.subTest(receives_stdin=receives_stdin):
                expected = {"stdin": stdin, **expected_base}
                first = context.popen_kwargs_v1(receives_stdin)
                second = context.popen_kwargs_v1(receives_stdin)
                self.assertEqual(first, expected)
                self.assertEqual(second, expected)
                self.assertIsNot(first, second)
                self.assertIsNot(first["env"], second["env"])

        backend = transport.NativeDockerBuildBackendV1(
            Path("/usr/bin/true"),
            _policy(),
            platform_name="linux",
            machine_name="x86_64",
            host_user=(501, 20),
        )
        with mock.patch.object(
            transport.subprocess,
            "Popen",
            side_effect=OSError("do not launch in identity test"),
        ) as spawn:
            for receives_stdin, stdin in (
                (False, subprocess.DEVNULL),
                (True, subprocess.PIPE),
            ):
                with self.subTest(receives_stdin=receives_stdin):
                    result = backend._observe_command(
                        ("/usr/bin/true",),
                        stdout_limit=64,
                        stderr_limit=64,
                        timeout_ns=1_000_000_000,
                        input_bundle=_sealed_input() if receives_stdin else None,
                    )
                    self.assertIs(
                        type(result),
                        transport.DockerBuildObserverFailureV1,
                    )
                    expected = {"stdin": stdin, **expected_base}
                    self.assertEqual(spawn.call_args.kwargs, expected)

    def test_native_backend_requires_its_exact_probe_lease(self) -> None:
        policy = _policy()
        backend = transport.NativeDockerBuildBackendV1(
            Path("/usr/bin/true"),
            policy,
            platform_name="linux",
            machine_name="x86_64",
            host_user=(501, 20),
        )
        unobserved = _capability(policy=policy)
        request = transport.DockerBuildRequestV1(
            1,
            unobserved,
            _sealed_input(),
            64,
        )
        with self.assertRaises(TypeError):
            backend._bound_request_capability_v1(request)

        server_stdout = b'{"Version":"identity-test"}\n'
        image_stdout = json.dumps(
            [
                {
                    "Os": "linux",
                    "Architecture": "amd64",
                    "RepoDigests": [policy.image_reference],
                }
            ],
            separators=(",", ":"),
        ).encode("ascii")
        observed = (
            transport._docker_command_exited_v1(0, server_stdout, b""),
            transport._docker_command_exited_v1(0, image_stdout, b""),
        )
        with mock.patch.object(backend, "_observe_command", side_effect=observed):
            capability = backend.probe()
        self.assertIs(type(capability), transport.DockerSupportedV1)

        equal_but_foreign = transport.DockerSupportedV1(*tuple(capability))
        self.assertEqual(equal_but_foreign, capability)
        self.assertIsNot(equal_but_foreign, capability)
        cloned_request = transport.DockerBuildRequestV1(
            1,
            equal_but_foreign,
            _sealed_input(),
            64,
        )
        with self.assertRaises(TypeError):
            backend._bound_request_capability_v1(cloned_request)

    def test_native_adapter_expands_the_versioned_template_to_exact_argv(self) -> None:
        policy = _policy()
        docker_path = Path("/usr/bin/true")
        backend = transport.NativeDockerBuildBackendV1(
            docker_path,
            policy,
            platform_name="linux",
            machine_name="x86_64",
            host_user=(501, 20),
        )
        server_stdout = b'{"Version":"identity-test"}\n'
        image_stdout = json.dumps(
            [
                {
                    "Os": "linux",
                    "Architecture": "amd64",
                    "RepoDigests": [policy.image_reference],
                }
            ],
            separators=(",", ":"),
        ).encode("ascii")
        observed = (
            transport._docker_command_exited_v1(0, server_stdout, b""),
            transport._docker_command_exited_v1(0, image_stdout, b""),
        )
        with mock.patch.object(
            backend,
            "_observe_command",
            side_effect=observed,
        ):
            capability = backend.probe()
        self.assertIs(type(capability), transport.DockerSupportedV1)

        input_bundle = _sealed_input()
        request = transport.DockerBuildRequestV1(
            1,
            capability,
            input_bundle,
            64,
        )
        lease = backend._next_run_lease_v1(capability)
        try:
            command = backend._command_for_v1(request, lease)
            expected = (
                str(docker_path),
                "run",
                "--rm",
                "--interactive",
                "--pull",
                "never",
                "--platform",
                policy.platform,
                "--network",
                "none",
                "--read-only",
                "--tmpfs",
                policy.tmpfs_specs[0],
                "--tmpfs",
                policy.tmpfs_specs[1],
                "--cap-drop",
                "ALL",
                "--security-opt",
                "no-new-privileges:true",
                "--hostname",
                policy.hostname,
                "--user",
                "501:20",
                "--workdir",
                "/",
                "--cidfile",
                str(lease.cid_file),
                "--entrypoint",
                "/usr/bin/env",
                policy.image_reference,
                "-i",
                "PATH=/usr/local/bin:/usr/bin:/bin",
                "LC_ALL=C",
                "LANG=C",
                "TZ=UTC",
                "HOME=/nonexistent",
                "/bin/sh",
                "-c",
                policy.bootstrap,
                policy.bootstrap_argv0,
                str(input_bundle.length),
                input_bundle.sha256.hex(),
            )
            self.assertEqual(command, expected)
            self.assertEqual(command.count("--tmpfs"), len(policy.tmpfs_specs))
            self.assertLess(
                command.index(policy.tmpfs_specs[0]),
                command.index(policy.tmpfs_specs[1]),
            )
            self.assertEqual(
                transport.native_command_contract_identity_v1(),
                _expected_command_contract_identity(),
            )
        finally:
            backend._release_run_lease_v1(lease)

    def test_foreign_capability_is_rejected_before_backend_run(self) -> None:
        policy = _policy()
        owned = _capability(policy=policy)
        foreign = _capability(policy=policy, docker_path=Path("/bin/sh"))

        class Backend:
            def __init__(self) -> None:
                self.requests: list[object] = []

            def probe(self) -> object:
                return owned

            def run_build(self, request: object) -> object:
                self.requests.append(request)
                raise AssertionError("foreign capability reached backend")

        backend = Backend()
        controller = transport.ControlledBuildTransportV1(
            policy=policy,
            backend=backend,
        )
        self.assertIs(controller.probe(), owned)
        result = controller.build(
            foreign,
            _sealed_input(),
            64,
            input_admission=lambda _value: True,
            output_admission=lambda _value: True,
        )
        self.assertIs(type(result), transport.BuildRejectedV1)
        self.assertEqual(
            result.reason,
            transport.BuildFailureReasonV1.CONTRACT_VIOLATION,
        )
        self.assertEqual(backend.requests, [])

    def test_request_is_deeply_immutable_and_owns_capability_not_policy(self) -> None:
        capability = _capability()
        request = transport.DockerBuildRequestV1(
            1,
            capability,
            _sealed_input(),
            64,
        )

        self.assertIs(request.capability, capability)
        self.assertFalse(hasattr(request, "policy"))
        _assert_deeply_immutable(self, request)
        _assert_deeply_immutable(self, request.capability)


if __name__ == "__main__":
    unittest.main(verbosity=2)
