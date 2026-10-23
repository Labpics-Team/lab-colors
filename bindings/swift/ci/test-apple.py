#!/usr/bin/env python3
"""Проверить байты архива как независимый Swift-потребитель на macOS и iOS."""
from __future__ import annotations
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[3]


def run(*args: str, cwd: Path, capture: bool = False) -> str:
    result = subprocess.run(args, cwd=cwd, check=True, text=True,
                            stdout=subprocess.PIPE if capture else None)
    return result.stdout.strip() if capture else ""


def main(distribution: Path, consumer: Path) -> None:
    manifest = json.loads((distribution / "swift-release-manifest.json").read_text())
    if manifest.get("schemaVersion") != 1 or manifest.get("archive") != "LabColors.swiftpm.zip":
        raise ValueError("unknown SDK distribution schema")
    if manifest["sourceCommit"] != run("git", "rev-parse", "HEAD", cwd=ROOT, capture=True):
        raise ValueError("SDK was built from a different source revision")
    if manifest["coreTree"] != run("git", "rev-parse", "HEAD:crates/labcolors-core", cwd=ROOT, capture=True):
        raise ValueError("SDK has a different Core source tree")
    archive = distribution / manifest["archive"]
    with archive.open("rb") as stream:
        if hashlib.file_digest(stream, "sha256").hexdigest() != manifest["archiveSha256"]:
            raise ValueError("SDK archive digest mismatch")
    if consumer.exists():
        raise ValueError("consumer must be a fresh directory")
    consumer.mkdir(parents=True)
    extracted = consumer / "Dependencies"
    with zipfile.ZipFile(archive) as bundle:
        names = bundle.namelist()
        expected = {"LabColors/" + path for path in manifest["files"]}
        if len(names) != len(set(names)) or set(names) != expected:
            raise ValueError("SDK archive has a missing, duplicate or additional entry")
        for entry in bundle.infolist():
            parts = Path(entry.filename).parts
            if not parts or parts[0] != "LabColors" or ".." in parts or Path(entry.filename).is_absolute():
                raise ValueError("SDK path escapes its package")
            data = bundle.read(entry)
            relative = "/".join(parts[1:])
            if hashlib.sha256(data).hexdigest() != manifest["files"][relative]:
                raise ValueError("SDK entry digest mismatch")
            path = extracted / entry.filename
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    (consumer / "Package.swift").write_text('''// swift-tools-version:5.9
import PackageDescription
let package = Package(
    name: "Consumer",
    platforms: [.macOS("12.0"), .iOS("15.0")],
    products: [.library(name: "Consumer", targets: ["Consumer"])],
    dependencies: [.package(path: "Dependencies/LabColors")],
    targets: [
        .target(name: "Consumer", dependencies: [.product(name: "LabColors", package: "LabColors")]),
        .testTarget(name: "LabColorsConformanceTests",
            dependencies: ["Consumer", .product(name: "LabColors", package: "LabColors")],
            resources: [.copy("Fixtures")]),
    ]
)
''')
    source = consumer / "Sources/Consumer"
    source.mkdir(parents=True)
    (source / "Consumer.swift").write_text('''import Foundation
import LabColors
public func makeRuntime(_ bytes: Data) throws -> ProgramRuntime {
    try compileProgramWire(bytes: bytes, streamId: 1)
}
''')
    tests = consumer / "Tests/LabColorsConformanceTests"
    tests.mkdir(parents=True)
    for path in sorted((ROOT / "bindings/swift/Tests/LabColorsConformanceTests").glob("*.swift")):
        shutil.copy2(path, tests)
    fixtures = tests / "Fixtures"
    fixtures.mkdir()
    for path in sorted((ROOT / "conformance/vectors").glob("*.json")):
        shutil.copy2(path, fixtures)
    shutil.copy2(ROOT / "conformance/program-runtime-v1.json", fixtures)
    (fixtures / "build-identity.json").write_text(json.dumps({"coreTree": manifest["coreTree"]}) + "\n")
    # Сборка потребителя не видит Cargo/Rust и ничего не загружает из исходного repo.
    previous_path = os.environ["PATH"]
    os.environ["PATH"] = "/usr/bin:/bin:/usr/sbin:/sbin"
    try:
        run("xcrun", "swift", "test", cwd=consumer)
        run("xcodebuild", "-scheme", "Consumer", "-destination", "generic/platform=iOS",
            "CODE_SIGNING_ALLOWED=NO", "build-for-testing", cwd=consumer)
        runtimes = json.loads(run("xcrun", "simctl", "list", "runtimes", "--json", cwd=consumer, capture=True))["runtimes"]
        sdk_version = run("xcrun", "--sdk", "iphonesimulator", "--show-sdk-version", cwd=consumer, capture=True)
        supported = [runtime for runtime in runtimes if runtime.get("isAvailable")
                     and runtime.get("name", "").startswith("iOS ")
                     and runtime.get("version", "").split(".")[:2] == sdk_version.split(".")[:2]]
        if not supported:
            raise ValueError("no available iOS Simulator runtime")
        runtime = sorted(supported, key=lambda item: tuple(map(int, item["version"].split("."))))[-1]
        identifier = run("xcrun", "simctl", "create", "LabColorsConsumer", "com.apple.CoreSimulator.SimDeviceType.iPhone-16",
                         runtime["identifier"], cwd=consumer, capture=True)
        try:
            run("xcodebuild", "-scheme", "Consumer", "-destination", "id=" + identifier,
                "-parallel-testing-enabled", "NO", "CODE_SIGNING_ALLOWED=NO", "test", cwd=consumer)
        finally:
            subprocess.run(("xcrun", "simctl", "shutdown", identifier), cwd=consumer, check=False)
            run("xcrun", "simctl", "delete", identifier, cwd=consumer)
    finally:
        os.environ["PATH"] = previous_path
    print("Verified installed SDK on macOS and iOS Simulator; iOS device ABI built and linked.")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: test-apple.py DISTRIBUTION_DIRECTORY NEW_CONSUMER_DIRECTORY")
    main(Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve())
