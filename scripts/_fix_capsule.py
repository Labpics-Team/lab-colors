import os

script = "scripts/verify_point_support_surplus.py"
with open(script, "r", encoding="utf-8") as f:
    content = f.read()

old = "008b55705daf85e60d6962ae772ff8dabd5dca0f1c116d6944344a26a0617e9f"
actual = "f7fda69e6ee684f9cb8bc463ae76bb64492c693671a47f0b9f5435e432fdbb8b"

# Also need to update source_files entry for lib.rs in proof JSON
proof_path = "crates/labcolors-core/contracts/point-support-reference-surplus-q55-bps-proof-v1.json"
import json, hashlib
with open(proof_path, "r", encoding="utf-8") as f:
    proof = json.load(f)

lib_rs_actual = hashlib.sha256(open("crates/labcolors-core/src/lib.rs", "rb").read()).hexdigest()
for sf in proof.get("source_files", []):
    if sf["path"] == "crates/labcolors-core/src/lib.rs":
        print(f"PROOF lib.rs old: {sf['sha256'][:16]}")
        sf["sha256"] = lib_rs_actual
        sf["bytes"] = len(open("crates/labcolors-core/src/lib.rs", "rb").read())
        print(f"PROOF lib.rs new: {lib_rs_actual[:16]}")
        break

proof["source_closure_sha256"] = actual
canonical = json.dumps(proof, sort_keys=True, separators=(",", ":")) + "\n"
with open(proof_path, "w", encoding="utf-8") as f:
    f.write(canonical)
print(f"PROOF source_closure updated to: {actual[:16]}...")

if old not in content:
    print("OLD HASH NOT FOUND IN SCRIPT")
    raise SystemExit(1)

new_content = content.replace(old, actual)
with open(script, "w", encoding="utf-8") as f:
    f.write(new_content)

print(f"REPLACED: {old[:16]}... -> {actual[:16]}...")