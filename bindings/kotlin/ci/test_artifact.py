"""Контрольные примеры для установки архива и проверки Android ELF."""
import hashlib
import os
import subprocess
import importlib.util
from pathlib import Path
import struct
import shutil
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parent


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


consumer = load("prepare-consumer")
elf = load("test-consumer")


class ArtifactTests(unittest.TestCase):
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

    def test_consumer_reports_require_current_tests_and_reject_partial_success(self):
        with tempfile.TemporaryDirectory() as directory:
            reports = Path(directory) / "reports"
            reports.mkdir()
            report = reports / "TEST-consumer.xml"
            report.write_text('<testsuite tests="2" failures="0"><testcase name="app"/><testcase name="page"/></testsuite>')
            elf.verify_reports(reports, {"app", "page"})
            # Same reset used between variants: no prior debug result remains.
            shutil.rmtree(reports)
            with self.assertRaisesRegex(ValueError, "required tests"):
                elf.verify_reports(reports, {"app", "page"})
            reports.mkdir()
            for body in [
                '<testsuite><testcase name="app"/></testsuite>',
                '<testsuite failures="1"><testcase name="app"/><testcase name="page"/></testsuite>',
                '<testsuite><testcase name="app"/><testcase name="page"><skipped/></testcase></testsuite>',
            ]:
                report.write_text(body)
                with self.assertRaises(ValueError):
                    elf.verify_reports(reports, {"app", "page"})

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
