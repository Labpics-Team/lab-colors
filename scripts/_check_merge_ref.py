import hashlib, json, subprocess, sys
from pathlib import Path

root = Path(r"C:\Users\Daniel\projects\lab-colors")
receipt = json.loads((root / "crates/labcolors-core/contracts/clean-set-srgb8-v1/receipt-v1.json").read_bytes())
arts = receipt["artifacts"]
print(f"Checking {len(arts)} artifacts against merge-ref (FETCH_HEAD)...")
failed = []
for i, a in enumerate(arts):
    p = a["path"]
    try:
        data = subprocess.check_output(["git", "-C", str(root), "show", f"FETCH_HEAD:{p}"], stderr=subprocess.STDOUT)
    except subprocess.CalledProcessError as e:
        print(f"  [{i}] {a['role']}: {p} — GIT SHOW FAILED: {e.output.decode(errors='replace').strip()}")
        failed.append((i, a["role"], p))
        continue
    actual_bytes = len(data)
    actual_sha = hashlib.sha256(data).hexdigest()
    if actual_bytes != a["bytes"] or actual_sha != a["sha256"]:
        failed.append((i, a["role"], p, a["bytes"], a["sha256"][:16], actual_bytes, actual_sha[:16]))
        print(f"  [{i}] {a['role']}: {p}")
        print(f"       receipt: bytes={a['bytes']} sha={a['sha256'][:16]}...")
        print(f"       merge:   bytes={actual_bytes} sha={actual_sha[:16]}...")

if not failed:
    print("ALL ARTIFACTS MATCH MERGE-REF")
else:
    print(f"\nFAILED: {len(failed)} artifact(s)")

# Also check legal_files
print("\nChecking legal_files against merge-ref...")
legal = receipt.get("license_scope", {}).get("legal_files", [])
for i, lf in enumerate(legal):
    p = lf["path"]
    try:
        data = subprocess.check_output(["git", "-C", str(root), "show", f"FETCH_HEAD:{p}"], stderr=subprocess.STDOUT)
    except subprocess.CalledProcessError as e:
        print(f"  legal[{i}] {lf['role']}: {p} — GIT SHOW FAILED")
        continue
    actual_bytes = len(data)
    actual_sha = hashlib.sha256(data).hexdigest()
    if actual_bytes != lf["bytes"] or actual_sha != lf["sha256"]:
        print(f"  legal[{i}] {lf['role']}: {p}")
        print(f"       receipt: bytes={lf['bytes']} sha={lf['sha256'][:16]}...")
        print(f"       merge:   bytes={actual_bytes} sha={actual_sha[:16]}...")
    else:
        print(f"  legal[{i}] {lf['role']}: OK")