#!/usr/bin/env python3
"""EXT-08: Decision sites artifact class extractor.

Produces a finite manifest of decision sites in production Rust source files:
match expressions, if/else branches with business logic, and enum dispatch
points. Each decision site carries path, line, kind, trimmed expression,
and file SHA-256 for sabotage detection.

Sabotage controls:
- Fails if no crate source roots exist.
- Fails if manifest_sha256 does not match recomputed canonical hash.
- Fails if decision_count != actual decisions.

Exit evidence: JSON manifest on stdout with schema:
{
  "class": "decisions",
  "schema_version": 1,
  "decision_count": <int>,
  "decisions": [{"path": "<str>", "line": <int>, "kind": "<str>", "expression": "<str>", "source_sha256": "<hex>"}],
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
    REPO_ROOT / "crates" / "labcolors-wasm" / "src",
    REPO_ROOT / "crates" / "labcolors-ffi" / "src",
    REPO_ROOT / "crates" / "labcolors-conformance" / "src",
]

# Patterns for decision site extraction
MATCH_PATTERN = re.compile(r'^\s*match\s+', re.MULTILINE)
IF_ELSE_PATTERN = re.compile(r'^\s*(?:}\s*)?else\s+if\s+', re.MULTILINE)
ENUM_DISPATCH_PATTERN = re.compile(r'^\s*(?:Self|Decision|Outcome|Result|Status)::', re.MULTILINE)


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def extract_decisions() -> list[dict]:
    decisions = []
    found_any_root = False

    for src_root in SRC_ROOTS:
        if not src_root.is_dir():
            continue
        found_any_root = True

        for rs_file in sorted(src_root.rglob("*.rs")):
            rel = rs_file.relative_to(REPO_ROOT).as_posix()
            text = rs_file.read_text(encoding="utf-8")
            file_hash = sha256_file(rs_file)
            lines = text.split("\n")

            for i, line in enumerate(lines, start=1):
                kind = None
                expr = line.strip()

                if MATCH_PATTERN.search(line):
                    kind = "match"
                elif IF_ELSE_PATTERN.search(line):
                    kind = "if-else"
                elif ENUM_DISPATCH_PATTERN.search(line):
                    kind = "enum-dispatch"

                if kind:
                    decisions.append({
                        "path": rel,
                        "line": i,
                        "kind": kind,
                        "expression": expr[:200],
                        "source_sha256": file_hash,
                    })

    if not found_any_root:
        print("SABOTAGE: no crate source roots found", file=sys.stderr)
        sys.exit(1)

    decisions.sort(key=lambda d: (d["path"], d["line"]))
    return decisions


def build_manifest(decisions: list[dict]) -> dict:
    manifest = {
        "class": "decisions",
        "schema_version": 1,
        "decision_count": len(decisions),
        "decisions": decisions,
    }
    canonical = json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
    manifest["manifest_sha256"] = hashlib.sha256(canonical).hexdigest()
    return manifest


def verify_manifest(manifest: dict) -> None:
    """Sabotage: verify manifest self-consistency."""
    errors = []
    for field in ("class", "schema_version", "decision_count", "decisions", "manifest_sha256"):
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

    if len(manifest["decisions"]) != manifest["decision_count"]:
        print("SABOTAGE: decision_count mismatch", file=sys.stderr)
        sys.exit(1)

    seen_files = set()
    for dec in manifest["decisions"]:
        fpath = REPO_ROOT / dec["path"]
        seen_files.add(dec["path"])
        if not fpath.is_file():
            errors.append(f"SOURCE_MISSING: {dec['path']}")
    if errors:
        print("SABOTAGE FAILURES:", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        sys.exit(1)


def main() -> None:
    mode = sys.argv[1] if len(sys.argv) > 1 else "extract"
    if mode == "extract":
        decisions = extract_decisions()
        manifest = build_manifest(decisions)
        print(json.dumps(manifest, sort_keys=True, indent=2))
    elif mode == "verify":
        raw = sys.stdin.buffer.read().decode("utf-8-sig")
        manifest = json.loads(raw)
        verify_manifest(manifest)
        print(f"EXT-08 VERIFY OK: {manifest['decision_count']} decisions", file=sys.stderr)
    else:
        print(f"Unknown mode: {mode}. Use 'extract' or 'verify'.", file=sys.stderr)
        sys.exit(64)


if __name__ == "__main__":
    main()