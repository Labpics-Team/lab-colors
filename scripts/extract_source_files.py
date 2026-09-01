#!/usr/bin/env python3
"""EXT-01: Source files artifact class extractor.

Produces a finite manifest of all production Rust source files in
crates/labcolors-core/src/ with their SHA-256 digests.

Sabotage controls:
- Fails if any listed file is missing from disk.
- Fails if computed digest does not match the pinned manifest.
- Fails if an unexpected .rs file appears (new file without manifest update).

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
    for rs_file in sorted(SRC_ROOT.rglob("*.rs")):
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
    """Sabotage: verify every listed file exists and matches digest."""
    errors = []
    seen_paths = set()

    for entry in manifest["files"]:
        path = REPO_ROOT / entry["path"]
        seen_paths.add(entry["path"])
        if not path.is_file():
            errors.append(f"MISSING: {entry['path']}")
            continue
        actual = sha256_file(path)
        if actual != entry["sha256"]:
            errors.append(
                f"DRIFT: {entry['path']} expected={entry['sha256'][:16]}… "
                f"actual={actual[:16]}…"
            )

    # Detect unlisted .rs files (new files without manifest update)
    for rs_file in sorted(SRC_ROOT.rglob("*.rs")):
        rel = rs_file.relative_to(REPO_ROOT).as_posix()
        if rel not in seen_paths:
            errors.append(f"UNLISTED: {rel}")

    if errors:
        print("SABOTAGE FAILURES:", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        sys.exit(1)


def main() -> None:
    mode = sys.argv[1] if len(sys.argv) > 1 else "extract"

    if mode == "extract":
        manifest = extract_manifest()
        print(json.dumps(manifest, sort_keys=True, indent=2))
    elif mode == "verify":
        raw = sys.stdin.buffer.read().decode("utf-8-sig")
        manifest = json.loads(raw)
        verify_manifest(manifest)
        print(f"EXT-01 VERIFY OK: {manifest['file_count']} files", file=sys.stderr)
    else:
        print(f"Unknown mode: {mode}. Use 'extract' or 'verify'.", file=sys.stderr)
        sys.exit(64)


if __name__ == "__main__":
    main()