#!/usr/bin/env python3
"""Controls for artifact substitution and invalid cross-capability comparisons."""

import ast
import copy
import hashlib
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("compare-program-performance.py")
SOURCE_REVISION = "c" * 40
PRODUCER_REVISION = "d" * 40


class ProgramPerformanceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Load the exact pure helpers without running CLI, Git checks or a benchmark.
        tree = ast.parse(SCRIPT.read_text(), filename=str(SCRIPT))
        helpers = ast.Module(body=[node for node in tree.body
                                   if isinstance(node, (ast.Import, ast.ImportFrom, ast.FunctionDef))],
                             type_ignores=[])
        cls.namespace = {}
        exec(compile(helpers, str(SCRIPT), "exec"), cls.namespace)
        cls.namespace.update(roots={"baseline": None, "candidate": None},
                             candidate_only=("v3-gray", "v3-limit"))

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "package"
        self.root.mkdir()
        self.wasm = b"\0asm\x01\0\0\0"
        self.wasm_digest = hashlib.sha256(self.wasm).hexdigest()
        files = {
            "index.js": b"export default async function init() {}\n",
            "program-wire/abi-v1.js": b"export const version = 1;\n",
            "program-wire/abi-v3.js": b"export const version = 3;\n",
            "pkg/labcolors_bg.wasm": self.wasm,
            "build-metadata.json": json.dumps({
                "sourceSha": SOURCE_REVISION,
                "wasm": [{"role": "runtime", "path": "pkg/labcolors_bg.wasm",
                          "bytes": len(self.wasm), "sha256": self.wasm_digest}],
            }).encode(),
        }
        for name, data in files.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        self.tarball = Path(self.temporary.name) / "verified.tgz"
        self.pack()

    def pack(self, extra=None):
        with tarfile.open(self.tarball, "w:gz") as archive:
            for path in sorted(self.root.rglob("*")):
                if path.is_file():
                    archive.add(path, arcname="package/" + path.relative_to(self.root).as_posix())
            if extra is not None:
                info, data = extra
                archive.addfile(info, io.BytesIO(data) if data is not None else None)
        self.archive_digest = hashlib.sha256(self.tarball.read_bytes()).hexdigest()

    def bind(self):
        return self.namespace["verified_candidate"](self.root, self.tarball,
                                                     self.archive_digest, SOURCE_REVISION)

    def measurement(self, pattern="v3-gray", nodes=1, checksum=100):
        sample = {"compileMs": 2, "cycleMs": 4, "updatesMs": [.01] * 64, "checksum": checksum}
        return {"pattern": pattern, "nodes": nodes, "samples": 3,
                "wasmSha256": self.wasm_digest, "wasmBytes": len(self.wasm),
                "producer": PRODUCER_REVISION, "initializationMs": 1,
                "maxRssKiB": 1024, "cold": copy.deepcopy(sample),
                "samplesAfterWarmup": [copy.deepcopy(sample) for _ in range(3)]}

    def validate(self, value):
        return self.namespace["measurement_output"](json.dumps(value), "v3-gray", 1,
                                                     self.wasm_digest, len(self.wasm), PRODUCER_REVISION)

    def records(self, pattern, nodes, label, measurement, statuses=("completed", "completed")):
        return [{"pattern": pattern, "nodes": nodes, "variant": label, "round": index,
                 "status": status, **({"measurement": copy.deepcopy(measurement)}
                                         if status == "completed" else {})}
                for index, status in enumerate(statuses)]

    def test_verified_archive_and_extracted_package_are_one_input(self):
        evidence = self.bind()
        self.assertEqual(evidence["kind"], "verified-npm-tarball")
        self.assertEqual(evidence["sourceRevision"], SOURCE_REVISION)
        self.assertEqual(evidence["tarballSha256"], self.archive_digest)
        self.assertEqual(self.validate(self.measurement())["producer"], PRODUCER_REVISION)

    def test_archive_substitution_and_unpacked_drift_are_rejected(self):
        original = self.tarball.read_bytes()
        self.tarball.write_bytes(original + b"changed")
        with self.assertRaisesRegex(ValueError, "SHA-256"):
            self.bind()
        self.tarball.write_bytes(original)
        (self.root / "index.js").write_bytes(b"substituted implementation")
        with self.assertRaisesRegex(ValueError, "differs from archive"):
            self.bind()

    def test_extra_extracted_files_are_rejected(self):
        (self.root / "unpacked-only.js").write_text("unbound implementation")
        with self.assertRaisesRegex(ValueError, "inventory"):
            self.bind()

    def test_packed_source_and_wasm_metadata_must_match(self):
        path = self.root / "build-metadata.json"
        original = json.loads(path.read_text())
        for fault in ("sourceSha", "wasm"):
            with self.subTest(fault=fault):
                changed = copy.deepcopy(original)
                if fault == "sourceSha":
                    changed["sourceSha"] = "a" * 40
                else:
                    changed["wasm"][0]["sha256"] = "0" * 64
                path.write_text(json.dumps(changed))
                self.pack()
                with self.assertRaises(ValueError):
                    self.bind()

    def test_unsafe_link_and_duplicate_archive_members_are_rejected(self):
        for name, kind in (("package/../outside", tarfile.REGTYPE),
                           ("package/link", tarfile.SYMTYPE),
                           ("package/index.js", tarfile.REGTYPE)):
            with self.subTest(name=name):
                info = tarfile.TarInfo(name)
                info.type = kind
                info.linkname = "../../outside" if kind == tarfile.SYMTYPE else ""
                info.size = 0 if kind == tarfile.SYMTYPE else 1
                self.pack((info, None if kind == tarfile.SYMTYPE else b"x"))
                with self.assertRaises(ValueError):
                    self.bind()

    def test_wrong_binary_runtime_or_workload_identity_is_rejected(self):
        for key, value in (("wasmSha256", "0" * 64), ("wasmBytes", 9),
                           ("producer", SOURCE_REVISION), ("pattern", "repeated"),
                           ("nodes", True), ("samples", 2)):
            with self.subTest(key=key):
                changed = self.measurement()
                changed[key] = value
                with self.assertRaises(ValueError):
                    self.validate(changed)

    def test_invalid_and_partial_measurements_are_not_completed_samples(self):
        for fault in ("nan", "infinity", "negative", "overflow", "missing-update", "missing-warm", "checksum-drift"):
            with self.subTest(fault=fault):
                changed = self.measurement()
                if fault in ("nan", "infinity", "negative", "overflow"):
                    changed["cold"]["compileMs"] = {"nan": float("nan"), "infinity": float("inf"),
                                                     "negative": -1, "overflow": 10 ** 400}[fault]
                elif fault == "missing-update":
                    changed["cold"]["updatesMs"].pop()
                elif fault == "missing-warm":
                    changed["samplesAfterWarmup"].pop()
                else:
                    changed["cold"]["checksum"] += 1
                with self.assertRaises(ValueError):
                    self.validate(changed)

    def test_v3_has_no_baseline_or_speedup_even_if_unrelated_data_exist(self):
        for pattern in ("v3-gray", "v3-limit"):
            with self.subTest(pattern=pattern):
                self.assertEqual(self.namespace["variants"](pattern), ("candidate",))
                candidate = self.measurement(pattern=pattern)
                records = self.records(pattern, 1, "candidate", candidate)
                records += self.records(pattern, 1, "baseline", self.measurement(pattern=pattern, checksum=999))
                row = self.namespace["summary_row"](pattern, 1, records, ("candidate",))
                self.assertEqual(row["comparison"], "candidate-only")
                self.assertNotIn("baseline", row)
                self.assertFalse(any("Speedup" in key for key in row))
                self.assertNotIn("comparisonError", row)

    def test_v1_pair_preserves_speedup_and_rejects_changed_outputs(self):
        self.assertEqual(self.namespace["variants"]("repeated"), ("baseline", "candidate"))
        candidate = self.measurement(pattern="repeated", nodes=8)
        baseline = copy.deepcopy(candidate)
        for sample in [baseline["cold"], *baseline["samplesAfterWarmup"]]:
            sample["compileMs"] *= 2
            sample["cycleMs"] *= 2
        records = self.records("repeated", 8, "baseline", baseline) + self.records("repeated", 8, "candidate", candidate)
        row = self.namespace["summary_row"]("repeated", 8, records, ("baseline", "candidate"))
        self.assertEqual(row["compileSpeedup"], 2)
        self.assertEqual(row["cycleSpeedup"], 2)
        for record in records[:2]:
            measurement = record["measurement"]
            for sample in [measurement["cold"], *measurement["samplesAfterWarmup"]]:
                sample["checksum"] += 1
        rejected = self.namespace["summary_row"]("repeated", 8, records, ("baseline", "candidate"))
        self.assertIn("comparisonError", rejected)
        self.assertNotIn("compileSpeedup", rejected)
        self.assertNotIn("cycleSpeedup", rejected)

    def test_timeout_budget_and_errors_remain_visible_without_latency_claims(self):
        for status in ("timeout", "budget-exhausted", "failed", "invalid-output"):
            with self.subTest(status=status):
                records = self.records("v3-limit", 1, "candidate", self.measurement(pattern="v3-limit"),
                                       ("completed", status))
                row = self.namespace["summary_row"]("v3-limit", 1, records, ("candidate",))
                self.assertEqual(row["candidate"]["statuses"], ["completed", status])
                self.assertNotIn("compileMedianMs", row["candidate"])


if __name__ == "__main__":
    unittest.main()
