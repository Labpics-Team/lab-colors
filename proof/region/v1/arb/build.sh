#!/bin/sh
# Public Arb build entrypoint. It always creates the sealed environment before
# invoking the recipe; no caller-controlled sentinel can select the inner path.
set -eu

if [ "$#" -ne 0 ]; then
    printf '%s\n' 'arb build takes no arguments' >&2
    exit 64
fi

script_dir=$(/usr/bin/dirname -- "$0")
inner="$script_dir/build-inner.sh"
if [ ! -f "$inner" ] || [ -L "$inner" ]; then
    printf '%s\n' 'missing regular Arb inner build recipe' >&2
    exit 66
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
    /bin/sh "$inner"
