"""Generate bounded Kotlin ABI keeps from saved test APKs; no build integration.

Method IDs (including declarations), their signatures and actual class hierarchy
are linkage roots. Field IDs alone also contain annotation enum metadata: only
fields reached by DEX field-access instructions contribute owners/signatures.
Exact classes retain all members; stdlib superclass/interface closure preserves
inherited facade static methods as well as Lambda/coroutine base identities.
"""
import argparse
from pathlib import Path
import re
import struct
import sys
import zipfile


class Binary:
    def __init__(self, data, endian="<"):
        self.data = data
        self.endian = endian

    def span(self, offset, size):
        if offset < 0 or size < 0 or offset + size > len(self.data):
            raise ValueError("binary range out of bounds")
        return self.data[offset:offset + size]

    def unpack(self, fmt, offset):
        return struct.unpack(self.endian + fmt, self.span(offset, struct.calcsize(fmt)))

    def uleb(self, offset):
        value = 0
        for shift in range(0, 35, 7):
            byte = self.span(offset, 1)[0]
            offset += 1
            if shift == 28 and byte > 15:
                raise ValueError("invalid ULEB128")
            value |= (byte & 127) << shift
            if byte < 128:
                return value, offset
        raise ValueError("invalid ULEB128")


def kotlin_type(descriptor):
    descriptor = descriptor.lstrip("[")
    return descriptor if descriptor.startswith("Lkotlin/") and descriptor.endswith(";") else None


def instruction_fields(binary, offset, field_count):
    """Walk standard DEX code units; collect field operands, not metadata IDs."""
    if offset % 4:
        raise ValueError("unaligned code item")
    _, _, _, _, _, count = binary.unpack("HHHHII", offset)
    units = binary.unpack("H" * count, offset + 16)
    pc = 0
    fields = set()
    while pc < count:
        word = units[pc]
        opcode = word & 255
        if opcode == 0 and word:
            if word == 0x100:
                width = 4 + 2 * units[pc + 1] if pc + 1 < count else count + 1
            elif word == 0x200:
                width = 2 + 4 * units[pc + 1] if pc + 1 < count else count + 1
            elif word == 0x300 and pc + 3 < count:
                size = units[pc + 2] | units[pc + 3] << 16
                width = 4 + (units[pc + 1] * size + 1) // 2
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
        if pc + width > count:
            raise ValueError("truncated DEX instruction")
        if 0x52 <= opcode <= 0x6d:
            index = units[pc + 1]
            if index >= field_count:
                raise ValueError("field operand out of bounds")
            fields.add(index)
        pc += width
    return fields


def dex_linkage(data):
    """Return Kotlin roots, defined classes, method IDs and executable field IDs."""
    if len(data) < 112 or data[:8] not in (b"dex\n035\0", b"dex\n037\0", b"dex\n038\0", b"dex\n039\0", b"dex\n040\0"):
        raise ValueError("unsupported standard DEX magic/header")
    binary = Binary(data)
    if binary.unpack("III", 32) != (len(data), 112, 0x12345678):
        raise ValueError("invalid DEX file size/header/endian")
    tables = []
    for position, width in ((56, 4), (64, 4), (72, 12), (80, 8), (88, 8), (96, 32)):
        count, offset = binary.unpack("II", position)
        if (count and (offset < 112 or offset % 4)) or (not count and offset):
            raise ValueError("invalid DEX table offset")
        binary.span(offset, count * width)
        tables.append((count, offset))
    data_size, data_off = binary.unpack("II", 104)
    if data_size and (data_off < 112 or data_off % 4 or data_off + data_size != len(data)):
        raise ValueError("invalid DEX data section")
    binary.span(data_off, data_size)

    def item(values, index):
        if index >= len(values):
            raise ValueError("DEX index out of bounds")
        return values[index]

    strings = []
    for i in range(tables[0][0]):
        offset, = binary.unpack("I", tables[0][1] + i * 4)
        length, offset = binary.uleb(offset)
        end = data.find(b"\0", offset)
        if end < 0:
            raise ValueError("unterminated DEX string")
        # MUTF-8 encodes UTF-16 surrogate units and NUL rather than UTF-8 scalars.
        raw = data[offset:end].replace(b"\xc0\x80", b"\0")
        try:
            text = raw.decode("utf-8", errors="surrogatepass")
        except UnicodeDecodeError as error:
            raise ValueError("invalid DEX string") from error
        if len(text.encode("utf-16-le", errors="surrogatepass")) // 2 != length:
            raise ValueError("invalid DEX string length")
        strings.append(text)
    types = [item(strings, binary.unpack("I", tables[1][1] + i * 4)[0]) for i in range(tables[1][0])]

    def type_list(offset):
        if not offset:
            return ()
        if offset % 4:
            raise ValueError("unaligned DEX type list")
        count, = binary.unpack("I", offset)
        return tuple(item(types, index) for index in binary.unpack("H" * count, offset + 4))

    protos = []
    for i in range(tables[2][0]):
        shorty, result, parameters = binary.unpack("III", tables[2][1] + i * 12)
        item(strings, shorty)
        protos.append((item(types, result), type_list(parameters)))
    fields = []
    for i in range(tables[3][0]):
        owner, field_type, name = binary.unpack("HHI", tables[3][1] + i * 8)
        fields.append((item(types, owner), item(strings, name), item(types, field_type)))
    methods = []
    roots = set()

    def add(*descriptors):
        roots.update(value for descriptor in descriptors if (value := kotlin_type(descriptor)))

    for i in range(tables[4][0]):
        owner, proto, name = binary.unpack("HHI", tables[4][1] + i * 8)
        result, parameters = item(protos, proto)
        owner = item(types, owner)
        methods.append((owner, item(strings, name), "(" + "".join(parameters) + ")" + result))
        add(owner, result, *parameters)
    defined = set()
    referenced_fields = set()
    for i in range(tables[5][0]):
        cls, _, parent, interfaces, _, _, class_data, _ = binary.unpack("IIIIIIII", tables[5][1] + i * 32)
        defined.add(item(types, cls))
        add(*type_list(interfaces))
        if parent != 0xffffffff:
            add(item(types, parent))
        if not class_data:
            continue
        sizes = []
        for _ in range(4):
            size, class_data = binary.uleb(class_data)
            sizes.append(size)
        for size in sizes[:2]:
            index = 0
            for _ in range(size):
                delta, class_data = binary.uleb(class_data)
                index += delta
                item(fields, index)
                _, class_data = binary.uleb(class_data)
        for size in sizes[2:]:
            index = 0
            for _ in range(size):
                delta, class_data = binary.uleb(class_data)
                index += delta
                item(methods, index)
                _, class_data = binary.uleb(class_data)
                code, class_data = binary.uleb(class_data)
                if code:
                    referenced_fields.update(instruction_fields(binary, code, len(fields)))
    runtime_fields = {fields[index] for index in referenced_fields}
    for owner, _, descriptor in runtime_fields:
        add(owner, descriptor)
    return roots, defined, set(methods), runtime_fields


