#!/usr/bin/env python3
"""Отрицательные контроли чтения формального результата; Kani запускается отдельно."""

import copy
import json
import os
import unittest
from unittest import mock
import signal
import subprocess
import tempfile
from pathlib import Path

from verify_core_formal import (
    CONTRACTS, KANI_VERSION,
    assemble_fragments, assert_snapshot, checkout_is_clean, checkout_snapshot, digest, formal_source_files,
    selected_mutants, selected_positive_harnesses,
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
    def test_checkout_ignores_foreign_git_repository_environment(self):
        root = Path(__file__).resolve().parent.parent
        sources = {path.relative_to(root).as_posix(): path.read_bytes() for path in formal_source_files()}
        oids = {path: f"{index:040x}".encode("ascii") for index, path in enumerate(sources, 1)}
        foreign = {
            "GIT_DIR": "/foreign/.git",
            "GIT_WORK_TREE": "/foreign",
            "GIT_INDEX_FILE": "/foreign/index",
            "GIT_OBJECT_DIRECTORY": "/foreign/objects",
            "GIT_CONFIG_COUNT": "1",
            "GIT_CONFIG_KEY_0": "core.repositoryformatversion",
            "GIT_CONFIG_VALUE_0": "99",
        }
        def git_output(command, **kwargs):
            if command[3] == "version":
                return "git version 2.51.0"
            if command[3] == "ls-files":
                return "H source.rs\0"
            if command[3] == "status":
                return ""
            if command[3] == "ls-tree":
                return b"".join((b"100755" if (root / path).stat().st_mode & 0o100 else b"100644")
                                + b" blob " + oid + b"\t" + path.encode() + b"\0"
                                for path, oid in oids.items())
            if command[3] == "cat-file":
                self.assertEqual(kwargs["input"], b"".join(oid + b"\n" for oid in oids.values()))
                return b"".join(oid + b" blob " + str(len(sources[path])).encode() + b"\n"
                                + sources[path] + b"\n" for path, oid in oids.items())
            return "local-head"

        with mock.patch.dict(os.environ, foreign), \
             mock.patch("verify_core_formal.subprocess.check_output", side_effect=git_output) as command, \
             mock.patch("verify_core_formal.subprocess.run"):
            identities, commit, clean = checkout_snapshot()
            self.assertTrue(clean)
            self.assertTrue(assert_snapshot(identities, commit))
        self.assertEqual(command.call_count, 12)
        for call in command.call_args_list:
            environment = call.kwargs["env"]
            self.assertFalse(set(foreign) & set(environment))
            self.assertEqual(environment["GIT_NO_REPLACE_OBJECTS"], "1")
            self.assertEqual(call.args[0][1:3], ["-c", "core.fsmonitor=false"])

        with mock.patch("verify_core_formal.git_checkout_output", return_value="git version 2.35.1"):
            with self.assertRaisesRegex(ValueError, "Git 2.35.2"):
                checkout_is_clean()

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

            def assemble_claimed_source():
                with mock.patch("verify_core_formal.checkout_snapshot", return_value=(common["verified_source_sha256"], common["checkout_commit"], True)), \
                     mock.patch("verify_core_formal.assert_snapshot", return_value=True):
                    assemble_fragments(root, root)

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

            with mock.patch("verify_core_formal.checkout_snapshot", return_value=({"source.rs": "actual-digest"}, common["checkout_commit"], True)):
                with self.assertRaisesRegex(ValueError, "source state"):
                    assemble_fragments(root, root)
            with mock.patch("verify_core_formal.checkout_snapshot", return_value=(common["verified_source_sha256"], "actual-head", True)):
                with self.assertRaisesRegex(ValueError, "source state"):
                    assemble_fragments(root, root)
            assemble_claimed_source()
            receipt = json.loads((root / "receipt.json").read_text(encoding="utf-8"))
            self.assertEqual(set(receipt["negative_sha256"]), {mutant[0] for mutant in MUTANTS})
            self.assertEqual(receipt["harnesses"], sorted(CONTRACTS))
            self.assertTrue(receipt["semantic_mutant_rejected"])
            validate_report(json.loads((root / "positive.json").read_text(encoding="utf-8")))

            with mock.patch("verify_core_formal.checkout_snapshot", return_value=(common["verified_source_sha256"], common["checkout_commit"], False)):
                with self.assertRaisesRegex(ValueError, "clean source state"):
                    assemble_fragments(root, root)
            with mock.patch("verify_core_formal.checkout_snapshot", return_value=(common["verified_source_sha256"], common["checkout_commit"], True)), \
                 mock.patch("verify_core_formal.assert_snapshot", return_value=False):
                with self.assertRaisesRegex(ValueError, "became dirty"):
                    assemble_fragments(root, root)

            unknown_fragment = root / "fragment-unknown.json"
            unknown_fragment.write_text(json.dumps({**common, "kind": "future"}), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "unknown formal fragment kind"):
                assemble_claimed_source()
            unknown_fragment.unlink()

            positive_fragment = root / "fragment-positive-4.json"
            saved_positive = positive_fragment.read_bytes()
            positive_fragment.unlink()
            with self.assertRaisesRegex(ValueError, "missing or duplicate positive shard"):
                assemble_claimed_source()
            positive_fragment.write_bytes(saved_positive)

            (root / "fragment-mutants-4.json").unlink()
            with self.assertRaisesRegex(ValueError, "missing or duplicate mutant shard"):
                assemble_claimed_source()

    def test_assembly_rejects_index_hidden_source_changes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            checkout = root / "checkout"
            evidence = root / "evidence"
            checkout.mkdir()
            evidence.mkdir()
            source = checkout / "formal.rs"
            source.write_bytes(b"trusted\n")
            other = checkout / "other.txt"
            other.write_bytes(b"trusted other\n")

            def git(*arguments):
                return subprocess.check_output(["git", *arguments], cwd=checkout)

            git("init", "-q")
            git("config", "core.autocrlf", "false")
            git("add", "--", source.name, other.name)
            git("-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                "commit", "-qm", "initial")

            positive = evidence / "positive.json"
            positive.write_text(json.dumps(valid_report()), encoding="utf-8")
            negatives = {}
            for mutant in MUTANTS:
                name = mutant[0]
                path = evidence / f"negative-{name}.json"
                path.write_text(json.dumps({"mutant": name}), encoding="utf-8")
                negatives[name] = digest(path)

            commit = git("rev-parse", "HEAD").decode("ascii").strip()

            def write_fragments(source_digest):
                common = {
                    "fragment_schema": 1,
                    "kani_version": KANI_VERSION,
                    "checkout_commit": commit,
                    "source_commit": commit,
                    "working_tree_clean": True,
                    "verified_source_sha256": {source.name: source_digest},
                }
                (evidence / "fragment-positive-0.json").write_text(json.dumps({
                    **common, "kind": "positive", "shard_index": 0, "shard_count": 1,
                    "positive_sha256": digest(positive), "harnesses": sorted(CONTRACTS),
                }), encoding="utf-8")
                (evidence / "fragment-mutants-0.json").write_text(json.dumps({
                    **common, "kind": "mutants", "shard_index": 0, "shard_count": 1,
                    "negative_sha256": negatives,
                }), encoding="utf-8")

            real_run = subprocess.run

            def run_without_cargo(command, *args, **kwargs):
                if command[:2] == ["cargo", "metadata"]:
                    return subprocess.CompletedProcess(command, 0)
                return real_run(command, *args, **kwargs)

            with mock.patch("verify_core_formal.ROOT", checkout), \
                 mock.patch("verify_core_formal.formal_source_files", return_value=[source]), \
                 mock.patch("verify_core_formal.subprocess.run", side_effect=run_without_cargo):
                write_fragments(digest(source))
                assemble_fragments(evidence, evidence)
                receipt = evidence / "receipt.json"
                self.assertEqual(json.loads(receipt.read_text(encoding="utf-8"))[
                    "verified_source_sha256"], {source.name: digest(source)})
                receipt.unlink()

                monitor = checkout / ".git" / "fsmonitor.sh"
                monitor.write_text("#!/bin/sh\nprintf 'token\\0'\n", encoding="ascii")
                monitor.chmod(0o755)
                git("config", "core.fsmonitor", ".git/fsmonitor.sh")
                self.assertEqual(git("status", "--porcelain", "--untracked-files=all").strip(), b"")
                source.write_bytes(b"tampered\n")
                self.assertEqual(git("status", "--porcelain", "--untracked-files=all").strip(), b"")
                self.assertEqual(git("ls-files", "-v", "--", source.name).strip(), b"H formal.rs")
                write_fragments(digest(source))
                with self.assertRaisesRegex(ValueError, "clean source state"):
                    assemble_fragments(evidence, evidence)
                self.assertFalse(receipt.exists())
                source.write_bytes(b"trusted\n")
                other.write_bytes(b"trusteX other\n")
                self.assertEqual(git("status", "--porcelain", "--untracked-files=all").strip(), b"")
                write_fragments(digest(source))
                with self.assertRaisesRegex(ValueError, "clean source state"):
                    assemble_fragments(evidence, evidence)
                self.assertFalse(receipt.exists())
                other.write_bytes(b"trusted other\n")
                git("config", "--unset", "core.fsmonitor")

                for flag, clear in (("--assume-unchanged", "--no-assume-unchanged"),
                                    ("--skip-worktree", "--no-skip-worktree")):
                    with self.subTest(flag=flag):
                        git("update-index", flag, "--", source.name)
                        source.write_bytes(b"tampered\n")
                        self.assertEqual(git("status", "--porcelain", "--untracked-files=all").strip(), b"")
                        self.assertNotEqual(source.read_bytes(), git("show", f"HEAD:{source.name}"))
                        write_fragments(digest(source))
                        with self.assertRaisesRegex(ValueError, "clean source state"):
                            assemble_fragments(evidence, evidence)
                        self.assertFalse(receipt.exists())
                        git("update-index", clear, "--", source.name)
                        source.write_bytes(b"trusted\n")

                fixed_time = 1_600_000_000_000_000_000
                os.utime(source, ns=(fixed_time, fixed_time))
                os.utime(other, ns=(fixed_time, fixed_time))
                git("add", "--", source.name, other.name)
                git("config", "core.trustctime", "false")
                try:
                    git("update-index", "--refresh")
                    original = source.stat()
                    other_original = other.stat()
                    source.write_bytes(b"trusteX\n")
                    os.utime(source, ns=(original.st_atime_ns, original.st_mtime_ns))
                    self.assertEqual(git("status", "--porcelain", "--untracked-files=all").strip(), b"")
                    self.assertEqual(git("ls-files", "-v", "--", source.name).strip(), b"H formal.rs")
                    self.assertNotEqual(source.read_bytes(), git("show", f"HEAD:{source.name}"))
                    write_fragments(digest(source))
                    with self.assertRaisesRegex(ValueError, "clean source state"):
                        assemble_fragments(evidence, evidence)
                    self.assertFalse(receipt.exists())
                    source.write_bytes(b"trusted\n")
                    os.utime(source, ns=(original.st_atime_ns, original.st_mtime_ns))
                    other.write_bytes(b"trusteX other\n")
                    os.utime(other, ns=(other_original.st_atime_ns, other_original.st_mtime_ns))
                    self.assertEqual(git("status", "--porcelain", "--untracked-files=all").strip(), b"")
                    write_fragments(digest(source))
                    with self.assertRaisesRegex(ValueError, "clean source state"):
                        assemble_fragments(evidence, evidence)
                    self.assertFalse(receipt.exists())
                finally:
                    git("config", "--unset", "core.trustctime")
                    source.write_bytes(b"trusted\n")
                    other.write_bytes(b"trusted other\n")


if __name__ == "__main__":
    unittest.main()
