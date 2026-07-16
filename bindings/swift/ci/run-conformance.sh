#!/usr/bin/env bash
# Прогон Swift-conformance в swift-контейнере (Linux x86_64) — ЕДИНЫЙ источник
# для локального прогона и self-hosted CI-джобы (native-conformance.yml).
#
# Заменяет платную macOS-джобу (GitHub-hosted macOS исключён владельцем
# навсегда). Ядро Rust компилируется под Linux, uniffi-bindgen генерит Swift из
# .so, swift test прогоняет conformance-пак против выхода FFI.
#
# Ожидает исходники репозитория в /src (read-only bind-mount); собирает в /work.
# Запуск:
#   docker run --rm -v "<repo>":/src:ro \
#     swift:6.1.3@sha256:e1cdaf7ddc9de37d8561da7a260535236694fca8c1b67d3129d47d8b180a9394 \
#     bash /src/bindings/swift/ci/run-conformance.sh
set -euo pipefail

readonly RUST_TOOLCHAIN=1.96.0

echo "==================== ПЛАТФОРМА / SWIFT ===================="
uname -a
swift --version

echo "==================== УСТАНОВКА RUST ===================="
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq curl build-essential pkg-config >/dev/null
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- \
  -y --default-toolchain "$RUST_TOOLCHAIN" --profile minimal
# shellcheck disable=SC1091
source "$HOME/.cargo/env"
rustc +"$RUST_TOOLCHAIN" --version
cargo +"$RUST_TOOLCHAIN" --version

echo "==================== КОПИЯ ИСХОДНИКОВ (без target/.git) ===================="
mkdir -p /work
tar -C /src \
  --exclude=target --exclude=.git --exclude=node_modules \
  --exclude='mutants.out' --exclude='mutants.out.old' \
  --exclude='bindings/swift/.build' --exclude='bindings/swift/generated' \
  -cf - . | tar -C /work -xf -
cd /work

echo "==================== СБОРКА labcolors-ffi (Linux) + bindgen ===================="
cargo +"$RUST_TOOLCHAIN" build -p labcolors-ffi --features cli --locked
ls -la target/debug/liblabcolors.*

echo "==================== РАННЕР-РЕФЕРЕНС (ядро на Linux воспроизводит пак) ===================="
cargo +"$RUST_TOOLCHAIN" test -p labcolors-conformance -p labcolors-ffi --locked

echo "==================== ГЕНЕРАЦИЯ SWIFT-БИНДИНГОВ (из .so) ===================="
cargo +"$RUST_TOOLCHAIN" run -p labcolors-ffi --features cli --bin uniffi-bindgen --locked -- \
  generate --library target/debug/liblabcolors.so \
  --language swift --out-dir bindings/swift/generated

echo "==================== РАСКЛАДКА В SwiftPM ===================="
gen=bindings/swift/generated
mkdir -p bindings/swift/Sources/LabColors bindings/swift/Sources/labcolorsFFI
cp "$gen/labcolors.swift"        bindings/swift/Sources/LabColors/labcolors.swift
cp "$gen/labcolorsFFI.h"         bindings/swift/Sources/labcolorsFFI/labcolorsFFI.h
cp "$gen/labcolorsFFI.modulemap" bindings/swift/Sources/labcolorsFFI/module.modulemap
test -f bindings/swift/Sources/LabColors/Wcag22FeasibilityProtocol.swift
echo "--- module.modulemap ---"; cat bindings/swift/Sources/labcolorsFFI/module.modulemap

echo "==================== SWIFT TEST (пак против FFI) ===================="
cd bindings/swift
# .so линкуется динамически (-llabcolors находит liblabcolors.so в -L); .so ищем
# в рантайме через LD_LIBRARY_PATH.
export LD_LIBRARY_PATH="/work/target/debug:${LD_LIBRARY_PATH:-}"
swift test -Xlinker -L/work/target/debug

echo "==================== ГОТОВО: SWIFT CONFORMANCE ЗЕЛЁНЫЙ ===================="
