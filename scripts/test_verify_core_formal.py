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
    assemble_fragments, digest, selected_mutants, selected_positive_harnesses,
    validate_report, validate_mutant, run_kani,
)


from formal_contracts import MUTANTS, WCAG_INTERVAL_PARTITIONS

MUTANT = MUTANTS[0]
MUTANT_HARNESS = MUTANT[2]
MUTANT_ASSERTION = json.dumps(MUTANT[3])


def valid_report():
    return {
        "metadata": {
            "version": "1.0",
            "timestamp": "2026-10-02T00:00:00Z",
            "kani_version": KANI_VERSION,
            "target": "x86_64-unknown-linux-gnu",
            "build_mode": "release",
        },
        "project": {"crate_name": "labcolors-core", "workspace_root": "/workspace", "output_dir": "/tmp"},
        "tools": {"kani": KANI_VERSION},
        "harness_metadata": [],
        "error_details": [],
        "property_details": [],
        "cbmc": [],
        "verification_results": {
            "summary": {
                "total_harnesses": len(CONTRACTS),
                "executed": len(CONTRACTS),
                "successful": len(CONTRACTS),
                "failed": 0,
                "status": "completed",
                "duration_ms": len(CONTRACTS),
            },
            "results": [
                {"harness_id": name, "status": "Success", "duration_ms": 1, "checks": [
                    *({"category": "assertion", "description": json.dumps(label), "status": "Success"}
                      for label in sorted(assertions)),
                    *({"category": "cover", "description": label, "status": "Satisfied"}
                      for label in sorted(covers)),
                ]} for name, (assertions, covers) in CONTRACTS.items()
            ],
        },
        "coverage": {"enabled": False},
    }


