"""Check an app/test APK pair's duplicate j$ definitions and test-first linkage.

Exit 0 means shared j$ classes are structurally and instruction-equivalent and
all executable j$ references in either APK resolve in the test-first runtime.
Unrecognized DEX constructs fail closed rather than silently weakening the gate.
"""
import argparse
import importlib.util
from pathlib import Path
import re
import struct
import sys
import zipfile

spec = importlib.util.spec_from_file_location("kotlin_linkage", Path(__file__).with_name("kotlin-linkage.py"))
assert spec is not None and spec.loader is not None
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)


def jtype(value):
    value = value.lstrip("[")
    return value if value.startswith("Lj$/") and value.endswith(";") else None


def sleb(binary, offset):
    value = 0
    for shift in range(0, 35, 7):
        byte = binary.span(offset, 1)[0]
        offset += 1
        value |= (byte & 127) << shift
        if byte < 128:
            if byte & 64:
                value -= 1 << (shift + 7)
            return value, offset
    raise ValueError("invalid SLEB128")


def dex_tables(data):
    if len(data) < 112 or data[:8] not in (b"dex\n035\0", b"dex\n037\0", b"dex\n038\0", b"dex\n039\0", b"dex\n040\0"):
        raise ValueError("unsupported standard DEX")
    binary = base.Binary(data)
    if binary.unpack("III", 32) != (len(data), 112, 0x12345678):
        raise ValueError("invalid DEX header/endian")
    sizes = (4, 4, 12, 8, 8, 32)
    tables = []
    for i, width in enumerate(sizes):
        count, offset = binary.unpack("II", 56 + i * 8)
        if (count and (offset < 112 or offset % 4)) or (not count and offset):
            raise ValueError("invalid DEX table")
        binary.span(offset, count * width)
        tables.append((count, offset))
    data_size, data_off = binary.unpack("II", 104)
    if data_size and (data_off < 112 or data_off % 4 or data_off + data_size != len(data)):
        raise ValueError("invalid DEX data section")
    binary.span(data_off, data_size)

    def get(values, index):
        if index >= len(values):
            raise ValueError("DEX index out of bounds")
        return values[index]

    strings = []
    for i in range(tables[0][0]):
        offset, = binary.unpack("I", tables[0][1] + 4 * i)
        length, offset = binary.uleb(offset)
        end = data.find(b"\0", offset)
        if end < 0:
            raise ValueError("unterminated DEX string")
        raw = data[offset:end].replace(b"\xc0\x80", b"\0")
        try:
            value = raw.decode("utf-8", errors="surrogatepass")
        except UnicodeDecodeError as error:
            raise ValueError("invalid DEX string") from error
        if len(value.encode("utf-16-le", errors="surrogatepass")) // 2 != length:
            raise ValueError("invalid DEX string length")
        strings.append(value)
    types = [get(strings, binary.unpack("I", tables[1][1] + 4 * i)[0]) for i in range(tables[1][0])]

    def type_list(offset):
        if not offset:
            return ()
        if offset % 4:
            raise ValueError("unaligned type list")
        count, = binary.unpack("I", offset)
        return tuple(get(types, index) for index in binary.unpack("H" * count, offset + 4))

    protos = []
    for i in range(tables[2][0]):
        shorty, result, parameters = binary.unpack("III", tables[2][1] + 12 * i)
        get(strings, shorty)
        protos.append("(" + "".join(type_list(parameters)) + ")" + get(types, result))
    fields = []
    for i in range(tables[3][0]):
        owner, field_type, name = binary.unpack("HHI", tables[3][1] + 8 * i)
        fields.append((get(types, owner), get(strings, name), get(types, field_type)))
    methods = []
    for i in range(tables[4][0]):
        owner, proto, name = binary.unpack("HHI", tables[4][1] + 8 * i)
        methods.append((get(types, owner), get(strings, name), get(protos, proto)))
    return binary, tables, strings, types, protos, fields, methods, type_list, get


