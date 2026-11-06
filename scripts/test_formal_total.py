"""Control tests exercise actual patch application and closed artifact publication."""
import gzip
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('total', Path(__file__).with_name('formal-total-run.py'))
assert spec is not None and spec.loader is not None
total = importlib.util.module_from_spec(spec)
spec.loader.exec_module(total)


class TotalTests(unittest.TestCase):
    def test_sequential_capture_binding_and_second_failure_retention(self):
        actual_run = subprocess.run
        repository = Path(__file__).resolve().parents[1]
        base = actual_run(['git', '-C', str(repository), 'show', total.BASE_SHA + ':' + total.adaptive.PROOF],
                          check=True, capture_output=True).stdout
        for outcome in ('pass', 'first_rejected', 'second_rejected', 'refused', 'capture_error', 'bad_json', 'wrong_binding'):
            with self.subTest(outcome=outcome), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                proof = root / 'source' / total.adaptive.PROOF
                proof.parent.mkdir(parents=True)
                proof.write_bytes(base)
                output = root / 'output'
                output.touch()
                captures = []
                first_hashes = {}

                def run(command, *, env=None, check):
                    if env is None:
                        return actual_run([command[0], '-c', 'core.autocrlf=false', *command[1:]], check=check)
                    arm = env['FORMAL_ARM']
                    captures.append(arm)
                    self.assertEqual(env['FORMAL_HARNESS'], dict(total.HARNESSES)[arm])
                    self.assertEqual(total.adaptive.digest(proof), total.VARIANT_HASH)
                    prior = sum(p.stat().st_size for p in (root / 'artifact').rglob('*') if p.is_file())
                    self.assertEqual(int(env['FORMAL_PRIOR_BYTES']), prior)
                    folder = root / 'artifact' / arm
                    folder.mkdir(parents=True)
                    complete = not (arm == 'B' and outcome == 'refused')
                    rejected = (arm == 'A' and outcome == 'first_rejected') or (arm == 'B' and outcome == 'second_rejected')
                    manifest = {'complete': complete,
                                'source_variant_sha256': 'wrong' if outcome == 'wrong_binding' and arm == 'B' else total.VARIANT_HASH,
                                'command': ['adapter', env['FORMAL_HARNESS']],
                                'proof_result': 'canonical_validation_rejected' if rejected else 'canonical_validation_passed'}
                    for name, key in (('proof-stdout.gz', 'gzip_sha256'), ('proof-json.gz', 'json_gzip_sha256')):
                        with gzip.open(folder / name, 'wb') as stream:
                            stream.write(b'closed corpus' if complete else b'partial')
                        manifest[key] = total.adaptive.digest(folder / name)
                        if arm == 'A':
                            first_hashes[name] = manifest[key]
                    (folder / 'manifest.json').write_text('{' if arm == 'B' and outcome == 'bad_json' else json.dumps(manifest))
                    if arm == 'B' and outcome == 'capture_error':
                        raise subprocess.CalledProcessError(1, command)

                env = {'DIAGNOSTIC_ROOT': str(root), 'FORMAL_SOURCE_SHA': total.BASE_SHA,
                       'WORKFLOW_SHA': 'fixture', 'GITHUB_RUN_ID': 'fixture', 'GITHUB_OUTPUT': str(output)}
                with patch.dict(os.environ, env), patch.object(total.subprocess, 'run', side_effect=run):
                    if outcome in ('capture_error', 'bad_json', 'wrong_binding'):
                        with self.assertRaises((subprocess.CalledProcessError, ValueError, RuntimeError)):
                            total.main()
                    else:
                        total.main()
                self.assertEqual(captures, ['A'] if outcome == 'first_rejected' else ['A', 'B'])
                self.assertEqual(output.read_text(), 'artifact_ready=true\n')
                for name, digest in first_hashes.items():
                    self.assertEqual(total.adaptive.digest(root / 'artifact' / 'A' / name), digest)
                if outcome in ('refused', 'bad_json'):
                    self.assertFalse((root / 'artifact' / 'B' / 'proof-stdout.gz').exists())
                self.assertLess(sum(p.stat().st_size for p in (root / 'artifact').rglob('*') if p.is_file()), total.adaptive.LIMIT)


if __name__ == '__main__':
    unittest.main()
