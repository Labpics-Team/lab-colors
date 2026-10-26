"""Binary regression fixtures for the saved-APK Kotlin linkage rule generator."""
import importlib.util
from pathlib import Path
import struct
import tempfile
import unittest
import zipfile


spec = importlib.util.spec_from_file_location("kotlin_linkage", Path(__file__).with_name("kotlin-linkage.py"))
assert spec is not None and spec.loader is not None
linkage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(linkage)


def leb(value):
    result = bytearray()
    while value > 127:
        result.append((value & 127) | 128)
        value >>= 7
    return bytes(result + bytes([value]))


def dex_fixture():
    """A real bounded standard DEX layout with metadata-only and executable fields."""
    strings = ["Ltest/Owner;", "Ljava/lang/Object;", "Lkotlin/Used;", "Lkotlin/MetadataOnly;", "Lkotlin/FieldType;", "Lkotlin/Signature;", "Lkotlin/Interface;", "V", "method", "FIELD", "unused", "VL"]
    blob = bytearray(112)
    tables = []

    def align():
        blob.extend(b"\0" * (-len(blob) % 4))

    def table(rows, width):
        align()
        offset = len(blob)
        for row in rows:
            blob.extend(row)
        assert len(blob) == offset + len(rows) * width
        tables.append((len(rows), offset))

    table([bytes(4) for _ in strings], 4)
    table([struct.pack("<I", i) for i in range(8)], 4)
    table([struct.pack("<III", 11, 7, 0), struct.pack("<III", 11, 5, 0)], 12)
    table([struct.pack("<HHI", 3, 4, 10), struct.pack("<HHI", 2, 4, 9)], 8)
    table([struct.pack("<HHI", 2, 1, 8), struct.pack("<HHI", 0, 0, 8)], 8)
    table([bytes(32)], 32)
    align()
    data_off = len(blob)
    offsets = []
    for name in strings:
        offsets.append(len(blob))
        blob.extend(leb(len(name)) + name.encode() + b"\0")
    align()
    interface_offset = len(blob)
    blob.extend(struct.pack("<IH", 1, 6))
    align()
    class_data = len(blob)
    # one executable method is the second method ID, with an sget on field #1.
    blob.extend(b"\x00\x00\x01\x00" + leb(1) + b"\x01")
    code_placeholder = len(blob)
    blob.extend(b"\x80\x80\x80\x80\x00")
    align()
    code = len(blob)
    blob.extend(struct.pack("<HHHHIIHHH", 1, 0, 0, 0, 0, 3, 0x0062, 1, 0x000e))
    encoded = leb(code)
    blob[code_placeholder:code_placeholder + 5] = encoded + bytes(5 - len(encoded))
    for i, offset in enumerate(offsets):
        struct.pack_into("<I", blob, tables[0][1] + i * 4, offset)
    struct.pack_into("<IIIIIIII", blob, tables[5][1], 0, 1, 1, interface_offset, 0, 0, class_data, 0)
    blob[:8] = b"dex\n035\0"
    struct.pack_into("<III", blob, 32, len(blob), 112, 0x12345678)
    for i, (size, offset) in enumerate(tables):
        struct.pack_into("<II", blob, 56 + 8 * i, size, offset)
    struct.pack_into("<II", blob, 104, len(blob) - data_off, data_off)
    return bytes(blob)


def jvm_class(name, parent, interfaces=(), interface=False):
    pool = bytearray()
    indices = {}
    for value in (name, parent, *interfaces):
        raw = value.encode()
        utf = len(indices) * 2 + 1
        pool.extend(b"\x01" + struct.pack(">H", len(raw)) + raw + b"\x07" + struct.pack(">H", utf))
        indices[value] = utf + 1
    return (struct.pack(">IHHH", 0xcafebabe, 0, 52, 1 + 2 * len(indices)) + pool +
            struct.pack(">HHHH", 0x0201 if interface else 1, indices[name], indices[parent], len(interfaces)) +
            b"".join(struct.pack(">H", indices[value]) for value in interfaces))


