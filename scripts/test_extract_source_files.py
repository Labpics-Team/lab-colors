#!/usr/bin/env python3
"""EXT-01 sabotage controls and RED→GREEN proof tests.

Verifies that the source-files extractor:
1. Produces a valid manifest with correct schema.
2. Fails when a listed file is missing (MISSING sabotage).
3. Fails when a listed file's content drifts (DRIFT sabotage).
4. Fails when an unlisted .rs file appears (UNLISTED sabotage).
5. Passes on the current production tree (GREEN proof).
"""
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
EXTRACTOR = REPO_ROOT / "scripts" / "extract_source_files.py"


def run_extractor(mode: str, stdin_data: bytes | None = None) -> subprocess.CompletedProcess:
    cmd = [sys.executable, str(EXTRACTOR), mode]
    return subprocess.run(
        cmd,
        capture_output=True,
        input=stdin_data,
        cwd=str(REPO_ROOT),
    )


class EXT01SourceFilesTests(unittest.TestCase):
    """RED→GREEN proof + sabotage controls for EXT-01."""

    def test_extract_produces_valid_manifest_schema(self):
        """GREEN: extractor produces manifest with required fields."""
        result = run_extractor("extract")
        self.assertEqual(result.returncode, 0, f"extractor failed: {result.stderr.decode()}")
        manifest = json.loads(result.stdout)
        self.assertEqual(manifest["class"], "source-files")
        self.assertEqual(manifest["schema_version"], 1)
        self.assertIn("files", manifest)
        self.assertIn("manifest_sha256", manifest)
        self.assertGreater(manifest["file_count"], 0)
        for entry in manifest["files"]:
            self.assertIn("path", entry)
            self.assertIn("sha256", entry)
            self.assertEqual(len(entry["sha256"]), 64)

    def test_verify_passes_on_current_tree(self):
        """GREEN: verify mode passes on live production sources."""
        extract = run_extractor("extract")
        self.assertEqual(extract.returncode, 0)
        verify = run_extractor("verify", stdin_data=extract.stdout)
        self.assertEqual(verify.returncode, 0, f"verify failed: {verify.stderr.decode()}")
        self.assertIn(b"EXT-01 VERIFY OK", verify.stderr)

    def test_sabotage_missing_file_detected(self):
        """RED: verify fails when a listed file is absent."""
        extract = run_extractor("extract")
        manifest = json.loads(extract.stdout)
        # Remove one file entry's actual file temporarily
        victim = manifest["files"][0]
        victim_path = REPO_ROOT / victim["path"]
        backup = victim_path.read_bytes()
        try:
            victim_path.unlink()
            verify = run_extractor("verify", stdin_data=extract.stdout)
            self.assertNotEqual(verify.returncode, 0, "verify should fail on missing file")
            self.assertIn(b"MISSING", verify.stderr)
        finally:
            victim_path.write_bytes(backup)

    def test_sabotage_content_drift_detected(self):
        """RED: verify fails when file content changes."""
        extract = run_extractor("extract")
        manifest = json.loads(extract.stdout)
        victim = manifest["files"][0]
        victim_path = REPO_ROOT / victim["path"]
        backup = victim_path.read_bytes()
        try:
            victim_path.write_bytes(b"SABOTAGE_DRIFT_TEST_CONTENT")
            verify = run_extractor("verify", stdin_data=extract.stdout)
            self.assertNotEqual(verify.returncode, 0, "verify should fail on drift")
            self.assertIn(b"DRIFT", verify.stderr)
        finally:
            victim_path.write_bytes(backup)

    def test_sabotage_unlisted_file_detected(self):
        """RED: verify fails when an unlisted .rs file exists."""
        extract = run_extractor("extract")
        phantom = REPO_ROOT / "crates" / "labcolors-core" / "src" / "_phantom_sabotage_test.rs"
        try:
            phantom.write_text("// phantom file for sabotage test\n")
            verify = run_extractor("verify", stdin_data=extract.stdout)
            self.assertNotEqual(verify.returncode, 0, "verify should fail on unlisted file")
            self.assertIn(b"UNLISTED", verify.stderr)
        finally:
            if phantom.exists():
                phantom.unlink()

    def test_manifest_sha256_self_consistent(self):
        """GREEN: manifest_sha256 matches recomputed canonical hash."""
        extract = run_extractor("extract")
        manifest = json.loads(extract.stdout)
        claimed = manifest.pop("manifest_sha256")
        canonical = json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
        recomputed = hashlib.sha256(canonical).hexdigest()
        self.assertEqual(claimed, recomputed)

    def test_unknown_mode_exits_64(self):
        """RED: invalid mode returns exit code 64 (usage error)."""
        result = run_extractor("bogus-mode")
        self.assertEqual(result.returncode, 64)


if __name__ == "__main__":
    unittest.main()