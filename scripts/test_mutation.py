#!/usr/bin/env python3
"""Hostile-тесты границы scheduled mutation evidence."""

from __future__ import annotations

import copy
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from unittest import mock


SCRIPT = Path(__file__).with_name("mutation.py")
SPEC = importlib.util.spec_from_file_location("mutation", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
mutation = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(mutation)


def mutant(index: int, file: str = "crates/core/src/lib.rs") -> dict:
    line = index + 1
    return {
        "package": "core",
        "file": file,
        "function": {
            "function_name": f"function_{index}",
            "return_type": "-> bool",
            "span": {
                "start": {"line": line, "column": 1},
                "end": {"line": line, "column": 10},
            },
        },
        "span": {
            "start": {"line": line, "column": 5},
            "end": {"line": line, "column": 9},
        },
        "replacement": "false",
        "genre": "FnValue",
    }


TEST_CARGO = "/runner/toolchains/1.96.0-x86_64-unknown-linux-gnu/bin/cargo"
TEST_PACKAGE = "core@0.1.0"


def phase(summary: str) -> list[dict]:
    if summary == "CaughtMutant":
        terminal = {"Failure": 101}
    elif summary == "MissedMutant":
        terminal = "Success"
    elif summary == "Timeout":
        terminal = "Timeout"
    elif summary == "Unviable":
        return [
            {
                "phase": "Build",
                "duration": 0.25,
                "process_status": {"Failure": 101},
                "argv": [
                    TEST_CARGO,
                    "test",
                    "--no-run",
                    "--verbose",
                    f"--package={TEST_PACKAGE}",
                ],
            }
        ]
    elif summary == "Success":
        terminal = "Success"
    else:
        terminal = {"Failure": 1}
    return [
        {
            "phase": "Build",
            "duration": 0.25,
            "process_status": "Success",
            "argv": [
                TEST_CARGO,
                "test",
                "--no-run",
                "--verbose",
                f"--package={TEST_PACKAGE}",
            ],
        },
        {
            "phase": "Test",
            "duration": 0.25,
            "process_status": terminal,
            "argv": [
                TEST_CARGO,
                "test",
                "--verbose",
                f"--package={TEST_PACKAGE}",
            ],
        },
    ]


class MutationTruthTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.external_temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.external = Path(self.external_temp.name)
        self.config = self.root / ".cargo" / "mutants.toml"
        self.config.parent.mkdir(parents=True)
        self.config.write_text(
            'examine_globs = ["crates/core/src/lib.rs"]\n',
            encoding="utf-8",
        )
        self.source = self.root / "crates" / "core" / "src" / "lib.rs"
        self.source.parent.mkdir(parents=True)
        self.source.write_text("pub fn value() -> bool { true }\n", encoding="utf-8")
        (self.root / "Cargo.toml").write_text(
            '[workspace]\nmembers = ["crates/core"]\nresolver = "2"\n',
            encoding="utf-8",
        )
        (self.source.parents[1] / "Cargo.toml").write_text(
            '[package]\nname = "core"\nversion = "0.1.0"\nedition = "2021"\n',
            encoding="utf-8",
        )
        subprocess.run(["git", "init", "--quiet"], cwd=self.root, check=True)
        subprocess.run(["git", "add", "."], cwd=self.root, check=True)
        subprocess.run(
            [
                "git",
                "-c",
                "user.name=Mutation Truth Test",
                "-c",
                "user.email=mutation@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "fixture",
            ],
            cwd=self.root,
            check=True,
        )
        self.revision = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=self.root, text=True
        ).strip()

    def tearDown(self) -> None:
        self.temp.cleanup()
        self.external_temp.cleanup()

    def make_manifest(self, population: list[dict] | None = None) -> dict:
        return mutation.build_manifest(
            population if population is not None else [mutant(index) for index in range(64)],
            repo_root=self.root,
            config_path=self.config,
            package_versions={"core": "0.1.0"},
            repository="Labpics-Team/lab-colors",
            revision=self.revision,
            run_id=123,
            run_attempt=1,
        )

    def replace_with_symlink(self, path: Path) -> None:
        target = path.with_name(f"real-{path.name}")
        path.rename(target)
        path.symlink_to(target.name)

    def write_manifest(self, manifest: dict) -> Path:
        path = self.root / "mutation-manifest.json"
        mutation.write_json(path, manifest)
        return path

    def write_outcomes(
        self,
        parent: Path,
        mutants: list[dict],
        summaries: list[str] | None = None,
    ) -> None:
        summaries = summaries or ["CaughtMutant"] * len(mutants)
        out = parent / "mutants.out"
        (out / "log").mkdir(parents=True, exist_ok=True)
        (out / "diff").mkdir(exist_ok=True)
        mutation.write_json(
            out / "lock.json",
            {
                "cargo_mutants_version": mutation.TOOL_VERSION,
                "start_time": "2026-07-28T00:00:00Z",
                "hostname": "runner",
                "username": "runner",
            },
        )
        mutation.write_json(out / "mutants.json", mutants)
        outcomes = [
            {
                "scenario": "Baseline",
                "summary": "Success",
                "log_path": "log/baseline.log",
                "diff_path": None,
                "phase_results": phase("Success"),
            }
        ]
        (out / "log" / "baseline.log").write_text("baseline\n", encoding="utf-8")
        counts = {
            "CaughtMutant": 0,
            "MissedMutant": 0,
            "Timeout": 0,
            "Unviable": 0,
        }
        for index, (item, summary) in enumerate(zip(mutants, summaries)):
            log_path = f"log/mutant-{index}.log"
            diff_path = f"diff/mutant-{index}.diff"
            (out / log_path).write_text("log\n", encoding="utf-8")
            (out / diff_path).write_text("diff\n", encoding="utf-8")
            outcomes.append(
                {
                    "scenario": {"Mutant": item},
                    "summary": summary,
                    "log_path": log_path,
                    "diff_path": diff_path,
                    "phase_results": phase(summary),
                }
            )
            counts[summary] += 1
        mutation.write_json(
            out / "outcomes.json",
            {
                "outcomes": outcomes,
                "total_mutants": len(mutants),
                "missed": counts["MissedMutant"],
                "caught": counts["CaughtMutant"],
                "timeout": counts["Timeout"],
                "unviable": counts["Unviable"],
                "success": 0,
            },
        )

    def record(
        self,
        manifest: dict,
        index: int,
        summaries: list[str] | None = None,
        exit_code: int | None = None,
    ) -> tuple[Path, dict]:
        expected = mutation.expected_shard_specs(manifest, index)
        parent = self.root / f"mutation-shard-{index}"
        self.write_outcomes(parent, expected, summaries)
        if exit_code is None:
            exit_code = mutation.expected_exit_code(
                summaries or ["CaughtMutant"] * len(expected)
            )
        record = mutation.validate_and_record_shard(
            manifest,
            repo_root=self.root,
            config_path=self.config,
            output_parent=parent,
            shard_index=index,
            observed_revision=self.revision,
            tool_version_output="cargo-mutants 25.3.1",
            exit_code=exit_code,
        )
        mutation.write_json(parent / "shard.json", record)
        return parent, record

    def complete_shards(self, manifest: dict) -> Path:
        shards = self.root / "downloaded"
        shards.mkdir()
        for index in range(mutation.SHARD_COUNT):
            parent, _ = self.record(manifest, index)
            parent.rename(shards / parent.name)
        return shards

    def test_manifest_is_exact_deterministic_round_robin_population(self) -> None:
        manifest = self.make_manifest()
        self.assertEqual(manifest["population"]["count"], 64)
        self.assertEqual(manifest["sharding"], {"algorithm": "round-robin", "count": 32})
        shard_zero = mutation.expected_shard_specs(manifest, 0)
        shard_one = mutation.expected_shard_specs(manifest, 1)
        self.assertEqual(
            [item["function"]["function_name"] for item in shard_zero],
            ["function_0", "function_32"],
        )
        self.assertEqual(
            [item["function"]["function_name"] for item in shard_one],
            ["function_1", "function_33"],
        )
        self.assertNotEqual(manifest["population"]["sha256"], manifest["manifest_sha256"])

    def test_manifest_rejects_duplicate_and_foreign_mutants(self) -> None:
        duplicate = mutant(0)
        with self.assertRaisesRegex(mutation.ContractError, "duplicate mutant"):
            self.make_manifest([duplicate, copy.deepcopy(duplicate)])
        with self.assertRaisesRegex(mutation.ContractError, "unsafe mutant file"):
            self.make_manifest([mutant(0, "../../outside.rs")])
        with self.assertRaisesRegex(mutation.ContractError, "does not exist"):
            self.make_manifest([mutant(0, "crates/core/src/foreign.rs")])

    def test_shard_rejects_wrong_version_revision_config_and_exit_code(self) -> None:
        manifest = self.make_manifest()
        parent, _ = self.record(manifest, 0)
        lock = parent / "mutants.out" / "lock.json"
        data = json.loads(lock.read_text(encoding="utf-8"))
        data["cargo_mutants_version"] = "25.0.0"
        mutation.write_json(lock, data)
        with self.assertRaisesRegex(mutation.ContractError, "tool version"):
            mutation.validate_and_record_shard(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                output_parent=parent,
                shard_index=0,
                observed_revision=self.revision,
                tool_version_output="cargo-mutants 25.3.1",
                exit_code=0,
            )
        data["cargo_mutants_version"] = mutation.TOOL_VERSION
        mutation.write_json(lock, data)
        with self.assertRaisesRegex(mutation.ContractError, "revision"):
            mutation.validate_and_record_shard(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                output_parent=parent,
                shard_index=0,
                observed_revision="b" * 40,
                tool_version_output="cargo-mutants 25.3.1",
                exit_code=0,
            )
        self.config.write_text("# drift\n", encoding="utf-8")
        with self.assertRaisesRegex(mutation.ContractError, "config"):
            mutation.validate_and_record_shard(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                output_parent=parent,
                shard_index=0,
                observed_revision=self.revision,
                tool_version_output="cargo-mutants 25.3.1",
                exit_code=0,
            )
        self.config.write_text(
            'examine_globs = ["crates/core/src/lib.rs"]\n',
            encoding="utf-8",
        )
        with self.assertRaisesRegex(mutation.ContractError, "exit code"):
            mutation.validate_and_record_shard(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                output_parent=parent,
                shard_index=0,
                observed_revision=self.revision,
                tool_version_output="cargo-mutants 25.3.1",
                exit_code=2,
            )

    def test_same_head_with_dirty_tracked_source_is_not_the_manifest_snapshot(self) -> None:
        manifest = self.make_manifest()
        expected = mutation.expected_shard_specs(manifest, 0)
        parent = self.root / "mutation-shard-0"
        self.write_outcomes(parent, expected)
        self.source.write_text(
            "pub fn value() -> bool { false }\n",
            encoding="utf-8",
        )
        with self.assertRaisesRegex(mutation.ContractError, "tracked.*dirty|source snapshot"):
            mutation.validate_and_record_shard(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                output_parent=parent,
                shard_index=0,
                observed_revision=self.revision,
                tool_version_output=mutation.TOOL_VERSION_OUTPUT,
                exit_code=0,
            )

    def test_source_bytes_are_bound_even_when_git_hides_the_dirty_path(self) -> None:
        manifest = self.make_manifest()
        expected = mutation.expected_shard_specs(manifest, 0)
        parent = self.root / "mutation-shard-0"
        self.write_outcomes(parent, expected)
        subprocess.run(
            ["git", "update-index", "--assume-unchanged", "crates/core/src/lib.rs"],
            cwd=self.root,
            check=True,
        )
        self.source.write_text(
            "pub fn value() -> bool { false }\n",
            encoding="utf-8",
        )
        status = subprocess.check_output(
            ["git", "status", "--porcelain=v1", "--untracked-files=no"],
            cwd=self.root,
            text=True,
        )
        self.assertEqual(status, "")
        with self.assertRaisesRegex(mutation.ContractError, "index flag|execution source"):
            mutation.validate_and_record_shard(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                output_parent=parent,
                shard_index=0,
                observed_revision=self.revision,
                tool_version_output=mutation.TOOL_VERSION_OUTPUT,
                exit_code=0,
            )

    def test_execution_source_rejects_forbidden_index_flags_outside_population(self) -> None:
        original = (self.root / "Cargo.toml").read_bytes()
        for flag in ("--assume-unchanged", "--skip-worktree"):
            with self.subTest(flag=flag):
                subprocess.run(
                    ["git", "update-index", flag, "Cargo.toml"],
                    cwd=self.root,
                    check=True,
                )
                try:
                    (self.root / "Cargo.toml").write_text(
                        '[workspace]\nmembers = ["crates/core", "hidden-input"]\nresolver = "2"\n',
                        encoding="utf-8",
                    )
                    status = subprocess.check_output(
                        ["git", "status", "--porcelain=v1", "--untracked-files=no"],
                        cwd=self.root,
                        text=True,
                    )
                    self.assertEqual(status, "")
                    with self.assertRaisesRegex(mutation.ContractError, "index flag"):
                        self.make_manifest()
                finally:
                    (self.root / "Cargo.toml").write_bytes(original)
                    clear = (
                        "--no-assume-unchanged"
                        if flag == "--assume-unchanged"
                        else "--no-skip-worktree"
                    )
                    subprocess.run(
                        ["git", "update-index", clear, "Cargo.toml"],
                        cwd=self.root,
                        check=True,
                    )

    def test_materialized_execution_source_excludes_untracked_and_ignored_inputs(self) -> None:
        untracked = self.root / "tests" / "integration_input.rs"
        untracked.parent.mkdir()
        untracked.write_text("compile_error!(\"untracked input executed\");\n", encoding="utf-8")
        ignored = self.root / "ignored-build.rs"
        ignored.write_text("compile_error!(\"ignored input executed\");\n", encoding="utf-8")
        (self.root / ".gitignore").write_text("ignored-build.rs\n", encoding="utf-8")
        subprocess.run(["git", "add", ".gitignore"], cwd=self.root, check=True)
        subprocess.run(
            [
                "git",
                "-c",
                "user.name=Mutation Truth Test",
                "-c",
                "user.email=mutation@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "ignore hostile execution input",
            ],
            cwd=self.root,
            check=True,
        )
        self.revision = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=self.root, text=True
        ).strip()
        self.assertEqual(
            subprocess.run(
                ["git", "check-ignore", "--quiet", "ignored-build.rs"],
                cwd=self.root,
                check=False,
            ).returncode,
            0,
        )

        source_root = self.external / "execution-source"
        snapshot = mutation.materialize_execution_source(
            self.root,
            self.revision,
            source_root,
        )

        self.assertFalse((source_root / "tests" / "integration_input.rs").exists())
        self.assertFalse((source_root / "ignored-build.rs").exists())
        self.assertNotIn(
            "tests/integration_input.rs",
            {entry["path"] for entry in snapshot["entries"]},
        )
        self.assertNotIn("ignored-build.rs", {entry["path"] for entry in snapshot["entries"]})
        (source_root / "copied-extra.rs").write_text("extra\n", encoding="utf-8")
        with self.assertRaisesRegex(mutation.ContractError, "entry set mismatch"):
            mutation._verify_materialized_execution_source(
                source_root,
                snapshot,
                repo_root=self.root,
            )

    def test_execution_source_digest_binds_all_tracked_modes_and_symlink_targets(self) -> None:
        baseline = mutation._build_execution_source(self.root, self.revision)
        executable = self.root / "scripts" / "integration.sh"
        executable.parent.mkdir()
        executable.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        executable.chmod(0o755)
        link = self.root / "integration-link"
        link.symlink_to("Cargo.toml")
        subprocess.run(
            ["git", "add", "scripts/integration.sh", "integration-link"],
            cwd=self.root,
            check=True,
        )
        subprocess.run(
            [
                "git",
                "-c",
                "user.name=Mutation Truth Test",
                "-c",
                "user.email=mutation@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "tracked execution inputs",
            ],
            cwd=self.root,
            check=True,
        )
        self.revision = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=self.root, text=True
        ).strip()
        source_root = self.external / "execution-source"

        snapshot = mutation.materialize_execution_source(
            self.root,
            self.revision,
            source_root,
        )
        entries = {entry["path"]: entry for entry in snapshot["entries"]}
        committed_paths = set(
            subprocess.check_output(
                ["git", "ls-tree", "-r", "--name-only", self.revision],
                cwd=self.root,
                text=True,
            ).splitlines()
        )
        committed_tree = subprocess.check_output(
            ["git", "rev-parse", f"{self.revision}^{{tree}}"],
            cwd=self.root,
            text=True,
        ).strip()

        self.assertNotEqual(snapshot["sha256"], baseline["sha256"])
        self.assertEqual(set(entries), committed_paths)
        self.assertEqual(snapshot["git_tree"], committed_tree)
        self.assertEqual(entries["scripts/integration.sh"]["mode"], "100755")
        self.assertEqual(entries["integration-link"]["mode"], "120000")
        self.assertEqual(entries["integration-link"]["symlink_target"], "Cargo.toml")
        self.assertTrue((source_root / "scripts" / "integration.sh").is_file())
        self.assertTrue((source_root / "integration-link").is_symlink())
        self.assertEqual(os.readlink(source_root / "integration-link"), "Cargo.toml")

    def test_materialized_execution_source_rejects_external_symlink_target(self) -> None:
        link = self.root / "external-input"
        link.symlink_to("../outside-input")
        subprocess.run(["git", "add", "external-input"], cwd=self.root, check=True)
        subprocess.run(
            [
                "git",
                "-c",
                "user.name=Mutation Truth Test",
                "-c",
                "user.email=mutation@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "hostile external symlink",
            ],
            cwd=self.root,
            check=True,
        )
        self.revision = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=self.root, text=True
        ).strip()

        with self.assertRaisesRegex(mutation.ContractError, "symlink escapes or is dangling"):
            mutation.materialize_execution_source(
                self.root,
                self.revision,
                self.external / "execution-source",
            )

    def test_materialized_execution_source_rejects_file_with_directory_suffix(self) -> None:
        link = self.root / "not-a-directory"
        link.symlink_to("Cargo.toml/.")
        subprocess.run(["git", "add", "not-a-directory"], cwd=self.root, check=True)
        subprocess.run(
            [
                "git",
                "-c",
                "user.name=Mutation Truth Test",
                "-c",
                "user.email=mutation@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "hostile directory suffix",
            ],
            cwd=self.root,
            check=True,
        )
        self.revision = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=self.root, text=True
        ).strip()

        with self.assertRaisesRegex(mutation.ContractError, "symlink escapes or is dangling"):
            mutation.materialize_execution_source(
                self.root,
                self.revision,
                self.external / "execution-source",
            )

    def test_materialization_rejects_symlink_parent_resolving_inside_worktree(self) -> None:
        physical_parent = self.root / "untracked-execution-parent"
        physical_parent.mkdir()
        linked_parent = self.external / "linked-parent"
        linked_parent.symlink_to(physical_parent, target_is_directory=True)

        with self.assertRaisesRegex(mutation.ContractError, "disjoint from the Git worktree"):
            mutation.materialize_execution_source(
                self.root,
                self.revision,
                linked_parent / "execution-source",
            )

    def test_materialization_rejects_dangling_destination_symlink(self) -> None:
        destination = self.external / "execution-source"
        destination.symlink_to(self.external / "missing-target", target_is_directory=True)

        with self.assertRaisesRegex(mutation.ContractError, "symlink"):
            mutation.materialize_execution_source(
                self.root,
                self.revision,
                destination,
            )

    def test_parent_swap_after_containment_proof_cannot_redirect_writes(self) -> None:
        swappable_parent = self.external / "swappable-parent"
        swappable_parent.mkdir()
        source_root = swappable_parent / "execution-source"
        original_parent = self.external / "original-parent"
        redirected_parent = self.root / "redirected-parent"
        redirected_parent.mkdir()
        real_verify_binding = mutation._verify_source_root_binding
        calls = 0

        def verify_then_swap(repo_fd: int, candidate: Path, source_fd: int) -> None:
            nonlocal calls
            real_verify_binding(repo_fd, candidate, source_fd)
            calls += 1
            if calls == 1:
                swappable_parent.rename(original_parent)
                swappable_parent.symlink_to(redirected_parent, target_is_directory=True)

        with mock.patch.object(
            mutation,
            "_verify_source_root_binding",
            side_effect=verify_then_swap,
        ):
            with self.assertRaisesRegex(mutation.ContractError, "changed|disjoint"):
                mutation.materialize_execution_source(
                    self.root,
                    self.revision,
                    source_root,
                )
        self.assertEqual(list(redirected_parent.rglob("*")), [])

    def test_verification_reproves_physical_disjointness(self) -> None:
        source_root = self.external / "execution-source"
        snapshot = mutation.materialize_execution_source(
            self.root,
            self.revision,
            source_root,
        )
        hostile_root = self.root / "untracked-execution-source"
        source_root.rename(hostile_root)

        with self.assertRaisesRegex(mutation.ContractError, "disjoint from the Git worktree"):
            mutation._verify_materialized_execution_source(
                hostile_root,
                snapshot,
                repo_root=self.root,
            )

    def test_execution_layout_keeps_output_caches_and_tool_homes_external(self) -> None:
        source_root = self.external / "execution-source"
        source_root.mkdir()
        external = self.external / "artifacts"
        paths = {
            "output": external / "mutation-shard-0",
            "cargo_home": external / "cargo-home",
            "rustup_home": external / "rustup-home",
            "temp": external / "tmp",
            "tool_home": external / "mutants-bin",
        }
        mutation.validate_execution_layout(source_root, paths)
        for label in paths:
            with self.subTest(label=label):
                hostile = dict(paths)
                hostile[label] = source_root / label
                with self.assertRaisesRegex(mutation.ContractError, "overlap execution source"):
                    mutation.validate_execution_layout(source_root, hostile)

        containing_cache = self.external / "containing-cache"
        nested_source = containing_cache / "execution-source"
        nested_source.mkdir(parents=True)
        with self.assertRaisesRegex(mutation.ContractError, "overlap execution source"):
            mutation.validate_execution_layout(
                nested_source,
                {"cargo_home": containing_cache},
            )

    def test_shard_json_inputs_reject_symlinks_and_symlinked_output_parent(self) -> None:
        manifest = self.make_manifest()
        expected = mutation.expected_shard_specs(manifest, 0)
        for name in ("lock.json", "mutants.json", "outcomes.json"):
            with self.subTest(name=name):
                parent = self.root / f"hostile-{name}"
                self.write_outcomes(parent, expected)
                self.replace_with_symlink(parent / "mutants.out" / name)
                with self.assertRaisesRegex(mutation.ContractError, "regular.*file|symlink"):
                    mutation.validate_and_record_shard(
                        manifest,
                        repo_root=self.root,
                        config_path=self.config,
                        output_parent=parent,
                        shard_index=0,
                        observed_revision=self.revision,
                        tool_version_output=mutation.TOOL_VERSION_OUTPUT,
                        exit_code=0,
                    )

        real_parent = self.root / "real-output-parent"
        self.write_outcomes(real_parent, expected)
        linked_parent = self.root / "linked-output-parent"
        linked_parent.symlink_to(real_parent.name, target_is_directory=True)
        with self.assertRaisesRegex(mutation.ContractError, "output parent.*symlink"):
            mutation.validate_and_record_shard(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                output_parent=linked_parent,
                shard_index=0,
                observed_revision=self.revision,
                tool_version_output=mutation.TOOL_VERSION_OUTPUT,
                exit_code=0,
            )
        shard_json = real_parent / "shard.json"
        shard_json.symlink_to("real-shard.json")
        with self.assertRaisesRegex(mutation.ContractError, "shard.json output.*symlink"):
            mutation._safe_json_output(real_parent, "shard.json", "shard.json")

    def test_aggregate_rejects_symlinked_shard_json(self) -> None:
        manifest = self.make_manifest()
        shards = self.complete_shards(manifest)
        self.replace_with_symlink(shards / "mutation-shard-0" / "shard.json")

        with self.assertRaisesRegex(mutation.ContractError, "shard.json.*regular.*file|symlink"):
            mutation.aggregate(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                shards_root=shards,
                observed_revision=self.revision,
            )

    def test_phase_automaton_rejects_work_after_a_failed_check(self) -> None:
        manifest = self.make_manifest()
        expected = mutation.expected_shard_specs(manifest, 0)
        parent = self.root / "mutation-shard-0"
        self.write_outcomes(parent, expected)
        outcomes_path = parent / "mutants.out" / "outcomes.json"
        outcomes = json.loads(outcomes_path.read_text(encoding="utf-8"))
        outcomes["outcomes"][1]["phase_results"] = [
            {
                "phase": "Check",
                "duration": 0.25,
                "process_status": {"Failure": 101},
                "argv": [
                    TEST_CARGO,
                    "check",
                    "--tests",
                    "--verbose",
                    f"--package={TEST_PACKAGE}",
                ],
            },
            {
                "phase": "Test",
                "duration": 0.25,
                "process_status": "Success",
                "argv": [
                    TEST_CARGO,
                    "test",
                    "--verbose",
                    f"--package={TEST_PACKAGE}",
                ],
            },
        ]
        outcomes["outcomes"][1]["summary"] = "Unviable"
        outcomes["caught"] -= 1
        outcomes["unviable"] += 1
        mutation.write_json(outcomes_path, outcomes)
        with self.assertRaisesRegex(mutation.ContractError, "phase automaton"):
            mutation.validate_and_record_shard(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                output_parent=parent,
                shard_index=0,
                observed_revision=self.revision,
                tool_version_output=mutation.TOOL_VERSION_OUTPUT,
                exit_code=0,
            )

    def test_phase_rejects_substituted_command_identity(self) -> None:
        manifest = self.make_manifest()
        expected = mutation.expected_shard_specs(manifest, 0)
        parent = self.root / "mutation-shard-0"
        self.write_outcomes(parent, expected)
        outcomes_path = parent / "mutants.out" / "outcomes.json"
        outcomes = json.loads(outcomes_path.read_text(encoding="utf-8"))
        outcomes["outcomes"][0]["phase_results"][0]["argv"] = [
            TEST_CARGO,
            "check",
            "--workspace",
        ]
        mutation.write_json(outcomes_path, outcomes)
        with self.assertRaisesRegex(mutation.ContractError, "command identity"):
            mutation.validate_and_record_shard(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                output_parent=parent,
                shard_index=0,
                observed_revision=self.revision,
                tool_version_output=mutation.TOOL_VERSION_OUTPUT,
                exit_code=0,
            )
        self.write_outcomes(parent, expected)
        outcomes = json.loads(outcomes_path.read_text(encoding="utf-8"))
        outcomes["outcomes"][1]["phase_results"][0]["argv"][-1] = "--package=core@9.9.9"
        mutation.write_json(outcomes_path, outcomes)
        with self.assertRaisesRegex(mutation.ContractError, "command identity"):
            mutation.validate_and_record_shard(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                output_parent=parent,
                shard_index=0,
                observed_revision=self.revision,
                tool_version_output=mutation.TOOL_VERSION_OUTPUT,
                exit_code=0,
            )

    def test_shard_rejects_partial_overlap_invalid_outcome_and_path_traversal(self) -> None:
        manifest = self.make_manifest()
        expected = mutation.expected_shard_specs(manifest, 0)
        parent = self.root / "mutation-shard-0"
        self.write_outcomes(parent, expected[:-1])
        with self.assertRaisesRegex(mutation.ContractError, "slice"):
            mutation.validate_and_record_shard(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                output_parent=parent,
                shard_index=0,
                observed_revision=self.revision,
                tool_version_output="cargo-mutants 25.3.1",
                exit_code=0,
            )
        self.write_outcomes(parent, [expected[0], mutation.expected_shard_specs(manifest, 1)[0]])
        with self.assertRaisesRegex(mutation.ContractError, "slice"):
            mutation.validate_and_record_shard(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                output_parent=parent,
                shard_index=0,
                observed_revision=self.revision,
                tool_version_output="cargo-mutants 25.3.1",
                exit_code=0,
            )
        self.write_outcomes(parent, expected)
        outcomes_path = parent / "mutants.out" / "outcomes.json"
        outcomes = json.loads(outcomes_path.read_text(encoding="utf-8"))
        outcomes["outcomes"][1]["summary"] = "Success"
        mutation.write_json(outcomes_path, outcomes)
        with self.assertRaisesRegex(mutation.ContractError, "summary"):
            mutation.validate_and_record_shard(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                output_parent=parent,
                shard_index=0,
                observed_revision=self.revision,
                tool_version_output="cargo-mutants 25.3.1",
                exit_code=0,
            )
        self.write_outcomes(parent, expected)
        outcomes = json.loads(outcomes_path.read_text(encoding="utf-8"))
        outcomes["outcomes"][1]["log_path"] = "../foreign.log"
        mutation.write_json(outcomes_path, outcomes)
        with self.assertRaisesRegex(mutation.ContractError, "unsafe"):
            mutation.validate_and_record_shard(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                output_parent=parent,
                shard_index=0,
                observed_revision=self.revision,
                tool_version_output="cargo-mutants 25.3.1",
                exit_code=0,
            )

    def test_aggregate_fails_closed_on_missing_duplicate_and_population_drift(self) -> None:
        manifest = self.make_manifest()
        shards = self.complete_shards(manifest)
        aggregate = mutation.aggregate(
            manifest,
            repo_root=self.root,
            config_path=self.config,
            shards_root=shards,
            observed_revision=self.revision,
        )
        self.assertTrue(aggregate["complete"])
        missing = shards / "mutation-shard-31"
        backup = self.root / "missing"
        missing.rename(backup)
        with self.assertRaisesRegex(mutation.ContractError, "missing shard"):
            mutation.aggregate(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                shards_root=shards,
                observed_revision=self.revision,
            )
        backup.rename(missing)
        shard_json = shards / "mutation-shard-31" / "shard.json"
        record = json.loads(shard_json.read_text(encoding="utf-8"))
        record["shard"]["index"] = 30
        record["record_sha256"] = mutation._digest_value(mutation._shard_payload(record))
        mutation.write_json(shard_json, record)
        with self.assertRaisesRegex(mutation.ContractError, "duplicate shard"):
            mutation.aggregate(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                shards_root=shards,
                observed_revision=self.revision,
            )
        record["shard"]["index"] = 31
        record["population_sha256"] = "0" * 64
        record["record_sha256"] = mutation._digest_value(mutation._shard_payload(record))
        mutation.write_json(shard_json, record)
        with self.assertRaisesRegex(mutation.ContractError, "population"):
            mutation.aggregate(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                shards_root=shards,
                observed_revision=self.revision,
            )

    def test_aggregate_binds_baseline_log_and_mutant_diff_bytes(self) -> None:
        manifest = self.make_manifest()
        shards = self.complete_shards(manifest)
        first = shards / "mutation-shard-0" / "mutants.out"
        baseline_log = first / "log" / "baseline.log"
        baseline_log.write_text("tampered baseline\n", encoding="utf-8")
        with self.assertRaisesRegex(mutation.ContractError, "exact artifact bytes"):
            mutation.aggregate(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                shards_root=shards,
                observed_revision=self.revision,
            )
        baseline_log.write_text("baseline\n", encoding="utf-8")
        mutant_diff = next((first / "diff").iterdir())
        mutant_diff.write_text("tampered diff\n", encoding="utf-8")
        with self.assertRaisesRegex(mutation.ContractError, "exact artifact bytes"):
            mutation.aggregate(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                shards_root=shards,
                observed_revision=self.revision,
            )
        mutant_diff.write_text("diff\n", encoding="utf-8")
        mutant_log = next(
            path for path in (first / "log").iterdir() if path.name != "baseline.log"
        )
        mutant_log.write_text("tampered mutant log\n", encoding="utf-8")
        with self.assertRaisesRegex(mutation.ContractError, "exact artifact bytes"):
            mutation.aggregate(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                shards_root=shards,
                observed_revision=self.revision,
            )

    def test_report_only_accepts_complete_missed_and_timeout_outcomes(self) -> None:
        manifest = self.make_manifest()
        shards = self.complete_shards(manifest)
        for index, summary in [
            (0, "MissedMutant"),
            (1, "Timeout"),
            (2, "Unviable"),
        ]:
            parent = shards / f"mutation-shard-{index}"
            expected = mutation.expected_shard_specs(manifest, index)
            self.write_outcomes(
                parent,
                expected,
                [summary] + ["CaughtMutant"] * (len(expected) - 1),
            )
            record = mutation.validate_and_record_shard(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                output_parent=parent,
                shard_index=index,
                observed_revision=self.revision,
                tool_version_output="cargo-mutants 25.3.1",
                exit_code=mutation.expected_exit_code([summary]),
            )
            mutation.write_json(parent / "shard.json", record)
        aggregate = mutation.aggregate(
            manifest,
            repo_root=self.root,
            config_path=self.config,
            shards_root=shards,
            observed_revision=self.revision,
        )
        self.assertEqual(aggregate["quality_policy"], "report-only")
        self.assertEqual(aggregate["counts"]["missed"], 1)
        self.assertEqual(aggregate["counts"]["timeout"], 1)
        self.assertEqual(aggregate["counts"]["unviable"], 1)

    def test_invalid_json_and_tool_archive_fail_closed(self) -> None:
        manifest = self.make_manifest()
        path = self.write_manifest(manifest)
        path.write_text("{", encoding="utf-8")
        with self.assertRaisesRegex(mutation.ContractError, "invalid JSON"):
            mutation.read_json(path)
        archive = self.root / "cargo-mutants.tar.gz"
        archive.write_bytes(b"not the pinned release")
        with self.assertRaisesRegex(mutation.ContractError, "archive digest"):
            mutation.verify_tool_archive(archive, "cargo-mutants 25.3.1")

    def test_help_describes_report_only_contract(self) -> None:
        parser = mutation.build_parser()
        help_text = parser.format_help()
        self.assertIn("materialize-source", help_text)
        self.assertIn("manifest", help_text)
        self.assertIn("record-shard", help_text)
        self.assertIn("aggregate", help_text)
        self.assertIn("report-only", help_text)

    def test_workflow_and_scope_lock_the_truth_contract(self) -> None:
        repo = Path(__file__).resolve().parents[1]
        workflow = (repo / ".github" / "workflows" / "mutation.yml").read_text(
            encoding="utf-8"
        )
        config = (repo / ".cargo" / "mutants.toml").read_text(encoding="utf-8")
        self.assertNotIn("--in-place", workflow)
        self.assertNotIn("full_workspace", workflow)
        self.assertEqual(workflow.count("materialize-source"), 3)
        self.assertEqual(workflow.count("--gitignore=false"), 2)
        self.assertEqual(workflow.count('cd "$source_root"'), 2)
        self.assertNotIn('--config "$GITHUB_WORKSPACE/.cargo/mutants.toml"', workflow)
        self.assertIn('--config "$source_root/.cargo/mutants.toml"', workflow)
        self.assertIn('RUSTUP_HOME=$RUNNER_TEMP/', workflow)
        self.assertIn('CARGO_HOME=$RUNNER_TEMP/', workflow)
        self.assertIn('TMPDIR=$RUNNER_TEMP/', workflow)
        self.assertIn("max-parallel: 4", workflow)
        self.assertIn("--baseline=run", workflow)
        self.assertIn('--shard "$SHARD_INDEX/32"', workflow)
        self.assertEqual(
            re.findall(r"^          - ([0-9]+)$", workflow, flags=re.MULTILINE),
            [str(index) for index in range(mutation.SHARD_COUNT)],
        )
        self.assertIn(mutation.TOOL_ARCHIVE_SHA256, workflow)
        self.assertIn(mutation.TOOL_ARCHIVE_URL, workflow)
        self.assertIn(mutation.TOOL_VERSION_OUTPUT, workflow)
        self.assertIn("RUST_TOOLCHAIN: 1.96.0", workflow)
        self.assertEqual(
            mutation.CARGO_TOOLCHAIN_ID,
            "1.96.0-x86_64-unknown-linux-gnu",
        )
        self.assertEqual(
            mutation.TOOL_RELEASE_TAG_OBJECT,
            "e6113423fb6e94bd7d9e70fedca058eb8468b92c",
        )
        self.assertEqual(
            mutation.TOOL_RELEASE_COMMIT,
            "49940940bd9846a25e4c2db1c4f00e39a668ed0a",
        )
        self.assertEqual(
            mutation.TOOL_ARCHIVE_SHA256,
            "be41e6f74b633452fb17ef3b6b6113e180130f7b5693863b400c58b39e476726",
        )
        self.assertNotIn("crates/labcolors-core/src/recheck.rs", config)
        self.assertIn("crates/labcolors-core/src/point_support.rs", config)
        self.assertIn("crates/labcolors-core/src/program_identity.rs", config)


if __name__ == "__main__":
    unittest.main()
