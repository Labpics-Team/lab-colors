#!/usr/bin/env python3
"""Regenerate clean-set receipt against FETCH_HEAD (merge ref).

Uses canonical JSON (sorted keys, 2-space indent, LF, no BOM).
"""
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RECEIPT_PATH = ROOT / "crates/labcolors-core/contracts/clean-set-srgb8-v1/receipt-v1.json"
PIN_PATH = ROOT / "crates/labcolors-core/contracts/clean-set-srgb8-v1/receipt-v1.sha256"


def canonical_json(value):
    return json.dumps(value, allow_nan=False, ensure_ascii=True, indent=2, sort_keys=True) + "\n"


def git_show(path):
    return subprocess.check_output(
        ["git", "-C", str(ROOT), "show", f"FETCH_HEAD:{path}"],
        stderr=subprocess.STDOUT,
    )


def sha256hex(data):
    return hashlib.sha256(data).hexdigest()


receipt = json.loads(RECEIPT_PATH.read_bytes())

changed_artifacts = 0
for entry in receipt["artifacts"]:
    data = git_show(entry["path"])
    new_bytes = len(data)
    new_sha = sha256hex(data)
    if entry["bytes"] != new_bytes or entry["sha256"] != new_sha:
        print(f"artifact {entry['role']}: {entry['bytes']}->{new_bytes} bytes")
        entry["bytes"] = new_bytes
        entry["sha256"] = new_sha
        changed_artifacts += 1

changed_legal = 0
for entry in receipt.get("license_scope", {}).get("legal_files", []):
    data = git_show(entry["path"])
    new_bytes = len(data)
    new_sha = sha256hex(data)
    if entry["bytes"] != new_bytes or entry["sha256"] != new_sha:
        print(f"legal {entry['role']}: {entry['bytes']}->{new_bytes} bytes")
        entry["bytes"] = new_bytes
        entry["sha256"] = new_sha
        changed_legal += 1

if changed_artifacts == 0 and changed_legal == 0:
    print("No changes needed — receipt already matches FETCH_HEAD.")
    sys.exit(0)

new_receipt_bytes = canonical_json(receipt).encode("ascii")
RECEIPT_PATH.write_bytes(new_receipt_bytes)
print(f"Wrote {RECEIPT_PATH} ({len(new_receipt_bytes)} bytes)")

pin_content = f"{sha256hex(new_receipt_bytes)}  receipt-v1.json\n"
PIN_PATH.write_bytes(pin_content.encode("ascii"))
print(f"Wrote {PIN_PATH}")
print(f"New receipt SHA-256: {sha256hex(new_receipt_bytes)}")
print(f"Changed: {changed_artifacts} artifacts, {changed_legal} legal files")