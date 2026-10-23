#!/usr/bin/env python3
"""Сравнить два настоящих Swift builders в одном внешнем release-потребителе."""
from __future__ import annotations
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
SOURCE = "bindings/swift/Sources/LabColors/ProgramWireBuilder.swift"


def run(*args: str, cwd: Path = ROOT, capture: bool = False) -> str:
    result = subprocess.run(args, cwd=cwd, check=True, text=True,
                            stdout=subprocess.PIPE if capture else None)
    return result.stdout if capture else ""


def main(consumer: Path, baseline: str) -> None:
    if not re.fullmatch(r"[0-9a-f]{40}", baseline):
        raise ValueError("baseline must be an exact Git commit")
    run("git", "fetch", "--no-tags", "--depth=1", "origin", baseline)
    old = run("git", "show", baseline + ":" + SOURCE, capture=True)
    package = consumer / "Package.swift"
    text = package.read_text()
    marker = '    targets: [\n'
    if text.count(marker) != 1:
        raise ValueError("expected one external consumer target list")
    text = text.replace(marker, marker + '''        .target(name: "BaselineAuthoring", dependencies: [.product(name: "LabColors", package: "LabColors")]),
        .executableTarget(name: "AuthoringBenchmark", dependencies: ["BaselineAuthoring", .product(name: "LabColors", package: "LabColors")]),
''')
    package.write_text(text)
    previous = consumer / "Sources/BaselineAuthoring"
    previous.mkdir()
    (previous / "ProgramWireBuilder.swift").write_text("import LabColors\n" + old)
    target = consumer / "Sources/AuthoringBenchmark"
    target.mkdir()
    workload = '''
@inline(never) func NAME(_ count: Int) throws -> Data {
    let b = MODULE.ProgramWireBuilderV1()
    try b.surfaceInputPort(1).inputSurface(1, input: 1)
    for index in 1...count {
        let id = UInt32(index)
        try b.source(id, rgb: [80, 80, 80]).fixedTarget(id, source: id).solidPaint(id, target: id)
            .sourceOverOccurrence(id, subject: id, against: 1, adaptingLuminance: 64, backgroundRatio: 0.2, surround: .average)
            .presentationRoot(id, terminal: id).presentationTarget(id, occurrence: id)
            .wcag22VisibleUnary(true, id: id, occurrence: id, criterion: .sc1411UiComponentOrState)
            .output(id, paint: id)
    }
    return try b.finish()
}
'''
    swift = '''import Foundation
import LabColors
import BaselineAuthoring
''' + workload.replace("NAME", "baseline").replace("MODULE", "BaselineAuthoring") + workload.replace("NAME", "current").replace("MODULE", "LabColors") + '''
func timed(_ body: () throws -> Data, iterations: Int, expected: Data) throws -> Double {
    let begin = DispatchTime.now().uptimeNanoseconds
    for _ in 0..<iterations {
        let result = try body()
        precondition(result == expected, "encoding changed during the measured workload")
    }
    return Double(DispatchTime.now().uptimeNanoseconds - begin) / Double(iterations)
}
func median(_ values: [Double]) -> Double { values.sorted()[values.count / 2] }
var rows: [[String: Any]] = []
for count in [1, 32, 256, 4096] {
    let expected = try baseline(count)
    let actual = try current(count)
    precondition(actual == expected, "canonical bytes changed")
    let iterations = max(4, 4096 / count)
    var before: [Double] = [], after: [Double] = []
    for round in 0..<11 {
        var a = 0.0, b = 0.0
        if round.isMultiple(of: 2) {
            a = try timed({ try baseline(count) }, iterations: iterations, expected: expected)
            b = try timed({ try current(count) }, iterations: iterations, expected: expected)
        } else {
            b = try timed({ try current(count) }, iterations: iterations, expected: expected)
            a = try timed({ try baseline(count) }, iterations: iterations, expected: expected)
        }
        if round >= 2 { before.append(a); after.append(b) }
    }
    rows.append(["outputs": count, "bytes": expected.count, "iterationsPerRound": iterations,
                 "baselineNs": before, "currentNs": after,
                 "medianSpeedup": median(zip(before, after).map { pair in pair.0 / pair.1 })])
}
let result: [String: Any] = ["workload": "swift-authoring-create-encode-release-v1", "rows": rows]
print(String(decoding: try JSONSerialization.data(withJSONObject: result, options: [.sortedKeys]), as: UTF8.self))
'''
    (target / "main.swift").write_text(swift)
    raw = run("xcrun", "swift", "run", "-c", "release", "AuthoringBenchmark", cwd=consumer, capture=True)
    result = json.loads(raw)
    result.update(baseline=baseline, current=run("git", "rev-parse", "HEAD", capture=True).strip(),
                  swift=run("xcrun", "swift", "--version", capture=True).strip(),
                  scope="authoring plus byte equality and lifetime; excludes Core evaluation and UI rendering")
    print(json.dumps(result, indent=2))
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a") as out:
            out.write("\n### Swift authoring, alternating release builds\n\n")
            out.write("Baseline `" + baseline + "`; current `" + result["current"] + "`.\n\n")
            for row in result["rows"]:
                out.write(f"{row['outputs']} outputs: {row['medianSpeedup']:.3f}×; {row['bytes']} canonical bytes.\n\n")
            out.write("Scope: authoring, byte equality and cleanup. Not a whole-engine or UI speedup.\n")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: benchmark-authoring.py EXTERNAL_CONSUMER BASELINE_COMMIT")
    main(Path(sys.argv[1]).resolve(), sys.argv[2])
