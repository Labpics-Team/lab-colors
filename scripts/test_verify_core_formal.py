#!/usr/bin/env python3
"""Отрицательные контроли чтения формального результата; Kani запускается отдельно."""

import copy
from contextlib import contextmanager
import json
import os
import select
import sys
import time
import unittest
from unittest import mock
import signal
import subprocess
import tempfile
import zlib
from pathlib import Path

from verify_core_formal import (
    CONTRACTS, KANI_VERSION,
    assemble_fragments, assert_snapshot, checkout_is_clean, checkout_snapshot, digest, formal_source_files,
    fragment_base,
    selected_mutants, selected_positive_harnesses,
    validate_report, validate_mutant, run_kani, _run_logged_command,
)


from formal_contracts import MUTANTS, WCAG_INTERVAL_PARTITIONS

HANDOFF_BASES = (
    "preparation_is_exact_and_has_no_publication_effect",
    "rejected_install_is_atomic_retryable_and_success_is_once_only",
    "stale_prepared_command_never_reaches_the_host",
)
HANDOFF_PARTITIONS = ("set_all", "revoke_all", "confirm_exact")
HANDOFF_HARNESSES = tuple(f"program::attachment::handoff::proofs::{base}_{partition}"
    for base in HANDOFF_BASES for partition in HANDOFF_PARTITIONS)

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
    def setUp(self):
        self.command_log = Path(self.enterContext(tempfile.TemporaryDirectory())) / "command.log"
        self.enterContext(mock.patch("verify_core_formal._open_live_output_fd", side_effect=lambda: os.dup(1)))
        if not hasattr(signal, "SIGKILL"):
            # Windows only exercises the mocked Linux launcher contract.
            self.enterContext(mock.patch("verify_core_formal.signal.SIGKILL", 9, create=True))
            self.enterContext(mock.patch("verify_core_formal._open_regular_log", side_effect=lambda log: log.open("wb")))
            self.enterContext(mock.patch("verify_core_formal._open_log_reader", side_effect=lambda writer: Path(writer.name).open("rb")))

    def test_checkout_rejects_corrupt_blob_even_when_status_and_bytes_match(self):
        with tempfile.TemporaryDirectory() as temporary:
            checkout = Path(temporary)
            source = checkout / "formal.rs"
            source.write_bytes(b"trusted\n")

            def git(*arguments):
                return subprocess.check_output(["git", *arguments], cwd=checkout)

            git("init", "-q")
            git("config", "core.autocrlf", "false")
            git("config", "commit.gpgsign", "false")
            git("add", "--", source.name)
            git("-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                "commit", "-qm", "initial")
            fixed_time = 1_600_000_000_000_000_000
            os.utime(source, ns=(fixed_time, fixed_time))
            git("add", "--", source.name)
            git("config", "core.trustctime", "false")
            git("update-index", "--refresh")
            commit = git("rev-parse", "HEAD").decode("ascii").strip()

            with mock.patch("verify_core_formal.ROOT", checkout), \
                 mock.patch("verify_core_formal.core_tree_has_no_physical_extras", return_value=True):
                identities = {source.name: digest(source)}
                self.assertTrue(checkout_is_clean(identities, commit))

                original = source.stat()
                source.write_bytes(b"trusteX\n")
                os.utime(source, ns=(original.st_atime_ns, original.st_mtime_ns))
                oid = git("rev-parse", f"HEAD:{source.name}").decode("ascii").strip()
                object_path = checkout / ".git" / "objects" / oid[:2] / oid[2:]
                self.assertTrue(object_path.is_file())
                object_path.chmod(0o600)
                payload = source.read_bytes()
                object_path.write_bytes(zlib.compress(b"blob " + str(len(payload)).encode("ascii") + b"\0" + payload))

                self.assertEqual(git("status", "--porcelain", "--untracked-files=all").strip(), b"")
                self.assertNotEqual(subprocess.run(
                    ["git", "fsck", "--strict", "--no-reflogs", "--no-progress", "--no-dangling", commit],
                    cwd=checkout, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                ).returncode, 0)
                self.assertFalse(checkout_is_clean({source.name: digest(source)}, commit))

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
             mock.patch("verify_core_formal.core_tree_has_no_physical_extras", return_value=True), \
             mock.patch("verify_core_formal.subprocess.run"):
            identities, commit, clean = checkout_snapshot()
            self.assertTrue(clean)
            self.assertTrue(assert_snapshot(identities, commit))
        self.assertEqual(command.call_count, 14)
        for call in command.call_args_list:
            environment = call.kwargs["env"]
            self.assertFalse(set(foreign) & set(environment))
            self.assertEqual(environment["GIT_NO_REPLACE_OBJECTS"], "1")
            self.assertEqual(call.args[0][1:3], ["-c", "core.fsmonitor=false"])

        with mock.patch("verify_core_formal.git_checkout_output", return_value="git version 2.35.1"):
            with self.assertRaisesRegex(ValueError, "Git 2.35.2"):
                checkout_is_clean()

    def test_checkout_rejects_ignored_core_extras_but_allows_workspace_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            checkout = Path(temporary)
            core = checkout / "crates/labcolors-core"
            source = core / "src/lib.rs"
            source.parent.mkdir(parents=True)
            source.write_bytes(b"trusted source\n")
            (checkout / ".gitignore").write_text("target/\n", encoding="ascii")

            def git(*arguments):
                return subprocess.check_output(["git", *arguments], cwd=checkout)

            git("init", "-q")
            git("config", "core.autocrlf", "false")
            git("add", "--", ".gitignore", "crates/labcolors-core/src/lib.rs")
            git("-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                "commit", "-qm", "initial")
            (checkout / "target").mkdir()
            (checkout / "target/workspace-build.txt").write_bytes(b"ordinary build output\n")
            self.assertEqual(git("status", "--porcelain", "--untracked-files=all").strip(), b"")

            real_run = subprocess.run

            def run_without_cargo(command, *args, **kwargs):
                if command[:2] == ["cargo", "metadata"]:
                    return subprocess.CompletedProcess(command, 0)
                return real_run(command, *args, **kwargs)

            with mock.patch("verify_core_formal.ROOT", checkout), \
                 mock.patch("verify_core_formal.formal_source_files", return_value=[source]), \
                 mock.patch("verify_core_formal.subprocess.run", side_effect=run_without_cargo):
                identities, commit, clean = checkout_snapshot()
                self.assertTrue(clean)
                self.assertTrue(assert_snapshot(identities, commit))

                ignored_dir = core / "target"
                ignored_dir.mkdir()
                (ignored_dir / "ignored-marker.txt").write_bytes(b"changes compiled source identity\n")
                self.assertEqual(git("status", "--porcelain", "--untracked-files=all").strip(), b"")
                self.assertFalse(checkout_snapshot()[2])
                self.assertFalse(assert_snapshot(identities, commit))

                (ignored_dir / "ignored-marker.txt").unlink()
                self.assertFalse(checkout_snapshot()[2], "even an empty extra directory changes the Core tree")
                ignored_dir.rmdir()
                self.assertTrue(checkout_snapshot()[2])

    @unittest.skipUnless(os.name == "posix", "requires POSIX symlink semantics")
    def test_checkout_rejects_tracked_core_symlink_to_ignored_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            checkout = Path(temporary)
            source = checkout / "crates/labcolors-core/src/numerics.rs"
            source.parent.mkdir(parents=True)
            source.write_bytes(b"regular source\n")
            (checkout / ".gitignore").write_text("target/\n", encoding="ascii")
            target = checkout / "target/numerics.rs"
            target.parent.mkdir()
            target.write_bytes(b"mutable source\n")

            def git(*arguments):
                return subprocess.check_output(["git", *arguments], cwd=checkout)

            git("init", "-q")
            git("config", "core.autocrlf", "false")
            git("config", "core.symlinks", "true")
            git("add", "--", ".gitignore", "crates/labcolors-core/src/numerics.rs")
            git("-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                "commit", "-qm", "regular source")
            self.assertEqual(git("status", "--porcelain", "--untracked-files=all").strip(), b"")

            real_run = subprocess.run

            def run_without_cargo(command, *args, **kwargs):
                if command[:2] == ["cargo", "metadata"]:
                    return subprocess.CompletedProcess(command, 0)
                return real_run(command, *args, **kwargs)

            with mock.patch("verify_core_formal.ROOT", checkout), \
                 mock.patch("verify_core_formal.formal_source_files", return_value=[source]), \
                 mock.patch("verify_core_formal.subprocess.run", side_effect=run_without_cargo):
                self.assertTrue(checkout_snapshot()[2], "regular Core source must remain admissible")

                source.unlink()
                source.symlink_to("../../../target/numerics.rs")
                git("add", "--", "crates/labcolors-core/src/numerics.rs")
                git("-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                    "commit", "-qm", "tracked symlink")
                mode = git("ls-files", "-s", "--", "crates/labcolors-core/src/numerics.rs").split(b" ", 1)[0]
                self.assertEqual(mode, b"120000")
                self.assertEqual(git("status", "--porcelain", "--untracked-files=all").strip(), b"")
                before, _, clean = checkout_snapshot()
                self.assertFalse(clean)

                target.write_bytes(b"changed while Git remains clean\n")
                self.assertEqual(git("status", "--porcelain", "--untracked-files=all").strip(), b"")
                after, _, clean = checkout_snapshot()
                self.assertNotEqual(after, before)
                self.assertFalse(clean)

    @unittest.skipUnless(os.name == "posix", "requires POSIX symlink semantics")
    def test_checkout_rejects_symlinks_for_each_formal_input_outside_core(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            origin = root / "origin"
            checkout = root / "checkout"
            origin.mkdir()
            manifest_bytes = (b'[package]\nname = "formal-source-link"\n'
                              b'version = "0.1.0"\nedition = "2021"\n')
            script_bytes = b"CONTRACTS = {}\n"
            (origin / "Cargo.toml").write_bytes(manifest_bytes)
            (origin / ".gitignore").write_text("target/\n", encoding="ascii")
            (origin / "src").mkdir()
            (origin / "src/lib.rs").write_bytes(b"pub fn witness() {}\n")
            (origin / "crates/labcolors-core").mkdir(parents=True)
            (origin / "crates/labcolors-core/README.md").write_bytes(b"Core fixture\n")
            (origin / "scripts").mkdir()
            (origin / "scripts/formal_contracts.py").write_bytes(script_bytes)
            (origin / "docs").mkdir()
            (origin / "docs/guide.md").symlink_to("../target/guide.md")

            def git(directory, *arguments):
                return subprocess.check_output(["git", *arguments], cwd=directory)

            git(origin, "init", "-q")
            git(origin, "config", "core.autocrlf", "false")
            git(origin, "config", "core.symlinks", "true")
            subprocess.run(["cargo", "generate-lockfile", "--offline"], cwd=origin,
                           stdout=subprocess.DEVNULL, check=True)
            git(origin, "add", "-A")
            git(origin, "-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                "commit", "-qm", "regular formal inputs")
            git(root, "clone", "-q", str(origin), str(checkout))
            git(checkout, "config", "core.autocrlf", "false")
            git(checkout, "config", "core.symlinks", "true")
            manifest = checkout / "Cargo.toml"
            script = checkout / "scripts/formal_contracts.py"
            ignored = checkout / "target"
            (ignored / "src").mkdir(parents=True)
            (ignored / "Cargo.toml").write_bytes(manifest_bytes)
            (ignored / "src/lib.rs").write_bytes(b"pub fn witness() {}\n")
            (ignored / "formal_contracts.py").write_bytes(script_bytes)
            (ignored / "guide.md").write_bytes(b"unrelated documentation\n")

            def commit(message, path):
                git(checkout, "add", "--", path)
                git(checkout, "-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                    "commit", "-qm", message)

            with mock.patch("verify_core_formal.ROOT", checkout), \
                 mock.patch("verify_core_formal.formal_source_files", return_value=[manifest, script]):
                identities, head, clean = checkout_snapshot()
                self.assertTrue(clean, "regular formal inputs and unrelated docs link remain admissible")
                self.assertEqual(fragment_base(identities, head, clean)["source_commit"], head)

                manifest.unlink()
                manifest.symlink_to("target/Cargo.toml")
                commit("linked manifest", "Cargo.toml")
                self.assertEqual(git(checkout, "ls-files", "-s", "--", "Cargo.toml").split(b" ", 1)[0], b"120000")
                self.assertEqual(git(checkout, "status", "--porcelain", "--untracked-files=all").strip(), b"")
                before, head, clean = checkout_snapshot()
                self.assertFalse(clean)
                self.assertIsNone(fragment_base(before, head, clean)["source_commit"])

                (ignored / "Cargo.toml").write_bytes(manifest_bytes + b'description = "mutable"\n')
                self.assertEqual(git(checkout, "status", "--porcelain", "--untracked-files=all").strip(), b"")
                after, head, clean = checkout_snapshot()
                self.assertNotEqual(after, before)
                self.assertFalse(clean)

                manifest.unlink()
                manifest.write_bytes(manifest_bytes)
                commit("regular manifest", "Cargo.toml")
                self.assertTrue(checkout_snapshot()[2])

                script.unlink()
                script.symlink_to("../target/formal_contracts.py")
                commit("linked formal contract", "scripts/formal_contracts.py")
                self.assertEqual(git(checkout, "ls-files", "-s", "--", "scripts/formal_contracts.py").split(b" ", 1)[0], b"120000")
                self.assertEqual(git(checkout, "status", "--porcelain", "--untracked-files=all").strip(), b"")
                self.assertFalse(checkout_snapshot()[2])

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

        self.assertEqual(len(CONTRACTS), 47)
        self.assertEqual(set(HANDOFF_HARNESSES),
                         {name for name in CONTRACTS if "::handoff::proofs::" in name})
        for harness in HANDOFF_HARNESSES:
            with self.subTest(harness=harness):
                report = valid_report()
                verification = report["verification_results"]
                verification["results"] = [r for r in verification["results"]
                    if r["harness_id"] != harness]
                for field in ("total_harnesses", "executed", "successful"):
                    verification["summary"][field] -= 1
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

        for harness in HANDOFF_HARNESSES:
            for category, labels in zip(("assertion", "cover"), CONTRACTS[harness], strict=True):
                for label in labels:
                    with self.subTest(harness=harness, category=category, label=label):
                        report = valid_report()
                        result = next(r for r in report["verification_results"]["results"]
                                      if r["harness_id"] == harness)
                        description = json.dumps(label) if category == "assertion" else label
                        result["checks"] = [c for c in result["checks"]
                            if c["category"] != category or c["description"] != description]
                        with self.assertRaises(ValueError):
                            validate_report(report)
            report = valid_report()
            result = next(r for r in report["verification_results"]["results"]
                          if r["harness_id"] == harness)
            result["checks"].append({"category": "cover", "description": "foreign operation cover",
                                     "status": "Satisfied"})
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
            with mock.patch("verify_core_formal.subprocess.Popen") as launch, mock.patch(
                "verify_core_formal.os.killpg", create=True
            ) as terminate, mock.patch("verify_core_formal.time.monotonic", side_effect=[0, 0, 1200]):
                launch.return_value.pid = 4242
                launch.return_value.poll.return_value = None
                launch.return_value.wait.side_effect = [subprocess.TimeoutExpired("cargo", 0.1), -9]
                with self.assertRaises(subprocess.TimeoutExpired):
                    run_kani(Path(temporary) / "result.json")
                terminate.assert_called_once_with(4242, signal.SIGKILL)
                launch.return_value.wait.assert_has_calls([mock.call(timeout=0.1), mock.call()])
                self.assertTrue(launch.call_args.kwargs["start_new_session"])

    def test_completed_wrapper_still_cleans_its_solver_group(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "result.json"
            with mock.patch("verify_core_formal.subprocess.Popen") as launch, mock.patch("verify_core_formal.os.killpg", create=True) as terminate:
                launch.return_value.pid = 4243
                launch.return_value.poll.return_value = 1
                def completed(*args, **kwargs):
                    path.write_text("{}")
                    return 1
                launch.return_value.wait.side_effect = completed
                self.assertEqual(run_kani(path), ({}, 1))
                terminate.assert_called_once_with(4243, signal.SIGKILL)
                self.assertIn("--jobs=1", launch.call_args.args[0])

    def test_positive_subset_uses_exact_multi_filter_without_concrete_playback(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "result.json"
            with mock.patch("verify_core_formal.subprocess.Popen") as launch, mock.patch(
                "verify_core_formal.os.killpg", create=True
            ):
                launch.return_value.pid = 4244
                launch.return_value.poll.return_value = 0

                def completed(*args, **kwargs):
                    path.write_text("{}")
                    return 0

                launch.return_value.wait.side_effect = completed
                run_kani(path, positive_harnesses=("proof::one", "proof::two"))
                command = launch.call_args.args[0]
                self.assertIn("--jobs=1", command)
                self.assertEqual(command.count("--harness"), 2)
                self.assertIn("proof::one", command)
                self.assertIn("proof::two", command)
                self.assertIn("--exact", command)
                self.assertNotIn("--concrete-playback", command)

    def test_exact_mutant_harness_keeps_concrete_playback_single_threaded(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "result.json"
            with mock.patch("verify_core_formal.subprocess.Popen") as launch, mock.patch(
                "verify_core_formal.os.killpg", create=True
            ):
                launch.return_value.pid = 4245
                launch.return_value.poll.return_value = 1

                def completed(*args, **kwargs):
                    path.write_text("{}")
                    return 1

                launch.return_value.wait.side_effect = completed
                run_kani(path, harness="demo::proof")
                command = launch.call_args.args[0]
                self.assertFalse(any(arg.startswith("--jobs") for arg in command))
                self.assertIn("--concrete-playback", command)

    def test_direct_command_launch_preserves_argv_and_uses_regular_logfile(self):
        command = ["cargo", "kani", "space and 'quote", "$(not-a-shell-command)"]
        with mock.patch("verify_core_formal.subprocess.Popen") as launch, mock.patch(
            "verify_core_formal.os.killpg", create=True
        ):
            launch.return_value.pid = 4246
            launch.return_value.poll.return_value = 0
            launch.return_value.wait.return_value = 0
            self.assertEqual(_run_logged_command(command, self.command_log), 0)
            self.assertEqual(launch.call_args.args[0], command)
            self.assertTrue(launch.call_args.kwargs["stdout"].closed)
            self.assertEqual(self.command_log.read_bytes(), b"")
            self.assertEqual(launch.call_args.kwargs["stdout"].mode, "wb")
            self.assertEqual(launch.call_args.kwargs["stderr"], subprocess.STDOUT)

    def test_cancellation_during_spawn_still_cleans_group_and_restores_handlers(self):
        previous = {signum: signal.getsignal(signum) for signum in (signal.SIGTERM, signal.SIGINT)}
        for signum in previous:
            with self.subTest(signum=signum), mock.patch(
                "verify_core_formal.subprocess.Popen"
            ) as launch, mock.patch("verify_core_formal.os.killpg", create=True) as terminate:
                process = mock.Mock(pid=4247)
                process.wait.return_value = 0

                def spawned(*args, **kwargs):
                    signal.getsignal(signum)(signum, None)
                    return process

                launch.side_effect = spawned
                with self.assertRaises(SystemExit) as stopped:
                    _run_logged_command(["unused"], self.command_log)
                self.assertEqual(stopped.exception.code, 128 + signum)
                terminate.assert_called_once_with(4247, signal.SIGKILL)
                process.wait.assert_called_once_with()
                for restored, handler in previous.items():
                    self.assertEqual(signal.getsignal(restored), handler)

    def test_cancellation_during_cleanup_is_not_success(self):
        with mock.patch("verify_core_formal.subprocess.Popen") as launch, mock.patch(
            "verify_core_formal.os.killpg", create=True
        ) as terminate:
            launch.return_value.pid = 4248
            launch.return_value.poll.return_value = 0
            launch.return_value.wait.return_value = 0
            terminate.side_effect = lambda *args: signal.getsignal(signal.SIGINT)(signal.SIGINT, None)
            with self.assertRaises(SystemExit) as stopped:
                _run_logged_command(["unused"], self.command_log)
            self.assertEqual(stopped.exception.code, 130)

    def test_spawn_failure_restores_signal_handlers(self):
        previous = {signum: signal.getsignal(signum) for signum in (signal.SIGTERM, signal.SIGINT)}
        with mock.patch("verify_core_formal.subprocess.Popen", side_effect=OSError("spawn failed")):
            with self.assertRaisesRegex(OSError, "spawn failed"):
                _run_logged_command(["unused"], self.command_log)
        for signum, handler in previous.items():
            self.assertEqual(signal.getsignal(signum), handler)

    def test_parent_log_read_failure_returns_125_and_cleans_command_group(self):
        reader = mock.MagicMock()
        reader.__enter__.return_value = reader
        reader.read.side_effect = OSError("read failed")
        with mock.patch("verify_core_formal._open_log_reader", return_value=reader), mock.patch(
            "verify_core_formal.subprocess.Popen"
        ) as launch, mock.patch("verify_core_formal.os.killpg", create=True) as terminate:
            launch.return_value.pid = 4249
            launch.return_value.poll.return_value = 0
            launch.return_value.wait.return_value = 0
            self.assertEqual(_run_logged_command(["unused"], self.command_log), 125)
            terminate.assert_called_once_with(4249, signal.SIGKILL)

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

        handoff_mutants = [mutant for mutant in MUTANTS if mutant[0].startswith("handoff-")]
        expected = {("handoff-foreign-confirmation",
                     f"program::attachment::handoff::proofs::{HANDOFF_BASES[0]}_confirm_exact")}
        for mutant_name, base in (("handoff-swallowed-host-refusal", HANDOFF_BASES[1]),
                                 ("handoff-repeated-install", HANDOFF_BASES[1]),
                                 ("handoff-stale-install", HANDOFF_BASES[2])):
            expected.update((f"{mutant_name}-{partition}",
                             f"program::attachment::handoff::proofs::{base}_{partition}")
                            for partition in HANDOFF_PARTITIONS)
        self.assertEqual(len(MUTANTS), 30)
        self.assertEqual(len(handoff_mutants), 10)
        self.assertEqual({(mutant[0], mutant[2]) for mutant in handoff_mutants}, expected)
        for mutant in handoff_mutants:
            with self.subTest(mutant=mutant[0]):
                report = valid_subset_report((mutant[2],))
                verification = report["verification_results"]
                verification["summary"].update({"successful": 0, "failed": 1})
                result = verification["results"][0]
                result["status"] = "Failure"
                next(c for c in result["checks"]
                     if c["description"] == json.dumps(mutant[3]))["status"] = "Failure"
                validate_mutant(report, 1, mutant)
                result["harness_id"] = next(h for h in HANDOFF_HARNESSES if h != mutant[2])
                with self.assertRaises(ValueError):
                    validate_mutant(report, 1, mutant)


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

            for mutant in MUTANTS:
                if not mutant[0].startswith("handoff-"):
                    continue
                name = mutant[0]
                index = next(i for i in range(5)
                             if mutant in selected_mutants(i, 5))
                fragment_path = root / f"fragment-mutants-{index}.json"
                saved_fragment = fragment_path.read_bytes()
                fragment = json.loads(saved_fragment)
                del fragment["negative_sha256"][name]
                fragment_path.write_text(json.dumps(fragment), encoding="utf-8")
                with self.subTest(missing_mutant=name), self.assertRaises(ValueError):
                    assemble_claimed_source()
                fragment_path.write_bytes(saved_fragment)

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
                 mock.patch("verify_core_formal.core_tree_has_no_physical_extras", return_value=True), \
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


@unittest.skipUnless(sys.platform == "linux", "relay uses Linux process groups and proc descriptors")
class FormalRelayProcessTests(unittest.TestCase):
    @contextmanager
    def driver(self, directory, code, *, args=(), timeout=5, log=None, stdout=subprocess.PIPE):
        log = log or directory / "command.log"
        group = directory / "group"
        group.unlink(missing_ok=True)
        command = [sys.executable, "-u", "-c", (
            "import os; from pathlib import Path; "
            f"Path({str(group)!r}).write_text(str(os.getpgrp()))\n" + code
        ), *args]
        launcher = (
            "import json, subprocess, sys; from pathlib import Path; "
            f"sys.path.insert(0, {str(Path(__file__).resolve().parent)!r})\n"
            "from verify_core_formal import _run_logged_command\n"
            "try:\n"
            "    result = _run_logged_command(json.loads(sys.argv[1]), Path(sys.argv[2]), "
            "timeout=float(sys.argv[3]))\n"
            "except subprocess.TimeoutExpired:\n"
            "    sys.exit(124)\n"
            "sys.exit(result)\n"
        )
        process = subprocess.Popen(
            [sys.executable, "-u", "-c", launcher, json.dumps(command), str(log), str(timeout)],
            stdout=stdout, stderr=subprocess.STDOUT, start_new_session=True,
        )
        try:
            yield process, log, group
        finally:
            for pid in (int(group.read_text()) if group.exists() else None, process.pid):
                if pid is not None:
                    try:
                        os.killpg(pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
            process.wait(timeout=10)
            if process.stdout is not None:
                process.stdout.close()

    def test_command_status_and_literal_arguments_survive_relay(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            arguments = ["space and 'quote", "$(not-a-shell-command)", "semicolon;literal"]
            for status in (0, 1):
                with self.subTest(status=status), self.driver(
                    directory,
                    f"import json, sys; print(json.dumps(sys.argv[1:])); sys.exit({status})",
                    args=arguments,
                ) as (process, log, group):
                    output, _ = process.communicate(timeout=10)
                    self.assertEqual(process.returncode, status)
                    self.assertEqual(json.loads(output), arguments)
                    self.assertEqual(log.read_bytes(), output)

    def test_final_tail_larger_than_a_chunk_is_relayed_exactly(self):
        payload = b"part" * 32769 + b"\nfinal-tail\x00"
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for status in (0, 1):
                with self.subTest(status=status), self.driver(
                    directory,
                    "import sys; sys.stdout.buffer.write(b'part' * 32769 + b'\\nfinal-tail\\x00'); "
                    f"sys.exit({status})",
                ) as (process, log, group):
                    output, _ = process.communicate(timeout=10)
                    self.assertEqual(process.returncode, status)
                    self.assertEqual(output, payload)
                    self.assertEqual(log.read_bytes(), payload)

    def test_regular_stdout_preserves_existing_position(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            destination = directory / "stdout"
            with destination.open("wb") as sink:
                sink.write(b"previous-output\n")
                sink.flush()
                with self.driver(directory, "print('command-output')", stdout=sink) as (process, log, group):
                    self.assertEqual(process.wait(timeout=5), 0)
                    self.assertEqual(destination.read_bytes(), b"previous-output\ncommand-output\n")
                    self.assertEqual(log.read_bytes(), b"command-output\n")

    def test_cancellation_during_final_drain_preserves_signal_status(self):
        import fcntl
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for signum in (signal.SIGTERM, signal.SIGINT):
                read_fd, write_fd = os.pipe()
                try:
                    capacity = fcntl.fcntl(write_fd, fcntl.F_SETPIPE_SZ, 4096)
                    os.write(write_fd, b"p" * capacity)
                    with self.subTest(signum=signum), self.driver(
                        directory, "print('final-tail')", stdout=write_fd,
                    ) as (process, log, group):
                        deadline = time.monotonic() + 3
                        while not group.exists() or Path(f"/proc/{group.read_text()}/stat").exists():
                            self.assertIsNone(process.poll())
                            self.assertLess(time.monotonic(), deadline)
                            time.sleep(0.01)
                        self.assertEqual(log.read_bytes(), b"final-tail\n")
                        self.assertIsNone(process.poll(), "relay should still be draining the full sink")
                        process.send_signal(signum)
                        self.assertEqual(process.wait(timeout=2), 128 + signum)
                        self.assertEqual(log.read_bytes(), b"final-tail\n")
                finally:
                    os.close(write_fd)
                    os.close(read_fd)

    def test_full_sink_keeps_complete_log_and_deadline_without_mutating_inherited_flags(self):
        import fcntl
        payload = b"x" * 32768 + b"final-tail\n"
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            read_fd, write_fd = os.pipe()
            try:
                capacity = fcntl.fcntl(write_fd, fcntl.F_SETPIPE_SZ, 4096)
                os.write(write_fd, b"p" * capacity)
                flags = fcntl.fcntl(write_fd, fcntl.F_GETFL)
                with self.driver(
                    directory,
                    "import sys; sys.stdout.buffer.write(b'x' * 32768 + b'final-tail\\n')",
                    stdout=write_fd, timeout=2,
                ) as (process, log, group):
                    started = time.monotonic()
                    deadline = started + 2
                    while not log.exists() or log.stat().st_size < len(payload):
                        self.assertIsNone(process.poll())
                        self.assertLess(time.monotonic(), deadline)
                        time.sleep(0.01)
                    self.assertIsNone(process.poll(), "full sink must still have an undrained tail")
                    self.assertEqual(fcntl.fcntl(write_fd, fcntl.F_GETFL), flags)
                    self.assertEqual(log.read_bytes(), payload)
                    self.assertEqual(process.wait(timeout=4), 125)
                    self.assertLess(time.monotonic() - started, 3)
                    self.assertEqual(fcntl.fcntl(write_fd, fcntl.F_GETFL), flags)
                    self.assertEqual(log.read_bytes(), payload)
            finally:
                os.close(write_fd)
                os.close(read_fd)

    def test_slow_sink_drains_final_tail_without_mutating_inherited_flags(self):
        import array
        import fcntl
        import termios
        payload = b"x" * 32768 + b"final-tail\n"
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            read_fd, write_fd = os.pipe()
            try:
                capacity = fcntl.fcntl(write_fd, fcntl.F_SETPIPE_SZ, 4096)
                flags = fcntl.fcntl(write_fd, fcntl.F_GETFL)
                with self.driver(
                    directory, "import sys; sys.stdout.buffer.write(b'x' * 32768 + b'final-tail\\n')",
                    stdout=write_fd,
                ) as (process, log, group):
                    deadline = time.monotonic() + 5
                    while True:
                        available = array.array("i", [0])
                        fcntl.ioctl(read_fd, termios.FIONREAD, available, True)
                        if available[0] == capacity and log.exists() and log.stat().st_size == len(payload):
                            break
                        self.assertIsNone(process.poll())
                        self.assertLess(time.monotonic(), deadline)
                        time.sleep(0.01)
                    self.assertEqual(fcntl.fcntl(write_fd, fcntl.F_GETFL), flags)
                    output = bytearray()
                    while len(output) < len(payload):
                        self.assertLess(time.monotonic(), deadline)
                        readable, _, _ = select.select([read_fd], [], [], 0.1)
                        if readable:
                            output.extend(os.read(read_fd, min(512, len(payload) - len(output))))
                            time.sleep(0.005)
                        self.assertEqual(fcntl.fcntl(write_fd, fcntl.F_GETFL), flags)
                    self.assertEqual(process.wait(timeout=3), 0)
                    self.assertEqual(output, payload)
                    self.assertEqual(log.read_bytes(), payload)
                    self.assertEqual(fcntl.fcntl(write_fd, fcntl.F_GETFL), flags)
            finally:
                os.close(write_fd)
                os.close(read_fd)

    def test_log_admission_rejects_unavailable_and_nonregular_paths(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for destination in (directory / "missing-directory" / "command.log", Path("/dev/full")):
                with self.subTest(destination=destination), self.driver(
                    directory, "print('must-not-start')", log=destination,
                ) as (process, log, group):
                    output, _ = process.communicate(timeout=10)
                    self.assertEqual(process.returncode, 125)
                    self.assertEqual(output, b"")
                    self.assertFalse(group.exists())

    def test_log_admission_does_not_follow_symlinks_or_wait_for_fifo_readers(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            target = directory / "target"
            target.write_bytes(b"preserved-target\n")
            link = directory / "symlink"
            link.symlink_to(target)
            fifo = directory / "fifo"
            os.mkfifo(fifo)
            for destination, existing_reader in ((link, False), (fifo, False), (fifo, True)):
                reader = os.open(fifo, os.O_RDONLY | os.O_NONBLOCK) if existing_reader else None
                try:
                    with self.subTest(destination=destination, reader=existing_reader), self.driver(
                        directory, "print('must-not-start')", log=destination, timeout=2,
                    ) as (process, log, group):
                        started = time.monotonic()
                        output, _ = process.communicate(timeout=2)
                        self.assertEqual(process.returncode, 125)
                        self.assertLess(time.monotonic() - started, 1)
                        self.assertEqual(output, b"")
                        self.assertFalse(group.exists())
                        self.assertEqual(target.read_bytes(), b"preserved-target\n")
                finally:
                    if reader is not None:
                        os.close(reader)

    def test_closed_stdout_retains_the_complete_log_and_reports_relay_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            with self.driver(directory, "print('begin'); print('x' * 65536); print('end')") as (
                process, log, group
            ):
                process.stdout.close()
                self.assertEqual(process.wait(timeout=10), 125)
                self.assertEqual(log.read_bytes(), b"begin\n" + b"x" * 65536 + b"\nend\n")

    def test_leader_exit_cleans_descendant_that_inherits_stdout_without_waiting_for_eof(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            descendant = directory / "descendant"
            for status in (0, 1):
                code = (
                    "import subprocess, sys; from pathlib import Path; "
                    "child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)']); "
                    f"Path({str(descendant)!r}).write_text(str(child.pid)); "
                    f"print('leader-done', flush=True); sys.exit({status})"
                )
                with self.subTest(status=status), self.driver(directory, code, timeout=2) as (process, log, group):
                    started = time.monotonic()
                    output, _ = process.communicate(timeout=5)
                    self.assertEqual(process.returncode, status)
                    self.assertLess(time.monotonic() - started, 1.5)
                    self.assertEqual(output, b"leader-done\n")
                    self.assertEqual(log.read_bytes(), output)
                    state = Path(f"/proc/{int(descendant.read_text())}/stat")
                    if state.exists():
                        self.assertEqual(state.read_text().split(") ", 1)[1].split()[0], "Z")

    def test_live_output_and_tree_cleanup_on_signals_and_timeout(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            tree = directory / "tree.json"
            child = (
                "import json, os, subprocess, sys, time; from pathlib import Path; "
                "descendant = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)']); "
                f"Path({str(tree)!r}).write_text(json.dumps([os.getppid(), os.getpid(), descendant.pid])); "
                "print('relay-ready', flush=True); time.sleep(60)"
            )
            command = f"import subprocess, sys; subprocess.run([sys.executable, '-u', '-c', {child!r}])"
            for signum, timeout, expected in ((signal.SIGTERM, 5, 143), (signal.SIGINT, 5, 130), (None, 2, 124)):
                with self.subTest(signum=signum), self.driver(directory, command, timeout=timeout) as (
                    process, log, group
                ):
                    readable, _, _ = select.select([process.stdout], [], [], 10)
                    self.assertTrue(readable, "command did not produce its readiness marker")
                    self.assertEqual(process.stdout.readline(), b"relay-ready\n")
                    self.assertIsNone(process.poll(), "stdout was only replayed after completion")
                    owned = [int(group.read_text()), *json.loads(tree.read_text())]
                    if signum is not None:
                        process.send_signal(signum)
                    self.assertEqual(process.wait(timeout=10), expected)
                    self.assertIn(b"relay-ready\n", log.read_bytes())
                    deadline = time.monotonic() + 3
                    while True:
                        live = []
                        for pid in owned:
                            state = Path(f"/proc/{pid}/stat")
                            try:
                                if state.read_text().split(") ", 1)[1].split()[0] != "Z":
                                    live.append(pid)
                            except FileNotFoundError:
                                pass
                        if not live or time.monotonic() >= deadline:
                            break
                        time.sleep(0.01)
                    self.assertEqual(live, [], "owned command processes survived cleanup")


if __name__ == "__main__":
    unittest.main()
