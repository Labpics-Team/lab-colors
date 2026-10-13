"""Проверка опубликованного текста и доменного разделителя протокола."""

import ast
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]
PROTOCOL = ROOT / "proof/region/v1/PROTOCOL.md"
SOURCE = ROOT / "proof/region/v1/region_proof_protocol.py"
TEXT_EXTENSIONS = {".md", ".mdx", ".txt", ".rst"}
ALLOWED_CONTROLS = {9, 10, 13}


def control_at(data: bytes):
    return next(
        ((index, value) for index, value in enumerate(data)
         if (value < 32 and value not in ALLOWED_CONTROLS) or value == 127),
        None,
    )


def tracked_documents():
    result = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=ROOT,
        capture_output=True,
        check=True,
    )
    return sorted(
        ROOT / Path(raw.decode("utf-8", "strict"))
        for raw in result.stdout.split(b"\0")
        if raw and Path(raw.decode("utf-8", "strict")).suffix.lower() in TEXT_EXTENSIONS
    )


class DocumentBytesTests(unittest.TestCase):
    def test_control_bytes_refuse_without_rejecting_normal_whitespace(self):
        for value in (0, 1, 8, 11, 12, 14, 31, 127):
            with self.subTest(value=value):
                self.assertEqual(control_at(b"before" + bytes([value]) + b"after"),
                                 (6, value))
        self.assertIsNone(control_at(b"tab\tline\r\nUTF-8: " + "цвет".encode()))

    def test_tracked_documents_are_utf8_without_hidden_controls(self):
        paths = tracked_documents()
        self.assertIn(PROTOCOL, paths)
        for path in paths:
            with self.subTest(path=path.relative_to(ROOT).as_posix()):
                self.assertFalse(path.is_symlink(), "tracked document is a symlink")
                data = path.read_bytes()
                self.assertIsNone(control_at(data), "hidden control byte in tracked document")
                data.decode("utf-8", "strict")

    def test_documented_domain_separator_matches_protocol_source(self):
        module = ast.parse(SOURCE.read_text(encoding="utf-8"))
        labels = [
            node.value for node in module.body
            if isinstance(node, ast.Assign)
            and any(isinstance(name, ast.Name)
                    and name.id == "COMPARATOR_SOURCE_ID_LABEL_V2" for name in node.targets)
        ]
        self.assertEqual(len(labels), 1)
        label = ast.literal_eval(labels[0])
        self.assertIsInstance(label, bytes)
        self.assertTrue(label.endswith(b"\0"))
        expected = 'SHA256("' + label[:-1].decode("ascii") + r'\x00" ||'
        self.assertIn(expected, PROTOCOL.read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
