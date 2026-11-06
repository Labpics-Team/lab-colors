import gzip
import hashlib
import json
import os
from pathlib import Path
import selectors
import shlex
import signal
import subprocess
import sys
import time

root = Path(os.environ['DIAGNOSTIC_ROOT'])
source = root / 'source'
artifact = root / 'artifact' / os.environ['FORMAL_ARM']
artifact.mkdir(parents=True)
stdout_path = artifact / 'proof-stdout.gz'
json_path = root / (os.environ['FORMAL_ARM'] + '-proof.json')
json_gzip_path = artifact / 'proof-json.gz'
artifact_limit = 16 * 1024 * 1024
# Both compressed corpora share this allowance, including gzip footers.
# Reserve room for the manifest and the upload-artifact ZIP envelope.
metadata_child = None
phase_log_limit = 64 * 1024
phase_log_bytes = 0
# Preserve the original compression allowance; removing observations
# does not expand the artifact budget.
gzip_limit = artifact_limit - 96 * 1024
json_limit = artifact_limit
manifest_limit = 16 * 1024
deadline = time.monotonic() + 600
compressed_size = int(os.environ.get('FORMAL_PRIOR_BYTES', '0'))
harness = 'program::attachment::handoff::proofs::stale_prepared_command_never_reaches_the_host_revoke_all'
command = [sys.executable, os.environ['FORMAL_OWNER_ADAPTER'], str(source), str(json_path), harness]
manifest = {
    'diagnostic_only': True,
    'representation': 'canonical isolated exact proof JSON and regular stdout',
    'proof_result': 'unvalidated',
    'failed_required_run': os.environ['FORMAL_FAILED_RUN'],
    'source_repository': 'https://github.com/Labpics-Team/lab-colors.git',
    'base_sha': os.environ['FORMAL_SOURCE_SHA'],
    'workflow_sha': os.environ['WORKFLOW_SHA'],
    'run_id': os.environ['GITHUB_RUN_ID'],
    'run_attempt': os.environ['GITHUB_RUN_ATTEMPT'],
    'command': command,
    'command_shell': shlex.join(command),
    'command_sha256': hashlib.sha256(json.dumps(command, separators=(',', ':')).encode()).hexdigest(),
    'observation_target': 'complete isolated RevokeAll property results versus original runner cancellation',
    'stdout_capture': 'raw bytes streamed without a raw file; no filtering, no line or sample limit',
    'stderr_capture': 'merged byte-exact native regular output',
    'arm': os.environ['FORMAL_ARM'],
    'source_variant_sha256': hashlib.sha256((source / 'crates/labcolors-core/src/program/attachment/handoff/proofs.rs').read_bytes()).hexdigest(),
    'phase_log_complete': True,
    'phase_log_limit': phase_log_limit,
    'limits': {
        'gzip_bytes': gzip_limit,
        'raw_json_bytes': json_limit,
        'manifest_bytes': manifest_limit,
        'artifact_bytes': artifact_limit,
        'command_seconds': 600,
        'harness_seconds': 300,
    },
    'status': 'refused',
    'complete': False,
}

class Refusal(Exception):
    pass

class BoundedWriter:
    def __init__(self, stream):
        self.stream = stream
        self.size = 0

    def write(self, data):
        global compressed_size
        if compressed_size + len(data) > gzip_limit:
            raise Refusal('compressed JSON and stdout exceed the shared artifact budget')
        written = self.stream.write(data)
        self.size += written
        compressed_size += written
        return written

    def flush(self):
        self.stream.flush()

def remaining():
    seconds = deadline - time.monotonic()
    if seconds <= 0:
        raise Refusal('diagnostic capture exceeded the overall 600 seconds')
    return seconds

def expire_capture(signum, frame):
    raise Refusal('diagnostic capture exceeded the overall 600 seconds')

def phase(name):
    manifest['phase'] = name
    print(f'Diagnostic phase: {name}', flush=True)
    remaining()

def relay_native_phases(chunk):
    global phase_log_bytes
    # Relay every native regular-output byte until the explicit display limit;
    # never select prefixes or alter the independently compressed corpus.
    allowance = max(0, phase_log_limit - phase_log_bytes)
    if allowance:
        shown = chunk[:allowance]
        sys.stdout.buffer.write(shown)
        sys.stdout.buffer.flush()
        phase_log_bytes += len(shown)
    if len(chunk) > allowance and manifest['phase_log_complete']:
        manifest['phase_log_complete'] = False
        print('\nNative display limit reached; complete unfiltered corpus remains bounded in artifact.', flush=True)

