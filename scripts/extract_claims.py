#!/usr/bin/env python3
"""EXT-06: Public claims artifact class extractor.

Produces a finite manifest of verifiable claims in production Rust source files:
assert!, debug_assert!, const assertions, #[test] functions, and doc contract
sections (# Panics, # Safety, # Invariants). Each claim carries path, line,
kind, trimmed expression, and file SHA-256 for sabotage detection.

Sabotage controls:
- Refuses missing/unreadable source roots instead of certifying a partial inventory.
- Fails if manifest_sha256 does not match recomputed canonical hash.
- Fails if claim_count != actual claims.
- Compares every recorded claim with a fresh extraction from the current sources.

Exit evidence: JSON manifest on stdout with schema:
{
  "class": "claims",
  "schema_version": 1,
  "claim_count": <int>,
  "claims": [{"path": "<str>", "line": <int>, "kind": "<str>", "expression": "<str>", "source_sha256": "<hex>"}],
  "manifest_sha256": "<hex>"
}
"""
import hashlib
import json
import os
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

# Patterns for claim extraction
ASSERT_PATTERN = re.compile(r'^\s*(?:debug_)?assert!\s*\(', re.MULTILINE)
CONST_ASSERT_PATTERN = re.compile(r'^\s*const\s+_\s*:\s*\(\)\s*=\s*assert!', re.MULTILINE)
TEST_PATTERN = re.compile(r'^\s*#\[(?:cfg\(test\)\s*)?test\]', re.MULTILINE)
DOC_CONTRACT_PATTERN = re.compile(r'^\s*///\s*#\s+(Panics|Safety|Invariants)', re.MULTILINE)


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def rust_sources(src_root: Path) -> list[Path]:
    """Enumerate the complete declared tree; inaccessible paths cannot mean absence."""
    if not src_root.is_dir() or src_root.is_symlink():
        raise FileNotFoundError(f"source root is missing or not a real directory: {src_root}")

    def fail(error: OSError) -> None:
        raise error

    files = []
    for directory, subdirs, names in os.walk(src_root, onerror=fail):
        # Following symlinks can escape the proof scope or introduce cycles;
        # skipping them would instead silently certify an incomplete tree.
        for name in [*subdirs, *names]:
            path = Path(directory) / name
            if path.is_symlink():
                raise ValueError(f"symlink in source tree: {path}")
        subdirs.sort()
        files.extend(Path(directory) / name for name in names if name.endswith(".rs"))
    return sorted(files)


def extract_claims() -> list[dict]:
    claims = []

    for src_root in SRC_ROOTS:
        for rs_file in rust_sources(src_root):
            rel = rs_file.relative_to(REPO_ROOT).as_posix()
            text = rs_file.read_text(encoding="utf-8")
            file_hash = sha256_file(rs_file)
            lines = text.split("\n")

            for i, line in enumerate(lines, start=1):
                kind = None
                expr = line.strip()

                if CONST_ASSERT_PATTERN.match(line):
                    kind = "compile-time-invariant"
                elif ASSERT_PATTERN.search(line):
                    kind = "production-invariant"
                elif TEST_PATTERN.match(line):
                    kind = "test-contract"
                elif DOC_CONTRACT_PATTERN.match(line):
                    kind = "doc-contract"

                if kind:
                    claims.append({
                        "path": rel,
                        "line": i,
                        "kind": kind,
                        "expression": expr[:200],  # truncate long expressions
                        "source_sha256": file_hash,
                    })

    # Deterministic sort by (path, line)
    claims.sort(key=lambda c: (c["path"], c["line"]))
    return claims


def build_manifest(claims: list[dict]) -> dict:
    manifest = {
        "class": "claims",
        "schema_version": 1,
        "claim_count": len(claims),
        "claims": claims,
    }
    canonical = json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
    manifest["manifest_sha256"] = hashlib.sha256(canonical).hexdigest()
    return manifest


def verify_manifest(manifest: dict) -> None:
    """Sabotage: verify manifest self-consistency."""
    errors = []
    for field in ("class", "schema_version", "claim_count", "claims", "manifest_sha256"):
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

    if len(manifest["claims"]) != manifest["claim_count"]:
        print("SABOTAGE: claim_count mismatch", file=sys.stderr)
        sys.exit(1)

    # A self-consistent, freshly rehashed manifest can still omit a real claim
    # or lie about its source digest. Compare the full live extraction instead
    # of trusting paths supplied by the manifest being verified.
    current = build_manifest(extract_claims())
    if manifest != current:
        print("SABOTAGE: manifest differs from current source claims", file=sys.stderr)
        sys.exit(1)


def main() -> None:
    mode = sys.argv[1] if len(sys.argv) > 1 else "extract"
    if mode in ("extract", "verify"):
        try:
            if mode == "extract":
                print(json.dumps(build_manifest(extract_claims()), sort_keys=True, indent=2))
            else:
                raw = sys.stdin.buffer.read().decode("utf-8-sig")
                manifest = json.loads(raw)
                verify_manifest(manifest)
                print(f"EXT-06 VERIFY OK: {manifest['claim_count']} claims", file=sys.stderr)
        except (OSError, ValueError) as error:
            print(f"SABOTAGE: unable to enumerate claims: {error}", file=sys.stderr)
            sys.exit(1)
    else:
        print(f"Unknown mode: {mode}. Use 'extract' or 'verify'.", file=sys.stderr)
        sys.exit(64)


if __name__ == "__main__":
    main()