#!/usr/bin/env python3
"""RED/green contract for the exact Arb runtime profile binding."""

from __future__ import annotations

import subprocess
import sys
import unittest
from pathlib import Path


PROOF = Path(__file__).resolve().parents[2]
ARB = PROOF / "arb"
sys.path.insert(0, str(PROOF))

import executor  # noqa: E402
from arb import runtime as arb_runtime  # noqa: E402


def _limits(**changes: int) -> executor.ExecutionLimitsV1:
    values = {
        "max_executable_bytes": 16 * 1024 * 1024,
        "max_stdin_bytes": arb_runtime.ARB_MAX_JOB_BYTES_V1,
        "max_argument_bytes": 4096,
        "max_stdout_bytes": arb_runtime.ARB_MAX_OUTPUT_BYTES_V1,
        "max_stderr_bytes": 64 * 1024,
        "wall_timeout_ns": 60_000_000_000,
        "memory_max_bytes": 1024 * 1024 * 1024,
        "pids_max": 1,
    }
    values.update(changes)
    return executor.ExecutionLimitsV1(**values)


class ArbRuntimeProfileTests(unittest.TestCase):
    def test_profile_rejects_int_subclasses_and_identity_totalizes_forgery(self) -> None:
        class EqualInt(int):
            def to_bytes(self, *_args: object, **_kwargs: object) -> bytes:
                raise RuntimeError("foreign scalar executed")

        values = tuple(arb_runtime.arb_runtime_profile_v1())
        hostile_values = (EqualInt(values[0]), *values[1:])
        with self.assertRaises(TypeError):
            arb_runtime.ArbRuntimeProfileV1(*hostile_values)

        forged = tuple.__new__(arb_runtime.ArbRuntimeProfileV1, hostile_values)
        result = arb_runtime.runtime_profile_identity_v1(forged)
        self.assertIs(type(result), arb_runtime.ArbRuntimeIdentityRejectedV1)
        self.assertEqual(
            result.reason,
            arb_runtime.ArbRuntimeProfileReasonV1.NONCANONICAL,
        )

    def test_arb_package_has_one_pipeline_receipt_and_runtime_identity(self) -> None:
        program = f"""
import sys
import types

sys.path.insert(0, {str(PROOF)!r})
foreign_runtime = types.ModuleType("runtime")
sys.modules["runtime"] = foreign_runtime
foreign_pipeline = types.ModuleType("pipeline")
sys.modules["pipeline"] = foreign_pipeline

from arb import pipeline
from arb import receipt
from arb import runtime as expected_runtime

if pipeline.arb_runtime is not expected_runtime:
    raise SystemExit("Arb pipeline accepted a foreign runtime module")
if receipt.pipeline is not pipeline or receipt.arb_runtime is not expected_runtime:
    raise SystemExit("Arb receipt split the package module identities")
"""
        completed = subprocess.run(
            (sys.executable, "-c", program),
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)

    def test_profile_is_one_exact_wire_v1_value(self) -> None:
        profile = arb_runtime.arb_runtime_profile_v1()
        self.assertEqual(
            tuple(profile),
            (
                16 * 1024 * 1024,
                16 * 1024 * 1024,
                4096,
                32,
                1024,
            ),
        )
        self.assertEqual(arb_runtime.ARB_RUNTIME_PROFILE_ID_V1, "LC-ARB-RUNTIME-V1")
        self.assertEqual(arb_runtime.ArbRuntimeProfileV1(*tuple(profile)), profile)
        with self.assertRaises(ValueError):
            arb_runtime.ArbRuntimeProfileV1(
                profile.max_job_bytes,
                profile.max_output_bytes,
                profile.max_precision_bits + 1,
                profile.max_policy_rungs,
                profile.max_knots,
            )

    def test_profile_identity_is_typed_and_total_for_foreign_input(self) -> None:
        profile = arb_runtime.arb_runtime_profile_v1()
        identity = arb_runtime.runtime_profile_identity_v1(profile)
        self.assertIs(type(identity), bytes)
        self.assertEqual(identity, arb_runtime.runtime_profile_identity_v1(profile))
        rejected = arb_runtime.runtime_profile_identity_v1(tuple(profile))
        self.assertIs(type(rejected), arb_runtime.ArbRuntimeIdentityRejectedV1)
        self.assertEqual(
            rejected.reason,
            arb_runtime.ArbRuntimeProfileReasonV1.WRONG_TYPE,
        )

    def test_binding_requires_exact_job_and_output_limits(self) -> None:
        profile = arb_runtime.arb_runtime_profile_v1()
        binding = arb_runtime.ArbRuntimeBindingV1(profile, _limits())
        identity = arb_runtime.runtime_binding_identity_v1(binding)
        self.assertIs(type(identity), bytes)
        for field in ("max_stdin_bytes", "max_stdout_bytes"):
            exact = getattr(_limits(), field)
            for delta in (-1, 1):
                with self.subTest(field=field, delta=delta), self.assertRaises(ValueError):
                    arb_runtime.ArbRuntimeBindingV1(
                        profile,
                        _limits(**{field: exact + delta}),
                    )

    def test_binding_identity_commits_each_variable_nonprofile_limit(self) -> None:
        profile = arb_runtime.arb_runtime_profile_v1()
        baseline = _limits()
        first = arb_runtime.ArbRuntimeBindingV1(profile, baseline)
        first_identity = arb_runtime.runtime_binding_identity_v1(first)
        self.assertIs(type(first_identity), bytes)
        for field in (
            "max_executable_bytes",
            "max_argument_bytes",
            "max_stderr_bytes",
            "wall_timeout_ns",
            "memory_max_bytes",
        ):
            with self.subTest(field=field):
                second = arb_runtime.ArbRuntimeBindingV1(
                    profile,
                    _limits(**{field: getattr(baseline, field) - 1}),
                )
                second_identity = arb_runtime.runtime_binding_identity_v1(second)
                self.assertIs(type(second_identity), bytes)
                self.assertNotEqual(first_identity, second_identity)

    def test_protocol_documents_the_lane_specific_arb_binding(self) -> None:
        reference = (PROOF / "PROTOCOL.md").read_text(encoding="utf-8")
        self.assertIn("ArbRuntimeProfileV1", reference)
        self.assertIn("ArbRuntimeBindingV1", reference)
        self.assertIn("LC-ARB-RUNTIME-V1", reference)


if __name__ == "__main__":
    unittest.main(verbosity=2)
