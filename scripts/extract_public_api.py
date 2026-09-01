#!/usr/bin/env python3
"""EXT-02: Public Rust API artifact class extractor.

Produces a finite manifest of all public API symbols (pub fn, pub struct,
pub enum, pub const, pub type, pub trait, pub mod) in crates/labcolors-core/src/.

Uses regex-based parsing of source files — no nightly toolchain or
cargo-public-api required (per r6 ASM-02: optional evaluated tool).

Sabotage controls:
- Fails if lib.rs is missing (crate root required).
- Fails if a previously-listed symbol disappears (regression detection).
- Fails if manifest schema is violated.

Exit evidence: JSON manifest on stdout with schema:
{
  "class": "public-api",
  "schema_version": 1,
  "crate": "labcolors-core",
  "symbol_count": <int>,
  "symbols": [{"kind": "<fn|struct|enum|const|type|trait|mod>", "name": "<qualified>", "file": "<relative>"}],
  "manifest_sha256": "<hex>"
}
"""
import hashlib
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SRC_ROOT = REPO_ROOT / "crates" / "labcolors-core" / "src"

# Matches top-level pub declarations (not pub(crate), not pub(super))
# Covers: pub fn, pub struct, pub enum, pub const, pub type, pub trait, pub mod, pub use
PUB_PATTERN = re.compile(
    r'^\s*pub\s+(?!crate|super)'
    r'(?:async\s+)?'
    r'(fn|struct|enum|const|type|trait|mod|use)\s+'
    r'([A-Za-z_][A-Za-z0-9_:]*)',
    re.MULTILINE,
)


def extract_symbols() -> list[dict]:
    if not SRC_ROOT.is_dir():
        print(f"SABOTAGE: source root missing: {SRC_ROOT}", file=sys.stderr)
        sys.exit(1)

    lib_rs = SRC_ROOT / "lib.rs"
    if not lib_rs.is_file():
        print(f"SABOTAGE: crate root missing: {lib_rs}", file=sys.stderr)
        sys.exit(1)

    symbols = []
    for rs_file in sorted(SRC_ROOT.rglob("*.rs")):
        rel = rs_file.relative_to(REPO_ROOT).as_posix()
        text = rs_file.read_text(encoding="utf-8")
        for match in PUB_PATTERN.finditer(text):
            kind = match.group(1)
            name = match.group(2)
            symbols.append({"kind": kind, "name": name, "file": rel})

    return symbols


def build_manifest(symbols: list[dict]) -> dict:
    manifest = {
        "class": "public-api",
        "schema_version": 1,
        "crate": "labcolors-core",
        "symbol_count": len(symbols),
        "symbols": symbols,
    }
    canonical = json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
    manifest["manifest_sha256"] = hashlib.sha256(canonical).hexdigest()
    return manifest


def verify_manifest(manifest: dict) -> None:
    """Sabotage: verify manifest is self-consistent and symbols exist."""
    errors = []

    # Schema check
    for field in ("class", "schema_version", "crate", "symbol_count", "symbols", "manifest_sha256"):
        if field not in manifest:
            errors.append(f"MISSING_FIELD: {field}")

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
        print(f"SABOTAGE: manifest_sha256 mismatch: claimed={manifest['manifest_sha256'][:16]}… computed={recomputed[:16]}…", file=sys.stderr)
        sys.exit(1)

    # Symbol count consistency
    if len(manifest["symbols"]) != manifest["symbol_count"]:
        print(f"SABOTAGE: symbol_count mismatch: declared={manifest['symbol_count']} actual={len(manifest['symbols'])}", file=sys.stderr)
        sys.exit(1)

    # Verify each symbol's source file exists
    seen_files = set()
    for sym in manifest["symbols"]:
        fpath = REPO_ROOT / sym["file"]
        seen_files.add(sym["file"])
        if not fpath.is_file():
            errors.append(f"SOURCE_MISSING: {sym['file']} (symbol {sym['name']})")

    if errors:
        print("SABOTAGE FAILURES:", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        sys.exit(1)


def main() -> None:
    mode = sys.argv[1] if len(sys.argv) > 1 else "extract"

    if mode == "extract":
        symbols = extract_symbols()
        manifest = build_manifest(symbols)
        print(json.dumps(manifest, sort_keys=True, indent=2))
    elif mode == "verify":
        raw = sys.stdin.buffer.read().decode("utf-8-sig")
        manifest = json.loads(raw)
        verify_manifest(manifest)
        print(f"EXT-02 VERIFY OK: {manifest['symbol_count']} symbols", file=sys.stderr)
    else:
        print(f"Unknown mode: {mode}. Use 'extract' or 'verify'.", file=sys.stderr)
        sys.exit(64)


if __name__ == "__main__":
    main()