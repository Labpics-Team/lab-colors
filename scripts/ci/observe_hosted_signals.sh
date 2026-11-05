# Temporary hosted-runner observation. Remove after the shutdown sender is known.
# The proof command and its status remain owned by the caller.

signal_probe_reap_child() {
  local signal_probe_child=$1 signal_probe_reap_attempt
  # A failed root signal must not hold the proof's exit behind a live reader.
  for signal_probe_reap_attempt in {1..10}; do
    if ! jobs -pr | grep -Fxq "$signal_probe_child"; then
      wait "$signal_probe_child" || true
      return 0
    fi
    sleep 0.05
  done
  printf 'SIGNAL_TRACE_CLEANUP_INCOMPLETE child=%s\n' "$signal_probe_child" >&2
}

signal_probe_cleanup() {
  local signal_probe_status=$?
  trap - EXIT
  if [[ "${signal_probe_active:-false}" == true ]]; then
    sleep 10 &
    signal_probe_control=$!
    printf 'SIGNAL_TRACE_END_CONTROL sender=%s recipient=%s signal=15\n' "$BASHPID" "$signal_probe_control" >&2
    kill -TERM "$signal_probe_control" || true
    signal_probe_reap_child "$signal_probe_control"
    signal_probe_control=
  fi
  if [[ -n "$signal_probe_control" ]]; then
    kill -TERM "$signal_probe_control" || true
    signal_probe_reap_child "$signal_probe_control"
  fi
  if [[ -n "$signal_probe_instance" ]]; then
    sudo -n sh -c 'printf 0 > "$1/tracing_on"; printf 0 > "$1/events/signal/signal_generate/enable"' sh "$signal_probe_instance" || true
    sleep 0.1
    sudo -n cat "$signal_probe_instance/trace" || printf 'SIGNAL_TRACE_SNAPSHOT_UNAVAILABLE\n' >&2
    sudo -n sh -c '
      for stats in "$1"/per_cpu/cpu*/stats; do
        printf "SIGNAL_TRACE_DRAIN %s\n" "$stats"
        cat "$stats" || exit
      done
    ' sh "$signal_probe_instance" || printf 'SIGNAL_TRACE_DRAIN_UNAVAILABLE\n' >&2
    if ! (
      set -o pipefail
      sudo -n dmesg --notime |
        awk '/oom-kill:|Out of memory: Killed process|Memory cgroup out of memory: Killed process|oom_reaper:|invoked oom-killer/' |
        tail -n 12
    ); then
      printf 'SIGNAL_TRACE_OOM_LOG_UNAVAILABLE\n' >&2
    fi
    if [[ -f "$RUNNER_TEMP/core-formal/positive-3.log" ]]; then
      awk '/Checking harness |^Verification|^VERIFICATION|CBMC version|Starting Bounded Model Checking|size of program expression:|Generated .*VCC|Passing problem to propositional reduction|converting SSA|Solving with |variables.*clauses|^Runtime (Symex|Postprocess Equation|Convert SSA|Post-process|Solver|decision procedure):/' "$RUNNER_TEMP/core-formal/positive-3.log" |
        tail -n 20 || true
    fi
  fi
  if [[ -n "$signal_probe_reader" && -n "$signal_probe_reader_start" ]]; then
    sudo -n sh -c '
      test "$(awk "{print \$22}" "/proc/$1/stat" 2>/dev/null)" = "$2" &&
      test "$(readlink "/proc/$1/exe" 2>/dev/null)" = "$3" && kill -TERM "$1"
    ' sh "$signal_probe_reader" "$signal_probe_reader_start" "$signal_probe_cat" || true
  fi
  if [[ -n "$signal_probe_launcher" ]]; then
    if [[ -n "$signal_probe_launcher_start" ]]; then
      sudo -n sh -c 'test "$(awk "{print \$22}" "/proc/$1/stat" 2>/dev/null)" = "$2" && kill -TERM "$1"' sh \
        "$signal_probe_launcher" "$signal_probe_launcher_start" || true
    fi
    signal_probe_reap_child "$signal_probe_launcher"
  fi
  if [[ -n "$signal_probe_instance" ]]; then
    sudo -n rmdir "$signal_probe_instance" || printf 'SIGNAL_TRACE_CLEANUP_INCOMPLETE %s\n' "$signal_probe_instance" >&2
  fi
  if [[ -n "$signal_probe_dir" ]]; then
    rm -f "$signal_probe_dir/reader.pid"
    rmdir "$signal_probe_dir" || true
  fi
  return "$signal_probe_status"
}

