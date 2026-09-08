#!/usr/bin/env python3
"""Anti-vacuum для исполняемой матрицы артефактов.

Каждый тест — мутант, который гейт обязан поймать. Гейт, не способный упасть
на этих мутантах, доказывал бы лишь согласие инструмента с самим собой.

Мутанты `tree` работают в копии дерева (git init + добавление), чтобы
`hash-object --stdin-paths` и `ls-files` видели тот же набор атрибутов.
Мутанты `tests` не запускают cargo: они подменяют инструментальные функции
и проверяют, что гейт различает состояния инвентаря.
"""
from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT / "scripts"))
import artifact_matrix as matrix  # noqa: E402

CI_WORKER = REPO_ROOT / ".github" / "workflows" / "ci-worker.yml"


def git(cwd: Path, *args: str) -> str:
    return subprocess.run(["git", *args], cwd=cwd, capture_output=True, check=True).stdout.decode()


def make_repo_copy(destination: Path) -> None:
    """Копия объявленных корней как отдельный Git-репозиторий с теми же атрибутами."""
    destination.mkdir()
    for name in (".gitattributes", *matrix.TREE_ROOTS):
        source = REPO_ROOT / name
        if source.is_dir():
            shutil.copytree(source, destination / name, symlinks=True,
                            ignore=shutil.ignore_patterns(*matrix.TREE_EXCLUDE_DIRS))
        elif source.is_file():
            (destination / name).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination / name)
    git(destination, "init", "-q")
    git(destination, "config", "user.email", "t@example.invalid")
    git(destination, "config", "user.name", "t")
    git(destination, "add", "-A", ".")
    git(destination, "commit", "-qm", "base")
    # Запись `tree` закрепляется внутри копии: тесты проверяют закон гейта, а не
    # совпадение копии с pinned записью репозитория (которая закрепляется в Linux CI).
    env = {k: v for k, v in os.environ.items() if k != "GITHUB_ACTIONS"}
    env.update(PYTHONDONTWRITEBYTECODE="1", ARTIFACT_MATRIX_ONLY="tree")
    subprocess.run([sys.executable, str(destination / "scripts" / "artifact_matrix.py"), "refresh"],
                   capture_output=True, cwd=destination, env=env, check=True)
    git(destination, "add", "-A", "proof/artifact")
    # Копия, совпадающая с pinned записью репозитория, даёт пустой коммит — это норма.
    subprocess.run(["git", "commit", "-qm", "pin tree", "--allow-empty"], cwd=destination, capture_output=True, check=True)


def run_check(root: Path, only: str) -> subprocess.CompletedProcess:
    env = {k: v for k, v in os.environ.items() if k != "GITHUB_ACTIONS"}
    env.update(PYTHONDONTWRITEBYTECODE="1", ARTIFACT_MATRIX_ONLY=only)
    return subprocess.run([sys.executable, str(root / "scripts" / "artifact_matrix.py"), "check"],
                          capture_output=True, cwd=root, env=env)


