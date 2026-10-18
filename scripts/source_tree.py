"""Strict inventory of Rust source paths for source/claims proof extractors.

These scripts must never interpret a failed directory walk as a verified absence.
A symlink is refused instead of silently skipped or traversed outside the scope.
"""
import os
from pathlib import Path


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


