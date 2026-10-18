#!/usr/bin/env python3
"""EXT-09 sabotage controls and RED→GREEN proof tests.

Verifies that the CI/build/release extractor:
1. Produces a valid manifest with correct schema.
2. Fails when a listed file is missing (MISSING sabotage).
3. Fails when a listed file's content drifts (DRIFT sabotage).
4. Fails when an unlisted .yml file appears (UNLISTED sabotage).
5. Passes on the current production tree (GREEN proof).
"""
import hashlib
import json
import subprocess
import sys
import io
import tempfile
import copy
import unittest
from contextlib import redirect_stderr
from unittest.mock import patch
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
EXTRACTOR = REPO_ROOT / "scripts" / "extract_ci_build.py"


def run_extractor(mode: str, stdin_data: bytes | None = None) -> subprocess.CompletedProcess:
    cmd = [sys.executable, str(EXTRACTOR), mode]
    return subprocess.run(
        cmd,
        capture_output=True,
        input=stdin_data,
        cwd=str(REPO_ROOT),
    )


class EXT09CIBuildTests(unittest.TestCase):
    """RED→GREEN proof + sabotage controls for EXT-09."""

    def test_extract_produces_valid_manifest_schema(self):
        """GREEN: extractor produces manifest with required fields."""
        result = run_extractor("extract")
        self.assertEqual(result.returncode, 0, f"extractor failed: {result.stderr.decode()}")
        manifest = json.loads(result.stdout)
        self.assertEqual(manifest["class"], "ci-build-release")
        self.assertEqual(manifest["schema_version"], 1)
        self.assertIn("files", manifest)
        self.assertIn("manifest_sha256", manifest)
        self.assertGreater(manifest["file_count"], 0)
        for entry in manifest["files"]:
            self.assertIn("path", entry)
            self.assertIn("sha256", entry)
            self.assertEqual(len(entry["sha256"]), 64)

    def test_verify_passes_on_current_tree(self):
        """GREEN: verify mode passes on live workflow files."""
        extract = run_extractor("extract")
        self.assertEqual(extract.returncode, 0)
        verify = run_extractor("verify", stdin_data=extract.stdout)
        self.assertEqual(verify.returncode, 0, f"verify failed: {verify.stderr.decode()}")
        self.assertIn(b"EXT-09 VERIFY OK", verify.stderr)

    def test_sabotage_missing_file_detected(self):
        """RED: verify fails when a listed file is absent."""
        extract = run_extractor("extract")
        manifest = json.loads(extract.stdout)
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
        """RED: verify fails when an unlisted .yml file exists."""
        extract = run_extractor("extract")
        phantom = REPO_ROOT / ".github" / "workflows" / "_phantom_sabotage_test.yml"
        try:
            phantom.write_text("# phantom workflow for sabotage test\n")
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




