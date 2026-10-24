#!/usr/bin/env python3
"""Установить опубликованные байты в чистый Android-проект без Rust-исходников."""
from pathlib import Path, PurePosixPath
import hashlib
import json
import os
import shutil
import sys
import zipfile
from build import ROOT, digest, gradle, run


def extract(archive: Path, manifest: dict, target: Path) -> None:
    if digest(archive) != manifest["archiveSha256"]:
        raise ValueError("SDK archive checksum mismatch")
    with zipfile.ZipFile(archive) as bundle:
        names = bundle.namelist()
        if len(names) != len(set(names)) or set(names) != set(manifest["files"]):
            raise ValueError("SDK archive inventory mismatch")
        for entry in bundle.infolist():
            path = PurePosixPath(entry.filename)
            if path.is_absolute() or ".." in path.parts or "\\" in entry.filename or entry.is_dir() or path.as_posix() != entry.filename:
                raise ValueError("unsafe SDK archive path")
            if (entry.external_attr >> 16) & 0o170000 not in (0, 0o100000):
                raise ValueError("SDK entries must be regular files")
            data = bundle.read(entry)
            if hashlib.sha256(data).hexdigest() != manifest["files"][entry.filename]:
                raise ValueError("SDK file checksum mismatch")
            destination = target / path
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)


def main(distribution: Path, consumer: Path) -> None:
    manifest = json.loads((distribution / "android-release-manifest.json").read_text())
    if manifest.get("schemaVersion") != 1 or manifest.get("archive") != "LabColors.android.zip":
        raise ValueError("unknown Android distribution")
    if manifest["sourceCommit"] != run("git", "rev-parse", "HEAD", capture=True):
        raise ValueError("SDK source does not match this consumer run")
    if manifest["coreTree"] != run("git", "rev-parse", "HEAD:crates/labcolors-core", capture=True):
        raise ValueError("SDK has a different Core identity")
    shutil.copytree(ROOT / "bindings/kotlin/consumer", consumer)
    extract(distribution / manifest["archive"], manifest, consumer / "installed")
    assets = consumer / "src/androidTest/assets"
    assets.mkdir(parents=True)
    for path in (ROOT / "conformance/vectors").glob("*.json"):
        shutil.copy2(path, assets)
    shutil.copy2(ROOT / "conformance/program-runtime-v1.json", assets)
    for name in ["native-surface.json", "authoring-surface.json"]:
        shutil.copy2(distribution / name, assets)
    (assets / "build-metadata.json").write_text(json.dumps(manifest) + "\n")
    with (consumer / "gradle.properties").open("a") as out:
        out.write("publicationVersion=" + manifest["version"] + "\n")
    sdkmanager = Path(os.environ["ANDROID_HOME"]) / "cmdline-tools/latest/bin/sdkmanager"
    run(str(sdkmanager), "--install", "platforms;android-36", "build-tools;36.0.0")
    executable = gradle(consumer / "tools")
    (consumer / "gradle-path.txt").write_text(str(executable))
    print("Prepared external Android consumer of " + manifest["sourceCommit"])


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: prepare-consumer.py DISTRIBUTION NEW_CONSUMER")
    main(Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve())
