"""Отказ до внешнего эффекта при некорректных координатах workflow."""

from __future__ import annotations

from contextlib import redirect_stderr, redirect_stdout
import io
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch

import test_workflow_inputs as workflow_inputs
import test_corpus_campaign_guard as corpus_guard
import test_corpus_assembly as assembly_fixture
import dual_proof_gate as native_gate

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
                coordinates = {
                    "POINTS": "'; printf points > marker; #",
                    "SHARD_POINTS": "'; printf shard > marker; #",
                    "WINDOW_START": "'; printf start > marker; #",
                    "WINDOW_POINTS": "'; printf window > marker; #",
                }
                result = subprocess.run(
                    ("bash", "-c", script), cwd=root, capture_output=True,
                    env={**os.environ, **coordinates, "PATH": str(root) + os.pathsep + os.environ["PATH"], "CAPTURE": str(root / "args")},
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertFalse((root / "marker").exists())
                args = (root / "args").read_bytes().split(b"\0")
                expected = (
                    ((b"--points", "POINTS"), (b"--shard-points", "SHARD_POINTS"))
                    if name == "replay the bounded full-domain prefix through the shard runner"
                    else (
                        (b"--window-start", "WINDOW_START"),
                        (b"--window-points", "WINDOW_POINTS"),
                        (b"--shard-points", "SHARD_POINTS"),
                    )
                )
                for flag, key in expected:
                    self.assertEqual(args.count(flag), 1)
                    index = args.index(flag)
                    self.assertLess(index + 1, len(args))
                    self.assertEqual(args[index + 1], coordinates[key].encode())

    def test_download_rejects_entire_invalid_list_before_gh(self) -> None:
        for workflow, name in (
            ("full-domain-corpus.yml", "download the lane wire evidence"),
            ("dual-proof.yml", "download both engines' verification lane covers"),
        ):
            with self.subTest(workflow=workflow):
                self._assert_invalid_run_list_refused(workflow, name)

    def _assert_invalid_run_list_refused(self, workflow: str, name: str) -> None:
        script = step_script(workflow, name)
        script = script.replace("${{ github.repository }}", "Labpics-Team/test")
        for value in ("-R", "../foreign", "0", "00123", "123,,456", "123,", "123, invalid", "123\n456"):
            with self.subTest(value=value), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                stub = root / "gh"
                stub.write_text('#!/bin/sh\nprintf called >> "$CAPTURE"\n')
                stub.chmod(0o700)
                initial_files = {"gh": stub.read_bytes()}
                result = subprocess.run(
                    ("bash", "-c", script), cwd=root, capture_output=True,
                    env={**os.environ, "LANE_RUN_IDS": value, "PATH": str(root) + os.pathsep + os.environ["PATH"], "CAPTURE": str(root / "calls")},
                )
                self.assertFalse((root / "calls").exists())
                self.assertEqual(
                    {str(path.relative_to(root)): path.read_bytes() if path.is_file() else None
                     for path in root.rglob("*")},
                    initial_files,
                    "invalid input changed files before rejection",
                )
                self.assertEqual(result.returncode, 64, result.stderr)

    def _run_cover_preflight_v1(self, root: Path, job, comparators) -> tuple[int, str]:
        shell = step_script(
            "dual-proof.yml",
            "refuse an incomplete cover before anything expensive is built",
        )
        self.assertIn(
            "python3 proof/region/v1/tests/dual_proof_gate.py --preflight", shell
        )
        # Only source-archive admission is replaced by the reduced fixture.
        # The real CLI, source roster, lane parser and cover law all execute.
        modules = [
            SimpleNamespace(full_domain_request_v1=lambda: SimpleNamespace(job=job))
            for _ in comparators
        ]
        output = io.StringIO()
        with (
            patch.dict(os.environ, {native_gate.LANES_ENV_V1: str(root)}),
            patch.object(native_gate, "_load_lane_module_v1", side_effect=modules),
            patch.object(native_gate, "_assert_engine_modules_are_unmixed_v1"),
            patch.object(
                native_gate.arb_pipeline, "expected_comparator_source_identity_v1",
                return_value=comparators[0].source_identity,
            ),
            patch.object(
                native_gate.mpfi_receipt, "expected_comparator_source_identity_v1",
                return_value=comparators[1].source_identity,
            ),
            patch.object(native_gate, "_sealed_engine_receipt_v1") as native,
            redirect_stdout(output), redirect_stderr(output),
        ):
            result = native_gate.main(["--preflight"])
            native.assert_not_called()
        return result, output.getvalue()

    def test_cover_rejects_malformed_manifests_with_usage_status(self) -> None:
        job = assembly_fixture._job_with_pregrant(tuple(range(64)), 0)
        comparators = tuple(
            assembly_fixture._comparator(kind)
            for kind in assembly_fixture.protocol.ComparatorKindV1
        )
        valid = {
            "comparator_source_identity": comparators[0].source_identity.hex(),
            "window_start": 0, "window_points": 64,
        }
        invalid_manifests = (
            "{", "[]", "{}", json.dumps({**valid, "comparator_source_identity": []}),
            json.dumps({**valid, "window_start": True}),
            json.dumps({**valid, "window_points": 0}),
            json.dumps({**valid, "window_points": "64"}),
        )
        for invalid in invalid_manifests:
            with self.subTest(value=invalid), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                lane = root / "lane"
                lane.mkdir()
                (lane / "lane-manifest.json").write_text(invalid, encoding="ascii")
                status, output = self._run_cover_preflight_v1(root, job, comparators)
                self.assertEqual(status, 64, output)
                self.assertIn("lane preflight rejected", output)

    def test_download_preserves_valid_run_ids(self) -> None:
        for workflow, name in (
            ("full-domain-corpus.yml", "download the lane wire evidence"),
            ("dual-proof.yml", "download both engines' verification lane covers"),
        ):
            with self.subTest(workflow=workflow):
                self._assert_valid_run_list_downloaded(workflow, name)

    def _assert_valid_run_list_downloaded(self, workflow: str, name: str) -> None:
        script = step_script(workflow, name)
        script = script.replace("${{ github.repository }}", "Labpics-Team/test")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            stub = root / "gh"
            stub.write_text('''#!/bin/sh
run="$3"
printf "%s\\n" "$run" >> "$CAPTURE"
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--dir" ]; then
    destination="$2/verification-lane-$run"
    mkdir -p "$destination"
    printf "%s" "$run" > "$destination/payload"
    exit 0
  fi
  shift
done
exit 65
''')
            stub.chmod(0o700)
            result = subprocess.run(
                ("bash", "-c", script), cwd=root, capture_output=True,
                env={**os.environ, "LANE_RUN_IDS": " 123, 456 ", "PATH": str(root) + os.pathsep + os.environ["PATH"], "CAPTURE": str(root / "calls"), "GITHUB_WORKSPACE": str(root), "GITHUB_ENV": str(root / "github-env")},
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((root / "calls").read_text().splitlines(), ["123", "456"])
            if workflow == "dual-proof.yml":
                for run in ("123", "456"):
                    self.assertEqual((root / "lanes-in" / f"verification-lane-{run}" / "payload").read_text(), run)

    def test_run_list_is_admitted_before_checkout(self) -> None:
        # Отказ, который стоит checkout, загрузки source closures и OCI-образов,
        # не является ранним отказом. Шаг admission обязан стоять до checkout
        # в обоих потребителях lane_run_ids и сам отвергать hostile список.
        for workflow in ("full-domain-corpus.yml", "dual-proof.yml"):
            with self.subTest(workflow=workflow):
                text = (WORKFLOWS / workflow).read_text("utf-8")
                admission = text.index("      - name: refuse a malformed lane run list before checkout")
                # Границы job — заголовки второго уровня `  <job-id>:`; шаг обязан стоять
                # до checkout именно своего job, а не любого checkout далее в файле.
                headers = [m.start() for m in re.finditer(r"^  [a-z][a-z0-9_-]*:\s*$", text, re.MULTILINE)]
                job_start = max(h for h in headers if h < admission)
                job_end = min([h for h in headers if h > admission] + [len(text)])
                job = text[job_start:job_end]
                self.assertIn("actions/checkout@", job, "the admission job must perform a checkout")
                self.assertLess(
                    admission - job_start, job.index("actions/checkout@"),
                    "run-list admission must precede checkout in its job",
                )
                # Каждый job, потребляющий lane_run_ids, обязан иметь этот шаг.
                for other_start, other_end in zip(headers, headers[1:] + [len(text)]):
                    body = text[other_start:other_end]
                    if "inputs.lane_run_ids" in body:
                        self.assertIn("refuse a malformed lane run list before checkout", body,
                                      f"job at offset {other_start} consumes lane_run_ids without admission")
                script = step_script(workflow, "refuse a malformed lane run list before checkout")
                self.assertNotRegex(script, r"\$\{\{[^}]*inputs\.")
                for value in ("-R", "0", "00123", "123,,456", "123,", "123, invalid", "123\n456", "'; touch pwned; #"):
                    with self.subTest(value=value), tempfile.TemporaryDirectory() as directory:
                        result = subprocess.run(
                            ("bash", "-c", script), cwd=directory, capture_output=True,
                            env={**os.environ, "LANE_RUN_IDS": value},
                        )
                        self.assertEqual(result.returncode, 64, result.stderr)
                        self.assertEqual(list(Path(directory).iterdir()), [], "admission must not create files")
                result = subprocess.run(
                    ("bash", "-c", script), capture_output=True, env={**os.environ, "LANE_RUN_IDS": " 123, 456 "},
                )
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_cover_accepts_two_complete_engines(self) -> None:
        job = assembly_fixture._job_with_pregrant(tuple(range(64)), 0)
        comparators = tuple(
            assembly_fixture._comparator(kind)
            for kind in assembly_fixture.protocol.ComparatorKindV1
        )
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for index, comparator in enumerate(comparators):
                assembly_fixture._write_lane_dir(
                    root, str(index), job, comparator, 0, 64, 32
                )
            status, output = self._run_cover_preflight_v1(root, job, comparators)
            self.assertEqual(status, 0, output)
            self.assertIn("complete, validated lane covers", output)


if __name__ == "__main__":
    unittest.main()
