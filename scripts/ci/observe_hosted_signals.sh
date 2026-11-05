# Temporary source attribution for a formal failure on the disposable hosted VM.
# The caller retains the proof command and its exit status. No files are archived.

formal_failure_cleanup() {
  local formal_failure_status=$?
  trap - EXIT
  local formal_running_pid
  for formal_running_pid in $(jobs -pr); do
    if [[ "$formal_running_pid" == "${formal_memory_watcher_pid:-}" ]]; then
      kill "$formal_running_pid" 2>/dev/null || true
    fi
  done
  if [[ -n "${formal_memory_watcher_pid:-}" ]]; then
    wait "$formal_memory_watcher_pid" 2>/dev/null || true
    unset formal_memory_watcher_pid
  fi
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
  python3 -u - <<'PY' &
from pathlib import Path
import os
import re
import time

cgroup = next((line.split(':', 2)[2] for line in Path('/proc/self/cgroup').read_text().splitlines()
               if line.startswith('0::')), None)
cgroup_root = Path('/sys/fs/cgroup') / cgroup.lstrip('/') if cgroup is not None else None
page_kib = os.sysconf('SC_PAGE_SIZE') // 1024
for sample in range(120):
    memory = {}
    for line in Path('/proc/meminfo').read_text().splitlines():
        key, value = line.split(':', 1)
        if key in ('MemTotal', 'MemAvailable', 'SwapTotal', 'SwapFree'):
            memory[key] = int(value.split()[0])
    print('FORMAL_MEMORY', f'sample={sample + 1}', f'epoch_s={int(time.time())}',
          *(f'{key}_kib={value}' for key, value in memory.items()), flush=True)
    if cgroup_root is not None:
        counters = []
        for name in ('memory.current', 'memory.peak', 'memory.max', 'memory.swap.current', 'memory.swap.max'):
            try:
                value = (cgroup_root / name).read_text().strip()
                if value == 'max' or value.isdecimal():
                    counters.append(f'{name}={value}')
            except OSError:
                pass
        try:
            for line in (cgroup_root / 'memory.events').read_text().splitlines():
                key, value = line.split()
                if key in ('low', 'high', 'max', 'oom', 'oom_kill', 'oom_group_kill') and value.isdecimal():
                    counters.append(f'event.{key}={value}')
        except OSError:
            pass
        print('FORMAL_CGROUP_MEMORY', *counters, flush=True)
    processes = []
    for process in Path('/proc').glob('[0-9]*'):
        try:
            rss = int((process / 'statm').read_text().split()[1]) * page_kib
            comm = re.sub(r'[^a-zA-Z0-9_.-]', '_', (process / 'comm').read_text().strip())[:32]
            processes.append((rss, int(process.name), comm))
        except (OSError, ValueError, IndexError):
            pass
    print('FORMAL_RSS', *(f'pid={pid},rss_kib={rss},comm={comm}'
                          for rss, pid, comm in sorted(processes, reverse=True)[:6]), flush=True)
    time.sleep(5)
PY
  formal_memory_watcher_pid=$!
}
