#!/usr/bin/env python3
"""Обновить точные привязки исходников в квитанции и её единственный SHA-256 pin.

Численные данные и научный допуск не изменяются. После обновления обязательны
независимые проверки квитанции и её источников.
"""
import hashlib
import json
import re
import sys
from pathlib import Path

from verify_clean_set_receipt import (
    PRODUCT_ARTIFACT_LICENSES,
    PRODUCT_ARTIFACT_PATHS,
    RELEASE_PATH,
    RESEARCH_RELEASE_SHA256,
)

ROOT = Path(__file__).resolve().parent.parent
RECEIPT_PATH = ROOT / "crates/labcolors-core/contracts/clean-set-srgb8-v2/receipt-v2.json"
PIN_PATH = ROOT / "crates/labcolors-core/contracts/clean-set-srgb8-v2/receipt-v2.sha256"


def canonical_json(value):
    return json.dumps(value, allow_nan=False, ensure_ascii=True, indent=2, sort_keys=True) + "\n"


def sha256hex(data):
    return hashlib.sha256(data).hexdigest()


receipt = json.loads(RECEIPT_PATH.read_bytes())

# The immutable scientific release is the authority for the runtime selector.
# Refreshing product provenance must not retain a selector from its predecessor.
release_sha = sha256hex((ROOT / RELEASE_PATH).read_bytes())
if (release_sha != RESEARCH_RELEASE_SHA256
        or receipt["research"]["release_sha256"] != RESEARCH_RELEASE_SHA256):
    raise ValueError("scientific release and receipt must match the verifier-pinned identity")
classifier_path = ROOT / PRODUCT_ARTIFACT_PATHS["classifier_source"]
classifier = classifier_path.read_bytes()
selector = re.compile(
    rb"(pub\(crate\) const EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2: \[u8; 32\] = \[\n)"
    rb"(?:    (?:0x[0-9a-f]{2}, ){15}0x[0-9a-f]{2},\n){2}(\];)"
)
values = [f"0x{value:02x}" for value in bytes.fromhex(release_sha)]
body = "".join("    " + ", ".join(values[start:start + 16]) + ",\n" for start in (0, 16)).encode("ascii")
updated, count = selector.subn(lambda match: match[1] + body + match[2], classifier)
if count != 1:
    raise ValueError("expected exactly one canonical runtime release selector")
changed_artifacts = 0
previous_artifacts = {entry["role"]: entry for entry in receipt["artifacts"]}
artifacts = []
for role, path in sorted(PRODUCT_ARTIFACT_PATHS.items()):
    data = updated if role == "classifier_source" else (ROOT / path).read_bytes()
    entry = {"role": role, "path": path, "bytes": len(data),
             "sha256": sha256hex(data), "license": PRODUCT_ARTIFACT_LICENSES[role]}
    previous = previous_artifacts.get(role)
    if entry != previous:
        print(f"artifact {role}: {previous['bytes'] if previous else 0}->{len(data)} bytes")
        changed_artifacts += 1
    artifacts.append(entry)
changed_artifacts += len(previous_artifacts.keys() - PRODUCT_ARTIFACT_PATHS.keys())
receipt["artifacts"] = artifacts

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

if updated == classifier and changed_artifacts == 0 and changed_legal == 0:
    print("No changes needed: receipt already matches working tree.")
    sys.exit(0)

new_receipt_bytes = canonical_json(receipt).encode("ascii")
pin_bytes = f"{sha256hex(new_receipt_bytes)}  receipt-v2.json\n".encode("ascii")

# Read every input and derive every output before changing a tracked file.
# A missing late artifact or legal file must leave the previous release intact.
if updated != classifier:
    classifier_path.write_bytes(updated)
    print(f"Updated runtime release selector: {release_sha}")
RECEIPT_PATH.write_bytes(new_receipt_bytes)
print(f"Wrote {RECEIPT_PATH.relative_to(ROOT)} ({len(new_receipt_bytes)} bytes)")

PIN_PATH.write_bytes(pin_bytes)
print(f"Wrote {PIN_PATH.relative_to(ROOT)}")
print(f"New receipt SHA-256: {sha256hex(new_receipt_bytes)}")
print(f"Changed: {changed_artifacts} artifacts, {changed_legal} legal files")
