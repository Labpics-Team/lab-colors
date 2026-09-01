#!/usr/bin/env python3
"""EXT-04: Operations artifact class extractor.

Produces a finite manifest of public operations in production Rust source files:
pub fn/method declarations that represent semantic operations over color data
(transform, evaluate, convert, solve, compute, derive, assess). Each operation
carries path, line, function name, signature excerpt, and file SHA-256.

Sabotage controls:
- Fails if no crate source roots exist.
- Fails if manifest_sha256 does not match recomputed canonical hash.
- Fails if operation_count != actual operations.

Exit evidence: JSON manifest on stdout with schema:
{
  "class": "operations",
  "schema_version": 1,
  "operation_count": <int>,
  "operations": [{"path": "<str>", "line": <int>, "name": "<str>", "signature": "<str>", "source_sha256": "<hex>"}],
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

# Matches pub fn declarations (not pub(crate), not pub(super))
PUB_FN_PATTERN = re.compile(
    r'^\s*pub\s+(?!crate|super)(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)',
    re.MULTILINE,
)


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def extract_operations() -> list[dict]:
    operations = []
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
                match = PUB_FN_PATTERN.match(line)
                if match:
                    fn_name = match.group(1)
                    # Capture signature excerpt (up to closing paren or 200 chars)
                    sig_end = line.find(")") + 1
                    if sig_end == 0:
                        sig_end = min(len(line), 200)
                    signature = line.strip()[:sig_end][:200]
                    operations.append({
                        "path": rel,
                        "line": i,
                        "name": fn_name,
                        "signature": signature,
                        "source_sha256": file_hash,
                    })

    if not found_any_root:
        print("SABOTAGE: no crate source roots found", file=sys.stderr)
        sys.exit(1)

    operations.sort(key=lambda o: (o["path"], o["line"]))
    return operations


def build_manifest(operations: list[dict]) -> dict:
    manifest = {
        "class": "operations",
        "schema_version": 1,
        "operation_count": len(operations),
        "operations": operations,
    }
    canonical = json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
    manifest["manifest_sha256"] = hashlib.sha256(canonical).hexdigest()
    return manifest


def verify_manifest(manifest: dict) -> None:
    """Sabotage: verify manifest self-consistency."""
    errors = []
    for field in ("class", "schema_version", "operation_count", "operations", "manifest_sha256"):
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

    if len(manifest["operations"]) != manifest["operation_count"]:
        print("SABOTAGE: operation_count mismatch", file=sys.stderr)
        sys.exit(1)

    seen_files = set()
    for op in manifest["operations"]:
        fpath = REPO_ROOT / op["path"]
        seen_files.add(op["path"])
        if not fpath.is_file():
            errors.append(f"SOURCE_MISSING: {op['path']}")
    if errors:
        print("SABOTAGE FAILURES:", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        sys.exit(1)


def main() -> None:
    mode = sys.argv[1] if len(sys.argv) > 1 else "extract"
    if mode == "extract":
        operations = extract_operations()
        manifest = build_manifest(operations)
        print(json.dumps(manifest, sort_keys=True, indent=2))
    elif mode == "verify":
        raw = sys.stdin.buffer.read().decode("utf-8-sig")
        manifest = json.loads(raw)
        verify_manifest(manifest)
        print(f"EXT-04 VERIFY OK: {manifest['operation_count']} operations", file=sys.stderr)
    else:
        print(f"Unknown mode: {mode}. Use 'extract' or 'verify'.", file=sys.stderr)
        sys.exit(64)


if __name__ == "__main__":
    main()