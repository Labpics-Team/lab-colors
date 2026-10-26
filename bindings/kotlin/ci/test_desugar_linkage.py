"""Machine-consumed DEX fixtures for the desugar APK acceptance contract."""
import importlib.util
from pathlib import Path
import struct
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location("desugar_linkage", Path(__file__).with_name("desugar-linkage.py"))
assert spec is not None and spec.loader is not None
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


def leb(value):
    result = bytearray()
    while value > 127:
        result.append((value & 127) | 128)
        value >>= 7
    return bytes(result + bytes([value]))


def fixture(defined=True, method=True, invoke=True, constant=7, reordered=False, field=False, field_access=False, caller_parent=None, caller_interface=False, caller_name="Ltest/Caller;", member_access=9, self_call=False):
    """Standard DEX with a j$ provider, a non-j$ executable caller and optional field."""
    strings = ["Lj$/util/Target;", caller_name, "Ljava/lang/Object;", "V", "run", "synchronizedMap", "VALUE", "I", "VL"]
    if reordered:
        strings = list(reversed(strings))
    names = {name: i for i, name in enumerate(strings)}
    type_names = ["Lj$/util/Target;", caller_name, "Ljava/lang/Object;", "V", "I"]
    types = {name: i for i, name in enumerate(type_names)}
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
    table([struct.pack("<I", names[name]) for name in type_names], 4)
    table([struct.pack("<III", names["V"], types["V"], 0)], 12)
    table([struct.pack("<HHI", 0, types["I"], names["VALUE"])], 8)
    table([struct.pack("<HHI", 0, 0, names["synchronizedMap"]), struct.pack("<HHI", 1, 0, names["run"])], 8)
    table([bytes(32) for _ in range(1 + defined)], 32)
    align()
    data_off = len(blob)
    offsets = []
    for value in strings:
        offsets.append(len(blob))
        blob.extend(leb(len(value)) + value.encode() + b"\0")
    align()
    interface_list = 0
    if caller_interface:
        interface_list = len(blob)
        blob.extend(struct.pack("<IH", 1, types["Lj$/util/Target;"]))
        align()
    provider_data = len(blob)
    # One static field and one static method. Their method IDs remain stable.
    blob.extend(b"\x01\x00" + leb(int(method)) + b"\x00" + b"\x00" + leb(member_access) if field else b"\x00\x00" + leb(int(method)) + b"\x00")
    if method:
        blob.extend(b"\x00" + leb(member_access))
        provider_code_slot = len(blob)
        blob.extend(bytes(5))
    else:
        provider_code_slot = None
    caller_data = len(blob)
    blob.extend(b"\x00\x00\x01\x00\x01\x09")
    caller_code_slot = len(blob)
    blob.extend(bytes(5))
    align()
    provider_code = len(blob)
    if method:
        # Literal changes must fail equivalence, even with identical method IDs.
        provider_body = (0x0071, 0, 0, 0x000e) if self_call else (0x0013, constant, 0x000e)
        blob.extend(struct.pack("<HHHHII", 1, 0, 0, 0, 0, len(provider_body)))
        blob.extend(struct.pack("<" + "H" * len(provider_body), *provider_body))
    align()
    caller_code = len(blob)
    body = (0x0071, 0, 0, 0x000e) if invoke else ((0x0060, 0, 0x000e) if field_access else (0x000e,))
    blob.extend(struct.pack("<HHHHII", 1, 0, 0, 0, 0, len(body)))
    blob.extend(struct.pack("<" + "H" * len(body), *body))
    if provider_code_slot is not None:
        code = leb(provider_code)
        blob[provider_code_slot:provider_code_slot + 5] = code + bytes(5 - len(code))
    code = leb(caller_code)
    blob[caller_code_slot:caller_code_slot + 5] = code + bytes(5 - len(code))
    for i, offset in enumerate(offsets):
        struct.pack_into("<I", blob, tables[0][1] + i * 4, offset)
    if defined:
        provider_flags = 0x0601 if caller_interface else 1
        struct.pack_into("<IIIIIIII", blob, tables[5][1], types["Lj$/util/Target;"], provider_flags, types["Ljava/lang/Object;"], 0, 0, 0, provider_data, 0)
    parent = caller_parent or "Ljava/lang/Object;"
    struct.pack_into("<IIIIIIII", blob, tables[5][1] + 32 * defined, types[caller_name], 1, types[parent], interface_list, 0, 0, caller_data, 0)
    blob[:8] = b"dex\n035\0"
    struct.pack_into("<III", blob, 32, len(blob), 112, 0x12345678)
    for i, (count, offset) in enumerate(tables):
        struct.pack_into("<II", blob, 56 + i * 8, count, offset)
    struct.pack_into("<II", blob, 104, len(blob) - data_off, data_off)
    return bytes(blob)


