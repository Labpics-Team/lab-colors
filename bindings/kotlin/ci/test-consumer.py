#!/usr/bin/env python3
"""Проверить установленный SDK на Android и в приложении после R8."""
import json
import os
from collections.abc import Iterable
from pathlib import Path
import struct
import shutil
import subprocess
import sys
import threading
import time
import uuid
from tempfile import TemporaryFile
from typing import IO
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


def verify_instrumentation(lines: Iterable[str], required: set[str]) -> list[dict]:
    """Принять завершённый raw-протокол AndroidJUnitRunner, а не exit code adb."""
    # AndroidJUnitRunner reports paired start/success status bundles, a stable
    # total count and a final Instrumentation result (-1). Failure and ignored
    # status codes must never become a successful runtime qualification.
    status = {}
    result = {}
    completed = []
    seen = set()
    active = None
    total = None
    last_key = None
    finished = False
    bytes_observed = 0
    for line in lines:
        bytes_observed += len(line.encode("utf-8"))
        if bytes_observed > 1 << 20:
            raise ValueError("instrumentation report exceeds its capture budget")
        text = line.rstrip("\r\n")
        if finished:
            if text.strip():
                raise ValueError("instrumentation emitted data after completion")
            continue
        if text.startswith("INSTRUMENTATION_STATUS: "):
            fields, content = status, text.removeprefix("INSTRUMENTATION_STATUS: ")
        elif text.startswith("INSTRUMENTATION_RESULT: "):
            if status or active:
                raise ValueError("instrumentation ended before its active test completed")
            fields, content = result, text.removeprefix("INSTRUMENTATION_RESULT: ")
        else:
            fields = None
        if fields is not None:
            key, separator, value = content.partition("=")
            if not separator or not key or key in fields:
                raise ValueError("malformed or duplicate instrumentation field")
            if key in ("shortMsg", "longMsg", "Error") and value:
                raise ValueError("instrumentation reported an execution error: " + value)
            fields[key] = value
            last_key = key
            continue
        if text.startswith("INSTRUMENTATION_STATUS_CODE: "):
            code = text.removeprefix("INSTRUMENTATION_STATUS_CODE: ")
            if code not in ("1", "0"):
                raise ValueError("instrumentation test failed, was skipped, or has an unknown status: " + code)
            if result:
                raise ValueError("instrumentation test status follows its final result")
            if status.get("id") != "AndroidJUnitRunner" or not status.get("class") or not status.get("test"):
                raise ValueError("instrumentation test identity is missing")
            if any(not status.get(key, "").isascii() or not status.get(key, "").isdigit()
                   for key in ("numtests", "current")):
                raise ValueError("instrumentation test counts are invalid")
            count, current = int(status["numtests"]), int(status["current"])
            if not count or (total is not None and count != total) or current != len(completed) + 1 or current > count:
                raise ValueError("instrumentation test count changed or sequence is incomplete")
            total = count
            identity = (status["class"], status["test"])
            if code == "1":
                if active is not None or identity in seen:
                    raise ValueError("instrumentation test started more than once")
                active = identity
            else:
                if active != identity or status.get("stack"):
                    raise ValueError("instrumentation success has no matching clean start")
                completed.append({"class": identity[0], "test": identity[1]})
                seen.add(identity)
                active = None
            status = {}
            last_key = None
        elif text.startswith("INSTRUMENTATION_CODE: "):
            if text.removeprefix("INSTRUMENTATION_CODE: ") != "-1" or status or active or not total or len(completed) != total:
                raise ValueError("instrumentation run did not complete every declared test")
            finished = True
            last_key = None
        elif text.startswith("INSTRUMENTATION_") or (text.strip() and last_key not in ("stream", "stack")):
            raise ValueError("invalid instrumentation protocol line: " + text)
    if not finished:
        raise ValueError("instrumentation run has no completion result")
    passed = {case["test"] for case in completed}
    if not required.issubset(passed):
        raise ValueError("consumer did not execute its required tests: " + str(required - passed))
    return completed


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


def connected_test(command: list[str], consumer: Path, environment: dict, adb: Path, variant: str,
                   evidence_dir: Path | None = None) -> Path:
    evidence = evidence_dir or consumer / "build/outputs/consumer-diagnostics" / variant
    evidence.mkdir(parents=True, exist_ok=True)
    output = evidence / "instrumentation.txt"
    capture = CrashCapture(adb)
    primary = None
    capture_error = None
    try:
        capture.start()
        try:
            with output.open("w", encoding="utf-8") as stream, \
                 (evidence / "instrumentation-stderr.txt").open("w", encoding="utf-8") as errors:
                result = subprocess.run(command, cwd=consumer, env=environment, check=False,
                                        stdout=stream, stderr=errors, text=True)
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
    if primary is not None or capture_error is not None or capture.block.found:
        print(f"{variant}: Android startup capture", file=sys.stderr)
        print(capture.block.text or "No matching AndroidRuntime exception was captured.",
              file=sys.stderr, flush=True)
        if capture_error is not None:
            print(f"Startup capture failed: {capture_error!r}", file=sys.stderr, flush=True)
    if primary:
        if capture_error:
            raise primary from capture_error
        raise primary
    if capture_error:
        raise capture_error
    if capture.block.found:
        raise ValueError("consumer process reported a fatal AndroidRuntime exception")
    return output


