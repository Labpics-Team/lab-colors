#!/bin/sh
# Build the independent MPFI evaluator from one sealed, offline input.
# Source acquisition, archive admission and toolchain identity belong to the
# controller; this recipe deliberately accepts no network or ambient state.

set -eu

# The compiler flag string is a sealed, space-delimited profile; word splitting
# is intentional because the recipe runs without a shell-generated environment.

if [ "$#" -ne 0 ]; then
    printf '%s\n' 'mpfi build takes no arguments' >&2
    exit 64
fi

if [ "${LC_MPFI_BUILD_ENV_V1-}" != 1 ]; then
    exec /usr/bin/env -i \
        LC_MPFI_BUILD_ENV_V1=1 \
        PATH=/usr/bin:/bin \
        LC_ALL=C \
        LANG=C \
        TZ=UTC \
        HOME=/nonexistent \
        TMPDIR=/build/work/tmp \
        SOURCE_DATE_EPOCH=0 \
        ZERO_AR_DATE=1 \
        ARFLAGS=crD \
        /bin/sh "$0"
fi
unset LC_MPFI_BUILD_ENV_V1

umask 022

readonly inputs=/build/snapshot/inputs
readonly workspace=/build/snapshot/workspace
readonly build=/build/work
readonly compiler=/usr/bin/clang-19
readonly common_cflags='-O2 -g0 -fno-ident -fno-fast-math -ffp-contract=off -fno-lto -std=gnu17 -march=x86-64 -mtune=generic -ffile-prefix-map=/build=. -fdebug-prefix-map=/build=.'
readonly evaluator_cflags='-O2 -g0 -fno-ident -fno-fast-math -ffp-contract=off -fno-lto -march=x86-64 -mtune=generic -ffile-prefix-map=/build=. -fdebug-prefix-map=/build=. -std=c17 -Wall -Wextra -Werror -pedantic'
readonly prefix="$build/prefix"
readonly evaluator_sources='main.c wire.c hash.c interval.c region.c'

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

require_absent_pattern() {
    pattern=$1
    path=$2
    message=$3
    inspection_error=$4
    if /usr/bin/grep -q "$pattern" "$path"; then
        printf '%s\n' "$message" >&2
        exit 70
    else
        grep_status=$?
        if [ "$grep_status" -ne 1 ]; then
            printf '%s\n' "$inspection_error" >&2
            exit 70
        fi
    fi
}

require_regular "$inputs/formula.generated.c"
require_directory "$inputs/sources/gmp"
require_directory "$inputs/sources/mpfr"
require_directory "$inputs/sources/mpfi"
require_regular "$workspace/proof/region/v1/mpfi/operations.py"
for source in main.c wire.c hash.c interval.c region.c; do
    require_regular "$workspace/proof/region/v1/mpfi/evaluator/$source"
done
for header in wire.h hash.h interval.h region.h formula.h; do
    require_regular "$workspace/proof/region/v1/mpfi/evaluator/$header"
done
printf '%s  %s\n' \
    'a8df7529261ba68e8fbf591cff283ec88a35cb98958b293bc7885d9fb4dd0fb6' \
    "$inputs/formula.generated.c" \
    | /usr/bin/sha256sum --check --strict -
/usr/bin/python3 "$workspace/proof/region/v1/mpfi/operations.py" \
    "$workspace/proof/region/v1/mpfi/evaluator"
require_regular "$compiler"
if ! "$compiler" --version | /usr/bin/grep -q '^clang version 19\.'; then
    printf '%s\n' 'MPFI build requires the admitted Clang 19 compiler family' >&2
    exit 67
fi
require_empty_directory "$build"

/usr/bin/mkdir "$build/prefix" "$build/gmp" "$build/mpfr" "$build/mpfi" "$build/tmp"

cd "$build/gmp"
ABI=64 CC="$compiler" CFLAGS="$common_cflags" \
    "$inputs/sources/gmp/configure" \
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
CC="$compiler" CFLAGS="$common_cflags" \
    "$inputs/sources/mpfr/configure" \
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

cd "$build/mpfi"
CC="$compiler" CFLAGS="$common_cflags" \
    "$inputs/sources/mpfi/configure" \
    --build=x86_64-pc-linux-gnu \
    --host=x86_64-pc-linux-gnu \
    --prefix="$prefix" \
    --with-gmp="$prefix" \
    --with-mpfr="$prefix" \
    --disable-shared \
    --enable-static
/usr/bin/make -j1
# MPFI 1.5.4's tdiv_ext test declares an incompatible callback under Clang;
# this diagnostic-only exception is pinned here and does not widen evaluator
# operations.  The test still compiles, links and runs under the same build.
/usr/bin/make check -j1 CFLAGS="$common_cflags -Wno-error=incompatible-function-pointer-types"
/usr/bin/make install

cd "$workspace/proof/region/v1/mpfi/evaluator"
for source in $evaluator_sources; do
    object="$build/${source%.c}.o"
    # shellcheck disable=SC2086
    "$compiler" $evaluator_cflags \
        -I. -I"$prefix/include" \
        -c "$source" \
        -o "$object"
done
# shellcheck disable=SC2086
"$compiler" $evaluator_cflags \
    -I. -I"$prefix/include" \
    -c "$inputs/formula.generated.c" \
    -o "$build/formula.generated.o"
if ! /usr/bin/nm --undefined-only "$build"/*.o > "$build/evaluator-undefined-symbols"; then
    printf '%s\n' 'cannot inspect evaluator undefined symbols' >&2
    exit 70
fi
/usr/bin/python3 "$workspace/proof/region/v1/mpfi/operations.py" \
    --undefined-symbols "$build/evaluator-undefined-symbols"
# shellcheck disable=SC2086
"$compiler" $evaluator_cflags \
    "$build/main.o" "$build/wire.o" "$build/hash.o" "$build/interval.o" \
    "$build/region.o" "$build/formula.generated.o" \
    -static -Wl,--build-id=none -fno-lto \
    "$prefix/lib/libmpfi.a" "$prefix/lib/libmpfr.a" "$prefix/lib/libgmp.a" \
    -lm -lpthread \
    -o "$build/mpfi-evaluator-v1"

if ! /usr/bin/readelf -l "$build/mpfi-evaluator-v1" > "$build/program-headers"; then
    printf '%s\n' 'cannot inspect evaluator program headers' >&2
    exit 70
fi
require_absent_pattern \
    INTERP \
    "$build/program-headers" \
    'evaluator unexpectedly contains PT_INTERP' \
    'cannot inspect evaluator program headers'
if ! /usr/bin/readelf -d "$build/mpfi-evaluator-v1" > "$build/dynamic-section"; then
    printf '%s\n' 'cannot inspect evaluator dynamic section' >&2
    exit 70
fi
require_absent_pattern \
    NEEDED \
    "$build/dynamic-section" \
    'evaluator unexpectedly contains DT_NEEDED' \
    'cannot inspect evaluator dynamic section'

/usr/bin/sha256sum "$build/mpfi-evaluator-v1"
