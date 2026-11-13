"""Контрольные примеры для установки архива и проверки Android ELF."""
from contextlib import redirect_stderr
import hashlib
import io
import json
import os
import subprocess
import sys
import importlib.util
from pathlib import Path
import struct
import shutil
import tempfile
import threading
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parent


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


consumer = load("prepare-consumer")
elf = load("test-consumer")


PREFIX = "10-24 12:34:56.789  1234  1234 E AndroidRuntime: "


def emitted_artifacts(root, variant):
    paths = [f"apk/{variant}/app.apk", f"apk/androidTest/{variant}/test.apk"]
    if variant == "release":
        paths += ["mapping/release/" + name for name in ("mapping.txt", "configuration.txt", "usage.txt", "seeds.txt")]
    for path in paths:
        target = root / "build/outputs" / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text("produced")


class PipeProcess:
    """A real bounded pipe preserves streaming, backpressure and EOF behavior."""
    def __init__(self):
        read_fd, write_fd = os.pipe()
        self.stdout = os.fdopen(read_fd, "rb")
        self.writer = os.fdopen(write_fd, "wb", buffering=0)
        self.returncode = None
        self.waited = False

    def poll(self):
        return self.returncode

    def terminate(self):
        self.returncode = -15
        self.writer.close()

    def kill(self):
        self.terminate()

    def wait(self, timeout):
        self.waited = True
        return self.returncode


