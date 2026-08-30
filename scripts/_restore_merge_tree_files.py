"""Restore local working tree files to match the merge-tree content.

The receipt pins bytes from the simulated merge of PR + origin/main.
Local files may still have pre-rebase content. This script extracts
each artifact and legal file from the merge-tree and writes them
to disk with exact binary fidelity (no encoding transforms).
"""
import json
import subprocess
from pathlib import Path

ROOT = Path(r"C:\Users\Daniel\projects\lab-colors")
RECEIPT_PATH = ROOT / "crates/labcolors-core/contracts/clean-set-srgb8-v1/receipt-v1.json"

MERGE_TREE = subprocess.check_output(
    ["git", "-C", str(ROOT), "merge-tree", "--write-tree", "origin/main", "ext03-exports-metadata"],
    stderr=subprocess.STDOUT,
).decode("ascii").strip()
print(f"Merge tree: {MERGE_TREE}")

receipt = json.loads(RECEIPT_PATH.read_bytes())

restored = 0
for entry in receipt["artifacts"]:
    p = entry["path"]
    data = subprocess.check_output(
        ["git", "-C", str(ROOT), "cat-file", "blob", f"{MERGE_TREE}:{p}"],
        stderr=subprocess.STDOUT,
    )
    target = ROOT / p
    current = target.read_bytes()
    if current != data:
        target.write_bytes(data)
        print(f"  restored: {p} ({len(current)} -> {len(data)} bytes)")
        restored += 1
    else:
        print(f"  ok:       {p}")

for entry in receipt.get("license_scope", {}).get("legal_files", []):
    p = entry["path"]
    data = subprocess.check_output(
        ["git", "-C", str(ROOT), "cat-file", "blob", f"{MERGE_TREE}:{p}"],
        stderr=subprocess.STDOUT,
    )
    target = ROOT / p
    current = target.read_bytes()
    if current != data:
        target.write_bytes(data)
        print(f"  restored: {p} ({len(current)} -> {len(data)} bytes)")
        restored += 1
    else:
        print(f"  ok:       {p}")

print(f"\nRestored {restored} file(s).")