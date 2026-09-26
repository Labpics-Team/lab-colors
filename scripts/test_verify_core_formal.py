#!/usr/bin/env python3
"""Отрицательные контроли чтения формального результата; Kani запускается отдельно."""

import copy
import json
import unittest
from unittest import mock
import signal
import subprocess
import tempfile
from pathlib import Path

from verify_core_formal import (
    CONTRACTS, KANI_VERSION,
    validate_report, validate_mutant, run_kani,
)


from formal_contracts import MUTANTS

MUTANT = MUTANTS[0]
MUTANT_HARNESS = MUTANT[2]
MUTANT_ASSERTION = json.dumps(MUTANT[3])


def valid_report():
    return {
        "metadata": {"kani_version": KANI_VERSION, "target": "x86_64-unknown-linux-gnu"},
        "verification_results": {
            "summary": {"total_harnesses": len(CONTRACTS), "executed": len(CONTRACTS), "successful": len(CONTRACTS),
                        "failed": 0, "status": "completed"},
            "results": [
                {"harness_id": name, "status": "Success", "checks": [
                    *({"category": "assertion", "description": json.dumps(label), "status": "Success"}
                      for label in sorted(assertions)),
                    *({"category": "cover", "description": label, "status": "Satisfied"}
                      for label in sorted(covers)),
                ]} for name, (assertions, covers) in CONTRACTS.items()
            ],
        },
    }


class FormalReportTests(unittest.TestCase):
    def test_complete_result_passes(self):
        validate_report(valid_report())

    def test_missing_duplicate_extra_or_foreign_harness_is_rejected(self):
        original = valid_report()
        results = original["verification_results"]["results"]
        for changed in ([], results[:-1], results + results[:1], [results[0]] * len(CONTRACTS)):
            with self.subTest(changed=changed):
                report = copy.deepcopy(original)
                report["verification_results"]["results"] = changed
                with self.assertRaises(ValueError):
                    validate_report(report)

    def test_non_successful_property_or_harness_is_rejected(self):
        for status in ("Failure", "Unknown", "Unreachable", "Unsatisfiable", "SolverError", ""):
            with self.subTest(status=status):
                report = valid_report()
                report["verification_results"]["results"][0]["checks"][0]["status"] = status
                with self.assertRaises(ValueError):
                    validate_report(report)
                report = valid_report()
                report["verification_results"]["results"][0]["status"] = status
                with self.assertRaises(ValueError):
                    validate_report(report)

    def test_deleted_semantic_assertion_or_cover_is_rejected(self):
        for category in ("assertion", "cover"):
            report = valid_report()
            checks = report["verification_results"]["results"][0]["checks"]
            checks.remove(next(check for check in checks if check["category"] == category))
            with self.assertRaises(ValueError):
                validate_report(report)

    def test_only_unnamed_generated_unreachable_checks_are_admissible(self):
        report = valid_report()
        report["verification_results"]["results"][0]["checks"].append({
            "category": "assertion", "description": "compiler-generated impossible branch", "status": "Unreachable"})
        validate_report(report)

    def test_unreachable_cover_is_not_success(self):
        report = valid_report()
        check = next(c for c in report["verification_results"]["results"][0]["checks"]
                     if c["category"] == "cover")
        check["status"] = "Unsatisfiable"
        with self.assertRaises(ValueError):
            validate_report(report)

    def test_incomplete_or_wrong_tool_result_is_rejected(self):
        for key, value in (("executed", 0), ("successful", 3), ("failed", 1), ("status", "timeout")):
            report = valid_report()
            report["verification_results"]["summary"][key] = value
            with self.assertRaises(ValueError):
                validate_report(report)
        for key in ("kani_version", "target"):
            report = valid_report()
            report["metadata"][key] = "wrong"
            with self.assertRaises(ValueError):
                validate_report(report)

    def test_timeout_terminates_the_solver_process_group(self):
        with tempfile.TemporaryDirectory() as temporary:
            with mock.patch("verify_core_formal.subprocess.Popen") as launch, mock.patch("verify_core_formal.os.killpg") as terminate:
                launch.return_value.pid = 4242
                launch.return_value.wait.side_effect = [subprocess.TimeoutExpired("cargo", 1200), -9]
                with self.assertRaises(subprocess.TimeoutExpired):
                    run_kani(Path(temporary) / "result.json")
                terminate.assert_called_once_with(4242, signal.SIGKILL)
                self.assertEqual(launch.return_value.wait.call_count, 2)
                self.assertTrue(launch.call_args.kwargs["start_new_session"])

    def test_completed_wrapper_still_cleans_its_solver_group(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "result.json"
            with mock.patch("verify_core_formal.subprocess.Popen") as launch, mock.patch("verify_core_formal.os.killpg") as terminate:
                launch.return_value.pid = 4243
                launch.return_value.returncode = 1
                def completed(*args, **kwargs):
                    path.write_text("{}")
                    return 1
                launch.return_value.wait.side_effect = completed
                self.assertEqual(run_kani(path), ({}, 1))
                terminate.assert_called_once_with(4243, signal.SIGKILL)

    def test_mutant_requires_target_semantic_failure(self):
        report = {"verification_results": {
            "summary": {"total_harnesses": 1, "executed": 1, "successful": 0,
                        "failed": 1, "status": "completed"},
            "results": [{"harness_id": MUTANT_HARNESS, "status": "Failure",
            "checks": [{"status": "Failure", "category": "assertion", "description": MUTANT_ASSERTION}]}]}}
        validate_mutant(report, 1, MUTANT)
        with self.assertRaises(ValueError):
            validate_mutant(report, 0, MUTANT)
        with self.assertRaises(ValueError):
            validate_mutant(report, 2, MUTANT)
        report["verification_results"]["results"][0]["checks"][0]["description"] = "compiler error"
        with self.assertRaises(ValueError):
            validate_mutant(report, 1, MUTANT)


if __name__ == "__main__":
    unittest.main()
