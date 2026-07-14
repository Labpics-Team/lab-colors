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
apt-get install -y -qq curl build-essential jq pkg-config time >/dev/null
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

echo "==================== OBSERVATIONAL WHOLE-CALL EVIDENCE ===================="
# No latency or memory threshold is invented here. The test uses the Core-owned
# benchmark sample count for end-to-end Swift → UniFFI → protocol → Core calls;
# GNU time records process peak RSS for the already-built focused replay.
# Correctness and the no-oversize-copy law are executable assertions above.
evidence_dir=${LABCOLORS_SWIFT_EVIDENCE_DIR:-/work/target/swift-evidence}
mkdir -p "$evidence_dir"
whole_log="$evidence_dir/whole-call.log"
whole_time="$evidence_dir/whole-call.time"
extreme_log="$evidence_dir/extreme-shapes.log"
extreme_time="$evidence_dir/extreme-shapes.time"

/usr/bin/time -v -o "$whole_time" \
  swift test --skip-build \
    --filter ConformanceTests.testWcag22FeasibilityWholeCallObservation \
    -Xlinker -L/work/target/debug 2>&1 | tee "$whole_log"
/usr/bin/time -v -o "$extreme_time" \
  swift test --skip-build \
    --filter ConformanceTests.testWcag22FeasibilityExtremeShapeObservation \
    -Xlinker -L/work/target/debug 2>&1 | tee "$extreme_log"

whole_json=$(jq -Rn '
  def fields:
    split(" ")[1:] | map(split("=") | {(.[0]): .[1]}) | add;
  [inputs
   | select(startswith("LABCOLORS_FEASIBILITY_OBSERVATION "))
   | fields
   | {
       medianNs: (.whole_call_median_ns | tonumber),
       sampleCount: (.sample_count | tonumber),
       samplesNs: (.samples | split(",") | map(tonumber)),
       requestBytes: (.request_bytes | tonumber),
       maxRequestBytes: (.max_request_bytes | tonumber)
     }]
' <"$whole_log")
oversize_json=$(jq -Rn '
  def fields:
    split(" ")[1:] | map(split("=") | {(.[0]): .[1]}) | add;
  [inputs
   | select(startswith("LABCOLORS_FEASIBILITY_OVERSIZE "))
   | fields
   | {
       requestedBytes: (.requested_bytes | tonumber),
       maxRequestBytes: (.max_request_bytes | tonumber),
       ffiSubmittedBytes: (.ffi_submitted_bytes | tonumber),
       outputBytes: (.output_bytes | tonumber),
       rawCalls: (.raw_calls | tonumber),
       scalarCalls: (.scalar_calls | tonumber)
     }]
' <"$whole_log")
extreme_json=$(jq -Rn '
  def fields:
    split(" ")[1:] | map(split("=") | {(.[0]): .[1]}) | add;
  [inputs
   | select(startswith("LABCOLORS_FEASIBILITY_EXTREME "))
   | fields
   | {
       shape: .shape,
       rawRelations: (.raw_relations | tonumber),
       rawAdjacentEntries: (.raw_adjacent_entries | tonumber),
       opaqueUtf8Bytes: (.opaque_utf8_bytes | tonumber),
       wholeCallNs: (.whole_call_ns | tonumber),
       requestBytes: (.request_bytes | tonumber),
       ffiSubmittedBytes: (.ffi_submitted_bytes | tonumber),
       outputBytes: (.output_bytes | tonumber),
       rawCalls: (.raw_calls | tonumber),
       scalarCalls: (.scalar_calls | tonumber)
     }]
' <"$extreme_log")

peak_whole=$(awk -F: '/Maximum resident set size \(kbytes\)/ {
  gsub(/[[:space:]]/, "", $2); print $2
}' "$whole_time")
peak_extreme=$(awk -F: '/Maximum resident set size \(kbytes\)/ {
  gsub(/[[:space:]]/, "", $2); print $2
}' "$extreme_time")
peak_rss_kib=$peak_whole
if ((peak_extreme > peak_rss_kib)); then
  peak_rss_kib=$peak_extreme
fi

subject_sha=${LABCOLORS_SUBJECT_SHA:-local-unbound}
subject_bound=false
if [[ $subject_sha =~ ^[0-9a-f]{40}$ ]]; then
  subject_bound=true
fi
swift_container=${LABCOLORS_SWIFT_CONTAINER:-local-unbound}
swift_container_bound=false
if [[ $swift_container =~ @sha256:[0-9a-f]{64}$ ]]; then
  swift_container_bound=true
fi
evidence_mode=${LABCOLORS_EVIDENCE_MODE:-local-diagnostic}

evidence_json="$evidence_dir/uniffi-swift-observation-v1.json"
jq -n \
  --arg mode "$evidence_mode" \
  --arg subjectSha "$subject_sha" \
  --argjson subjectBound "$subject_bound" \
  --arg swiftContainer "$swift_container" \
  --argjson swiftContainerBound "$swift_container_bound" \
  --arg swiftVersion "$(swift --version | sed -n '1p')" \
  --arg rustToolchain "$RUST_TOOLCHAIN" \
  --arg packDigest "$(jq -r .packDigest ../../conformance/vectors/manifest.json)" \
  --arg manifestSha256 "$(sha256sum ../../conformance/vectors/manifest.json | awk '{print $1}')" \
  --arg feasibilityFamilySha256 "$(sha256sum ../../conformance/vectors/wcag22-feasibility.json | awk '{print $1}')" \
  --arg benchmarkSha256 "$(sha256sum ../../crates/labcolors-core/contracts/wcag22-feasibility-benchmark-v1.json | awk '{print $1}')" \
  --arg ffiSha256 "$(sha256sum ../../target/debug/liblabcolors.so | awk '{print $1}')" \
  --arg runnerKernel "$(uname -a)" \
  --argjson peakRssKib "$peak_rss_kib" \
  --argjson wholeCall "$whole_json" \
  --argjson oversizePreflight "$oversize_json" \
  --argjson extremeShapes "$extreme_json" \
  '{
    schemaVersion: 1,
    evidenceClass: "uniffi-swift-whole-call-observation-v1",
    mode: $mode,
    status: "observed-not-admitted",
    subject: {gitSha: $subjectSha, bound: $subjectBound},
    toolchain: {
      swiftContainer: $swiftContainer,
      swiftContainerBound: $swiftContainerBound,
      swiftVersion: $swiftVersion,
      rustToolchain: $rustToolchain,
      target: "x86_64-unknown-linux-gnu",
      runnerKernel: $runnerKernel
    },
    artifacts: {
      packDigest: $packDigest,
      manifestSha256: $manifestSha256,
      feasibilityFamilySha256: $feasibilityFamilySha256,
      benchmarkSha256: $benchmarkSha256,
      ffiSha256: $ffiSha256
    },
    contract: {
      latencyThresholdNs: null,
      peakRssThresholdKib: null,
      copyObservation: "one-admitted-submission-zero-oversize-submission",
      wholeCallLatencyScope: "after-envelope-construction-through-typed-outcome-decode",
      peakRssScope: "max-of-two-focused-swift-test-processes-including-swiftpm-xctest-and-preconstructed-envelopes"
    },
    wholeCall: $wholeCall,
    oversizePreflight: $oversizePreflight,
    extremeShapes: $extremeShapes,
    process: {peakRssKib: $peakRssKib}
  }' >"$evidence_json"

bash ci/check-observation.sh "$evidence_json" --mutation-test
cat "$evidence_json"

echo "==================== ГОТОВО: SWIFT CONFORMANCE ЗЕЛЁНЫЙ ===================="
