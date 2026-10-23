#!/usr/bin/env python3
"""Собрать самостоятельный SwiftPM-пакет из точной ревизии, без Rust у потребителя."""
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
MINIMUM = {"macOS": "12.0", "iOS": "15.0"}
SLICES = {
    "macos": ("aarch64-apple-darwin", "x86_64-apple-darwin"),
    "ios": ("aarch64-apple-ios",),
    "ios-simulator": ("aarch64-apple-ios-sim", "x86_64-apple-ios"),
}


def run(*args: str, capture: bool = False, cwd: Path = ROOT) -> str:
    result = subprocess.run(args, cwd=cwd, check=True, text=True,
                            stdout=subprocess.PIPE if capture else None)
    return result.stdout.strip() if capture else ""


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def main(output: Path) -> None:
    if sys.platform != "darwin":
        raise ValueError("Apple SDK assembly requires macOS and Xcode")
    if output.exists() and any(entry.name != "target" for entry in output.iterdir()):
        raise ValueError("output may contain only the restored Cargo target cache")
    toolchain = os.environ["RUST_TOOLCHAIN"]
    output.mkdir(parents=True, exist_ok=True)
    target = output / "target"
    os.environ.update(CARGO_TARGET_DIR=str(target), MACOSX_DEPLOYMENT_TARGET=MINIMUM["macOS"],
                      IPHONEOS_DEPLOYMENT_TARGET=MINIMUM["iOS"])
    revision = run("git", "rev-parse", "HEAD", capture=True)
    core_tree = run("git", "rev-parse", "HEAD:crates/labcolors-core", capture=True)
    if run("git", "status", "--porcelain", "--untracked-files=no", capture=True):
        raise ValueError("SDK source must be a clean Git revision")
    triples = [triple for values in SLICES.values() for triple in values]
    run("rustup", "target", "add", "--toolchain", toolchain, *triples)
    os.environ["LABCOLORS_NATIVE_SURFACE_OUTPUT"] = str(output / "native-surface.json")
    run("cargo", f"+{toolchain}", "test", "--locked", "-p", "labcolors-audit", "--test", "native_surface")
    del os.environ["LABCOLORS_NATIVE_SURFACE_OUTPUT"]
    build_packages: set[str] = set()
    for triple in triples:
        messages = run("cargo", f"+{toolchain}", "build", "--release", "--lib", "--locked",
                       "-p", "labcolors-ffi", "--target", triple,
                       "--message-format=json-render-diagnostics", capture=True)
        for line in messages.splitlines():
            message = json.loads(line)
            if message.get("reason") == "compiler-artifact":
                build_packages.add(message["package_id"])
    package_inventory = output / "build-packages.json"
    package_inventory.write_text(json.dumps(sorted(build_packages)) + "\n")
    host = next(line[6:] for line in run("rustc", f"+{toolchain}", "-vV", capture=True).splitlines()
                if line.startswith("host: "))
    generated = output / "generated"
    run("cargo", f"+{toolchain}", "run", "--locked", "-p", "labcolors-ffi", "--features", "cli",
        "--bin", "uniffi-bindgen", "--", "generate", "--library",
        str(target / host / "release/liblabcolors.dylib"), "--language", "swift", "--out-dir", str(generated))
    headers = output / "headers"
    headers.mkdir()
    shutil.copy2(generated / "labcolorsFFI.h", headers)
    shutil.copy2(generated / "labcolorsFFI.modulemap", headers / "module.modulemap")
    sdk = output / "LabColors"
    sources = sdk / "Sources/LabColors"
    sources.mkdir(parents=True)
    shutil.copy2(generated / "labcolors.swift", sources)
    for path in sorted((ROOT / "bindings/swift/Sources/LabColors").glob("*.swift")):
        if path.name != "labcolors.swift":
            shutil.copy2(path, sources)
    libraries: list[str] = []
    for name, targets in SLICES.items():
        directory = output / "slices" / name
        directory.mkdir(parents=True)
        library = directory / "liblabcolors.a"
        inputs = [str(target / triple / "release/liblabcolors.a") for triple in targets]
        if len(inputs) == 1:
            shutil.copy2(inputs[0], library)
        else:
            run("xcrun", "lipo", "-create", *inputs, "-output", str(library))
        libraries.extend(("-library", str(library), "-headers", str(headers)))
    run("xcodebuild", "-create-xcframework", *libraries, "-output", str(sdk / "labcolorsFFI.xcframework"))
    (sdk / "Package.swift").write_text(f'''// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "LabColors",
    platforms: [.macOS("{MINIMUM['macOS']}"), .iOS("{MINIMUM['iOS']}")],
    products: [.library(name: "LabColors", targets: ["LabColors"])],
    targets: [
        .binaryTarget(name: "labcolorsFFI", path: "labcolorsFFI.xcframework"),
        .target(name: "LabColors", dependencies: ["labcolorsFFI"], path: "Sources/LabColors"),
    ]
)
''')
    shutil.copy2(ROOT / "LICENSE", sdk)
    shutil.copy2(ROOT / "bindings/swift/DISTRIBUTION.md", sdk / "README.md")
    run("node", "bindings/swift/ci/licenses.mjs", str(sdk), str(package_inventory))
    metadata = {"schemaVersion": 1, "sourceCommit": revision, "coreTree": core_tree,
                "rustToolchain": toolchain, "xcode": run("xcodebuild", "-version", capture=True),
                "swift": run("swift", "--version", capture=True), "minimumPlatforms": MINIMUM,
                "slices": {name: list(targets) for name, targets in SLICES.items()}}
    (sdk / "build-metadata.json").write_text(json.dumps(metadata, indent=2, sort_keys=True) + "\n")
    files = {}
    archive = output / "LabColors.swiftpm.zip"
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
        for path in sorted(sdk.rglob("*")):
            if path.is_symlink():
                raise ValueError(f"SDK must not contain a symlink: {path.relative_to(sdk)}")
            if path.is_file():
                relative = path.relative_to(sdk).as_posix()
                files[relative] = digest(path)
                info = zipfile.ZipInfo("LabColors/" + relative, date_time=(1980, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = 0o100644 << 16
                bundle.writestr(info, path.read_bytes())
    manifest = {**metadata, "archive": archive.name, "archiveSha256": digest(archive), "files": files}
    (output / "swift-release-manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    (output / "SHA256SUMS").write_text(f"{digest(archive)}  {archive.name}\n")
    print(f"SDK source {revision}: {archive}")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: build-apple.py NEW_OUTPUT_DIRECTORY")
    main(Path(sys.argv[1]).resolve())
