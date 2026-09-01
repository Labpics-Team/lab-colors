#!/usr/bin/env python3
"""EXT-03: Public exports/package metadata artifact class extractor.

Produces a finite manifest of all workspace crate metadata from
`cargo metadata --no-deps`: crate names, versions, features, dependencies,
and targets. This covers the "public exports/package metadata" artifact class
per r5/r6 specification.

Sabotage controls:
- Fails if cargo metadata returns non-zero exit code.
- Fails if labcolors-core is missing from workspace packages.
- Fails if manifest_sha256 does not match recomputed canonical hash.

Exit evidence: JSON manifest on stdout with schema:
{
  "class": "exports-metadata",
  "schema_version": 1,
  "crate_count": <int>,
  "crates": [{"name": "<str>", "version": "<str>", "features": [...], "deps": [...], "targets": [...]}],
  "manifest_sha256": "<hex>"
}
"""
import hashlib
import json
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent


def extract_metadata() -> dict:
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        capture_output=True,
        text=True,
        cwd=str(REPO_ROOT),
    )
    if result.returncode != 0:
        print(f"SABOTAGE: cargo metadata failed: {result.stderr}", file=sys.stderr)
        sys.exit(1)

    meta = json.loads(result.stdout)
    crates = []
    for pkg in sorted(meta["packages"], key=lambda p: p["name"]):
        features = sorted(pkg.get("features", {}).keys())
        deps = sorted(
            d["name"]
            for d in pkg.get("dependencies", [])
            if d.get("kind") is None  # normal deps only
        )
        targets = [
            {"name": t["name"], "kind": t["kind"][0] if t["kind"] else "unknown"}
            for t in pkg.get("targets", [])
        ]
        crates.append({
            "name": pkg["name"],
            "version": pkg["version"],
            "features": features,
            "deps": deps,
            "targets": targets,
        })

    if not any(c["name"] == "labcolors-core" for c in crates):
        print("SABOTAGE: labcolors-core missing from workspace packages", file=sys.stderr)
        sys.exit(1)

    manifest = {
        "class": "exports-metadata",
        "schema_version": 1,
        "crate_count": len(crates),
        "crates": crates,
    }
    canonical = json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
    manifest["manifest_sha256"] = hashlib.sha256(canonical).hexdigest()
    return manifest


def verify_manifest(manifest: dict) -> None:
    """Sabotage: verify manifest self-consistency."""
    errors = []
    for field in ("class", "schema_version", "crate_count", "crates", "manifest_sha256"):
        if field not in manifest:
            errors.append(f"MISSING_FIELD: {field}")
    if errors:
        print("SABOTAGE FAILURES:", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        sys.exit(1)

    payload = {k: v for k, v in manifest.items() if k != "manifest_sha256"}
    canonical = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
    recomputed = hashlib.sha256(canonical).hexdigest()
    if recomputed != manifest["manifest_sha256"]:
        print(f"SABOTAGE: manifest_sha256 mismatch", file=sys.stderr)
        sys.exit(1)

    if len(manifest["crates"]) != manifest["crate_count"]:
        print(f"SABOTAGE: crate_count mismatch", file=sys.stderr)
        sys.exit(1)


def main() -> None:
    mode = sys.argv[1] if len(sys.argv) > 1 else "extract"
    if mode == "extract":
        manifest = extract_metadata()
        print(json.dumps(manifest, sort_keys=True, indent=2))
    elif mode == "verify":
        raw = sys.stdin.buffer.read().decode("utf-8-sig")
        manifest = json.loads(raw)
        verify_manifest(manifest)
        print(f"EXT-03 VERIFY OK: {manifest['crate_count']} crates", file=sys.stderr)
    else:
        print(f"Unknown mode: {mode}. Use 'extract' or 'verify'.", file=sys.stderr)
        sys.exit(64)


if __name__ == "__main__":
    main()