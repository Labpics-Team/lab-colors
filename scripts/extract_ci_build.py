#!/usr/bin/env python3
"""Inventory of direct .yml and .yaml workflow files in .github/workflows.

Nested directories are not GitHub workflow entrypoints. Verification checks
schema, count, self-hash and the complete current file set. Paths from an input
record are never used for file access. Unreadable inputs and links are refused.
Historical drift is checked separately by scripts/artifact_matrix.py.
"""
import sys
from pathlib import Path

from file_manifest import build_file_manifest, run_file_manifest, verify_file_manifest
from source_tree import regular_sources

REPO_ROOT = Path(__file__).resolve().parent.parent
WORKFLOWS_ROOT = REPO_ROOT / ".github" / "workflows"


def extract_manifest() -> dict:
    paths = regular_sources(WORKFLOWS_ROOT, {".yml", ".yaml"}, recursive=False)
    return build_file_manifest(REPO_ROOT, ".github/workflows", "ci-build-release", paths)


def verify_manifest(manifest: dict) -> None:
    verify_file_manifest(manifest, extract_manifest())


def main() -> None:
    raise SystemExit(run_file_manifest(extract_manifest, verify_manifest, "EXT-09", sys.argv[1:]))


if __name__ == "__main__":
    main()
