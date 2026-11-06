"""Standalone control regression using the actual reviewed projection, not a solver substitute."""
import gzip
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('standalone', Path(__file__).with_name('formal-b-run.py'))
assert spec is not None and spec.loader is not None
standalone = importlib.util.module_from_spec(spec)
spec.loader.exec_module(standalone)


class StandaloneTests(unittest.TestCase):
    def test_single_projected_capture_and_exception_retention(self):
        frozen = Path(__file__).resolve().parents[1] / standalone.adaptive.PROOF
        actual_run = subprocess.run
        for outcome in ('pass', 'timeout', 'semantic', 'refused', 'capture_error', 'bad_json'):
            with self.subTest(outcome=outcome), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                proof = root / 'source' / standalone.adaptive.PROOF
                proof.parent.mkdir(parents=True)
                proof.write_bytes(frozen.read_bytes())
                output = root / 'output'
                output.touch()
                calls = []

                def run(command, *, env=None, check):
                    self.assertTrue(check)
                    if env is None:
                        calls.append('patch-check' if '--check' in command else 'patch')
                        # Fixture bytes must match the hosted Linux checkout even on Windows.
                        return actual_run([command[0], '-c', 'core.autocrlf=false', *command[1:]], check=check)
                    calls.append(env['FORMAL_ARM'])
                    self.assertEqual(env['FORMAL_ARM'], 'B')
                    self.assertEqual(env['FORMAL_PRIOR_BYTES'], '0')
                    self.assertEqual(env['FORMAL_EXPECTED_DIFF'], standalone.adaptive.PROOF)
                    self.assertEqual(standalone.adaptive.digest(proof), standalone.adaptive.VARIANT_HASH)
                    folder = root / 'artifact' / 'B'
                    folder.mkdir(parents=True)
                    complete = outcome != 'refused'
                    result = 'canonical_validation_passed' if outcome == 'pass' else 'canonical_validation_rejected'
                    manifest = {'complete': complete, 'proof_result': result}
                    for name in ('proof-stdout.gz', 'proof-json.gz'):
                        with gzip.open(folder / name, 'wb') as stream:
                            stream.write(b'fixture corpus' if complete else b'partial')
                    manifest['gzip_sha256'] = standalone.adaptive.digest(folder / 'proof-stdout.gz')
                    manifest['json_gzip_sha256'] = standalone.adaptive.digest(folder / 'proof-json.gz')
                    (folder / 'manifest.json').write_text('{' if outcome == 'bad_json' else json.dumps(manifest))
                    if outcome == 'capture_error':
                        raise subprocess.CalledProcessError(1, command)

                environment = {'DIAGNOSTIC_ROOT': str(root), 'FORMAL_SOURCE_SHA': 'frozen',
                               'WORKFLOW_SHA': 'workflow', 'GITHUB_RUN_ID': 'fixture',
                               'GITHUB_OUTPUT': str(output)}
                with patch.dict(os.environ, environment), patch.object(standalone.subprocess, 'run', side_effect=run):
                    if outcome in ('capture_error', 'bad_json'):
                        with self.assertRaises((subprocess.CalledProcessError, ValueError)):
                            standalone.main()
                    else:
                        standalone.main()
                self.assertEqual(calls, ['patch-check', 'patch', 'B'])
                self.assertFalse((root / 'artifact' / 'A').exists())
                self.assertEqual(output.read_text(), 'artifact_ready=true\n')
                decision = json.loads((root / 'artifact' / 'decision.json').read_text())
                self.assertEqual(decision['protocol'], 'standalone-B-feasibility')
                self.assertEqual(decision['A'], 'not_run')
                folder = root / 'artifact' / 'B'
                retained = outcome not in ('refused', 'bad_json')
                self.assertEqual((folder / 'proof-stdout.gz').exists(), retained)
                self.assertEqual((folder / 'proof-json.gz').exists(), retained)
                if outcome in ('timeout', 'semantic'):
                    self.assertEqual(decision['B'], 'canonical_validation_rejected')


if __name__ == '__main__':
    unittest.main()
