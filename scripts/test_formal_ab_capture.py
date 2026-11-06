"""Full workflow capture regression. No proof/hosted execution or source mutation."""
import ast
import ctypes
import gzip
import hashlib
import json
import os
from pathlib import Path
import selectors
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import unittest

if sys.platform != 'linux':
    raise unittest.SkipTest('Linux process-group capture regression')

HERE = Path(__file__).resolve().parent
SCRIPTS = Path(os.environ.get('FORMAL_TEST_OWNER', '/owner')) / 'scripts'
HARNESS = 'program::attachment::handoff::proofs::stale_prepared_command_never_reaches_the_host_revoke_all'
SHA = 'fa8409d6e563ab560ccd037b41f5e8b6a7775539'
# Adopt orphaned fixture children so cleanup verification can reap them, not poll zombies.
if ctypes.CDLL(None, use_errno=True).prctl(36, 1, 0, 0, 0):
    raise OSError(ctypes.get_errno(), 'PR_SET_CHILD_SUBREAPER')

FAKE = r'''#!/usr/bin/env python3
import json, os, signal, socket, sys
from pathlib import Path
name = Path(sys.argv[0]).name
case = os.environ['CASE']
if name == 'journalctl':
    Path(os.environ['JOURNAL_PID']).write_text(str(os.getpid()))
    signal.pause()
elif name == 'git':
    if case == 'metadata_cleanup_expiry' and 'rev-parse' in sys.argv:
        child = os.fork()
        if child == 0:
            sys.stdout.close()
            os.close(1)
            signal.signal(signal.SIGTERM, signal.SIG_IGN)
            Path(os.environ['CHILD_PID']).write_text(str(os.getpid()))
            conn = socket.socket(socket.AF_UNIX)
            conn.connect(os.environ['READY_SOCKET'])
            conn.sendall(b'METADATA_CHILD_READY\n')
            conn.close()
            signal.pause()
            os._exit(0)
        Path(os.environ['LEADER_PID']).write_text(str(os.getpid()))
    if 'rev-parse' in sys.argv:
        print(os.environ['FORMAL_SOURCE_SHA'] if sys.argv[-1] == 'HEAD' else 'frozen-tree')
elif name == 'rustc':
    print('rustc 1.96.0')
elif sys.argv[-1] == '--version':
    print('Kani Rust Verifier 0.68.0 (cargo plugin)\nCBMC 6.11.0')
else:
    assert '--harness-timeout' in sys.argv and sys.argv[sys.argv.index('--harness-timeout') + 1] == '300s'
    assert '--exact' in sys.argv and '--jobs=1' in sys.argv
    Path(os.environ['LEADER_PID']).write_text(str(os.getpid()))
    assert '--verbose' in sys.argv
    native = b'[Kani] Running: `goto-instrument input`\nFinished goto-instrument in 1.0s\n[Kani] Running: `cbmc input`\nCBMC version 6.11.0\n'
    os.write(1, native[:8])
    os.write(1, native[8:])
    os.write(1, b'x' * 70000 + b'\n')
    if case == 'phase_limit':
        os.write(1, b'Finished cbmc in 1.0s\n' * 4000)
    if case == 'inherited_pipe':
        reader, writer = os.pipe()
        child = os.fork()
        if child == 0:
            os.close(reader)
            signal.signal(signal.SIGTERM, signal.SIG_IGN)
            os.write(writer, b'R')
            os.close(writer)
            signal.pause()
            os._exit(0)
        os.close(writer)
        assert os.read(reader, 1) == b'R'
        os.close(reader)
        Path(os.environ['CHILD_PID']).write_text(str(child))
        conn = socket.socket(socket.AF_UNIX)
        conn.connect(os.environ['READY_SOCKET'])
        conn.sendall(b'INHERITED_READY\n')
        conn.close()
        # Leader exits; the exact wrapper must still terminate the inherited pipe group.
        print('leader complete, child holds stdout', flush=True)
        sys.exit(0)
    report = json.loads(os.environ['REPORT'])
    if case == 'semantic_reject':
        report['verification_results']['results'][0]['checks'][0]['status'] = 'Failure'
    if case == 'overflow':
        sys.stdout.buffer.write(os.urandom(65536))
    else:
        print('complete exact fixture stdout')
    Path(sys.argv[sys.argv.index('--export-json') + 1]).write_text(json.dumps(report))
    if case == 'semantic_reject':
        sys.exit(1)
'''

