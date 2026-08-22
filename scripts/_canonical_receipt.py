"""Rewrite receipt-v1.json as canonical sorted-key indented LF JSON and regenerate pin."""
import hashlib
import json
import os
import sys

ROOT = r"C:\temp\wave5-627"
RECEIPT = os.path.join(ROOT, "crates", "labcolors-core", "contracts", "clean-set-srgb8-v1", "receipt-v1.json")
PIN = os.path.join(ROOT, "crates", "labcolors-core", "contracts", "clean-set-srgb8-v1", "receipt-v1.sha256")


def main() -> int:
    with open(RECEIPT, "rb") as fh:
        raw = fh.read()
    if raw.startswith(b"\xef\xbb\xbf"):
        raw = raw[3:]
    data = json.loads(raw.decode("utf-8"))
    for i, art in enumerate(data.get("artifacts", [])):
        fp = os.path.join(ROOT, art["path"])
        if not os.path.isfile(fp):
            print(f"[{i}] MISSING {fp}", file=sys.stderr)
            return 1
        actual_bytes = os.path.getsize(fp)
        with open(fp, "rb") as af:
            actual_sha = hashlib.sha256(af.read()).hexdigest()
        if actual_bytes != art["bytes"] or actual_sha != art["sha256"]:
            print(f"[{i}] DRIFT {art['path']}: old=({art['bytes']},{art['sha256'][:16]}) new=({actual_bytes},{actual_sha[:16]})", file=sys.stderr)
            art["bytes"] = actual_bytes
            art["sha256"] = actual_sha
    source = json.dumps(data, allow_nan=False, ensure_ascii=True, indent=2, sort_keys=True)
    canonical = f"{source}\n".encode("ascii")
    with open(RECEIPT, "wb") as fh:
        fh.write(canonical)
    digest = hashlib.sha256(canonical).hexdigest()
    pin_line = f"{digest}  receipt-v1.json\n".encode("ascii")
    with open(PIN, "wb") as fh:
        fh.write(pin_line)
    print(f"REGENERATED sha={digest}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())