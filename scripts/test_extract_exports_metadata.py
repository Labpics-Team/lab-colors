#!/usr/bin/env python3
"""EXT-03 sabotage controls and RED→GREEN proof tests.

Verifies that the exports/metadata extractor:
1. Produces a valid manifest with correct schema.
2. Fails when manifest_sha256 is tampered (integrity sabotage).
3. Fails when crate_count mismatches actual crates (consistency sabotage).
4. Passes on the current production tree (GREEN proof).
"""
import hashlib
import json
import subprocess
import sys
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
EXTRACTOR = REPO_ROOT / "scripts" / "extract_exports_metadata.py"


def run_extractor(mode: str, stdin_data: bytes | None = None) -> subprocess.CompletedProcess:
    cmd = [sys.executable, str(EXTRACTOR), mode]
    return subprocess.run(
        cmd,
        capture_output=True,
        input=stdin_data,
        cwd=str(REPO_ROOT),
    )


class EXT03ExportsMetadataTests(unittest.TestCase):
    """RED→GREEN proof + sabotage controls for EXT-03."""

    def test_extract_produces_valid_manifest_schema(self):
        """GREEN: extractor produces manifest with required fields."""
        result = run_extractor("extract")
        self.assertEqual(result.returncode, 0, f"extractor failed: {result.stderr.decode()}")
        manifest = json.loads(result.stdout)
        self.assertEqual(manifest["class"], "exports-metadata")
        self.assertEqual(manifest["schema_version"], 1)
        self.assertIn("crates", manifest)
        self.assertIn("manifest_sha256", manifest)
        self.assertGreater(manifest["crate_count"], 0)
        # labcolors-core must be present
        names = [c["name"] for c in manifest["crates"]]
        self.assertIn("labcolors-core", names)
        for crate in manifest["crates"]:
            self.assertIn("name", crate)
            self.assertIn("version", crate)
            self.assertIn("features", crate)
            self.assertIn("deps", crate)
            self.assertIn("targets", crate)

    def test_verify_passes_on_current_tree(self):
        """GREEN: verify mode passes on live workspace metadata."""
        extract = run_extractor("extract")
        self.assertEqual(extract.returncode, 0)
        verify = run_extractor("verify", stdin_data=extract.stdout)
        self.assertEqual(verify.returncode, 0, f"verify failed: {verify.stderr.decode()}")
        self.assertIn(b"EXT-03 VERIFY OK", verify.stderr)

    def test_sabotage_tampered_manifest_hash_detected(self):
        """RED: verify fails when manifest_sha256 is tampered."""
        extract = run_extractor("extract")
        manifest = json.loads(extract.stdout)
        manifest["manifest_sha256"] = "0" * 64
        tampered = json.dumps(manifest, sort_keys=True).encode()
        verify = run_extractor("verify", stdin_data=tampered)
        self.assertNotEqual(verify.returncode, 0, "verify should fail on tampered hash")
        self.assertIn(b"mismatch", verify.stderr)

    def test_sabotage_crate_count_mismatch_detected(self):
        """RED: verify fails when crate_count doesn't match actual crates."""
        extract = run_extractor("extract")
        manifest = json.loads(extract.stdout)
        manifest["crate_count"] = 999999
        payload = {k: v for k, v in manifest.items() if k != "manifest_sha256"}
        canonical = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
        manifest["manifest_sha256"] = hashlib.sha256(canonical).hexdigest()
        tampered = json.dumps(manifest, sort_keys=True).encode()
        verify = run_extractor("verify", stdin_data=tampered)
        self.assertNotEqual(verify.returncode, 0, "verify should fail on count mismatch")
        self.assertIn(b"crate_count mismatch", verify.stderr)

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