"""Разрешённый источник и отказ при чужой или отсутствующей identity до загрузки кода."""

from __future__ import annotations

import ast
import inspect
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

from trusted_ci_source import guard_program, validate_owned_source


_UNSET = object()


class TestOwnedSourceGuard(unittest.TestCase):
    def fixture(self):
        return {
            "GITHUB_ACTIONS": "true",
            "CI_REPOSITORY": "Labpics-Team/lab-colors",
            "CI_REPOSITORY_ID": "1221255339",
            "CI_SOURCE_SHA": "a" * 40,
            "CI_CALLER_SHA": "a" * 40,
            "CI_RECIPE_SHA": "a" * 40,
            "CI_RECIPE_REPOSITORY": "Labpics-Team/lab-colors",
            "CI_RECIPE_PATH": ".github/workflows/ci-worker.yml",
            "CI_EXPECTED_RECIPE_PATH": ".github/workflows/ci-worker.yml",
            "CI_RECIPE_REF": "Labpics-Team/lab-colors/.github/workflows/ci-worker.yml@refs/heads/main",
            "CI_EVENT": "pull_request",
            "CI_REF": "refs/pull/849/merge",
            "CI_HEAD_REPOSITORY": "Labpics-Team/lab-colors",
            "CI_HEAD_REPOSITORY_ID": "1221255339",
            "CI_HEAD_SHA": "b" * 40,
            "CI_RUNNER_ENVIRONMENT": "self-hosted",
            "CI_REQUIRED_RUNNER_ENVIRONMENT": "self-hosted",
            "CI_RUNNER_OS": "Linux",
            "CI_RUNNER_ARCH": "X64",
            "CI_RUNNER_NAME": "labpics-colors-ci-test",
            "CI_RUN_ID": "100",
            "CI_RUN_ATTEMPT": "2",
            "CI_CHECK_RUN_ID": "101",
        }

    def execute(self, environment, mode="bootstrap", program=None, check_run_id=_UNSET):
        with tempfile.TemporaryDirectory(prefix="owned-source-guard-") as name:
            root = Path(name)
            env_file, summary = root / "env", root / "summary"
            env_file.touch()
            summary.touch()
            env = dict(os.environ)
            environment = dict(environment)
            native_check = environment.get("CI_CHECK_RUN_ID") if check_run_id is _UNSET else check_run_id
            if isinstance(native_check, str) and native_check.isdigit():
                native_check = int(native_check)
            environment["CI_NATIVE_JOB"] = json.dumps({
                "workflow_sha": environment.get("CI_RECIPE_SHA"),
                "workflow_ref": environment.get("CI_RECIPE_REF"),
                "workflow_repository": environment.get("CI_RECIPE_REPOSITORY"),
                "workflow_file_path": environment.get("CI_RECIPE_PATH"),
                "check_run_id": native_check,
            })
            env.update(environment)
            env.update(GITHUB_ENV=str(env_file), GITHUB_STEP_SUMMARY=str(summary), GITHUB_OUTPUT=str(root / "outputs"))
            result = subprocess.run(
                [sys.executable, "-c", program or guard_program(mode)],
                env=env, capture_output=True, text=True, timeout=5,
            )
            outputs = root / "outputs"
            return result, env_file.read_text(), summary.read_text(), outputs.read_text() if outputs.exists() else ""

    def test_owned_source_has_nonempty_distinct_native_identities(self):
        environment = self.fixture()
        environment["CI_RECIPE_SHA"] = "c" * 40
        result, output, summary, outputs = self.execute(environment, mode="protected")
        self.assertEqual(result.returncode, 0, result.stderr)
        binding = json.loads(result.stdout)
        self.assertEqual(binding["source_sha"], "a" * 40)
        self.assertEqual(binding["recipe_sha"], "c" * 40)
        self.assertEqual(binding["identity_relation"], "distinct-revisions")
        self.assertIn("CI_VERIFIED_SOURCE_SHA=" + "a" * 40, output)
        self.assertIn("CI_VERIFIED_RECIPE_SHA=" + "c" * 40, output)
        self.assertNotIn("PASS", summary)
        lines = outputs.splitlines()
        self.assertEqual(len(lines), len(binding))
        self.assertEqual(dict(line.split("=", 1) for line in lines), binding)

    def test_native_check_run_number_is_positive_integer_only(self):
        for value in (True, False, 0, -1, 101.0, None):
            with self.subTest(value=value):
                result, output, summary, outputs = self.execute(self.fixture(), check_run_id=value)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("INCOMPLETE", result.stderr)
                self.assertEqual((output, summary, outputs), ("", "", ""))
        result, _, _, outputs = self.execute(self.fixture(), check_run_id=101)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["check_run_id"], "101")
        self.assertIn("check_run_id=101", outputs.splitlines())

    def test_metadata_control_characters_are_refused_before_identity_outputs(self):
        for field in ("CI_RUNNER_NAME", "CI_RUNNER_OS", "CI_RUNNER_ARCH", "CI_RECIPE_REF"):
            for control in ("\n", "\r", "\x7f"):
                with self.subTest(field=field, control=repr(control)):
                    environment = self.fixture()
                    environment[field] += control + "source_sha=" + "d" * 40
                    result, output, summary, outputs = self.execute(environment)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("REFUSED: control character", result.stderr)
                    self.assertEqual((output, summary, outputs), ("", "", ""))

    def require_guarded_condition(self, block):
        condition = re.search(r"(?ms)^    if: (.*?)(?=^    \S|\Z)", block)
        if condition and "always()" in condition.group(1):
            self.assertIn("needs.source-admission.result == 'success'", condition.group(1))

    def test_multiline_always_condition_cannot_hide_failed_admission(self):
        for marker in (">-", "|"):
            with self.subTest(marker=marker):
                prefix = "    if: " + marker + "\n      ${{ always()"
                suffix = " }}\n    runs-on: [self-hosted, Linux, X64]\n"
                with self.assertRaises(AssertionError):
                    self.require_guarded_condition(prefix + suffix)
                self.require_guarded_condition(prefix + " && needs.source-admission.result == 'success'" + suffix)

    def test_foreign_head_is_refused_before_any_published_identity(self):
        for field, value in (
            ("CI_HEAD_REPOSITORY", "foreign/lab-colors"),
            ("CI_HEAD_REPOSITORY_ID", "42"),
            ("CI_REPOSITORY", "foreign/lab-colors"),
            ("CI_REPOSITORY_ID", "42"),
        ):
            with self.subTest(field=field):
                environment = self.fixture()
                environment[field] = value
                result, output, summary, outputs = self.execute(environment)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("REFUSED", result.stderr)
                self.assertEqual((output, summary, outputs), ("", "", ""))

    def test_missing_or_malformed_actual_recipe_cannot_use_caller_as_fallback(self):
        for field in (
            "CI_RECIPE_SHA", "CI_RECIPE_REPOSITORY", "CI_RECIPE_PATH",
            "CI_RECIPE_REF", "CI_SOURCE_SHA", "CI_CALLER_SHA",
            "CI_RUN_ID", "CI_RUN_ATTEMPT", "CI_CHECK_RUN_ID",
        ):
            for value in ("", "0" * 40 if field.endswith("_SHA") else ""):
                with self.subTest(field=field, value=value):
                    environment = self.fixture()
                    environment[field] = value
                    result, output, summary, outputs = self.execute(environment)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("INCOMPLETE", result.stderr)
                    self.assertEqual((output, summary, outputs), ("", "", ""))

    def test_bootstrap_keeps_caller_recipe_snapshot_and_requires_native_context(self):
        environment = self.fixture()
        environment["CI_RECIPE_REF"] = "Labpics-Team/lab-colors/.github/workflows/ci-worker.yml@refs/pull/849/merge"
        result, _, _, outputs = self.execute(environment)
        self.assertEqual(result.returncode, 0, result.stderr)
        environment["CI_RECIPE_SHA"] = "c" * 40
        result, output, summary, outputs = self.execute(environment)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((output, summary, outputs), ("", "", ""))
        environment = self.fixture()
        environment["GITHUB_ACTIONS"] = "false"
        result, output, summary, outputs = self.execute(environment)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((output, summary, outputs), ("", "", ""))

    def test_protected_mode_rejects_unexecuted_main_recipe_and_wrong_callee(self):
        cases = (
            {"CI_EVENT": "push", "CI_REF": "refs/heads/main", "CI_RECIPE_SHA": "c" * 40},
            {"CI_RECIPE_REF": "Labpics-Team/lab-colors/.github/workflows/ci-worker.yml@refs/heads/other"},
            {"CI_RECIPE_PATH": ".github/workflows/other.yml"},
            {"CI_RECIPE_REPOSITORY": "foreign/repo"},
            {"CI_EVENT": "workflow_dispatch", "CI_REF": "refs/heads/other"},
        )
        for change in cases:
            with self.subTest(change=change):
                environment = self.fixture() | change
                result, output, summary, outputs = self.execute(environment, mode="protected")
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual((output, summary, outputs), ("", "", ""))

    def test_each_source_worker_is_bound_to_native_admission_before_steps(self):
        root = Path(__file__).resolve().parents[1] / ".github/workflows"
        expected_function = ast.dump(ast.parse(inspect.getsource(validate_owned_source)), include_attributes=False)
        workers = (
            "arb.yml", "ci-worker.yml", "dual-proof.yml", "full-domain-corpus.yml",
            "full-domain-run.yml", "mutation-worker.yml", "native-conformance-worker.yml",
            "package-worker.yml", "verification-lanes.yml", "publish-worker.yml",
            "trusted-source-smoke-worker.yml",
        )
        for filename in workers:
            with self.subTest(worker=filename):
                source = (root / filename).read_text()
                blocks = dict(re.findall(
                    r"(?m)^  ([a-zA-Z0-9_-]+):\n((?:(?!^  [a-zA-Z0-9_-]+:\n)[\s\S])*)",
                    source.split("\njobs:\n", 1)[1],
                ))
                admission = blocks["source-admission"]
                self.assertIn("CI_NATIVE_JOB: ${{ toJSON(job) }}", admission)
                self.assertNotIn("continue-on-error:", admission)
                if filename == "publish-worker.yml":
                    expected = (
                        "github.repository == 'Labpics-Team/lab-colors' && "
                        "github.event_name == 'push' && github.ref_type == 'tag' && "
                        "startsWith(github.ref_name, 'colors-v')"
                    )
                    for job in ("source-admission", "prepare", "publish"):
                        condition = re.search(r"(?ms)^    if: >-\n(.*?)(?=^    \S|\Z)", blocks[job])
                        self.assertIsNotNone(condition, job)
                        self.assertEqual(" ".join(line.strip() for line in condition.group(1).splitlines()), expected, job)
                guard = admission.split("          python3 - <<'PY'\n", 1)[1].split("          PY\n", 1)[0]
                program = "\n".join(line[10:] for line in guard.splitlines())
                observed_function = next(node for node in ast.parse(program).body if isinstance(node, ast.FunctionDef))
                self.assertEqual(ast.dump(ast.Module(body=[observed_function], type_ignores=[]), include_attributes=False), expected_function)
                for job, block in blocks.items():
                    if job == "source-admission":
                        continue
                    match = re.search(r"(?m)^    needs: (.+)$", block)
                    self.assertIsNotNone(match, job)
                    value = match.group(1)
                    needs = json.loads(value) if value.startswith("[") else [value]
                    self.assertIn("source-admission", needs, job)
                    self.require_guarded_condition(block)

    @staticmethod
    def require_self_hosted_selector(block):
        selector = re.search(r"(?m)^    runs-on: (.+)$", block)
        if selector is None:
            raise AssertionError("guarded job has no declared runner selector")
        labels = selector.group(1).strip().strip("[]").split(",")
        labels = [label.strip().strip("\"'") for label in labels]
        if "self-hosted" not in labels:
            raise AssertionError("self-hosted guard requires a self-hosted selector")

    def test_self_hosted_guard_and_runner_selector_agree(self):
        workflow = Path(__file__).resolve().parents[1] / ".github/workflows/native-conformance-worker.yml"
        source = workflow.read_text().split("\njobs:\n", 1)[1]
        blocks = dict(re.findall(
            r"(?m)^  ([a-zA-Z0-9_-]+):\n((?:(?!^  [a-zA-Z0-9_-]+:\n)[\s\S])*)",
            source,
        ))
        guarded = 0
        for job, block in blocks.items():
            if 'test "$ACTUAL_RUNNER_ENVIRONMENT" = self-hosted' in block:
                with self.subTest(job=job):
                    self.require_self_hosted_selector(block)
                    guarded += 1
        self.assertGreater(guarded, 0, "environment checks must be exercised")

    def test_hosted_selector_mutation_is_detected_with_legal_platform_neighbors(self):
        for selector in ("ubuntu-24.04", "macos-15"):
            with self.subTest(selector=selector):
                with self.assertRaisesRegex(AssertionError, "self-hosted selector"):
                    self.require_self_hosted_selector("    runs-on: " + selector + "\n")
        for selector in ("[self-hosted, Linux, X64]", "[self-hosted, macOS]"):
            with self.subTest(selector=selector):
                self.require_self_hosted_selector("    runs-on: " + selector + "\n")

    def test_reject_all_and_repo_only_mutants_are_detected(self):
        environment = self.fixture()
        healthy, _, _, outputs = self.execute(environment)
        self.assertEqual(healthy.returncode, 0, healthy.stderr)
        program = guard_program("bootstrap")
        reject_all = program.replace('    event = required("CI_EVENT")', '    raise ValueError("REFUSED: reject-all mutant")\n    event = required("CI_EVENT")')
        rejected, output, summary, outputs = self.execute(environment, program=reject_all)
        self.assertNotEqual(rejected.returncode, 0)
        self.assertEqual((output, summary, outputs), ("", "", ""))
        mutant = program.replace(
            "if head_repository != repository or head_id != repository_id:",
            "if False:",
        )
        self.assertNotEqual(mutant, program)
        foreign = environment | {"CI_HEAD_REPOSITORY_ID": "42"}
        healthy, _, _, outputs = self.execute(foreign)
        sabotaged, output, _, outputs = self.execute(foreign, program=mutant)
        self.assertNotEqual(healthy.returncode, 0)
        self.assertEqual(sabotaged.returncode, 0, sabotaged.stderr)
        self.assertIn("CI_VERIFIED_SOURCE_SHA", output)


if __name__ == "__main__":
    unittest.main()
