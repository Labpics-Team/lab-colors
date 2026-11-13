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


DEBUG = ["runtimeHistoryAndThreadMigration", "authoringIsCanonicalAndTransactional",
         "hostFailureReentryRevocationAndRecovery", "numericalVectors", "sourceIdentityAndUntrustedEnvelope",
         "compiledApiIncludesEveryNativeAndAuthoringOperation", "allCapabilityFieldsMatchTheCanonicalManifest",
         "applicationUsesTheRequestedMemoryPageSize"]
RELEASE = ["optimizedApplicationExecutesTheInstalledNativePackage", "applicationUsesTheRequestedMemoryPageSize"]

ADB = r'''
import json, os, pathlib, sys
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
else:
    save()
    raise SystemExit("unrecognized adb arguments: " + repr(args))
save()
'''


class PreparedRuntimeTests(unittest.TestCase):
    def exercise(self, root, *, debug_output=None, release_output=None, debug_exit=0, clear_result="Success", verify=None):
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
        captures = []

        class Capture:
            def __init__(self, adb):
                self.block = SimpleNamespace(text="")
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
            return real_run(actual_command(command), **kwargs)
        def query(command, **kwargs):
            return real_run(actual_command(command), stdout=subprocess.PIPE, check=True, **kwargs).stdout

        artifact = SimpleNamespace(verify_artifact=verify or (lambda path: {}))
        environment = {"ANDROID_HOME": str(sdk), "FAKE_ADB_STATE": str(state), "FAKE_REPORT_DEBUG": str(debug),
                       "FAKE_REPORT_RELEASE": str(release), "FAKE_EXIT_DEBUG": str(debug_exit),
                       "FAKE_CLEAR_RESULT": clear_result}
        try:
            with patch.dict(os.environ, environment), patch.dict(sys.modules, {"consumer_artifact": artifact}), \
                 patch.object(runtime, "CrashCapture", Capture), patch.object(runtime.subprocess, "run", side_effect=execute), \
                 patch.object(runtime.subprocess, "check_output", side_effect=query), \
                 redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
                runtime.main(root, 16384)
        finally:
            self.assertTrue(all(capture.closed for capture in captures))
        return json.loads(state.read_text())["calls"]

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

    def test_previous_app_state_must_be_cleared_before_any_installation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(ValueError, "clear previous"):
                self.exercise(root, clear_result="Error")
            calls = json.loads((root / "adb-state.json").read_text())["calls"]
            self.assertFalse(any(call[0] == "install" for call in calls))

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