class WorkflowInventoryTests(unittest.TestCase):
    def setUp(self):
        import extract_ci_build as ci
        self.ci = ci
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.workflows = self.root / '.github' / 'workflows'
        self.workflows.mkdir(parents=True)
        for attribute, value in [('REPO_ROOT', self.root), ('WORKFLOWS_ROOT', self.workflows)]:
            patcher = patch.object(ci, attribute, value)
            patcher.start()
            self.addCleanup(patcher.stop)

    @staticmethod
    def reseal(record):
        body = {k: v for k, v in record.items() if k != 'manifest_sha256'}
        record['manifest_sha256'] = hashlib.sha256(
            json.dumps(body, sort_keys=True, separators=(',', ':')).encode()
        ).hexdigest()
        return record

    def reject(self, record, reason=None):
        stream = io.StringIO()
        with redirect_stderr(stream), self.assertRaises((SystemExit, ValueError, OSError)):
            self.ci.verify_manifest(record)
        if reason:
            self.assertIn(reason, stream.getvalue())

    def test_both_workflow_extensions_are_complete_and_nested_data_is_excluded(self):
        for name in ['first.yml', 'second.yaml']:
            (self.workflows / name).write_text('name: Example\n', encoding='utf-8')
        nested = self.workflows / 'samples'
        nested.mkdir()
        (nested / 'ignored.yaml').write_text('fixture\n', encoding='utf-8')
        manifest = self.ci.extract_manifest()
        self.assertEqual([e['path'] for e in manifest['files']],
                         ['.github/workflows/first.yml', '.github/workflows/second.yaml'])
        self.ci.verify_manifest(manifest)
        (self.workflows / 'second.yaml').write_text('name: Changed\n', encoding='utf-8')
        self.reject(manifest, 'DRIFT')

    def test_verify_rejects_resealed_metadata_and_duplicate_records(self):
        (self.workflows / 'one.yml').write_text('name: One\n', encoding='utf-8')
        current = self.ci.extract_manifest()
        self.ci.verify_manifest(current)
        mutations = [
            {'class': 'unrelated'}, {'schema_version': True}, {'root': '../another'},
            {'file_count': 99}, {'extra': 'unknown'},
            {'files': current['files'] * 2, 'file_count': 2},
        ]
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                forged = copy.deepcopy(current)
                forged.update(mutation)
                self.reject(self.reseal(forged))
        forged = copy.deepcopy(current)
        forged['manifest_sha256'] = '0' * 64
        self.reject(forged)

    def test_hidden_file_names_are_not_omitted_from_declared_suffixes(self):
        from source_tree import rust_sources
        names = [".rs", ".yml", ".yaml", ".hidden.yml", ".hidden.yaml", "one.yml", "one.yaml", "backup.yml.bak"]
        for name in names:
            (self.workflows / name).write_text("fixture\n", encoding="utf-8")
        manifest = self.ci.extract_manifest()
        self.assertEqual([entry["path"] for entry in manifest["files"]], [
            ".github/workflows/.hidden.yaml", ".github/workflows/.hidden.yml",
            ".github/workflows/.yaml", ".github/workflows/.yml",
            ".github/workflows/one.yaml", ".github/workflows/one.yml",
        ])
        self.ci.verify_manifest(manifest)
        self.assertEqual(rust_sources(self.workflows), [self.workflows / ".rs"])

    def test_missing_workflow_root_is_not_an_admitted_empty_inventory(self):
        empty = self.ci.extract_manifest()
        self.ci.verify_manifest(empty)
        self.workflows.rmdir()
        self.reject(empty)

    def test_workflow_symlink_is_refused_instead_of_followed(self):
        target = self.root / 'external.yml'
        target.write_text('name: External\n', encoding='utf-8')
        (self.workflows / 'borrowed.yml').symlink_to(target)
        with self.assertRaises((SystemExit, ValueError, OSError)):
            self.ci.extract_manifest()


    def test_record_paths_never_select_files_to_read(self):
        (self.workflows / 'one.yml').write_text('name: One\n', encoding='utf-8')
        record = self.ci.extract_manifest()
        record['files'].append({'path': '../outside.txt', 'sha256': '0' * 64})
        record['file_count'] += 1
        self.reseal(record)
        original = Path.open
        observed = []
        def read(path, *args, **kwargs):
            observed.append(path)
            self.assertTrue(path.is_relative_to(self.workflows))
            return original(path, *args, **kwargs)
        with patch.object(Path, 'open', read):
            self.reject(record, 'MISSING')
        self.assertEqual(observed, [self.workflows / 'one.yml'])

    def test_directory_listing_failure_reaches_the_consumer(self):
        import source_tree
        manifest = self.ci.extract_manifest()
        original = source_tree.os.scandir
        def unavailable(path):
            if Path(path) == self.workflows:
                raise PermissionError('injected inventory failure')
            return original(path)
        with patch('source_tree.os.scandir', side_effect=unavailable):
            self.reject(manifest)

if __name__ == "__main__":
    unittest.main()