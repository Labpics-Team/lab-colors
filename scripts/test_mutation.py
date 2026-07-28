#!/usr/bin/env python3
"""Hostile-тесты границы scheduled mutation evidence."""

from __future__ import annotations

import copy
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import tempfile
import unittest


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
        self.root = Path(self.temp.name)
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
        with self.assertRaisesRegex(mutation.ContractError, "source snapshot"):
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
