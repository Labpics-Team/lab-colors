#!/usr/bin/env python3
"""Assemble the exact native builds into one self-contained desktop Maven package."""
from __future__ import annotations

import json
from pathlib import Path, PurePosixPath
import shutil
import sys
import zipfile

from build_native import ROOT, PLATFORMS, archive_tree, digest, run
sys.path.insert(0, str(ROOT / "bindings/kotlin/ci"))
from gradle_distribution import gradle


def assemble(parts, output, require_all=True):
    if output.exists():
        raise ValueError("package output must be new")
    project = output / "producer"
    shutil.copytree(ROOT / "bindings/kotlin/desktop", project,
                    ignore=shutil.ignore_patterns("ci", "consumer", ".gradle", "build"))
    shutil.copytree(ROOT / "bindings/kotlin/src/main/kotlin", project / "src/main/kotlin")
    expected = {row[0] for row in PLATFORMS.values()}
    manifests = {}
    shared = {}
    revision = run("git", "rev-parse", "HEAD", capture=True)
    version = None
    for archive in sorted(parts.glob("LabColors.*.zip")):
        with zipfile.ZipFile(archive) as bundle:
            manifest = json.loads(bundle.read("manifest.json"))
            prefix = manifest["resourcePrefix"]
            if prefix in manifests or PLATFORMS.get(manifest["target"]) != (prefix, manifest["library"]):
                raise ValueError("duplicate or unrecognized native platform")
            if manifest["sourceCommit"] != revision or (version and version != manifest["version"]):
                raise ValueError("native artifacts were not built from the same revision")
            version = manifest["version"]
            manifests[prefix] = manifest
            for info in bundle.infolist():
                name = PurePosixPath(info.filename)
                if name.is_absolute() or ".." in name.parts or "\\" in info.filename:
                    raise ValueError("unsafe native archive path")
                if name.parts[0] == "generated":
                    destination = project / str(name)
                elif name.parts[0] == "notices":
                    destination = project / "nativeResources/META-INF/labcolors" / Path(*name.parts[1:])
                elif info.filename == prefix + "/" + manifest["library"]:
                    destination = project / "nativeResources" / str(name)
                elif info.filename == "manifest.json":
                    continue
                else:
                    raise ValueError("unexpected native archive member: " + info.filename)
                data = bundle.read(info)
                if name.parts[0] == "generated":
                    if info.filename in shared and shared[info.filename] != data:
                        raise ValueError("platforms generated different Kotlin API")
                    shared[info.filename] = data
                # Per-platform license inventories can differ; keep each complete.
                if name.parts[0] == "notices":
                    destination = project / "nativeResources/META-INF/labcolors" / prefix / Path(*name.parts[1:])
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(data)
            library = project / "nativeResources" / prefix / manifest["library"]
            if digest(library) != manifest["librarySha256"]:
                raise ValueError("native library digest mismatch")
    if not manifests or (require_all and set(manifests) != expected):
        raise ValueError("missing supported desktop platforms")
    native_metadata = project / "nativeResources/META-INF/labcolors/build-metadata.json"
    native_metadata.write_text(json.dumps(manifests, indent=2) + "\n", encoding="utf-8")
    publication = output / "publication"
    executable = gradle(output / "tools")
    run(str(executable), "--no-daemon", "--max-workers=2", "publish", "-PpublicationVersion=" + version,
        "-PpublicationRepository=" + str(publication / "repository"), cwd=project)
    shutil.copy2(ROOT / "LICENSE", publication)
    shutil.copy2(ROOT / "bindings/kotlin/desktop/README.md", publication)
    archive = output / "LabColors.desktop.zip"
    archive_tree(publication, archive)
    manifest = {"schemaVersion": 1, "sourceCommit": revision, "version": version,
                "platforms": sorted(manifests), "archive": archive.name, "archiveSha256": digest(archive)}
    (output / "desktop-release-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    (output / "SHA256SUMS").write_text(digest(archive) + "  " + archive.name + "\n", encoding="utf-8")
    print("Desktop Maven package: pics.lab:colors-desktop:" + version)


if __name__ == "__main__":
    assemble(Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve(), "--host-only" not in sys.argv[3:])
