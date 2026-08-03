#!/bin/sh
# Internal MPFI recipe. The source-bound transport dispatches this file only
# after establishing its clean child environment; it is not a standalone API.
set -eu

if [ "$#" -ne 0 ]; then
    printf '%s\n' 'mpfi build takes no arguments' >&2
    exit 64
fi

umask 022

readonly inputs=/build/snapshot/inputs
readonly workspace=/build/snapshot/workspace
readonly build=/build/work
readonly compiler=/usr/bin/clang-19
readonly common_cflags='-O2 -g0 -fno-ident -fno-fast-math -ffp-contract=off -fno-lto -std=gnu17 -march=x86-64 -mtune=generic -ffile-prefix-map=/build=. -fdebug-prefix-map=/build=.'
readonly evaluator_cflags='-O2 -g0 -fno-fast-math -ffp-contract=off -fno-lto -march=x86-64 -mtune=generic -ffile-prefix-map=/build=. -fdebug-prefix-map=/build=. -std=c17 -Wall -Wextra -Werror -pedantic'
readonly prefix="$build/prefix"
readonly evaluator_sources='main.c wire.c hash.c interval.c region.c'
readonly mpfi_test_exclusions='^(tdiv_ext|texp10|trec_sqrt)$'

require_regular() {
    if [ ! -f "$1" ] || [ -L "$1" ]; then
        printf 'missing regular build input: %s\n' "$1" >&2
        exit 66
    fi
}

require_executable() {
    if [ ! -f "$1" ] || [ ! -x "$1" ]; then
        printf 'missing executable build tool: %s\n' "$1" >&2
        exit 66
    fi
}

require_clang_19() {
    version=$("$1" --version) || {
        printf '%s\n' 'cannot inspect the admitted Clang compiler' >&2
        exit 67
    }
    if ! printf '%s\n' "$version" | /usr/bin/grep -q 'clang version 19\.'; then
        printf '%s\n' 'MPFI build requires the admitted Clang 19 compiler family' >&2
        exit 67
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
require_executable "$compiler"
require_clang_19 "$compiler"
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
# MPFI 1.5.4 ships three defective tests: two pass incompatible function
# pointers to the generic harness, which Clang 19 rejects at compile time,
# while texp10 names a fixture absent from the sealed source archive.
# Exclude those upstream defects from compilation and from the run alike;
# every other shipped test remains part of this source-bound library check.
make_database="$build/mpfi-check-database"
if ! /usr/bin/make -pn > "$make_database"; then
    printf '%s\n' 'cannot inspect MPFI upstream test inventory' >&2
    exit 70
fi
mpfi_tests=$(
    /usr/bin/awk -v exclusions="$mpfi_test_exclusions" '
        /^check_PROGRAMS =/ && !found {
            found = 1
            for (i = 3; i <= NF; i++) {
                gsub(/\$\(EXEEXT\)/, "", $i)
                if ($i !~ exclusions)
                    printf "%s ", $i
            }
        }
    ' "$make_database"
)
if [ -z "$mpfi_tests" ]; then
    printf '%s\n' 'MPFI upstream test inventory is empty after exclusions' >&2
    exit 70
fi
/usr/bin/make check -j1 TESTS="$mpfi_tests" check_PROGRAMS="$mpfi_tests" CFLAGS="$common_cflags"
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