class AndroidReadinessTests(unittest.TestCase):
    def test_package_service_must_be_ready_again_after_successful_debug_tests(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for variant in ("debug", "release"):
                emitted_artifacts(root, variant)
            shutil.move(root / "build/outputs", root / "outputs")
            registered = True
            probes = []
            variants = []
            clears = []

            def execute(command, **kwargs):
                nonlocal registered
                args = command[1:]
                if args == ["shell", "service", "check", "package"]:
                    probes.append(registered)
                    if not registered:
                        registered = True
                        return "Service package: not found\n"
                    return "Service package: found\n"
                if not registered:
                    raise subprocess.CalledProcessError(20, command, stderr="cmd: Can't find service: package")
                if args == ["shell", "pm", "list", "packages", "pics.lab.colors.consumer"]:
                    return "package:pics.lab.colors.consumer\n"
                if args == ["shell", "pm", "clear", "pics.lab.colors.consumer"]:
                    clears.append(variants.copy())
                    return "Success\n"
                if args[:3] == ["install", "-r", "-t"]:
                    return "Success\n"
                self.fail("unexpected Android command: " + repr(command))

            def connected(command, consumer, environment, adb, variant, evidence_dir=None):
                nonlocal registered
                variants.append(variant)
                evidence_dir.mkdir(parents=True, exist_ok=True)
                transcript = evidence_dir / "instrumentation.txt"
                transcript.write_text("runtime result")
                # The original failure occurred after successful debug instrumentation.
                if variant == "debug":
                    registered = False
                return transcript

            with patch.dict(os.environ, {"ANDROID_HOME": "/sdk", "JAVA_HOME": "/java"}), \
                 patch.object(elf.subprocess, "check_output", side_effect=execute), \
                 patch.object(elf, "connected_test", side_effect=connected), \
                 patch("consumer_artifact.verify_artifact", return_value={}), \
                 patch.object(elf, "verify_instrumentation", return_value=[]), \
                 patch("time.sleep") as sleep, patch("builtins.print"):
                elf.main(root, 16384)
            self.assertEqual(variants, ["debug", "release"])
            self.assertEqual(probes, [True, False, True])
            self.assertEqual(clears, [[], ["debug"]])
            sleep.assert_called_once()

    def test_ready_service_is_accepted_without_waiting_or_replaying_commands(self):
        with patch.object(elf.subprocess, "check_output", return_value="Service package: found\n") as command, \
             patch("time.sleep") as sleep:
            elf.wait_for_package_manager(Path("adb"))
        command.assert_called_once()
        self.assertEqual(command.call_args.args[0], ["adb", "shell", "service", "check", "package"])
        self.assertGreater(command.call_args.kwargs["timeout"], 0)
        self.assertLessEqual(command.call_args.kwargs["timeout"], 30)
        sleep.assert_not_called()

    def test_one_deadline_rejects_missing_and_late_ready_service(self):
        for response in ["Service package: not found\n", "Service package: found\n"]:
            with self.subTest(response=response), \
                 patch.object(elf.subprocess, "check_output", return_value=response) as command, \
                 patch("time.monotonic", side_effect=[0, 0, 1]), patch("time.sleep") as sleep:
                with self.assertRaisesRegex(TimeoutError, "Package Manager.*readiness deadline"):
                    elf.wait_for_package_manager(Path("adb"), timeout=1)
                command.assert_called_once()
                self.assertEqual(command.call_args.kwargs["timeout"], 1)
                sleep.assert_not_called()

    def test_readiness_transport_and_malformed_responses_fail_without_retry(self):
        failures = [subprocess.CalledProcessError(1, ["adb"], stderr="device offline"),
                    subprocess.TimeoutExpired(["adb"], 30)]
        for failure in failures:
            with self.subTest(failure=failure), \
                 patch.object(elf.subprocess, "check_output", side_effect=failure) as command:
                with self.assertRaises(type(failure)) as caught:
                    elf.wait_for_package_manager(Path("adb"))
                self.assertIs(caught.exception, failure)
                command.assert_called_once()
        for response in ["", "Service package: unknown", "Service activity: found"]:
            with self.subTest(response=response), \
                 patch.object(elf.subprocess, "check_output", return_value=response) as command:
                with self.assertRaisesRegex(ValueError, "invalid.*readiness response"):
                    elf.wait_for_package_manager(Path("adb"))
                command.assert_called_once()

    def test_package_clear_failure_after_readiness_is_preserved_without_replaying_it(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for variant in ("debug", "release"):
                emitted_artifacts(root, variant)
            shutil.move(root / "build/outputs", root / "outputs")
            command = ["adb", "shell", "pm", "clear", "pics.lab.colors.consumer"]
            failure = subprocess.CalledProcessError(20, command)
            responses = ["Service package: found\n", "package:pics.lab.colors.consumer\n", failure]
            with patch.dict(os.environ, {"ANDROID_HOME": "/sdk", "JAVA_HOME": "/java"}), \
                 patch("consumer_artifact.verify_artifact", return_value={}), \
                 patch.object(elf.subprocess, "check_output", side_effect=responses) as adb, \
                 patch.object(elf, "connected_test") as connected, patch("time.sleep") as sleep:
                with self.assertRaises(subprocess.CalledProcessError) as caught:
                    elf.main(root, 16384)
            self.assertIs(caught.exception, failure)
            self.assertEqual(adb.call_count, 3)
            self.assertEqual(adb.call_args.args[0][1:], command[1:])
            connected.assert_not_called()
            sleep.assert_not_called()


class ArtifactTests(unittest.TestCase):
    def interval(self, root, log, result: int | BaseException):
        process = PipeProcess()
        captures = []
        original = elf.CrashCapture
        def construct(adb):
            capture = original(adb)
            captures.append(capture)
            return capture
        def execute(command, **kwargs):
            if "shell" in command:
                process.writer.write(("10-24 12:34:56.789  1234  1234 E ConsumerCapture: " + command[-1] + "\n").encode())
            elif command == ["gradle"]:
                self.assertTrue(captures[0].started.is_set())
                process.writer.write(log.encode())
                if isinstance(result, BaseException):
                    raise result
                return subprocess.CompletedProcess(command, result)
            return subprocess.CompletedProcess(command, 0)
        try:
            with patch.object(elf, "CrashCapture", side_effect=construct), patch.object(elf.subprocess, "Popen", return_value=process), patch.object(elf.subprocess, "run", side_effect=execute):
                elf.connected_test(["gradle"], root, {}, Path("adb"), "debug")
        finally:
            self.assertTrue(process.waited)
            self.assertFalse(captures[0].reader.is_alive())
            self.assertTrue(process.stdout.closed)
        return captures[0]

    def test_interval_retains_early_crash_after_noise(self):
        first = PREFIX + "FATAL EXCEPTION: main\n" + PREFIX + "Process: pics.lab.colors.consumer, PID: 1234\n" + PREFIX + "java.lang.NoClassDefFoundError: FIRST_CAUSE\n"
        noise = (PREFIX + "FATAL EXCEPTION: main\n" + PREFIX + "Process: other.app, PID: 1234\n" + PREFIX + "LATER_CAUSE\n") * 300
        for result_code in (0, 17):
            with self.subTest(result_code=result_code), tempfile.TemporaryDirectory() as directory, \
                 redirect_stderr(io.StringIO()) as diagnostic:
                root = Path(directory)
                if result_code:
                    with self.assertRaises(subprocess.CalledProcessError) as failure:
                        self.interval(root, first + noise, result_code)
                    self.assertEqual(failure.exception.returncode, result_code)
                    self.assertIn("FIRST_CAUSE", diagnostic.getvalue())
                    self.assertNotIn("LATER_CAUSE", diagnostic.getvalue())
                else:
                    with self.assertRaisesRegex(ValueError, "fatal AndroidRuntime"):
                        self.interval(root, first + noise, result_code)
                    self.assertIn("FIRST_CAUSE", diagnostic.getvalue())
                    self.assertNotIn("LATER_CAUSE", diagnostic.getvalue())
                evidence = root / "build/outputs/consumer-diagnostics/debug"
                block = (evidence / "first-startup-exception.txt").read_text()
                self.assertIn("FIRST_CAUSE", block)
                self.assertNotIn("LATER_CAUSE", block)
                self.assertFalse(json.loads((evidence / "startup-capture.json").read_text())["truncated"])

    def test_truncation_overflow_and_no_match_are_distinct(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            evidence = root / "build/outputs/consumer-diagnostics/debug"
            self.interval(root, "", 0)
            self.assertEqual(json.loads((evidence / "startup-capture.json").read_text())["status"], "no-match")
            with self.assertRaisesRegex(ValueError, "fatal AndroidRuntime"):
                self.interval(root, PREFIX + "FATAL EXCEPTION: main\n" + PREFIX + "Process: pics.lab.colors.consumer, PID: 1234\n" + PREFIX + "TRUNCATED\n", 0)
            self.assertEqual(json.loads((evidence / "startup-capture.json").read_text())["status"], "truncated")
            block = elf.CrashBlock()
            block.feed(PREFIX + "FATAL EXCEPTION: main\n")
            block.feed(PREFIX + "Process: pics.lab.colors.consumer, PID: 1234\n")
            with self.assertRaisesRegex(ValueError, "overflow"):
                block.feed(PREFIX + "x" * (1 << 20))
            self.assertTrue(block.overflow)
            self.assertTrue(block.truncated)
            self.assertLess(len(block.text.encode()), 1 << 20)
            block = elf.CrashBlock()
            block.feed(PREFIX + "FATAL EXCEPTION: main\n")
            block.feed(PREFIX + "Process: pics.lab.colors.consumer, PID: 1234")
            block.finish()
            self.assertTrue(block.truncated)

    def test_connected_exception_is_preserved_and_reader_joined(self):
        with tempfile.TemporaryDirectory() as directory:
            failure = OSError("connected launch failed")
            with self.assertRaises(OSError) as caught:
                self.interval(Path(directory), "", failure)
            self.assertIs(caught.exception, failure)
            receipt = json.loads((Path(directory) / "build/outputs/consumer-diagnostics/debug/connected-exit.json").read_text())
            self.assertIn("exception", receipt)

    def test_inventory_missing_categories_fail_with_reports(self):
        for missing in ("app", "test", "mapping.txt", "configuration.txt", "usage.txt", "seeds.txt"):
            with self.subTest(missing=missing), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / "gradle-path.txt").write_text("gradle")
                reports = root / "build/outputs/androidTest-results"
                reports.mkdir(parents=True)
                (reports / "TEST-report.xml").write_text('<testsuite/>')
                def execute(command, **kwargs):
                    if "stdout" in kwargs:
                        kwargs["stdout"].write("resolved dependencies\n")
                    else:
                        variant = "release" if "-PconsumerBuildType=release" in command else "debug"
                        is_test = command[-1].endswith("AndroidTest")
                        if ("test" if is_test else "app") != missing:
                            path = f"apk/androidTest/{variant}/test.apk" if is_test else f"apk/{variant}/app.apk"
                            target = root / "build/outputs" / path
                            target.parent.mkdir(parents=True, exist_ok=True)
                            target.write_bytes(b"apk")
                        if variant == "release" and not is_test:
                            for name in ("mapping.txt", "configuration.txt", "usage.txt", "seeds.txt"):
                                if name != missing:
                                    target = root / "build/outputs/mapping/release" / name
                                    target.parent.mkdir(parents=True, exist_ok=True)
                                    target.write_text("R8 output")
                    return subprocess.CompletedProcess(command, 0)
                with patch.dict(os.environ, {"JAVA_HOME": directory, "ANDROID_HOME": directory}), \
                     patch.object(elf, "check_apk"), patch.object(elf.subprocess, "run", side_effect=execute):
                    with self.assertRaisesRegex(ValueError, "missing.*artifact"):
                        elf.assemble(root)

    def test_startup_capture_selects_first_consumer_exception(self):
        log = ("FATAL EXCEPTION: main\nProcess: unrelated.app, PID: 1\nOTHER\n"
               "FATAL EXCEPTION: main\nProcess: pics.lab.colors.consumer, PID: 2\nCAUSE_ONE\n"
               "FATAL EXCEPTION: main\nProcess: pics.lab.colors.consumer, PID: 3\nCAUSE_TWO\n")
        block = elf.CrashBlock()
        for line in log.splitlines(keepends=True):
            block.feed(line)
        self.assertIn("CAUSE_ONE", block.text)
        self.assertNotIn("OTHER", block.text)
        self.assertNotIn("CAUSE_TWO", block.text)
        self.assertFalse(block.truncated)
        block = elf.CrashBlock()
        block.feed("Process: unrelated.app, PID: 1\n")
        self.assertEqual(block.text, "")

    def test_connected_failure_is_preserved_when_capture_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            command = ["gradle", "connectedDebugAndroidTest"]
            for result_code in (0, 17):
                with self.subTest(result_code=result_code), redirect_stderr(io.StringIO()) as diagnostic, patch.object(elf.CrashCapture, "start"), patch.object(elf.CrashCapture, "close") as close, patch.object(elf.CrashCapture, "finish", side_effect=OSError("capture failure")), patch.object(elf.subprocess, "run", return_value=subprocess.CompletedProcess(command, result_code)):
                    if result_code:
                        with self.assertRaises(subprocess.CalledProcessError) as failure:
                            elf.connected_test(command, root, {}, Path("adb"), "debug")
                        self.assertEqual(failure.exception.returncode, result_code)
                        self.assertIsInstance(failure.exception.__cause__, OSError)
                    else:
                        with self.assertRaises(OSError):
                            elf.connected_test(command, root, {}, Path("adb"), "debug")
                    self.assertIn("No matching AndroidRuntime exception", diagnostic.getvalue())
                    self.assertIn("capture failure", diagnostic.getvalue())
                    evidence = root / "build/outputs/consumer-diagnostics/debug"
                    self.assertEqual(json.loads((evidence / "connected-exit.json").read_text())["returncode"], result_code)

    def test_connected_capture_has_scoped_commands_and_no_false_exception(self):
        with tempfile.TemporaryDirectory() as directory:
            capture = elf.CrashCapture(Path("adb"))
            with patch.object(elf.subprocess, "run") as clear, patch.object(elf.subprocess, "Popen", side_effect=OSError("no adb")) as spawn:
                with self.assertRaises(OSError):
                    capture.start()
                self.assertEqual(clear.call_args.args[0], ["adb", "logcat", "-b", "crash", "-c"])
                self.assertEqual(spawn.call_args.args[0], ["adb", "logcat", "-b", "crash", "-b", "main", "-v", "threadtime", "AndroidRuntime:E", "ConsumerCapture:E", "*:S"])
            capture = self.interval(Path(directory), "", 0)
            self.assertFalse(capture.block.found)


    def test_assemble_captures_each_resolved_runtime_configuration(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "gradle-path.txt").write_text("gradle")
            calls = []
            def execute(command, **kwargs):
                calls.append(command)
                if "stdout" in kwargs:
                    kwargs["stdout"].write("pics.lab:colors-android:1.0\nnet.java.dev.jna:jna:5.17\n")
                else:
                    emitted_artifacts(root, "release" if "-PconsumerBuildType=release" in command else "debug")
                return subprocess.CompletedProcess(command, 0)
            with patch.dict(os.environ, {"JAVA_HOME": directory, "ANDROID_HOME": directory}), \
                 patch.object(elf, "check_apk") as inspected, patch.object(elf.subprocess, "run", side_effect=execute):
                elf.assemble(root)
            self.assertEqual({call.args[0] for call in inspected.call_args_list},
                             {root / "build/outputs/apk" / variant / "app.apk" for variant in ("debug", "release")})
            self.assertEqual(len(calls), 9)
            linkage = calls[-1]
            self.assertEqual(linkage[:2], [sys.executable, str(ROOT / "desugar-linkage.py")])
            self.assertEqual(linkage[2:], [str(root / "build/outputs/apk/release/app.apk"),
                                           str(root / "build/outputs/apk/androidTest/release/test.apk")])
            self.assertTrue((root / "build/outputs/consumer-diagnostics/release/desugar-linkage.txt").is_file())
            for variant in ("debug", "release"):
                for configuration in (variant + "RuntimeClasspath", variant + "AndroidTestRuntimeClasspath"):
                    self.assertTrue((root / "build/outputs/consumer-diagnostics" / variant / (configuration + ".txt")).is_file())
                    self.assertTrue(any(command[-2:] == ["--configuration", configuration] for command in calls))

    def test_dependency_capture_failure_does_not_run_assemble(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "gradle-path.txt").write_text("gradle")
            with patch.dict(os.environ, {"JAVA_HOME": directory}), patch.object(elf.subprocess, "run",
                    side_effect=subprocess.CalledProcessError(23, ["gradle"])) as run:
                with self.assertRaises(subprocess.CalledProcessError) as failure:
                    elf.assemble(root)
                self.assertEqual(failure.exception.returncode, 23)
                self.assertEqual(run.call_count, 1)
                receipt = json.loads((root / "build/outputs/consumer-diagnostics/artifact-inventory.json").read_text())
                self.assertEqual(set(item["status"] for item in receipt["categories"].values()), {"not-built"})

    def test_stop_uses_prepared_gradle_and_current_dependency_cache(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "gradle-path.txt").write_text("prepared-gradle")
            with patch.dict(os.environ, {"JAVA_HOME": directory, "GRADLE_USER_HOME": "cached-home"}), \
                    patch.object(elf.subprocess, "run", return_value=subprocess.CompletedProcess(
                        ["prepared-gradle", "--stop"], 0, stdout="No Gradle daemons are running.\n")) as run:
                elf.stop(root)
            self.assertEqual(run.call_args.args[0], ["prepared-gradle", "--stop"])
            self.assertEqual(run.call_args.kwargs["cwd"], root)
            self.assertEqual(run.call_args.kwargs["env"]["GRADLE_USER_HOME"], "cached-home")
            self.assertEqual(run.call_args.kwargs["env"]["JAVA_HOME"], directory)
            self.assertEqual(run.call_args.kwargs["timeout"], 30)
            self.assertTrue(run.call_args.kwargs["text"])

    def test_stop_never_reports_success_after_cleanup_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with patch.dict(os.environ, {"JAVA_HOME": directory}), patch.object(elf.subprocess, "run") as run:
                with self.assertRaises(FileNotFoundError):
                    elf.stop(root)
                run.assert_not_called()
            (root / "gradle-path.txt").write_text("prepared-gradle")
            for result, error in (
                    (subprocess.CompletedProcess(["prepared-gradle", "--stop"], 23, stdout=""), subprocess.CalledProcessError),
                    (subprocess.CompletedProcess(["prepared-gradle", "--stop"], 0,
                         stdout="Unable to stop one of the daemons. The daemon may have crashed.\n"), RuntimeError)):
                with self.subTest(error=error.__name__), \
                        patch.dict(os.environ, {"JAVA_HOME": directory}), \
                        patch.object(elf.subprocess, "run", return_value=result):
                    with self.assertRaises(error):
                        elf.stop(root)
            with patch.dict(os.environ, {"JAVA_HOME": directory}), patch.object(elf.subprocess, "run",
                    side_effect=subprocess.TimeoutExpired(["prepared-gradle", "--stop"], 30)):
                with self.assertRaises(subprocess.TimeoutExpired):
                    elf.stop(root)

    def test_partial_assembly_failure_preserves_successful_app_stage(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "gradle-path.txt").write_text("gradle")
            def execute(command, **kwargs):
                if "stdout" in kwargs:
                    kwargs["stdout"].write("resolved\n")
                elif command[-1] == "assembleDebug":
                    target = root / "build/outputs/apk/debug/app.apk"
                    target.parent.mkdir(parents=True)
                    target.write_bytes(b"apk")
                else:
                    raise subprocess.CalledProcessError(29, command)
                return subprocess.CompletedProcess(command, 0)
            with patch.dict(os.environ, {"JAVA_HOME": directory}), patch.object(elf.subprocess, "run", side_effect=execute):
                with self.assertRaises(subprocess.CalledProcessError) as failure:
                    elf.assemble(root)
                self.assertEqual(failure.exception.returncode, 29)
            elf.inventory(root)
            receipt = json.loads((root / "build/outputs/consumer-diagnostics/artifact-inventory.json").read_text())
            self.assertEqual(receipt["categories"]["debug/app-apk"]["status"], "present")
            self.assertEqual(receipt["categories"]["debug/test-apk"]["status"], "not-built")
            self.assertEqual(receipt["categories"]["release/mapping.txt"]["status"], "not-built")

    def test_subscription_failure_never_triggers_test_and_stops_reader(self):
        with tempfile.TemporaryDirectory() as directory:
            process = PipeProcess()
            def execute(command, **kwargs):
                self.assertNotEqual(command, ["gradle"])
                if "shell" in command:
                    raise subprocess.CalledProcessError(31, command)
                return subprocess.CompletedProcess(command, 0)
            with patch.object(elf.subprocess, "Popen", return_value=process), patch.object(elf.subprocess, "run", side_effect=execute):
                with self.assertRaises(subprocess.CalledProcessError) as failure:
                    elf.connected_test(["gradle"], Path(directory), {}, Path("adb"), "debug")
                self.assertEqual(failure.exception.returncode, 31)
            self.assertTrue(process.waited)
            self.assertTrue(process.stdout.closed)
            self.assertFalse(any(thread.name == "consumer-crash-capture" for thread in threading.enumerate()))

    def test_inventory_readback_detects_artifact_removed_after_assembly(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for variant in ("debug", "release"):
                emitted_artifacts(root, variant)
            elf.inventory(root, {variant: {"app": "assembled", "test": "assembled"} for variant in ("debug", "release")})
            (root / "build/outputs/apk/androidTest/release/test.apk").unlink()
            with self.assertRaisesRegex(ValueError, "missing.*artifact"):
                elf.inventory(root)
            receipt = json.loads((root / "build/outputs/consumer-diagnostics/artifact-inventory.json").read_text())
            self.assertEqual(receipt["missing"], ["release/test-apk"])

    def test_archive_checks_bytes_inventory_and_path_before_install(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, permitted in [("repository/a.aar", True), ("../escape", False), ("repository/./a", False), ("/absolute", False)]:
                with self.subTest(name=name):
                    archive = root / "sdk.zip"
                    with zipfile.ZipFile(archive, "w") as bundle:
                        bundle.writestr(name, b"verified bytes")
                    manifest = {"archiveSha256": consumer.digest(archive),
                                "files": {name: hashlib.sha256(b"verified bytes").hexdigest()}}
                    target = root / "consumer"
                    if permitted:
                        consumer.extract(archive, manifest, target)
                        self.assertEqual((target / name).read_bytes(), b"verified bytes")
                        manifest["files"][name] = "0" * 64
                        with self.assertRaisesRegex(ValueError, "checksum"):
                            consumer.extract(archive, manifest, root / "changed")
                    else:
                        with self.assertRaisesRegex(ValueError, "path"):
                            consumer.extract(archive, manifest, target)

    def test_android_aggregate_requires_all_finished_consumers(self):
        workflow = (ROOT.parents[2] / ".github/workflows/native-conformance-worker.yml").read_text()
        aggregate = workflow.split("  android-package:\n", 1)[1]
        self.assertIn("needs: [android-build, android-consumer]", aggregate)
        self.assertIn("if: always()", aggregate)
        self.assertIn("BUILD_RESULT: ${{ needs.android-build.result }}", aggregate)
        self.assertIn("CONSUMER_RESULT: ${{ needs.android-consumer.result }}", aggregate)
        script = "\n".join(line[10:] for line in aggregate.split("        run: |\n", 1)[1].splitlines())
        good = {"BUILD_RESULT": "success", "CONSUMER_RESULT": "success"}
        environment = {key: value for key, value in os.environ.items() if key not in (*good, "BASH_ENV", "ENV")}
        def execute(values):
            return subprocess.run(["bash", "--noprofile", "--norc", "-c", script],
                                  env=environment | values, capture_output=True, check=False, timeout=5)
        self.assertEqual(execute(good).returncode, 0)
        for key in good:
            for result in ["failure", "cancelled", "skipped", "neutral", "in_progress", ""]:
                with self.subTest(key=key, result=result):
                    self.assertNotEqual(execute(good | {key: result}).returncode, 0)
            self.assertNotEqual(execute({name: value for name, value in good.items() if name != key}).returncode, 0)

    def test_elf_admits_aligned_native_slices_and_rejects_wrong_abi_or_alignment(self):
        for abi, (word, machine) in elf.MACHINES.items():
            with self.subTest(abi=abi):
                data = bytearray(256)
                data[:6] = b"\x7fELF" + bytes((word, 1))
                struct.pack_into("<H", data, 18, machine)
                struct.pack_into("<Q" if word == 2 else "<I", data, 32 if word == 2 else 28, 64)
                struct.pack_into("<HH", data, 54 if word == 2 else 42, 56 if word == 2 else 32, 1)
                struct.pack_into("<I", data, 64, 1)
                if word == 2:
                    struct.pack_into("<QQQ", data, 96, 128, 128, 16384)
                else:
                    struct.pack_into("<II", data, 80, 128, 128)
                    struct.pack_into("<I", data, 92, 16384)
                elf.check_elf(data, abi)
                malformed = data.copy()
                struct.pack_into("<H", malformed, 18, 0)
                with self.assertRaisesRegex(ValueError, "ABI"):
                    elf.check_elf(malformed, abi)
                struct.pack_into("<Q" if word == 2 else "<I", data, 112 if word == 2 else 92, 2048)
                with self.assertRaisesRegex(ValueError, "page size"):
                    elf.check_elf(data, abi)
                struct.pack_into("<Q" if word == 2 else "<I", data, 112 if word == 2 else 92, 4096)
                if word == 2:
                    with self.assertRaisesRegex(ValueError, "page size"):
                        elf.check_elf(data, abi)
                else:
                    elf.check_elf(data, abi)
                if word == 2:
                    # LOAD remains valid while a separate RELRO end changes.
                    struct.pack_into("<Q", data, 112, 16384)
                    struct.pack_into("<H", data, 56, 2)
                    struct.pack_into("<I", data, 120, 0x6474E552)
                    struct.pack_into("<Q", data, 136, 0x3F00)
                    struct.pack_into("<Q", data, 160, 0x100)
                    elf.check_elf(data, abi)
                    struct.pack_into("<Q", data, 160, 0x1100)
                    with self.assertRaisesRegex(ValueError, "RELRO"):
                        elf.check_elf(data, abi)


if __name__ == "__main__":
    unittest.main()
