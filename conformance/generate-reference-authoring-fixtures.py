"""Independent LCPW encoder for reference-authoring byte vectors (no solver)."""

import argparse
import json
import struct
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def u32(value):
    return struct.pack("<I", value)


def reference_program(hierarchy):
    sections = [[] for _ in range(14)]
    members = [(10, 3, 5, 7, 68)]
    if hierarchy:
        members.append((20, 13, 15, 17, 102))
    for target, paint, occurrence, output, channel in members:
        sections[1].append(u32(target) + bytes([4, channel, channel, channel, 0, 0])
                           + struct.pack("<d", 2.0) + bytes([2]) + u32(occurrence) + bytes([0]))
        sections[6].append(u32(paint) + bytes([1]) + u32(target))
        sections[8].append(u32(occurrence) + u32(paint) + u32(4) + struct.pack("<ddB", 0.0, 0.0, 0))
        sections[13].append(u32(output) + u32(paint))
    sections[3].append(bytes([4, 0]) + u32(len(members)) + b"".join(u32(row[0]) for row in members)
                       + u32(1) + u32(2) + bytes([255, 255, 255]))
    sections[4].append(u32(2))
    sections[7].append(u32(4) + bytes([1]) + u32(2))
    if hierarchy:
        sections[11].append(u32(30) + bytes([13]) + u32(2) + u32(5) + u32(15))
    for _, _, occurrence, _, _ in members:
        sections[11].append(u32(occurrence + 1) + bytes([9]) + u32(occurrence) + bytes([1]))
    return encode(sections)


def encode(sections):
    payload = b"".join(u32(len(section)) + b"".join(section) for section in sections)
    return b"LCPW" + struct.pack("<HI", 3, 10 + len(payload)) + payload


def composition(targets, reference=None):
    sections = [[] for _ in range(14)]
    payload = bytes([5 if reference is None else 6, 0]) + u32(len(targets))
    payload += b"".join(u32(target) for target in targets)
    if reference is not None:
        payload += u32(len(reference))
        payload += b"".join(u32(port) + bytes(rgb) for port, rgb in reference)
    sections[3].append(payload)
    return encode(sections)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="compare committed vectors without rewriting them")
    arguments = parser.parse_args()
    path = ROOT / "conformance/program-authoring-v3.json"
    fixture = json.loads(path.read_text(encoding="utf-8"))
    original = fixture.copy()
    fixture["referenceSingleton"] = reference_program(False).hex()
    hierarchy = reference_program(True)
    if hierarchy != (ROOT / "crates/labcolors-core/tests/fixtures/reference-hierarchy.lcpw").read_bytes():
        raise SystemExit("reference-hierarchy.lcpw differs from independent encoder")
    fixture["referenceHierarchy"] = hierarchy.hex()
    empty = [[] for _ in range(14)]
    empty[3].append(bytes([4, 0]) + u32(0) + u32(0))
    fixture["referenceEmpty"] = encode(empty).hex()
    fixture["compose"] = composition([0x01020304, 0xFFFFFFFF, 0]).hex()
    fixture["composeReference"] = composition([0x01020304, 0xFFFFFFFF, 0], [
        (0x10203040, [1, 128, 255]), (0xFFFFFFFF, [254, 129, 0]),
    ]).hex()
    fixture["composeEmpty"] = composition([]).hex()
    fixture["composeReferenceEmpty"] = composition([], []).hex()
    if arguments.check:
        if original != fixture:
            raise SystemExit("reference-authoring vectors differ; regenerate with this script")
        print("Reference-authoring vectors match the independent encoder and Rust fixture")
    else:
        path.write_text(json.dumps(fixture, indent=2) + "\n", encoding="utf-8")
