#!/usr/bin/env python3
"""Передать один собранный Android consumer двум средам исполнения."""
from __future__ import annotations

import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import sys

from build import ROOT, digest, run


def artifact_inventory(outputs: Path, stages: dict) -> dict:
    """Один договор полноты APK-пар и результатов R8, включая частичный отказ."""
    categories = {}
    missing = []
    for variant in ("debug", "release"):
        required = {"app-apk": list((outputs / "apk" / variant).glob("*.apk")),
                    "test-apk": list((outputs / "apk/androidTest" / variant).glob("*.apk"))}
        if variant == "release":
            for name in ("mapping.txt", "configuration.txt", "usage.txt", "seeds.txt"):
                required[name] = [outputs / "mapping/release" / name]
        for category, paths in required.items():
            files = [path for path in paths if path.is_file() and
                     (path.stat().st_size or category in ("usage.txt", "seeds.txt"))]
            stage = stages[variant]["test" if category == "test-apk" else "app"]
            status = "not-built" if stage != "assembled" else "present" if files else "missing"
            key = variant + "/" + category
            categories[key] = {"status": status,
                               "files": [path.relative_to(outputs).as_posix() for path in files]}
            if status == "missing":
                missing.append(key)
    return {"stages": stages, "categories": categories, "missing": missing}


def _linked(path: Path) -> bool:
    return path.is_symlink() or (hasattr(path, "is_junction") and path.is_junction())


def _files(directory: Path) -> dict[str, str]:
    if _linked(directory) or not directory.is_dir():
        raise ValueError("consumer outputs must be a regular directory")
    files = {}

    def failure(error):
        raise error

    # glob suppresses directory read errors; an incomplete inventory cannot seal.
    for parent, directories, names in os.walk(directory, onerror=failure):
        for name in directories + names:
            if _linked(Path(parent) / name):
                raise ValueError("consumer artifacts must not contain filesystem links")
        for name in names:
            path = Path(parent) / name
            if not path.is_file():
                raise ValueError("consumer artifacts must contain only regular files")
            files[path.relative_to(directory).as_posix()] = digest(path)
    return files


def _json(path: Path) -> dict:
    if _linked(path) or not path.is_file():
        raise ValueError("consumer manifest must be a regular file")

    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError("duplicate consumer manifest key")
            result[key] = value
        return result

    value = json.loads(path.read_text(), object_pairs_hook=unique)
    if not isinstance(value, dict):
        raise ValueError("consumer manifest must be an object")
    return value


def _source_identity() -> dict[str, str]:
    if run("git", "status", "--porcelain", "--untracked-files=all", cwd=ROOT, capture=True):
        raise ValueError("consumer source revision must be clean")
    return {"sourceCommit": run("git", "rev-parse", "HEAD", cwd=ROOT, capture=True),
            "sourceTree": run("git", "rev-parse", "HEAD^{tree}", cwd=ROOT, capture=True),
            "coreTree": run("git", "rev-parse", "HEAD:crates/labcolors-core", cwd=ROOT, capture=True)}


def _sdk_identity(sdk: dict, source: dict) -> None:
    if sdk.get("schemaVersion") != 1 or sdk.get("archive") != "LabColors.android.zip":
        raise ValueError("unknown Android SDK distribution")
    if sdk.get("sourceCommit") != source["sourceCommit"] or sdk.get("coreTree") != source["coreTree"]:
        raise ValueError("SDK source identity does not match the checked-out consumer")
    if not isinstance(sdk.get("archiveSha256"), str) or not re.fullmatch("[0-9a-f]{64}", sdk["archiveSha256"]):
        raise ValueError("invalid SDK archive identity")