def metadata(args):
    global metadata_child
    phase('metadata: ' + shlex.join(args))
    child = subprocess.Popen(args, stdout=subprocess.PIPE, text=True, start_new_session=True)
    metadata_child = child
    assert child.stdout is not None
    try:
        output, _ = child.communicate(timeout=remaining())
        if child.returncode:
            raise subprocess.CalledProcessError(child.returncode, args, output=output)
        return output.strip()
    finally:
        errors = stop_group(child)
        child.stdout.close()
        if errors:
            raise Refusal('metadata process group cleanup failed: ' + '; '.join(errors))
        metadata_child = None

def git(*args):
    return metadata(['git', '-C', str(source), *args])

def stop_group(process):
    errors = []
    if process is None:
        return errors
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    except OSError as error:
        errors.append(str(error))
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        pass
    except Exception as error:
        errors.append(str(error))
    # The leader may have exited while a child ignores TERM. Always
    # kill the group after the bounded grace, including that case.
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    except OSError as error:
        errors.append(str(error))
    try:
        process.wait(timeout=5)
    except Exception as error:
        errors.append(str(error))
    return errors

process = None
raw_size = 0
raw_digest = hashlib.sha256()
signal.signal(signal.SIGALRM, expire_capture)
try:
    signal.setitimer(signal.ITIMER_REAL, remaining())
    manifest['source_sha'] = git('rev-parse', 'HEAD')
    manifest['source_tree'] = git('rev-parse', 'HEAD^{tree}')
    expected_diff = os.environ.get('FORMAL_EXPECTED_DIFF', '')
    expected_status = 'M ' + expected_diff if expected_diff else ''
    if manifest['source_sha'] != manifest['base_sha'] or git('status', '--porcelain') != expected_status:
        raise Refusal('source checkout does not match the frozen or reviewed experimental snapshot')
    manifest['source_file_sha256'] = {
        name: hashlib.sha256((source / name).read_bytes()).hexdigest()
        for name in ['Cargo.lock', '.github/workflows/ci-worker.yml', 'scripts/verify_core_formal.py']
    }
    manifest['rustc_version'] = metadata(['rustc', '--version'])
    manifest['kani_version'] = metadata(['cargo', 'kani', '--version'])
    if manifest['kani_version'].splitlines() != ['Kani Rust Verifier 0.68.0 (cargo plugin)', 'CBMC 6.11.0']:
        raise Refusal('installed Kani version is not the frozen 0.68.0')
    phase('canonical exact proof')
    with stdout_path.open('xb') as output:
        bounded = BoundedWriter(output)
        with gzip.GzipFile(filename='', mode='wb', fileobj=bounded, mtime=0) as compressed:
            process = subprocess.Popen(command, cwd=source, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True)
            assert process.stdout is not None
            with selectors.DefaultSelector() as selector:
                selector.register(process.stdout, selectors.EVENT_READ)
                while True:
                    seconds = remaining()
                    if json_path.exists() and json_path.stat().st_size > json_limit:
                        raise Refusal('raw canonical JSON exceeds its bounded allowance')
                    ready = selector.select(timeout=min(1, seconds))
                    if not any(key.fileobj is process.stdout for key, _ in ready):
                        continue
                    chunk = os.read(process.stdout.fileno(), 64 * 1024)
                    if not chunk:
                        break
                    raw_size += len(chunk)
                    raw_digest.update(chunk)
                    relay_native_phases(chunk)
                    compressed.write(chunk)
            manifest['command_exit_code'] = process.wait(timeout=remaining())
            if manifest['command_exit_code'] not in (0, 1):
                raise Refusal('exact proof command failed outside verification result; corpus is not complete')
    if not raw_size:
        raise Refusal('exact proof command returned empty stdout')
    if (git('diff', '--name-only', manifest['base_sha'], '--') != expected_diff
            or hashlib.sha256((source / 'crates/labcolors-core/src/program/attachment/handoff/proofs.rs').read_bytes()).hexdigest() != manifest['source_variant_sha256']):
        raise Refusal('tracked source bytes changed during collection')
    phase('bounded canonical JSON capture')
    if not json_path.is_file() or not 0 < json_path.stat().st_size <= json_limit:
        raise Refusal('canonical JSON missing, empty, or above its bounded allowance')
    json_digest = hashlib.sha256()
    json_size = 0
    with json_path.open('rb') as raw, json_gzip_path.open('xb') as output:
        with gzip.GzipFile(filename='', mode='wb', fileobj=BoundedWriter(output), mtime=0) as compressed:
            while chunk := raw.read(64 * 1024):
                remaining()
                json_size += len(chunk)
                if json_size > json_limit:
                    raise Refusal('raw canonical JSON exceeds its bounded allowance')
                json_digest.update(chunk)
                compressed.write(chunk)
    phase('canonical contract validation')
    report = json.loads(json_path.read_text(encoding='utf-8'))
    sys.path.insert(0, str(source / 'scripts'))
    import verify_core_formal as validator
    try:
        validator.validate_positive_subset(report, (harness,))
        if manifest['command_exit_code'] != 0:
            raise ValueError('canonical properties passed but command exited nonzero')
    except ValueError as error:
        manifest['proof_result'] = 'canonical_validation_rejected'
        manifest['canonical_validation_error'] = str(error)
    else:
        manifest['proof_result'] = 'canonical_validation_passed'
    remaining()
    manifest.update({
        'json_file': json_gzip_path.name,
        'raw_json_bytes': json_size,
        'raw_json_sha256': json_digest.hexdigest(),
        'json_gzip_bytes': json_gzip_path.stat().st_size,
        'json_gzip_sha256': hashlib.sha256(json_gzip_path.read_bytes()).hexdigest(),
        'status': 'complete',
        'complete': True,
        'stdout_file': stdout_path.name,
        'raw_stdout_bytes': raw_size,
        'raw_stdout_sha256': raw_digest.hexdigest(),
        'gzip_bytes': stdout_path.stat().st_size,
        'gzip_sha256': hashlib.sha256(stdout_path.read_bytes()).hexdigest(),
    })
