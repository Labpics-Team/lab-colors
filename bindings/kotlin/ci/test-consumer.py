#!/usr/bin/env python3
"""Проверить установленный SDK на Android и в приложении после R8."""
import json
import os
from pathlib import Path
import struct
import shutil
import subprocess
import sys
import threading
import uuid
from typing import IO
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


class CrashBlock:
    """Retain only the first matching block; never replace it with a later tail."""
    def __init__(self):
        self.text = ""
        self.pending = ""
        self.in_block = False
        self.found = False
        self.complete = False
        self.truncated = False
        self.overflow = False
        self.bytes = 0

    def feed(self, line: str) -> None:
        self.bytes += len(line.encode())
        if self.bytes > 1 << 20:
            self.overflow = True
            self.truncated = self.truncated or (self.found and not self.complete)
            raise ValueError("startup capture overflow")
        if self.complete:
            return
        if "FATAL EXCEPTION:" in line:
            if self.found:
                self.complete = True
                return
            self.pending = line
            self.in_block = True
        elif "Process:" in line and not self.found:
            if "Process: pics.lab.colors.consumer," in line:
                self.found = True
                self.truncated = not self.in_block
                self.text = self.pending + line
            else:
                self.pending = ""
                self.in_block = False
        elif self.found:
            self.text += line

    def finish(self) -> None:
        if self.found and not self.complete:
            self.truncated = True
        elif self.in_block and not self.found:
            self.truncated = True


class CrashCapture:
    def __init__(self, adb: Path):
        self.adb = adb
        self.block = CrashBlock()
        self.token = uuid.uuid4().hex
        self.started = threading.Event()
        self.ended = threading.Event()
        self.error = None
        self.process: subprocess.Popen[bytes]
        self.stream: IO[bytes]
        self.reader: threading.Thread
        self.launched = False
        self.stopping = False

    def marker(self, phase: str) -> None:
        subprocess.run([str(self.adb), "shell", "log", "-p", "e", "-t", "ConsumerCapture",
                        self.token + ":" + phase], check=True, timeout=30)

    def read(self) -> None:
        try:
            while True:
                line = self.stream.readline((1 << 20) + 1)
                if not line:
                    if not self.stopping:
                        raise ValueError("startup capture unexpected EOF")
                    return
                text = line.decode("utf-8", errors="replace")
                if len(line) > 1 << 20:
                    self.block.overflow = True
                    self.block.truncated = self.block.found and not self.block.complete
                    raise ValueError("startup capture line overflow")
                if "ConsumerCapture:" in text and self.token + ":start" in text:
                    self.started.set()
                elif "ConsumerCapture:" in text and self.token + ":end" in text:
                    self.block.finish()
                    self.ended.set()
                    return
                elif self.started.is_set() and "AndroidRuntime:" in text:
                    self.block.feed(text)
        except BaseException as error:
            self.error = error
            self.started.set()
            self.ended.set()
            if self.process.poll() is None:
                self.process.terminate()

    def start(self) -> None:
        subprocess.run([str(self.adb), "logcat", "-b", "crash", "-c"], check=True, timeout=30)
        self.process = subprocess.Popen([str(self.adb), "logcat", "-b", "crash", "-b", "main",
                                         "-v", "threadtime", "AndroidRuntime:E", "ConsumerCapture:E", "*:S"],
                                        stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        self.launched = True
        assert self.process.stdout is not None
        self.stream = self.process.stdout
        self.reader = threading.Thread(target=self.read, name="consumer-crash-capture")
        self.reader.start()
        self.marker("start")
        if not self.started.wait(30):
            raise ValueError("startup capture subscription marker not observed")
        if self.error:
            raise self.error

    def finish(self) -> None:
        if self.error:
            raise self.error
        self.marker("end")
        if not self.ended.wait(30):
            raise ValueError("startup capture end marker not observed")
        if self.error:
            raise self.error

    def close(self) -> None:
        self.stopping = True
        if self.launched:
            if self.process.poll() is None:
                self.process.terminate()
            try:
                self.process.wait(timeout=30)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=30)
            finally:
                self.reader.join(30)
                if self.reader.is_alive():
                    raise ValueError("startup capture reader did not stop")
                self.stream.close()

    def save(self, evidence: Path, error: BaseException | None) -> None:
        block = self.block
        status = "overflow" if block.overflow else "error" if error else "truncated" if block.truncated else "captured" if block.found else "no-match"
        (evidence / "first-startup-exception.txt").write_text(block.text)
        (evidence / "startup-capture.json").write_text(json.dumps({
            "status": status, "exceptionFound": block.found, "truncated": block.truncated,
            "overflow": block.overflow, "bytesObserved": block.bytes,
            "error": repr(error) if error else None}) + "\n")


