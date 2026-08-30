"""Regenerate clean-set receipt artifact hashes from merge-ref (FETCH_HEAD).

CI checks out the merge of PR branch + origin/main. The receipt must pin
the exact bytes that exist in that merge commit, not the local branch tip.
This script reads each artifact from FETCH_HEAD via git cat-file blob
(with autocrlf disabled), updates bytes/sha256 in receipt-v1.json, rewrites
the canonical JSON, and updates the .sha256 pin file.
"""
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(r"C:\Users\Daniel\projects\lab-colors")
RECEIPT_PATH = ROOT / "crates/labcolors-core/contracts/clean-set-srgb8-v1/receipt-v1.json"
PIN_PATH = ROOT / "crates/labcolors-core/contracts/clean-set-srgb8-v1/receipt-v1.sha256"


def canonical_json(value):
    return json.dumps(value, allow_nan=False, ensure_ascii=True, indent=2, sort_keys=True) + "\n"


def git_blob_raw(path):
    """Read raw blob bytes from FETCH_HEAD:path with autocrlf disabled."""
    result = subprocess.run(
        ["git", "-C", str(ROOT), "-c", "core.autocrlf=false",
         "show", f"FETCH_HEAD:{path}"],
        capture_output=True,
    )
    if result.returncode != 0:
        print(f"ERROR: git show FETCH_HEAD:{path} failed: {result.stderr.decode()}", file=sys.stderr)
        sys.exit(1)
    return result.stdout


def sha256hex(data):
    return hashlib.sha256(data).hexdigest()


receipt = json.loads(RECEIPT_PATH.read_bytes())

# Update artifacts[]
changed_artifacts = 0
for entry in receipt["artifacts"]:
    p = entry["path"]
    data = git_blob_raw(p)
    new_bytes = len(data)
    new_sha = sha256hex(data)
    if entry["bytes"] != new_bytes or entry["sha256"] != new_sha:
        print(f"artifact {entry['role']}: {entry['bytes']}->{new_bytes} bytes, sha updated")
        entry["bytes"] = new_bytes
        entry["sha256"] = new_sha
        changed_artifacts += 1

# Update legal_files[]
changed_legal = 0
for entry in receipt.get("license_scope", {}).get("legal_files", []):
    p = entry["path"]
    data = git_blob_raw(p)
    new_bytes = len(data)
    new_sha = sha256hex(data)
    if entry["bytes"] != new_bytes or entry["sha256"] != new_sha:
        print(f"legal {entry['role']}: {entry['bytes']}->{new_bytes} bytes, sha updated")
        entry["bytes"] = new_bytes
        entry["sha256"] = new_sha
        changed_legal += 1

if changed_artifacts == 0 and changed_legal == 0:
    print("No changes needed - receipt already matches merge-ref.")
    sys.exit(0)

# Write canonical receipt
new_receipt_bytes = canonical_json(receipt).encode("ascii")
RECEIPT_PATH.write_bytes(new_receipt_bytes)
print(f"Wrote {RECEIPT_PATH} ({len(new_receipt_bytes)} bytes)")

# Update pin file
pin_content = f"{sha256hex(new_receipt_bytes)}  receipt-v1.json\n"
PIN_PATH.write_bytes(pin_content.encode("ascii"))
print(f"Wrote {PIN_PATH}")
print(f"New receipt SHA-256: {sha256hex(new_receipt_bytes)}")
print(f"Changed: {changed_artifacts} artifacts, {changed_legal} legal files")