except Exception as error:
    manifest.update({
        'status': 'refused', 'complete': False,
        'reason': str(error), 'observed_raw_stdout_bytes': raw_size,
        'command_exit_code': None if process is None else process.returncode,
    })
finally:
    # Capture is over; disarm before the existing bounded group cleanup.
    signal.setitimer(signal.ITIMER_REAL, 0)
    cleanup_errors = stop_group(metadata_child)
    if metadata_child is not None:
        metadata_child.stdout.close()
    cleanup_errors.extend(stop_group(process))
    if process is not None and process.stdout is not None:
        try:
            process.stdout.close()
        except OSError as error:
            cleanup_errors.append(str(error))
    if cleanup_errors:
        manifest.update({
            'status': 'refused', 'complete': False,
            'reason': manifest.get('reason', 'command group cleanup failed'),
            'cleanup_errors': cleanup_errors,
        })
    if not manifest['complete']:
        for path in (stdout_path, json_gzip_path):
            try:
                path.unlink(missing_ok=True)
            except OSError as error:
                manifest.setdefault('discard_errors', []).append(str(error))
        manifest['partial_corpus_discarded'] = not stdout_path.exists() and not json_gzip_path.exists()
    json_path.unlink(missing_ok=True)
print('Diagnostic phase: publish bounded artifact', flush=True)
payload = (json.dumps(manifest, indent=2, sort_keys=True) + '\n').encode()
if len(payload) > manifest_limit:
    raise RuntimeError('diagnostic manifest exceeds 16 KiB; refusing upload')
manifest_path = artifact / 'manifest.json'
manifest_path.write_bytes(payload)
# Publish readiness only after both files are closed and bounded.
if manifest_path.read_bytes() != payload:
    raise RuntimeError('diagnostic manifest readback failed; refusing upload')
if manifest['complete']:
    if not stdout_path.is_file() or stdout_path.stat().st_size != manifest['gzip_bytes']:
        raise RuntimeError('complete stdout file readback failed; refusing upload')
    if not json_gzip_path.is_file() or json_gzip_path.stat().st_size != manifest['json_gzip_bytes']:
        raise RuntimeError('complete JSON file readback failed; refusing upload')
    if compressed_size > gzip_limit or compressed_size + len(payload) > artifact_limit:
        raise RuntimeError('closed diagnostic files exceed the artifact budget; refusing upload')
elif stdout_path.exists() or json_gzip_path.exists():
    raise RuntimeError('partial corpus could not be discarded; refusing upload')
# The orchestrator publishes readiness after all selected arms close.
print(f"Diagnostic status: {manifest['status']}; proof result: {manifest['proof_result']}", flush=True)
if not manifest['complete']:
    print(f"Refusal: {manifest['reason']}; no partial stdout artifact retained")
