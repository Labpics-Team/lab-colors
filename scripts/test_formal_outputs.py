"""Потери, подмена и пределы передачи формальных JSON-отчётов."""
import base64
import importlib.util
import json
import os
from pathlib import Path
import random
import tempfile
import unittest
import zlib

SPEC = importlib.util.spec_from_file_location("formal_outputs", Path(__file__).parent / "ci/formal_outputs.py")
assert SPEC is not None and SPEC.loader is not None
OUTPUTS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(OUTPUTS)


def message(value):
    return base64.b64encode(zlib.compress(json.dumps(value).encode())).decode()


class FormalOutputTests(unittest.TestCase):
    def test_json_report_bytes_survive_generated_messages(self):
        rng = random.Random(20261013)
        for count in range(1, 35):
            files = {f"positive-{index}.json": json.dumps({
                "index": index, "passed": bool(rng.randrange(2)),
                "text": "Цвет: " + "".join(chr(rng.randrange(32, 127)) for _ in range(index * 9)),
            }, ensure_ascii=False, indent=2) + "\n" for index in range(count)}
            self.assertEqual(files, OUTPUTS.decode(OUTPUTS.encode(files)))

    def test_empty_malformed_duplicate_and_external_names_refuse(self):
        for files in [{}, {"../outside.json": "{}"}, {"/absolute.json": "{}"},
                      {"valid.json": False}, {"valid.json": "not JSON"},
                      {"valid.json": '{"a":1,"a":2}'}, {"valid.txt": "{}"}]:
            with self.subTest(files=files), self.assertRaises(ValueError):
                OUTPUTS.encode(files)
        for envelope in [{}, {"version": True,"files":{"valid.json":"{}"}},
                         {"version":2,"files":{"valid.json":"{}"}},
                         {"version":1,"files":{"valid.json":"{}"},"extra":1}]:
            with self.subTest(envelope=envelope), self.assertRaises(ValueError):
                OUTPUTS.decode(message(envelope))
        repeated = b'{"version":1,"files":{"valid.json":"{}","valid.json":"[]"}}'
        with self.assertRaises(ValueError):
            OUTPUTS.decode(base64.b64encode(zlib.compress(repeated)).decode())

    def test_truncation_corruption_and_trailing_streams_refuse(self):
        valid = OUTPUTS.encode({"valid.json":"{}"})
        raw = base64.b64decode(valid)
        for value in ["", valid[:-4], "!" + valid, valid + "=",
                      base64.b64encode(raw[:-1]).decode(),
                      base64.b64encode(raw + raw).decode()]:
            with self.subTest(value=value[:30]), self.assertRaises((ValueError,zlib.error)):
                OUTPUTS.decode(value)

    def test_output_and_expansion_limits_do_not_allow_partial_results(self):
        with self.assertRaises(ValueError):
            OUTPUTS.decode("a" * (OUTPUTS.MAX_ENCODED + 1))
        bomb = base64.b64encode(zlib.compress(b" " * (OUTPUTS.MAX_DECODED + 1))).decode()
        with self.assertRaises(ValueError):
            OUTPUTS.decode(bomb)
        with self.assertRaises(ValueError):
            OUTPUTS.encode({"valid.json": json.dumps("x" * OUTPUTS.MAX_DECODED)})

    def test_colliding_parts_leave_no_destination(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary)/"reports"
            part = OUTPUTS.encode({"valid.json":"{}"})
            with self.assertRaises(ValueError):
                OUTPUTS.restore(destination,[part,part])
            self.assertFalse(destination.exists())
            OUTPUTS.restore(destination,[part])
            self.assertEqual("{}", (destination/"valid.json").read_text())
            with self.assertRaises(FileExistsError):
                OUTPUTS.restore(destination,[part])

    def test_collection_follows_the_fragment_and_preserves_declared_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            fragment={"kind":"positive","file":"positive-0.json","shard_index":0,"shard_count":5}
            (root/"fragment-positive-0.json").write_text(json.dumps(fragment))
            (root/"positive-0.json").write_text('{"checked":true}\n')
            (root/"unused.log").write_text("diagnostic already in job log")
            files=OUTPUTS.collect(root)
            self.assertEqual({"fragment-positive-0.json","positive-0.json"},set(files))
            (root/"positive-0.json").unlink()
            with self.assertRaises(FileNotFoundError):
                OUTPUTS.collect(root)
            (root/"positive-0.json").symlink_to(root/"unused.log")
            with self.assertRaises(OSError):
                OUTPUTS.collect(root)
            (root/"positive-0.json").unlink()
            os.mkfifo(root/"positive-0.json")
            with self.assertRaises(ValueError):
                OUTPUTS.collect(root)

    def test_mutant_manifest_selects_every_named_negative_report(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            fragment={"kind":"mutants","shard_index":0,"shard_count":5,
                      "negative_sha256":{"one":"0"*64,"two":"1"*64}}
            (root/"fragment-mutants-0.json").write_text(json.dumps(fragment))
            for name in ["one","two"]:
                (root/("negative-"+name+".json")).write_text("{}\n")
            self.assertEqual({"fragment-mutants-0.json","negative-one.json","negative-two.json"},
                             set(OUTPUTS.collect(root)))
            (root/"negative-two.json").unlink()
            with self.assertRaises(FileNotFoundError):
                OUTPUTS.collect(root)


if __name__ == "__main__":
    unittest.main()
