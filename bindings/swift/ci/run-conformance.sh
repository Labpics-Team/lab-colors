#!/usr/bin/env bash
# Linux Swift-conformance использует уже допущенные toolchain capability и
# работает только в эфемерной копии checkout. Так один скрипт проверяет FFI,
# не изменяя исходники и не создавая второй Docker trust boundary внутри gVisor.
set -euo pipefail

: "${RUST_TOOLCHAIN:?RUST_TOOLCHAIN must declare the exact Rust version}"
: "${SWIFT_TOOLCHAIN:?SWIFT_TOOLCHAIN must declare the exact Swift version}"
[[ "$RUST_TOOLCHAIN" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
[[ "$SWIFT_TOOLCHAIN" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]

readonly source_root="${GITHUB_WORKSPACE:-/src}"
readonly temp_root="${RUNNER_TEMP:-/work}"
readonly expected_swift="Swift version ${SWIFT_TOOLCHAIN} (swift-${SWIFT_TOOLCHAIN}-RELEASE)"

[[ -d "$source_root" ]] || {
  echo "source root does not exist: $source_root" >&2
  exit 64
}
install -d -m 0700 "$temp_root"
work_root="$(mktemp -d "${temp_root%/}/labcolors-swift.XXXXXX")"
readonly work_root
cleanup() {
  rm -rf -- "$work_root"
}
trap cleanup EXIT

echo "==================== TOOLCHAINS ===================="
uname -a
actual_swift="$(swift --version | sed -n '1p')"
[[ "$actual_swift" == "$expected_swift" ]] || {
  echo "Swift toolchain mismatch: expected '$expected_swift', got '${actual_swift:-missing}'" >&2
  exit 64
}
actual_rust="$(rustc +"$RUST_TOOLCHAIN" --version)"
actual_cargo="$(cargo +"$RUST_TOOLCHAIN" --version)"
[[ "$actual_rust" == "rustc ${RUST_TOOLCHAIN} "* ]] || {
  echo "Rust toolchain mismatch: $actual_rust" >&2
  exit 64
}
[[ "$actual_cargo" == "cargo ${RUST_TOOLCHAIN} "* ]] || {
  echo "Cargo toolchain mismatch: $actual_cargo" >&2
  exit 64
}
printf '%s\n%s\n%s\n' "$actual_swift" "$actual_rust" "$actual_cargo"

echo "==================== EPHEMERAL SOURCE COPY ===================="
tar -C "$source_root" \
  --exclude=target --exclude=.git --exclude=node_modules \
  --exclude='mutants.out' --exclude='mutants.out.old' \
  --exclude='bindings/swift/.build' --exclude='bindings/swift/generated' \
  -cf - . | tar -C "$work_root" -xf -
cd "$work_root"

echo "==================== BUILD FFI AND BINDGEN ===================="
cargo +"$RUST_TOOLCHAIN" build -p labcolors-ffi --features cli --locked
test -f target/debug/liblabcolors.so

echo "==================== RUST REFERENCE ===================="
cargo +"$RUST_TOOLCHAIN" test -p labcolors-conformance -p labcolors-ffi --locked

echo "==================== GENERATE SWIFT BINDINGS ===================="
cargo +"$RUST_TOOLCHAIN" run -p labcolors-ffi --features cli --bin uniffi-bindgen --locked -- \
  generate --library target/debug/liblabcolors.so \
  --language swift --out-dir bindings/swift/generated

echo "==================== ARRANGE SWIFTPM SOURCES ===================="
readonly generated=bindings/swift/generated
install -d bindings/swift/Sources/LabColors bindings/swift/Sources/labcolorsFFI
install -m 0644 "$generated/labcolors.swift" bindings/swift/Sources/LabColors/labcolors.swift
install -m 0644 "$generated/labcolorsFFI.h" bindings/swift/Sources/labcolorsFFI/labcolorsFFI.h
install -m 0644 "$generated/labcolorsFFI.modulemap" bindings/swift/Sources/labcolorsFFI/module.modulemap

echo "==================== SWIFT CONFORMANCE ===================="
cd bindings/swift
export LD_LIBRARY_PATH="$work_root/target/debug:${LD_LIBRARY_PATH:-}"
swift test -Xlinker -L"$work_root/target/debug"

echo "==================== SWIFT CONFORMANCE: OK ===================="
