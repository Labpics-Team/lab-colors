#!/usr/bin/env python3
"""Hostile-тесты границы scheduled mutation evidence."""

from __future__ import annotations

import copy
import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
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
    elif summary == "Failure":
        terminal = {"Signalled": 9}
    else:
        raise ValueError(f"unsupported summary: {summary!r}")
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


def workflow_job_blocks(source: str, label: str) -> dict[str, str]:
    if "\njobs:\n" not in source:
        raise AssertionError(f"{label} has no jobs mapping")
    jobs = source.split("\njobs:\n", 1)[1]
    anchors = list(re.finditer(r"(?m)^  ([A-Za-z0-9_-]+):\n", jobs))
    if not anchors:
        raise AssertionError(f"{label} has no statically declared jobs")
    return {
        anchor.group(1): jobs[
            anchor.start() : (
                anchors[index + 1].start()
                if index + 1 < len(anchors)
                else len(jobs)
            )
        ]
        for index, anchor in enumerate(anchors)
    }


def load_publish_worker() -> tuple[str, str]:
    repo = Path(__file__).resolve().parents[1]
    workflow = (
        repo / ".github" / "workflows" / "publish-worker.yml"
    ).read_text(encoding="utf-8-sig")
    publish_job = workflow_job_blocks(workflow, "publish-worker.yml")["publish"]
    return workflow, publish_job


# The publish step's `run: |` block, dedented, byte-for-byte. The step is a
# token-bearing, security-critical contract, so the validator requires the
# script to be EXACTLY this reviewed canonical form. No blacklist of shell
# write mechanisms can close the rebinding class (printf -v, read, eval,
# indirect expansion, positional indirection all evade assignment patterns);
# exact equality rejects every deviation — a rebind, a reorder, an added
# indirection, even a comment. Changing the step means reviewing and updating
# this golden, never silently mutating the script.
CANONICAL_PUBLISH_SCRIPT = (
    "set -euo pipefail\n"
    'actual_sha256="$(sha256sum --binary -- "$TARBALL_PATH")"\n'
    'actual_sha256="${actual_sha256%% *}"\n'
    'if [[ ! "$TARBALL_SHA256" =~ ^[0-9a-f]{64}$ '
    '|| "$actual_sha256" != "$TARBALL_SHA256" ]]; then\n'
    '  echo "verified tarball changed before npm publish" >&2\n'
    "  exit 1\n"
    "fi\n"
    'npm publish --ignore-scripts --@labpics:registry=https://registry.npmjs.org '
    '"$TARBALL_PATH"\n'
)


class MutationTruthTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.external_temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.external = Path(self.external_temp.name)
        self.git_binary = shutil.which("git")
        self.assertIsNotNone(self.git_binary)
        self.git_env = {
            **os.environ,
            "GIT_CONFIG_GLOBAL": os.devnull,
            "GIT_CONFIG_SYSTEM": os.devnull,
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_COUNT": "6",
            "GIT_CONFIG_KEY_0": "user.name",
            "GIT_CONFIG_VALUE_0": "Mutation Truth Test",
            "GIT_CONFIG_KEY_1": "user.email",
            "GIT_CONFIG_VALUE_1": "mutation@example.invalid",
            "GIT_CONFIG_KEY_2": "commit.gpgsign",
            "GIT_CONFIG_VALUE_2": "false",
            "GIT_CONFIG_KEY_3": "core.hooksPath",
            "GIT_CONFIG_VALUE_3": os.devnull,
            "GIT_CONFIG_KEY_4": "init.templateDir",
            "GIT_CONFIG_VALUE_4": "",
            "GIT_CONFIG_KEY_5": "init.defaultBranch",
            "GIT_CONFIG_VALUE_5": "main",
            "GIT_TERMINAL_PROMPT": "0",
        }
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
        self.git("init", "--quiet")
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", "fixture")
        self.revision = self.git_output("rev-parse", "HEAD")

    def tearDown(self) -> None:
        self.temp.cleanup()
        self.external_temp.cleanup()

    def git(
        self,
        *arguments: str,
        check: bool = True,
    ) -> subprocess.CompletedProcess:
        assert self.git_binary is not None
        result = subprocess.run(
            [self.git_binary, *arguments],
            cwd=self.root,
            check=False,
            env=self.git_env,
            capture_output=True,
        )
        if check and result.returncode != 0:
            diagnostic = result.stderr.decode("utf-8", errors="replace").strip()
            raise AssertionError(
                f"git {' '.join(arguments)} failed with {result.returncode}: {diagnostic}"
            )
        return result

    def git_output(self, *arguments: str) -> str:
        return self.git(*arguments).stdout.decode("utf-8").strip()

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

    def make_bound_manifest(self, source_root: Path) -> dict:
        mutation.materialize_execution_source(
            self.root,
            self.revision,
            source_root,
        )
        return mutation.build_manifest(
            [mutant(index) for index in range(64)],
            repo_root=self.root,
            config_path=source_root / ".cargo" / "mutants.toml",
            package_versions={"core": "0.1.0"},
            repository="Labpics-Team/lab-colors",
            revision=self.revision,
            run_id=123,
            run_attempt=1,
            source_root=source_root,
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
        summaries = (
            ["CaughtMutant"] * len(mutants) if summaries is None else summaries
        )
        if len(summaries) != len(mutants):
            raise ValueError(
                "fixture summary count must equal the mutant count: "
                f"{len(summaries)} != {len(mutants)}"
            )
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
            "Failure": 0,
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
                ["CaughtMutant"] * len(expected)
                if summaries is None
                else summaries
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

    def test_mutant_validation_accepts_canonical_equivalent_repo_roots(self) -> None:
        relative_root = Path(os.path.relpath(self.root, Path.cwd()))
        linked_root = self.external / "linked-repo"
        linked_root.symlink_to(self.root, target_is_directory=True)

        for candidate in (relative_root, linked_root):
            with self.subTest(candidate=candidate):
                self.assertEqual(
                    mutation._validate_mutant(mutant(0), "mutant", candidate),
                    mutant(0),
                )

        loop = self.external / "loop"
        loop.symlink_to("loop", target_is_directory=True)
        with self.assertRaisesRegex(mutation.ContractError, "canonicalize mutant repo root"):
            mutation._validate_mutant(mutant(0), "mutant", loop)

    def test_git_fixture_ignores_hostile_global_configuration(self) -> None:
        hostile_home = self.external / "hostile-home"
        hostile_home.mkdir()
        hostile_hooks = hostile_home / "hooks"
        hostile_hooks.mkdir()
        pre_commit = hostile_hooks / "pre-commit"
        pre_commit.write_text("#!/bin/sh\nexit 97\n", encoding="utf-8")
        pre_commit.chmod(0o755)
        (hostile_home / ".gitconfig").write_text(
            "[commit]\n"
            "\tgpgsign = true\n"
            "[core]\n"
            f"\thooksPath = {hostile_hooks}\n"
            "[hostile]\n"
            "\tmarker = visible\n",
            encoding="utf-8",
        )

        with mock.patch.dict(self.git_env, {"HOME": str(hostile_home)}):
            marker = self.git("config", "--get", "hostile.marker", check=False)
            self.assertEqual(marker.returncode, 1)
            self.git("commit", "--allow-empty", "--quiet", "-m", "isolated commit")

    def test_manifest_rejects_duplicate_and_foreign_mutants(self) -> None:
        duplicate = mutant(0)
        with self.assertRaisesRegex(mutation.ContractError, "duplicate mutant"):
            self.make_manifest([duplicate, copy.deepcopy(duplicate)])
        with self.assertRaisesRegex(mutation.ContractError, "unsafe mutant file"):
            self.make_manifest([mutant(0, "../../outside.rs")])
        with self.assertRaisesRegex(mutation.ContractError, "does not exist"):
            self.make_manifest([mutant(0, "crates/core/src/foreign.rs")])

    def test_v4_evidence_rejects_pre_failure_count_schemas(self) -> None:
        manifest = self.make_manifest()
        old_manifest = copy.deepcopy(manifest)
        old_manifest["schema"] = "lab-colors-mutation-population-v3"
        old_manifest["manifest_sha256"] = mutation._digest_value(
            mutation._manifest_payload(old_manifest)
        )
        with self.assertRaisesRegex(mutation.ContractError, "manifest schema"):
            mutation.validate_manifest(old_manifest)

        _, record = self.record(manifest, 0)
        old_record = copy.deepcopy(record)
        old_record["schema"] = "lab-colors-mutation-shard-v3"
        old_record["record_sha256"] = mutation._digest_value(
            mutation._shard_payload(old_record)
        )
        with self.assertRaisesRegex(mutation.ContractError, "shard record schema"):
            mutation._validate_record_header(old_record)

    def test_shard_rejects_wrong_version_revision_config_and_exit_code(self) -> None:
        manifest = self.make_manifest()
        parent, _ = self.record(manifest, 0)
        lock = parent / "mutants.out" / "lock.json"
        data = json.loads(lock.read_text(encoding="utf-8-sig"))
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
        self.git("update-index", "--assume-unchanged", "crates/core/src/lib.rs")
        self.source.write_text(
            "pub fn value() -> bool { false }\n",
            encoding="utf-8",
        )
        status = self.git_output("status", "--porcelain=v1", "--untracked-files=no")
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

    def test_commit_replace_ref_cannot_change_the_committed_execution_source(self) -> None:
        original_revision = self.revision
        self.source.write_text(
            "pub fn value() -> bool { false }\n",
            encoding="utf-8",
        )
        self.git("add", "crates/core/src/lib.rs")
        self.git("commit", "--quiet", "-m", "hostile alternate commit")
        alternate_revision = self.git_output("rev-parse", "HEAD")
        self.git("reset", "--soft", original_revision)
        self.git("replace", original_revision, alternate_revision)
        self.assertEqual(
            self.git_output("status", "--porcelain=v1", "--untracked-files=no"),
            "",
        )

        with self.assertRaisesRegex(mutation.ContractError, "replace ref"):
            mutation.materialize_execution_source(
                self.root,
                original_revision,
                self.external / "execution-source",
            )

    def test_blob_payload_must_match_its_reported_git_object_id(self) -> None:
        original = b"original\n"
        replacement = b"replacement\n"
        expected_oid = hashlib.sha1(
            b"blob " + str(len(original)).encode("ascii") + b"\0" + original
        ).hexdigest()
        forged_stream = (
            f"{expected_oid} blob {len(replacement)}\n".encode("ascii")
            + replacement
            + b"\n"
        )
        completed = subprocess.CompletedProcess(
            args=["git", "cat-file", "--batch"],
            returncode=0,
            stdout=forged_stream,
            stderr=b"",
        )

        with (
            mock.patch.object(mutation.subprocess, "run", return_value=completed),
            self.assertRaisesRegex(mutation.ContractError, "object ID"),
        ):
            mutation._git_blob_bytes(self.root, [expected_oid])

    def test_git_authority_environment_drops_host_overrides(self) -> None:
        hostile = {
            "GIT_DIR": str(self.external / "foreign-git-dir"),
            "GIT_WORK_TREE": str(self.external / "foreign-worktree"),
            "GIT_COMMON_DIR": str(self.external / "foreign-common-dir"),
            "GIT_INDEX_FILE": str(self.external / "foreign-index"),
            "GIT_OBJECT_DIRECTORY": str(self.external / "foreign-objects"),
            "GIT_ALTERNATE_OBJECT_DIRECTORIES": str(self.external / "foreign-alternates"),
            "GIT_REPLACE_REF_BASE": "refs/hostile/",
            "GIT_CONFIG_COUNT": "1",
            "GIT_CONFIG_KEY_0": "core.repositoryformatversion",
            "GIT_CONFIG_VALUE_0": "99",
        }
        with mock.patch.dict(os.environ, hostile):
            environment = mutation._git_authority_environment()

        for variable in hostile:
            self.assertNotIn(variable, environment)
        self.assertEqual(environment["GIT_NO_REPLACE_OBJECTS"], "1")
        self.assertEqual(environment["GIT_CONFIG_NOSYSTEM"], "1")
        self.assertEqual(environment["GIT_CONFIG_GLOBAL"], os.devnull)
        self.assertEqual(environment["GIT_CONFIG_SYSTEM"], os.devnull)

    def test_execution_source_rejects_forbidden_index_flags_outside_population(self) -> None:
        original = (self.root / "Cargo.toml").read_bytes()
        for flag in ("--assume-unchanged", "--skip-worktree"):
            with self.subTest(flag=flag):
                self.git("update-index", flag, "Cargo.toml")
                try:
                    (self.root / "Cargo.toml").write_text(
                        '[workspace]\nmembers = ["crates/core", "hidden-input"]\nresolver = "2"\n',
                        encoding="utf-8",
                    )
                    status = self.git_output(
                        "status",
                        "--porcelain=v1",
                        "--untracked-files=no",
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
                    self.git("update-index", clear, "Cargo.toml")

    def test_materialized_execution_source_excludes_untracked_and_ignored_inputs(self) -> None:
        untracked = self.root / "tests" / "integration_input.rs"
        untracked.parent.mkdir()
        untracked.write_text("compile_error!(\"untracked input executed\");\n", encoding="utf-8")
        ignored = self.root / "ignored-build.rs"
        ignored.write_text("compile_error!(\"ignored input executed\");\n", encoding="utf-8")
        (self.root / ".gitignore").write_text("ignored-build.rs\n", encoding="utf-8")
        self.git("add", ".gitignore")
        self.git("commit", "--quiet", "-m", "ignore hostile execution input")
        self.revision = self.git_output("rev-parse", "HEAD")
        self.assertEqual(
            self.git("check-ignore", "--quiet", "ignored-build.rs", check=False).returncode,
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

    def test_materialized_execution_source_closes_every_scandir_iterator(self) -> None:
        source_root = self.external / "execution-source"
        snapshot = mutation.materialize_execution_source(
            self.root,
            self.revision,
            source_root,
        )
        real_scandir = os.scandir
        opened = 0
        closed = 0

        class Probe:
            def __init__(self, descriptor: int) -> None:
                nonlocal opened
                self.iterator = real_scandir(descriptor)
                opened += 1

            def __iter__(self):
                return iter(self.iterator)

            def __enter__(self):
                return self.iterator

            def __exit__(self, exc_type, exc_value, traceback) -> None:
                nonlocal closed
                self.iterator.close()
                closed += 1

        def observe_descriptor(path):
            if isinstance(path, int):
                return Probe(path)
            return real_scandir(path)

        with mock.patch.object(
            mutation.os,
            "scandir",
            side_effect=observe_descriptor,
        ):
            mutation._verify_materialized_execution_source(
                source_root,
                snapshot,
                repo_root=self.root,
            )

        self.assertGreater(opened, 1)
        self.assertEqual(closed, opened)

    def test_materialized_execution_source_walk_is_not_python_stack_bounded(self) -> None:
        source_root = self.external / "deep-source"
        source_root.mkdir()
        leaf_parent = source_root
        depth = 160
        for _ in range(depth):
            leaf_parent /= "d"
            leaf_parent.mkdir()
        (leaf_parent / "leaf").write_bytes(b"content")

        descriptor = os.open(source_root, mutation._DIRECTORY_OPEN_FLAGS)
        original_limit = sys.getrecursionlimit()
        try:
            sys.setrecursionlimit(96)
            files, directories = mutation._walk_materialized_source_fd(descriptor)
        finally:
            sys.setrecursionlimit(original_limit)
            os.close(descriptor)

        leaf_path = "/".join(["d"] * depth + ["leaf"])
        self.assertEqual(files[leaf_path][0], "100000")
        self.assertEqual(len(directories), depth)

    def test_materialization_consumes_one_validated_git_inventory(self) -> None:
        real_run_bytes = mutation._run_bytes
        inventory_calls = 0

        def observe_inventory(argv, *, cwd, label, **kwargs):
            nonlocal inventory_calls
            if "ls-tree" in argv:
                inventory_calls += 1
            return real_run_bytes(argv, cwd=cwd, label=label, **kwargs)

        with mock.patch.object(mutation, "_run_bytes", side_effect=observe_inventory):
            mutation.materialize_execution_source(
                self.root,
                self.revision,
                self.external / "execution-source",
            )

        self.assertEqual(inventory_calls, 1)

    def test_materialization_rejects_malformed_git_inventory_as_contract_error(self) -> None:
        real_run_bytes = mutation._run_bytes

        def malformed_inventory(argv, *, cwd, label, **kwargs):
            if "ls-tree" in argv:
                return b"malformed\0"
            return real_run_bytes(argv, cwd=cwd, label=label, **kwargs)

        with mock.patch.object(mutation, "_run_bytes", side_effect=malformed_inventory):
            with self.assertRaisesRegex(mutation.ContractError, "inventory is malformed"):
                mutation.materialize_execution_source(
                    self.root,
                    self.revision,
                    self.external / "execution-source",
                )

    def test_execution_source_digest_binds_all_tracked_modes_and_symlink_targets(self) -> None:
        baseline = mutation._build_execution_source(self.root, self.revision)
        executable = self.root / "scripts" / "integration.sh"
        executable.parent.mkdir()
        executable.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        executable.chmod(0o755)
        link = self.root / "integration-link"
        link.symlink_to("Cargo.toml")
        self.git("add", "scripts/integration.sh", "integration-link")
        self.git("commit", "--quiet", "-m", "tracked execution inputs")
        self.revision = self.git_output("rev-parse", "HEAD")
        source_root = self.external / "execution-source"

        snapshot = mutation.materialize_execution_source(
            self.root,
            self.revision,
            source_root,
        )
        entries = {entry["path"]: entry for entry in snapshot["entries"]}
        committed_paths = set(
            self.git_output(
                "ls-tree",
                "-r",
                "--name-only",
                self.revision,
            ).splitlines()
        )
        committed_tree = self.git_output("rev-parse", f"{self.revision}^{{tree}}")

        self.assertNotEqual(snapshot["sha256"], baseline["sha256"])
        self.assertEqual(set(entries), committed_paths)
        self.assertEqual(snapshot["git_tree"], committed_tree)
        self.assertEqual(entries["scripts/integration.sh"]["mode"], "100755")
        self.assertEqual(entries["integration-link"]["mode"], "120000")
        self.assertEqual(entries["integration-link"]["symlink_target"], "Cargo.toml")
        self.assertTrue((source_root / "scripts" / "integration.sh").is_file())
        self.assertTrue((source_root / "integration-link").is_symlink())
        self.assertEqual(os.readlink(source_root / "integration-link"), "Cargo.toml")

        executable.chmod(0o644)
        self.git("add", "scripts/integration.sh")
        self.git("commit", "--quiet", "-m", "change tracked mode only")
        mode_revision = self.git_output("rev-parse", "HEAD")
        mode_snapshot = mutation._build_execution_source(self.root, mode_revision)
        self.assertEqual(
            {entry["path"] for entry in mode_snapshot["entries"]},
            set(entries),
        )
        self.assertNotEqual(mode_snapshot["sha256"], snapshot["sha256"])

        executable.chmod(0o755)
        link.unlink()
        link.symlink_to(".cargo/mutants.toml")
        self.git("add", "scripts/integration.sh", "integration-link")
        self.git("commit", "--quiet", "-m", "change tracked link target only")
        link_revision = self.git_output("rev-parse", "HEAD")
        link_snapshot = mutation._build_execution_source(self.root, link_revision)
        self.assertEqual(
            {entry["path"] for entry in link_snapshot["entries"]},
            set(entries),
        )
        self.assertNotEqual(link_snapshot["sha256"], snapshot["sha256"])

    def test_materialized_execution_source_rejects_external_symlink_target(self) -> None:
        outside = self.external / "outside-input"
        outside.write_text("existing external input\n", encoding="utf-8")
        link = self.root / "external-input"
        link.symlink_to(os.path.relpath(outside, link.parent))
        self.git("add", "external-input")
        self.git("commit", "--quiet", "-m", "hostile external symlink")
        self.revision = self.git_output("rev-parse", "HEAD")
        self.assertTrue(outside.is_file())
        self.assertEqual(link.resolve(strict=True), outside.resolve(strict=True))

        with self.assertRaisesRegex(mutation.ContractError, "symlink escapes or is dangling"):
            mutation.materialize_execution_source(
                self.root,
                self.revision,
                self.external / "execution-source",
            )

    def test_materialized_execution_source_rejects_file_with_directory_suffix(self) -> None:
        link = self.root / "not-a-directory"
        link.symlink_to("Cargo.toml/.")
        self.git("add", "not-a-directory")
        self.git("commit", "--quiet", "-m", "hostile directory suffix")
        self.revision = self.git_output("rev-parse", "HEAD")

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

    def test_cli_types_symlink_loop_in_repo_root(self) -> None:
        first = self.external / "loop-a"
        second = self.external / "loop-b"
        first.symlink_to(second)
        second.symlink_to(first)
        source_root = self.external / "source-root"
        source_root.mkdir()
        with self.assertRaisesRegex(mutation.ContractError, "aggregate output"):
            mutation.validate_execution_layout(
                source_root,
                {"aggregate output": first / "aggregate.json"},
            )
        stderr = io.StringIO()

        with contextlib.redirect_stderr(stderr):
            result = mutation.main(
                [
                    "materialize-source",
                    "--repo-root",
                    str(first),
                    "--revision",
                    self.revision,
                    "--source-root",
                    str(self.external / "execution-source"),
                ]
            )

        self.assertEqual(result, 2)
        self.assertIn("mutation truth error:", stderr.getvalue())
        self.assertIn("Git worktree root", stderr.getvalue())
        self.assertNotIn("Traceback", stderr.getvalue())

    def test_execution_layout_resolves_missing_outputs_but_rejects_dangling_prefix(
        self,
    ) -> None:
        source_root = self.external / "source-root"
        source_root.mkdir()
        output_parent = self.external / "output-parent"
        output_parent.mkdir()
        linked_parent = self.external / "linked-output-parent"
        linked_parent.symlink_to(output_parent, target_is_directory=True)
        mutation.validate_execution_layout(
            source_root,
            {"aggregate output": linked_parent / "missing" / "aggregate.json"},
        )

        dangling = self.external / "dangling-output"
        dangling.symlink_to(self.external / "missing-output", target_is_directory=True)

        with self.assertRaisesRegex(mutation.ContractError, "aggregate output"):
            mutation.validate_execution_layout(
                source_root,
                {"aggregate output": dangling / "aggregate.json"},
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
            calls += 1
            real_verify_binding(repo_fd, candidate, source_fd)
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
        self.assertGreaterEqual(calls, 2)
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

    def test_aggregate_types_shard_inventory_io_failure(self) -> None:
        manifest = self.make_manifest()
        shards = self.complete_shards(manifest)

        with mock.patch.object(
            Path,
            "iterdir",
            side_effect=OSError("hostile enumeration failure"),
        ):
            with self.assertRaisesRegex(
                mutation.ContractError,
                "cannot inspect shards root",
            ):
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
        outcomes = json.loads(outcomes_path.read_text(encoding="utf-8-sig"))
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

    def test_phase_fixture_rejects_unknown_summary(self) -> None:
        with self.assertRaisesRegex(ValueError, "unsupported summary"):
            phase("Caught")

    def test_unattributed_tool_statuses_are_preserved_as_failure(self) -> None:
        manifest = self.make_manifest()
        expected = mutation.expected_shard_specs(manifest, 0)
        summaries = ["Failure"] + ["CaughtMutant"] * (len(expected) - 1)
        parent = self.root / "mutation-shard-0"
        self.write_outcomes(parent, expected, summaries)

        record = mutation.validate_and_record_shard(
            manifest,
            repo_root=self.root,
            config_path=self.config,
            output_parent=parent,
            shard_index=0,
            observed_revision=self.revision,
            tool_version_output=mutation.TOOL_VERSION_OUTPUT,
            exit_code=0,
        )

        self.assertEqual(record["schema"], "lab-colors-mutation-shard-v4")
        self.assertEqual(record["counts"]["failure"], 1)
        self.assertEqual(record["exit_code"], 0)

    def test_pinned_process_automaton_matrix_is_exhaustive(self) -> None:
        execution = self.make_manifest()["execution"]
        statuses = {
            "Failure": {"Failure": 101},
            "Other": "Other",
            "Signalled": {"Signalled": 9},
            "Success": "Success",
            "Timeout": "Timeout",
        }

        def phases(build: str, test: str | None = None) -> list[dict]:
            result = phase("Success")
            result[0]["process_status"] = statuses[build]
            if test is None:
                return result[:1]
            result[1]["process_status"] = statuses[test]
            return result

        cases = [
            ("Baseline", "Failure", None, "Failure"),
            ("Baseline", "Signalled", None, "Failure"),
            ("Baseline", "Other", None, "Failure"),
            ("Baseline", "Timeout", None, "Timeout"),
            ("Baseline", "Success", "Success", "Success"),
            ("Baseline", "Success", "Failure", "Failure"),
            ("Baseline", "Success", "Signalled", "Failure"),
            ("Baseline", "Success", "Other", "Failure"),
            ("Baseline", "Success", "Timeout", "Timeout"),
            ("Mutant", "Failure", None, "Unviable"),
            ("Mutant", "Signalled", None, "Failure"),
            ("Mutant", "Other", None, "Failure"),
            ("Mutant", "Timeout", None, "Timeout"),
            ("Mutant", "Success", "Success", "MissedMutant"),
            ("Mutant", "Success", "Failure", "CaughtMutant"),
            ("Mutant", "Success", "Signalled", "Failure"),
            ("Mutant", "Success", "Other", "Failure"),
            ("Mutant", "Success", "Timeout", "Timeout"),
        ]
        for scenario, build_status, test_status, expected in cases:
            with self.subTest(
                scenario=scenario,
                build=build_status,
                test=test_status,
            ):
                self.assertEqual(
                    mutation._derive_summary(
                        scenario,
                        phases(build_status, test_status),
                        "outcome",
                        package_names=["core"],
                        execution=execution,
                    )[0],
                    expected,
                )

        for terminal in ("Failure", "Signalled", "Other", "Timeout"):
            with self.subTest(continued_after=terminal):
                with self.assertRaisesRegex(
                    mutation.ContractError,
                    "continued after a terminal Build",
                ):
                    mutation._derive_summary(
                        "Mutant",
                        phases(terminal, "Success"),
                        "outcome",
                        package_names=["core"],
                        execution=execution,
                    )

    def test_exit_priority_is_exhaustive_and_rejects_unbound_process_codes(self) -> None:
        summaries = sorted(mutation.MUTANT_SUMMARIES)
        for mask in range(1 << len(summaries)):
            subset = [
                summary
                for index, summary in enumerate(summaries)
                if mask & (1 << index)
            ]
            expected = (
                3
                if "Timeout" in subset
                else 2
                if "MissedMutant" in subset
                else 0
            )
            with self.subTest(subset=subset):
                self.assertEqual(mutation.expected_exit_code(subset), expected)

        manifest = self.make_manifest()
        expected = mutation.expected_shard_specs(manifest, 0)
        parent = self.root / "mutation-shard-0"
        self.write_outcomes(parent, expected)
        for exit_code in (1, 4, 5, 6, 70, 143):
            with self.subTest(exit_code=exit_code):
                with self.assertRaisesRegex(mutation.ContractError, "exit code mismatch"):
                    mutation.validate_and_record_shard(
                        manifest,
                        repo_root=self.root,
                        config_path=self.config,
                        output_parent=parent,
                        shard_index=0,
                        observed_revision=self.revision,
                        tool_version_output=mutation.TOOL_VERSION_OUTPUT,
                        exit_code=exit_code,
                    )

    def test_phase_rejects_substituted_command_identity(self) -> None:
        manifest = self.make_manifest()
        expected = mutation.expected_shard_specs(manifest, 0)
        parent = self.root / "mutation-shard-0"
        self.write_outcomes(parent, expected)
        outcomes_path = parent / "mutants.out" / "outcomes.json"
        outcomes = json.loads(outcomes_path.read_text(encoding="utf-8-sig"))
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
        outcomes = json.loads(outcomes_path.read_text(encoding="utf-8-sig"))
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
        outcomes = json.loads(outcomes_path.read_text(encoding="utf-8-sig"))
        outcomes["outcomes"][1]["summary"] = "Success"
        outcomes["caught"] -= 1
        outcomes["success"] += 1
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
        outcomes = json.loads(outcomes_path.read_text(encoding="utf-8-sig"))
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
        record = json.loads(shard_json.read_text(encoding="utf-8-sig"))
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

    def test_aggregate_rechecks_bound_source_before_and_after_artifacts(self) -> None:
        source_root = self.external / "execution-source"
        manifest = self.make_bound_manifest(source_root)
        shards = self.complete_shards(manifest)

        with (
            mock.patch.object(
                mutation,
                "_validate_checkout",
                wraps=mutation._validate_checkout,
            ) as validate_checkout,
            mock.patch.object(
                mutation,
                "validate_manifest",
                wraps=mutation.validate_manifest,
            ) as validate_manifest,
        ):
            mutation.aggregate(
                manifest,
                repo_root=self.root,
                config_path=source_root / ".cargo" / "mutants.toml",
                shards_root=shards,
                observed_revision=self.revision,
                source_root=source_root,
            )

        self.assertEqual(validate_checkout.call_count, 2)
        self.assertEqual(validate_manifest.call_count, 1)

    def test_aggregate_rejects_source_swap_during_artifact_regeneration(self) -> None:
        source_root = self.external / "execution-source"
        manifest = self.make_bound_manifest(source_root)
        shards = self.complete_shards(manifest)
        real_read_json = mutation.read_json
        tampered = False

        def read_then_tamper(path: Path):
            nonlocal tampered
            value = real_read_json(path)
            if path.name == "shard.json" and not tampered:
                (source_root / "crates" / "core" / "src" / "lib.rs").write_text(
                    "pub fn value() -> bool { false }\n",
                    encoding="utf-8",
                )
                tampered = True
            return value

        with mock.patch.object(mutation, "read_json", side_effect=read_then_tamper):
            with self.assertRaisesRegex(
                mutation.ContractError,
                "execution source content mismatch",
            ):
                mutation.aggregate(
                    manifest,
                    repo_root=self.root,
                    config_path=source_root / ".cargo" / "mutants.toml",
                    shards_root=shards,
                    observed_revision=self.revision,
                    source_root=source_root,
                )
        self.assertTrue(tampered)

    def test_report_only_accepts_complete_missed_and_timeout_outcomes(self) -> None:
        manifest = self.make_manifest()
        shards = self.complete_shards(manifest)
        for index, summary in [
            (0, "MissedMutant"),
            (1, "Timeout"),
            (2, "Unviable"),
            (3, "Failure"),
        ]:
            parent = shards / f"mutation-shard-{index}"
            expected = mutation.expected_shard_specs(manifest, index)
            summaries = [summary] + ["CaughtMutant"] * (len(expected) - 1)
            self.write_outcomes(
                parent,
                expected,
                summaries,
            )
            record = mutation.validate_and_record_shard(
                manifest,
                repo_root=self.root,
                config_path=self.config,
                output_parent=parent,
                shard_index=index,
                observed_revision=self.revision,
                tool_version_output="cargo-mutants 25.3.1",
                exit_code=mutation.expected_exit_code(summaries),
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
        self.assertEqual(aggregate["counts"]["failure"], 1)
        self.assertEqual(aggregate["schema"], "lab-colors-mutation-aggregate-v4")

    def test_outcome_fixture_rejects_missing_or_extra_summaries(self) -> None:
        parent = self.root / "fixture-length"
        entries = [mutant(0), mutant(1)]
        for summaries in ([], ["CaughtMutant"], ["CaughtMutant"] * 3):
            with self.subTest(summaries=summaries):
                with self.assertRaisesRegex(ValueError, "summary count"):
                    self.write_outcomes(parent, entries, summaries)

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

    def test_json_boundaries_type_runtime_failures_without_hiding_bugs(self) -> None:
        json_input = self.root / "runtime-failure.json"
        json_input.write_text("[]", encoding="utf-8")
        with (
            mock.patch.object(
                mutation.json,
                "loads",
                side_effect=RecursionError("hostile nesting"),
            ),
            self.assertRaisesRegex(mutation.ContractError, "invalid JSON"),
        ):
            mutation.read_json(json_input)
        with (
            mock.patch.dict(os.environ, {"CARGO": "/usr/bin/true"}),
            mock.patch.object(mutation, "_run_bytes", return_value=b"[]"),
            mock.patch.object(
                mutation.json,
                "loads",
                side_effect=RecursionError("hostile nesting"),
            ),
            self.assertRaisesRegex(
                mutation.ContractError,
                "cargo metadata is invalid JSON",
            ),
        ):
            mutation._cargo_package_versions(self.root, [mutant(0)])
        with self.assertRaisesRegex(mutation.ContractError, "cannot write JSON"):
            mutation.write_json(Path("/dev/null/child.json"), {})

        parser = mock.Mock()
        arguments = mock.Mock()
        arguments.handler.side_effect = lambda _: mutation.read_json(json_input)
        parser.parse_args.return_value = arguments
        stderr = io.StringIO()
        with (
            mock.patch.object(mutation, "build_parser", return_value=parser),
            mock.patch.object(
                mutation.json,
                "loads",
                side_effect=RecursionError("hostile nesting"),
            ),
            contextlib.redirect_stderr(stderr),
        ):
            self.assertEqual(mutation.main([]), 2)
        self.assertIn("mutation truth error:", stderr.getvalue())
        self.assertNotIn("Traceback", stderr.getvalue())

        arguments.handler.side_effect = KeyError("internal invariant")
        with (
            mock.patch.object(mutation, "build_parser", return_value=parser),
            self.assertRaises(KeyError),
        ):
            mutation.main([])

    def test_external_command_timeout_is_a_typed_contract_failure(self) -> None:
        expired = subprocess.TimeoutExpired(cmd=["git", "status"], timeout=1)
        with (
            mock.patch.object(mutation.subprocess, "run", side_effect=expired) as run,
            self.assertRaisesRegex(mutation.ContractError, "timed out"),
        ):
            mutation._run_bytes(
                ["git", "status"],
                cwd=self.root,
                label="git status",
            )

        self.assertEqual(
            run.call_args.kwargs["timeout"],
            mutation.EXTERNAL_COMMAND_TIMEOUT_SECONDS,
        )

    def test_execution_contract_does_not_alias_module_command_templates(self) -> None:
        first = mutation._build_execution_contract(
            [mutant(0)],
            {"core": "0.1.0"},
        )
        first["commands"]["Build"][0] = "hostile"
        second = mutation._build_execution_contract(
            [mutant(0)],
            {"core": "0.1.0"},
        )

        self.assertEqual(mutation.EXECUTION_COMMANDS["Build"][0], "test")
        self.assertEqual(second["commands"]["Build"][0], "test")

    def test_shared_runner_workflows_cancel_stale_prs_without_canceling_evidence(
        self,
    ) -> None:
        repo = Path(__file__).resolve().parents[1]
        workflows = repo / ".github" / "workflows"
        mutation_workflow = (workflows / "mutation.yml").read_text(encoding="utf-8-sig")
        ci_workflow = (workflows / "ci.yml").read_text(encoding="utf-8-sig")
        ci_worker = (workflows / "ci-worker.yml").read_text(encoding="utf-8-sig")
        native_workflow = (workflows / "native-conformance.yml").read_text(
            encoding="utf-8"
        )
        native_worker = (workflows / "native-conformance-worker.yml").read_text(
            encoding="utf-8"
        )

        self.assertIn(
            "concurrency:\n"
            "  group: mutation\n"
            "  queue: max\n",
            mutation_workflow,
        )
        mutation_concurrency = mutation_workflow.split("concurrency:\n", 1)[1].split(
            "\nenv:", 1
        )[0]
        self.assertNotIn("cancel-in-progress", mutation_concurrency)

        def enqueue_three(queue_mode: str) -> tuple[list[str], list[str]]:
            pending: list[str] = []
            cancelled: list[str] = []
            for run in ("R2", "R3"):
                if queue_mode == "single" and pending:
                    cancelled.extend(pending)
                    pending = []
                pending.append(run)
            return pending, cancelled

        self.assertEqual(enqueue_three("max"), (["R2", "R3"], []))
        self.assertEqual(enqueue_three("single"), (["R3"], ["R2"]))

        group_tail = (
            "${{ github.event_name == 'pull_request' && github.run_attempt == 1 "
            "&& format('pr-{0}', github.event.pull_request.number) || "
            "format('run-{0}', github.run_id) }}"
        )
        cancel_rule = (
            "${{ github.event_name == 'pull_request' && github.run_attempt == 1 }}"
        )
        for workflow_name, workflow in (
            ("ci", ci_workflow),
            ("native-conformance", native_workflow),
        ):
            self.assertIn(
                "concurrency:\n"
                f"  group: {workflow_name}-{group_tail}\n"
                f"  cancel-in-progress: {cancel_rule}\n",
                workflow,
            )
        # Эфемерные раннеры (GitHub-hosted или self-hosted без секретов)
        # не несят ни секретов, ни состояния, поэтому fork-PR — штатный
        # режим опенсорс-гейта: workers не держат fork-гейтов, а каждая
        # job закреплена на одной одноразовой метке.
        ci_ephemeral = {
            name: block
            for name, block in workflow_job_blocks(ci_worker, "ci-worker.yml").items()
            if "runs-on: ubuntu-latest" in block
        }
        self.assertEqual(
            set(ci_ephemeral),
            {"node-consumer-floor", "msrv", "lint", "docs", "test", "audit", "wasm"},
        )
        native_blocks = workflow_job_blocks(
            native_worker, "native-conformance-worker.yml"
        )
        # Linux-конформанс мигрирован на self-hosted; инвариант теста —
        # cancel-stale-PRs, не тип раннера.
        linux_runner = next(
            line.strip()
            for line in native_blocks["swift-conformance-linux"].splitlines()
            if line.strip().startswith("runs-on:")
        )
        self.assertIn(
            linux_runner,
            {"runs-on: ubuntu-latest", "runs-on: self-hosted"},
        )
        self.assertIn(
            "runs-on: macos-15", native_blocks["swift-conformance-macos-reference"]
        )
        for worker_name, source in (
            ("ci-worker.yml", ci_worker),
            ("native-conformance-worker.yml", native_worker),
        ):
            with self.subTest(worker=worker_name):
                self.assertNotIn("head.repo.full_name", source)
        ci_jobs = ci_worker.split("\njobs:\n", 1)[1]
        native_jobs = native_worker.split("\njobs:\n", 1)[1]
        self.assertNotIn("github.run_attempt == 1", ci_jobs)
        self.assertNotIn("github.run_attempt == 1", native_jobs)

        # Модель выбора concurrency-группы caller'ов: допуск больше не
        # ветвится по происхождению PR — эфемерные раннеры исполняют любой.
        def run_policy(
            event: str,
            attempt: int,
            pr_number: int,
            run_id: int,
        ) -> tuple[str, bool]:
            initial_pr = event == "pull_request" and attempt == 1
            group_value = f"pr-{pr_number}" if initial_pr else f"run-{run_id}"
            return group_value, initial_pr

        self.assertEqual(run_policy("pull_request", 1, 42, 100), ("pr-42", True))
        self.assertEqual(run_policy("pull_request", 1, 42, 101), ("pr-42", True))
        self.assertEqual(run_policy("pull_request", 2, 42, 42), ("run-42", False))
        self.assertEqual(run_policy("push", 2, 0, 100), ("run-100", False))
        self.assertNotEqual(
            run_policy("pull_request", 1, 42, 42)[0],
            run_policy("pull_request", 2, 42, 42)[0],
        )

    def test_reusable_workers_bound_jobs_and_binaryen_transport(self) -> None:
        repo = Path(__file__).resolve().parents[1]
        workflows = repo / ".github" / "workflows"

        workers = {
            name: (workflows / name).read_text(encoding="utf-8-sig")
            for name in (
                "ci-worker.yml",
                "mutation-worker.yml",
                "native-conformance-worker.yml",
                "publish-worker.yml",
            )
        }
        ci_caller = (workflows / "ci.yml").read_text(encoding="utf-8-sig")
        # Extract the pinned SHA dynamically from ci.yml so this test does not
        # break every time the worker reference is bumped.
        ci_worker_ref_match = re.search(
            r"uses:\s+Labpics-Team/lab-colors/\.github/workflows/ci-worker\.yml@([0-9a-f]{40})",
            ci_caller,
        )
        self.assertIsNotNone(
            ci_worker_ref_match,
            "ci.yml must pin ci-worker.yml to a full 40-char commit SHA",
        )
        assert ci_worker_ref_match is not None
        admitted_ci_worker = (
            "uses: Labpics-Team/lab-colors/.github/workflows/ci-worker.yml@"
+ ci_worker_ref_match.group(1)
        )
        self.assertEqual(ci_caller.count("ci-worker.yml@"), 1)
        self.assertIn(admitted_ci_worker, ci_caller)
        for worker_name, source in workers.items():
            for job_name, block in workflow_job_blocks(source, worker_name).items():
                with self.subTest(worker=worker_name, job=job_name):
                    timeout = re.search(
                        r"(?m)^    timeout-minutes: ([1-9][0-9]*)\n",
                        block,
                    )
                    self.assertIsNotNone(timeout)
                    assert timeout is not None
                    self.assertLess(int(timeout.group(1)), 360)

        wasm = workflow_job_blocks(workers["ci-worker.yml"], "ci-worker.yml")[
            "wasm"
        ]
        self.assertNotIn("BINARYEN_CORES", workers["ci-worker.yml"])
        self.assertIn("      BINARYEN_RELEASE: version_117\n", wasm)
        self.assertIn(
            '      BINARYEN_NODE_SHA256: "'
            '2d5a42f2d167a7cc2b4b6664c44c5ace1690d13db4f527324f052afbad461a07"\n',
            wasm,
        )
        self.assertIn("binaryen-${BINARYEN_RELEASE}-node.tar.gz", wasm)
        self.assertIn('printf \'%s  %s\\n\' "$BINARYEN_NODE_SHA256"', wasm)
        self.assertIn("sha256sum --check -", wasm)
        self.assertIn("wasm-pack build --no-opt --release", wasm)
        self.assertEqual(wasm.count("wasm-pack build "), 1)
        self.assertIn(
            'node "$BINARYEN_ROOT/wasm-opt.js" "$wasm" -o "$optimized" \\\n'
            "              -Oz --enable-bulk-memory "
            "--enable-nontrapping-float-to-int",
            wasm,
        )
        self.assertEqual(
            wasm.count(
                "-Oz --enable-bulk-memory --enable-nontrapping-float-to-int"
            ),
            1,
        )
        self.assertNotIn(
            "wasm-pack build crates/labcolors-wasm --release",
            wasm,
        )
        self.assertLess(
            wasm.index("uses: actions/setup-node@"),
            wasm.index("name: install byte-bound Binaryen Node transport"),
        )
        self.assertLess(
            wasm.index("name: install byte-bound Binaryen Node transport"),
            wasm.index("name: repeat runtime WASM build"),
        )

        native = workers["native-conformance-worker.yml"]
        native_env = native.split("\njobs:\n", 1)[0]
        swift_job = workflow_job_blocks(native, "native-conformance-worker.yml")[
            "swift-conformance-linux"
        ]
        self.assertEqual(native_env.count("SWIFT_TOOLCHAIN: 6.1.3"), 1)
        # Swift больше не capability образа раннера: он закреплён content-
        # addressed digest'ом официального OCI-образа (мутабельные теги в
        # позиции образа запрещены), TMPDIR создаётся до первого вызова
        # драйвера, а fail-closed проверка версии остаётся в скрипте.
        self.assertRegex(
            native_env,
            r"(?m)^\s+SWIFT_IMAGE_V1: swift@sha256:"
            r"fddaf02db3d41844916167ef4d199d5ca14c6003d052a0b9ab579646a9c720ec$",
        )
        self.assertIn('mkdir -p "$RUNNER_TEMP/tmp-$job"', swift_job)
        # В pinned swift-образе нет cc/gcc (только clang 17), поэтому
        # линкер cargo пробрасывается явно — иначе rustc падает
        # `linker `cc` not found` (доказано live-прогоном Stage A).
        self.assertIn(
            '--env "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=clang"',
            swift_job,
        )
        # rustup ставится workflow'ом НАПРЯМУЮ в изолированные homes job'а
        # (dtolnay/rust-toolchain ставит в дефолтный HOME раннера, который
        # контейнер не видит); бинарник закреплён sha256 официального
        # дистрибутива, установка без модификации PATH раннера.
        self.assertIn("name: install rustup into the isolated homes", swift_job)
        self.assertIn(
            "rustup/dist/x86_64-unknown-linux-gnu/rustup-init",
            swift_job,
        )
        self.assertIn(
            "4acc9acc76d5079515b46346a485974457b5a79893cfb01112423c89aeb5aa10"
            "  /tmp/rustup-init",
            swift_job,
        )
        self.assertIn("| sha256sum -c -", swift_job)
        self.assertIn("--no-modify-path", swift_job)
        self.assertIn("name: verify the pinned toolchain is live", swift_job)
        self.assertNotIn("dtolnay/rust-toolchain", swift_job)
        self.assertNotIn("sh.rustup.rs", swift_job)
        self.assertIn(
            '--mount "type=bind,src=$GITHUB_WORKSPACE,dst=/workspace,readonly"',
            swift_job,
        )
        self.assertIn(
            "bash bindings/swift/ci/run-conformance.sh",
            swift_job,
        )

        swift_runner = (
            repo / "bindings" / "swift" / "ci" / "run-conformance.sh"
        ).read_text(encoding="utf-8-sig")
        self.assertIn('expected_swift="Swift version ${SWIFT_TOOLCHAIN} ', swift_runner)
        self.assertIn('readonly source_root="${GITHUB_WORKSPACE:-/src}"', swift_runner)
        self.assertIn('readonly temp_root="${RUNNER_TEMP:-/work}"', swift_runner)
        self.assertNotIn('install -d -m 0700 "$temp_root"', swift_runner)
        self.assertIn(
            '[[ -d "$temp_root" && -w "$temp_root" && -x "$temp_root" ]]',
            swift_runner,
        )
        self.assertNotIn("apt-get", swift_runner)
        self.assertNotIn("https://sh.rustup.rs", swift_runner)

        swift_readme = (repo / "bindings" / "swift" / "README.md").read_text(
            encoding="utf-8"
        )
        self.assertIn(".github/workflows/native-conformance-worker.yml", swift_readme)
        self.assertIn(".github/workflows/native-conformance.yml", swift_readme)

    def test_publish_worker_receipt_identity_is_fail_closed(self) -> None:
        workflow, _ = load_publish_worker()
        self.assertIn(
            'const workerName = job.name.split(" / ").at(-1);',
            workflow,
        )
        self.assertIn("workerName === name", workflow)
        self.assertNotIn("callerJob:", workflow)
        self.assertNotIn("job.name === name || job.name.endsWith", workflow)
        self.assertNotIn("легаси", workflow.casefold())
        self.assertIn(
            'path: "Labpics-Team/lab-colors/.github/workflows/ci-worker.yml@'
            '1461bc2ed60142aed3a8723e618b883be6418156"',
            workflow,
        )
        self.assertIn(
            'path: "Labpics-Team/lab-colors/.github/workflows/'
            'native-conformance-worker.yml@1461bc2ed60142aed3a8723e618b883be6418156"',
            workflow,
        )
        self.assertIn("const references = run.referenced_workflows;", workflow)
        self.assertIn("references.length !== 1", workflow)
        self.assertIn("reference?.path !== spec.worker.path", workflow)
        self.assertIn("reference?.sha !== spec.worker.sha", workflow)

        def canonical_worker_name(display_name: str) -> str:
            return display_name.rsplit(" / ", 1)[-1]

        self.assertEqual(canonical_worker_name("test"), "test")
        self.assertEqual(canonical_worker_name("CI / test"), "test")
        self.assertEqual(canonical_worker_name("outer / CI / test"), "test")
        self.assertNotEqual(canonical_worker_name("CI / other"), "test")

    def test_publish_worker_secret_context_is_fail_closed(self) -> None:
        _, publish_job = load_publish_worker()
        job_if = re.search(
            r"(?m)^    if:\s*>-\n(?P<expression>(?:^      [^\n]*\n)+)",
            publish_job,
        )
        self.assertIsNotNone(job_if)
        assert job_if is not None
        expression = " ".join(
            line.strip() for line in job_if.group("expression").splitlines()
        )
        expected_expression = (
            "github.repository == 'Labpics-Team/lab-colors' && "
            "github.event_name == 'push' && "
            "github.ref_type == 'tag' && "
            "startsWith(github.ref_name, 'colors-v')"
        )
        self.assertEqual(expression, expected_expression)

        # `environment` is intentionally job-scoped. The immutable context guard
        # must be present on the same secret-bearing job, not deferred to a step.
        self.assertRegex(publish_job, r"(?m)^    environment: npm-publish\s*$")

        def eligible(context: dict[str, str]) -> bool:
            return (
                context["repository"] == "Labpics-Team/lab-colors"
                and context["event_name"] == "push"
                and context["ref_type"] == "tag"
                and context["ref_name"].startswith("colors-v")
            )

        self.assertTrue(
            eligible(
                {
                    "repository": "Labpics-Team/lab-colors",
                    "event_name": "push",
                    "ref_type": "tag",
                    "ref_name": "colors-v1.2.3",
                }
            )
        )
        for hostile in (
            {
                "repository": "attacker/lab-colors",
                "event_name": "push",
                "ref_type": "tag",
                "ref_name": "colors-v1.2.3",
            },
            {
                "repository": "Labpics-Team/lab-colors",
                "event_name": "pull_request",
                "ref_type": "branch",
                "ref_name": "colors-v1.2.3",
            },
            {
                "repository": "Labpics-Team/lab-colors",
                "event_name": "workflow_call",
                "ref_type": "tag",
                "ref_name": "colors-v1.2.3",
            },
            {
                "repository": "Labpics-Team/lab-colors",
                "event_name": "push",
                "ref_type": "branch",
                "ref_name": "colors-v1.2.3",
            },
            {
                "repository": "Labpics-Team/lab-colors",
                "event_name": "push",
                "ref_type": "tag",
                "ref_name": "release-v1.2.3",
            },
        ):
            with self.subTest(hostile=hostile):
                self.assertFalse(eligible(hostile))

    def test_publish_worker_registry_is_fail_closed(self) -> None:
        _, publish_job = load_publish_worker()
        publish_step = re.search(
            r"(?ms)^      - name: npm publish verified CI tarball .*?\n"
            r"(?P<step>.*?)(?=^      - name:|\Z)",
            publish_job,
        )
        self.assertIsNotNone(publish_step)
        assert publish_step is not None
        script = self._extract_publish_step_script(publish_step.group("step"))
        self._assert_publish_script_fail_closed(script)

        # Mutation sensitivity: every hostile drift below must trip at least
        # one of the fail-closed checks above.
        expected_publish = (
            'npm publish --ignore-scripts '
            '--@labpics:registry=https://registry.npmjs.org "$TARBALL_PATH"'
        )
        mutants = {
            "second npm publish": script.replace(
                expected_publish,
                expected_publish + '\nnpm publish "$TARBALL_PATH"',
                1,
            ),
            "missing --ignore-scripts": script.replace(
                "npm publish --ignore-scripts",
                "npm publish",
                1,
            ),
            "wrong registry": script.replace(
                "--@labpics:registry=https://registry.npmjs.org",
                "--@labpics:registry=https://registry.npmjs.com",
                1,
            ),
            "alternate registry flag": script.replace(
                "--@labpics:registry=https://registry.npmjs.org",
                "--registry=https://registry.npmjs.org",
                1,
            ),
            "unquoted tarball": script.replace(
                expected_publish,
                "npm publish --ignore-scripts "
                "--@labpics:registry=https://registry.npmjs.org $TARBALL_PATH",
                1,
            ),
            "foreign tarball": script.replace(
                expected_publish,
                "npm publish --ignore-scripts "
                "--@labpics:registry=https://registry.npmjs.org \"$FOREIGN_PATH\"",
                1,
            ),
            "foreign sha256sum path": script.replace(
                'actual_sha256="$(sha256sum --binary -- "$TARBALL_PATH")"',
                'actual_sha256="$(sha256sum --binary -- "$FOREIGN_PATH")"',
                1,
            ),
            "rebound TARBALL_PATH before recheck": script.replace(
                'actual_sha256="$(sha256sum --binary -- "$TARBALL_PATH")"',
                'TARBALL_PATH="/tmp/foreign.tgz"\n'
                'actual_sha256="$(sha256sum --binary -- "$TARBALL_PATH")"',
                1,
            ),
            "export-rebound TARBALL_PATH before recheck": script.replace(
                'actual_sha256="$(sha256sum --binary -- "$TARBALL_PATH")"',
                'export TARBALL_PATH="/tmp/foreign.tgz"\n'
                'actual_sha256="$(sha256sum --binary -- "$TARBALL_PATH")"',
                1,
            ),
            "rebound TARBALL_SHA256 before recheck": script.replace(
                'if [[ ! "$TARBALL_SHA256" =~ ^[0-9a-f]{64}$ ',
                'TARBALL_SHA256="deadbeef"\n'
                'if [[ ! "$TARBALL_SHA256" =~ ^[0-9a-f]{64}$ ',
                1,
            ),
            "printf -v TARBALL_PATH": script.replace(
                'actual_sha256="$(sha256sum --binary -- "$TARBALL_PATH")"',
                'printf -v TARBALL_PATH "/tmp/foreign.tgz"\n'
                'actual_sha256="$(sha256sum --binary -- "$TARBALL_PATH")"',
                1,
            ),
            "read -r TARBALL_PATH": script.replace(
                'actual_sha256="$(sha256sum --binary -- "$TARBALL_PATH")"',
                'read -r TARBALL_PATH <<< "/tmp/foreign.tgz"\n'
                'actual_sha256="$(sha256sum --binary -- "$TARBALL_PATH")"',
                1,
            ),
            "eval TARBALL_PATH rebind": script.replace(
                'actual_sha256="$(sha256sum --binary -- "$TARBALL_PATH")"',
                "eval 'TARBALL_PATH=/tmp/foreign.tgz'\n"
                'actual_sha256="$(sha256sum --binary -- "$TARBALL_PATH")"',
                1,
            ),
            "indirect printf -v via name": script.replace(
                'actual_sha256="$(sha256sum --binary -- "$TARBALL_PATH")"',
                'name=TARBALL_PATH\nprintf -v "$name" "/tmp/foreign.tgz"\n'
                'actual_sha256="$(sha256sum --binary -- "$TARBALL_PATH")"',
                1,
            ),
            "positional indirection": script.replace(
                'actual_sha256="$(sha256sum --binary -- "$TARBALL_PATH")"',
                'set -- TARBALL_PATH\nprintf -v "$1" "/tmp/foreign.tgz"\n'
                'actual_sha256="$(sha256sum --binary -- "$TARBALL_PATH")"',
                1,
            ),
            "missing hash recheck": self._drop_sha256_recheck(script),
            "injected NPM_REGISTRY": script.replace(
                expected_publish,
                "NPM_REGISTRY=https://registry.npmjs.com\n"
                + expected_publish.replace(
                    "--@labpics:registry=https://registry.npmjs.org",
                    "--@labpics:registry=$NPM_REGISTRY",
                ),
                1,
            ),
        }
        for name, mutant in mutants.items():
            with self.subTest(mutant=name):
                with self.assertRaises(AssertionError):
                    self._assert_publish_script_fail_closed(mutant, label=name)

    @staticmethod
    def _extract_publish_step_script(step: str) -> str:
        run_block = re.search(
            r"(?m)^        run: \|\n(?P<script>(?:^          [^\n]*\n)+)",
            step,
        )
        if run_block is None:
            raise AssertionError("publish step has no `run: |` block scalar")
        lines = [line[10:] for line in run_block.group("script").splitlines()]
        return "\n".join(lines) + "\n"

    @staticmethod
    def _drop_sha256_recheck(script: str) -> str:
        kept: list[str] = []
        skipping = False
        for line in script.splitlines():
            if "sha256sum --binary" in line:
                skipping = True
                continue
            if skipping:
                if line == "fi":
                    skipping = False
                continue
            kept.append(line)
        return "\n".join(kept) + "\n"

    def _assert_publish_script_fail_closed(
        self, script: str, label: str = "publish step"
    ) -> None:
        # The canonical golden is the complete class closer: the token-bearing
        # script must be exactly the reviewed form, so no shell write
        # mechanism (assignment, printf -v, read, eval, indirect or positional
        # indirection) can rebind the verified tarball identity.
        self.assertEqual(
            script,
            CANONICAL_PUBLISH_SCRIPT,
            f"{label}: script must be exactly the canonical reviewed publish form",
        )
        # Count npm publish COMMANDS, not prose mentioning the command: the
        # fail-closed echo message itself names the command.
        publish_lines = [
            line
            for line in script.splitlines()
            if re.match(r"^npm publish(?: |$)", line)
        ]
        self.assertEqual(
            len(publish_lines),
            1,
            f"{label}: exactly one npm publish command",
        )
        self.assertEqual(
            publish_lines,
            [
                'npm publish --ignore-scripts '
                '--@labpics:registry=https://registry.npmjs.org "$TARBALL_PATH"'
            ],
            f"{label}: exact quoted publish invocation",
        )
        self.assertNotIn("NPM_REGISTRY", script, f"{label}: no registry env injection")
        self.assertNotIn("--registry=", script, f"{label}: no alternate registry flag")
        self.assertNotIn("npm pack", script, f"{label}: no repack")
        self._assert_no_tarball_rebind(script, label)
        # The pre-publish sha256 recheck must be adjacent: the same step body,
        # immediately before the publish, so no command can run between the
        # recheck and the publish. The exact commands are bound, not
        # independent fragments: a script may hash a foreign path while a
        # spurious mention of the tarball satisfies a loose fragment search.
        publish_index = script.index(publish_lines[0])
        pre_publish = script[:publish_index]
        self.assertIn("set -euo pipefail", pre_publish, f"{label}: fail-fast shell")
        self.assertIn(
            'actual_sha256="$(sha256sum --binary -- "$TARBALL_PATH")"',
            pre_publish,
            f"{label}: exact pre-publish sha256 command binds the quoted verified tarball",
        )
        self.assertIn(
            'if [[ ! "$TARBALL_SHA256" =~ ^[0-9a-f]{64}$ '
            '|| "$actual_sha256" != "$TARBALL_SHA256" ]]; then',
            pre_publish,
            f"{label}: recheck compares the bound digest",
        )
        self.assertIn(
            "verified tarball changed before npm publish",
            pre_publish,
            f"{label}: recheck fails closed on drift",
        )
        self.assertTrue(
            pre_publish.rstrip().endswith("fi"),
            f"{label}: publish immediately follows the recheck block",
        )

    def _assert_no_tarball_rebind(self, script: str, label: str) -> None:
        # Defense-in-depth diagnostics for plain assignment forms; the
        # canonical golden above is the complete class closer (shell write
        # mechanisms beyond plain assignments — printf -v, read, eval,
        # indirection — can never be exhaustively blacklisted).
        rebind = re.search(
            r"(?m)(?:^|[;&|() \t])(?:export[ \t]+|readonly[ \t]+|local[ \t]+)?"
            r"TARBALL_(?:PATH|SHA256)[ \t]*\+?=",
            script,
        )
        self.assertIsNone(
            rebind,
            f"{label}: TARBALL_PATH/TARBALL_SHA256 must never be rebound inside "
            "the publish script (they are bound once by the workflow env block)",
        )

    def test_publish_caller_pins_admitted_worker(self) -> None:
        repo = Path(__file__).resolve().parents[1]
        caller = (repo / ".github" / "workflows" / "publish.yml").read_text(
            encoding="utf-8"
        )
        expected = (
            "uses: Labpics-Team/lab-colors/.github/workflows/publish-worker.yml@"
            "1461bc2ed60142aed3a8723e618b883be6418156"
        )
        self.assertEqual(caller.count("publish-worker.yml@"), 1)
        self.assertIn(expected, caller)

    def test_swift_conformance_does_not_mutate_temp_root(self) -> None:
        repo = Path(__file__).resolve().parents[1]
        script = repo / "bindings" / "swift" / "ci" / "run-conformance.sh"
        with tempfile.TemporaryDirectory() as fixture:
            fixture_root = Path(fixture)
            temp_root = fixture_root / "shared-temp"
            stub_bin = fixture_root / "bin"
            temp_root.mkdir(mode=0o777)
            temp_root.chmod(0o777)
            stub_bin.mkdir()
            stub_mktemp = stub_bin / "mktemp"
            stub_mktemp.write_text("#!/bin/sh\nexit 73\n", encoding="utf-8")
            stub_mktemp.chmod(0o755)
            before_mode = os.stat(temp_root).st_mode & 0o777
            env = {
                **os.environ,
                "PATH": f"{stub_bin}:{os.environ['PATH']}",
                "RUST_TOOLCHAIN": "1.96.0",
                "SWIFT_TOOLCHAIN": "6.1.3",
                "GITHUB_WORKSPACE": str(repo),
                "RUNNER_TEMP": str(temp_root),
            }
            result = subprocess.run(
                ["bash", str(script)],
                env=env,
                check=False,
                capture_output=True,
                text=True,
                timeout=mutation.EXTERNAL_COMMAND_TIMEOUT_SECONDS,
            )
            self.assertEqual(result.returncode, 73, result.stderr)
            self.assertEqual(os.stat(temp_root).st_mode & 0o777, before_mode)

    def test_workflow_and_scope_lock_the_truth_contract(self) -> None:
        def between(source: str, start: str, end: str, label: str) -> str:
            if start not in source:
                self.fail(f"{label} start anchor is missing: {start!r}")
            tail = source.split(start, 1)[1]
            if end not in tail:
                self.fail(f"{label} end anchor is missing: {end!r}")
            return tail.split(end, 1)[0]

        repo = Path(__file__).resolve().parents[1]
        workflow = (
            repo / ".github" / "workflows" / "mutation-worker.yml"
        ).read_text(encoding="utf-8-sig")
        config = (repo / ".cargo" / "mutants.toml").read_text(encoding="utf-8-sig")
        ci = (repo / ".github" / "workflows" / "ci-worker.yml").read_text(
            encoding="utf-8"
        )
        self.assertLess(
            ci.index("      - name: mutation evidence verifier hostile tests\n"),
            ci.index("      - name: cargo test\n"),
        )
        self.assertNotIn("$(git rev-parse HEAD)", workflow)
        self.assertEqual(workflow.count('--observed-revision "$GITHUB_SHA"'), 2)
        manifest_job = between(
            workflow,
            "\n  manifest:\n",
            "\n  shard:\n",
            "manifest job",
        )
        shard_job = between(
            workflow,
            "\n  shard:\n",
            "\n  aggregate:\n",
            "shard job",
        )
        shard_step = between(
            workflow,
            "      - name: run isolated shard with its own baseline\n",
            "      - name:",
            "shard execution step",
        )
        manifest_step = between(
            workflow,
            "      - name: discover and bind exact population\n",
            "      - name:",
            "manifest discovery step",
        )
        aggregate_step = between(
            workflow,
            "      - name: verify exact non-overlapping aggregate\n",
            "      - name:",
            "aggregate verification step",
        )

        self.assertIn("set -euo pipefail", aggregate_step)
        self.assertIn('diagnostic="$report_root/aggregate.log"', aggregate_step)
        self.assertIn(': > "$diagnostic"', aggregate_step)
        self.assertEqual(
            aggregate_step.count('2>&1 | tee -a "$diagnostic"'),
            2,
        )

        for step in (manifest_step, shard_step, aggregate_step):
            self.assertNotIn("--in-place", step)
            self.assertNotIn("full_workspace", step)
            self.assertEqual(step.count("materialize-source"), 1)
            self.assertIn('--config "$source_root/.cargo/mutants.toml"', step)
            self.assertNotIn('--config "$GITHUB_WORKSPACE/.cargo/mutants.toml"', step)
        self.assertEqual(manifest_step.count("--gitignore=false"), 1)
        self.assertEqual(shard_step.count("--gitignore=false"), 1)
        self.assertEqual(manifest_step.count('cd "$source_root"'), 1)
        self.assertEqual(shard_step.count('cd "$source_root"'), 1)
        for job in (manifest_job, shard_job):
            self.assertIn('RUSTUP_HOME=$RUNNER_TEMP/', job)
            self.assertIn('CARGO_HOME=$RUNNER_TEMP/', job)
            self.assertIn('TMPDIR=$RUNNER_TEMP/', job)
            self.assertEqual(
                job.count(
                    '"$RUNNER_TEMP/mutants-bin/cargo-mutants" mutants --version'
                ),
                1,
            )
            self.assertNotIn(
                '"$RUNNER_TEMP/mutants-bin/cargo-mutants" --version',
                job,
            )
        self.assertRegex(shard_job, r"(?m)^\s+max-parallel:\s*8\s*$")
        self.assertIn("--baseline=run", shard_step)
        self.assertIn('--shard "$SHARD_INDEX/32"', shard_step)
        self.assertLess(
            shard_step.index('cd "$source_root"'),
            shard_step.index("set +e"),
        )
        self.assertIn('--observed-revision "$GITHUB_SHA"', shard_step)
        self.assertNotIn("git rev-parse HEAD", shard_step)
        self.assertNotIn("--baseline=run", manifest_step)
        matrix_match = re.search(
            r"(?m)^\s+shard:\s*\n(?P<items>(?:\s+-\s+\d+\s*\n)+)",
            shard_job,
        )
        if matrix_match is None:
            self.fail("matrix.shard list is missing")
        self.assertEqual(
            re.findall(
                r"(?m)^\s*-\s+(\d+)\s*$",
                matrix_match.group("items"),
            ),
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
