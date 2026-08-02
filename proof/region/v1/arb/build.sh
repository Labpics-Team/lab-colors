#!/bin/sh
# Source-owned Arb dispatcher. The trusted Docker transport invokes this file
# from the fixed bundle path with a clean environment. Keep the outer shell
# limited to builtins: path resolution must happen only in the clean child.
# shellcheck disable=SC2016
set -eu

if [ "$#" -ne 0 ]; then
    printf '%s\n' 'arb build takes no arguments' >&2
    exit 64
fi

exec /usr/bin/env -i \
    PATH=/usr/local/bin:/usr/bin:/bin \
    LC_ALL=C \
    LANG=C \
    TZ=UTC \
    HOME=/nonexistent \
    TMPDIR=/build/work/tmp \
    SOURCE_DATE_EPOCH=0 \
    ZERO_AR_DATE=1 \
    ARFLAGS=crD \
    /bin/sh -c '
        set -eu
        script_path=$(/usr/bin/readlink -f -- "$1")
        script_dir=$(/usr/bin/dirname -- "$script_path")
        inner="$script_dir/build-inner.sh"
        if [ ! -f "$inner" ] || [ -L "$inner" ]; then
            printf "%s\\n" "missing regular Arb inner build recipe" >&2
            exit 66
        fi
        exec /bin/sh "$inner"
    ' /bin/sh "$0"
