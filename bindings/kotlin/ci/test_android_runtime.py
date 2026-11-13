"""Контракт готовых APK и настоящего raw-протокола AndroidJUnitRunner."""
from contextlib import redirect_stderr, redirect_stdout
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("android_runtime", ROOT / "test-consumer.py")
runtime = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runtime)


def report(names, class_name="pics.lab.colors.consumer.ConsumerTest"):
    lines = []
    for current, name in enumerate(names, 1):
        fields = ["id=AndroidJUnitRunner", f"numtests={len(names)}", f"current={current}",
                  f"class={class_name}", f"test={name}"]
        for code in (1, 0):
            lines += ["INSTRUMENTATION_STATUS: " + field for field in fields]
            lines += ["INSTRUMENTATION_STATUS: stream=", class_name + ":" if code else ".",
                      f"INSTRUMENTATION_STATUS_CODE: {code}"]
    lines += ["INSTRUMENTATION_RESULT: stream=", "Time: 0.01", f"OK ({len(names)} tests)",
              "INSTRUMENTATION_CODE: -1"]
    return "\n".join(lines) + "\n"


class InstrumentationTests(unittest.TestCase):
    def test_accepts_each_paired_identity_and_complete_run_without_human_summary_dependency(self):
        output = report(["first", "second"]).replace("OK (2 tests)", "A different summary renderer")
        self.assertEqual(runtime.verify_instrumentation(iter(output.splitlines(True)), {"first", "second"}),
                         [{"class": "pics.lab.colors.consumer.ConsumerTest", "test": "first"},
                          {"class": "pics.lab.colors.consumer.ConsumerTest", "test": "second"}])

    def test_failure_skip_and_unknown_status_cannot_be_replaced_by_final_success(self):
        for code in (-1, -2, -3, -4, 2):
            with self.subTest(code=code):
                output = report(["first"]).replace("INSTRUMENTATION_STATUS_CODE: 0", f"INSTRUMENTATION_STATUS_CODE: {code}")
                with self.assertRaisesRegex(ValueError, "failed.*skipped.*unknown"):
                    runtime.verify_instrumentation(output.splitlines(True), {"first"})

    def test_refuses_empty_partial_duplicate_crashed_and_inconsistent_streams(self):
        healthy = report(["first", "second"])
        variants = {
            "empty": "",
            "zero": report([]),
            "no final result": healthy.rsplit("INSTRUMENTATION_CODE:", 1)[0],
            "cancelled final result": healthy.replace("INSTRUMENTATION_CODE: -1", "INSTRUMENTATION_CODE: 0"),
            "missing status identity": healthy.replace("INSTRUMENTATION_STATUS: id=AndroidJUnitRunner\n", "", 1),
            "foreign runner": healthy.replace("id=AndroidJUnitRunner", "id=unknown", 1),
            "duplicate field": healthy.replace("id=AndroidJUnitRunner", "id=AndroidJUnitRunner\nINSTRUMENTATION_STATUS: id=AndroidJUnitRunner", 1),
            "count changed": healthy.replace("numtests=2", "numtests=3", 1),
            "non-decimal count": healthy.replace("numtests=2", "numtests=2.0", 1),
            "unicode count": healthy.replace("numtests=2", "numtests=٢", 1),
            "missing preceding test": healthy.replace("current=1", "current=2", 1),
            "end does not match start": healthy.replace("test=first", "test=other", 1),
            "duplicate testcase": report(["first", "first"]),
            "more data after completion": healthy + healthy,
            "failure without status": "INSTRUMENTATION_FAILED: Process crashed.\n" + healthy,
            "crash final bundle": healthy.replace("INSTRUMENTATION_RESULT: stream=", "INSTRUMENTATION_RESULT: shortMsg=Process crashed."),
            "unfinished active testcase": healthy[:healthy.index("INSTRUMENTATION_STATUS_CODE: 1")] + "INSTRUMENTATION_CODE: -1\n",
            "unknown protocol": healthy.replace("INSTRUMENTATION_STATUS_CODE: 0", "INSTRUMENTATION_ABORTED: reason", 1),
        }
        for name, output in variants.items():
            with self.subTest(name=name), self.assertRaises(ValueError):
                runtime.verify_instrumentation(output.splitlines(True), {"first", "second"})

    def test_declared_success_does_not_replace_missing_required_property(self):
        with self.assertRaisesRegex(ValueError, "required tests"):
            runtime.verify_instrumentation(report(["unrelated"]).splitlines(True), {"required"})

    def test_report_capture_is_bounded(self):
        with self.assertRaisesRegex(ValueError, "capture budget"):
            runtime.verify_instrumentation(["INSTRUMENTATION_RESULT: stream=" + "x" * (1 << 20)], set())

    def test_real_capture_rejects_consumer_fatal_with_successful_eight_test_protocol(self):
        from test_artifact import PipeProcess, PREFIX

        for package, fatal in (("pics.lab.colors.consumer", True), ("other.app", False)):
            with self.subTest(package=package), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                evidence = root / "runtime/debug"
                process = PipeProcess()
                captures = []
                original = runtime.CrashCapture
                command = ["adb", "shell", "am", "instrument", "-w", "-r"]

                def construct(adb):
                    capture = original(adb)
                    captures.append(capture)
                    return capture

                def execute(arguments, **kwargs):
                    if arguments == command:
                        self.assertTrue(captures[0].started.is_set())
                        kwargs["stdout"].write(report(DEBUG))
                        process.writer.write((PREFIX + "FATAL EXCEPTION: main\n" + PREFIX +
                                              f"Process: {package}, PID: 1234\n" + PREFIX +
                                              "java.lang.NoClassDefFoundError: NATIVE_STARTUP_FAILURE\n").encode())
                    elif arguments[1:3] == ["shell", "log"]:
                        process.writer.write(("10-24 12:34:56.789  1234  1234 E ConsumerCapture: " +
                                              arguments[-1] + "\n").encode())
                    elif arguments[1:] != ["logcat", "-b", "crash", "-c"]:
                        self.fail("unexpected capture command: " + repr(arguments))
                    return subprocess.CompletedProcess(arguments, 0)

                try:
                    with patch.object(runtime, "CrashCapture", side_effect=construct), \
                         patch.object(runtime.subprocess, "Popen", return_value=process), \
                         patch.object(runtime.subprocess, "run", side_effect=execute), \
                         redirect_stderr(io.StringIO()) as diagnostic:
                        if fatal:
                            with self.assertRaisesRegex(ValueError, "fatal AndroidRuntime"):
                                runtime.connected_test(command, root, {}, Path("adb"), "debug", evidence)
                            self.assertIn("NATIVE_STARTUP_FAILURE", diagnostic.getvalue())
                        else:
                            runtime.connected_test(command, root, {}, Path("adb"), "debug", evidence)
                            self.assertEqual(diagnostic.getvalue(), "")
                finally:
                    self.assertTrue(process.waited)
                    self.assertTrue(process.stdout.closed)
                    self.assertFalse(captures[0].reader.is_alive())
                # Prove this is a fatal-capture refusal, not a malformed report
                # or nonzero command result disguised as the counterexample.
                with (evidence / "instrumentation.txt").open() as output:
                    self.assertEqual(len(runtime.verify_instrumentation(output, set(DEBUG))), 8)
                self.assertEqual(json.loads((evidence / "connected-exit.json").read_text())["returncode"], 0)
                capture = json.loads((evidence / "startup-capture.json").read_text())
                self.assertEqual(capture["exceptionFound"], fatal)