def require_assembled(outputs: Path) -> None:
    stages = _json(outputs / "consumer-diagnostics/assembly-stages.json")
    expected = {variant: {"app": "assembled", "test": "assembled"}
                for variant in ("debug", "release")}
    if stages != expected:
        raise ValueError("external consumer assembly did not complete")
    inventory = artifact_inventory(outputs, stages)
    if inventory["missing"]:
        raise ValueError("missing required consumer artifacts: " + ", ".join(inventory["missing"]))
    if not (outputs / "mapping/release/mapping.txt").read_text().strip():
        raise ValueError("release consumer did not run R8")
    for variant in stages:
        for kind in ("app-apk", "test-apk"):
            if len(inventory["categories"][variant + "/" + kind]["files"]) != 1:
                raise ValueError("expected one assembled application/test APK pair per variant")
        evidence = outputs / "consumer-diagnostics" / variant
        for configuration in (variant + "RuntimeClasspath", variant + "AndroidTestRuntimeClasspath"):
            report = evidence / (configuration + ".txt")
            if not report.is_file() or not report.stat().st_size or report.stat().st_size > 1 << 20 or "FAILED" in report.read_text():
                raise ValueError("consumer dependency identity is incomplete")
    linkage = outputs / "consumer-diagnostics/release/desugar-linkage.txt"
    if not linkage.is_file() or not linkage.read_text().strip():
        raise ValueError("release desugar linkage evidence is missing")


def seal(consumer: Path, distribution: Path, destination: Path) -> None:
    source = _source_identity()
    sdk_manifest = distribution / "android-release-manifest.json"
    sdk = _json(sdk_manifest)
    sdk_manifest_sha = digest(sdk_manifest)
    _sdk_identity(sdk, source)
    archive = distribution / sdk["archive"]
    if _linked(archive) or not archive.is_file() or digest(archive) != sdk["archiveSha256"]:
        raise ValueError("SDK archive checksum mismatch")
    outputs = consumer / "build/outputs"
    files = _files(outputs)
    require_assembled(outputs)
    destination.mkdir(parents=True, exist_ok=False)
    owned_destination = destination.resolve()
    try:
        shutil.copytree(outputs, destination / "outputs")
        shutil.copyfile(sdk_manifest, destination / "sdk-manifest.json")
        if _files(destination / "outputs") != files:
            raise ValueError("consumer outputs changed during transfer")
        if digest(destination / "sdk-manifest.json") != sdk_manifest_sha:
            raise ValueError("SDK manifest changed during transfer")
        manifest = {"schemaVersion": 1, **source,
                    "sdkManifestSha256": sdk_manifest_sha,
                    "sdkArchiveSha256": sdk["archiveSha256"],
                    "files": files}
        (destination / "manifest.json").write_text(json.dumps(manifest, sort_keys=True) + "\n")
        verify_artifact(destination)
    except BaseException:
        if _linked(destination) or destination.resolve() != owned_destination:
            raise RuntimeError("consumer artifact cleanup target changed")
        shutil.rmtree(destination)
        raise


def verify_artifact(root: Path) -> dict:
    """Проверить перед установкой и после исполнения именно переданные байты."""
    if _linked(root) or not root.is_dir():
        raise ValueError("consumer artifact must be a regular directory")
    manifest = _json(root / "manifest.json")
    if manifest.get("schemaVersion") != 1:
        raise ValueError("unknown consumer artifact")
    source = _source_identity()
    if any(manifest.get(key) != value for key, value in source.items()):
        raise ValueError("consumer source identity does not match this run")
    sdk = _json(root / "sdk-manifest.json")
    _sdk_identity(sdk, source)
    if digest(root / "sdk-manifest.json") != manifest.get("sdkManifestSha256") or sdk["archiveSha256"] != manifest.get("sdkArchiveSha256"):
        raise ValueError("SDK manifest identity mismatch")
    expected = manifest.get("files")
    if not isinstance(expected, dict) or not expected:
        raise ValueError("consumer output inventory is empty")
    for name, sha in expected.items():
        path = PurePosixPath(name)
        if path.is_absolute() or ".." in path.parts or "\\" in name or path.as_posix() != name or not isinstance(sha, str) or not re.fullmatch("[0-9a-f]{64}", sha):
            raise ValueError("invalid consumer output inventory")
    if _files(root / "outputs") != expected:
        raise ValueError("consumer output inventory or checksum mismatch")
    require_assembled(root / "outputs")
    return manifest


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit("usage: consumer_artifact.py CONSUMER DISTRIBUTION NEW_ARTIFACT")
    seal(Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve(), Path(sys.argv[3]).absolute())
