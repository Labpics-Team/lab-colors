#!/usr/bin/env python3
"""Контроль непустого охвата и отказа второго прохода секретов."""

from __future__ import annotations

from contextlib import redirect_stderr
from datetime import datetime, timezone
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


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "ci" / "strict_secret_scan.py"
CARRIER_SNAPSHOT = (
    ROOT / ".github/actions/labpics-code-admission/plans/tools/code-admission/snapshot.mjs"
)
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
    def test_scan_uses_raw_git_blob_when_checkout_transcodes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            subject = work / ".code-subject.public01"
            subject.mkdir()
            environment = os.environ | {
                "GIT_CONFIG_NOSYSTEM": "1",
                "GIT_CONFIG_GLOBAL": os.devnull,
                "GIT_NO_REPLACE_OBJECTS": "1",
            }

            def git(*args: str) -> bytes:
                return subprocess.run(
                    ["git", "-C", str(subject), *args],
                    env=environment, capture_output=True, check=True,
                ).stdout

            git("init", "-q")
            (subject / ".gitattributes").write_text(
                "tests/secret.txt text working-tree-encoding=UTF-16LE\n", encoding="utf-8"
            )
            target = subject / "tests" / "secret.txt"
            target.parent.mkdir()
            marker = "AKIA" + "ABCDEFGHIJKLMNOP"
            content = f"access_key_id = {marker}\n"
            target.write_bytes(content.encode("utf-16le"))
            git("add", ".gitattributes", "tests/secret.txt")
            git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                "commit", "-qm", "fixture")
            sha = git("rev-parse", "HEAD").decode().strip()
            blob = git("show", f"{sha}:tests/secret.txt")
            self.assertEqual(blob, content.encode("utf-8"))
            self.assertNotEqual(target.read_bytes(), blob)
            self.assertFalse(git("status", "--porcelain", "--untracked-files=all"))

            binary = work / "fixture-trivy"
            binary.write_text(
                f"#!{sys.executable}\n"
                "from datetime import datetime, timezone\n"
                "import json, pathlib, sys\n"
                "root = pathlib.Path(sys.argv[-1])\n"
                "marker = b'AKIA' + b'ABCDEFGHIJKLMNOP'\n"
                "paths = sorted(p.relative_to(root).as_posix() for p in root.rglob('*') "
                "if p.is_file() and marker in p.read_bytes())\n"
                "print(json.dumps({'SchemaVersion': 2, 'ArtifactType': 'filesystem', "
                "'ArtifactName': str(root), 'CreatedAt': datetime.now(timezone.utc).isoformat(), "
                "'Trivy': {'Version': '0.74.0'}, 'Results': "
                "[{'Class': 'secret', 'Target': p, 'Secrets': [{'RuleID': 'fixture'}]} "
                "for p in paths]}))\n"
                "raise SystemExit(bool(paths))\n",
                encoding="utf-8",
            )
            binary.chmod(0o700)
            errors = io.StringIO()
            with redirect_stderr(errors):
                self.assertEqual(scanner.run(binary, subject, sha, work, CARRIER_SNAPSHOT), 1)
            self.assertIn("1 файлов", errors.getvalue())
            self.assertNotIn(marker, errors.getvalue())
            installed = work / "strict_secret_scan.py"
            shutil.copyfile(SCRIPT, installed)
            cli = subprocess.run(
                [sys.executable, "-I", str(installed), str(binary), str(subject), sha,
                 str(work), str(CARRIER_SNAPSHOT)],
                capture_output=True, text=True, timeout=30, check=False,
            )
            self.assertEqual(cli.returncode, 1)
            self.assertNotIn(marker, cli.stderr)

            target.write_bytes("safe content\n".encode("utf-16le"))
            git("add", "tests/secret.txt")
            git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                "commit", "-qm", "clean fixture")
            clean_sha = git("rev-parse", "HEAD").decode().strip()
            self.assertNotEqual(clean_sha, sha)
            self.assertFalse(git("status", "--porcelain", "--untracked-files=all"))
            self.assertEqual(scanner.run(binary, subject, clean_sha, work, CARRIER_SNAPSHOT), 0)
            clean_cli = subprocess.run(
                [sys.executable, "-I", str(installed), str(binary), str(subject), clean_sha,
                 str(work), str(CARRIER_SNAPSHOT)],
                capture_output=True, text=True, timeout=30, check=False,
            )
            self.assertEqual(clean_cli.returncode, 0, clean_cli.stderr)

    def test_run_rejects_incomplete_probe_and_bad_sha(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            subject = work / "subject"
            subject.mkdir()
            with self.assertRaises(scanner.IncompleteScan):
                scanner._verify_candidate(subject, "not-a-commit")
            with (mock.patch.object(scanner, "_verify_candidate", return_value="0" * 40),
                  mock.patch.object(scanner, "_run_scan", return_value={"README.md"}),
                  mock.patch.object(scanner, "_materialize_candidate") as materialize):
                with self.assertRaisesRegex(scanner.IncompleteScan, "контрольные секреты"):
                    scanner.run(Path(sys.executable), subject, "0" * 40, work, CARRIER_SNAPSHOT)
                materialize.assert_not_called()

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
        self.assertIn('install -m 0600 scripts/ci/strict_secret_scan.py "$helper"', source)
        self.assertIn('printf "%s  %s\\n" "$helper_sha256" "$helper" | sha256sum -c -', source)
        self.assertIn('python3 -I "$helper"', source)
        self.assertLess(
            source.index('printf "%s  %s\\n" "$helper_sha256" "$helper" | sha256sum -c -'),
            source.index('python3 -I "$helper"'),
        )
        self.assertIn(
            '"$GITHUB_WORKSPACE/.github/actions/labpics-code-admission/'
            'plans/tools/code-admission/snapshot.mjs"', source,
        )
        pinned = re.search(r"(?m)^          helper_sha256=([0-9a-f]{64})$", source)
        self.assertIsNotNone(pinned)
        assert pinned is not None
        self.assertEqual(pinned.group(1), hashlib.sha256(SCRIPT.read_bytes()).hexdigest())
        sabotage = SCRIPT.read_bytes().replace(
            b"    tree = _verify_candidate(subject, sha)\n",
            b'    if subject.name == ".code-subject.public01":\n'
            b"        return 0\n"
            b"    tree = _verify_candidate(subject, sha)\n",
            1,
        )
        self.assertNotEqual(sabotage, SCRIPT.read_bytes())
        self.assertNotEqual(hashlib.sha256(sabotage).hexdigest(), pinned.group(1))
        self.assertIn("2ae6fe3ee734b7fdf11335663e18c75ea12dccc76062f09f164a3b0f8be4371a", source)
        self.assertIn("- name: Remove candidate checkout\n        if: ${{ always() }}", source)


if __name__ == "__main__":
    unittest.main()
