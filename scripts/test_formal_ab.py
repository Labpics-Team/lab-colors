"""Deterministic experiment control tests; no proof result is manufactured as evidence."""
import copy
import gzip
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('ab', Path(__file__).with_name('formal-ab-run.py'))
assert spec is not None and spec.loader is not None
ab = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ab)


def timeout_report():
    return {'metadata': {'kani_version': '0.68.0', 'target': 'x86_64-unknown-linux-gnu'},
            'verification_results': {'summary': {'status': 'completed', 'total_harnesses': 1,
                'executed': 1, 'failed': 1, 'successful': 0},
                'results': [{'harness_id': ab.HARNESS, 'status': 'Failure', 'checks': [], 'duration_ms': 300001}]},
            'error_details': [{'harness_id': ab.HARNESS, 'has_errors': True,
                'exit_status': 'timeout', 'failed_properties_type': 'None'}]}


class AdaptiveTests(unittest.TestCase):
    def test_native_timeout_only(self):
        baseline = timeout_report()
        self.assertTrue(ab.bounded_timeout(baseline, 1))
        for code in (0, -9, 125, 137):
            self.assertFalse(ab.bounded_timeout(baseline, code))
        changes = [copy.deepcopy(baseline) for _ in range(6)]
        changes[0]['error_details'][0]['exit_status'] = 'out_of_memory'
        changes[1]['error_details'][0]['harness_id'] = 'other'
        changes[2]['verification_results']['results'][0]['duration_ms'] = 299999
        changes[3]['verification_results']['results'][0]['checks'] = [{'status': 'Failure'}]
        changes[4]['metadata']['kani_version'] = '0.67.0'
        changes[5]['verification_results']['summary']['executed'] = 2
        for changed in changes:
            self.assertFalse(ab.bounded_timeout(changed, 1), changed)

    def run_decision(self, kind):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            proof = root / 'source' / ab.PROOF
            proof.parent.mkdir(parents=True)
            proof.write_bytes(b'baseline')
            output = root / 'output'
            output.touch()
            calls = []
            report = timeout_report()
            if kind == 'semantic':
                report['error_details'][0]['exit_status'] = 'properties_failed'

            def run(command, *, env=None, check):
                if env is None:
                    self.assertEqual(command[:3], ['git', '-C', str(root / 'source')])
                    calls.append('patch-check' if '--check' in command else 'patch')
                    if '--check' not in command:
                        proof.write_bytes(b'variant')
                    return
                arm = env['FORMAL_ARM']
                calls.append(arm)
                if arm == 'B':
                    self.assertEqual(calls, ['A', 'patch-check', 'patch', 'B'])
                    self.assertGreater(int(env['FORMAL_PRIOR_BYTES']), 0)
                    self.assertEqual(env['FORMAL_EXPECTED_DIFF'], ab.PROOF)
                folder = root / 'artifact' / arm
                folder.mkdir(parents=True)
                complete = kind != 'refusal'
                manifest = {'complete': complete, 'proof_result': 'canonical_validation_passed' if kind == 'pass' or arm == 'B' else 'canonical_validation_rejected',
                            'command_exit_code': 0 if kind == 'pass' else 1, 'json_file': 'proof-json.gz'}
                if complete:
                    with gzip.open(folder / 'proof-json.gz', 'wt') as stream:
                        if kind == 'bad_json':
                            stream.write('{')
                        else:
                            json.dump(report, stream)
                    with gzip.open(folder / 'proof-stdout.gz', 'wb') as stream:
                        stream.write(b'native complete stdout')
                    manifest['gzip_sha256'] = ab.digest(folder / 'proof-stdout.gz')
                    manifest['json_gzip_sha256'] = ab.digest(folder / 'proof-json.gz')
                (folder / 'manifest.json').write_text(json.dumps(manifest))
                if kind == 'capture_error' or kind == 'B_error' and arm == 'B':
                    raise ab.subprocess.CalledProcessError(1, command)

            environment = {'DIAGNOSTIC_ROOT': str(root), 'FORMAL_SOURCE_SHA': 'frozen',
                           'WORKFLOW_SHA': 'workflow', 'GITHUB_RUN_ID': 'fixture', 'GITHUB_OUTPUT': str(output)}
            import hashlib
            with patch.dict(os.environ, environment), patch.object(ab, 'BASE_HASH', hashlib.sha256(b'baseline').hexdigest()), patch.object(ab, 'VARIANT_HASH', hashlib.sha256(b'variant').hexdigest()), patch.object(ab.subprocess, 'run', side_effect=run):
                if kind in ('bad_json', 'capture_error', 'B_error'):
                    with self.assertRaises((ValueError, ab.subprocess.CalledProcessError)):
                        ab.main()
                else:
                    ab.main()
            if kind in ('bad_json', 'capture_error', 'B_error'):
                self.assertTrue((root / 'artifact/A/proof-stdout.gz').is_file())
                self.assertTrue((root / 'artifact/A/proof-json.gz').is_file())
            decision = json.loads((root / 'artifact/decision.json').read_text())
            self.assertEqual(output.read_text(), 'artifact_ready=true\n')
            self.assertEqual(calls, ['A', 'patch-check', 'patch', 'B'] if kind in ('timeout', 'B_error') else ['A'])
            self.assertEqual(decision['B'], 'canonical_validation_passed' if kind == 'timeout' else 'not_run')

    def test_refused_or_missing_manifest_discards_partial_only(self):
        for missing in (False, True):
            with self.subTest(missing=missing), tempfile.TemporaryDirectory() as tmp:
                artifact = Path(tmp) / 'artifact'
                folder = artifact / 'B'
                folder.mkdir(parents=True)
                for name in ('proof-stdout.gz', 'proof-json.gz'):
                    (folder / name).write_bytes(b'partial')
                if not missing:
                    (folder / 'manifest.json').write_text(json.dumps({'complete': False}))
                output = Path(tmp) / 'output'
                output.touch()
                with patch.dict(os.environ, {'GITHUB_OUTPUT': str(output)}):
                    ab.publish_closed_evidence(artifact, {'diagnostic_only': True})
                self.assertEqual(sorted(p.name for p in folder.iterdir()), ['manifest.json'])
                self.assertEqual(output.read_text(), 'artifact_ready=true\n')

    def test_adaptive_branches(self):
        for kind in ('pass', 'timeout', 'semantic', 'refusal', 'bad_json', 'capture_error', 'B_error'):
            with self.subTest(kind=kind):
                self.run_decision(kind)


if __name__ == '__main__':
    unittest.main()