# Patch only the external /proc IO boundary, not descendants()/remaining()/cleanup.
# Signal READY before the real blocking syscall. No elapsed-time startup assumptions.
BLOCK_HOOK = r'''
if os.environ['CASE'] == 'metadata_cleanup_expiry':
    import socket
    original_killpg = os.killpg
    injected = False
    def expire_at_metadata_cleanup(pgid, sig):
        global injected
        if not injected and sig == signal.SIGTERM:
            injected = True
            conn = socket.socket(socket.AF_UNIX)
            conn.connect(os.environ['READY_SOCKET'])
            conn.sendall(b'METADATA_CLEANUP_EXPIRY\n')
            conn.close()
            signal.raise_signal(signal.SIGALRM)
        return original_killpg(pgid, sig)
    os.killpg = expire_at_metadata_cleanup
if os.environ['CASE'] in ('blocked_observation', 'blocked_observation_write'):
    import socket
    def block_observation():
        conn = socket.socket(socket.AF_UNIX)
        conn.connect(os.environ['READY_SOCKET'])
        conn.sendall(b'OBSERVATION_BLOCKED\n')
        conn.close()
        read_fd, write_fd = os.pipe()
        try:
            os.read(read_fd, 1)
        finally:
            os.close(read_fd)
            os.close(write_fd)
    original_read_text = Path.read_text
    def blocked_proc_read(self, *args, **kwargs):
        if os.environ['CASE'] == 'blocked_observation' and '/task/' in str(self) and str(self).endswith('/children'):
            block_observation()
        return original_read_text(self, *args, **kwargs)
    Path.read_text = blocked_proc_read
    original_open = Path.open
    class BlockedOutput:
        def __init__(self, stream): self.stream = stream
        def __enter__(self): return self
        def __exit__(self, *args): self.stream.close()
        def write(self, payload):
            if json.loads(payload)['event'] == 'proof_descendant':
                block_observation()
            return self.stream.write(payload)
    def blocked_observation_open(self, *args, **kwargs):
        stream = original_open(self, *args, **kwargs)
        if os.environ['CASE'] == 'blocked_observation_write' and self.name == 'executor-observations.jsonl':
            return BlockedOutput(stream)
        return stream
    Path.open = blocked_observation_open
'''