def encoded_value(binary, offset, strings, types, fields, methods, protos, get):
    header = binary.span(offset, 1)[0]
    offset += 1
    tag, argument = header & 31, header >> 5
    if tag == 0x1c:
        if argument:
            raise ValueError("invalid encoded array")
        count, offset = binary.uleb(offset)
        values = []
        for _ in range(count):
            value, offset = encoded_value(binary, offset, strings, types, fields, methods, protos, get)
            values.append(value)
        return ("array", tuple(values)), offset
    if tag == 0x1d:
        if argument:
            raise ValueError("invalid encoded annotation")
        cls, offset = binary.uleb(offset)
        count, offset = binary.uleb(offset)
        pairs = []
        for _ in range(count):
            name, offset = binary.uleb(offset)
            value, offset = encoded_value(binary, offset, strings, types, fields, methods, protos, get)
            pairs.append((get(strings, name), value))
        return ("annotation", get(types, cls), tuple(pairs)), offset
    if tag in (0x1e, 0x1f):
        if (tag == 0x1e and argument) or (tag == 0x1f and argument > 1):
            raise ValueError("invalid null/boolean encoding")
        return ("null",) if tag == 0x1e else ("boolean", bool(argument)), offset
    maximum = {0: 0, 2: 1, 3: 1, 4: 3, 6: 7, 16: 3, 17: 7, 21: 3, 22: 3, 23: 3, 24: 3, 25: 3, 26: 3, 27: 3}.get(tag)
    if maximum is None or argument > maximum:
        raise ValueError("unsupported encoded value")
    raw = binary.span(offset, argument + 1)
    offset += len(raw)
    value = int.from_bytes(raw, "little", signed=tag in (0, 2, 4, 6))
    if tag in (16, 17):
        value <<= (4 if tag == 16 else 8) * 8 - len(raw) * 8
    if tag == 22:
        raise ValueError("encoded method handle requires handle normalization")
    if tag in (21, 23, 24, 25, 26, 27):
        table = {21: protos, 23: strings, 24: types, 25: fields, 26: methods, 27: fields}[tag]
        value = get(table, value)
    return (tag, value), offset