def apk(path, dex):
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr("classes.dex", dex)


def caller_refs(words, types=(), methods=()):
    data = struct.pack("<HHHHII", 1, 0, 1, 0, 0, len(words)) + struct.pack("<" + "H" * len(words), *words)
    def get(values, index):
        if index >= len(values):
            raise ValueError("index")
        return values[index]
    _, refs = checker.instructions(checker.base.Binary(data), 0, [], types, [], [], methods, get)
    return tuple({("Ltest/Caller;", value) for value in group} for group in refs)


class DesugarLinkageTests(unittest.TestCase):
    def test_private_external_method_counterexample_and_same_owner(self):
        data = bytearray(fixture())
        class_offset = struct.unpack_from("<I", data, 100)[0]
        class_data = struct.unpack_from("<I", data, class_offset + 24)[0]
        self.assertEqual(data[class_data + 5], 9)
        data[class_data + 5] = 10
        parsed = checker.parse_dex(bytes(data))
        with self.assertRaisesRegex(ValueError, "synchronizedMap"):
            checker.validate(parsed, parsed)
        same_owner = checker.parse_dex(fixture(invoke=False, self_call=True, member_access=10))
        self.assertEqual(checker.validate(same_owner, same_owner), (1, 1))

    def test_method_and_field_visibility_with_real_binary_callers(self):
        for field in (False, True):
            for access, caller, parent, opcode, valid in (
                (10, "Ltest/Caller;", None, None, False),
                (8, "Ltest/Caller;", None, None, False),
                (8, "Lj$/util/Peer;", None, None, True),
                (12, "Ltest/Caller;", "Lj$/util/Target;", None, True),
                (12, "Ltest/Caller;", None, None, False),
                (12, "Ltest/Caller;", "Lj$/util/Target;", 0x6e, False),
                (4, "Ltest/Caller;", "Lj$/util/Target;", 0x6f, True),
                (4, "Ltest/Caller;", None, 0x6f, False),
            ):
                if field and opcode is not None:
                    continue
                data = bytearray(fixture(invoke=not field, field=field, field_access=field,
                                         caller_name=caller, caller_parent=parent, member_access=access))
                if opcode is not None:
                    code = bytes(data).find(struct.pack("<HHHHIIHHHH", 1, 0, 0, 0, 0, 4, 0x0071, 0, 0, 0x000e))
                    self.assertGreaterEqual(code, 0)
                    data[code + 16] = opcode
                parsed = checker.parse_dex(data)
                with self.subTest(field=field, access=access, caller=caller, parent=parent, opcode=opcode):
                    if valid:
                        checker.validate(parsed, parsed)
                    else:
                        with self.assertRaisesRegex(ValueError, "unresolved executable"):
                            checker.validate(parsed, parsed)

    def test_inherited_visibility_uses_declaring_owner_not_symbolic_owner(self):
        target, child = "Lj$/util/Target;", "Lj$/util/Child;"
        for access, valid in ((9, True), (10, False), (8, True)):
            parsed = checker.parse_dex(fixture(invoke=False, member_access=access))
            parsed[0][child] = (1, (target,), (), (), (), (), ())
            parsed[1][3].add((child, (target,)))
            refs = caller_refs((0x0071, 0, 0, 0x000e), (), ((child, "synchronizedMap", "()V"),))
            parsed[1][2].update((child, value) for _, value in refs[2])
            with self.subTest(access=access):
                if valid:
                    checker.validate(parsed, parsed)
                else:
                    with self.assertRaisesRegex(ValueError, "unresolved executable"):
                        checker.validate(parsed, parsed)

    def test_non_desugar_class_missing_parent_or_interface(self):
        for interface in (False, True):
            app = checker.parse_dex(fixture(defined=False, invoke=False,
                                            caller_interface=interface,
                                            caller_parent=None if interface else "Lj$/util/Target;"))
            target = "Lj$/util/Target;"
            self.assertIn(("Ltest/Caller;", target), app[1][0])
            test = checker.parse_dex(fixture(defined=False, invoke=False))
            with self.subTest(interface=interface), self.assertRaisesRegex(ValueError, "Lj\\$/util/Target"):
                checker.validate(app, test)

    def test_non_desugar_class_loadable_parent_or_interface(self):
        for interface in (False, True):
            app = checker.parse_dex(fixture(defined=False, invoke=False,
                                            caller_interface=interface,
                                            caller_parent=None if interface else "Lj$/util/Target;"))
            test = checker.parse_dex(fixture(defined=True, invoke=False, caller_interface=interface))
            with self.subTest(interface=interface):
                self.assertEqual(checker.validate(app, test), (0, 1))

    def test_array_types_and_vm_clone_from_binary_operands(self):
        for array in ("[Lj$/util/Target;", "[[Lj$/util/Target;"):
            app = checker.parse_dex(fixture())
            test = checker.parse_dex(fixture(defined=False))
            refs = caller_refs((0x001c, 0, 0x006e, 0, 0, 0x000e), (array,), ((array, "clone", "()Ljava/lang/Object;"),))
            for group, values in zip(app[1], refs):
                group.update(values)
            self.assertEqual(checker.validate(app, test), (0, 1))

    def test_array_absent_element_is_not_a_vm_exemption(self):
        for words in ((0x001c, 0, 0x000e), (0x006e, 0, 0, 0x000e)):
            app = checker.parse_dex(fixture())
            test = checker.parse_dex(fixture(defined=False))
            array = "[Lj$/util/Missing;"
            refs = caller_refs(words, (array,), ((array, "clone", "()Ljava/lang/Object;"),))
            for group, values in zip(app[1], refs):
                group.update(values)
            with self.assertRaisesRegex(ValueError, "Missing"):
                checker.validate(app, test)

    def test_object_public_inheritance_vs_missing_member_binary(self):
        for owner in ("Lj$/util/Target;", "[Lj$/util/Target;"):
            for name, descriptor, opcode, valid in (("toString", "()Ljava/lang/String;", 0x6e, True), ("missing", "()V", 0x6e, False), ("toString", "()V", 0x6e, False), ("toString", "()Ljava/lang/String;", 0x71, False)):
                app = checker.parse_dex(fixture())
                test = checker.parse_dex(fixture(defined=False))
                refs = caller_refs((opcode, 0, 0, 0x000e), (), ((owner, name, descriptor),))
                for group, values in zip(app[1], refs):
                    group.update(values)
                with self.subTest(owner=owner, name=name, descriptor=descriptor, opcode=opcode):
                    if valid:
                        self.assertEqual(checker.validate(app, test), (0, 1))
                    else:
                        with self.assertRaisesRegex(ValueError, "unresolved executable"):
                            checker.validate(app, test)

    def test_clone_is_array_virtual_only_not_element_member(self):
        for owner, opcode, descriptor in (("Lj$/util/Target;", 0x6e, "()Ljava/lang/Object;"), ("[Lj$/util/Target;", 0x71, "()Ljava/lang/Object;"), ("[Lj$/util/Target;", 0x6e, "()V")):
            app = checker.parse_dex(fixture())
            test = checker.parse_dex(fixture(defined=False))
            refs = caller_refs((opcode, 0, 0, 0x000e), (), ((owner, "clone", descriptor),))
            for group, values in zip(app[1], refs):
                group.update(values)
            with self.assertRaisesRegex(ValueError, "unresolved executable"):
                checker.validate(app, test)

    def test_equivalent_definitions_ignore_dex_index_order(self):
        app = checker.parse_dex(fixture(reordered=False))
        test = checker.parse_dex(fixture(reordered=True))
        self.assertEqual(checker.validate(app, test), (1, 1))

    def test_shared_code_difference_rejected_despite_same_methods(self):
        with self.assertRaisesRegex(ValueError, "incompatible shared"):
            checker.validate(checker.parse_dex(fixture(constant=7)), checker.parse_dex(fixture(constant=8)))

    def test_test_first_override_does_not_check_unselected_app_body(self):
        app = checker.parse_dex(fixture(constant=7))
        test = checker.parse_dex(fixture(constant=8))
        target = "Lj$/util/Target;"
        app[1][2].add((target, (0x71, (target, "absent", "()V"))))
        with self.assertRaisesRegex(ValueError, "incompatible shared") as error:
            checker.validate(app, test)
        self.assertNotIn("absent", str(error.exception))

    def test_missing_executable_type_and_inherited_member(self):
        app = checker.parse_dex(fixture())
        test = checker.parse_dex(fixture(defined=False))
        target = "Lj$/util/Target;"
        app[1][0].add(("Ltest/Caller;", "Lj$/util/Missing;"))
        with self.assertRaisesRegex(ValueError, r"Lj\$/util/Missing"):
            checker.validate(app, test)
        app[1][0].clear()
        child = "Lj$/util/Child;"
        app[0][child] = (1, (target,), (), (), (), (), ())
        app[1][2].add(("Ltest/Caller;", (0x71, (child, "synchronizedMap", "()V"))))
        self.assertEqual(checker.validate(app, test), (0, 2))

    def test_missing_executable_method_and_field_rejected(self):
        for field in (False, True):
            member = "VALUE" if field else "synchronizedMap"
            app = checker.parse_dex(fixture(method=field, invoke=not field, field_access=field))
            test = checker.parse_dex(fixture(defined=False, method=field, invoke=not field, field_access=field))
            with self.subTest(member=member), self.assertRaisesRegex(ValueError, member):
                checker.validate(app, test)

    def test_catch_handlers_normalize_type_ids_and_preserve_targets(self):
        def get(values, index):
            if index >= len(values):
                raise ValueError("index")
            return values[index]
        header = struct.pack("<HHHHII", 1, 0, 0, 1, 999, 2)
        body = struct.pack("<HH", 0x0000, 0x000e)
        def code(index, address):
            return checker.base.Binary(header + body + struct.pack("<IHH", 0, 1, 1) + b"\x01\x01" + leb(index) + leb(address))
        first = checker.instructions(code(0, 1), 0, [], ["Lj$/util/Target;"], [], [], [], get)
        second = checker.instructions(code(1, 1), 0, [], ["Ljava/lang/Object;", "Lj$/util/Target;"], [], [], [], get)
        self.assertEqual(first, second)
        self.assertNotEqual(first[0], checker.instructions(code(0, 0), 0, [], ["Lj$/util/Target;"], [], [], [], get)[0])
        with self.assertRaisesRegex(ValueError, "catch address"):
            checker.instructions(code(0, 2), 0, [], ["Lj$/util/Target;"], [], [], [], get)

    def test_apk_boundaries_and_invalid_instruction_fail_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            app_path, test_path = Path(directory) / "app.apk", Path(directory) / "test.apk"
            apk(app_path, fixture())
            apk(test_path, fixture(reordered=True))
            self.assertEqual(checker.validate(checker.apk(app_path), checker.apk(test_path)), (1, 1))
            with zipfile.ZipFile(test_path, "w") as archive:
                archive.writestr("not-dex.txt", "missing")
            with self.assertRaisesRegex(ValueError, "no standard DEX"):
                checker.apk(test_path)
        original = fixture()
        for data in (original[:-1], b"not dex"):
            with self.assertRaises(ValueError):
                checker.parse_dex(data)
        invalid = bytearray(original)
        code = original.find(struct.pack("<HHHHIIHHH", 1, 0, 0, 0, 0, 3, 0x0013, 7, 0x000e))
        struct.pack_into("<H", invalid, code + 16, 0x003e)
        with self.assertRaisesRegex(ValueError, "unsupported DEX opcode"):
            checker.parse_dex(bytes(invalid))


if __name__ == "__main__":
    unittest.main()
