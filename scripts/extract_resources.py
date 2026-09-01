#!/usr/bin/env python3
"""EXT-07: Resource dimensions/cardinalities artifact class extractor.

Produces a finite manifest of resource dimension constants and cardinality
bounds in production Rust source files. Extracts pub const declarations
that define domain sizes, scale factors, iteration limits, and capacity bounds.

RESOURCE_CONST_PATTERN is intentionally restricted to public Rust constants
with uppercase identifiers, a simple type annotation, and a semicolon-terminated
value expression. This invariant keeps manifest generation deterministic across
filesystem enumeration orders and platforms. Intentionally excluded forms:
non-public constants (pub(crate), pub(super)), lowercase or mixed-case names,
complex expressions spanning multiple lines, function-like macros, and any
declaration without an explicit semicolon terminator. These exclusions are by
design — the manifest captures only stable, machine-verifiable resource bounds,
not arbitrary constant definitions.

Sabotage controls:
- Fails if no crate source roots exist.
- Fails if manifest_sha256 does not match recomputed canonical hash.
- Fails if entry_count != actual entries.
- Fails if any entry path escapes configured SRC_ROOTS.
- Fails if any entry's source file content has drifted since extraction.

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

# Matches pub const declarations with uppercase names, simple type, and
# semicolon-terminated value. See module docstring for exclusion rationale.
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


def _is_within_src_roots(path: Path) -> bool:
    """Check that path is under one of the configured SRC_ROOTS."""
    try:
        resolved = path.resolve()
        return any(resolved.is_relative_to(root.resolve()) for root in SRC_ROOTS if root.exists())
    except (OSError, ValueError):
        return False


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
    """Sabotage: verify manifest self-consistency, scope, and content integrity."""
    errors = []

    # Schema validation
    if not isinstance(manifest, dict):
        print("SABOTAGE: manifest is not a JSON object", file=sys.stderr)
        sys.exit(1)

    for field in ("class", "schema_version", "entry_count", "entries", "manifest_sha256"):
        if field not in manifest:
            errors.append(f"MISSING_FIELD: {field}")

    if not isinstance(manifest.get("entries"), list):
        errors.append("INVALID_TYPE: entries must be a list")

    if not isinstance(manifest.get("entry_count"), int):
        errors.append("INVALID_TYPE: entry_count must be an integer")

    if errors:
        print("SABOTAGE FAILURES:", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        sys.exit(1)

    # Self-hash check
    payload = {k: v for k, v in manifest.items() if k != "manifest_sha256"}
    canonical = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
    recomputed = hashlib.sha256(canonical).hexdigest()
    if recomputed != manifest["manifest_sha256"]:
        print("SABOTAGE: manifest_sha256 mismatch", file=sys.stderr)
        sys.exit(1)

    # Count consistency
    if len(manifest["entries"]) != manifest["entry_count"]:
        print("SABOTAGE: entry_count mismatch", file=sys.stderr)
        sys.exit(1)

    # Per-entry validation: scope, existence, and content integrity
    seen_files = set()
    for entry in manifest["entries"]:
        if not isinstance(entry, dict):
            errors.append(f"INVALID_TYPE: entry is not a dict")
            continue

        for field in ("path", "line", "name", "value", "source_sha256"):
            if field not in entry:
                errors.append(f"MISSING_ENTRY_FIELD: {field} in {entry.get('path', '<unknown>')}")

        fpath = REPO_ROOT / entry.get("path", "")
        seen_files.add(entry.get("path", ""))

        # Scope check: path must be within SRC_ROOTS
        if not _is_within_src_roots(fpath):
            errors.append(f"SCOPE_VIOLATION: {entry['path']} outside configured SRC_ROOTS")
            continue

        if not fpath.is_file():
            errors.append(f"SOURCE_MISSING: {entry['path']}")
            continue

        # Content integrity: SHA256 must match current file content
        actual_hash = sha256_file(fpath)
        if actual_hash != entry["source_sha256"]:
            errors.append(
                f"CONTENT_DRIFT: {entry['path']} expected={entry['source_sha256'][:16]}… "
                f"actual={actual_hash[:16]}…"
            )

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
        try:
            raw = sys.stdin.buffer.read().decode("utf-8-sig")
        except UnicodeDecodeError as exc:
            print(f"SABOTAGE: invalid UTF-8 input: {exc}", file=sys.stderr)
            sys.exit(65)
        try:
            manifest = json.loads(raw)
        except json.JSONDecodeError as exc:
            print(f"SABOTAGE: malformed JSON input: {exc}", file=sys.stderr)
            sys.exit(66)
        if not isinstance(manifest, dict):
            print("SABOTAGE: JSON root is not an object", file=sys.stderr)
            sys.exit(67)
        verify_manifest(manifest)
        print(f"EXT-07 VERIFY OK: {manifest['entry_count']} entries", file=sys.stderr)
    else:
        print(f"Unknown mode: {mode}. Use 'extract' or 'verify'.", file=sys.stderr)
        sys.exit(64)


if __name__ == "__main__":
    main()