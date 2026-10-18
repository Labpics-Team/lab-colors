#!/usr/bin/env python3
"""Inventory of Rust source files, including in-crate tests.

The scope is crates/labcolors-core/src. Verification compares file identities
with the complete live tree; it does not classify runtime reachability.
Historical drift is checked separately by scripts/artifact_matrix.py.
"""
import sys
from pathlib import Path

from file_manifest import build_file_manifest, run_file_manifest, verify_file_manifest
from source_tree import rust_sources

REPO_ROOT = Path(__file__).resolve().parent.parent
SRC_ROOT = REPO_ROOT / "crates" / "labcolors-core" / "src"


def extract_manifest() -> dict:
    return build_file_manifest(REPO_ROOT, "crates/labcolors-core/src", "source-files",
                               rust_sources(SRC_ROOT))


def verify_manifest(manifest: dict) -> None:
    verify_file_manifest(manifest, extract_manifest())


def main() -> None:
    raise SystemExit(run_file_manifest(extract_manifest, verify_manifest, "EXT-01", sys.argv[1:]))


if __name__ == "__main__":
    main()