class TreeRecordTests(unittest.TestCase):
    """Omission / decoy / parser на уровне байтов дерева."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.tmp = tempfile.TemporaryDirectory()
        cls.base = Path(cls.tmp.name) / "base"
        make_repo_copy(cls.base)

    @classmethod
    def tearDownClass(cls) -> None:
        # Объекты .git на Windows read-only; снимаем флаг перед удалением.
        for path in Path(cls.tmp.name).rglob("*"):
            try:
                path.chmod(0o700)
            except OSError:
                pass
        cls.tmp.cleanup()

    _counter = 0

    def fresh(self) -> Path:
        type(self)._counter += 1
        root = Path(self.tmp.name) / f"case-{self._counter}"
        shutil.copytree(self.base, root, symlinks=True)
        return root

    def test_pinned_tree_reproduces_on_current_tree(self) -> None:
        result = run_check(self.fresh(), "tree")
        self.assertEqual(result.returncode, 0, result.stderr.decode(errors="replace"))

    def test_omission_of_the_pinned_record_is_red(self) -> None:
        root = self.fresh()
        (root / "proof" / "artifact" / "tree.json").unlink()
        result = run_check(root, "tree")
        self.assertEqual(result.returncode, 1, result.stderr.decode(errors="replace"))
        self.assertIn(b"MISSING pinned record proof/artifact/tree.json", result.stderr)

    def test_untracked_decoy_source_file_is_drift(self) -> None:
        root = self.fresh()
        (root / "crates" / "labcolors-core" / "src" / "zz_decoy.rs").write_text("// decoy\n")
        result = run_check(root, "tree")
        self.assertEqual(result.returncode, 1, result.stderr.decode(errors="replace"))
        self.assertIn(b"+ files.crates/labcolors-core/src/zz_decoy.rs", result.stderr)

    def test_deleted_workflow_is_drift(self) -> None:
        root = self.fresh()
        (root / ".github" / "workflows" / "ci.yml").unlink()
        result = run_check(root, "tree")
        self.assertEqual(result.returncode, 1, result.stderr.decode(errors="replace"))
        self.assertIn(b"- files..github/workflows/ci.yml", result.stderr)

    def test_one_byte_edit_is_drift_and_names_the_file(self) -> None:
        root = self.fresh()
        target = root / "scripts" / "extract_source_files.py"
        target.write_bytes(target.read_bytes() + b"\n# mutant\n")
        result = run_check(root, "tree")
        self.assertEqual(result.returncode, 1, result.stderr.decode(errors="replace"))
        self.assertIn(b"~ files.scripts/extract_source_files.py", result.stderr)

    def test_crlf_checkout_of_lf_pinned_file_is_not_drift(self) -> None:
        # Blob id считается с атрибутами: CRLF на диске у text eol=lf файла не меняет запись.
        root = self.fresh()
        target = root / "scripts" / "artifact_matrix.py"
        target.write_bytes(target.read_bytes().replace(b"\n", b"\r\n"))
        result = run_check(root, "tree")
        self.assertEqual(result.returncode, 0, result.stderr.decode(errors="replace"))

    def test_stale_record_with_valid_self_hash_is_drift_not_malformed(self) -> None:
        root = self.fresh()
        path = root / "proof" / "artifact" / "tree.json"
        record = json.loads(path.read_bytes())
        victim = sorted(record["files"])[0]
        record["files"][victim] = "0" * 40
        path.write_bytes(json.dumps(matrix.seal(record), sort_keys=True).encode())
        result = run_check(root, "tree")
        self.assertEqual(result.returncode, 1, result.stderr.decode(errors="replace"))
        self.assertIn(f"~ files.{victim}".encode(), result.stderr)

    def test_parser_corruption_is_usage_error_not_green(self) -> None:
        good = json.loads((self.base / "proof" / "artifact" / "tree.json").read_bytes())
        cases = {
            "truncated": b'{"class": "tree", "schema_ver',
            "array": b"[]",
            "wrong-class": json.dumps(matrix.seal(dict(good, **{"class": "tests"}))).encode(),
            "bad-self-hash": json.dumps(dict(good, record_sha256="0" * 64)).encode(),
            "bool-schema": json.dumps(matrix.seal(dict(good, schema_version=True))).encode(),
        }
        for name, payload in cases.items():
            with self.subTest(case=name):
                root = self.fresh()
                (root / "proof" / "artifact" / "tree.json").write_bytes(payload)
                result = run_check(root, "tree")
                self.assertEqual(result.returncode, 64, result.stderr.decode(errors="replace"))
                self.assertIn(b"malformed", result.stderr)

    def test_record_outside_registry_is_red(self) -> None:
        root = self.fresh()
        shutil.copy(root / "proof" / "artifact" / "tree.json", root / "proof" / "artifact" / "bonus.json")
        result = run_check(root, "tree")
        self.assertEqual(result.returncode, 1, result.stderr.decode(errors="replace"))
        self.assertIn(b"UNLISTED record not in registry: proof/artifact/bonus.json", result.stderr)


class TestsRecordTests(unittest.TestCase):
    """Disabled-test мутанты: инвентарь тестов различает включённые и отключённые."""

    def pinned(self) -> dict:
        return json.loads((REPO_ROOT / "proof" / "artifact" / "tests.json").read_bytes())

    def test_pinned_tests_record_is_well_formed_and_nonvacuous(self) -> None:
        record = matrix.parse_record((REPO_ROOT / "proof" / "artifact" / "tests.json").read_bytes(), "tests")
        self.assertGreater(record["rust"]["enabled_count"], 1000)
        self.assertEqual(len(record["rust"]["enabled"]), record["rust"]["enabled_count"])
        self.assertEqual(len(record["rust"]["ignored"]), record["rust"]["ignored_count"])
        self.assertTrue(set(record["rust"]["ignored"]).isdisjoint(record["rust"]["enabled"]))
        self.assertEqual([s["start"] for s in record["python"]], [s for s, _ in matrix.PYTHON_SUITES])
        for suite in record["python"]:
            self.assertEqual(suite["load_errors"], [], suite["start"])
            self.assertEqual(suite["count"], len(suite["names"]))
        self.assertGreaterEqual(len(record["node_skip_sites"]), 1)

    def _with_mutated_extraction(self, mutate) -> subprocess.CompletedProcess | tuple[int, str]:
        record = self.pinned()
        mutated = json.loads(json.dumps(record))
        mutate(mutated)
        with mock.patch.object(matrix, "extract_tests", lambda: matrix.seal(mutated)), \
             mock.patch.object(matrix, "extract_tree", lambda: matrix.parse_record(
                 (REPO_ROOT / "proof" / "artifact" / "tree.json").read_bytes(), "tree")):
            import io
            import contextlib
            err = io.StringIO()
            with contextlib.redirect_stderr(err):
                code = matrix.check()
        return code, err.getvalue()

    def test_newly_ignored_rust_test_is_named_in_diff(self) -> None:
        def mutate(record: dict) -> None:
            victim = record["rust"]["enabled"].pop(0)
            record["rust"]["enabled_count"] -= 1
            record["rust"]["ignored"].append(victim)
            record["rust"]["ignored"].sort()
            record["rust"]["ignored_count"] += 1
            self.victim = victim
        code, err = self._with_mutated_extraction(mutate)
        self.assertEqual(code, 1, err)
        self.assertIn(f"- rust.enabled: {json.dumps(self.victim)}", err)
        self.assertIn(f"+ rust.ignored: {json.dumps(self.victim)}", err)

    def test_silently_dropped_rust_test_is_drift(self) -> None:
        def mutate(record: dict) -> None:
            record["rust"]["enabled"].pop()
            record["rust"]["enabled_count"] -= 1
        code, err = self._with_mutated_extraction(mutate)
        self.assertEqual(code, 1, err)
        self.assertIn("~ rust.enabled_count", err)

    def test_python_module_that_fails_to_import_is_drift(self) -> None:
        def mutate(record: dict) -> None:
            suite = record["python"][0]
            suite["load_errors"] = ["test_build"]
            suite["names"] = [n for n in suite["names"] if not n.startswith("test_build.")]
            suite["count"] = len(suite["names"])
        code, err = self._with_mutated_extraction(mutate)
        self.assertEqual(code, 1, err)
        self.assertIn("load_errors", err)

    def test_new_node_skip_site_is_drift(self) -> None:
        def mutate(record: dict) -> None:
            record["node_skip_sites"].append({"path": "packages/colors/test/zz.test.mjs", "line": 1, "text": "t.skip('x')"})
        code, err = self._with_mutated_extraction(mutate)
        self.assertEqual(code, 1, err)
        self.assertIn("+ node_skip_sites: ", err)

    def test_unchanged_inventory_is_green(self) -> None:
        code, err = self._with_mutated_extraction(lambda record: None)
        self.assertEqual(code, 0, err)


class ToolingTests(unittest.TestCase):
    def test_unknown_mode_exits_64(self) -> None:
        result = subprocess.run([sys.executable, str(REPO_ROOT / "scripts" / "artifact_matrix.py"), "bogus"],
                                capture_output=True, cwd=REPO_ROOT)
        self.assertEqual(result.returncode, 64)

    def test_narrowing_knob_is_refused_in_ci(self) -> None:
        with mock.patch.dict(os.environ, {"GITHUB_ACTIONS": "true", "ARTIFACT_MATRIX_ONLY": "tree"}):
            with self.assertRaises(matrix.MalformedRecord):
                matrix.selected()

    def test_required_ci_executes_gate_and_sabotage_suites(self) -> None:
        # Инвентарь без исполнения — ровно тот дефект, который закрывает ARTIFACT-01.
        text = CI_WORKER.read_text("utf-8")
        steps = [line for line in text.splitlines() if line.strip().startswith("run:")]
        joined = "\n".join(steps)
        self.assertIn("python3 scripts/artifact_matrix.py check", joined)
        self.assertIn("python3 -m unittest discover -s scripts -p 'test_extract_*.py'", joined)
        self.assertIn("python3 scripts/test_artifact_matrix.py", joined)
        self.assertNotRegex(joined, r"extract_\w+\.py extract \| python3 scripts/extract_\w+\.py verify")


if __name__ == "__main__":
    unittest.main()
