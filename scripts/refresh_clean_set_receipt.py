#!/usr/bin/env python3
"""Обновить точные привязки исходников в квитанции и её единственный SHA-256 pin.

Численные данные и научный допуск не изменяются. После обновления обязательны
независимые проверки квитанции и её источников.
"""
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RECEIPT_PATH = ROOT / "crates/labcolors-core/contracts/clean-set-srgb8-v2/receipt-v2.json"
PIN_PATH = ROOT / "crates/labcolors-core/contracts/clean-set-srgb8-v2/receipt-v2.sha256"


def canonical_json(value):
    return json.dumps(value, allow_nan=False, ensure_ascii=True, indent=2, sort_keys=True) + "\n"


def sha256hex(data):
    return hashlib.sha256(data).hexdigest()


receipt = json.loads(RECEIPT_PATH.read_bytes())

changed_artifacts = 0
for entry in receipt["artifacts"]:
    data = (ROOT / entry["path"]).read_bytes()
    new_bytes = len(data)
    new_sha = sha256hex(data)
    if entry["bytes"] != new_bytes or entry["sha256"] != new_sha:
        print(f"artifact {entry['role']}: {entry['bytes']}->{new_bytes} bytes")
        entry["bytes"] = new_bytes
        entry["sha256"] = new_sha
        changed_artifacts += 1

changed_legal = 0
for entry in receipt.get("license_scope", {}).get("legal_files", []):
    data = (ROOT / entry["path"]).read_bytes()
    new_bytes = len(data)
    new_sha = sha256hex(data)
    if entry["bytes"] != new_bytes or entry["sha256"] != new_sha:
        print(f"legal {entry['role']}: {entry['bytes']}->{new_bytes} bytes")
        entry["bytes"] = new_bytes
        entry["sha256"] = new_sha
        changed_legal += 1

if changed_artifacts == 0 and changed_legal == 0:
    print("No changes needed — receipt already matches working tree.")
    sys.exit(0)

new_receipt_bytes = canonical_json(receipt).encode("ascii")
RECEIPT_PATH.write_bytes(new_receipt_bytes)
print(f"Wrote {RECEIPT_PATH} ({len(new_receipt_bytes)} bytes)")

pin_content = f"{sha256hex(new_receipt_bytes)}  receipt-v2.json\n"
PIN_PATH.write_bytes(pin_content.encode("ascii"))
print(f"Wrote {PIN_PATH}")
print(f"New receipt SHA-256: {sha256hex(new_receipt_bytes)}")
print(f"Changed: {changed_artifacts} artifacts, {changed_legal} legal files")
