"""Sequential candidate feasibility only; not baseline A/B or a mandatory gate."""
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
BASE_SHA = '7cca74e85cd125c53cdf26a15cd8d4e0f66d645d'
BASE_HASH = '279d7f2aea0c7a5b47e79e46ca34f36894f48f0ebd82e136346cc287b30e5ce2'
VARIANT_HASH = 'e965e58a039bb7948ce3163f50412dcb91d8a4b278267aa41a6728d719c2ee58'
HARNESSES = (
    ('A', 'program::attachment::handoff::proofs::rejected_install_is_atomic_retryable_and_success_is_once_only_set_all'),
    ('B', 'program::attachment::handoff::proofs::rejected_install_is_atomic_retryable_and_success_is_once_only_revoke_all'),
)


def main():
    root = Path(os.environ['DIAGNOSTIC_ROOT'])
    source = root / 'source'
    artifact = root / 'artifact'
    proof = source / adaptive.PROOF
    if os.environ['FORMAL_SOURCE_SHA'] != BASE_SHA or adaptive.digest(proof) != BASE_HASH:
        raise RuntimeError('source is not the exact integrated baseline')
    projection = scripts / 'formal-total-projection.patch'
    decision = {
        'diagnostic_only': True, 'protocol': 'sequential-total-paint-candidate',
        'baseline_source_sha': BASE_SHA, 'variant_sha256': VARIANT_HASH,
        'workflow_sha': os.environ['WORKFLOW_SHA'], 'run_id': os.environ['GITHUB_RUN_ID'],
        'projection_patch_sha256': adaptive.digest(projection),
        'harnesses': dict(HARNESSES), 'A': 'not_run', 'B': 'not_run',
        'package_capture_seconds': 1200, 'per_capture_seconds': 600,
        'per_native_harness_seconds': 300, 'shared_artifact_bytes': adaptive.LIMIT,
    }
    try:
        subprocess.run(['git', '-C', str(source), 'apply', '--check', str(projection)], check=True)
        subprocess.run(['git', '-C', str(source), 'apply', str(projection)], check=True)
        if adaptive.digest(proof) != VARIANT_HASH:
            raise RuntimeError('candidate differs from reviewed bytes')
        for arm, harness in HARNESSES:
            prior = sum(p.stat().st_size for p in artifact.rglob('*') if p.is_file())
            env = dict(os.environ, FORMAL_OWNER_ADAPTER=str(scripts / 'formal-ab-owner.py'),
                       FORMAL_ARM=arm, FORMAL_HARNESS=harness, FORMAL_PRIOR_BYTES=str(prior),
                       FORMAL_EXPECTED_DIFF=adaptive.PROOF)
            subprocess.run([sys.executable, str(scripts / 'formal-ab-capture.py')], env=env, check=True)
            manifest = json.loads((artifact / arm / 'manifest.json').read_text())
            if (manifest['source_variant_sha256'] != VARIANT_HASH
                    or manifest['command'][-1] != harness):
                raise RuntimeError('capture binding differs from exact candidate/harness')
            decision[arm] = manifest['proof_result']
            decision[arm + '_complete'] = manifest['complete']
            if not manifest['complete'] or manifest['proof_result'] != 'canonical_validation_passed':
                decision['stopped_after'] = arm
                break
    except Exception as error:
        decision['refusal'] = repr(error)
        raise
    finally:
        adaptive.publish_closed_evidence(artifact, decision)


if __name__ == '__main__':
    main()
