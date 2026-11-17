#!/usr/bin/env python3
"""Build one JNA resource from the common Rust runtime, with matching bindings."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[4]
PLATFORMS = {
    "x86_64-pc-windows-msvc": ("win32-x86-64", "labcolors.dll"),
    "aarch64-pc-windows-msvc": ("win32-aarch64", "labcolors.dll"),
    "x86_64-unknown-linux-gnu": ("linux-x86-64", "liblabcolors.so"),
    "aarch64-unknown-linux-gnu": ("linux-aarch64", "liblabcolors.so"),
    "aarch64-apple-darwin": ("darwin-aarch64", "liblabcolors.dylib"),
}


def run(*args, capture=False, env=None, cwd=ROOT):
    result = subprocess.run(args, check=True, cwd=cwd, env=env, text=True,
                            stdout=subprocess.PIPE if capture else None)
    return result.stdout.strip() if capture else ""


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def archive_tree(root, archive):
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as bundle:
        for path in sorted(root.rglob("*")):
            if path.is_symlink():
                raise ValueError("distribution cannot contain symlinks")
            if path.is_file():
                info = zipfile.ZipInfo(path.relative_to(root).as_posix(), (1980, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = 0o100644 << 16
                bundle.writestr(info, path.read_bytes())


def main(output):
    if run("git", "status", "--porcelain", "--untracked-files=no", capture=True):
        raise ValueError("native producer needs a clean source revision")
    if output.exists() and any(p.name != "target" for p in output.iterdir()):
        raise ValueError("output may contain only the Cargo cache")
    toolchain = os.environ.get("RUST_TOOLCHAIN", "1.96.0")
    host = next(line[6:] for line in run("rustc", "+" + toolchain, "-vV", capture=True).splitlines()
                if line.startswith("host: "))
    prefix, filename = PLATFORMS[host]
    environment = dict(os.environ, CARGO_TARGET_DIR=str(output / "target"))
    if sys.platform == "darwin":
        environment["MACOSX_DEPLOYMENT_TARGET"] = "13.0"
    run("cargo", "+" + toolchain, "build", "--locked", "-p", "labcolors-ffi", "--features", "cli", env=environment)
    content = output / "content"
    generated = content / "generated"
    run("cargo", "+" + toolchain, "run", "--locked", "-p", "labcolors-ffi", "--features", "cli",
        "--bin", "uniffi-bindgen", "--", "generate", "--library", str(output / "target/debug" / filename),
        "--language", "kotlin", "--config", str(ROOT / "bindings/kotlin/desktop/uniffi.toml"),
        "--out-dir", str(generated), env=environment)
    messages = run("cargo", "+" + toolchain, "build", "--locked", "--release", "--lib", "-p", "labcolors-ffi",
                   "--message-format=json-render-diagnostics", capture=True, env=environment)
    packages = sorted({row["package_id"] for line in messages.splitlines()
                       for row in [json.loads(line)] if row.get("reason") == "compiler-artifact"})
    library = content / prefix / filename
    library.parent.mkdir(parents=True)
    shutil.copy2(output / "target/release" / filename, library)
    if sys.platform == "linux":
        symbols = run("readelf", "--version-info", str(library), capture=True)
        versions = {tuple(map(int, match.split("."))) for match in re.findall(r"GLIBC_([0-9.]+)", symbols)}
        if not versions or max(versions) > (2, 31):
            raise ValueError("desktop Linux library requires newer than Ubuntu 20.04 glibc")
    inventory = output / "build-packages.json"
    inventory.write_text(json.dumps(packages) + "\n", encoding="utf-8")
    run("node", "scripts/native-distribution-licenses.mjs", str(content / "notices"), str(inventory), env=environment)
    revision = run("git", "rev-parse", "HEAD", capture=True)
    metadata = json.loads(run("cargo", "+" + toolchain, "metadata", "--no-deps", "--format-version=1", capture=True))
    core_version = next(p["version"] for p in metadata["packages"] if p["name"] == "labcolors-core")
    manifest = {"schemaVersion": 1, "sourceCommit": revision, "version": core_version + "-" + revision,
                "target": host, "resourcePrefix": prefix, "library": filename,
                "librarySha256": digest(library), "rustToolchain": toolchain}
    (content / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    archive = output / ("LabColors." + prefix + ".zip")
    archive_tree(content, archive)
    print(str(archive))


if __name__ == "__main__":
    main(Path(sys.argv[1]).resolve())
