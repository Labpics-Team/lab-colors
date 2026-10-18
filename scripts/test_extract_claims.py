#!/usr/bin/env python3
"""EXT-06 sabotage controls and RED→GREEN proof tests.

Verifies that the claims extractor:
1. Produces a valid manifest with correct schema.
2. Fails when manifest_sha256 is tampered (integrity sabotage).
3. Fails when claim_count mismatches actual claims (consistency sabotage).
4. Passes on the current production tree (GREEN proof).
"""
import hashlib
import io
import json
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stderr
from unittest.mock import patch
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
EXTRACTOR = REPO_ROOT / "scripts" / "extract_claims.py"


def run_extractor(mode: str, stdin_data: bytes | None = None) -> subprocess.CompletedProcess:
    cmd = [sys.executable, str(EXTRACTOR), mode]
    return subprocess.run(
        cmd,
        capture_output=True,
        input=stdin_data,
        cwd=str(REPO_ROOT),
    )


def reseal(manifest: dict) -> bytes:
    """Forge a valid self-hash to exercise the verifier's source comparison."""
    payload = {k: v for k, v in manifest.items() if k != "manifest_sha256"}
    canonical = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
    manifest["manifest_sha256"] = hashlib.sha256(canonical).hexdigest()
    return json.dumps(manifest, sort_keys=True).encode()