def class_parents(data, kinds=None):
    """Read only the constant pool and declared JVM superclass/interfaces."""
    binary = Binary(data, ">")
    if binary.unpack("I", 0)[0] != 0xcafebabe:
        raise ValueError("invalid JVM class magic")
    count, = binary.unpack("H", 8)
    pool = [None] * count
    offset = 10
    index = 1
    while index < count:
        tag = binary.span(offset, 1)[0]
        offset += 1
        if tag == 1:
            size, = binary.unpack("H", offset)
            offset += 2
            pool[index] = binary.span(offset, size)
            offset += size
        elif tag == 7:
            pool[index] = binary.unpack("H", offset)[0]
            offset += 2
        else:
            width = {3: 4, 4: 4, 5: 8, 6: 8, 8: 2, 9: 4, 10: 4, 11: 4, 12: 4, 15: 3, 16: 2, 17: 4, 18: 4, 19: 2, 20: 2}.get(tag)
            if width is None:
                raise ValueError("invalid JVM constant pool tag")
            binary.span(offset, width)
            offset += width
            if tag in (5, 6):
                index += 1
        index += 1
    access, this, parent, size = binary.unpack("HHHH", offset)
    interfaces = binary.unpack("H" * size, offset + 8)

    def name(index):
        if not 0 < index < count or not isinstance(pool[index], int):
            raise ValueError("invalid JVM class index")
        utf = pool[index]
        if not 0 < utf < count or not isinstance(pool[utf], bytes):
            raise ValueError("invalid JVM name index")
        return "L" + pool[utf].decode("ascii") + ";"

    descriptor = name(this)
    if kinds is not None:
        kinds[descriptor] = "interface" if access & 0x0200 else "class"
    return descriptor, tuple(name(i) for i in ((parent,) if parent else ()) + interfaces)


def hierarchy_closure(roots, jar, defined=(), kinds=None):
    closure = set(roots)
    with zipfile.ZipFile(jar) as archive:
        pending = sorted(roots)
        names = set(archive.namelist())
        while pending:
            descriptor = pending.pop()
            entry = descriptor[1:-1] + ".class"
            if entry not in names:
                if descriptor in defined:
                    continue
                raise ValueError(f"Kotlin class absent from stdlib: {descriptor}")
            actual, parents = class_parents(archive.read(entry), kinds)
            if actual != descriptor:
                raise ValueError("JAR entry/class name mismatch")
            for parent in parents:
                if kotlin_type(parent) and parent not in closure:
                    closure.add(parent)
                    pending.append(parent)
    return closure


def apk_linkage(apks):
    combined = [set(), set(), set(), set()]
    for apk in apks:
        with zipfile.ZipFile(apk) as archive:
            entries = sorted(name for name in archive.namelist() if re.fullmatch(r"classes(?:[2-9]|[1-9][0-9]+)?\.dex", name))
            if not entries:
                raise ValueError(f"APK has no standard DEX: {apk}")
            for entry in entries:
                for target, values in zip(combined, dex_linkage(archive.read(entry))):
                    target.update(values)
    return tuple(combined)


def rules(classes, kinds=None):
    kinds = kinds or {}
    result = []
    for descriptor in sorted(classes):
        name = descriptor[1:-1].replace("/", ".")
        if not re.fullmatch(r"kotlin\.[A-Za-z0-9_$.]+", name):
            raise ValueError("invalid exact Kotlin class name")
        result.append(f"-keep {kinds.get(descriptor, 'class')} {name} {{ *; }}\n")
    return "".join(result)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("apks", nargs="+", type=Path)
    parser.add_argument("--stdlib", type=Path, help="pinned kotlin-stdlib JAR for parent/interface closure")
    parser.add_argument("--output", type=Path, help="otherwise emit rules to stdout")
    args = parser.parse_args()
    try:
        roots, defined, _, _ = apk_linkage(args.apks)
        kinds = {}
        classes = hierarchy_closure(roots, args.stdlib, defined, kinds) if args.stdlib else roots
        text = rules(classes, kinds)
        if args.output:
            args.output.write_text(text, encoding="utf-8", newline="\n")
        else:
            sys.stdout.write(text)
    except (ValueError, OSError, zipfile.BadZipFile) as error:
        parser.exit(1, f"kotlin-linkage: {error}\n")


if __name__ == "__main__":
    main()
