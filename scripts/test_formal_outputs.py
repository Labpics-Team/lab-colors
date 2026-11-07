"""Потери, подмена и пределы передачи формальных JSON-отчётов."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import random
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location("formal_outputs", Path(__file__).parent / "ci/formal_outputs.py")
assert SPEC is not None and SPEC.loader is not None
OUTPUTS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(OUTPUTS)
SCRIPT = Path(__file__).parent / "ci/formal_outputs.py"


def report_part(root, kind="positive", index=0, count=1, reports=None):
    root.mkdir(parents=True, exist_ok=True)
    if reports is None:
        name = f"positive-{index}.json" if kind == "positive" else f"negative-example-{index}.json"
        reports = {name: b'{"checked":true}\n'}
    fragment = {
        "fragment_schema": 1, "kani_version": "fixture", "kind": kind,
        "shard_index": index, "shard_count": count,
        "checkout_commit": "a" * 40, "source_commit": "a" * 40,
        "working_tree_clean": True, "verified_source_sha256": {"core.rs": "b" * 64},
    }
    if kind == "positive":
        name, data = next(iter(reports.items()))
        fragment.update({"file": name, "sha256": hashlib.sha256(data).hexdigest()})
    else:
        fragment["negative_sha256"] = {
            name.removeprefix("negative-").removesuffix(".json"): hashlib.sha256(data).hexdigest()
            for name, data in reports.items()
        }
    files = {f"fragment-{kind}-{index}.json": json.dumps(fragment, indent=2).encode() + b"\n", **reports}
    for name, data in files.items():
        (root / name).write_bytes(data)
    return files


def artifact_part(root, kind="positive", index=0, count=1, attempt=1, reports=None):
    directory = root / f"core-formal-{kind}-{index}-{attempt}"
    return directory, report_part(directory, kind, index, count, reports)


def run_adapter(*arguments, environment=None):
    clean = {key: value for key, value in os.environ.items()
             if not key.startswith("CI_FORMAL_PART_") and key != "CI_FORMAL_ARTIFACT_ID"}
    return subprocess.run([sys.executable, str(SCRIPT), *map(str, arguments)],
                          capture_output=True, text=True, env={**clean, **(environment or {})})


class FormalOutputTests(unittest.TestCase):
    def test_large_report_publishes_artifact_id_instead_of_raw_data(self):
        rng = random.Random(42356199601)
        report = json.dumps({"trace": rng.randbytes(200_000).hex()}, indent=2).encode() + b"\n"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "producer"
            original = report_part(source, "mutants", 2, 5, {"negative-example-2.json": report})
            artifacts = root / "artifacts"
            staged = artifacts / "core-formal-mutants-2-2"
            prepared = run_adapter("prepare", source, "--output-dir", staged)
            self.assertEqual(prepared.returncode, 0, prepared.stderr)
            output = root / "job-output"
            published = run_adapter("publish", staged, environment={"GITHUB_OUTPUT": str(output),
                                    "CI_FORMAL_ARTIFACT_ID": "13469449244"})
            self.assertEqual(published.returncode, 0, published.stderr)
            self.assertEqual(output.read_text(), "mutants_2=13469449244\n")
            all_files = dict(original)
            for kind in ("positive", "mutants"):
                for index in range(5):
                    if (kind, index) != ("mutants", 2):
                        _, files = artifact_part(artifacts, kind, index, 5)
                        all_files.update(files)
            ids = root / "ids-output"
            environment = {"GITHUB_OUTPUT": str(ids),
                           **{f"CI_FORMAL_PART_{index}": str(13469449244 + index) for index in range(10)}}
            selected = run_adapter("ids", "--parts", 10, environment=environment)
            self.assertEqual(selected.returncode, 0, selected.stderr)
            self.assertEqual(ids.read_text(), "artifact-ids=" + ",".join(
                str(13469449244 + index) for index in range(10)) + "\n")
            destination = root / "restored"
            restored = run_adapter("restore", destination, "--input-dir", artifacts, "--parts", 10)
            self.assertEqual(restored.returncode, 0, restored.stderr)
            self.assertEqual({name: (destination / name).read_bytes() for name in all_files}, all_files)
            self.assertEqual(hashlib.sha256((destination / "negative-example-2.json").read_bytes()).hexdigest(),
                             json.loads(original["fragment-mutants-2.json"])["negative_sha256"]["example-2"])
            shutil.rmtree(artifacts / "core-formal-positive-4-1")
            incomplete = root / "incomplete"
            failed = run_adapter("restore", incomplete, "--input-dir", artifacts, "--parts", 10)
            self.assertNotEqual(failed.returncode, 0)
            self.assertFalse(incomplete.exists())

    def test_json_report_bytes_survive_generated_messages(self):
        rng = random.Random(20261013)
        for count in range(1, 35):
            files = {f"negative-example-{index}.json": json.dumps({
                "index": index, "passed": bool(rng.randrange(2)),
                "text": "Цвет: " + "".join(chr(rng.randrange(32, 127)) for _ in range(index * 9)),
            }, ensure_ascii=False, indent=2).encode() + b"\r\n" for index in range(count)}
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                original = report_part(root / "producer", "mutants", reports=files)
                artifacts = root / "artifacts"
                staged = artifacts / "core-formal-mutants-0-1"
                OUTPUTS.prepare(root / "producer", staged)
                destination = root / "restored"
                OUTPUTS.restore(destination, artifacts, 1)
                self.assertEqual({name: (destination / name).read_bytes() for name in original}, original)

    def test_empty_malformed_duplicate_and_external_names_refuse(self):
        fragments = [[], None, {}, {"kind": "unknown"},
                     {"kind": "positive", "shard_index": True, "shard_count": 1},
                     {"kind": "positive", "shard_index": 0, "shard_count": False},
                     {"kind": "positive", "shard_index": 1, "shard_count": 1},
                     {"kind": "positive", "shard_index": 0, "shard_count": 1, "file": "../outside.json"},
                     {"kind": "positive", "shard_index": 0, "shard_count": 1, "file": {}},
                     {"kind": "mutants", "shard_index": 0, "shard_count": 1, "negative_sha256": []},
                     {"kind": "mutants", "shard_index": 0, "shard_count": 1, "negative_sha256": {}}]
        for fragment in fragments:
            with self.subTest(fragment=fragment), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                (root / "fragment-positive-0.json").write_text(json.dumps(fragment))
                with self.assertRaises(ValueError):
                    OUTPUTS.collect(root)
        for data in [b"not JSON", b'{"a":1,"a":2}', b"\xff"]:
            with self.subTest(data=data), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                report_part(root)
                (root / "positive-0.json").write_bytes(data)
                with self.assertRaises(ValueError):
                    OUTPUTS.collect(root)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.assertRaises(ValueError):
                OUTPUTS.collect(root)
            report_part(root)
            (root / "fragment-positive-1.json").write_text("{}")
            with self.assertRaises(ValueError):
                OUTPUTS.collect(root)

    def test_truncation_corruption_and_trailing_streams_refuse(self):
        for data in [b'{"checked":', b'{"checked":true}{}', b'{"checked":true}\x00']:
            with self.subTest(data=data), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                artifacts = root / "artifacts"
                part, _ = artifact_part(artifacts)
                (part / "positive-0.json").write_bytes(data)
                destination = root / "restored"
                with self.assertRaises(ValueError):
                    OUTPUTS.restore(destination, artifacts, 1)
                self.assertFalse(destination.exists())
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifacts = root / "artifacts"
            part, _ = artifact_part(artifacts)
            (part / "extra.json").write_text("{}")
            with self.assertRaises(ValueError):
                OUTPUTS.restore(root / "restored", artifacts, 1)
            (part / "extra.json").unlink()
            part.rename(artifacts / "core-formal-mutants-0-1")
            with self.assertRaises(ValueError):
                OUTPUTS.restore(root / "restored", artifacts, 1)
            self.assertFalse((root / "restored").exists())

    def test_output_and_read_limits_do_not_allow_partial_results(self):
        for identity in [None, "", "0", "01", "+1", "-1", " 1", "1,2", "1\n", "9" * 100,
                         "9007199254740992"]:
            with self.subTest(identity=identity), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                report_part(root)
                output = root / "job-output"
                environment = {"GITHUB_OUTPUT": str(output)}
                if identity is not None:
                    environment["CI_FORMAL_ARTIFACT_ID"] = identity
                result = run_adapter("publish", root, environment=environment)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(output.exists())
        for parts in [{"CI_FORMAL_PART_0": "1"},
                      {"CI_FORMAL_PART_0": "1", "CI_FORMAL_PART_1": "1"},
                      {"CI_FORMAL_PART_0": "1", "CI_FORMAL_PART_1": "2", "CI_FORMAL_PART_2": "3"}]:
            with self.subTest(parts=parts), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary) / "job-output"
                result = run_adapter("ids", "--parts", 2,
                                     environment={"GITHUB_OUTPUT": str(output), **parts})
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(output.exists())
        for reports in [{"positive-0.json": json.dumps("x" * OUTPUTS.MAX_REPORT_BYTES).encode()},
                        {f"negative-example-{index}.json": json.dumps("x" * (OUTPUTS.MAX_REPORT_BYTES // 2)).encode()
                         for index in range(2)},
                        {f"negative-example-{index}.json": b"{}" for index in range(OUTPUTS.MAX_FILES)}]:
            with self.subTest(count=len(reports)), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                source = root / "producer"
                report_part(source, "positive" if len(reports) == 1 else "mutants", reports=reports)
                destination = root / "staged"
                with self.assertRaises(ValueError):
                    OUTPUTS.prepare(source, destination)
                self.assertFalse(destination.exists())
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifacts = root / "artifacts"
            for index in range(2):
                artifact_part(artifacts, "mutants", index, 2, reports={
                    f"negative-example-{index}-{number}.json": b"{}" for number in range(64)})
            with self.assertRaises(ValueError):
                OUTPUTS.restore(root / "restored", artifacts, 2)
            self.assertFalse((root / "restored").exists())

    def test_colliding_parts_leave_no_destination(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifacts = root / "artifacts"
            artifact_part(artifacts, index=0, count=2, reports={"shared.json": b"{}"})
            artifact_part(artifacts, index=1, count=2, reports={"shared.json": b"{}"})
            with self.assertRaises(ValueError):
                OUTPUTS.restore(root / "restored", artifacts, 2)
            self.assertFalse((root / "restored").exists())
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifacts = root / "artifacts"
            part, files = artifact_part(artifacts)
            duplicate, _ = artifact_part(artifacts, attempt=2)
            with self.assertRaises(ValueError):
                OUTPUTS.restore(root / "restored", artifacts, 2)
            self.assertFalse((root / "restored").exists())
            shutil.rmtree(duplicate)
            OUTPUTS.restore(root / "restored", artifacts, 1)
            self.assertEqual({name: (root / "restored" / name).read_bytes() for name in files}, files)
            with self.assertRaises(FileExistsError):
                OUTPUTS.restore(root / "restored", artifacts, 1)
            (part / "positive-0.json").unlink()
            with self.assertRaises((ValueError, FileNotFoundError)):
                OUTPUTS.restore(root / "missing", artifacts, 1)
            self.assertFalse((root / "missing").exists())

    def test_collection_follows_the_fragment_and_preserves_declared_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            files = report_part(root)
            (root / "unused.log").write_text("diagnostic already in job log")
            self.assertEqual(files, OUTPUTS.collect(root))
            (root / "positive-0.json").unlink()
            with self.assertRaises(FileNotFoundError):
                OUTPUTS.collect(root)
            (root / "positive-0.json").symlink_to(root / "unused.log")
            with self.assertRaises(OSError):
                OUTPUTS.collect(root)
            (root / "positive-0.json").unlink()
            os.mkfifo(root / "positive-0.json")
            with self.assertRaises(ValueError):
                OUTPUTS.collect(root)
            alias = root.parent / (root.name + "-alias")
            alias.symlink_to(root, target_is_directory=True)
            try:
                with self.assertRaises(OSError):
                    OUTPUTS.collect(alias)
            finally:
                alias.unlink()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifacts = root / "artifacts"
            part, _ = artifact_part(artifacts)
            shutil.rmtree(part)
            part.symlink_to(root, target_is_directory=True)
            with self.assertRaises(OSError):
                OUTPUTS.restore(root / "restored", artifacts, 1)
            self.assertFalse((root / "restored").exists())
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source"
            original = report_part(source)
            read = OUTPUTS.read_report
            replaced = False

            def replace_directory_after_admission(directory, name, budget):
                nonlocal replaced
                if not replaced:
                    replaced = True
                    source.rename(root / "admitted")
                    report_part(source, reports={"positive-0.json": b'{"checked":false}\n'})
                return read(directory, name, budget)

            with mock.patch.object(OUTPUTS, "read_report", side_effect=replace_directory_after_admission):
                self.assertEqual(OUTPUTS.collect(source), original)

    def test_mutant_manifest_selects_every_named_negative_report(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            files = report_part(root, "mutants", reports={"negative-one.json": b"{}\n",
                                                        "negative-two.json": b"[]\n"})
            self.assertEqual(files, OUTPUTS.collect(root))
            (root / "negative-two.json").unlink()
            with self.assertRaises(FileNotFoundError):
                OUTPUTS.collect(root)


if __name__ == "__main__":
    unittest.main()