DEBUG = ["runtimeHistoryAndThreadMigration", "authoringIsCanonicalAndTransactional",
         "hostFailureReentryRevocationAndRecovery", "numericalVectors", "sourceIdentityAndUntrustedEnvelope",
         "compiledApiIncludesEveryNativeAndAuthoringOperation", "allCapabilityFieldsMatchTheCanonicalManifest",
         "applicationUsesTheRequestedMemoryPageSize"]
RELEASE = ["optimizedApplicationExecutesTheInstalledNativePackage", "applicationUsesTheRequestedMemoryPageSize"]

PROCESS_CRASH = "INSTRUMENTATION_RESULT: shortMsg=Process crashed.\nINSTRUMENTATION_CODE: 0\n"
DEVICE_LOGCAT = b"""--------- beginning of crash
11-14 12:34:56.000  1234  1234 F libc    : Fatal signal 11 (SIGSEGV), code 1, fault addr 0x0
11-14 12:34:56.001  1235  1235 F DEBUG   : pid: 1234, tid: 1234, name: colors.consumer >>> pics.lab.colors.consumer <<<
11-14 12:34:56.002  1235  1235 F DEBUG   : backtrace: #00 pc 000000000000abcd /data/app/lib/x86_64/liblabcolors.so
--------- beginning of main
11-14 12:34:56.003  1234  1234 E libc    : native startup aborted
--------- beginning of system
11-14 12:34:56.004   999   999 I ActivityManager: Killing 1234:pics.lab.colors.consumer/u0a123: low memory
11-14 12:34:56.005   998   998 I lmkd    : Kill 'pics.lab.colors.consumer' (1234), oom_score_adj 0 to free 16384kB
"""

