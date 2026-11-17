#!/usr/bin/env python3
"""Publish tested native distribution bytes; no compilation in the publisher."""
from __future__ import annotations

import base64
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import zipfile


def run(*args, cwd=None, env=None, capture=False):
    result = subprocess.run(args, cwd=cwd, env=env, check=True, text=True,
                            stdout=subprocess.PIPE if capture else None)
    return result.stdout.strip() if capture else ""


def api(path):
    return json.loads(run("gh", "api", path, capture=True))


def identify():
    repository = os.environ["GITHUB_REPOSITORY"]
    run_id = os.environ["NATIVE_RUN_ID"]
    if not re.fullmatch(r"[1-9][0-9]*", run_id):
        raise ValueError("native run ID must be a positive decimal integer")
    build = api(f"repos/{repository}/actions/runs/{run_id}")
    if build["path"] != ".github/workflows/native-conformance.yml" or build["conclusion"] != "success":
        raise ValueError("a successful Native conformance run is required")
    sha = build["head_sha"]
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        raise ValueError("invalid native source revision")
    comparison = api(f"repos/{repository}/compare/{sha}...main")
    if comparison["status"] not in ("ahead", "identical") or comparison["merge_base_commit"]["sha"] != sha:
        raise ValueError("native source revision is not in main history")
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        output.write("sha=" + sha + "\n")
    print("Native build is successful and merged: " + sha)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify_source_tag(remote, tag, sha):
    ref = "refs/tags/" + tag
    refs = dict(line.split()[::-1] for line in
                run("git", "ls-remote", remote, ref, ref + "^{}", capture=True).splitlines())
    commit = refs.get(ref + "^{}", refs.get(ref))
    if commit is not None and commit != sha:
        raise ValueError("release tag points to a different source")


def publish(artifacts, output):
    repository = os.environ["GITHUB_REPOSITORY"]
    sha = os.environ["NATIVE_SOURCE_SHA"]
    if not re.fullmatch(r"[0-9a-f]{40}", sha) or output.exists():
        raise ValueError("valid source SHA and a new output directory are required")
    maven = output / "maven"
    assets = output / "assets"
    maven.mkdir(parents=True)
    assets.mkdir()
    versions = set()
    for platform, manifest_name, archive_name in [
        ("android", "android-release-manifest.json", "LabColors.android.zip"),
        ("apple", "swift-release-manifest.json", "LabColors.swiftpm.zip"),
        ("desktop", "desktop-release-manifest.json", "LabColors.desktop.zip"),
    ]:
        directory = artifacts / f"{platform}-sdk-{sha}"
        manifest_path = directory / manifest_name
        manifest = json.loads(manifest_path.read_text())
        archive = directory / archive_name
        if manifest["sourceCommit"] != sha or manifest["archiveSha256"] != digest(archive):
            raise ValueError("distribution revision or bytes differ from tested artifact")
        platforms = json.loads((Path(__file__).resolve().parents[1] / "bindings/kotlin/desktop/platforms.json").read_text())
        if platform == "desktop" and set(manifest["platforms"]) != {row["platform"] for row in platforms}:
            raise ValueError("incomplete desktop distribution")
        if "version" in manifest:
            versions.add(manifest["version"])
        shutil.copy2(archive, assets)
        shutil.copy2(manifest_path, assets)
        if platform != "apple":
            with zipfile.ZipFile(archive) as bundle:
                for name in bundle.namelist():
                    path = PurePosixPath(name)
                    if path.is_absolute() or ".." in path.parts or "\\" in name:
                        raise ValueError("unsafe native archive path")
                    if path.parts[0] != "repository":
                        continue
                    target = maven / Path(*path.parts[1:])
                    if target.exists():
                        raise ValueError("overlapping Maven artifacts")
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(bundle.read(name))
    if len(versions) != 1:
        raise ValueError("Android and desktop Maven versions differ")
    version = versions.pop()
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+-" + sha, version):
        raise ValueError("Maven coordinate is not tied to this source revision")
    tag = "colors-native-v" + version[:-(len(sha) + 1)] + "-" + sha[:12]
    remote = f"https://github.com/{repository}.git"
    verify_source_tag(remote, tag, sha)
    branch = "maven-" + tag
    url = f"https://raw.githubusercontent.com/{repository}/{branch}/"
    notes = (f"Native Colors, source `{sha}`.\n\n"
             f"Maven repository: `{url}`\n\n"
             f"- Android: `pics.lab:colors-android:{version}`\n"
             f"- Desktop: `pics.lab:colors-desktop:{version}`\n"
             "- Apple: `LabColors.swiftpm.zip`, self-contained SwiftPM package and XCFramework.\n\n"
             f"Exact archives exercised by [Native conformance](https://github.com/{repository}/actions/runs/{os.environ['NATIVE_RUN_ID']}).\n")
    (maven / "README.md").write_text(notes)
    notes_path = output / "release-notes.md"
    notes_path.write_text(notes)
    (assets / "SHA256SUMS").write_text("".join(f"{digest(path)}  {path.name}\n" for path in sorted(assets.iterdir())))
    run("git", "init", "-b", branch, cwd=maven)
    run("git", "config", "user.name", "github-actions[bot]", cwd=maven)
    run("git", "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com", cwd=maven)
    run("git", "add", ".", cwd=maven)
    run("git", "commit", "-m", "Publish native Colors " + version, cwd=maven)
    credential = base64.b64encode(("x-access-token:" + os.environ["GH_TOKEN"]).encode()).decode()
    environment = dict(os.environ, GIT_CONFIG_COUNT="1", GIT_CONFIG_KEY_0="http.https://github.com/.extraheader",
                       GIT_CONFIG_VALUE_0="AUTHORIZATION: basic " + credential)
    existing = run("git", "ls-remote", remote, "refs/heads/" + branch, cwd=maven, env=environment, capture=True)
    if existing:
        run("git", "fetch", "--depth=1", remote, branch, cwd=maven, env=environment)
        if run("git", "rev-parse", "HEAD^{tree}", cwd=maven, capture=True) != run("git", "rev-parse", "FETCH_HEAD^{tree}", cwd=maven, capture=True):
            raise ValueError("published Maven version already contains different bytes")
    else:
        run("git", "push", remote, "HEAD:refs/heads/" + branch, cwd=maven, env=environment)
    release = subprocess.run(["gh", "release", "view", tag, "--repo", repository, "--json", "assets"], capture_output=True, text=True)
    if release.returncode != 0:
        run("gh", "release", "create", tag, "--repo", repository, "--target", sha, "--title", tag,
            "--notes-file", str(notes_path), *[str(path) for path in sorted(assets.iterdir())])
    else:
        published = {asset["name"]: asset for asset in json.loads(release.stdout)["assets"]}
        for path in sorted(assets.iterdir()):
            if path.name not in published:
                run("gh", "release", "upload", tag, str(path), "--repo", repository)
            else:
                checked = output / "already-published" / path.name
                checked.parent.mkdir(exist_ok=True)
                run("gh", "release", "download", tag, "--repo", repository, "--pattern", path.name, "--dir", str(checked.parent))
                if digest(checked) != digest(path):
                    raise ValueError("existing release asset has different bytes")
    print(notes)


if __name__ == "__main__":
    if sys.argv[1] == "identify":
        identify()
    elif sys.argv[1] == "publish":
        publish(Path(sys.argv[2]).resolve(), Path(sys.argv[3]).resolve())
    else:
        raise SystemExit("usage: publish-native.py identify | publish ARTIFACTS NEW_OUTPUT")
