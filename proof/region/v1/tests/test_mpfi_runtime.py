#!/usr/bin/env python3
"""RED/green contract for the MPFI runtime profile binding."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

import executor  # noqa: E402
from mpfi import runtime  # noqa: E402


def _limits(**changes: int) -> executor.ExecutionLimitsV1:
    values = {
        "max_executable_bytes": 8 * 1024 * 1024,
        "max_stdin_bytes": runtime.MPFI_MAX_JOB_BYTES_V1,
        "max_argument_bytes": 64 * 1024,
        "max_stdout_bytes": runtime.MPFI_MAX_OUTPUT_BYTES_V1,
        "max_stderr_bytes": 64 * 1024,
        "wall_timeout_ns": 300_000_000_000,
        "memory_max_bytes": 2 * 1024 * 1024 * 1024,
        "pids_max": 1,
    }
    values.update(changes)
    return executor.ExecutionLimitsV1(**values)


class MpfiRuntimeProfileTests(unittest.TestCase):
    def test_profile_is_one_exact_wire_v1_value(self) -> None:
        profile = runtime.mpfi_runtime_profile_v1()
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
        self.assertEqual(
            runtime.MPFI_RUNTIME_PROFILE_ID_V1,
            "LC-MPFI-RUNTIME-V1",
        )
        self.assertEqual(
            runtime.MpfiRuntimeProfileV1(*tuple(profile)),
            profile,
        )
        with self.assertRaises(ValueError):
            runtime.MpfiRuntimeProfileV1(
                profile.max_job_bytes - 1,
                profile.max_output_bytes,
                profile.max_precision_bits,
                profile.max_policy_rungs,
                profile.max_knots,
            )

    def test_profile_identity_is_typed_and_not_accepted_from_a_plain_tuple(self) -> None:
        profile = runtime.mpfi_runtime_profile_v1()
        identity = runtime.runtime_profile_identity_v1(profile)
        self.assertIs(type(identity), bytes)
        self.assertEqual(identity, runtime.runtime_profile_identity_v1(profile))
        rejected = runtime.runtime_profile_identity_v1(tuple(profile))
        self.assertIs(type(rejected), runtime.MpfiRuntimeIdentityRejectedV1)
        self.assertEqual(
            rejected.reason,
            runtime.MpfiRuntimeProfileReasonV1.WRONG_TYPE,
        )

    def test_binding_requires_profile_job_and_output_limits_to_match_exactly(self) -> None:
        profile = runtime.mpfi_runtime_profile_v1()
        binding = runtime.MpfiRuntimeBindingV1(profile, _limits())
        identity = runtime.runtime_binding_identity_v1(binding)
        self.assertIs(type(identity), bytes)
        self.assertEqual(identity, runtime.runtime_binding_identity_v1(binding))

        with self.assertRaises(ValueError):
            runtime.MpfiRuntimeBindingV1(
                profile,
                _limits(max_stdin_bytes=profile.max_job_bytes - 1),
            )
        with self.assertRaises(ValueError):
            runtime.MpfiRuntimeBindingV1(
                profile,
                _limits(max_stdout_bytes=profile.max_output_bytes - 1),
            )
        rejected = runtime.runtime_binding_identity_v1((profile, _limits()))
        self.assertIs(type(rejected), runtime.MpfiRuntimeIdentityRejectedV1)
        self.assertEqual(
            rejected.reason,
            runtime.MpfiRuntimeProfileReasonV1.WRONG_TYPE,
        )

    def test_binding_identity_commits_every_executor_limit(self) -> None:
        profile = runtime.mpfi_runtime_profile_v1()
        first = runtime.MpfiRuntimeBindingV1(profile, _limits())
        second = runtime.MpfiRuntimeBindingV1(
            profile,
            _limits(memory_max_bytes=2 * 1024 * 1024 * 1024 - 1),
        )
        first_identity = runtime.runtime_binding_identity_v1(first)
        second_identity = runtime.runtime_binding_identity_v1(second)
        self.assertIs(type(first_identity), bytes)
        self.assertIs(type(second_identity), bytes)
        self.assertNotEqual(first_identity, second_identity)

    def test_protocol_documents_the_single_binding_authority(self) -> None:
        reference = (ROOT / "PROTOCOL.md").read_text(encoding="utf-8")
        self.assertIn("MpfiRuntimeProfileV1", reference)
        self.assertIn("MpfiRuntimeBindingV1", reference)
        self.assertIn("max_stdin_bytes", reference)
        self.assertIn("max_stdout_bytes", reference)


if __name__ == "__main__":
    unittest.main(verbosity=2)