class EXT06ClaimsTests(unittest.TestCase):
    """RED→GREEN proof + sabotage controls for EXT-06."""

    def test_extract_produces_valid_manifest_schema(self):
        """GREEN: extractor produces manifest with required fields."""
        result = run_extractor("extract")
        self.assertEqual(result.returncode, 0, f"extractor failed: {result.stderr.decode()}")
        manifest = json.loads(result.stdout)
        self.assertEqual(manifest["class"], "claims")
        self.assertEqual(manifest["schema_version"], 1)
        self.assertIn("claims", manifest)
        self.assertIn("manifest_sha256", manifest)
        self.assertGreater(manifest["claim_count"], 0)
        for claim in manifest["claims"]:
            self.assertIn("path", claim)
            self.assertIn("line", claim)
            self.assertIn("kind", claim)
            self.assertIn("expression", claim)
            self.assertIn("source_sha256", claim)
            self.assertIn(claim["kind"], (
                "production-invariant",
                "compile-time-invariant",
                "test-contract",
                "doc-contract",
            ))

    def test_verify_passes_on_current_tree(self):
        """GREEN: verify mode passes on live production sources."""
        extract = run_extractor("extract")
        self.assertEqual(extract.returncode, 0)
        verify = run_extractor("verify", stdin_data=extract.stdout)
        self.assertEqual(verify.returncode, 0, f"verify failed: {verify.stderr.decode()}")
        self.assertIn(b"EXT-06 VERIFY OK", verify.stderr)

    def test_sabotage_tampered_manifest_hash_detected(self):
        """RED: verify fails when manifest_sha256 is tampered."""
        extract = run_extractor("extract")
        manifest = json.loads(extract.stdout)
        manifest["manifest_sha256"] = "0" * 64
        tampered = json.dumps(manifest, sort_keys=True).encode()
        verify = run_extractor("verify", stdin_data=tampered)
        self.assertNotEqual(verify.returncode, 0, "verify should fail on tampered hash")
        self.assertIn(b"mismatch", verify.stderr)

    def test_sabotage_claim_count_mismatch_detected(self):
        """RED: verify fails when claim_count doesn't match actual claims."""
        extract = run_extractor("extract")
        manifest = json.loads(extract.stdout)
        manifest["claim_count"] = 999999
        payload = {k: v for k, v in manifest.items() if k != "manifest_sha256"}
        canonical = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
        manifest["manifest_sha256"] = hashlib.sha256(canonical).hexdigest()
        tampered = json.dumps(manifest, sort_keys=True).encode()
        verify = run_extractor("verify", stdin_data=tampered)
        self.assertNotEqual(verify.returncode, 0, "verify should fail on count mismatch")
        self.assertIn(b"claim_count mismatch", verify.stderr)

    def test_resealed_omission_is_rejected_against_sources(self):
        """Dropping a real claim with a corrected count and hash must fail."""
        extract = run_extractor("extract")
        self.assertEqual(extract.returncode, 0, extract.stderr.decode())
        manifest = json.loads(extract.stdout)
        self.assertGreater(manifest["claim_count"], 1)
        manifest["claims"].pop(0)
        manifest["claim_count"] = len(manifest["claims"])
        verify = run_extractor("verify", stdin_data=reseal(manifest))
        self.assertNotEqual(verify.returncode, 0)
        self.assertIn(b"differs from current source claims", verify.stderr)

    def test_resealed_source_digest_is_rejected_against_sources(self):
        """A consistent record cannot substitute a source file's digest."""
        extract = run_extractor("extract")
        self.assertEqual(extract.returncode, 0, extract.stderr.decode())
        manifest = json.loads(extract.stdout)
        self.assertTrue(manifest["claims"])
        manifest["claims"][0]["source_sha256"] = "0" * 64
        verify = run_extractor("verify", stdin_data=reseal(manifest))
        self.assertNotEqual(verify.returncode, 0)
        self.assertIn(b"differs from current source claims", verify.stderr)

    def test_missing_one_source_root_is_not_silently_skipped(self):
        """One present root does not make absent siblings optional."""
        import extract_claims as extractor
        self.assertTrue(extractor.SRC_ROOTS[0].is_dir())
        missing = REPO_ROOT / "crates" / "nonexistent-claim-root" / "src"
        with patch.object(extractor, "SRC_ROOTS", [extractor.SRC_ROOTS[0], missing]):
            with self.assertRaisesRegex(FileNotFoundError, "source root is missing"):
                extractor.extract_claims()

    def test_nested_source_symlink_is_rejected(self):
        """A source symlink cannot bypass the inventory or escape its scope."""
        import extract_claims as extractor
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / "target"
            target.mkdir()
            (target / "claim.rs").write_text("fn claim() {}\n", encoding="utf-8")
            (root / "alias").symlink_to(target, target_is_directory=True)
            with patch.object(extractor, "SRC_ROOTS", [root]):
                with self.assertRaisesRegex(ValueError, "symlink in source tree"):
                    extractor.extract_claims()

    def test_inaccessible_nested_tree_fails_instead_of_certifying_partial_claims(self):
        """A scandir error must reach both the extractor and its public CLI."""
        import extract_claims as extractor
        import source_tree

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "sources"
            root.mkdir()
            nested = root / "hidden"
            nested.mkdir()
            (root / "a.rs").write_text("assert!(true);\n", encoding="utf-8")
            (nested / "b.rs").write_text("assert!(false);\n", encoding="utf-8")

            with patch.object(extractor, "SRC_ROOTS", [root]), patch.object(
                extractor, "REPO_ROOT", root.parent
            ):
                # Positive control: the same complete tree is admitted.
                full = extractor.build_manifest(extractor.extract_claims())
                self.assertEqual(full["claim_count"], 2)
                extractor.verify_manifest(full)

                original_scandir = source_tree.os.scandir

                def deny_nested(path):
                    if Path(path) == nested:
                        raise PermissionError("injected nested directory refusal")
                    return original_scandir(path)

                with patch("source_tree.os.scandir", side_effect=deny_nested):
                    with self.assertRaisesRegex(PermissionError, "injected nested"):
                        extractor.extract_claims()
                    with self.assertRaisesRegex(PermissionError, "injected nested"):
                        extractor.verify_manifest(full)

                    # Exercise the entrypoint, not only its internal helper.
                    input_stream = io.TextIOWrapper(
                        io.BytesIO(json.dumps(full).encode()), encoding="utf-8"
                    )
                    error_output = io.StringIO()
                    with patch.object(extractor.sys, "argv", ["extract_claims.py", "verify"]), patch.object(
                        extractor.sys, "stdin", input_stream
                    ), redirect_stderr(error_output):
                        with self.assertRaises(SystemExit) as exit_status:
                            extractor.main()
                    self.assertEqual(exit_status.exception.code, 1)
                    self.assertIn("unable to enumerate claims", error_output.getvalue())
                    self.assertNotIn("EXT-06 VERIFY OK", error_output.getvalue())

    def test_invalid_json_is_manifest_input_error(self):
        """Corrupt input is distinct from failure to enumerate sources."""
        verify = run_extractor("verify", stdin_data=b"{invalid json")
        self.assertNotEqual(verify.returncode, 0)
        self.assertIn(b"invalid manifest input", verify.stderr)

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