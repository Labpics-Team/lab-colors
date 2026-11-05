#!/usr/bin/env bash
# Keep the command's status distinct from a failed stdout/log relay.
log=$1
shift
set +e
# Continue writing the logfile if the CI stdout consumer closes its pipe.
"$@" 2>&1 | tee --output-error=warn -- "$log"
statuses=("${PIPESTATUS[@]}")
if (( statuses[1] != 0 )); then
    exit 125
fi
exit "${statuses[0]}"