def wait_for_package_manager(adb: Path, timeout: float = 30) -> None:
    """Ждать регистрацию нужного Binder-сервиса, а не только завершение boot."""
    deadline = time.monotonic() + timeout
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("Android Package Manager did not register before the readiness deadline")
        state = subprocess.check_output(
            [str(adb), "shell", "service", "check", "package"], text=True, timeout=remaining,
        ).strip()
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("Android Package Manager did not register before the readiness deadline")
        if state == "Service package: found":
            return
        if state != "Service package: not found":
            raise ValueError("invalid Android Package Manager readiness response: " + repr(state))
        time.sleep(min(0.5, remaining))


def capture_device_failure(adb: Path, evidence: Path) -> None:
    """Read the guest before teardown; app-scoped Java capture misses native/system deaths."""
    evidence.mkdir(parents=True, exist_ok=True)
    record = {"probe": "device-logcat"}
    with TemporaryFile() as output:
        try:
            result = subprocess.run(
                [str(adb), "logcat", "-d", "-b", "crash", "-b", "main", "-b", "system",
                 "-v", "threadtime", "-t", "200", "*:V"],
                stdout=output, stderr=subprocess.STDOUT, timeout=10, check=False)
            record.update({"status": "completed", "returncode": result.returncode})
        except (OSError, subprocess.SubprocessError) as error:
            record.update({"status": "probe-error", "error": type(error).__name__})
        size = output.seek(0, os.SEEK_END)
        output.seek(max(0, size - 65536))
        content = output.read(65536)
    record.update({"truncated": size > 65536, "bytesObserved": size})
    (evidence / "device-logcat.txt").write_bytes(content)
    (evidence / "device-logcat.json").write_text(json.dumps(record) + "\n")
    print("ANDROID_DEVICE_FAILURE " + json.dumps(record | {
        "output": content[-4096:].decode("utf-8", errors="replace"),
        "logTruncated": len(content) > 4096}), file=sys.stderr, flush=True)


def main(consumer: Path, expected_page: int) -> None:
    try:
        run_consumer(consumer, expected_page)
    except Exception:
        try:
            capture_device_failure(Path(os.environ["ANDROID_HOME"]) / "platform-tools/adb",
                                   consumer / "runtime")
        except BaseException as error:
            try:
                print("ANDROID_DEVICE_FAILURE " + json.dumps({
                    "probe": "device-logcat", "status": "probe-error", "error": type(error).__name__}),
                    file=sys.stderr, flush=True)
            except BaseException:
                pass
        raise


def run_consumer(consumer: Path, expected_page: int) -> None:
    from consumer_artifact import verify_artifact

    sdk = Path(os.environ["ANDROID_HOME"])
    adb = sdk / "platform-tools/adb"
    if expected_page not in (4096, 16384):
        raise ValueError("unknown memory page contract")
    verify_artifact(consumer)
    environment = dict(os.environ)
    expected_tests = {
        "debug": {"runtimeHistoryAndThreadMigration", "authoringIsCanonicalAndTransactional", "hostFailureReentryRevocationAndRecovery",
                  "numericalVectors", "sourceIdentityAndUntrustedEnvelope", "compiledApiIncludesEveryNativeAndAuthoringOperation",
                  "allCapabilityFieldsMatchTheCanonicalManifest", "applicationUsesTheRequestedMemoryPageSize"},
        "release": {"optimizedApplicationExecutesTheInstalledNativePackage", "applicationUsesTheRequestedMemoryPageSize"},
    }
    for variant, tests in expected_tests.items():
        application_apks = list((consumer / "outputs/apk" / variant).glob("*.apk"))
        test_apks = list((consumer / "outputs/apk/androidTest" / variant).glob("*.apk"))
        if len(application_apks) != 1 or len(test_apks) != 1:
            raise ValueError("expected one verified application/test APK pair")
        wait_for_package_manager(adb)
        # Application.onCreate должен выполнить этот APK, а не оставить успех
        # от предыдущей debug/release установки в SharedPreferences.
        package = "pics.lab.colors.consumer"
        installed = subprocess.check_output([str(adb), "shell", "pm", "list", "packages", package], text=True).splitlines()
        if "package:" + package in installed:
            cleared = subprocess.check_output([str(adb), "shell", "pm", "clear", package], text=True).strip()
            if cleared != "Success":
                raise ValueError("cannot clear previous consumer application state")
        for apk in (application_apks[0], test_apks[0]):
            installed = subprocess.check_output([str(adb), "install", "-r", "-t", str(apk)], text=True).strip().splitlines()
            if not installed or installed[-1] != "Success":
                raise ValueError("Android did not confirm installation of the verified APK")
        # Mutable runtime evidence never changes the material checked by its manifest.
        evidence = consumer / "runtime" / variant
        if evidence.exists():
            shutil.rmtree(evidence)
        output = connected_test([str(adb), "shell", "am", "instrument", "-w", "-r",
                                 "-e", "expectedPageSize", str(expected_page),
                                 "pics.lab.colors.consumer.test/androidx.test.runner.AndroidJUnitRunner"],
                                consumer, environment, adb, variant, evidence)
        with output.open(encoding="utf-8") as report:
            completed = verify_instrumentation(report, tests)
        verify_artifact(consumer)
        (evidence / "tests.json").write_text(json.dumps({"pageSize": expected_page, "tests": completed}) + "\n")
        print(f"{variant}: {len(completed)} Android instrumentation tests passed.")
    print(f"Installed SDK verified: debug, R8 release, {expected_page}-byte Android pages; all native dependencies inspected.")


