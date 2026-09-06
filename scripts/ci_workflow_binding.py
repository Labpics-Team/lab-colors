"""Общий контракт привязки исходников CI для двух hostile-наборов."""

from __future__ import annotations

from collections.abc import Mapping
from typing import Final
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest


WORKFLOW_PATHS: Final = (
    ".github/workflows/ci.yml",
    ".github/workflows/ci-worker.yml",
)
LOCAL_WORKER: Final = "./.github/workflows/ci-worker.yml"
SHA_PATTERN: Final = re.compile(r"[0-9a-f]{40}")


class BindingError(AssertionError):
    """Нарушена точная привязка caller'а или snapshot workflow."""


def _parse_caller(source: str) -> None:
    """Разобрать узкий block-mapping формат caller'а; прочий YAML запрещён."""
    values: dict[tuple[str, ...], str | None] = {}
    stack: list[str] = []
    for line_number, line in enumerate(source.splitlines(), start=1):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        match = re.fullmatch(r"( *)([A-Za-z_][A-Za-z0-9_-]*):(.*)", line)
        if match is None or "\t" in line or len(match[1]) % 2:
            raise BindingError(f"ci.yml line {line_number} uses unsupported YAML syntax")
        depth = len(match[1]) // 2
        if depth > len(stack):
            raise BindingError(f"ci.yml line {line_number} skips a mapping level")
        stack[depth:] = []
        if depth and values.get(tuple(stack)) is not None:
            raise BindingError(f"ci.yml line {line_number} nests below a scalar")
        key = match[2]
        raw_value = match[3]
        if raw_value == "":
            value = None
        elif raw_value.startswith(" ") and raw_value[1:] and not raw_value[1:].startswith(("|", ">", "&", "*", "!", "'", '"')):
            value = raw_value[1:]
        else:
            raise BindingError(f"ci.yml line {line_number} uses unsupported YAML syntax")
        path = (*stack, key)
        if path in values:
            raise BindingError(f"ci.yml duplicates {'/'.join(path)}")
        values[path] = value
        stack.append(key)

    uses = [path for path in values if path[-1:] == ("uses",)]
    if uses != [("jobs", "worker", "uses")]:
        raise BindingError("ci.yml must contain exactly one active uses directive")
    jobs = {path: value for path, value in values.items() if path[:1] == ("jobs",)}
    expected = {
        ("jobs",): None,
        ("jobs", "worker"): None,
        ("jobs", "worker", "name"): "CI",
        ("jobs", "worker", "permissions"): None,
        ("jobs", "worker", "permissions", "contents"): "read",
        ("jobs", "worker", "uses"): LOCAL_WORKER,
    }
    if jobs != expected:
        raise BindingError("ci.yml must contain exactly one local CI worker call")


