#!/usr/bin/env python3
"""Anti-vacuum contract for the offline Arb dependency build."""

from __future__ import annotations

import hashlib
import os
import subprocess
import unittest
from pathlib import Path

from proof.region.v1.arb.tests import gate as arb_gate


ARB = Path(__file__).resolve().parents[1]
BUILD = ARB / "build.sh"
WORKFLOW = ARB.parents[3] / ".github" / "workflows" / "arb.yml"
RECIPE_REJECTION_TIMEOUT_SECONDS = 5


class ArbBuildRecipeTests(unittest.TestCase):
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
        self.assertEqual(source.count("- .github/workflows/arb.yml"), 2)
        self.assertNotIn("arb-proof-observation.yml", source)
        for required in (
            "sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0",
            'mkdir "$scope/tasks" "$scope/proof"',
            "printf '+memory +pids' > \"$scope/cgroup.subtree_control\"",
            "printf '+memory +pids' > \"$scope/proof/cgroup.subtree_control\"",
            "printf '2' > \"$scope/proof/pids.max\"",
            'mkdir "$scope/proof/observer"',
            '"$scope/proof/cgroup.subtree_control"',
            'scope="/sys/fs/cgroup/labcolors-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT"',
            'binary="$RUNNER_TEMP/arb-native-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT"',
            'echo "LABCOLORS_CGROUP_SCOPE_V1=$scope"',
            'echo "LABCOLORS_EXECUTOR_CGROUP_V1=$scope/proof"',
            'echo "LABCOLORS_ARB_NATIVE_BINARY=$binary"',
            '"$LABCOLORS_CGROUP_SCOPE_V1/tasks/cgroup.procs"',
            '"$LABCOLORS_EXECUTOR_CGROUP_V1/observer/cgroup.procs"',
            "native_gate.py build",
            "native_gate.py executor",
            "exec python3",
            'stat --format=%a "$LABCOLORS_ARB_NATIVE_BINARY"',
            'rm -f -- "$LABCOLORS_ARB_NATIVE_BINARY"',
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
        self.assertLess(
            source.index("proof/region/v1/arb/tests/gate.py"),
            source.index("LABCOLORS_EXECUTOR_CGROUP_V1=$scope/proof"),
        )

    def test_pr_gate_cannot_green_skip_a_fork_without_execution(self) -> None:
        source = WORKFLOW.read_text(encoding="utf-8")

        self.assertNotIn("github.event.pull_request.head.repo.full_name", source)

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
        source = BUILD.read_text(encoding="utf-8")

        for required in (
            "/usr/bin/env -i",
            "LC_BUILD_ENV_V1=1",
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

    def test_recipe_rejects_ambient_or_incomplete_invocation_before_build(self) -> None:
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
