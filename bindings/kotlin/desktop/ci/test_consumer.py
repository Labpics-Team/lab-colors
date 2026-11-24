#!/usr/bin/env python3
"""Run a separate JVM application using only the distribution's Maven repository."""
from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import sys
import zipfile

from build_native import ROOT, digest, run
from package import gradle


def bytes_literal(values):
    return "byteArrayOf(" + ",".join(str(value if value < 128 else value - 256) for value in values) + ")"


def main(distribution, output):
    if output.exists():
        raise ValueError("consumer output must be new")
    manifest = json.loads((distribution / "desktop-release-manifest.json").read_text(encoding="utf-8"))
    archive = distribution / "LabColors.desktop.zip"
    if manifest["sourceCommit"] != run("git", "rev-parse", "HEAD", capture=True) or digest(archive) != manifest["archiveSha256"]:
        raise ValueError("consumer received the wrong SDK revision or archive")
    project = output / "application"
    shutil.copytree(ROOT / "bindings/kotlin/desktop/consumer", project)
    repository = output / "sdk"
    with zipfile.ZipFile(archive) as bundle:
        for name in bundle.namelist():
            if name.startswith("/") or ".." in Path(name).parts or "\\" in name:
                raise ValueError("unsafe distribution path")
        bundle.extractall(repository)
    vectors = json.loads((ROOT / "conformance/program-runtime-v1.json").read_text(encoding="utf-8"))
    authoring = json.loads((ROOT / "conformance/program-authoring-v3.json").read_text(encoding="utf-8"))
    if authoring["schemaVersion"] != 1:
        raise ValueError("unknown Program authoring fixture")
    authoring_rows = [f'    "{name}" to "{wire}"' for name, wire in authoring.items()
                      if name not in ("schemaVersion", "provenance")]
    authoring_tests = project / "src/main/kotlin/pics/lab/colors/conformance"
    authoring_tests.mkdir(parents=True)
    shutil.copy2(ROOT / "bindings/kotlin/conformance/ProgramAuthoringV3.kt", authoring_tests)
    rows = []
    for row in vectors["history"]:
        background = row.get("backgrounds", [])
        values = [value for rgb in background for value in rgb]
        ids = "listOf(" + ",".join(str(index + 1) + "u" for index in range(len(background))) + ")"
        reason = str(row["unknownReason"]) + "u" if "unknownReason" in row else "null"
        rgb = bytes_literal(row["rgb"]) if "rgb" in row else "null"
        rows.append(f'    Frame({row["revision"]}uL, {ids}, {bytes_literal(values)}, {reason}, "{row["state"]}", {rgb})')
    source = ('package consumer\n'
              'data class Frame(val revision: ULong, val ids: List<UInt>, val background: ByteArray, '
              'val reason: UInt?, val state: String, val rgb: ByteArray?)\n'
              f'val fixedWire = "{vectors["fixed"]}"\nval adaptiveWire = "{vectors["adaptive"]}"\n'
              'val history = listOf(\n' + ',\n'.join(rows) + '\n)\n'
              'val authoringV3Wires = mapOf(\n' + ',\n'.join(authoring_rows) + '\n)\n')
    (project / "src/main/kotlin/consumer/Vectors.kt").write_text(source, encoding="utf-8")
    environment = dict(os.environ)
    environment.pop("JNA_LIBRARY_PATH", None)
    run(str(gradle(output / "tools")), "--no-daemon", "--max-workers=2", "run",
        "-PcolorsVersion=" + manifest["version"], "-PcolorsRepository=" + str(repository / "repository"),
        cwd=project, env=environment)


if __name__ == "__main__":
    main(Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve())