ADB = r'''
import json, os, pathlib, sys, time
state_path = pathlib.Path(os.environ["FAKE_ADB_STATE"])
state = json.loads(state_path.read_text())
args = sys.argv[1:]
state["calls"].append(args)
def save(): state_path.write_text(json.dumps(state))
if args == ["shell", "service", "check", "package"]:
    print("Service package: found")
elif args == ["shell", "pm", "list", "packages", "pics.lab.colors.consumer"]:
    print("package:pics.lab.colors.consumer")
elif args == ["shell", "pm", "clear", "pics.lab.colors.consumer"]:
    print(os.environ.get("FAKE_CLEAR_RESULT", "Success"))
elif args[:3] == ["install", "-r", "-t"]:
    state["variant"] = "release" if "/release/" in args[3].replace("\\", "/") else "debug"
    print("Performing Streamed Install")
    print("Success")
elif args[:4] == ["shell", "am", "instrument", "-w"]:
    sys.stdout.write(pathlib.Path(os.environ["FAKE_REPORT_" + state["variant"].upper()]).read_text())
    save()
    raise SystemExit(int(os.environ.get("FAKE_EXIT_" + state["variant"].upper(), "0")))
elif args[:2] == ["logcat", "-d"]:
    # Refuse narrowed Java/tag-only captures: native tombstones and system
    # process deaths must survive the real device-diagnostic command.
    buffers = [args[index + 1] for index, arg in enumerate(args) if arg == "-b"]
    if set(buffers) != {"crash", "main", "system"} or args[-1] != "*:V":
        save()
        raise SystemExit("device logcat is filtered or missing a buffer: " + repr(args))
    if args[args.index("-v") + 1] != "threadtime" or args[args.index("-t") + 1] != "200":
        save()
        raise SystemExit("device logcat must request bounded recent context: " + repr(args))
    sys.stdout.buffer.write(pathlib.Path(os.environ["FAKE_DEVICE_LOGCAT"]).read_bytes())
    sys.stdout.buffer.flush()
    save()
    time.sleep(float(os.environ.get("FAKE_DEVICE_LOGCAT_DELAY", "0")))
    raise SystemExit(int(os.environ.get("FAKE_DEVICE_LOGCAT_EXIT", "0")))
else:
    save()
    raise SystemExit("unrecognized adb arguments: " + repr(args))
save()
'''


