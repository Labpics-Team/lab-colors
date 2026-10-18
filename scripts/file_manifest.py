"""File inventory format shared by source and workflow auditing.

The extractor owns its scope. Verification reads only independently enumerated
paths and never resolves a path supplied by the record being checked.
"""
import hashlib
import json
import sys
from pathlib import Path


def canonical(value: dict) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()


def file_sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def build_file_manifest(repo: Path, scope: str, kind: str, paths: list[Path]) -> dict:
    files = [
        {"path": path.relative_to(repo).as_posix(),
         "sha256": file_sha256(path)}
        for path in sorted(paths)
    ]
    payload = {"class": kind, "schema_version": 1, "root": scope,
               "file_count": len(files), "files": files}
    return {**payload, "manifest_sha256": hashlib.sha256(canonical(payload)).hexdigest()}


def reject(errors: list[str]) -> None:
    if errors:
        print("SABOTAGE FAILURES:", file=sys.stderr)
        for error in errors:
            print("  - " + error, file=sys.stderr)
        raise SystemExit(1)


def verify_file_manifest(manifest: object, current: dict) -> None:
    if not isinstance(manifest, dict):
        reject(["MALFORMED: record is not an object"])
    errors = []
    if set(manifest) != set(current):
        errors.append("MALFORMED: unexpected or missing record fields")
    if manifest.get("class") != current["class"]:
        errors.append("SCHEMA: unexpected class")
    if type(manifest.get("schema_version")) is not int or manifest["schema_version"] != 1:
        errors.append("SCHEMA: unexpected version")
    if manifest.get("root") != current["root"]:
        errors.append("ROOT: unexpected source root")
    files = manifest.get("files")
    if not isinstance(files, list):
        errors.append("MALFORMED: files is not a list")
    elif type(manifest.get("file_count")) is not int or manifest["file_count"] != len(files):
        errors.append("COUNT: file_count does not match entries")
    payload = {k: v for k, v in manifest.items() if k != "manifest_sha256"}
    if manifest.get("manifest_sha256") != hashlib.sha256(canonical(payload)).hexdigest():
        errors.append("HASH: manifest_sha256 mismatch")
    reject(errors)

    actual = {entry["path"]: entry["sha256"] for entry in current["files"]}
    seen = set()
    for entry in files:
        if not isinstance(entry, dict) or set(entry) != {"path", "sha256"}:
            errors.append("MALFORMED: unexpected source entry")
            continue
        path, digest = entry["path"], entry["sha256"]
        if not isinstance(path, str) or not isinstance(digest, str):
            errors.append("MALFORMED: path and sha256 must be strings")
            continue
        if path in seen:
            errors.append("DUPLICATE: " + path)
        seen.add(path)
        if path not in actual:
            errors.append("MISSING: " + path)
        elif digest != actual[path]:
            errors.append("DRIFT: " + path)
    errors.extend("UNLISTED: " + path for path in sorted(actual.keys() - seen))
    reject(errors)


def run_file_manifest(extract, verify, label: str, args: list[str]) -> int:
    if len(args) > 1 or (args and args[0] not in {"extract", "verify"}):
        print("Use 'extract' or 'verify'.", file=sys.stderr)
        return 64
    mode = args[0] if args else "extract"
    if mode == "verify":
        try:
            manifest = json.loads(sys.stdin.buffer.read().decode("utf-8-sig"))
        except (OSError, ValueError) as error:
            print("SABOTAGE: invalid manifest input: " + str(error), file=sys.stderr)
            return 1
    try:
        if mode == "extract":
            print(json.dumps(extract(), sort_keys=True, indent=2, allow_nan=False))
        else:
            verify(manifest)
            print(f"{label} VERIFY OK: {manifest['file_count']} files", file=sys.stderr)
    except (OSError, ValueError) as error:
        print("SABOTAGE: unable to enumerate sources: " + str(error), file=sys.stderr)
        return 1
    return 0
