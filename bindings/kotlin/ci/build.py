#!/usr/bin/env python3
"""Собрать Android AAR и Maven-метаданные из одной чистой ревизии."""
from __future__ import annotations
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tomllib
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[3]
GRADLE_VERSION = "8.13"
GRADLE_SHA256 = "20f1b1176237254a6fc204d8434196fa11a4cfb387567519c61556e8710aed78"
NDK = "28.2.13676358"
ABIS = {
    "arm64-v8a": ("aarch64-linux-android", "aarch64-linux-android"),
    "armeabi-v7a": ("armv7-linux-androideabi", "armv7a-linux-androideabi"),
    "x86": ("i686-linux-android", "i686-linux-android"),
    "x86_64": ("x86_64-linux-android", "x86_64-linux-android"),
}


def run(*args: str, cwd: Path = ROOT, capture: bool = False, env: dict | None = None) -> str:
    result = subprocess.run(args, cwd=cwd, env=env, check=True, text=True,
                            stdout=subprocess.PIPE if capture else None)
    return result.stdout.strip() if capture else ""


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def gradle(directory: Path) -> Path:
    archive = directory / "gradle.zip"
    directory.mkdir(parents=True, exist_ok=True)
    with urllib.request.urlopen(f"https://services.gradle.org/distributions/gradle-{GRADLE_VERSION}-bin.zip", timeout=120) as response:
        with archive.open("wb") as stream:
            shutil.copyfileobj(response, stream)
    if digest(archive) != GRADLE_SHA256:
        raise ValueError("Gradle distribution checksum mismatch")
    with zipfile.ZipFile(archive) as bundle:
        bundle.extractall(directory)
    archive.unlink()
    executable = directory / f"gradle-{GRADLE_VERSION}/bin/gradle"
    executable.chmod(0o755)
    return executable


