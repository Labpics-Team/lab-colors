#!/usr/bin/env python3
"""Контроль непустого охвата и отказа второго прохода секретов."""

from __future__ import annotations

from datetime import datetime, timezone
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "ci" / "strict_secret_scan.py"
SPEC = importlib.util.spec_from_file_location("strict_secret_scan", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
scanner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(scanner)


def report(subject: Path, paths: tuple[str, ...] = ()) -> dict:
    return {
        "SchemaVersion": 2,
        "ArtifactType": "filesystem",
        "ArtifactName": str(subject),
        "CreatedAt": datetime.now(timezone.utc).isoformat(),
        "Trivy": {"Version": "0.74.0"},
        "Results": [
            {"Class": "secret", "Target": path,
             "Secrets": [{"RuleID": "synthetic", "Match": "raw-match-must-stay-private"}]}
            for path in paths
        ],
    }


class StrictSecretScanTest(unittest.TestCase):
    def test_report_requires_consistent_exit_and_exact_subject(self) -> None:
        subject = Path("/tmp/subject")
        started = datetime.now(timezone.utc)
        clean = report(subject)
        found = report(subject, ("README.md",))
        self.assertEqual(scanner._secret_paths(clean, subject, 0, started), set())
        inventory = clean | {"Results": [{"Class": "lang-pkgs", "Target": "Cargo.lock"}]}
        self.assertEqual(scanner._secret_paths(inventory, subject, 0, started), set())
        self.assertEqual(scanner._secret_paths(found, subject, 1, started), {"README.md"})
        for raw, status in ((clean, 1), (found, 0), (clean, 2)):
            with self.subTest(status=status, findings=len(raw["Results"])):
                with self.assertRaises(scanner.IncompleteScan):
                    scanner._secret_paths(raw, subject, status, started)
        for change in (
            {"ArtifactName": "/tmp/other"},
            {"SchemaVersion": 1},
            {"Trivy": {"Version": "0.75.0"}},
            {"Results": [{"Class": "secret", "Target": "../outside",
                          "Secrets": [{"RuleID": "synthetic"}]}]},
            {"Results": None},
        ):
            with self.subTest(change=change):
                with self.assertRaises(scanner.IncompleteScan):
                    scanner._secret_paths(clean | change, subject, 0, started)

    def test_native_invocation_keeps_policy_and_raw_matches_private(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            subject = work / "subject"
            subject.mkdir()
            config = work / "secret.yaml"
            config.write_text(scanner.SECRET_CONFIG, encoding="utf-8")
            self.assertEqual(
                scanner.SECRET_CONFIG,
                "disable-allow-rules:\n  - tests\n  - examples\n  - markdown\n"
                "skip-patterns: []\n",
            )

            def native(args: list[str], **kwargs: object) -> subprocess.CompletedProcess:
                self.assertEqual(args[0:4], [str(work / "trivy"), "fs", "--config", "/dev/null"])
                self.assertEqual(args[args.index("--secret-config") + 1], str(config))
                self.assertEqual(args[args.index("--ignorefile") + 1], "/dev/null")
                self.assertEqual(args[args.index("--scanners") + 1], "secret")
                self.assertEqual(args[-1], str(subject))
                environment = kwargs["env"]
                self.assertFalse(any(key.startswith("TRIVY_") for key in environment))
                kwargs["stdout"].write(json.dumps(report(subject, ("README.md",))).encode())
                kwargs["stderr"].write(b"raw-match-must-stay-private")
                return subprocess.CompletedProcess(args, 1)

            with mock.patch.object(scanner.subprocess, "run", side_effect=native):
                self.assertEqual(
                    scanner._run_scan(work / "trivy", subject, config, work, "probe"),
                    {"README.md"},
                )

            def broken(args: list[str], **kwargs: object) -> subprocess.CompletedProcess:
                kwargs["stdout"].write(b"not JSON")
                kwargs["stderr"].write(b"raw-match-must-stay-private")
                return subprocess.CompletedProcess(args, 2)

            with mock.patch.object(scanner.subprocess, "run", side_effect=broken):
                with self.assertRaises(scanner.IncompleteScan) as captured:
                    scanner._run_scan(work / "trivy", subject, config, work, "broken")
            self.assertNotIn("raw-match-must-stay-private", str(captured.exception))

    def test_required_workflow_runs_probe_and_candidate_in_same_policy(self) -> None:
        source = (ROOT / ".github/workflows/code-admission-public.yml").read_text(
            encoding="utf-8"
        )
        self.assertEqual(source.count("- name: Scan secrets omitted by Trivy defaults"), 1)
        self.assertIn("candidate: ${{ github.sha }}", source)
        self.assertIn('"$GITHUB_WORKSPACE/.code-subject.public01" "$CANDIDATE_SHA"', source)
        self.assertIn("python3 scripts/ci/strict_secret_scan.py", source)
        self.assertIn("2ae6fe3ee734b7fdf11335663e18c75ea12dccc76062f09f164a3b0f8be4371a", source)
        self.assertIn("- name: Remove candidate checkout\n        if: ${{ always() }}", source)


if __name__ == "__main__":
    unittest.main()