class LinkageTests(unittest.TestCase):
    def test_runtime_roots_ignore_field_id_metadata_and_type_only(self):
        roots, defined, methods, fields = linkage.dex_linkage(dex_fixture())
        self.assertEqual(roots, {"Lkotlin/Used;", "Lkotlin/Signature;", "Lkotlin/Interface;", "Lkotlin/FieldType;"})
        self.assertEqual(defined, {"Ltest/Owner;"})
        self.assertEqual(fields, {("Lkotlin/Used;", "FIELD", "Lkotlin/FieldType;")})
        self.assertIn(("Lkotlin/Used;", "method", "()Lkotlin/Signature;"), methods)
        self.assertNotIn("Lkotlin/MetadataOnly;", roots)

    def test_apk_union_parent_closure_and_stable_exact_rules(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("a.apk", "b.apk"):
                with zipfile.ZipFile(root / name, "w") as archive:
                    archive.writestr("classes.dex", dex_fixture())
            with zipfile.ZipFile(root / "stdlib.jar", "w") as archive:
                for name, parent, interfaces in (("Used", "Base", ("Marker",)), ("Base", "java/lang/Object", ()), ("Marker", "java/lang/Object", ()), ("Signature", "java/lang/Object", ()), ("Interface", "java/lang/Object", ()), ("FieldType", "java/lang/Object", ())):
                    archive.writestr(f"kotlin/{name}.class", jvm_class("kotlin/" + name, "kotlin/" + parent if "/" not in parent else parent, tuple("kotlin/" + item for item in interfaces), name in ("Interface", "Marker")))
            roots, defined, _, _ = linkage.apk_linkage((root / "b.apk", root / "a.apk"))
            kinds = {}
            classes = linkage.hierarchy_closure(roots, root / "stdlib.jar", defined, kinds)
            result = linkage.rules(classes, kinds)
            self.assertEqual(result, "".join(f"-keep {'interface' if name in ('Interface', 'Marker') else 'class'} kotlin.{name} {{ *; }}\n" for name in ("Base", "FieldType", "Interface", "Marker", "Signature", "Used")))
            self.assertEqual(result, linkage.rules(linkage.hierarchy_closure(roots, root / "stdlib.jar", defined, kinds), kinds))
            self.assertNotIn("*kotlin", result)

    def test_payload_widths_do_not_invent_field_access(self):
        # Packed-switch payload holds six code units; embedded 0x62 must not
        # masquerade as an sget instruction. The real sget follows it.
        words = (0x0100, 0x0001, 0x0062, 0x0000, 0x000e, 0x0000, 0x0062, 0x0001)
        code = linkage.Binary(struct.pack("<HHHHII", 1, 0, 0, 0, 0, len(words)) + struct.pack("<" + "H" * len(words), *words))
        self.assertEqual(linkage.instruction_fields(code, 0, 2), {1})
        for bad in ((0x0100, 2), (0x003e,), (0x0062,), (0x0062, 2)):
            data = linkage.Binary(struct.pack("<HHHHII", 1, 0, 0, 0, 0, len(bad)) + struct.pack("<" + "H" * len(bad), *bad))
            with self.subTest(words=bad), self.assertRaises(ValueError):
                linkage.instruction_fields(data, 0, 2)

    def test_invalid_binary_boundaries_are_rejected(self):
        original = dex_fixture()
        cases = [b"broken", original[:-1]]
        wrong_endian = bytearray(original)
        struct.pack_into("<I", wrong_endian, 40, 0x78563412)
        cases.append(bytes(wrong_endian))
        bad_table = bytearray(original)
        struct.pack_into("<I", bad_table, 60, len(original) - 2)
        cases.append(bytes(bad_table))
        bad_field = bytearray(original)
        code = original.find(struct.pack("<HHHHIIHHH", 1, 0, 0, 0, 0, 3, 0x0062, 1, 0x000e))
        struct.pack_into("<H", bad_field, code + 18, 9)
        cases.append(bytes(bad_field))
        for data in cases:
            with self.subTest(size=len(data)), self.assertRaises(ValueError):
                linkage.dex_linkage(data)
        with self.assertRaises(ValueError):
            linkage.class_parents(b"\xca\xfe")
        with self.assertRaises(ValueError):
            linkage.rules({"Lkotlin/Bad*;"})


if __name__ == "__main__":
    unittest.main()