def main(output: Path) -> None:
    if sys.platform != "linux" or (output.exists() and any(path.name != "target" for path in output.iterdir())):
        raise ValueError("Android producer needs Linux; output may contain only the restored Cargo cache")
    if run("git", "status", "--porcelain", "--untracked-files=no", capture=True):
        raise ValueError("source revision must be clean")
    output.mkdir(parents=True, exist_ok=True)
    toolchain = os.environ["RUST_TOOLCHAIN"]
    sdk = Path(os.environ["ANDROID_HOME"])
    sdkmanager = sdk / "cmdline-tools/latest/bin/sdkmanager"
    run(str(sdkmanager), "--install", "platforms;android-36", "build-tools;36.0.0", "ndk;" + NDK)
    compiler = sdk / "ndk" / NDK / "toolchains/llvm/prebuilt/linux-x86_64/bin"
    environment = dict(os.environ, CARGO_TARGET_DIR=str(output / "target"))
    run("rustup", "target", "add", "--toolchain", toolchain, *(row[0] for row in ABIS.values()))
    # ELF release stripping removes bindgen metadata, so generation uses an
    # unstripped host build; published Android libraries remain release builds.
    run("cargo", "+" + toolchain, "build", "--locked", "-p", "labcolors-ffi", "--features", "cli", env=environment)
    project = output / "producer"
    shutil.copytree(ROOT / "bindings/kotlin", project,
                    ignore=shutil.ignore_patterns("ci", "consumer", "README.md", ".gradle", "build"))
    run("cargo", "+" + toolchain, "run", "--locked", "-p", "labcolors-ffi", "--features", "cli",
        "--bin", "uniffi-bindgen", "--", "generate", "--library", str(output / "target/debug/liblabcolors.so"),
        "--language", "kotlin", "--config", str(project / "uniffi.toml"), "--out-dir", str(project / "generated"), env=environment)
    packages: set[str] = set()
    for abi, (target, clang_target) in ABIS.items():
        key = target.upper().replace("-", "_")
        build_env = dict(environment)
        build_env["CARGO_TARGET_" + key + "_LINKER"] = str(compiler / (clang_target + "23-clang"))
        build_env["CARGO_TARGET_" + key + "_RUSTFLAGS"] = "-C link-arg=-Wl,-z,max-page-size=16384 -C link-arg=-Wl,-z,common-page-size=16384"
        messages = run("cargo", "+" + toolchain, "build", "--locked", "--release", "--lib", "-p", "labcolors-ffi",
                       "--target", target, "--message-format=json-render-diagnostics", capture=True, env=build_env)
        for line in messages.splitlines():
            item = json.loads(line)
            if item.get("reason") == "compiler-artifact":
                packages.add(item["package_id"])
        destination = project / "jniLibs" / abi / "liblabcolors.so"
        destination.parent.mkdir(parents=True)
        shutil.copy2(output / "target" / target / "release/liblabcolors.so", destination)
        run(str(compiler / "llvm-strip"), "--strip-unneeded", str(destination))
    package_inventory = output / "build-packages.json"
    package_inventory.write_text(json.dumps(sorted(packages)) + "\n")
    notices = project / "src/main/assets/labcolors"
    notices.mkdir(parents=True)
    run("node", "scripts/native-distribution-licenses.mjs", str(notices), str(package_inventory), env=environment)
    core_version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    source_commit = run("git", "rev-parse", "HEAD", capture=True)
    # CI-архивы разных ревизий не разделяют изменяемую Maven-координату.
    version = core_version + "-" + source_commit
    metadata = {"schemaVersion": 1, "version": version,
                "sourceCommit": source_commit, "coreVersion": core_version,
                "coreTree": run("git", "rev-parse", "HEAD:crates/labcolors-core", capture=True),
                "minimumApi": 23, "abis": sorted(ABIS), "ndk": NDK,
                "rustToolchain": toolchain, "gradle": GRADLE_VERSION}
    (notices / "build-metadata.json").write_text(json.dumps(metadata, sort_keys=True) + "\n")
    executable = gradle(output / "tools")
    publication = output / "publication"
    run(str(executable), "--no-daemon", "--max-workers=2", "publish", "-PpublicationVersion=" + version,
        "-PpublicationRepository=" + str(publication / "repository"), cwd=project)
    shutil.copy2(ROOT / "bindings/kotlin/README.md", publication)
    shutil.copy2(ROOT / "LICENSE", publication)
    for path in notices.iterdir():
        shutil.copy2(path, publication)
    aar = publication / "repository/pics/lab/colors-android" / version / f"colors-android-{version}.aar"
    with zipfile.ZipFile(aar) as bundle:
        libraries = {path for path in bundle.namelist() if path.endswith(".so")}
        if libraries != {f"jni/{abi}/liblabcolors.so" for abi in ABIS}:
            raise ValueError("AAR ABI inventory differs from the built product")
        for abi in ABIS:
            if bundle.read(f"jni/{abi}/liblabcolors.so") != (project / "jniLibs" / abi / "liblabcolors.so").read_bytes():
                raise ValueError("AAR changed a compiled native library")
    run("cargo", "+" + toolchain, "test", "--locked", "-p", "labcolors-audit", "--test", "native_surface",
        env=dict(environment, LABCOLORS_NATIVE_SURFACE_OUTPUT=str(output / "native-surface.json")))
    run("node", "scripts/emit-authoring-surface.mjs", str(output / "authoring-surface.json"))
    files = {}
    archive = output / "LabColors.android.zip"
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
        for path in sorted(publication.rglob("*")):
            if path.is_symlink():
                raise ValueError("publication must not contain symlinks")
            if path.is_file():
                name = path.relative_to(publication).as_posix()
                files[name] = digest(path)
                info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = 0o100644 << 16
                bundle.writestr(info, path.read_bytes())
    manifest = {**metadata, "archive": archive.name, "archiveSha256": digest(archive), "files": files}
    (output / "android-release-manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    (output / "SHA256SUMS").write_text(digest(archive) + "  " + archive.name + "\n")
    print("Android SDK built from " + metadata["sourceCommit"])


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: build.py NEW_OUTPUT_DIRECTORY")
    main(Path(sys.argv[1]).resolve())
