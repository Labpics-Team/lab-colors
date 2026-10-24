#!/usr/bin/env python3
"""Проверить установленный SDK на Android и в приложении после R8."""
import os
from pathlib import Path
import struct
import subprocess
import sys
import xml.etree.ElementTree as ET
import zipfile

MACHINES = {"arm64-v8a": (2, 183), "armeabi-v7a": (1, 40), "x86": (1, 3), "x86_64": (2, 62)}


def check_elf(data: bytes, abi: str) -> None:
    word, machine = MACHINES[abi]
    if data[:6] != b"\x7fELF" + bytes((word, 1)) or struct.unpack_from("<H", data, 18)[0] != machine:
        raise ValueError("native library has a foreign ELF ABI")
    offset = struct.unpack_from("<Q" if word == 2 else "<I", data, 32 if word == 2 else 28)[0]
    size, count = struct.unpack_from("<HH", data, 54 if word == 2 else 42)
    if size != (56 if word == 2 else 32) or count == 0 or offset + size * count > len(data):
        raise ValueError("native library has an invalid program header table")
    loads = 0
    for index in range(count):
        header = offset + size * index
        if struct.unpack_from("<I", data, header)[0] != 1:
            continue
        loads += 1
        if word == 2:
            file_offset, address = struct.unpack_from("<QQ", data, header + 8)
            file_size, memory_size, alignment = struct.unpack_from("<QQQ", data, header + 32)
        else:
            file_offset, address = struct.unpack_from("<II", data, header + 4)
            file_size, memory_size = struct.unpack_from("<II", data, header + 16)
            alignment = struct.unpack_from("<I", data, header + 28)[0]
        if alignment < (16384 if word == 2 else 4096) or alignment & (alignment - 1):
            raise ValueError("native library does not support the declared page size")
        if (address - file_offset) % alignment or file_offset + file_size > len(data) or memory_size < file_size:
            raise ValueError("native LOAD segment is inconsistent")
    if loads == 0:
        raise ValueError("native library contains no loadable segments")


def check_apk(path: Path, sdk: Path) -> None:
    with zipfile.ZipFile(path) as archive:
        libraries = [entry for entry in archive.namelist() if entry.endswith(".so")]
        expected = {f"lib/{abi}/liblabcolors.so" for abi in MACHINES}
        if not expected.issubset(libraries):
            raise ValueError("external APK is missing a product ABI")
        if not any(name.endswith("libjnidispatch.so") for name in libraries):
            raise ValueError("external APK is missing its declared JNA runtime")
        for name in libraries:
            parts = name.split("/")
            if len(parts) != 3 or parts[0] != "lib" or parts[1] not in MACHINES:
                raise ValueError("unknown native library location")
            check_elf(archive.read(name), parts[1])
    subprocess.run([str(sdk / "build-tools/36.0.0/zipalign"), "-c", "-P", "16", "4", str(path)], check=True)


def main(consumer: Path, expected_page: int) -> None:
    sdk = Path(os.environ["ANDROID_HOME"])
    adb = sdk / "platform-tools/adb"
    page = subprocess.check_output([str(adb), "shell", "getconf", "PAGE_SIZE"], text=True).strip()
    if page != str(expected_page):
        raise ValueError("emulator page size differs from the requested environment: " + page)
    executable = (consumer / "gradle-path.txt").read_text().strip()
    environment = dict(os.environ, PATH=os.environ["JAVA_HOME"] + "/bin:" + str(sdk / "platform-tools") + ":/usr/bin:/bin")
    expected_tests = {
        "debug": {"runtimeHistoryAndThreadMigration", "authoringIsCanonicalAndTransactional", "hostFailureReentryRevocationAndRecovery",
                  "numericalVectors", "sourceIdentityAndUntrustedEnvelope", "compiledApiIncludesEveryNativeAndAuthoringOperation",
                  "allCapabilityFieldsMatchTheCanonicalManifest"},
        "release": {"optimizedApplicationExecutesTheInstalledNativePackage"},
    }
    for variant, tests in expected_tests.items():
        subprocess.run([executable, "--no-daemon", "--max-workers=2", "-PconsumerBuildType=" + variant,
                        "connected" + variant.title() + "AndroidTest"], cwd=consumer, env=environment, check=True)
        passed = set()
        reports = list((consumer / "build/outputs/androidTest-results").rglob("TEST-*.xml"))
        for report in reports:
            tree = ET.parse(report)
            for case in tree.iter("testcase"):
                if list(case):
                    if any(child.tag in ("failure", "error", "skipped") for child in case):
                        raise ValueError("external consumer test failed or was skipped")
                passed.add(case.attrib["name"])
        if not tests.issubset(passed):
            raise ValueError("consumer did not execute its required tests: " + str(tests - passed))
        apks = list((consumer / "build/outputs/apk" / variant).glob("*.apk"))
        if len(apks) != 1:
            raise ValueError("expected one installed application APK")
        check_apk(apks[0], sdk)
    mapping = consumer / "build/outputs/mapping/release/mapping.txt"
    if not mapping.is_file() or not mapping.read_text().strip():
        raise ValueError("release consumer did not run R8")
    print(f"Installed SDK verified: debug, R8 release, {page}-byte Android pages; all native dependencies inspected.")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: test-consumer.py CONSUMER EXPECTED_PAGE_SIZE")
    main(Path(sys.argv[1]).resolve(), int(sys.argv[2]))
