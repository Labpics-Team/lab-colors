"""Complete source inventories. An unreadable tree is not a verified absence."""
import os
from pathlib import Path


def regular_sources(src_root: Path, suffixes: set[str], *, recursive: bool = True) -> list[Path]:
    """Enumerate regular source files without following links or losing walk errors."""
    if not src_root.is_dir() or src_root.is_symlink():
        raise FileNotFoundError(f"source root is missing or not a real directory: {src_root}")

    def fail(error: OSError) -> None:
        raise error

    files = []
    for directory, subdirs, names in os.walk(src_root, onerror=fail):
        for name in [*subdirs, *names]:
            path = Path(directory) / name
            if path.is_symlink():
                raise ValueError(f"symlink in source tree: {path}")
        for name in names:
            path = Path(directory) / name
            if path.suffix in suffixes:
                if not path.is_file():
                    raise ValueError(f"not a regular source file: {path}")
                files.append(path)
        if not recursive:
            break
        subdirs.sort()
    return sorted(files)


def rust_sources(src_root: Path) -> list[Path]:
    return regular_sources(src_root, {".rs"})
