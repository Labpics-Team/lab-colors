#!/usr/bin/env python3
"""Anti-vacuum contract for the offline Arb dependency build."""

from __future__ import annotations

import hashlib
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

from proof.region.v1.arb.tests import gate as arb_gate


ARB = Path(__file__).resolve().parents[1]
BUILD = ARB / "build.sh"
INNER_BUILD = ARB / "build-inner.sh"
WORKFLOW = ARB.parents[3] / ".github" / "workflows" / "arb.yml"
RECIPE_REJECTION_TIMEOUT_SECONDS = 5


class ArbBuildRecipeTests(unittest.TestCase):
    def test_fast_gate_includes_each_shared_contract_suite_exactly_once(self) -> None:
        tests = tuple(arb_gate.iter_tests_v1(arb_gate.full_suite_v1()))
        identifiers = tuple(test.id() for test in tests)
        for pattern, module_prefix in (
            ("test_executor.py", "test_executor."),
            ("test_mpfi_input.py", "test_mpfi_input."),
        ):
            with self.subTest(pattern=pattern):
                included = tuple(
                    identifier
                    for identifier in identifiers
                    if identifier.startswith(module_prefix)
                )
                expected = tuple(
                    test.id()
                    for test in arb_gate.iter_tests_v1(
                        unittest.defaultTestLoader.discover(
                            str(arb_gate.SHARED_TEST_DIRECTORY),
                            pattern=pattern,
                        )
                    )
                )

                self.assertTrue(expected)
                self.assertEqual(included, expected)
        self.assertEqual(len(identifiers), len(set(identifiers)))

    def test_mpfi_input_contract_cannot_green_by_skipping(self) -> None:
        suite = unittest.defaultTestLoader.discover(
            str(arb_gate.SHARED_TEST_DIRECTORY),
            pattern="test_mpfi_input.py",
        )
        test_ids = tuple(test.id() for test in arb_gate.iter_tests_v1(suite))
        result = unittest.TestResult()

        suite.run(result)

        self.assertTrue(test_ids)
        self.assertTrue(
            all(test_id.startswith("test_mpfi_input.") for test_id in test_ids)
        )
        expected_skip_ids = {
            test_id for test_id, _reason in arb_gate.EXPECTED_SKIPS
        }
        self.assertTrue(set(test_ids).isdisjoint(expected_skip_ids))
        self.assertEqual(result.testsRun, len(test_ids))
        self.assertFalse(result.skipped)
        self.assertFalse(result.expectedFailures)
        self.assertFalse(result.unexpectedSuccesses)
        self.assertFalse(result.failures)
        self.assertFalse(result.errors)

    def test_pr_gate_requires_a_disposable_exact_workflow_runner(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")
        runner_contracts = [
            line.strip()
            for line in source.splitlines()
            if line.lstrip().startswith("runs-on:")
        ]

        self.assertEqual(
            runner_contracts,
            ["runs-on: [self-hosted, Linux, X64, labcolors-ephemeral]"],
        )
        self.assertIn("proof/region/v1/arb/tests/gate.py", source)
        self.assertIn("proof/region/v1/arb/tests/native_gate.py", source)
        self.assertEqual(source.count("- .github/workflows/arb.yml"), 1)
        self.assertNotIn("arb-proof-observation.yml", source)
        self.assertIn(
            'echo "LABCOLORS_MPFI_ARCHIVE=$archive" >> "$GITHUB_ENV"',
            source,
        )
        # The Docker boundary probe must consume the canonical transport
        # backend and the retained pipeline policy; a bare pipeline-attribute
        # probe drifted once already and failed closed only on a real worker.
        for probe_contract in (
            "from build import transport as build_transport",
            "build_transport.NativeDockerBuildBackendV1(",
            "pipeline.ARB_BUILD_TRANSPORT_POLICY_V1,",
            "build_transport.DockerSupportedV1:",
        ):
            with self.subTest(probe_contract=probe_contract):
                self.assertIn(probe_contract, source)
        self.assertNotIn("pipeline.NativeDockerBuildBackendV1", source)
        for required in (
            'apparmor_userns=/proc/sys/kernel/apparmor_restrict_unprivileged_userns',
            'if [[ -f "$apparmor_userns" ]]; then',
            'original_userns="$(cat /proc/sys/kernel/apparmor_restrict_unprivileged_userns)"',
            'echo "LABCOLORS_APPARMOR_USERNS_V1=$original_userns"',
            "sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0",
            'echo "LABCOLORS_APPARMOR_USERNS_V1="',
            'kernel.apparmor_restrict_unprivileged_userns=$LABCOLORS_APPARMOR_USERNS_V1',
            'mkdir "$scope/tasks" "$scope/proof"',
            "printf '+memory +pids' > \"$scope/cgroup.subtree_control\"",
            "printf '+memory +pids' > \"$scope/proof/cgroup.subtree_control\"",
            "printf '2' > \"$scope/proof/pids.max\"",
            'mkdir "$scope/proof/observer"',
            '"$scope/proof/cgroup.subtree_control"',
            'scope="/sys/fs/cgroup/labcolors-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT"',
            'echo "LABCOLORS_CGROUP_SCOPE_V1=$scope"',
            'echo "LABCOLORS_EXECUTOR_CGROUP_V1=$scope/proof"',
            '"$LABCOLORS_EXECUTOR_CGROUP_V1/observer/cgroup.procs"',
            "native_gate.py receipt",
            "native_gate.py executor",
            "exec python3",
            '"$LABCOLORS_CGROUP_SCOPE_V1/cgroup.kill"',
            "'populated 0'",
            "for child in proof/observer proof tasks",
            'sudo rmdir "$LABCOLORS_CGROUP_SCOPE_V1/$child"',
            'sudo rmdir "$LABCOLORS_CGROUP_SCOPE_V1"',
        ):
            with self.subTest(required=required):
                self.assertIn(required, source)
        self.assertNotIn("grep --ignore-case --quiet skipped", source)
        self.assertNotIn("python3 -m unittest", source)
        # The kernel precondition is fail-closed and host-explicit: either the
        # restriction sysctl is read, validated, and lifted, or its absence is
        # the proof; an unconditional presence test would fail closed on
        # kernels without AppArmor userns mediation.
        self.assertNotIn(
            "test -f /proc/sys/kernel/apparmor_restrict_unprivileged_userns",
            source,
        )
        self.assertLess(
            source.index("proof/region/v1/arb/tests/gate.py"),
            source.index("LABCOLORS_EXECUTOR_CGROUP_V1=$scope/proof"),
        )

    def test_pr_gate_cannot_green_skip_a_fork_without_execution(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")

        self.assertNotIn("github.event.pull_request.head.repo.full_name", source)

    def test_privileged_workflow_has_no_automatic_pull_request_trigger(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")

        self.assertIn("\n  workflow_dispatch:\n", source)
        self.assertIn("\n  push:\n", source)
        self.assertIn("branches: [main]", source)
        self.assertNotIn("\n  pull_request:\n", source)
        self.assertNotIn("pull_request", source)

    def test_exact_suite_gate_rejects_expected_failure(self) -> None:
        class BrokenRequiredTest(unittest.TestCase):
            @unittest.expectedFailure
            def test_required(self) -> None:
                self.fail("broken")

        suite = unittest.defaultTestLoader.loadTestsFromTestCase(BrokenRequiredTest)
        expected = arb_gate.test_inventory_sha256_v1(suite)

        self.assertEqual(
            arb_gate.run_exact_suite_v1(
                suite,
                expected_inventory_sha256=expected,
                expected_skips=frozenset(),
                verbosity=0,
            ),
            1,
        )

    def test_exact_suite_gate_rejects_empty_or_same_count_replacement(self) -> None:
        class RequiredTest(unittest.TestCase):
            def test_required(self) -> None:
                pass

        class ReplacementTest(unittest.TestCase):
            def test_replacement(self) -> None:
                pass

        empty = unittest.TestSuite()
        self.assertEqual(
            arb_gate.run_exact_suite_v1(
                empty,
                expected_inventory_sha256=hashlib.sha256(b"").hexdigest(),
                expected_skips=frozenset(),
                verbosity=0,
            ),
            1,
        )
        required = unittest.defaultTestLoader.loadTestsFromTestCase(RequiredTest)
        replacement = unittest.defaultTestLoader.loadTestsFromTestCase(ReplacementTest)
        self.assertEqual(required.countTestCases(), replacement.countTestCases())
        self.assertEqual(
            arb_gate.run_exact_suite_v1(
                replacement,
                expected_inventory_sha256=arb_gate.test_inventory_sha256_v1(required),
                expected_skips=frozenset(),
                verbosity=0,
            ),
            1,
        )

    def test_recipe_is_offline_static_and_platform_explicit(self) -> None:
        source = INNER_BUILD.read_text(encoding="utf-8")
        entrypoint = BUILD.read_text(encoding="utf-8")
        self.assertIn("/usr/bin/env -i", entrypoint)
        self.assertNotIn("LC_BUILD_ENV_V1", entrypoint)

        for required in (
            'require_directory "$inputs/gmp-6.3.0"',
            'require_directory "$inputs/mpfr-4.2.2"',
            'require_directory "$inputs/flint-3.6.0"',
            'require_regular "$workspace/proof/region/v1/arb/evaluator/formula.h"',
            "9958f20c8ca598625db0593a45f8f8bc79e4b2f22b53263b6c32d78a5e1d2693",
            "-I.",
            "--build=x86_64-pc-linux-gnu",
            "--host=x86_64-pc-linux-gnu",
            "--disable-shared",
            "--enable-static",
            "--disable-assembly",
            "--enable-formally-proven-code",
            "--disable-lto",
            "--enable-assert",
            "-fno-fast-math",
            "-ffp-contract=off",
            "-fno-lto",
            "-std=gnu17",
            # One strict dialect for the whole chain: the dependency libraries
            # are built with gnu17 above, and the evaluator keeps -Wall
            # -Wextra -Werror -pedantic fully strict on its own sources.
            "-std=gnu17 -Wall -Wextra -Werror -pedantic",
            # Assert-enabled FLINT 3.6.0 degrades FLINT_UNUSED(x) to a bare
            # parameter, which -Wextra diagnoses inside flint_rand_clear in
            # any dialect; the pinned dependency headers therefore enter as
            # one system include directory while the evaluator's own headers
            # stay under -I. with full diagnostics.
            '-I. -isystem "$prefix/include"',
            "-march=x86-64",
            "-mtune=generic",
            "-Wl,--build-id=none",
            "make check",
            "readelf",
        ):
            with self.subTest(required=required):
                self.assertIn(required, source)

        for forbidden in (
            "curl ",
            "wget ",
            "git clone",
            "apt-get",
            "brew ",
            "tar --extract",
            "-ffast-math",
            "-march=native",
            "-flto",
        ):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, source)
        # Reverting the dependency headers to a regular include would turn
        # FLINT's assert-mode header diagnostics back into -Werror failures.
        self.assertNotIn('-I"$prefix/include"', source)

        self.assertIn(
            'if ! /usr/bin/readelf -l "$build/arb-evaluator-v1" '
            '> "$build/program-headers"; then',
            source,
        )
        self.assertIn(
            'if ! /usr/bin/readelf -d "$build/arb-evaluator-v1" '
            '> "$build/dynamic-section"; then',
            source,
        )
        self.assertNotIn("readelf -l \"$build/arb-evaluator-v1\" |", source)
        self.assertNotIn("readelf -d \"$build/arb-evaluator-v1\" 2>&1 |", source)

    def test_public_build_entrypoint_strips_hostile_environment_before_recipe(self) -> None:
        entrypoint = BUILD.read_text(encoding="utf-8")
        recipe = INNER_BUILD.read_text(encoding="utf-8")
        self.assertIn("/usr/bin/env -i", entrypoint)
        self.assertIn('inner="$script_dir/build-inner.sh"', entrypoint)
        self.assertIn('/bin/sh "$inner"', entrypoint)
        self.assertNotIn("LC_BUILD_ENV_V1", entrypoint)
        self.assertNotIn("LC_BUILD_ENV_V1", recipe)
        self.assertNotIn("/usr/bin/env -i", recipe)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            entrypoint_copy = root / "build.sh"
            inner = root / "build-inner.sh"
            observed = root / "environment"
            entrypoint_copy.write_text(entrypoint, encoding="utf-8")
            entrypoint_copy.chmod(0o755)
            inner.write_text(
                "#!/bin/sh\n"
                f"/usr/bin/env | /usr/bin/sort > '{observed}'\n",
                encoding="utf-8",
            )
            result = subprocess.run(
                [str(entrypoint_copy)],
                check=False,
                capture_output=True,
                text=True,
                env={
                    "LC_BUILD_ENV_V1": "1",
                    "MAKEFLAGS": "--jobserver-auth=spoof",
                    "PYTHONPATH": "/host-controlled",
                    "CONFIG_SITE": "/host-controlled/site",
                    "PATH": "/host-controlled/bin",
                },
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            environment = observed.read_text(encoding="utf-8")
            self.assertIn("PATH=/usr/local/bin:/usr/bin:/bin\n", environment)
            for forbidden in (
                "LC_BUILD_ENV_V1=1",
                "MAKEFLAGS=--jobserver-auth=spoof",
                "PYTHONPATH=/host-controlled",
                "CONFIG_SITE=/host-controlled/site",
                "PATH=/host-controlled/bin",
            ):
                self.assertNotIn(forbidden, environment)

    def test_recipe_rejects_ambient_or_incomplete_invocation_before_build(self) -> None:
        self.assertTrue(os.access(BUILD, os.X_OK), BUILD)
        result = subprocess.run(
            [str(BUILD)],
            check=False,
            capture_output=True,
            env={
                "PATH": os.environ.get("PATH", ""),
                "UNDECLARED": "must-not-be-observed",
            },
            stdin=subprocess.DEVNULL,
            timeout=RECIPE_REJECTION_TIMEOUT_SECONDS,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, b"")


if __name__ == "__main__":
    unittest.main(verbosity=2)