class FullDeadlineTests(unittest.TestCase):
    def run_case(self, case):
        source_name = os.environ.get('CAPTURE_SOURCE', 'phase-current.py')
        tree = ast.parse(HERE.joinpath('formal-ab-capture.py').read_text())
        for node in ast.walk(tree):
            if isinstance(node, ast.Assign):
                names = [n.id for n in node.targets if isinstance(n, ast.Name)]
                if names == ['deadline']:
                    assert isinstance(node.value, ast.BinOp)
                    assert isinstance(node.value.right, ast.Constant)
                    self.assertEqual(node.value.right.value, 600)
                    # Time itself is under test. Only shorten the outer budget; the
                    # real alarm/handler/pipe/walker/group owner all execute unchanged.
                    node.value.right = ast.Constant(2 if case in ('blocked_observation', 'blocked_observation_write', 'inherited_pipe') else 10)
                elif names == ['gzip_limit'] and case == 'overflow':
                    node.value = ast.Constant(128)
        # Install the boundary hook after imports, before any workflow side effects.
        root_index = next(i for i, n in enumerate(tree.body) if isinstance(n, ast.Assign) and any(isinstance(t, ast.Name) and t.id == 'root' for t in n.targets))
        tree.body[root_index:root_index] = ast.parse(BLOCK_HOOK).body
        with tempfile.TemporaryDirectory(prefix='full-deadline-') as temporary:
            root = Path(temporary)
            source = root / 'source'
            (source / '.github/workflows').mkdir(parents=True)
            (source / 'scripts').mkdir()
            for name in ('verify_core_formal.py', 'formal_contracts.py'):
                shutil.copyfile(SCRIPTS / name, source / 'scripts' / name)
            (source / 'crates/labcolors-core/src/program/attachment/handoff').mkdir(parents=True)
            (source / 'crates/labcolors-core/src/program/attachment/handoff/proofs.rs').write_text('fixture proof')
            (source / 'Cargo.lock').write_text('frozen fixture lock')
            (source / '.github/workflows/ci-worker.yml').write_text('frozen fixture workflow')
            bindir = root / 'bin'
            bindir.mkdir()
            for name in ('git', 'cargo', 'rustc', 'journalctl'):
                path = bindir / name
                path.write_text(FAKE)
                path.chmod(0o755)
            report = {
                'metadata': {'kani_version': '0.68.0', 'target': 'x86_64-unknown-linux-gnu'},
                'verification_results': {
                    'summary': {'total_harnesses': 1, 'executed': 1, 'successful': 1, 'failed': 0, 'status': 'completed'},
                    'results': [dict(harness_id=HARNESS, status='Success', checks=[
                        dict(category='assertion', description=json.dumps('stale prepared handoff must fail before any host call or state mutation'), status='Success'),
                        dict(category='cover', description='same sequence foreign epoch rejected', status='Satisfied'),
                        dict(category='cover', description='same epoch foreign sequence rejected', status='Satisfied'),
                    ])],
                },
            }
            script = root / 'full_capture.py'
            script.write_text(ast.unparse(ast.fix_missing_locations(tree)))
            ready = socket.socket(socket.AF_UNIX)
            ready.bind(str(root / 'ready.sock'))
            ready.listen()
            self.addCleanup(ready.close)
            output = root / 'output'
            output.touch()
            env = dict(os.environ, DIAGNOSTIC_ROOT=str(root), FORMAL_SOURCE_SHA=SHA,
                       FORMAL_FAILED_RUN='42140643837', WORKFLOW_SHA='exact-snapshot-under-test',
                       GITHUB_RUN_ID='fixture', GITHUB_RUN_ATTEMPT='1', GITHUB_OUTPUT=str(output),
                       FORMAL_ARM='A', FORMAL_OWNER_ADAPTER=str(HERE / 'formal-ab-owner.py'), CASE=case, REPORT=json.dumps(report), LEADER_PID=str(root / 'leader.pid'),
                       CHILD_PID=str(root / 'child.pid'), JOURNAL_PID=str(root / 'journal.pid'), READY_SOCKET=str(root / 'ready.sock'),
                       PATH=str(bindir) + ':' + os.environ['PATH'])
            process = subprocess.Popen([sys.executable, str(script)], env=env,
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                                       start_new_session=True)
            try:
                if case in ('inherited_pipe', 'blocked_observation', 'blocked_observation_write', 'metadata_cleanup_expiry'):
                    with selectors.DefaultSelector() as selector:
                        selector.register(ready, selectors.EVENT_READ)
                        self.assertTrue(selector.select(timeout=5), 'fixture did not reach exact blocked boundary')
                    connection, _ = ready.accept()
                    with connection:
                        connection.settimeout(3)
                        message = connection.recv(128)
                    if case == 'metadata_cleanup_expiry':
                        messages = {message}
                        with selectors.DefaultSelector() as selector:
                            selector.register(ready, selectors.EVENT_READ)
                            self.assertTrue(selector.select(timeout=3), 'second metadata boundary handshake missing')
                        connection, _ = ready.accept()
                        with connection:
                            connection.settimeout(3)
                            messages.add(connection.recv(128))
                        self.assertEqual(messages, {b'METADATA_CHILD_READY\n', b'METADATA_CLEANUP_EXPIRY\n'})
                    else:
                        self.assertEqual(message, b'INHERITED_READY\n' if case == 'inherited_pipe' else b'OBSERVATION_BLOCKED\n')
                try:
                    stdout, stderr = process.communicate(timeout=6)
                except subprocess.TimeoutExpired:
                    self.fail('RED: full wrapper did not publish refusal/cleanup after its deadline at the handshaken blocked observation')
                self.assertEqual(process.returncode, 0, stdout + stderr)
                manifest = json.loads((root / 'artifact/A/manifest.json').read_text())
                self.assertEqual(manifest['limits']['command_seconds'], 600)
                self.assertEqual(manifest['limits']['harness_seconds'], 300)
                self.assertEqual(manifest['limits']['artifact_bytes'], 16777216)
                self.assertEqual(manifest['base_sha'], SHA)
                self.assertIn('Canonical command:', stdout) if case != 'metadata_cleanup_expiry' else None
                self.assertEqual(output.read_text(), '')
                self.assertFalse((root / 'artifact/A/executor-observations.jsonl').exists())
                if case not in ('metadata_cleanup_expiry', 'overflow'):
                    self.assertIn('CBMC version 6.11.0', stdout)
                    self.assertIn('x' * 100, stdout)
                    self.assertLess(len(stdout.encode()), 75000)
                leader_pid = None
                if (root / 'leader.pid').exists():
                    leader_pid = int((root / 'leader.pid').read_text())
                    # The adapter is the group leader. Its cargo child can be
                    # adopted by this subreaper after cleanup; reap that exact
                    # child before checking PID absence, never race zombie exit.
                    try:
                        os.waitpid(leader_pid, 0)
                    except ChildProcessError:
                        pass  # The capture already reaped a metadata leader.
                    with self.assertRaises(ProcessLookupError):
                        os.kill(leader_pid, 0)
                if case in ('inherited_pipe', 'metadata_cleanup_expiry'):
                    assert leader_pid is not None
                    child_pid = int((root / 'child.pid').read_text())
                    waited, status = os.waitpid(child_pid, 0)
                    self.assertEqual(waited, child_pid)
                    self.assertEqual(os.waitstatus_to_exitcode(status), -signal.SIGKILL)
                    with self.assertRaises(ProcessLookupError):
                        os.killpg(leader_pid, 0)
                complete = case in ('pass', 'semantic_reject', 'phase_limit')
                self.assertEqual(manifest['complete'], complete, manifest)
                self.assertFalse((root / 'proof.json').exists())
                self.assertLessEqual(sum(p.stat().st_size for p in (root / 'artifact/A').iterdir()), 16777216)
                if complete:
                    self.assertEqual(manifest['phase_log_complete'], False)
                    raw_stdout = gzip.decompress((root / 'artifact/A/proof-stdout.gz').read_bytes())
                    self.assertIn(b'x' * 70000, raw_stdout)
                    for prefix in ('stdout', 'json'):
                        raw = gzip.decompress((root / 'artifact/A' / manifest[prefix + '_file']).read_bytes())
                        self.assertEqual(hashlib.sha256(raw).hexdigest(), manifest['raw_' + prefix + '_sha256'])
                    self.assertEqual(manifest['proof_result'], 'canonical_validation_rejected' if case == 'semantic_reject' else 'canonical_validation_passed')
                else:
                    self.assertEqual(manifest['status'], 'refused')
                    self.assertEqual(manifest['proof_result'], 'unvalidated')
                    self.assertTrue(manifest['partial_corpus_discarded'])
                    self.assertEqual(sorted(p.name for p in (root / 'artifact/A').iterdir()), ['manifest.json'])
                    self.assertIn('shared artifact budget' if case == 'overflow' else '600 seconds', manifest['reason'])
                print(case, 'GREEN', manifest['status'], manifest['proof_result'], flush=True)
            finally:
                if process.poll() is None:
                    # Test-only emergency cleanup for RED; never used as production owner.
                    if (root / 'leader.pid').exists():
                        try: os.killpg(int((root / 'leader.pid').read_text()), signal.SIGKILL)
                        except ProcessLookupError: pass
                    if (root / 'journal.pid').exists():
                        try: os.killpg(int((root / 'journal.pid').read_text()), signal.SIGKILL)
                        except ProcessLookupError: pass
                    try: os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError: pass
                    process.communicate(timeout=3)
                    if (root / 'child.pid').exists():
                        os.waitpid(int((root / 'child.pid').read_text()), 0)
                assert process.stdout is not None and process.stderr is not None
                process.stdout.close()
                process.stderr.close()

    def test_inherited_pipe_deadline_cleanup(self):
        self.run_case('inherited_pipe')

    def test_metadata_cleanup_signal_retains_group_owner(self):
        self.run_case('metadata_cleanup_expiry')

    def test_canonical_pass_rejection_and_overflow(self):
        for case in ('pass', 'semantic_reject', 'overflow', 'phase_limit'):
            with self.subTest(case=case): self.run_case(case)

if __name__ == '__main__':
    unittest.main()
