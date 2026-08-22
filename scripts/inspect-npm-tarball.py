#!/usr/bin/env python3
"""Fail-closed structural inspection for an npm package tarball."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import re
import stat
import sys
import tarfile
import unicodedata
import zlib
from pathlib import Path
from typing import NoReturn


SCHEMA_VERSION = 1
TAR_BLOCK_BYTES = 512
TAR_END_BLOCKS = 2

# These are abuse ceilings, not product-size budgets. The release has separate
# byte-exact WASM budgets; these limits only bound hostile parser work.
MAX_COMPRESSED_TARBALL_BYTES = 16 * 1024 * 1024
MAX_TOTAL_FILE_BYTES = 64 * 1024 * 1024
MAX_DECLARED_MEMBERS = 4_096
MAX_TAR_STREAM_BYTES = (
    MAX_TOTAL_FILE_BYTES
    + MAX_DECLARED_MEMBERS * TAR_BLOCK_BYTES
    + TAR_END_BLOCKS * TAR_BLOCK_BYTES
)
# A USTAR member path carries at most 100 name + 1 slash + 155 prefix = 256
# bytes, and a canonical declared inventory must mirror exactly those tar
# members. This ceiling is the abuse bound for that declaration (4096 members
# times the USTAR path width plus a generous JSON envelope), not a product-size
# budget, so a valid inventory can never trip it.
MAX_DECLARED_INVENTORY_BYTES = MAX_DECLARED_MEMBERS * 256 + 64 * 1024

_DRIVE_PATH = re.compile(r"^[A-Za-z]:")
_WINDOWS_FORBIDDEN_FILENAME_CHARACTERS = frozenset('<>:"/\\|?*')
_WINDOWS_SUPERSCRIPT_DEVICE_DIGITS = ("\u00b9", "\u00b2", "\u00b3")
_WINDOWS_RESERVED = frozenset({
    "aux",
    "con",
    "conin$",
    "conout$",
    "nul",
    "prn",
    *(f"com{index}" for index in range(1, 10)),
    *(f"lpt{index}" for index in range(1, 10)),
    *(f"com{digit}" for digit in _WINDOWS_SUPERSCRIPT_DEVICE_DIGITS),
    *(f"lpt{digit}" for digit in _WINDOWS_SUPERSCRIPT_DEVICE_DIGITS),
})
_WINDOWS_REPARSE_POINT = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)


class InspectionError(ValueError):
    """The archive or declared inventory is not canonical."""


def _fail(message: str) -> NoReturn:
    raise InspectionError(message)


def _portable_path_key(path: str) -> str:
    return unicodedata.normalize("NFC", path).casefold()


def _validate_segment(segment: str, label: str) -> None:
    if segment in {"", ".", ".."}:
        _fail(f"{label} contains an empty or traversal segment")
    if segment.endswith((" ", ".")):
        _fail(f"{label} is ambiguous on Windows: {segment!r}")
    forbidden = next(
        (
            character
            for character in segment
            if character in _WINDOWS_FORBIDDEN_FILENAME_CHARACTERS
        ),
        None,
    )
    if forbidden is not None:
        _fail(
            f"{label} contains a forbidden Windows filename character: "
            f"{forbidden!r}"
        )
    device = segment.split(".", 1)[0].casefold()
    if device in _WINDOWS_RESERVED:
        _fail(f"{label} contains a reserved Windows device name: {segment!r}")


def _normalise_relative_path(path: object, label: str) -> str:
    if not isinstance(path, str) or not path:
        _fail(f"{label} must be a non-empty string")
    if unicodedata.normalize("NFC", path) != path:
        _fail(f"{label} is not Unicode NFC")
    if any(ord(character) < 0x20 or ord(character) == 0x7F for character in path):
        _fail(f"{label} contains an ASCII control character")
    if "\\" in path:
        _fail(f"{label} contains an ambiguous backslash")
    if path.startswith("/") or _DRIVE_PATH.match(path):
        _fail(f"{label} is absolute or drive-qualified")

    segments = path.split("/")
    for segment in segments:
        _validate_segment(segment, label)
    normalised = "/".join(segments)
    if normalised != path:
        _fail(f"{label} is not canonical")
    return normalised


def _normalise_member_path(raw_path: str) -> str:
    if not raw_path.startswith("package/"):
        _fail(f"tar member is outside the single package/ namespace: {raw_path!r}")
    return _normalise_relative_path(raw_path.removeprefix("package/"), "tar member path")


def _reject_duplicate_object_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
    value: dict[str, object] = {}
    for key, child in pairs:
        if key in value:
            _fail(f"declared inventory contains duplicate JSON key: {key}")
        value[key] = child
    return value


def _load_declared_inventory(path: Path) -> list[str]:
    try:
        source = _read_regular_file_snapshot(
            path,
            MAX_DECLARED_INVENTORY_BYTES,
            "declared inventory",
        ).decode("utf-8", errors="strict")
        payload = json.loads(source, object_pairs_hook=_reject_duplicate_object_keys)
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        _fail(f"cannot read declared inventory: {error}")

    if not isinstance(payload, dict) or set(payload) != {"schemaVersion", "files"}:
        _fail("declared inventory must have exactly schemaVersion and files")
    if type(payload["schemaVersion"]) is not int or payload["schemaVersion"] != SCHEMA_VERSION:
        _fail(f"declared inventory schemaVersion must be {SCHEMA_VERSION}")
    files = payload["files"]
    if not isinstance(files, list) or not files:
        _fail("declared inventory files must be a non-empty array")
    if len(files) > MAX_DECLARED_MEMBERS:
        _fail(
            f"declared inventory has {len(files)} members; limit is {MAX_DECLARED_MEMBERS}"
        )

    canonical = [
        _normalise_relative_path(value, f"declared inventory files[{index}]")
        for index, value in enumerate(files)
    ]
    expected_order = sorted(canonical, key=lambda value: value.encode("utf-8"))
    if canonical != expected_order:
        _fail("declared inventory files must be unique and sorted by UTF-8 bytes")
    if len(set(canonical)) != len(canonical):
        _fail("declared inventory contains duplicate paths")
    portable_keys = [_portable_path_key(value) for value in canonical]
    if len(set(portable_keys)) != len(portable_keys):
        _fail("declared inventory contains portable case-folding path collisions")
    return canonical


def _parse_octal(field: bytes, label: str) -> int:
    if field and field[0] & 0x80:
        _fail(f"{label} uses a non-canonical base-256 integer")
    stripped = field.rstrip(b"\0 ").lstrip(b" ")
    if not stripped or any(byte not in b"01234567" for byte in stripped):
        _fail(f"{label} is not a canonical octal integer")
    return int(stripped, 8)


def _decode_ustar_text(field: bytes, label: str) -> str:
    value, separator, padding = field.partition(b"\0")
    if separator and any(padding):
        _fail(f"{label} has non-zero bytes after its NUL terminator")
    try:
        return value.decode("utf-8", errors="strict")
    except UnicodeDecodeError as error:
        _fail(f"{label} is not UTF-8: {error}")


def _decompress_single_gzip_member(compressed: bytes) -> bytes:
    decompressor = zlib.decompressobj(16 + zlib.MAX_WBITS)
    payload = bytearray()
    pending = compressed
    try:
        while pending:
            remaining = MAX_TAR_STREAM_BYTES - len(payload)
            if remaining < 0:
                _fail(f"tar stream exceeds {MAX_TAR_STREAM_BYTES} bytes")
            chunk = decompressor.decompress(pending, remaining + 1)
            payload.extend(chunk)
            if len(payload) > MAX_TAR_STREAM_BYTES:
                _fail(f"tar stream exceeds {MAX_TAR_STREAM_BYTES} bytes")
            next_pending = decompressor.unconsumed_tail
            if next_pending and not chunk and remaining == 0:
                _fail(f"tar stream exceeds {MAX_TAR_STREAM_BYTES} bytes")
            pending = next_pending
        remaining = MAX_TAR_STREAM_BYTES - len(payload)
        payload.extend(decompressor.flush(remaining + 1))
        if len(payload) > MAX_TAR_STREAM_BYTES:
            _fail(f"tar stream exceeds {MAX_TAR_STREAM_BYTES} bytes")
    except zlib.error as error:
        _fail(f"invalid gzip stream: {error}")
    if not decompressor.eof:
        _fail("gzip stream is truncated")
    if decompressor.unused_data:
        _fail("gzip stream has a concatenated member or trailing bytes")
    return bytes(payload)


def _is_reparse_point(metadata: os.stat_result) -> bool:
    return bool(
        getattr(metadata, "st_file_attributes", 0) & _WINDOWS_REPARSE_POINT
    )


def _require_regular_file(metadata: os.stat_result, label: str) -> None:
    if stat.S_ISLNK(metadata.st_mode) or _is_reparse_point(metadata):
        _fail(f"{label} path is a symbolic link or reparse point")
    if not stat.S_ISREG(metadata.st_mode):
        _fail(f"{label} path is not a regular file")


def _require_same_file(
    before: os.stat_result,
    after: os.stat_result,
    message: str,
) -> None:
    if not os.path.samestat(before, after):
        _fail(message)


def _read_regular_file_snapshot(path: Path, limit: int, label: str) -> bytes:
    """Read one regular-file snapshot without following or racing a path swap.

    Both the tarball and the declared inventory cross the same untrusted path
    boundary, so they share one lstat -> O_NOFOLLOW open -> fstat -> samestat
    discipline: a symlink, reparse point, non-regular node, or in-place swap
    between lstat and the final read fails the inspection closed.
    """
    descriptor: int | None = None
    try:
        path_before = os.lstat(path)
        _require_regular_file(path_before, label)
        flags = (
            os.O_RDONLY
            | getattr(os, "O_BINARY", 0)
            | getattr(os, "O_CLOEXEC", 0)
            | getattr(os, "O_NOFOLLOW", 0)
        )
        descriptor = os.open(path, flags)
        with os.fdopen(descriptor, "rb", closefd=False) as source:
            opened = os.fstat(descriptor)
            _require_regular_file(opened, label)
            _require_same_file(
                path_before,
                opened,
                f"{label} path changed between lstat and open",
            )
            path_after_open = os.lstat(path)
            _require_regular_file(path_after_open, label)
            _require_same_file(
                opened,
                path_after_open,
                f"{label} path changed after open",
            )
            if opened.st_size <= 0:
                _fail(f"{label} is empty")
            if opened.st_size > limit:
                _fail(f"{label} has {opened.st_size} bytes; limit is {limit}")
            snapshot = source.read(limit + 1)
            opened_after_read = os.fstat(descriptor)
            _require_regular_file(opened_after_read, label)
            _require_same_file(
                opened,
                opened_after_read,
                f"opened {label} identity changed while reading",
            )
            if (
                opened.st_size != opened_after_read.st_size
                or opened.st_mtime_ns != opened_after_read.st_mtime_ns
                or len(snapshot) != opened.st_size
            ):
                _fail(f"{label} changed while it was being snapshotted")
            path_after_read = os.lstat(path)
            _require_regular_file(path_after_read, label)
            _require_same_file(
                opened,
                path_after_read,
                f"{label} path changed while it was being snapshotted",
            )
    except OSError as error:
        _fail(f"cannot read {label}: {error}")
    finally:
        if descriptor is not None:
            os.close(descriptor)
    if len(snapshot) > limit:
        _fail(f"{label} exceeds {limit} bytes")
    return snapshot


def _read_snapshot(path: Path) -> bytes:
    return _read_regular_file_snapshot(path, MAX_COMPRESSED_TARBALL_BYTES, "tarball")


def _scan_raw_ustar(payload: bytes, maximum_members: int) -> tuple[list[dict[str, object]], int]:
    members: list[dict[str, object]] = []
    raw_paths: set[str] = set()
    normalised_paths: set[str] = set()
    portable_keys: set[str] = set()
    total_file_bytes = 0
    offset = 0

    while True:
        if offset + TAR_BLOCK_BYTES > len(payload):
            _fail("tar stream is missing its two end-of-archive blocks")
        header = payload[offset : offset + TAR_BLOCK_BYTES]
        if header == bytes(TAR_BLOCK_BYTES):
            expected_end = offset + TAR_END_BLOCKS * TAR_BLOCK_BYTES
            if expected_end != len(payload):
                _fail("tar stream must end with exactly two zero blocks")
            if payload[offset + TAR_BLOCK_BYTES : expected_end] != bytes(TAR_BLOCK_BYTES):
                _fail("tar stream has only one end-of-archive zero block")
            break

        if len(members) >= maximum_members:
            _fail(f"tar member count exceeds declared limit {maximum_members}")
        if header[257:263] != b"ustar\0" or header[263:265] != b"00":
            _fail("tar member is not canonical POSIX USTAR")
        expected_checksum = _parse_octal(header[148:156], "tar header checksum")
        actual_checksum = sum(header[:148]) + 8 * ord(" ") + sum(header[156:])
        if actual_checksum != expected_checksum:
            _fail("tar header checksum is invalid")
        if header[156:157] != tarfile.REGTYPE:
            _fail(f"tar member type {header[156:157]!r} is forbidden; only regular files are allowed")
        if any(header[157:257]):
            _fail("regular tar member has a non-empty link target")

        name = _decode_ustar_text(header[0:100], "tar header name")
        prefix = _decode_ustar_text(header[345:500], "tar header prefix")
        if not name:
            _fail("tar header name is empty")
        raw_path = f"{prefix}/{name}" if prefix else name
        normalised_path = _normalise_member_path(raw_path)
        portable_key = _portable_path_key(normalised_path)
        if raw_path in raw_paths:
            _fail(f"tar contains duplicate raw member path: {raw_path}")
        if normalised_path in normalised_paths:
            _fail(f"tar contains duplicate normalised member path: {normalised_path}")
        if portable_key in portable_keys:
            _fail(f"tar contains portable case-folding path collision: {normalised_path}")

        size = _parse_octal(header[124:136], f"tar member size for {raw_path}")
        total_file_bytes += size
        if total_file_bytes > MAX_TOTAL_FILE_BYTES:
            _fail(
                f"tar regular-file bytes exceed {MAX_TOTAL_FILE_BYTES}: {total_file_bytes}"
            )
        data_offset = offset + TAR_BLOCK_BYTES
        data_end = data_offset + size
        padded_end = data_offset + ((size + TAR_BLOCK_BYTES - 1) // TAR_BLOCK_BYTES) * TAR_BLOCK_BYTES
        if padded_end > len(payload):
            _fail(f"tar member payload is truncated: {raw_path}")
        if any(payload[data_end:padded_end]):
            _fail(f"tar member has non-zero alignment padding: {raw_path}")
        content = payload[data_offset:data_end]

        members.append(
            {
                "index": len(members),
                "rawPath": raw_path,
                "normalizedPath": normalised_path,
                "type": "file",
                "size": size,
                "sha256": hashlib.sha256(content).hexdigest(),
            }
        )
        raw_paths.add(raw_path)
        normalised_paths.add(normalised_path)
        portable_keys.add(portable_key)
        offset = padded_end

    return members, total_file_bytes


def _cross_check_tarfile(snapshot: bytes, raw_members: list[dict[str, object]]) -> None:
    try:
        with tarfile.open(
            fileobj=io.BytesIO(snapshot),
            mode="r:gz",
            encoding="utf-8",
            errors="strict",
        ) as archive:
            if archive.pax_headers:
                _fail("global PAX headers are forbidden")
            parsed = archive.getmembers()
    except (tarfile.TarError, UnicodeError, OSError) as error:
        _fail(f"stdlib tarfile rejected the archive: {error}")

    if len(parsed) != len(raw_members):
        _fail("raw USTAR and stdlib tarfile member counts differ")
    for raw, member in zip(raw_members, parsed, strict=True):
        if member.pax_headers:
            _fail(f"PAX headers are forbidden: {member.name}")
        if member.type != tarfile.REGTYPE or not member.isfile() or member.issparse():
            _fail(f"stdlib tarfile found a forbidden member type: {member.name}")
        if member.name != raw["rawPath"] or member.size != raw["size"]:
            _fail("raw USTAR and stdlib tarfile member records differ")


def inspect_tarball(tarball: Path, declared_inventory: Path) -> dict[str, object]:
    expected = _load_declared_inventory(declared_inventory)
    snapshot = _read_snapshot(tarball)
    payload = _decompress_single_gzip_member(snapshot)
    members, total_file_bytes = _scan_raw_ustar(payload, len(expected))
    _cross_check_tarfile(snapshot, members)

    actual = sorted(
        (str(member["normalizedPath"]) for member in members),
        key=lambda value: value.encode("utf-8"),
    )
    if actual != expected:
        missing = sorted(set(expected) - set(actual), key=lambda value: value.encode("utf-8"))
        undeclared = sorted(set(actual) - set(expected), key=lambda value: value.encode("utf-8"))
        _fail(
            "tar inventory differs from declaration: "
            f"missing={missing!r}, undeclared={undeclared!r}"
        )

    return {
        "schemaVersion": SCHEMA_VERSION,
        "verdict": "canonical",
        "tarball": {
            "bytes": len(snapshot),
            "sha256": hashlib.sha256(snapshot).hexdigest(),
        },
        "limits": {
            "maxMembers": len(expected),
            "maxTarballBytes": MAX_COMPRESSED_TARBALL_BYTES,
            "maxTotalFileBytes": MAX_TOTAL_FILE_BYTES,
        },
        "members": members,
        "inventory": {
            "files": actual,
            "totalFileBytes": total_file_bytes,
        },
    }


def _arguments(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tarball", required=True, type=Path)
    parser.add_argument("--declared-inventory-json", required=True, type=Path)
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    arguments = _arguments(argv)
    try:
        result = inspect_tarball(arguments.tarball, arguments.declared_inventory_json)
    except InspectionError as error:
        print(f"npm tarball rejected: {error}", file=sys.stderr)
        return 1
    json.dump(result, sys.stdout, ensure_ascii=True, separators=(",", ":"))
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
