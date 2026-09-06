"""Отказ до внешнего эффекта при некорректных координатах workflow."""

from __future__ import annotations

import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import test_workflow_inputs as workflow_inputs
import test_corpus_campaign_guard as corpus_guard

WORKFLOWS = workflow_inputs.WORKFLOWS


def step_script(workflow: str, name: str) -> str:
    text = (WORKFLOWS / workflow).read_text("utf-8")
    start = text.index("      - name: " + name)
    body = text.index("        run: |\n", start) + len("        run: |\n")
    following = re.search(r"^      - |^  [a-z].*:$", text[body:], re.MULTILINE)
    end = body + following.start() if following else len(text)
    return "\n".join(line[10:] for line in text[body:end].splitlines()) + "\n"


class WorkflowAdmissionTests(unittest.TestCase):
    def test_missing_bash_fails_both_guard_suites(self) -> None:
        for kind in (workflow_inputs.LaneCampaignContractTests, corpus_guard.CorpusCampaignContractTests):
            case = kind()
            case.setUp()
            with self.subTest(suite=kind.__name__), patch("shutil.which", return_value=None):
                try:
                    case._run_guard_v1("65536", "256", "0")
                except unittest.SkipTest:
                    self.fail("Отсутствие bash не должно пропускать обязательную проверку")
                except AssertionError:
                    pass
                else:
                    self.fail("Отсутствие bash должно завершать проверку ошибкой")

    def test_replay_preserves_inputs_as_arguments(self) -> None:
        for name in (
            "replay the bounded full-domain prefix through the shard runner",
            "replay one full-domain window lane",
        ):
            script = step_script("full-domain-corpus.yml", name)
            self.assertNotRegex(script, r"\$\{\{[^}]*inputs\.")
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                stub = root / "python3"
                stub.write_text('#!/bin/sh\nprintf "%s\\0" "$@" > "$CAPTURE"\n')
                stub.chmod(0o700)
                coordinates = {key: "'; printf injected > marker; #" for key in ("POINTS", "SHARD_POINTS", "WINDOW_START", "WINDOW_POINTS")}
                result = subprocess.run(
                    ("bash", "-c", script), cwd=root, capture_output=True,
                    env={**os.environ, **coordinates, "PATH": str(root) + os.pathsep + os.environ["PATH"], "CAPTURE": str(root / "args")},
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertFalse((root / "marker").exists())
                args = (root / "args").read_bytes().split(b"\0")
                self.assertIn(coordinates["SHARD_POINTS"].encode(), args)

    def test_download_rejects_entire_invalid_list_before_gh(self) -> None:
        script = step_script("full-domain-corpus.yml", "download the lane wire evidence")
        script = script.replace("${{ github.repository }}", "Labpics-Team/test")
        for value in ("-R", "../foreign", "0", "00123", "123,,456", "123,", "123, invalid", "123\n456"):
            with self.subTest(value=value), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                stub = root / "gh"
                stub.write_text('#!/bin/sh\nprintf called >> "$CAPTURE"\n')
                stub.chmod(0o700)
                result = subprocess.run(
                    ("bash", "-c", script), cwd=root, capture_output=True,
                    env={**os.environ, "LANE_RUN_IDS": value, "PATH": str(root) + os.pathsep + os.environ["PATH"], "CAPTURE": str(root / "calls")},
                )
                self.assertEqual(result.returncode, 64, result.stderr)
                self.assertFalse((root / "calls").exists())

    def test_cover_rejects_malformed_manifests_with_usage_status(self) -> None:
        shell = step_script("dual-proof.yml", "refuse an incomplete cover before anything expensive is built")
        script = shell.split("python3 - <<'PY'\n", 1)[1].rsplit("\nPY", 1)[0]
        identity = "a" * 64
        valid = {"comparator_source_identity": identity, "window_start": 0, "window_points": 16777216}
        for invalid in ("{", "[]", "{}", json.dumps({**valid, "comparator_source_identity": []}), json.dumps({**valid, "window_start": True}), json.dumps({**valid, "window_points": 0}), json.dumps({**valid, "window_points": "16777216"})):
            with self.subTest(value=invalid), tempfile.TemporaryDirectory() as directory:
                lane = Path(directory) / "lanes-in" / "lane"
                lane.mkdir(parents=True)
                (lane / "lane-manifest.json").write_text(invalid, encoding="ascii")
                result = subprocess.run(
                    (sys.executable, "-c", script), cwd=directory, capture_output=True,
                    env={**os.environ, "PYTHONPATH": str(WORKFLOWS.parents[1] / "proof/region/v1")},
                )
                self.assertEqual(result.returncode, 64, result.stderr)

    def test_download_preserves_valid_run_ids(self) -> None:
        script = step_script("full-domain-corpus.yml", "download the lane wire evidence")
        script = script.replace("${{ github.repository }}", "Labpics-Team/test")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            stub = root / "gh"
            stub.write_text('#!/bin/sh\nprintf "%s\\n" "$3" >> "$CAPTURE"\n')
            stub.chmod(0o700)
            result = subprocess.run(
                ("bash", "-c", script), cwd=root, capture_output=True,
                env={**os.environ, "LANE_RUN_IDS": " 123, 456 ", "PATH": str(root) + os.pathsep + os.environ["PATH"], "CAPTURE": str(root / "calls")},
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((root / "calls").read_text().splitlines(), ["123", "456"])

    def test_cover_accepts_two_complete_engines(self) -> None:
        shell = step_script("dual-proof.yml", "refuse an incomplete cover before anything expensive is built")
        script = shell.split("python3 - <<'PY'\n", 1)[1].rsplit("\nPY", 1)[0]
        with tempfile.TemporaryDirectory() as directory:
            for identity in ("a" * 64, "b" * 64):
                lane = Path(directory) / "lanes-in" / identity
                lane.mkdir(parents=True)
                (lane / "lane-manifest.json").write_text(json.dumps({"comparator_source_identity": identity, "window_start": 0, "window_points": 16777216}), encoding="ascii")
            result = subprocess.run(
                (sys.executable, "-c", script), cwd=directory, capture_output=True,
                env={**os.environ, "PYTHONPATH": str(WORKFLOWS.parents[1] / "proof/region/v1")},
            )
            self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