def inventory(consumer: Path, stages: dict | None = None) -> None:
    from consumer_artifact import artifact_inventory

    evidence = consumer / "build/outputs/consumer-diagnostics"
    evidence.mkdir(parents=True, exist_ok=True)
    state_path = evidence / "assembly-stages.json"
    if stages is not None:
        state_path.write_text(json.dumps(stages) + "\n")
        state = stages
    else:
        state = json.loads(state_path.read_text()) if state_path.is_file() else {variant: {"app": "not-built", "test": "not-built"} for variant in ("debug", "release")}
    data = artifact_inventory(consumer / "build/outputs", state)
    (evidence / "artifact-inventory.json").write_text(json.dumps(data, indent=2) + "\n")
    if data["missing"]:
        raise ValueError("missing required capture artifact: " + ", ".join(data["missing"]))


def assemble(consumer: Path) -> None:
    from consumer_artifact import require_assembled

    executable = (consumer / "gradle-path.txt").read_text().strip()
    environment = dict(os.environ, PATH=os.environ["JAVA_HOME"] + "/bin:/usr/bin:/bin")
    stages = {variant: {"app": "not-built", "test": "not-built"} for variant in ("debug", "release")}
    inventory(consumer, stages)
    for variant in ("debug", "release"):
        evidence = consumer / "build/outputs/consumer-diagnostics" / variant
        evidence.mkdir(parents=True, exist_ok=True)
        for configuration in (variant + "RuntimeClasspath", variant + "AndroidTestRuntimeClasspath"):
            with (evidence / (configuration + ".txt")).open("w") as output:
                subprocess.run([executable, "--max-workers=2", "-PconsumerBuildType=" + variant,
                                "dependencies", "--configuration", configuration],
                               cwd=consumer, env=environment, stdout=output, check=True)
            report = evidence / (configuration + ".txt")
            if report.stat().st_size > 1 << 20 or "FAILED" in report.read_text():
                raise ValueError("consumer dependency identity is incomplete or exceeds its budget")
        for category, task in (("app", "assemble" + variant.title()),
                               ("test", "assemble" + variant.title() + "AndroidTest")):
            subprocess.run([executable, "--max-workers=2", "-PconsumerBuildType=" + variant, task],
                           cwd=consumer, env=environment, check=True)
            stages[variant][category] = "assembled"
            inventory(consumer, stages)
        app_apks = list((consumer / "build/outputs/apk" / variant).glob("*.apk"))
        test_apks = list((consumer / "build/outputs/apk/androidTest" / variant).glob("*.apk"))
        if len(app_apks) != 1 or len(test_apks) != 1:
            raise ValueError("expected one application/test APK pair")
        check_apk(app_apks[0], Path(os.environ["ANDROID_HOME"]))
        if variant == "release":
            with (evidence / "desugar-linkage.txt").open("w") as output:
                subprocess.run([sys.executable, str(Path(__file__).with_name("desugar-linkage.py")),
                                str(app_apks[0]), str(test_apks[0])], check=True,
                               stdout=output, stderr=subprocess.STDOUT)
    require_assembled(consumer / "build/outputs")


def stop(consumer: Path) -> None:
    """Завершить daemon этой версии в текущем Gradle User Home, сохранив cache."""
    executable = (consumer / "gradle-path.txt").read_text().strip()
    environment = dict(os.environ, PATH=os.environ["JAVA_HOME"] + "/bin:/usr/bin:/bin")
    result = subprocess.run([executable, "--stop"], cwd=consumer, env=environment, timeout=30,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    print(result.stdout, end="")
    result.check_returncode()
    # Gradle StopDispatcher сообщает этот отказ даже с exit 0.
    if "Unable to stop one of the daemons." in result.stdout:
        raise RuntimeError("Gradle did not stop every consumer daemon")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: test-consumer.py CONSUMER EXPECTED_PAGE_SIZE|--assemble|--inventory|--stop")
    consumer = Path(sys.argv[1]).resolve()
    try:
        if sys.argv[2] == "--assemble":
            assemble(consumer)
        elif sys.argv[2] == "--inventory":
            inventory(consumer)
        elif sys.argv[2] == "--stop":
            stop(consumer)
        else:
            main(consumer, int(sys.argv[2]))
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.returncode) from error
