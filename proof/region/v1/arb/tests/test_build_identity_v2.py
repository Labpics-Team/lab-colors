#!/usr/bin/env python3
"""RED contract for causal BUILD policy and capability identities."""

from __future__ import annotations

import ast
import hashlib
import inspect
import json
import sys
import tempfile
import unittest
from dataclasses import fields as dataclass_fields
from pathlib import Path
from unittest import mock


PROOF = Path(__file__).resolve().parents[2]
ARB = PROOF / "arb"
sys.path[:0] = [str(PROOF), str(ARB), str(ARB / "tests")]

from build import transport as build_transport  # noqa: E402

import pipeline  # noqa: E402
import receipt  # noqa: E402
from test_pipeline import _BuildBackend, _request, _static_elf  # noqa: E402


_POLICY_FIELDS = (
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


def _called_names(function: object) -> set[str]:
    tree = ast.parse(inspect.getsource(function))
    names: set[str] = set()
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        if isinstance(node.func, ast.Attribute):
            names.add(node.func.attr)
        elif isinstance(node.func, ast.Name):
            names.add(node.func.id)
    return names


def _policy_with(
    policy: build_transport.DockerBuildPolicyV1,
    **changes: object,
) -> build_transport.DockerBuildPolicyV1:
    values = {name: getattr(policy, name) for name in _POLICY_FIELDS}
    values.update(changes)
    return build_transport.DockerBuildPolicyV1(
        *(values[name] for name in _POLICY_FIELDS)
    )


def _policy_mutants(
    policy: build_transport.DockerBuildPolicyV1,
) -> tuple[tuple[str, build_transport.DockerBuildPolicyV1], ...]:
    first_tmpfs, *remaining_tmpfs = policy.tmpfs_specs
    tmpfs_parts = first_tmpfs.split(",")
    size_index = next(
        index for index, part in enumerate(tmpfs_parts) if part.startswith("size=")
    )
    size_value = int(tmpfs_parts[size_index].removeprefix("size="))
    tmpfs_parts[size_index] = f"size={size_value - 1}"
    changed_tmpfs = ",".join(tmpfs_parts)
    numeric_fields = (
        "stdout_limit",
        "stderr_limit",
        "build_timeout_ns",
        "probe_output_limit",
        "probe_timeout_ns",
    )
    mutants: list[tuple[str, build_transport.DockerBuildPolicyV1]] = [
        (
            "image_reference",
            _policy_with(
                policy,
                image_reference="gcc@sha256:" + "ab" * 32,
            ),
        ),
        ("hostname", _policy_with(policy, hostname="labcolors-build-mutant")),
        ("bootstrap", _policy_with(policy, bootstrap=policy.bootstrap + "\n:")),
        (
            "bootstrap_argv0",
            _policy_with(policy, bootstrap_argv0="labcolors-mutant-bootstrap"),
        ),
        (
            "tmpfs_specs",
            _policy_with(
                policy,
                tmpfs_specs=(changed_tmpfs, *remaining_tmpfs),
            ),
        ),
    ]
    for name in numeric_fields:
        value = getattr(policy, name)
        mutants.append((name, _policy_with(policy, **{name: value - 1})))
    return tuple(mutants)


def _capability(
    docker_path: Path,
    policy: build_transport.DockerBuildPolicyV1,
    *,
    host_user: tuple[int, int] = (501, 20),
    daemon_marker: str = "daemon-a",
) -> build_transport.DockerSupportedV1:
    backend = build_transport.NativeDockerBuildBackendV1(
        docker_path,
        policy,
        platform_name="linux",
        machine_name="x86_64",
        host_user=host_user,
    )
    image_observation = json.dumps(
        [
            {
                "Os": "linux",
                "Architecture": "amd64",
                "RepoDigests": [policy.image_reference],
            }
        ],
        sort_keys=True,
        separators=(",", ":"),
    ).encode("ascii")
    observations = (
        build_transport._docker_command_exited_v1(
            0,
            json.dumps(
                {"daemon": daemon_marker},
                sort_keys=True,
                separators=(",", ":"),
            ).encode("ascii"),
            b"",
        ),
        build_transport._docker_command_exited_v1(0, image_observation, b""),
    )
    with mock.patch.object(backend, "_observe_command", side_effect=observations):
        capability = backend.probe()
    if type(capability) is not build_transport.DockerSupportedV1:
        raise AssertionError(capability)
    return capability


def _observed_build_coordinates(
    policy: build_transport.DockerBuildPolicyV1,
    capability: build_transport.DockerSupportedV1,
) -> tuple[bytes, bytes, bytes, bytes, tuple[bytes, bytes], bytes, bytes]:
    request = _request()
    binary = _static_elf(b"identity-v2-invariant-output")
    with mock.patch.object(pipeline, "ARB_BUILD_TRANSPORT_POLICY_V1", policy):
        result = pipeline.ControlledPipelineV1(
            build_backend=_BuildBackend((binary, binary), probe=capability)
        ).build(request)
        if type(result) is not pipeline.DiagnosticBuildObservationV1:
            raise AssertionError(result)
        source_identity = receipt._source_identity_v1(request)
        receipt_build_identity = receipt._build_identity_v2(
            request,
            source_identity,
            result,
        )
        source_bound_policy = receipt.source_bound_policy_identity_v2(
            result.docker_capability,
            request.host_trust,
        )
    process_encodings = tuple(
        build_transport.build_process_bytes_v1(process)
        for process in result.build_processes
    )
    return (
        build_transport.docker_capability_identity_v1(result.docker_capability),
        result.comparator.preimages.build_identity,
        receipt_build_identity,
        source_identity,
        process_encodings,
        result.input_bundle.contents,
        source_bound_policy,
    )


class BuildIdentityV2Tests(unittest.TestCase):
    def test_v2_surface_replaces_v1_aliases_and_preimage_labels(self) -> None:
        self.assertFalse(hasattr(pipeline, "pipeline_policy_identity_v1"))
        self.assertFalse(hasattr(receipt, "source_bound_policy_identity_v1"))
        self.assertFalse(hasattr(receipt, "_build_identity_v1"))
        self.assertTrue(callable(pipeline.pipeline_policy_identity_v2))
        self.assertTrue(callable(receipt.source_bound_policy_identity_v2))
        self.assertTrue(callable(receipt._build_identity_v2))

        pipeline_source = (ARB / "pipeline.py").read_text(encoding="utf-8")
        receipt_source = (ARB / "receipt.py").read_text(encoding="utf-8")
        for stale in (
            "labcolors.proof-region.arb-pipeline-policy.v1",
            "labcolors.proof-region.arb-comparator.build-identity.v1",
        ):
            with self.subTest(stale=stale):
                self.assertNotIn(stale, pipeline_source)
        for stale in (
            "labcolors.proof-region.arb-build-replay.v1",
            "labcolors.proof-region.arb-source-bound-policy.v1",
        ):
            with self.subTest(stale=stale):
                self.assertNotIn(stale, receipt_source)

    def test_pipeline_policy_consumes_both_owned_transport_identities(self) -> None:
        trust = pipeline.HostTrustBoundaryV1.UNSEALED_LINUX_X64_DOCKER_HOST
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        transport_identity = build_transport.transport_policy_identity_v1(policy)
        command_identity = build_transport.native_command_contract_identity_v1()
        pipeline_identity = pipeline.pipeline_policy_identity_v2(trust, policy)

        for name, value in (
            ("transport", transport_identity),
            ("command", command_identity),
            ("pipeline", pipeline_identity),
        ):
            with self.subTest(identity=name):
                self.assertIs(type(value), bytes)
                self.assertEqual(len(value), hashlib.sha256().digest_size)
                self.assertNotEqual(value, bytes(hashlib.sha256().digest_size))
        self.assertNotEqual(transport_identity, command_identity)
        self.assertNotEqual(transport_identity, pipeline_identity)
        self.assertNotEqual(command_identity, pipeline_identity)

        calls = _called_names(pipeline.pipeline_policy_identity_v2)
        self.assertIn("transport_policy_identity_v1", calls)
        self.assertIn("native_command_contract_identity_v1", calls)
        for surrogate in (tuple(policy), list(policy), object()):
            with self.subTest(surrogate=type(surrogate).__name__):
                with self.assertRaises(TypeError):
                    build_transport.transport_policy_identity_v1(surrogate)
                with self.assertRaises(TypeError):
                    pipeline.pipeline_policy_identity_v2(trust, surrogate)

    def test_every_admitted_policy_mutation_changes_transport_and_pipeline_identity(self) -> None:
        trust = pipeline.HostTrustBoundaryV1.UNSEALED_LINUX_X64_DOCKER_HOST
        policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        baseline_transport = build_transport.transport_policy_identity_v1(policy)
        baseline_pipeline = pipeline.pipeline_policy_identity_v2(trust, policy)
        mutants = _policy_mutants(policy)

        self.assertEqual(
            {name for name, _mutant in mutants},
            set(_POLICY_FIELDS) - {"platform", "user_mode"},
        )
        for name, mutant in mutants:
            with self.subTest(field=name):
                self.assertTrue(build_transport.docker_policy_is_valid_v1(mutant))
                self.assertNotEqual(
                    build_transport.transport_policy_identity_v1(mutant),
                    baseline_transport,
                )
                self.assertNotEqual(
                    pipeline.pipeline_policy_identity_v2(trust, mutant),
                    baseline_pipeline,
                )

        # These coordinates currently have singleton admitted domains. Their
        # only meaningful mutants are invalid inputs, not a second policy.
        self.assertEqual(tuple(build_transport.DockerUserModeV1), (policy.user_mode,))
        with self.assertRaises(TypeError):
            _policy_with(policy, platform="linux/arm64")
        with self.assertRaises(TypeError):
            _policy_with(policy, user_mode="host_effective_ids")

    def test_diagnostic_build_owns_one_capability_and_replayers_consume_its_identity(self) -> None:
        field_names = tuple(
            field.name for field in dataclass_fields(pipeline.DiagnosticBuildObservationV1)
        )
        self.assertEqual(field_names.count("docker_capability"), 1)
        for mirror in (
            "docker_daemon_observation_sha256",
            "oci_image_reference",
            "oci_platform",
            "docker_path",
            "host_user",
        ):
            with self.subTest(mirror=mirror):
                self.assertNotIn(mirror, field_names)

        comparator_calls = _called_names(
            pipeline._derive_arb_comparator_for_build_v1
        )
        comparator_replay_calls = _called_names(receipt._comparator_replays_v1)
        receipt_build_calls = _called_names(receipt._build_identity_v2)
        source_bound_calls = _called_names(
            receipt.source_bound_policy_identity_v2
        )
        self.assertIn("docker_capability_identity_v1", comparator_calls)
        self.assertIn("docker_capability_identity_v1", comparator_replay_calls)
        self.assertIn("docker_capability_identity_v1", receipt_build_calls)
        self.assertIn("docker_capability_identity_v1", source_bound_calls)

    def test_path_uid_daemon_and_hostname_flow_to_downstream_build_identity_only(self) -> None:
        baseline_policy = pipeline.ARB_BUILD_TRANSPORT_POLICY_V1
        hostname_policy = _policy_with(
            baseline_policy,
            hostname="labcolors-build-other-host",
        )
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            first_path = root / "docker-a"
            second_path = root / "docker-b"
            first_path.write_bytes(b"same-docker-cli-fixture")
            second_path.write_bytes(b"same-docker-cli-fixture")
            first_path.chmod(0o755)
            second_path.chmod(0o755)

            baseline = _observed_build_coordinates(
                baseline_policy,
                _capability(first_path, baseline_policy),
            )
            variants = {
                "path": _observed_build_coordinates(
                    baseline_policy,
                    _capability(second_path, baseline_policy),
                ),
                "uid": _observed_build_coordinates(
                    baseline_policy,
                    _capability(first_path, baseline_policy, host_user=(502, 20)),
                ),
                "daemon": _observed_build_coordinates(
                    baseline_policy,
                    _capability(
                        first_path,
                        baseline_policy,
                        daemon_marker="daemon-b",
                    ),
                ),
                "hostname": _observed_build_coordinates(
                    hostname_policy,
                    _capability(first_path, hostname_policy),
                ),
            }

        (
            baseline_capability,
            baseline_comparator_build,
            baseline_receipt_build,
            baseline_source,
            baseline_processes,
            baseline_bundle,
            baseline_source_bound_policy,
        ) = baseline
        for name, variant in variants.items():
            with self.subTest(mutation=name):
                (
                    capability_identity,
                    comparator_build,
                    receipt_build,
                    source_identity,
                    process_encodings,
                    bundle_bytes,
                    source_bound_policy,
                ) = variant
                self.assertNotEqual(capability_identity, baseline_capability)
                self.assertNotEqual(comparator_build, baseline_comparator_build)
                self.assertNotEqual(receipt_build, baseline_receipt_build)
                self.assertNotEqual(
                    source_bound_policy,
                    baseline_source_bound_policy,
                )
                self.assertEqual(source_identity, baseline_source)
                self.assertEqual(process_encodings, baseline_processes)
                self.assertEqual(bundle_bytes, baseline_bundle)


if __name__ == "__main__":
    unittest.main()