signal_probe_setup() {
  local signal_probe_format=/sys/kernel/tracing/events/signal/signal_generate/format
  sudo -n test -r "$signal_probe_format" || return
  sudo -n cat "$signal_probe_format" || return
  signal_probe_cat=$(readlink -f "$(command -v cat)") || return
  command -v setsid >/dev/null || return
  signal_probe_dir=$(mktemp -d "$RUNNER_TEMP/colors-signals.XXXXXX") || return
  local signal_probe_new_instance=/sys/kernel/tracing/instances/$(basename "$signal_probe_dir")
  sudo -n mkdir "$signal_probe_new_instance" || return
  signal_probe_instance=$signal_probe_new_instance
  trap signal_probe_cleanup EXIT

  local signal_probe_pid=$BASHPID signal_probe_tgid signal_probe_tid signal_probe_parent
  local signal_probe_found_worker=false signal_probe_found_listener=false
  while [[ "$signal_probe_pid" =~ ^[1-9][0-9]*$ && "$signal_probe_pid" != 1 ]]; do
    signal_probe_tgid=$(awk '$1 == "Tgid:" {print $2}' "/proc/$signal_probe_pid/status") || return
    case $(basename "$(readlink "/proc/$signal_probe_pid/exe")") in
      Runner.Worker) signal_probe_found_worker=true ;;
      Runner.Listener) signal_probe_found_listener=true ;;
    esac
    for signal_probe_tid in /proc/"$signal_probe_pid"/task/*; do
      signal_probe_tid=$(basename "$signal_probe_tid")
      [[ "$signal_probe_tid" =~ ^[1-9][0-9]*$ ]] || return
      printf 'SIGNAL_TRACE_TARGET tid=%s tgid=%s\n' "$signal_probe_tid" "$signal_probe_tgid" >&2
    done
    [[ "$signal_probe_found_listener" == true ]] && break
    signal_probe_parent=$(awk '$1 == "PPid:" {print $2}' "/proc/$signal_probe_pid/status") || return
    signal_probe_pid=$signal_probe_parent
  done
  [[ "$signal_probe_found_worker" == true && "$signal_probe_found_listener" == true ]] || return 1

  sudo -n sh -c 'printf 16 > "$1/buffer_size_kb"' sh "$signal_probe_instance" || return
  sudo -n setsid --wait sh -c 'printf "%s\n" "$$" > "$1"; exec cat "$2"' sh \
    "$signal_probe_dir/reader.pid" "$signal_probe_instance/trace_pipe" &
  signal_probe_launcher=$!
  signal_probe_launcher_start=$(sudo -n awk '{print $22}' "/proc/$signal_probe_launcher/stat" 2>/dev/null) || true
  local signal_probe_attempt
  for signal_probe_attempt in {1..50}; do
    if [[ -s "$signal_probe_dir/reader.pid" ]]; then
      read -r signal_probe_reader < "$signal_probe_dir/reader.pid"
      if [[ "$signal_probe_reader" =~ ^[1-9][0-9]*$ ]] &&
        [[ $(sudo -n readlink "/proc/$signal_probe_reader/exe" 2>/dev/null) == "$signal_probe_cat" ]]; then
        signal_probe_reader_start=$(sudo -n awk '{print $22}' "/proc/$signal_probe_reader/stat")
        break
      fi
    fi
    sleep 0.05
  done
  [[ -n "$signal_probe_reader_start" ]] || return 1

  sleep 10 &
  signal_probe_control=$!
  local signal_probe_control_pid=$signal_probe_control signal_probe_sender=$BASHPID signal_probe_filter
  # Later Python/Kani descendants and Runner threads are absent from the initial
  # ancestry. Trace only fatal signal metadata throughout this disposable VM.
  signal_probe_filter="sig == 2 || sig == 9 || sig == 15"
  if ! sudo -n sh -c 'printf "%s" "$2" > "$1/events/signal/signal_generate/filter"; printf 1 > "$1/events/signal/signal_generate/enable"; printf 1 > "$1/tracing_on"' sh "$signal_probe_instance" "$signal_probe_filter"; then
    kill -TERM "$signal_probe_control" || true
    wait "$signal_probe_control" || true
    signal_probe_control=
    return 1
  fi
  sudo -n cat "$signal_probe_instance/events/signal/signal_generate/filter" || return
  # The sleep is our child. This builtin kill has a known sender and recipient.
  kill -TERM "$signal_probe_control" || return
  wait "$signal_probe_control" || true
  signal_probe_control=
  signal_probe_active=true
  printf 'SIGNAL_TRACE_CONTROL sender=%s recipient=%s signal=15\n' "$signal_probe_sender" "$signal_probe_control_pid" >&2
  printf 'SIGNAL_TRACE_ACTIVE instance=%s; interpretation requires matched start/end controls and drained no-loss stats\n' "$signal_probe_instance" >&2
}

observe_hosted_signals() {
  signal_probe_instance= signal_probe_dir= signal_probe_reader= signal_probe_reader_start= signal_probe_launcher= signal_probe_launcher_start= signal_probe_cat= signal_probe_control= signal_probe_active=false
  if ! signal_probe_setup; then
    printf 'SIGNAL_TRACE_UNAVAILABLE; proof parameters and exit handling are unchanged\n' >&2
    signal_probe_cleanup || true
  fi
}
