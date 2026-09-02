#!/usr/bin/env python3
"""EXT-07 sabotage controls and RED→GREEN proof tests.

Verifies that the resource dimensions extractor:
1. Produces a valid manifest with correct schema.
2. Fails when manifest_sha256 is tampered (integrity sabotage).
3. Fails when entry_count mismatches actual entries (consistency sabotage).
4. Fails when source file content drifts after extraction (content integrity).
5. Fails on malformed JSON input (typed error, not traceback).
6. Passes on the current production tree (GREEN proof).
"""
import hashlib
import json
import subprocess
import sys
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
EXTRACTOR = REPO_ROOT / "scripts" / "extract_resources.py"


def run_extractor(mode: str, stdin_data: bytes | None = None) -> subprocess.CompletedProcess:
    cmd = [sys.executable, str(EXTRACTOR), mode]
    return subprocess.run(
        cmd,
        capture_output=True,
        input=stdin_data,
        cwd=str(REPO_ROOT),
    )


class EXT07ResourcesTests(unittest.TestCase):
    """RED→GREEN proof + sabotage controls for EXT-07."""

    def test_extract_produces_valid_manifest_schema(self):
        """GREEN: extractor produces manifest with required fields."""
        result = run_extractor("extract")
        self.assertEqual(result.returncode, 0, f"extractor failed: {result.stderr.decode()}")
        manifest = json.loads(result.stdout)
        self.assertEqual(manifest["class"], "resource-dimensions")
        self.assertEqual(manifest["schema_version"], 1)
        self.assertIn("entries", manifest)
        self.assertIn("manifest_sha256", manifest)
        self.assertGreater(manifest["entry_count"], 0)
        for entry in manifest["entries"]:
            self.assertIn("path", entry)
            self.assertIn("line", entry)
            self.assertIn("name", entry)
            self.assertIn("value", entry)
            self.assertIn("source_sha256", entry)

    def test_verify_passes_on_current_tree(self):
        """GREEN: verify mode passes on live production sources."""
        extract = run_extractor("extract")
        self.assertEqual(extract.returncode, 0)
        verify = run_extractor("verify", stdin_data=extract.stdout)
        self.assertEqual(verify.returncode, 0, f"verify failed: {verify.stderr.decode()}")
        self.assertIn(b"EXT-07 VERIFY OK", verify.stderr)

    def test_sabotage_tampered_manifest_hash_detected(self):
        """RED: verify fails when manifest_sha256 is tampered."""
        extract = run_extractor("extract")
        manifest = json.loads(extract.stdout)
        manifest["manifest_sha256"] = "0" * 64
        tampered = json.dumps(manifest, sort_keys=True).encode()
        verify = run_extractor("verify", stdin_data=tampered)
        self.assertNotEqual(verify.returncode, 0, "verify should fail on tampered hash")
        self.assertIn(b"mismatch", verify.stderr)

    def test_sabotage_entry_count_mismatch_detected(self):
        """RED: verify fails when entry_count doesn't match actual entries."""
        extract = run_extractor("extract")
        manifest = json.loads(extract.stdout)
        manifest["entry_count"] = 999999
        payload = {k: v for k, v in manifest.items() if k != "manifest_sha256"}
        canonical = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
        manifest["manifest_sha256"] = hashlib.sha256(canonical).hexdigest()
        tampered = json.dumps(manifest, sort_keys=True).encode()
        verify = run_extractor("verify", stdin_data=tampered)
        self.assertNotEqual(verify.returncode, 0, "verify should fail on count mismatch")
        self.assertIn(b"entry_count mismatch", verify.stderr)

    def test_sabotage_content_drift_detected(self):
        """RED: verify fails when source file content changes after extraction."""
        extract = run_extractor("extract")
        manifest = json.loads(extract.stdout)
        # Pick first entry and modify its source file
        victim = manifest["entries"][0]
        victim_path = REPO_ROOT / victim["path"]
        backup = victim_path.read_bytes()
        try:
            victim_path.write_bytes(b"SABOTAGE_DRIFT_TEST_CONTENT\n")
            verify = run_extractor("verify", stdin_data=extract.stdout)
            self.assertNotEqual(verify.returncode, 0, "verify should fail on content drift")
            self.assertIn(b"CONTENT_DRIFT", verify.stderr)
        finally:
            victim_path.write_bytes(backup)

    def test_sabotage_malformed_json_exits_typed_error(self):
        """RED: verify exits with typed error code on malformed JSON, not traceback."""
        verify = run_extractor("verify", stdin_data=b"{invalid json")
        self.assertNotEqual(verify.returncode, 0)
        self.assertIn(verify.returncode, (65, 66, 67))
        self.assertIn(b"SABOTAGE", verify.stderr)
        self.assertNotIn(b"Traceback", verify.stderr)

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