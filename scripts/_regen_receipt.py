import hashlib
import json
import os

ROOT = "."
RECEIPT = os.path.join(ROOT, "crates", "labcolors-core", "contracts", "clean-set-srgb8-v1", "receipt-v1.json")

with open(RECEIPT, "r", encoding="utf-8") as f:
    receipt = json.load(f)

changed = False
for item in receipt["artifacts"]:
    path = os.path.join(ROOT, item["path"])
    if not os.path.isfile(path):
        continue
    with open(path, "rb") as fh:
        data = fh.read()
    actual_sha = hashlib.sha256(data).hexdigest()
    actual_bytes = len(data)
    if actual_sha != item["sha256"] or actual_bytes != item["bytes"]:
        print("UPDATE %s: bytes %d->%d sha %s...->%s..." % (item["path"], item["bytes"], actual_bytes, item["sha256"][:16], actual_sha[:16]))
        item["sha256"] = actual_sha
        item["bytes"] = actual_bytes
        changed = True

if changed:
    canonical = json.dumps(receipt, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    with open(RECEIPT, "w", encoding="utf-8", newline="\n") as f:
        f.write(canonical)
    pin_sha = hashlib.sha256(canonical.encode("utf-8")).hexdigest()
    pin_path = os.path.join(ROOT, "crates", "labcolors-core", "contracts", "clean-set-srgb8-v1", "receipt-v1.sha256")
    with open(pin_path, "wb") as f:
        f.write(("%s  receipt-v1.json\n" % pin_sha).encode("ascii"))
    print("REGENERATED pin sha=%s" % pin_sha)
else:
    print("NO CHANGES")