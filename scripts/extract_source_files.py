#!/usr/bin/env python3
"""EXT-01: Source files artifact class extractor.

Produces a finite manifest of all production Rust source files in
crates/labcolors-core/src/ with their SHA-256 digests.

Sabotage controls:
- Verifies every listed file and its digest against the complete live source tree.
- This script pins nothing by itself: `extract | verify` in one run proves only
  self-consistency. Drift against the repository is detected by
  scripts/artifact_matrix.py (proof/artifact/tree.json), not here.
- Refuses incomplete walks and symlinked source trees.
- Checks the record self-hash, schema, counts and expected root.

Exit evidence: JSON manifest on stdout with schema:
{
  "class": "source-files",
  "schema_version": 1,
  "root": "crates/labcolors-core/src",
  "file_count": <int>,
  "files": [{"path": "<relative>", "sha256": "<hex>"}],
  "manifest_sha256": "<hex of canonical JSON excluding this field>"
}
"""
import hashlib
import json
import sys
from pathlib import Path

from source_tree import rust_sources

REPO_ROOT = Path(__file__).resolve().parent.parent
SRC_ROOT = REPO_ROOT / "crates" / "labcolors-core" / "src"


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def extract_manifest() -> dict:
    if not SRC_ROOT.is_dir():
        print(f"SABOTAGE: source root missing: {SRC_ROOT}", file=sys.stderr)
        sys.exit(1)

    files = []
    for rs_file in rust_sources(SRC_ROOT):
        rel = rs_file.relative_to(REPO_ROOT).as_posix()
        digest = sha256_file(rs_file)
        files.append({"path": rel, "sha256": digest})

    manifest = {
        "class": "source-files",
        "schema_version": 1,
        "root": "crates/labcolors-core/src",
        "file_count": len(files),
        "files": files,
    }

    # Compute self-hash over canonical form without manifest_sha256
    canonical = json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
    manifest["manifest_sha256"] = hashlib.sha256(canonical).hexdigest()

    return manifest


def verify_manifest(manifest: dict) -> None:
    """Compare a sealed record with a fresh complete source inventory."""
    errors = []
    if not isinstance(manifest, dict):
        errors.append("MALFORMED: record is not an object")
    else:
        fields = {"class", "schema_version", "root", "file_count", "files", "manifest_sha256"}
        if set(manifest) != fields:
            errors.append("MALFORMED: unexpected or missing record fields")
        if manifest.get("class") != "source-files" or manifest.get("schema_version") != 1:
            errors.append("SCHEMA: unexpected class or version")
        if manifest.get("root") != "crates/labcolors-core/src":
            errors.append("ROOT: unexpected source root")
        files = manifest.get("files")
        if not isinstance(files, list):
            errors.append("MALFORMED: files is not a list")
        else:
            if type(manifest.get("file_count")) is not int or manifest["file_count"] != len(files):
                errors.append("COUNT: file_count does not match entries")
        canonical = {k: v for k, v in manifest.items() if k != "manifest_sha256"}
        expected_hash = hashlib.sha256(
            json.dumps(canonical, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()
        if manifest.get("manifest_sha256") != expected_hash:
            errors.append("HASH: manifest_sha256 mismatch")

    if errors:
        print("SABOTAGE FAILURES:\n" + "\n".join("  - " + e for e in errors), file=sys.stderr)
        sys.exit(1)

    # Never open a path supplied by the record: it might point outside the
    # declared tree. Only the independently enumerated source tree is read.
    current = extract_manifest()
    actual = {entry["path"]: entry["sha256"] for entry in current["files"]}
    seen = set()
    for entry in manifest["files"]:
        if not isinstance(entry, dict) or set(entry) != {"path", "sha256"}:
            errors.append("MALFORMED: unexpected source entry")
            continue
        path, digest = entry["path"], entry["sha256"]
        if not isinstance(path, str) or not isinstance(digest, str):
            errors.append("MALFORMED: path and sha256 must be strings")
            continue
        if path in seen:
            errors.append(f"DUPLICATE: {path}")
        seen.add(path)
        if path not in actual:
            errors.append(f"MISSING: {path}")
        elif digest != actual[path]:
            errors.append(f"DRIFT: {path}")
    for path in sorted(actual.keys() - seen):
        errors.append(f"UNLISTED: {path}")
    if errors:
        print("SABOTAGE FAILURES:", file=sys.stderr)
        for error in errors:
            print("  - " + error, file=sys.stderr)
        sys.exit(1)


def main() -> None:
    mode = sys.argv[1] if len(sys.argv) > 1 else "extract"

    if mode in ("extract", "verify"):
        try:
            if mode == "extract":
                print(json.dumps(extract_manifest(), sort_keys=True, indent=2))
            else:
                raw = sys.stdin.buffer.read().decode("utf-8-sig")
                manifest = json.loads(raw)
                verify_manifest(manifest)
                print(f"EXT-01 VERIFY OK: {manifest['file_count']} files", file=sys.stderr)
        except (OSError, ValueError) as error:
            print(f"SABOTAGE: unable to enumerate sources: {error}", file=sys.stderr)
            sys.exit(1)
    else:
        print(f"Unknown mode: {mode}. Use 'extract' or 'verify'.", file=sys.stderr)
        sys.exit(64)


if __name__ == "__main__":
    main()