def instructions(binary, offset, strings, types, protos, fields, methods, get):
    if offset % 4:
        raise ValueError("unaligned code item")
    registers, ins, outs, tries, _, size = binary.unpack("HHHHII", offset)
    units = binary.unpack("H" * size, offset + 16)
    pc = 0
    normalized = []
    references = [set(), set(), set()]
    while pc < size:
        word = units[pc]
        opcode = word & 255
        if opcode == 0 and word:
            if word == 0x100 and pc + 1 < size:
                width = 4 + 2 * units[pc + 1]
            elif word == 0x200 and pc + 1 < size:
                width = 2 + 4 * units[pc + 1]
            elif word == 0x300 and pc + 3 < size:
                width = 4 + (units[pc + 1] * (units[pc + 2] | units[pc + 3] << 16) + 1) // 2
            else:
                raise ValueError("invalid DEX payload")
        elif opcode == 0x18:
            width = 5
        elif opcode in (0x03, 0x06, 0x09, 0x14, 0x17, 0x1b, 0x24, 0x25, 0x26, 0x2a, 0x2b, 0x2c) or 0x6e <= opcode <= 0x72 or 0x74 <= opcode <= 0x78 or opcode in (0xfc, 0xfd):
            width = 3
        elif opcode in (0xfa, 0xfb):
            width = 4
        elif opcode in (0x02, 0x05, 0x08, 0x13, 0x15, 0x16, 0x19, 0x1a, 0x1c, 0x1f, 0x20, 0x22, 0x23, 0x29, 0xfe, 0xff) or 0x2d <= opcode <= 0x3d or 0x44 <= opcode <= 0x6d or 0x90 <= opcode <= 0xaf or 0xd0 <= opcode <= 0xe2:
            width = 2
        elif opcode <= 0x12 or opcode in (0x1d, 0x1e, 0x21, 0x27, 0x28) or 0x7b <= opcode <= 0x8f or 0xb0 <= opcode <= 0xcf:
            width = 1
        else:
            raise ValueError(f"unsupported DEX opcode {opcode:#x}")
        if pc + width > size:
            raise ValueError("truncated DEX instruction")
        raw = list(units[pc:pc + width])
        if opcode == 0x1a:
            table, index, location = strings, raw[1], 1
        elif opcode == 0x1b:
            table, index, location = strings, raw[1] | raw[2] << 16, 1
        elif opcode in (0x1c, 0x1f, 0x20, 0x22, 0x23, 0x24, 0x25):
            table, index, location = types, raw[1] if width > 1 else -1, 1
        elif 0x52 <= opcode <= 0x6d:
            table, index, location = fields, raw[1], 1
            if jtype(get(table, index)[0]):
                references[1].add((opcode, get(table, index)))
        elif 0x6e <= opcode <= 0x72 or 0x74 <= opcode <= 0x78:
            table, index, location = methods, raw[1], 1
            if jtype(get(table, index)[0]):
                references[2].add((opcode, get(table, index)))
        elif opcode in (0xfa, 0xfb):
            table, index, location = methods, raw[1], 1
            proto_index = raw[3]
            proto = get(protos, proto_index)
            raw[3] = ("proto", proto)
            if jtype(get(table, index)[0]):
                references[2].add((opcode, get(table, index)))
        elif opcode in (0xfc, 0xfd):
            table, index, location = (), -1, -1
            raise ValueError("invoke-custom requires call-site normalization")
        elif opcode == 0xfe:
            table, index, location = (), -1, -1
            raise ValueError("const-method-handle requires handle normalization")
        elif opcode == 0xff:
            table, index, location = protos, raw[1], 1
        else:
            table, index, location = (), -1, -1
        if location >= 0:
            value = get(table, index)
            raw[location] = ("id", value)
            if table is fields or table is methods:
                references[0].update(descriptor for descriptor in re.findall(r"L[^;]+;", value[2]) if jtype(descriptor))
            if table is types and jtype(value):
                references[0].add(value)
            if opcode == 0x1b:
                raw[2] = ("high-string-id",)
        normalized.append(tuple(raw))
        pc += width
    handler = ()
    if tries:
        start = offset + 16 + size * 2 + (2 if size & 1 else 0)
        entries = []
        for i in range(tries):
            pc_start, count, target = binary.unpack("IHH", start + i * 8)
            if count == 0 or pc_start + count > size:
                raise ValueError("invalid try range")
            entries.append((pc_start, count, target))
        cursor = start + tries * 8
        n, cursor = binary.uleb(cursor)
        handlers = {}
        for _ in range(n):
            relative = cursor - (start + tries * 8)
            length, cursor = sleb(binary, cursor)
            catches = []
            for _ in range(abs(length)):
                type_index, cursor = binary.uleb(cursor)
                address, cursor = binary.uleb(cursor)
                descriptor = get(types, type_index)
                if address >= size:
                    raise ValueError("invalid catch address")
                catches.append((descriptor, address))
                if jtype(descriptor):
                    references[0].add(descriptor)
            default = None
            if length <= 0:
                default, cursor = binary.uleb(cursor)
                if default >= size:
                    raise ValueError("invalid catch-all address")
            handlers[relative] = (tuple(catches), default)
        if any(target not in handlers for _, _, target in entries):
            raise ValueError("invalid catch handler offset")
        handler = tuple((pc_start, count, handlers[target]) for pc_start, count, target in entries)
    return (registers, ins, outs, tuple(normalized), handler), tuple(references)


def annotations(binary, offset, strings, types, fields, methods, protos, get):
    if not offset:
        return ()

    def annotation_set(position):
        if not position:
            return ()
        count, = binary.unpack("I", position)
        result = []
        for i in range(count):
            entry, = binary.unpack("I", position + 4 + i * 4)
            visibility = binary.span(entry, 1)[0]
            # encoded_annotation omits encoded_value's leading tag.
            cls, cursor = binary.uleb(entry + 1)
            size, cursor = binary.uleb(cursor)
            pairs = []
            for _ in range(size):
                name, cursor = binary.uleb(cursor)
                value, cursor = encoded_value(binary, cursor, strings, types, fields, methods, protos, get)
                pairs.append((get(strings, name), value))
            result.append((visibility, get(types, cls), tuple(sorted(pairs))))
        return tuple(sorted(result))

    cls, field_count, method_count, parameter_count = binary.unpack("IIII", offset)
    cursor = offset + 16
    groups = []
    for table, count, parameter in ((fields, field_count, False), (methods, method_count, False), (methods, parameter_count, True)):
        result = []
        for _ in range(count):
            member, position = binary.unpack("II", cursor)
            cursor += 8
            if parameter:
                size, = binary.unpack("I", position)
                sets = tuple(annotation_set(binary.unpack("I", position + 4 + i * 4)[0]) for i in range(size))
            else:
                sets = annotation_set(position)
            result.append((get(table, member), sets))
        groups.append(tuple(sorted(result)))
    return annotation_set(cls), tuple(groups)


