#!/bin/sh
# Internal Arb recipe. The public build.sh entrypoint always starts this file
# through its sealed environment; no caller-controlled variable selects a
# pre-sanitized execution path.
set -eu

if [ "$#" -ne 0 ]; then
    printf '%s\n' 'arb build takes no arguments' >&2
    exit 64
fi

umask 022

readonly inputs=/build/snapshot/inputs
readonly workspace=/build/snapshot/workspace
readonly build=/build/work

require_regular() {
    if [ ! -f "$1" ] || [ -L "$1" ]; then
        printf 'missing regular build input: %s\n' "$1" >&2
        exit 66
    fi
}

require_directory() {
    if [ ! -d "$1" ] || [ -L "$1" ]; then
        printf 'missing normalized source directory: %s\n' "$1" >&2
        exit 66
    fi
}

require_empty_directory() {
    if [ ! -d "$1" ] || [ -L "$1" ]; then
        printf 'missing build directory: %s\n' "$1" >&2
        exit 66
    fi
    if [ -n "$(find "$1" -mindepth 1 -maxdepth 1 -print -quit)" ]; then
        printf 'build directory is not empty: %s\n' "$1" >&2
        exit 65
    fi
}

require_directory "$inputs/gmp-6.3.0"
require_directory "$inputs/mpfr-4.2.2"
require_directory "$inputs/flint-3.6.0"
require_regular "$inputs/formula.generated.c"
printf '%s  %s\n' \
    '9958f20c8ca598625db0593a45f8f8bc79e4b2f22b53263b6c32d78a5e1d2693' \
    "$inputs/formula.generated.c" \
    | /usr/bin/sha256sum --check --strict -
for source in main.c wire.c hash.c interval.c region.c; do
    require_regular "$workspace/proof/region/v1/arb/evaluator/$source"
done
require_regular "$workspace/proof/region/v1/arb/evaluator/formula.h"
for header in wire.h hash.h interval.h region.h; do
    require_regular "$workspace/proof/region/v1/arb/evaluator/$header"
done
require_empty_directory "$build"

/usr/bin/mkdir "$build/prefix" "$build/gmp" "$build/mpfr" "$build/flint" "$build/tmp"

# GCC 15 changed its implicit dialect to GNU C23, where GMP 6.3.0's locked
# no-prototype configure probes have different semantics.  GNU C17 is the last
# default those probes targeted; changing it requires a source/toolchain slice
# and a fresh live build, not reliance on a compiler's moving default.
readonly common_cflags='-O2 -g0 -fno-ident -fno-fast-math -ffp-contract=off -fno-lto -std=gnu17 -march=x86-64 -mtune=generic -ffile-prefix-map=/build=. -fdebug-prefix-map=/build=.'
readonly common_ldflags='-Wl,--build-id=none -fno-lto'
readonly prefix="$build/prefix"

cd "$build/gmp"
ABI=64 CC=/usr/local/bin/gcc CFLAGS="$common_cflags" LDFLAGS="$common_ldflags" \
    "$inputs/gmp-6.3.0/configure" \
    --build=x86_64-pc-linux-gnu \
    --host=x86_64-pc-linux-gnu \
    --prefix="$prefix" \
    --disable-shared \
    --enable-static \
    --disable-assembly \
    --disable-cxx
/usr/bin/make -j1
/usr/bin/make check -j1
/usr/bin/make install

cd "$build/mpfr"
CC=/usr/local/bin/gcc CFLAGS="$common_cflags" LDFLAGS="$common_ldflags" \
    "$inputs/mpfr-4.2.2/configure" \
    --build=x86_64-pc-linux-gnu \
    --host=x86_64-pc-linux-gnu \
    --prefix="$prefix" \
    --with-gmp="$prefix" \
    --disable-shared \
    --enable-static \
    --enable-formally-proven-code
/usr/bin/make -j1
/usr/bin/make check -j1
/usr/bin/make install

cd "$build/flint"
CC=/usr/local/bin/gcc CFLAGS="$common_cflags" LDFLAGS="$common_ldflags" \
    "$inputs/flint-3.6.0/configure" \
    --build=x86_64-pc-linux-gnu \
    --host=x86_64-pc-linux-gnu \
    --prefix="$prefix" \
    --with-gmp="$prefix" \
    --with-mpfr="$prefix" \
    --disable-shared \
    --enable-static \
    --disable-assembly \
    --disable-lto \
    --enable-assert
/usr/bin/make -j1
/usr/bin/make check -j1
/usr/bin/make install

cd "$workspace/proof/region/v1/arb/evaluator"
# The evaluator includes FLINT headers, whose attribute-guarded inlines
# require the GNU dialect; the dependency chain is built with -std=gnu17
# above, so the final link uses the same dialect while -Wall -Wextra
# -Werror -pedantic keep the evaluator's own C diagnostics strict.
/usr/local/bin/gcc \
    -O2 -g0 -fno-ident -fno-fast-math -ffp-contract=off -fno-lto \
    -march=x86-64 -mtune=generic \
    -ffile-prefix-map=/build=. -fdebug-prefix-map=/build=. \
    -std=gnu17 -Wall -Wextra -Werror -pedantic \
    -I. -I"$prefix/include" \
    main.c wire.c hash.c interval.c region.c "$inputs/formula.generated.c" \
    -static -Wl,--build-id=none -fno-lto \
    "$prefix/lib/libflint.a" "$prefix/lib/libmpfr.a" "$prefix/lib/libgmp.a" \
    -lm -lpthread \
    -o "$build/arb-evaluator-v1"

if ! /usr/bin/readelf -l "$build/arb-evaluator-v1" > "$build/program-headers"; then
    printf '%s\n' 'cannot inspect evaluator program headers' >&2
    exit 70
fi
if /usr/bin/grep -q INTERP "$build/program-headers"; then
    printf '%s\n' 'evaluator unexpectedly contains PT_INTERP' >&2
    exit 70
else
    grep_status=$?
    if [ "$grep_status" -ne 1 ]; then
        printf '%s\n' 'cannot search evaluator program headers' >&2
        exit 70
    fi
fi
if ! /usr/bin/readelf -d "$build/arb-evaluator-v1" > "$build/dynamic-section"; then
    printf '%s\n' 'cannot inspect evaluator dynamic section' >&2
    exit 70
fi
if /usr/bin/grep -q NEEDED "$build/dynamic-section"; then
    printf '%s\n' 'evaluator unexpectedly contains DT_NEEDED' >&2
    exit 70
else
    grep_status=$?
    if [ "$grep_status" -ne 1 ]; then
        printf '%s\n' 'cannot search evaluator dynamic section' >&2
        exit 70
    fi
fi

/usr/bin/sha256sum "$build/arb-evaluator-v1"