class PreparedRuntimeTests(unittest.TestCase):
    def exercise(self, root, *, debug_output=None, release_output=None, debug_exit=0, clear_result="Success", verify=None,
                 device_output=DEVICE_LOGCAT, device_exit=0, device_timeout=False, diagnostic_sink=None):
        for variant in ("debug", "release"):
            for kind in ("apk", "apk/androidTest"):
                apk = root / "outputs" / kind / variant / "consumer.apk"
                apk.parent.mkdir(parents=True)
                apk.write_bytes((variant + kind).encode())
        sdk = root / "sdk"
        adb = sdk / "platform-tools/adb"
        adb.parent.mkdir(parents=True)
        adb.write_text(ADB)
        state = root / "adb-state.json"
        state.write_text(json.dumps({"calls": []}))
        debug = root / "debug.txt"
        debug.write_text(report(DEBUG) if debug_output is None else debug_output)
        release = root / "release.txt"
        release.write_text(report(RELEASE) if release_output is None else release_output)
        device = root / "device-logcat.txt"
        device.write_bytes(device_output)
        captures = []

        class Capture:
            def __init__(self, adb):
                self.block = SimpleNamespace(text="", found=False)
                self.closed = False
                captures.append(self)
            def start(self): pass
            def finish(self): pass
            def close(self): self.closed = True
            def save(self, evidence, error):
                (evidence / "startup-capture.json").write_text(json.dumps({"error": repr(error) if error else None}))

        real_run = subprocess.run
        def actual_command(command):
            self.assertEqual(command[0], str(adb))
            return [sys.executable, str(adb), *command[1:]]
        def execute(command, **kwargs):
            if command[1] == "logcat":
                self.assertGreater(kwargs["timeout"], 0)
                self.assertLessEqual(kwargs["timeout"], 10)
                if device_timeout:
                    # The production deadline is checked above. Only shorten
                    # this real child's timeout; run still kills and waits it.
                    kwargs["timeout"] = 0.5
            return real_run(actual_command(command), **kwargs)
        def query(command, **kwargs):
            return real_run(actual_command(command), stdout=subprocess.PIPE, check=True, **kwargs).stdout

        artifact = SimpleNamespace(verify_artifact=verify or (lambda path: {}))
        environment = {"ANDROID_HOME": str(sdk), "FAKE_ADB_STATE": str(state), "FAKE_REPORT_DEBUG": str(debug),
                       "FAKE_REPORT_RELEASE": str(release), "FAKE_EXIT_DEBUG": str(debug_exit),
                       "FAKE_CLEAR_RESULT": clear_result, "FAKE_DEVICE_LOGCAT": str(device),
                       "FAKE_DEVICE_LOGCAT_EXIT": str(device_exit),
                       "FAKE_DEVICE_LOGCAT_DELAY": "5" if device_timeout else "0"}
        try:
            with patch.dict(os.environ, environment), patch.dict(sys.modules, {"consumer_artifact": artifact}), \
                 patch.object(runtime, "CrashCapture", Capture), patch.object(runtime.subprocess, "run", side_effect=execute), \
                 patch.object(runtime.subprocess, "check_output", side_effect=query), \
                 redirect_stdout(io.StringIO()), redirect_stderr(diagnostic_sink or io.StringIO()):
                runtime.main(root, 16384)
        finally:
            self.assertTrue(all(capture.closed for capture in captures))
        return json.loads(state.read_text())["calls"]

    def assert_device_capture(self, root, output=DEVICE_LOGCAT, *, returncode=0, error=None):
        calls = json.loads((root / "adb-state.json").read_text())["calls"]
        probes = [call for call in calls if call[0] == "logcat"]
        self.assertEqual(len(probes), 1, "failure context must be captured once before main returns")
        self.assertEqual(calls[-1], probes[0], "device context must follow the failing runtime operation")
        self.assertEqual((root / "runtime/device-logcat.txt").read_bytes(), output[:65536])
        receipt = json.loads((root / "runtime/device-logcat.json").read_text())
        self.assertEqual(receipt["probe"], "device-logcat")
        self.assertEqual(receipt["truncated"], len(output) > 65536)
        self.assertEqual(receipt["bytesObserved"], min(len(output), 65537))
        if error:
            self.assertEqual(receipt["status"], "probe-error")
            self.assertEqual(receipt["error"], error)
            self.assertNotIn("returncode", receipt)
        else:
            self.assertEqual(receipt["status"], "completed")
            self.assertEqual(receipt["returncode"], returncode)
            self.assertNotIn("error", receipt)
        return probes[0]

    def test_real_runtime_entry_installs_same_prepared_pairs_and_checks_both_variants(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            verified = []
            calls = self.exercise(root, verify=lambda path: verified.append(path))
            self.assertEqual(verified, [root, root, root])
            instrument = [call for call in calls if call[:3] == ["shell", "am", "instrument"]]
            self.assertEqual(instrument, [["shell", "am", "instrument", "-w", "-r", "-e", "expectedPageSize", "16384",
                                           "pics.lab.colors.consumer.test/androidx.test.runner.AndroidJUnitRunner"]] * 2)
            installs = [call for call in calls if call[0] == "install"]
            self.assertEqual([Path(call[-1]).relative_to(root).as_posix() for call in installs],
                             ["outputs/apk/debug/consumer.apk", "outputs/apk/androidTest/debug/consumer.apk",
                              "outputs/apk/release/consumer.apk", "outputs/apk/androidTest/release/consumer.apk"])
            self.assertEqual(sum(call[:3] == ["shell", "pm", "clear"] for call in calls), 2)
            for variant, expected in (("debug", DEBUG), ("release", RELEASE)):
                result = json.loads((root / "runtime" / variant / "tests.json").read_text())
                self.assertEqual({case["test"] for case in result["tests"]}, set(expected))
                self.assertEqual(result["pageSize"], 16384)
                self.assertTrue((root / "runtime" / variant / "instrumentation.txt").is_file())
            self.assertFalse((root / "gradle-path.txt").exists())
            self.assertFalse(any(call[0] == "logcat" for call in calls))
            self.assertFalse((root / "runtime/device-logcat.txt").exists())
            self.assertFalse((root / "runtime/device-logcat.json").exists())

    def test_raw_crashed_process_with_adb_exit_zero_keeps_native_and_system_context(self):
        for output in (PROCESS_CRASH, "INSTRUMENTATION_CODE: 0\n"):
            with self.subTest(output=output), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                with self.assertRaises(ValueError):
                    self.exercise(root, debug_output=output)
                self.assertEqual(json.loads((root / "runtime/debug/connected-exit.json").read_text())["returncode"], 0)
                self.assertEqual((root / "runtime/debug/instrumentation.txt").read_text(), output)
                self.assert_device_capture(root)
                self.assertFalse((root / "runtime/debug/tests.json").exists())
                self.assertFalse((root / "runtime/release/tests.json").exists())

    def test_cancelled_runtime_does_not_start_a_new_device_probe(self):
        interruption = KeyboardInterrupt("cancel consumer")
        def cancel(path):
            raise interruption
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaises(KeyboardInterrupt) as caught:
                self.exercise(root, verify=cancel)
            self.assertIs(caught.exception, interruption)
            calls = json.loads((root / "adb-state.json").read_text())["calls"]
            self.assertEqual(calls, [])

    def test_failed_device_probe_cannot_replace_original_runtime_failure(self):
        for primary in ("crashed protocol", "adb exit 29"):
            for probe in ("nonzero", "timeout", "oversize"):
                with self.subTest(primary=primary, probe=probe), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    output = DEVICE_LOGCAT + b"\xff\xfe" + b"x" * 70000 if probe == "oversize" else DEVICE_LOGCAT
                    options = {"device_output": output, "device_exit": 43 if probe == "nonzero" else 0,
                               "device_timeout": probe == "timeout"}
                    if primary == "crashed protocol":
                        with self.assertRaisesRegex(ValueError, "Process crashed"):
                            self.exercise(root, debug_output=PROCESS_CRASH, **options)
                    else:
                        with self.assertRaises(subprocess.CalledProcessError) as caught:
                            self.exercise(root, debug_exit=29, **options)
                        self.assertEqual(caught.exception.returncode, 29)
                        self.assertEqual(caught.exception.cmd[1:4], ["shell", "am", "instrument"])
                    self.assert_device_capture(root, output, returncode=43 if probe == "nonzero" else 0,
                                               error="TimeoutExpired" if probe == "timeout" else None)
                    self.assertFalse((root / "runtime/release/tests.json").exists())

    def test_device_evidence_file_and_log_errors_keep_original_command_exit(self):
        class BrokenDiagnosticLog(io.StringIO):
            def write(self, text):
                if text.startswith("ANDROID_DEVICE_FAILURE "):
                    raise OSError("diagnostic log sink failed")
                return super().write(text)

        for failure in ("file", "log"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                if failure == "file":
                    (root / "runtime/device-logcat.txt").mkdir(parents=True)
                with self.assertRaises(subprocess.CalledProcessError) as caught:
                    self.exercise(root, debug_exit=29,
                                  diagnostic_sink=BrokenDiagnosticLog() if failure == "log" else None)
                self.assertEqual(caught.exception.returncode, 29)
                self.assertEqual(caught.exception.cmd[1:4], ["shell", "am", "instrument"])
                calls = json.loads((root / "adb-state.json").read_text())["calls"]
                self.assertEqual(sum(call[0] == "logcat" for call in calls), 1)
                if failure == "log":
                    self.assert_device_capture(root)

    def test_adb_success_does_not_qualify_empty_partial_or_skipped_execution(self):
        variants = ["", report(DEBUG).replace("INSTRUMENTATION_CODE: -1", ""),
                    report(DEBUG).replace("INSTRUMENTATION_STATUS_CODE: 0", "INSTRUMENTATION_STATUS_CODE: -3", 1)]
        for output in variants:
            with self.subTest(output=output[:80]), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                with self.assertRaises(ValueError):
                    self.exercise(root, debug_output=output)
                self.assertFalse((root / "runtime/release/tests.json").exists())
                self.assertFalse((root / "runtime/debug/tests.json").exists())

    def test_subprocess_failure_keeps_original_exit_and_capture_cleanup(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaises(subprocess.CalledProcessError) as caught:
                self.exercise(root, debug_exit=29)
            self.assertEqual(caught.exception.returncode, 29)
            self.assertFalse((root / "runtime/release/tests.json").exists())
            self.assertEqual(json.loads((root / "runtime/debug/connected-exit.json").read_text())["returncode"], 29)
            self.assert_device_capture(root)

    def test_previous_app_state_must_be_cleared_before_any_installation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(ValueError, "clear previous"):
                self.exercise(root, clear_result="Error")
            calls = json.loads((root / "adb-state.json").read_text())["calls"]
            self.assertFalse(any(call[0] == "install" for call in calls))
            self.assert_device_capture(root)

    def test_post_execution_material_change_refuses_release_and_success_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            checks = []
            def verify(path):
                checks.append(path)
                if len(checks) == 2:
                    raise ValueError("APK checksum changed")
            with self.assertRaisesRegex(ValueError, "checksum changed"):
                self.exercise(root, verify=verify)
            self.assertFalse((root / "runtime/debug/tests.json").exists())
            self.assertFalse((root / "runtime/release/tests.json").exists())
            self.assertEqual(len(checks), 2)


if __name__ == "__main__":
    unittest.main()
