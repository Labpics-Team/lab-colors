#!/usr/bin/env bash
# Structural verifier for emit-first UniFFI/Swift whole-call evidence.
# It validates provenance and exact copy/shape laws only. Latency and memory
# remain observations: this checker deliberately rejects invented thresholds.
set -euo pipefail

readonly evidence=${1:?usage: check-observation.sh <evidence.json> [--mutation-test]}
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
readonly script_dir
repo_root=$(cd -- "$script_dir/../../.." && pwd)
readonly repo_root
readonly manifest="$repo_root/conformance/vectors/manifest.json"
readonly feasibility_family="$repo_root/conformance/vectors/wcag22-feasibility.json"
readonly benchmark="$repo_root/crates/labcolors-core/contracts/wcag22-feasibility-benchmark-v1.json"
readonly ffi_subject=${LABCOLORS_FFI_SUBJECT:-"$repo_root/target/debug/liblabcolors.so"}
readonly expected_mode=${LABCOLORS_EVIDENCE_MODE:-local-diagnostic}

live_pack_digest=$(jq -r .packDigest "$manifest")
readonly live_pack_digest
live_manifest_sha=$(sha256sum "$manifest" | awk '{print $1}')
readonly live_manifest_sha
live_family_sha=$(sha256sum "$feasibility_family" | awk '{print $1}')
readonly live_family_sha
live_benchmark_sha=$(sha256sum "$benchmark" | awk '{print $1}')
readonly live_benchmark_sha
live_ffi_sha=$(sha256sum "$ffi_subject" | awk '{print $1}')
readonly live_ffi_sha
live_sample_count=$(jq -r .sampleCount "$benchmark")
readonly live_sample_count
# Independent transport oracle: the compact V1 grammar contributes 101 fixed
# bytes, at most 115 bytes per raw relation, 14 per adjacent triple and six
# per opaque UTF-8 byte. Production owns the same ceiling in
# labcolors-protocol; this checker intentionally derives it from the public
# profile limits instead of trusting the observed Swift scalar.
live_max_request_bytes=$(jq -er '
  .profileLimits
  | 101
    + 115 * .rawRelations
    + 14 * .rawAdjacentEntries
    + 6 * .opaqueUtf8Bytes
' "$benchmark")
readonly live_max_request_bytes
live_max_relation_raw=$(jq -r '
  .scenarios[] | select(.name == "maximum-canonical-applicable-relations")
  | .shape.rawRelations
' "$benchmark")
readonly live_max_relation_raw
live_max_relation_adjacent=$(jq -r '
  .scenarios[] | select(.name == "maximum-canonical-applicable-relations")
  | .shape.rawAdjacentEntries
' "$benchmark")
readonly live_max_relation_adjacent
live_max_edge_raw=$(jq -r '
  .scenarios[] | select(.name == "maximum-applicable-edges")
  | .shape.rawRelations
' "$benchmark")
readonly live_max_edge_raw
live_max_edges=$(jq -r '
  .scenarios[] | select(.name == "maximum-applicable-edges")
  | .shape.rawAdjacentEntries
' "$benchmark")
readonly live_max_edges
live_max_opaque=$(jq -r '
  .scenarios[] | select(.name == "maximum-opaque-utf8-bytes")
  | .shape.opaqueUtf8Bytes
' "$benchmark")
readonly live_max_opaque
live_max_not_applicable_raw=$(jq -r '
  .scenarios[] | select(.name == "maximum-canonical-not-applicable-relations")
  | .shape.rawRelations
' "$benchmark")
readonly live_max_not_applicable_raw

jq -e \
  --arg expectedMode "$expected_mode" \
  --arg packDigest "$live_pack_digest" \
  --arg manifestSha256 "$live_manifest_sha" \
  --arg feasibilityFamilySha256 "$live_family_sha" \
  --arg benchmarkSha256 "$live_benchmark_sha" \
  --arg ffiSha256 "$live_ffi_sha" \
  --argjson sampleCount "$live_sample_count" \
  --argjson maxRequestBytes "$live_max_request_bytes" \
  --argjson maxRelationRaw "$live_max_relation_raw" \
  --argjson maxRelationAdjacent "$live_max_relation_adjacent" \
  --argjson maxEdgeRaw "$live_max_edge_raw" \
  --argjson maxEdges "$live_max_edges" \
  --argjson maxOpaque "$live_max_opaque" \
  --argjson maxNotApplicableRaw "$live_max_not_applicable_raw" '
  def exact_keys($expected): (keys | sort) == ($expected | sort);
  exact_keys([
    "artifacts", "contract", "evidenceClass", "extremeShapes", "mode",
    "oversizePreflight", "process", "schemaVersion", "status", "subject",
    "toolchain", "wholeCall"
  ]) and
  .schemaVersion == 1 and
  .evidenceClass == "uniffi-swift-whole-call-observation-v1" and
  .mode == $expectedMode and
  .status == "observed-not-admitted" and
  (.subject | exact_keys(["bound", "gitSha"])) and
  (.subject.bound | type == "boolean") and
  (if $expectedMode == "canonical-ci"
   then .subject.bound == true and (.subject.gitSha | test("^[0-9a-f]{40}$"))
   elif $expectedMode == "local-diagnostic"
   then .subject.bound == false and .subject.gitSha == "local-unbound"
   else false
   end) and
  (.toolchain | exact_keys([
    "runnerKernel", "rustToolchain", "swiftContainer", "swiftContainerBound",
    "swiftVersion", "target"
  ])) and
  (.toolchain.swiftContainerBound | type == "boolean") and
  (if $expectedMode == "canonical-ci"
   then .toolchain.swiftContainerBound == true and
        (.toolchain.swiftContainer | test("@sha256:[0-9a-f]{64}$"))
   else .toolchain.swiftContainerBound == false and
        .toolchain.swiftContainer == "local-unbound"
   end) and
  (.toolchain.swiftVersion | type == "string" and length > 0) and
  (.toolchain.runnerKernel | type == "string" and length > 0) and
  .toolchain.rustToolchain == "1.96.0" and
  .toolchain.target == "x86_64-unknown-linux-gnu" and
  (.artifacts | exact_keys([
    "benchmarkSha256", "feasibilityFamilySha256", "ffiSha256",
    "manifestSha256", "packDigest"
  ])) and
  .artifacts.packDigest == $packDigest and
  .artifacts.manifestSha256 == $manifestSha256 and
  .artifacts.feasibilityFamilySha256 == $feasibilityFamilySha256 and
  .artifacts.benchmarkSha256 == $benchmarkSha256 and
  .artifacts.ffiSha256 == $ffiSha256 and
  (.contract | exact_keys([
    "copyObservation", "latencyThresholdNs", "peakRssScope",
    "peakRssThresholdKib", "wholeCallLatencyScope"
  ])) and
  .contract.latencyThresholdNs == null and
  .contract.peakRssThresholdKib == null and
  .contract.copyObservation ==
    "one-admitted-submission-zero-oversize-submission" and
  .contract.wholeCallLatencyScope ==
    "after-envelope-construction-through-typed-outcome-decode" and
  .contract.peakRssScope ==
    "max-of-two-focused-swift-test-processes-including-swiftpm-xctest-and-preconstructed-envelopes" and
  (.wholeCall | length == 1) and
  (.wholeCall[0] | exact_keys([
    "maxRequestBytes", "medianNs", "requestBytes", "sampleCount", "samplesNs"
  ])) and
  .wholeCall[0].sampleCount == $sampleCount and
  (.wholeCall[0].samplesNs | length == $sampleCount) and
  all(.wholeCall[0].samplesNs[]; (type == "number") and . > 0) and
  (.wholeCall[0] as $call |
    ($call.samplesNs | sort | .[($sampleCount / 2 | floor)]) == $call.medianNs and
    ($call.requestBytes | type == "number") and $call.requestBytes > 0 and
    ($call.maxRequestBytes | type == "number") and
      $call.maxRequestBytes == $maxRequestBytes and
      $call.maxRequestBytes >= $call.requestBytes) and
  (.oversizePreflight | length == 1) and
  (.oversizePreflight[0] | exact_keys([
    "ffiSubmittedBytes", "maxRequestBytes", "outputBytes", "rawCalls",
    "requestedBytes", "scalarCalls"
  ])) and
  (.oversizePreflight[0] as $oversize |
    $oversize.maxRequestBytes == $maxRequestBytes and
    $oversize.requestedBytes == ($maxRequestBytes + 1) and
    $oversize.ffiSubmittedBytes == 0 and
    $oversize.outputBytes > 0 and
    $oversize.rawCalls == 0 and
    $oversize.scalarCalls == 1) and
  (.process | exact_keys(["peakRssKib"])) and
  (.process.peakRssKib | numbers) and .process.peakRssKib > 0 and
  (.extremeShapes | length == 4) and
  ([.extremeShapes[].shape] | sort) ==
    ([
      "maximum-applicable-edges",
      "maximum-canonical-applicable-relations",
      "maximum-canonical-not-applicable-relations",
      "maximum-opaque-utf8-bytes"
    ] | sort) and
  all(.extremeShapes[];
    exact_keys([
      "ffiSubmittedBytes", "outputBytes", "rawCalls", "requestBytes",
      "rawAdjacentEntries", "rawRelations", "opaqueUtf8Bytes", "scalarCalls",
      "shape", "wholeCallNs"
    ]) and
    (.rawRelations | type == "number") and .rawRelations > 0 and
    (.rawAdjacentEntries | type == "number") and .rawAdjacentEntries >= 0 and
    (.opaqueUtf8Bytes | type == "number") and .opaqueUtf8Bytes > 0 and
    (.wholeCallNs | numbers) and .wholeCallNs > 0 and
    (.requestBytes | numbers) and .requestBytes > 0 and
    .requestBytes <= $maxRequestBytes and
    (.outputBytes | numbers) and .outputBytes > 0 and
    .ffiSubmittedBytes == .requestBytes and
    .rawCalls == 1 and
    .scalarCalls == 0) and
  (.extremeShapes[] | select(
    .shape == "maximum-canonical-applicable-relations"
  ) | .rawRelations == $maxRelationRaw and
      .rawAdjacentEntries == $maxRelationAdjacent) and
  (.extremeShapes[] | select(.shape == "maximum-applicable-edges")
    | .rawRelations == $maxEdgeRaw and .rawAdjacentEntries == $maxEdges) and
  (.extremeShapes[] | select(.shape == "maximum-opaque-utf8-bytes")
    | .rawRelations == 1 and .rawAdjacentEntries == 0 and
      .opaqueUtf8Bytes == $maxOpaque) and
  (.extremeShapes[] | select(
    .shape == "maximum-canonical-not-applicable-relations"
  ) | .rawRelations == $maxNotApplicableRaw and .rawAdjacentEntries == 0)
' "$evidence" >/dev/null

if [[ ${2:-} == "--mutation-test" ]]; then
  tmp_dir=$(mktemp -d)
  trap 'rm -rf "$tmp_dir"' EXIT

  reject_mutation() {
    local name=$1
    local filter=$2
    local mutated="$tmp_dir/$name.json"
    jq "$filter" "$evidence" >"$mutated"
    if "$0" "$mutated" >/dev/null 2>&1; then
      echo "observation checker accepted mutation: $name" >&2
      exit 1
    fi
  }

  reject_mutation wrong-schema '.schemaVersion = 2'
  reject_mutation invented-latency-threshold '.contract.latencyThresholdNs = 1'
  reject_mutation latency-scope '.contract.wholeCallLatencyScope = "includes-construction"'
  reject_mutation copy-drift '.extremeShapes[0].ffiSubmittedBytes += 1'
  reject_mutation oversize-copy '.oversizePreflight[0].rawCalls = 1'
  reject_mutation derived-ceiling-drift \
    '.wholeCall[0].maxRequestBytes += 1 | .oversizePreflight[0].maxRequestBytes += 1 | .oversizePreflight[0].requestedBytes += 1'
  reject_mutation extreme-over-ceiling \
    '.extremeShapes[0].requestBytes = (.wholeCall[0].maxRequestBytes + 1) | .extremeShapes[0].ffiSubmittedBytes = .extremeShapes[0].requestBytes'
  reject_mutation sample-count '.wholeCall[0].sampleCount += 1'
  reject_mutation shape-count '.extremeShapes[0].rawRelations += 1'
  reject_mutation missing-shape '.extremeShapes |= .[0:3]'
  reject_mutation family-binding '.artifacts.feasibilityFamilySha256 = ("0" * 64)'
  reject_mutation extra-field '.contract.extra = true'
  reject_mutation provenance-bound-flip '.subject.bound = (.subject.bound | not)'
  reject_mutation false-admission '.status = "admitted"'
  reject_mutation zero-peak '.process.peakRssKib = 0'
fi
