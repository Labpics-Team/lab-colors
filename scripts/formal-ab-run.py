"""Isolated adaptive experiment; never a canonical gate or source correction."""
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

HARNESS = 'program::attachment::handoff::proofs::stale_prepared_command_never_reaches_the_host_revoke_all'
PROOF = 'crates/labcolors-core/src/program/attachment/handoff/proofs.rs'
BASE_HASH = 'e2f22e79b1decca708d1ce2639bf6d224a260a37947de5dc7b8c9307ee81aa89'
VARIANT_HASH = '279d7f2aea0c7a5b47e79e46ca34f36894f48f0ebd82e136346cc287b30e5ce2'
LIMIT = 16 * 1024 * 1024


def bounded_timeout(report, code):
    """Pinned Kani exports the child timeout separately from property failures."""
    verification = report['verification_results']
    summary = verification['summary']
    results = verification['results']
    errors = report.get('error_details', [])
    return (
        code == 1
        and report['metadata']['kani_version'] == '0.68.0'
        and report['metadata']['target'] == 'x86_64-unknown-linux-gnu'
        and summary['status'] == 'completed'
        and all(summary[k] == 1 for k in ('total_harnesses', 'executed', 'failed'))
        and summary['successful'] == 0
        and len(results) == 1
        and results[0]['harness_id'] == HARNESS
        and results[0]['status'] == 'Failure'
        and results[0]['checks'] == []
        and results[0]['duration_ms'] >= 300000
        and len(errors) == 1
        and errors[0]['harness_id'] == HARNESS
        and errors[0]['has_errors'] is True
        and errors[0]['exit_status'] == 'timeout'
        and errors[0]['failed_properties_type'] == 'None'
    )


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def publish_closed_evidence(artifact, decision):
    """Retain closed receipts even if an arm or classifier raises; discard partial corpora."""
    artifact.mkdir(exist_ok=True)
    for arm in ('A', 'B'):
        folder = artifact / arm
        if not folder.exists():
            continue
        path = folder / 'manifest.json'
        try:
            if path.stat().st_size > 16384:
                raise ValueError('arm manifest exceeds bound')
            manifest = json.loads(path.read_text())
            if manifest['complete']:
                for name, key in (('proof-stdout.gz', 'gzip_sha256'), ('proof-json.gz', 'json_gzip_sha256')):
                    if (folder / name).stat().st_size > LIMIT - 96 * 1024:
                        raise ValueError('closed corpus exceeds bound')
                    if digest(folder / name) != manifest[key]:
                        raise ValueError('closed corpus hash mismatch')
            else:
                for name in ('proof-stdout.gz', 'proof-json.gz'):
                    (folder / name).unlink(missing_ok=True)
        except (OSError, ValueError, KeyError, TypeError) as error:
            for name in ('proof-stdout.gz', 'proof-json.gz'):
                (folder / name).unlink(missing_ok=True)
            manifest = {'diagnostic_only': True, 'arm': arm, 'status': 'refused', 'complete': False,
                        'proof_result': 'unvalidated', 'partial_corpus_discarded': True,
                        'reason': 'Arm receipt refused: ' + repr(error)}
            path.write_text(json.dumps(manifest, sort_keys=True) + '\n')
        if set(p.name for p in folder.iterdir()) - {'manifest.json', 'proof-stdout.gz', 'proof-json.gz'}:
            raise RuntimeError('unexpected artifact file; refusing upload')
    payload = (json.dumps(decision, indent=2, sort_keys=True) + '\n').encode()
    if len(payload) > 16384:
        raise RuntimeError('decision manifest exceeds bound')
    (artifact / 'decision.json').write_bytes(payload)
    total = sum(p.stat().st_size for p in artifact.rglob('*') if p.is_file())
    if total > LIMIT - 64 * 1024:
        raise RuntimeError('aggregate A/B artifact exceeds reserved 16MiB envelope')
    with Path(os.environ['GITHUB_OUTPUT']).open('a') as output:
        output.write('artifact_ready=true\n')
    print(json.dumps(decision, sort_keys=True), flush=True)


def main():
    root = Path(os.environ['DIAGNOSTIC_ROOT'])
    source = root / 'source'
    artifact = root / 'artifact'
    scripts = Path(__file__).resolve().parent
    proof = source / PROOF
    if digest(proof) != BASE_HASH:
        raise RuntimeError('baseline proof bytes differ from frozen source')
    env = dict(os.environ, FORMAL_OWNER_ADAPTER=str(scripts / 'formal-ab-owner.py'))

    def capture(arm, prior=0):
        arm_env = dict(env, FORMAL_ARM=arm, FORMAL_PRIOR_BYTES=str(prior),
                       FORMAL_EXPECTED_DIFF=PROOF if arm == 'B' else '')
        subprocess.run([sys.executable, str(scripts / 'formal-ab-capture.py')], env=arm_env, check=True)
        return json.loads((artifact / arm / 'manifest.json').read_text())

    decision = {'diagnostic_only': True, 'baseline_source_sha': os.environ['FORMAL_SOURCE_SHA'],
                'workflow_sha': os.environ['WORKFLOW_SHA'], 'run_id': os.environ['GITHUB_RUN_ID'],
                'A': 'unvalidated', 'B': 'not_run', 'reason': 'experiment did not complete'}
    try:
        baseline = capture('A')
        decision['A'] = baseline['proof_result']
        decision['reason'] = 'A not an eligible normal harness timeout'
        if baseline['complete'] and baseline['proof_result'] == 'canonical_validation_passed':
            decision['reason'] = 'A canonical PASS; projection unnecessary'
        elif baseline['complete']:
            with gzip.open(artifact / 'A' / baseline['json_file'], 'rt') as stream:
                report = json.load(stream)
            try:
                eligible = bounded_timeout(report, baseline['command_exit_code'])
            except (KeyError, TypeError, IndexError) as error:
                eligible = False
                decision['reason'] = 'A timeout schema refused: ' + repr(error)
            if eligible:
                decision['reason'] = 'A normal bounded harness timeout; B on same VM'
                patch = scripts / 'formal-ab-projection.patch'
                decision['projection_patch_sha256'] = digest(patch)
                subprocess.run(['git', '-C', str(source), 'apply', '--check', str(patch)], check=True)
                subprocess.run(['git', '-C', str(source), 'apply', str(patch)], check=True)
                if digest(proof) != VARIANT_HASH:
                    raise RuntimeError('applied projection differs from reviewed variant')
                prior = sum(p.stat().st_size for p in artifact.rglob('*') if p.is_file())
                variant = capture('B', prior)
                decision['B'] = variant['proof_result']
                decision['B_complete'] = variant['complete']
                decision['variant_sha256'] = digest(proof)
    except Exception as error:
        decision['reason'] = 'Experiment refused: ' + repr(error)
        raise
    finally:
        publish_closed_evidence(artifact, decision)



if __name__ == '__main__':
    main()