def connected_test(command: list[str], consumer: Path, environment: dict, adb: Path, variant: str) -> None:
    evidence = consumer / "build/outputs/consumer-diagnostics" / variant
    evidence.mkdir(parents=True, exist_ok=True)
    capture = CrashCapture(adb)
    primary = None
    capture_error = None
    try:
        capture.start()
        try:
            result = subprocess.run(command, cwd=consumer, env=environment, check=False)
        except BaseException as error:
            primary = error
            (evidence / "connected-exit.json").write_text(json.dumps({"exception": repr(error)}) + "\n")
        else:
            if result.returncode:
                primary = subprocess.CalledProcessError(result.returncode, command)
            (evidence / "connected-exit.json").write_text(json.dumps({"returncode": result.returncode}) + "\n")
        capture.finish()
    except BaseException as error:
        capture_error = error
    finally:
        try:
            capture.close()
        except BaseException as error:
            if capture_error:
                capture_error = BaseExceptionGroup("capture and cleanup failed", [capture_error, error])
            else:
                capture_error = error
        try:
            capture.save(evidence, capture_error)
        except BaseException as error:
            capture_error = error
    if primary:
        if capture_error:
            raise primary from capture_error
        raise primary
    if capture_error:
        raise capture_error


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
        connected_test([executable, "--no-daemon", "--max-workers=2", "-PconsumerBuildType=" + variant,
                        "-Pandroid.testInstrumentationRunnerArguments.expectedPageSize=" + str(expected_page),
                        "connected" + variant.title() + "AndroidTest"], consumer, environment, adb, variant)
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


def inventory(consumer: Path, stages: dict | None = None) -> None:
    evidence = consumer / "build/outputs/consumer-diagnostics"
    evidence.mkdir(parents=True, exist_ok=True)
    state_path = evidence / "assembly-stages.json"
    if stages is not None:
        state_path.write_text(json.dumps(stages) + "\n")
        state = stages
    else:
        state = json.loads(state_path.read_text()) if state_path.is_file() else {variant: {"app": "not-built", "test": "not-built"} for variant in ("debug", "release")}
    outputs = consumer / "build/outputs"
    categories = {}
    missing = []
    for variant in ("debug", "release"):
        required = {"app-apk": list((outputs / "apk" / variant).glob("*.apk")),
                    "test-apk": list((outputs / "apk/androidTest" / variant).glob("*.apk"))}
        if variant == "release":
            for name in ("mapping.txt", "configuration.txt", "usage.txt", "seeds.txt"):
                required[name] = [outputs / "mapping/release" / name]
        for category, paths in required.items():
            files = [path for path in paths if path.is_file() and (path.stat().st_size or category in ("usage.txt", "seeds.txt"))]
            stage = state[variant]["test" if category == "test-apk" else "app"]
            status = "not-built" if stage != "assembled" else "present" if files else "missing"
            key = variant + "/" + category
            categories[key] = {"status": status, "files": [str(path.relative_to(outputs)) for path in files]}
            if status == "missing":
                missing.append(key)
    (evidence / "artifact-inventory.json").write_text(json.dumps({"stages": state, "categories": categories, "missing": missing}, indent=2) + "\n")
    if missing:
        raise ValueError("missing required capture artifact: " + ", ".join(missing))


def assemble(consumer: Path) -> None:
    executable = (consumer / "gradle-path.txt").read_text().strip()
    environment = dict(os.environ, PATH=os.environ["JAVA_HOME"] + "/bin:/usr/bin:/bin")
    stages = {variant: {"app": "not-built", "test": "not-built"} for variant in ("debug", "release")}
    inventory(consumer, stages)
    for variant in ("debug", "release"):
        evidence = consumer / "build/outputs/consumer-diagnostics" / variant
        evidence.mkdir(parents=True, exist_ok=True)
        for configuration in (variant + "RuntimeClasspath", variant + "AndroidTestRuntimeClasspath"):
            with (evidence / (configuration + ".txt")).open("w") as output:
                subprocess.run([executable, "--no-daemon", "--max-workers=2", "-PconsumerBuildType=" + variant,
                                "dependencies", "--configuration", configuration],
                               cwd=consumer, env=environment, stdout=output, check=True)
            report = evidence / (configuration + ".txt")
            if report.stat().st_size > 1 << 20 or "FAILED" in report.read_text():
                raise ValueError("consumer dependency identity is incomplete or exceeds its budget")
        for category, task in (("app", "assemble" + variant.title()),
                               ("test", "assemble" + variant.title() + "AndroidTest")):
            subprocess.run([executable, "--no-daemon", "--max-workers=2", "-PconsumerBuildType=" + variant, task],
                           cwd=consumer, env=environment, check=True)
            stages[variant][category] = "assembled"
            inventory(consumer, stages)
        if variant == "release":
            app_apks = list((consumer / "build/outputs/apk/release").glob("*.apk"))
            test_apks = list((consumer / "build/outputs/apk/androidTest/release").glob("*.apk"))
            if len(app_apks) != 1 or len(test_apks) != 1:
                raise ValueError("expected one release app/test APK pair")
            with (evidence / "desugar-linkage.txt").open("w") as output:
                subprocess.run([sys.executable, str(Path(__file__).with_name("desugar-linkage.py")),
                                str(app_apks[0]), str(test_apks[0])], check=True,
                               stdout=output, stderr=subprocess.STDOUT)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: test-consumer.py CONSUMER EXPECTED_PAGE_SIZE|--assemble|--inventory")
    consumer = Path(sys.argv[1]).resolve()
    try:
        if sys.argv[2] == "--assemble":
            assemble(consumer)
        elif sys.argv[2] == "--inventory":
            inventory(consumer)
        else:
            main(consumer, int(sys.argv[2]))
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.returncode) from error