def valid_subset_report(harnesses):
    report = valid_report()
    wanted = set(harnesses)
    report["verification_results"]["results"] = [
        result for result in report["verification_results"]["results"]
        if result["harness_id"] in wanted
    ]
    count = len(wanted)
    report["verification_results"]["summary"].update({
        "total_harnesses": count,
        "executed": count,
        "successful": count,
        "failed": 0,
        "duration_ms": count,
    })
    return report


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

    def test_each_wcag_partition_is_mandatory_even_with_adjusted_summary(self):
        for partition in WCAG_INTERVAL_PARTITIONS:
            with self.subTest(partition=partition):
                report = valid_report()
                verification = report["verification_results"]
                verification["results"] = [r for r in verification["results"]
                    if r["harness_id"] != f"wcag22::kernel::proofs::{partition}"]
                for field in ("total_harnesses", "executed", "successful"):
                    verification["summary"][field] -= 1
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
                self.assertIn("--jobs=4", launch.call_args.args[0])

    def test_positive_subset_uses_exact_multi_filter_without_concrete_playback(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "result.json"
            with mock.patch("verify_core_formal.subprocess.Popen") as launch, mock.patch(
                "verify_core_formal.os.killpg"
            ):
                launch.return_value.pid = 4244
                launch.return_value.returncode = 0

                def completed(*args, **kwargs):
                    path.write_text("{}")
                    return 0

                launch.return_value.wait.side_effect = completed
                run_kani(path, positive_harnesses=("proof::one", "proof::two"))
                command = launch.call_args.args[0]
                self.assertIn("--jobs=4", command)
                self.assertEqual(command.count("--harness"), 2)
                self.assertIn("proof::one", command)
                self.assertIn("proof::two", command)
                self.assertIn("--exact", command)
                self.assertNotIn("--concrete-playback", command)

    def test_exact_mutant_harness_keeps_concrete_playback_single_threaded(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "result.json"
            with mock.patch("verify_core_formal.subprocess.Popen") as launch, mock.patch(
                "verify_core_formal.os.killpg"
            ):
                launch.return_value.pid = 4245
                launch.return_value.returncode = 1

                def completed(*args, **kwargs):
                    path.write_text("{}")
                    return 1

                launch.return_value.wait.side_effect = completed
                run_kani(path, harness="demo::proof")
                command = launch.call_args.args[0]
                self.assertNotIn("--jobs=4", command)
                self.assertIn("--concrete-playback", command)

    def test_target_failure_may_make_its_later_cover_unreachable(self):
        report = {"verification_results": {
            "summary": {"total_harnesses": 1, "executed": 1, "successful": 0,
                        "failed": 1, "status": "completed"},
            "results": [{"harness_id": MUTANT_HARNESS, "status": "Failure", "checks": [
                {"category": "assertion", "status": "Failure", "description": MUTANT_ASSERTION},
                {"category": "cover", "status": "Unsatisfiable", "description": sorted(CONTRACTS[MUTANT_HARNESS][1])[0]},
            ]}],
        }}
        validate_mutant(report, 1, MUTANT)
        report["verification_results"]["results"][0]["checks"][1]["description"] = "unknown witness"
        with self.assertRaises(ValueError):
            validate_mutant(report, 1, MUTANT)

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


    def test_mutant_shards_cover_each_mutant_exactly_once(self):
        selected = []
        for index in range(5):
            shard = selected_mutants(index, 5)
            self.assertTrue(shard)
            selected.extend(mutant[0] for mutant in shard)
        expected = [mutant[0] for mutant in MUTANTS]
        self.assertCountEqual(selected, expected)
        self.assertEqual(len(selected), len(set(selected)))

    def test_positive_shards_cover_each_harness_exactly_once(self):
        selected = []
        for index in range(5):
            shard = selected_positive_harnesses(index, 5)
            self.assertTrue(shard)
            selected.extend(shard)
        self.assertCountEqual(selected, CONTRACTS)
        self.assertEqual(len(selected), len(set(selected)))
        with self.assertRaises(ValueError):
            selected_positive_harnesses(0, 4)

    def test_fragment_assembly_requires_exact_complete_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            common = {
                "fragment_schema": 1,
                "kani_version": KANI_VERSION,
                "checkout_commit": "abc123",
                "source_commit": "abc123",
                "working_tree_clean": True,
                "verified_source_sha256": {"source.rs": "source-digest"},
            }

            for index in range(5):
                harnesses = selected_positive_harnesses(index, 5)
                path = root / f"positive-{index}.json"
                path.write_text(json.dumps(valid_subset_report(harnesses)), encoding="utf-8")
                (root / f"fragment-positive-{index}.json").write_text(json.dumps({
                    **common,
                    "kind": "positive",
                    "shard_index": index,
                    "shard_count": 5,
                    "file": path.name,
                    "sha256": digest(path),
                    "harnesses": list(harnesses),
                }), encoding="utf-8")

            for index in range(5):
                negatives = {}
                for mutant in selected_mutants(index, 5):
                    name = mutant[0]
                    path = root / f"negative-{name}.json"
                    path.write_text(json.dumps({"mutant": name}), encoding="utf-8")
                    negatives[name] = digest(path)
                (root / f"fragment-mutants-{index}.json").write_text(json.dumps({
                    **common,
                    "kind": "mutants",
                    "shard_index": index,
                    "shard_count": 5,
                    "negative_sha256": negatives,
                }), encoding="utf-8")

            assemble_fragments(root, root)
            receipt = json.loads((root / "receipt.json").read_text(encoding="utf-8"))
            self.assertEqual(set(receipt["negative_sha256"]), {mutant[0] for mutant in MUTANTS})
            self.assertEqual(receipt["harnesses"], sorted(CONTRACTS))
            self.assertTrue(receipt["semantic_mutant_rejected"])
            validate_report(json.loads((root / "positive.json").read_text(encoding="utf-8")))

            positive_fragment = root / "fragment-positive-4.json"
            saved_positive = positive_fragment.read_bytes()
            positive_fragment.unlink()
            with self.assertRaisesRegex(ValueError, "missing or duplicate positive shard"):
                assemble_fragments(root, root)
            positive_fragment.write_bytes(saved_positive)

            (root / "fragment-mutants-4.json").unlink()
            with self.assertRaisesRegex(ValueError, "missing or duplicate mutant shard"):
                assemble_fragments(root, root)


if __name__ == "__main__":
    unittest.main()
