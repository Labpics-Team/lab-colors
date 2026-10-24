#!/usr/bin/env python3
"""Проверить установленный SDK на Android и в приложении после R8."""
import os
from pathlib import Path
import struct
import shutil
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
        kind = struct.unpack_from("<I", data, header)[0]
        if word == 2 and kind == 0x6474E552:  # PT_GNU_RELRO
            address = struct.unpack_from("<Q", data, header + 16)[0]
            memory_size = struct.unpack_from("<Q", data, header + 40)[0]
            end = address + memory_size
            if end > 0xFFFFFFFFFFFFFFFF or end % 16384:
                raise ValueError("native RELRO end is not 16-KB aligned")
        if kind != 1:
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


def verify_reports(directory: Path, required: set[str]) -> None:
    passed = set()
    for report in directory.rglob("TEST-*.xml"):
        tree = ET.parse(report)
        for suite in tree.iter("testsuite"):
            if any(int(suite.get(key, "0")) != 0 for key in ("failures", "errors", "skipped")):
                raise ValueError("external consumer test suite failed or was skipped")
        for case in tree.iter("testcase"):
            if any(child.tag in ("failure", "error", "skipped") for child in case):
                raise ValueError("external consumer test failed or was skipped")
            passed.add(case.attrib["name"])
    if not required.issubset(passed):
        raise ValueError("consumer did not execute its required tests: " + str(required - passed))


def main(consumer: Path, expected_page: int) -> None:
    sdk = Path(os.environ["ANDROID_HOME"])
    adb = sdk / "platform-tools/adb"
    if expected_page not in (4096, 16384):
        raise ValueError("unknown memory page contract")
    executable = (consumer / "gradle-path.txt").read_text().strip()
    environment = dict(os.environ, PATH=os.environ["JAVA_HOME"] + "/bin:" + str(sdk / "platform-tools") + ":/usr/bin:/bin")
    expected_tests = {
        "debug": {"runtimeHistoryAndThreadMigration", "authoringIsCanonicalAndTransactional", "hostFailureReentryRevocationAndRecovery",
                  "numericalVectors", "sourceIdentityAndUntrustedEnvelope", "compiledApiIncludesEveryNativeAndAuthoringOperation",
                  "allCapabilityFieldsMatchTheCanonicalManifest", "applicationUsesTheRequestedMemoryPageSize"},
        "release": {"optimizedApplicationExecutesTheInstalledNativePackage", "applicationUsesTheRequestedMemoryPageSize"},
    }
    for variant, tests in expected_tests.items():
        # Application.onCreate должен выполнить этот APK, а не оставить успех
        # от предыдущей debug/release установки в SharedPreferences.
        package = "pics.lab.colors.consumer"
        installed = subprocess.check_output([str(adb), "shell", "pm", "list", "packages", package], text=True).splitlines()
        if "package:" + package in installed:
            cleared = subprocess.check_output([str(adb), "shell", "pm", "clear", package], text=True).strip()
            if cleared != "Success":
                raise ValueError("cannot clear previous consumer application state")
        reports = consumer / "build/outputs/androidTest-results"
        # Результат debug не может подтвердить пропущенный тест release.
        if reports.exists():
            shutil.rmtree(reports)
        subprocess.run([executable, "--no-daemon", "--max-workers=2", "-PconsumerBuildType=" + variant,
                        "-Pandroid.testInstrumentationRunnerArguments.expectedPageSize=" + str(expected_page),
                        "connected" + variant.title() + "AndroidTest"], cwd=consumer, env=environment, check=True)
        verify_reports(reports, tests)
        evidence = consumer / "build/outputs/verified-android-tests" / variant
        if evidence.exists():
            shutil.rmtree(evidence)
        shutil.copytree(reports, evidence)
        apks = list((consumer / "build/outputs/apk" / variant).glob("*.apk"))
        if len(apks) != 1:
            raise ValueError("expected one installed application APK")
        check_apk(apks[0], sdk)
    mapping = consumer / "build/outputs/mapping/release/mapping.txt"
    if not mapping.is_file() or not mapping.read_text().strip():
        raise ValueError("release consumer did not run R8")
    print(f"Installed SDK verified: debug, R8 release, {expected_page}-byte Android pages; all native dependencies inspected.")


def assemble(consumer: Path) -> None:
    executable = (consumer / "gradle-path.txt").read_text().strip()
    environment = dict(os.environ, PATH=os.environ["JAVA_HOME"] + "/bin:/usr/bin:/bin")
    for variant in ("debug", "release"):
        subprocess.run([executable, "--no-daemon", "--max-workers=2", "-PconsumerBuildType=" + variant,
                        "assemble" + variant.title(), "assemble" + variant.title() + "AndroidTest"],
                       cwd=consumer, env=environment, check=True)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: test-consumer.py CONSUMER EXPECTED_PAGE_SIZE|--assemble")
    consumer = Path(sys.argv[1]).resolve()
    if sys.argv[2] == "--assemble":
        assemble(consumer)
    else:
        main(consumer, int(sys.argv[2]))
