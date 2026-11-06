"""Standalone projection feasibility; not an A/B comparison or canonical gate."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

scripts = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('adaptive', scripts / 'formal-ab-run.py')
assert spec is not None and spec.loader is not None
adaptive = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adaptive)


def main():
    root = Path(os.environ['DIAGNOSTIC_ROOT'])
    source = root / 'source'
    proof = source / adaptive.PROOF
    artifact = root / 'artifact'
    if adaptive.digest(proof) != adaptive.BASE_HASH:
        raise RuntimeError('baseline proof bytes differ from frozen source')
    patch = scripts / 'formal-ab-projection.patch'
    decision = {
        'diagnostic_only': True, 'protocol': 'standalone-B-feasibility',
        'baseline_source_sha': os.environ['FORMAL_SOURCE_SHA'],
        'workflow_sha': os.environ['WORKFLOW_SHA'], 'run_id': os.environ['GITHUB_RUN_ID'],
        'A': 'not_run', 'B': 'unvalidated',
        'projection_patch_sha256': adaptive.digest(patch),
    }
    try:
        subprocess.run(['git', '-C', str(source), 'apply', '--check', str(patch)], check=True)
        subprocess.run(['git', '-C', str(source), 'apply', str(patch)], check=True)
        if adaptive.digest(proof) != adaptive.VARIANT_HASH:
            raise RuntimeError('applied projection differs from reviewed variant')
        decision['variant_sha256'] = adaptive.digest(proof)
        env = dict(os.environ, FORMAL_OWNER_ADAPTER=str(scripts / 'formal-ab-owner.py'),
                   FORMAL_ARM='B', FORMAL_PRIOR_BYTES='0', FORMAL_EXPECTED_DIFF=adaptive.PROOF)
        subprocess.run([sys.executable, str(scripts / 'formal-ab-capture.py')], env=env, check=True)
        manifest = json.loads((artifact / 'B' / 'manifest.json').read_text())
        decision['B'] = manifest['proof_result']
        decision['B_complete'] = manifest['complete']
    except Exception as error:
        decision['refusal'] = repr(error)
        raise
    finally:
        adaptive.publish_closed_evidence(artifact, decision)


if __name__ == '__main__':
    main()
