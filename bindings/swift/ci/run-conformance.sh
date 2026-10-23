#!/usr/bin/env bash
# Rust-производитель и Swift-потребитель разделены. В контейнер потребителя
# попадают только готовая библиотека, Swift API и публичные тестовые векторы.
set -euo pipefail

: "${RUST_TOOLCHAIN:?declare the exact Rust toolchain}"
: "${SWIFT_TOOLCHAIN:?declare the exact Swift toolchain}"
: "${SWIFT_IMAGE_V1:?declare the digest-pinned Swift image}"
[[ "$RUST_TOOLCHAIN" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
[[ "$SWIFT_TOOLCHAIN" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
[[ "$SWIFT_IMAGE_V1" =~ @sha256:[0-9a-f]{64}$ ]]
readonly source_root="${GITHUB_WORKSPACE:-$(git rev-parse --show-toplevel)}"
readonly temp_root="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
readonly docker="${LABCOLORS_SWIFT_DOCKER:-$(command -v docker)}"
export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"

[[ "$(rustc +"$RUST_TOOLCHAIN" --version)" == "rustc ${RUST_TOOLCHAIN} "* ]]
[[ "$(cargo +"$RUST_TOOLCHAIN" --version)" == "cargo ${RUST_TOOLCHAIN} "* ]]
# Core проверяет локальные Git-объекты без скрытого сетевого получения.
# Отсутствующая возможность инструмента является ошибкой производителя,
# а не поводом распространять библиотеку с выдуманной source identity.
git --no-lazy-fetch --version

work_root="$(mktemp -d "${temp_root%/}/labcolors-native.XXXXXX")"
readonly work_root
trap 'rm -rf -- "$work_root"' EXIT
readonly source="$work_root/source"
readonly consumer="$work_root/consumer"
source_sha="$(git -C "$source_root" rev-parse HEAD)"
git clone --shared --no-checkout "$source_root" "$source"
git -C "$source" checkout --detach "$source_sha"
cd "$source"
export CARGO_TARGET_DIR="$source/target"

cargo +"$RUST_TOOLCHAIN" build -p labcolors-ffi --features cli --locked
cargo +"$RUST_TOOLCHAIN" test -p labcolors-conformance -p labcolors-ffi --locked
cargo +"$RUST_TOOLCHAIN" run -p labcolors-ffi --features cli --bin uniffi-bindgen --locked -- \
  generate --library target/debug/liblabcolors.so --language swift --out-dir "$work_root/generated"

install -d "$consumer/Sources/LabColors" "$consumer/Sources/labcolorsFFI" \
  "$consumer/Tests/LabColorsConformanceTests/Fixtures" "$consumer/lib"
cp bindings/swift/Package.swift "$consumer/"
cp bindings/swift/Sources/LabColors/*.swift "$consumer/Sources/LabColors/"
cp "$work_root/generated/labcolors.swift" "$consumer/Sources/LabColors/"
cp "$work_root/generated/labcolorsFFI.h" "$consumer/Sources/labcolorsFFI/"
cp "$work_root/generated/labcolorsFFI.modulemap" "$consumer/Sources/labcolorsFFI/module.modulemap"
cp target/debug/liblabcolors.so "$consumer/lib/"
cp bindings/swift/Tests/LabColorsConformanceTests/*.swift "$consumer/Tests/LabColorsConformanceTests/"
readonly fixtures="$consumer/Tests/LabColorsConformanceTests/Fixtures"
cp conformance/vectors/*.json conformance/program-runtime-v1.json "$fixtures/"
printf '{"coreTree":"%s"}\n' "$(git rev-parse HEAD:crates/labcolors-core)" > "$fixtures/build-identity.json"

# Noble-производитель и Noble-потребитель используют один Linux ABI.
# Нет исходников Rust, Git checkout, Cargo или каталога сборки производителя.
"$docker" run --rm --user "$(id -u):$(id -g)" --workdir /consumer \
  --env "SWIFT_TOOLCHAIN=$SWIFT_TOOLCHAIN" --env HOME=/consumer \
  --mount "type=bind,src=$consumer,dst=/consumer" \
  "$SWIFT_IMAGE_V1" bash -euc '
    expected="Swift version ${SWIFT_TOOLCHAIN} (swift-${SWIFT_TOOLCHAIN}-RELEASE)"
    test "$(swift --version | sed -n "1p")" = "$expected"
    export LD_LIBRARY_PATH="/consumer/lib:${LD_LIBRARY_PATH:-}"
    swift test -Xlinker -L/consumer/lib -Xlinker -llabcolors
  '