def verify_ci_binding(repo: Path, environment: Mapping[str, str]) -> str:
    """Сопоставить байты caller'а и worker'а с реальным snapshot workflow."""
    _parse_caller((repo / WORKFLOW_PATHS[0]).read_text(encoding="utf-8"))
    if "GITHUB_ACTIONS" in environment:
        if environment["GITHUB_ACTIONS"] != "true":
            raise BindingError("GITHUB_ACTIONS must be exactly true in GitHub context")
        revision = environment.get("CI_WORKFLOW_SHA", "")
        if SHA_PATTERN.fullmatch(revision) is None:
            raise BindingError("CI_WORKFLOW_SHA must be a lowercase full commit SHA")
    else:
        revision = "HEAD"

    git_environment = dict(environment) | {"GIT_NO_REPLACE_OBJECTS": "1"}

    def git(arguments: tuple[str, ...]) -> bytes:
        try:
            return subprocess.run(
                ["git", *arguments],
                cwd=repo,
                env=git_environment,
                check=True,
                capture_output=True,
                timeout=10,
            ).stdout
        except (OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
            raise BindingError(f"cannot read workflow snapshot {revision}") from error

    if git(("cat-file", "-t", revision)) != b"commit\n":
        raise BindingError(f"workflow snapshot {revision} is not a commit")
    observed = git(("rev-parse", revision)).decode("ascii").strip()
    for path in WORKFLOW_PATHS:
        if git(("show", f"{observed}:{path}")) != (repo / path).read_bytes():
            raise BindingError(f"{path} bytes differ from workflow snapshot {observed}")
    return observed


class TestCiWorkflowBinding(unittest.TestCase):
    """Проверить привязку в одноразовых репозиториях, включая squash-топологию."""

    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="ci-binding-")
        self.root = Path(self.temp.name)
        workflow_dir = self.root / ".github/workflows"
        workflow_dir.mkdir(parents=True)
        source = Path(__file__).resolve().parents[1]
        for path in WORKFLOW_PATHS:
            shutil.copyfile(source / path, self.root / path)
        self.git_env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith(("GIT_", "GITHUB_")) and key != "CI_WORKFLOW_SHA"
        } | {
            "GIT_CONFIG_GLOBAL": os.devnull,
            "GIT_CONFIG_SYSTEM": os.devnull,
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_AUTHOR_NAME": "CI binding test",
            "GIT_AUTHOR_EMAIL": "ci-binding@example.invalid",
            "GIT_COMMITTER_NAME": "CI binding test",
            "GIT_COMMITTER_EMAIL": "ci-binding@example.invalid",
            "GIT_MASTER": "1",
        }
        self._git(("init", "--quiet"))
        self._git(("add", ".github/workflows"))
        self._git(("commit", "--quiet", "-m", "base"))
        self.base = self._git_text(("rev-parse", "HEAD"))
        caller = self.root / WORKFLOW_PATHS[0]
        caller.write_text(
            caller.read_text(encoding="utf-8")
            + f"#    uses: Labpics-Team/lab-colors/.github/workflows/ci-worker.yml@{self.base}\n",
            encoding="utf-8",
        )
        self._git(("add", WORKFLOW_PATHS[0]))
        self._git(("commit", "--quiet", "-m", "candidate"))
        self.candidate = self._git_text(("rev-parse", "HEAD"))

    def tearDown(self) -> None:
        self.temp.cleanup()

    def _git(self, arguments: tuple[str, ...]) -> subprocess.CompletedProcess[bytes]:
        return subprocess.run(
            ["git", *arguments], cwd=self.root, env=self.git_env,
            check=True, capture_output=True, timeout=10,
        )

    def _git_text(self, arguments: tuple[str, ...]) -> str:
        return self._git(arguments).stdout.decode("utf-8").strip()

    def test_accepts_exact_local_call_at_workflow_sha(self) -> None:
        observed = verify_ci_binding(
            self.root,
            {**self.git_env, "GITHUB_ACTIONS": "true", "CI_WORKFLOW_SHA": self.candidate},
        )
        self.assertEqual(observed, self.candidate)

    def test_local_context_uses_head_only(self) -> None:
        observed = verify_ci_binding(
            self.root,
            {**self.git_env, "CI_WORKFLOW_SHA": self.base},
        )
        self.assertEqual(observed, self.candidate)

    def test_rejects_decoys_wrong_paths_refs_and_duplicate_jobs(self) -> None:
        mutations = (
            "    # uses: ./.github/workflows/ci-worker.yml",
            "    uses: ./.github/workflows/other.yml",
            "    uses: $/.github/workflows/ci-worker.yml",
            "    uses: ./.github/workflows/ci-worker.yml@main",
            f"    uses: Labpics-Team/lab-colors/.github/workflows/ci-worker.yml@{self.base}",
            "    uses: foreign/repo/.github/workflows/ci-worker.yml@main",
            "    uses: ./.github/workflows/ci-worker.yml\n"
            "    uses: ./.github/workflows/ci-worker.yml",
            "    uses: ./.github/workflows/ci-worker.yml\n"
            "  worker:\n    uses: ./.github/workflows/ci-worker.yml",
            "    uses: ./.github/workflows/ci-worker.yml\n"
            "  duplicate:\n    uses: ./.github/workflows/ci-worker.yml",
            "    name: |\n      uses: ./.github/workflows/ci-worker.yml",
            "    uses: ./.github/workflows/ci-worker.yml\njobs: {}",
        )
        original = (self.root / WORKFLOW_PATHS[0]).read_text(encoding="utf-8")
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                updated, count = re.subn(r"(?m)^    uses: \S+$", mutation, original)
                self.assertEqual(count, 1)
                with self.assertRaises(BindingError):
                    _parse_caller(updated)

    def test_rejects_missing_and_malformed_github_context(self) -> None:
        cases = {
            "missing": {**self.git_env, "GITHUB_ACTIONS": "true"},
            **{
                value: {**self.git_env, "GITHUB_ACTIONS": "true", "CI_WORKFLOW_SHA": value}
                for value in ("", "HEAD", "a" * 39, "A" * 40, "0" * 40, self.candidate + "\n")
            },
            "false-context": {
                **self.git_env,
                "GITHUB_ACTIONS": "false",
                "CI_WORKFLOW_SHA": self.candidate,
            },
        }
        for label, environment in cases.items():
            with self.subTest(label=label):
                with self.assertRaises(BindingError):
                    verify_ci_binding(self.root, environment)

    def test_rejects_caller_and_worker_byte_drift(self) -> None:
        for path in WORKFLOW_PATHS:
            original = (self.root / path).read_bytes()
            with self.subTest(path=path):
                (self.root / path).write_bytes(original + b"# drift\n")
                with self.assertRaises(AssertionError):
                    verify_ci_binding(
                        self.root,
                        {**self.git_env, "GITHUB_ACTIONS": "true", "CI_WORKFLOW_SHA": self.candidate},
                    )
                (self.root / path).write_bytes(original)

    def test_checkout_sha_may_differ_from_workflow_sha(self) -> None:
        tree = self._git_text(("rev-parse", f"{self.candidate}^{{tree}}"))
        merge = self._git_text(("commit-tree", tree, "-p", self.base, "-p", self.candidate, "-m", "merge"))
        observed = verify_ci_binding(
            self.root,
            {**self.git_env, "GITHUB_ACTIONS": "true", "CI_WORKFLOW_SHA": merge, "GITHUB_SHA": self.candidate},
        )
        self.assertEqual(observed, merge)

    def test_workflow_sha_cannot_fall_back_to_matching_checkout(self) -> None:
        with self.assertRaises(BindingError):
            verify_ci_binding(
                self.root,
                {**self.git_env, "GITHUB_ACTIONS": "true", "CI_WORKFLOW_SHA": self.base, "GITHUB_SHA": self.candidate},
            )

    def test_reference_free_one_parent_squash_snapshot_passes(self) -> None:
        tree = self._git_text(("rev-parse", f"{self.candidate}^{{tree}}"))
        squash = self._git_text(("commit-tree", tree, "-p", self.base, "-m", "squash"))
        self._git(("branch", "squashed", squash))
        clone = self.root.with_name(f"{self.root.name}-clone")
        try:
            subprocess.run(
                [
                    "git", "clone", "--quiet", "--no-local", "--single-branch",
                    "--no-tags", "--branch", "squashed", str(self.root), str(clone),
                ],
                env=self.git_env,
                check=True,
                capture_output=True,
            )
            producer_lookup = subprocess.run(
                ["git", "cat-file", "-e", f"{self.candidate}^{{commit}}"],
                cwd=clone,
                env=self.git_env,
                check=False,
                capture_output=True,
            )
            self.assertNotEqual(producer_lookup.returncode, 0)
            observed = verify_ci_binding(clone, self.git_env)
            self.assertEqual(observed, squash)
        finally:
            if clone.exists():
                shutil.rmtree(clone)