def parse_dex(data):
    binary, tables, strings, types, protos, fields, methods, type_list, get = dex_tables(data)
    definitions = {}
    references = [set(), set(), set(), set()]
    for i in range(tables[5][0]):
        cls_index, flags, parent, interfaces, _, annotation_offset, class_data, static_values = binary.unpack("IIIIIIII", tables[5][1] + i * 32)
        cls = get(types, cls_index)
        parents = (() if parent == 0xffffffff else (get(types, parent),)) + type_list(interfaces)
        references[3].add((cls, parents))
        static = ()
        if static_values:
            size, position = binary.uleb(static_values)
            items = []
            for _ in range(size):
                value, position = encoded_value(binary, position, strings, types, fields, methods, protos, get)
                items.append(value)
            static = tuple(items)
        members = [[], []]
        static_fields = []
        codes = {}
        if class_data:
            counts = []
            for _ in range(4):
                count, class_data = binary.uleb(class_data)
                counts.append(count)
            for group, count in enumerate(counts):
                index = 0
                table = fields if group < 2 else methods
                for _ in range(count):
                    delta, class_data = binary.uleb(class_data)
                    index += delta
                    member = get(table, index)
                    access, class_data = binary.uleb(class_data)
                    if member[0] != cls:
                        raise ValueError("class data member owner mismatch")
                    members[group // 2].append((member, access))
                    if group == 0:
                        static_fields.append(member)
                    if group >= 2:
                        code, class_data = binary.uleb(class_data)
                        if code:
                            normalized, refs = instructions(binary, code, strings, types, protos, fields, methods, get)
                            codes[member] = normalized
                            for target, values in zip(references, refs):
                                target.update((cls, value) for value in values)
        # Loading any defined class resolves its direct superclass/interfaces,
        # including a non-j$ caller extending or implementing a j$ type.
        references[0].update((cls, parent) for parent in parents if jtype(parent))
        if jtype(cls):
            if cls in definitions:
                raise ValueError(f"duplicate j$ class in DEX: {cls}")
            if len(static) > len(static_fields):
                raise ValueError("too many static field values")
            static = tuple(sorted(zip(static_fields[:len(static)], static)))
            metadata = annotations(binary, annotation_offset, strings, types, fields, methods, protos, get)
            definitions[cls] = (flags, parents, tuple(sorted(members[0])), tuple(sorted(members[1])), static, tuple(sorted(codes.items())), metadata)
        # Header metadata cannot establish executable linkage; executable type
        # instructions and catch handlers above do, including non-j$ callers.
        if annotation_offset and annotation_offset >= len(data):
            raise ValueError("invalid annotation offset")
    return definitions, tuple(references)


def apk(path):
    definitions = {}
    refs = [set(), set(), set(), set()]
    with zipfile.ZipFile(path) as archive:
        names = sorted(name for name in archive.namelist() if re.fullmatch(r"classes(?:[2-9]|[1-9][0-9]+)?\.dex", name))
        if not names:
            raise ValueError(f"APK has no standard DEX: {path}")
        for name in names:
            classes, found = parse_dex(archive.read(name))
            for cls, declaration in classes.items():
                if cls in definitions:
                    raise ValueError(f"duplicate j$ class in APK: {cls}")
                definitions[cls] = declaration
            for target, values in zip(refs, found):
                target.update(values)
    return definitions, tuple(refs)


# Only public, non-static Object methods are inherited through platform
# java/lang/Object; clone/finalize are protected and require a real override.
OBJECT_METHODS = frozenset({
    ("equals", "(Ljava/lang/Object;)Z"),
    ("getClass", "()Ljava/lang/Class;"),
    ("hashCode", "()I"),
    ("notify", "()V"),
    ("notifyAll", "()V"),
    ("toString", "()Ljava/lang/String;"),
    ("wait", "()V"),
    ("wait", "(J)V"),
    ("wait", "(JI)V"),
})


def validate(app, test):
    shared = sorted(app[0].keys() & test[0].keys())
    mismatches = [cls for cls in shared if app[0][cls] != test[0][cls]]

    runtime = dict(app[0])
    runtime.update(test[0])  # Android instrumentation loader selects test APK first.
    lookup = {}
    for cls, definition in runtime.items():
        _, parents, field_defs, method_defs, _, _, _ = definition
        lookup[cls] = (parents, dict(field_defs), dict(method_defs))

    def present(descriptor):
        element = jtype(descriptor)
        return element is None or element in runtime

    hierarchy = dict(app[1][3])
    hierarchy.update(test[1][3])

    def package(cls):
        return cls.rsplit("/", 1)[0]

    def subclass(caller, parent, seen=None):
        if caller == parent:
            return True
        if caller not in hierarchy:
            return False
        if seen is None:
            seen = set()
        if caller in seen:
            raise ValueError("cyclic j$ hierarchy")
        seen.add(caller)
        return any(subclass(ancestor, parent, seen.copy()) for ancestor in hierarchy[caller])

    def accessible(caller, declaring, access, kind, opcode):
        if access & 1:
            return True
        if access & 2:
            return caller == declaring
        if package(caller) == package(declaring):
            return True
        # Protected members require a subclass; for instance members,
        # invoke-super is the only cross-package receiver established by
        # the opcode alone. Other receiver types are not tracked here.
        if access & 4 and subclass(caller, declaring):
            return bool(access & 8) or (kind == 2 and opcode in (0x6f, 0x75))
        return False

    def resolves(caller, owner, member, kind, opcode, seen=None):
        if owner.startswith("["):
            # The VM creates array classes from a loadable element; array clone
            # is a public VM member, not a class_data_item declaration.
            if not present(owner):
                return False
            return kind == 2 and opcode in (0x6e, 0x74) and (
                member[1:] == ("clone", "()Ljava/lang/Object;") or
                member[1:] in OBJECT_METHODS
            )
        if owner not in lookup:
            return False
        if seen is None:
            seen = set()
        if owner in seen:
            raise ValueError("cyclic j$ hierarchy")
        seen.add(owner)
        parents, fields, methods = lookup[owner]
        for candidate, access in (fields if kind == 1 else methods).items():
            if candidate[1:] == member[1:]:
                static = 0x60 <= opcode <= 0x6d if kind == 1 else opcode in (0x71, 0x77)
                return bool(access & 8) == static and accessible(caller, owner, access, kind, opcode)
        if kind == 2 and member[1] in ("<init>", "<clinit>"):
            return False
        if any(resolves(caller, parent, member, kind, opcode, seen.copy()) for parent in parents if jtype(parent)):
            return True
        # Every j$ class has an Object ancestor; absent a declaration/override,
        # these precise public instance methods resolve in the platform.
        return kind == 2 and opcode in (0x6e, 0x74) and member[1:] in OBJECT_METHODS and (
            "Ljava/lang/Object;" in parents or any(
                parent in lookup and inherits_object(parent, {owner}) for parent in parents
            )
        )

    def inherits_object(owner, seen):
        if owner in seen:
            raise ValueError("cyclic j$ hierarchy")
        seen.add(owner)
        return any(parent == "Ljava/lang/Object;" or (
            parent in lookup and inherits_object(parent, seen.copy())
        ) for parent in lookup[owner][0])

    failures = []
    for source, (_, refs) in enumerate((app, test)):
        def selected(caller):
            return not (source == 0 and caller in test[0])
        for caller, cls in refs[0]:
            if selected(caller) and not present(cls):
                failures.append(cls)
        for kind in (1, 2):
            for caller, (opcode, member) in refs[kind]:
                if selected(caller) and not resolves(caller, member[0], member, kind, opcode):
                    failures.append(f"{member[0]}->{member[1]}:{member[2]}")
    errors = []
    if mismatches:
        errors.append(f"{len(mismatches)} incompatible shared j$ definitions: {', '.join(mismatches[:5])}")
    if failures:
        errors.append(f"{len(set(failures))} unresolved executable j$ references: {', '.join(sorted(set(failures), key=lambda value: ('DesugarCollections' not in value, value))[:8])}")
    if errors:
        raise ValueError("; ".join(errors))
    return len(shared), len(runtime)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("app", type=Path)
    parser.add_argument("test", type=Path)
    args = parser.parse_args()
    try:
        shared, runtime = validate(apk(args.app), apk(args.test))
    except (OSError, ValueError, zipfile.BadZipFile) as error:
        parser.exit(1, f"desugar-linkage: {error}\n")
    print(f"desugar-linkage: {shared} equivalent shared j$ classes; {runtime} runtime j$ classes; executable linkage resolved")


if __name__ == "__main__":
    main()
