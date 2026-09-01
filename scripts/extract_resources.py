#!/usr/bin/env python3
"""EXT-07: Resource dimensions/cardinalities artifact class extractor.

Produces a finite manifest of resource dimension constants and cardinality
bounds in production Rust source files. Extracts pub const declarations
that define domain sizes, scale factors, iteration limits, and capacity bounds.

Sabotage controls:
- Fails if no crate source roots exist.
- Fails if manifest_sha256 does not match recomputed canonical hash.
- Fails if entry_count != actual entries.

Exit evidence: JSON manifest on stdout with schema:
{
  "class": "resource-dimensions",
  "schema_version": 1,
  "entry_count": <int>,
  "entries": [{"path": "<str>", "line": <int>, "name": "<str>", "value": "<str>", "source_sha256": "<hex>"}],
  "manifest_sha256": "<hex>"
}
"""
import hashlib
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SRC_ROOTS = [
    REPO_ROOT / "crates" / "labcolors-core" / "src",
    REPO_ROOT / "crates" / "labcolors-conformance" / "src",
]

# Matches pub const declarations with numeric or expression values
RESOURCE_CONST_PATTERN = re.compile(
    r'^\s*pub\s+const\s+([A-Z_][A-Z0-9_]*)\s*:\s*\w+\s*=\s*(.+?)\s*;',
    re.MULTILINE,
)


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def extract_entries() -> list[dict]:
    entries = []
    found_any_root = False

    for src_root in SRC_ROOTS:
        if not src_root.is_dir():
            continue
        found_any_root = True

        for rs_file in sorted(src_root.rglob("*.rs")):
            rel = rs_file.relative_to(REPO_ROOT).as_posix()
            text = rs_file.read_text(encoding="utf-8")
            file_hash = sha256_file(rs_file)

            for match in RESOURCE_CONST_PATTERN.finditer(text):
                name = match.group(1)
                value = match.group(2).strip()[:200]
                line_no = text[:match.start()].count("\n") + 1
                entries.append({
                    "path": rel,
                    "line": line_no,
                    "name": name,
                    "value": value,
                    "source_sha256": file_hash,
                })

    if not found_any_root:
        print("SABOTAGE: no crate source roots found", file=sys.stderr)
        sys.exit(1)

    entries.sort(key=lambda e: (e["path"], e["line"]))
    return entries


def build_manifest(entries: list[dict]) -> dict:
    manifest = {
        "class": "resource-dimensions",
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
        print(f"EXT-07 VERIFY OK: {manifest['entry_count']} entries", file=sys.stderr)
    else:
        print(f"Unknown mode: {mode}. Use 'extract' or 'verify'.", file=sys.stderr)
        sys.exit(64)


if __name__ == "__main__":
    main()