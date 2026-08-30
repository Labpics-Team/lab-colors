"""Regenerate clean-set receipt from merge-tree (simulated merge-ref).

CI checks out a synthetic merge of PR branch + origin/main. We use
`git merge-tree --write-tree` to produce the exact tree that CI will see,
then pin every artifact's bytes/sha256 to that tree's content.
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


def sha256hex(data):
    return hashlib.sha256(data).hexdigest()


def git_cat_blob(tree_sha, path):
    return subprocess.check_output(
        ["git", "-C", str(ROOT), "cat-file", "blob", f"{tree_sha}:{path}"],
        stderr=subprocess.STDOUT,
    )


# Step 1: compute the merge tree
merge_tree = subprocess.check_output(
    ["git", "-C", str(ROOT), "merge-tree", "--write-tree", "origin/main", "ext03-exports-metadata"],
    stderr=subprocess.STDOUT,
).decode("ascii").strip()
print(f"Merge tree: {merge_tree}")

receipt = json.loads(RECEIPT_PATH.read_bytes())

# Update artifacts[]
changed_artifacts = 0
for entry in receipt["artifacts"]:
    p = entry["path"]
    data = git_cat_blob(merge_tree, p)
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
    data = git_cat_blob(merge_tree, p)
    new_bytes = len(data)
    new_sha = sha256hex(data)
    if entry["bytes"] != new_bytes or entry["sha256"] != new_sha:
        print(f"legal {entry['role']}: {entry['bytes']}->{new_bytes} bytes, sha updated")
        entry["bytes"] = new_bytes
        entry["sha256"] = new_sha
        changed_legal += 1

if changed_artifacts == 0 and changed_legal == 0:
    print("No changes needed — receipt already matches merge-tree.")
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