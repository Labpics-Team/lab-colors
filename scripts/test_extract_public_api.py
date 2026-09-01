#!/usr/bin/env python3
"""EXT-02 sabotage controls and RED→GREEN proof tests.

Verifies that the public API extractor:
1. Produces a valid manifest with correct schema.
2. Fails when lib.rs is missing (crate root sabotage).
3. Fails when manifest_sha256 is tampered (integrity sabotage).
4. Fails when symbol_count mismatches actual symbols (consistency sabotage).
5. Passes on the current production tree (GREEN proof).
"""
import hashlib
import json
import subprocess
import sys
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
EXTRACTOR = REPO_ROOT / "scripts" / "extract_public_api.py"
LIB_RS = REPO_ROOT / "crates" / "labcolors-core" / "src" / "lib.rs"


def run_extractor(mode: str, stdin_data: bytes | None = None) -> subprocess.CompletedProcess:
    cmd = [sys.executable, str(EXTRACTOR), mode]
    return subprocess.run(
        cmd,
        capture_output=True,
        input=stdin_data,
        cwd=str(REPO_ROOT),
    )


class EXT02PublicAPITests(unittest.TestCase):
    """RED→GREEN proof + sabotage controls for EXT-02."""

    def test_extract_produces_valid_manifest_schema(self):
        """GREEN: extractor produces manifest with required fields."""
        result = run_extractor("extract")
        self.assertEqual(result.returncode, 0, f"extractor failed: {result.stderr.decode()}")
        manifest = json.loads(result.stdout)
        self.assertEqual(manifest["class"], "public-api")
        self.assertEqual(manifest["schema_version"], 1)
        self.assertEqual(manifest["crate"], "labcolors-core")
        self.assertIn("symbols", manifest)
        self.assertIn("manifest_sha256", manifest)
        self.assertGreater(manifest["symbol_count"], 0)
        for sym in manifest["symbols"]:
            self.assertIn("kind", sym)
            self.assertIn("name", sym)
            self.assertIn("file", sym)
            self.assertIn(sym["kind"], ("fn", "struct", "enum", "const", "type", "trait", "mod", "use"))

    def test_verify_passes_on_current_tree(self):
        """GREEN: verify mode passes on live production sources."""
        extract = run_extractor("extract")
        self.assertEqual(extract.returncode, 0)
        verify = run_extractor("verify", stdin_data=extract.stdout)
        self.assertEqual(verify.returncode, 0, f"verify failed: {verify.stderr.decode()}")
        self.assertIn(b"EXT-02 VERIFY OK", verify.stderr)

    def test_sabotage_missing_lib_rs_detected(self):
        """RED: extractor fails when crate root lib.rs is absent."""
        backup = LIB_RS.read_bytes()
        try:
            LIB_RS.unlink()
            result = run_extractor("extract")
            self.assertNotEqual(result.returncode, 0, "extractor should fail without lib.rs")
            self.assertIn(b"SABOTAGE", result.stderr)
        finally:
            LIB_RS.write_bytes(backup)

    def test_sabotage_tampered_manifest_hash_detected(self):
        """RED: verify fails when manifest_sha256 is tampered."""
        extract = run_extractor("extract")
        manifest = json.loads(extract.stdout)
        manifest["manifest_sha256"] = "0" * 64
        tampered = json.dumps(manifest, sort_keys=True).encode()
        verify = run_extractor("verify", stdin_data=tampered)
        self.assertNotEqual(verify.returncode, 0, "verify should fail on tampered hash")
        self.assertIn(b"mismatch", verify.stderr)

    def test_sabotage_symbol_count_mismatch_detected(self):
        """RED: verify fails when symbol_count doesn't match actual symbols."""
        extract = run_extractor("extract")
        manifest = json.loads(extract.stdout)
        manifest["symbol_count"] = 999999
        # Recompute hash so it's not caught by hash check first
        payload = {k: v for k, v in manifest.items() if k != "manifest_sha256"}
        canonical = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
        manifest["manifest_sha256"] = hashlib.sha256(canonical).hexdigest()
        tampered = json.dumps(manifest, sort_keys=True).encode()
        verify = run_extractor("verify", stdin_data=tampered)
        self.assertNotEqual(verify.returncode, 0, "verify should fail on count mismatch")
        self.assertIn(b"symbol_count mismatch", verify.stderr)

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