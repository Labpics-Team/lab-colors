# Temporary source attribution for a formal failure on the disposable hosted VM.
# The caller retains the proof command and its exit status. No files are archived.

formal_failure_cleanup() {
  local formal_failure_status=$?
  trap - EXIT
  if [[ "$formal_failure_status" == 0 ]]; then
    return 0
  fi
  if ! (
    set -o pipefail
    sudo -n dmesg --notime |
      awk '/oom-kill:|Out of memory: Killed process|Memory cgroup out of memory: Killed process|oom_reaper:|invoked oom-killer/' |
      tail -n 12
  ); then
    printf 'FORMAL_OOM_LOG_UNAVAILABLE\n' >&2
  fi
  if [[ -f "$RUNNER_TEMP/core-formal/positive-3.log" ]]; then
    awk '/Checking harness |^Verification|^VERIFICATION|CBMC version|Generated C code written to|Demangled GotoC code written to|Starting Bounded Model Checking|size of program expression:|Generated .*VCC|Passing problem to propositional reduction|converting SSA|Solving with |variables.*clauses|^Runtime (Symex|Postprocess Equation|Convert SSA|Post-process|Solver|decision procedure):/' "$RUNNER_TEMP/core-formal/positive-3.log" |
      tail -n 30 || true
  fi
  python3 - <<'PY' || printf 'FORMAL_C_SOURCE_UNAVAILABLE\n' >&2
from pathlib import Path

root = Path('target/kani')
files = sorted(
    (path for path in root.rglob('*.demangled.c') if path.is_file() and not path.is_symlink()),
    key=lambda path: path.stat().st_mtime_ns,
    reverse=True,
)
print(f'FORMAL_C_SOURCE files={len(files)}; bounded source view, not execution attribution')
for path in files[:8]:
    print(f'FORMAL_C_SOURCE path={path} bytes={path.stat().st_size}')
    snippets = 0
    line_number = 1
    with path.open(encoding='utf-8', errors='replace') as stream:
        while fragment := stream.readline(4096):
            if any(name in fragment for name in ('Handoff', 'RawVec', 'collect', 'drop_in_place')):
                print(f'{line_number}: {fragment.strip()[:240]}')
                snippets += 1
                if snippets == 20:
                    break
            line_number += fragment.endswith('\n')
PY
  return "$formal_failure_status"
}

observe_hosted_signals() {
  trap formal_failure_cleanup EXIT
}
