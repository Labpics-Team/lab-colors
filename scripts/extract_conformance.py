#!/usr/bin/env python3
"""EXT-05: Conformance families artifact class extractor.

Produces a finite manifest of conformance test sources and proof artifacts
in crates/labcolors-conformance/ and proof/region/v1/. Each entry carries
path, kind (source|proof|test), SHA-256 digest, and size for sabotage detection.

Sabotage controls:
- Fails if conformance crate source root is missing.
- Fails if manifest_sha256 does not match recomputed canonical hash.
- Fails if entry_count != actual entries.

Exit evidence: JSON manifest on stdout with schema:
{
  "class": "conformance-families",
  "schema_version": 1,
  "entry_count": <int>,
  "entries": [{"path": "<str>", "kind": "<source|proof|test>", "sha256": "<hex>", "size": <int>}],
  "manifest_sha256": "<hex>"
}
"""
import hashlib
import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
CONFORMANCE_SRC = REPO_ROOT / "crates" / "labcolors-conformance" / "src"
PROOF_REGION = REPO_ROOT / "proof" / "region" / "v1"


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def extract_entries() -> list[dict]:
    entries = []

    if not CONFORMANCE_SRC.is_dir():
        print(f"SABOTAGE: conformance source root missing: {CONFORMANCE_SRC}", file=sys.stderr)
        sys.exit(1)

    # Conformance crate sources
    for rs_file in sorted(CONFORMANCE_SRC.rglob("*.rs")):
        rel = rs_file.relative_to(REPO_ROOT).as_posix()
        entries.append({
            "path": rel,
            "kind": "source",
            "sha256": sha256_file(rs_file),
            "size": rs_file.stat().st_size,
        })

    # Proof region artifacts (Python proofs, tests, fixtures)
    if PROOF_REGION.is_dir():
        for py_file in sorted(PROOF_REGION.rglob("*.py")):
            if "__pycache__" in str(py_file):
                continue
            rel = py_file.relative_to(REPO_ROOT).as_posix()
            kind = "test" if "/tests/" in rel else "proof"
            entries.append({
                "path": rel,
                "kind": kind,
                "sha256": sha256_file(py_file),
                "size": py_file.stat().st_size,
            })

    entries.sort(key=lambda e: (e["path"], e["kind"]))
    return entries


def build_manifest(entries: list[dict]) -> dict:
    manifest = {
        "class": "conformance-families",
        "schema_version": 1,
        "entry_count": len(entries),
        "entries": entries,
    }
    canonical = json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
    manifest["manifest_sha256"] = hashlib.sha256(canonical).hexdigest()
    return manifest


def verify_manifest(manifest: dict) -> None:
    """Sabotage: verify manifest self-consistency."""
    errors = []
    for field in ("class", "schema_version", "entry_count", "entries", "manifest_sha256"):
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
        print("SABOTAGE: manifest_sha256 mismatch", file=sys.stderr)
        sys.exit(1)

    if len(manifest["entries"]) != manifest["entry_count"]:
        print("SABOTAGE: entry_count mismatch", file=sys.stderr)
        sys.exit(1)

    seen_files = set()
    for entry in manifest["entries"]:
        fpath = REPO_ROOT / entry["path"]
        seen_files.add(entry["path"])
        if not fpath.is_file():
            errors.append(f"SOURCE_MISSING: {entry['path']}")
    if errors:
        print("SABOTAGE FAILURES:", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        sys.exit(1)


def main() -> None:
    mode = sys.argv[1] if len(sys.argv) > 1 else "extract"
    if mode == "extract":
        entries = extract_entries()
        manifest = build_manifest(entries)
        print(json.dumps(manifest, sort_keys=True, indent=2))
    elif mode == "verify":
        raw = sys.stdin.buffer.read().decode("utf-8-sig")
        manifest = json.loads(raw)
        verify_manifest(manifest)
        print(f"EXT-05 VERIFY OK: {manifest['entry_count']} entries", file=sys.stderr)
    else:
        print(f"Unknown mode: {mode}. Use 'extract' or 'verify'.", file=sys.stderr)
        sys.exit(64)


if __name__ == "__main__":
    main()