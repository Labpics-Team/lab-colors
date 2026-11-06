"""One-shot transport adapter: canonical owner command and result, streamed not raw-file logged."""
import subprocess
import sys
from pathlib import Path

source = Path(sys.argv[1])
output = Path(sys.argv[2])
harness = sys.argv[3]
sys.path.insert(0, str(source / 'scripts'))
import verify_core_formal as owner
# Only the raw-file transport is replaced. run_kani owns every command flag,
# target and exported result; the outer reviewed capture owns the 600s deadline.
def stream_command(command, log, *, timeout=1200):
    # Same native phase verbosity as the reviewed exact diagnostic; no SSA dump.
    command.append('--verbose')
    print('Canonical command: ' + repr(command), flush=True)
    return subprocess.call(command, cwd=owner.ROOT)
owner._run_logged_command = stream_command
report, code = owner.run_kani(output, positive_harnesses=(harness,))
raise SystemExit(code)
