"""Проверка опубликованного текста и доменного разделителя протокола."""

import ast
import re
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


def recovered_cyrillic_comment(line: str) -> str | None:
    """Recognize exactly reversible UTF-8 misread as Windows-1251."""
    signatures = ("Рґ", "Рµ", "СЃ", "Рѕ", "Рё", "Р°", "вЂ", "Рї", "С‚", "Рљ", "Рџ", "РЎ", "Р\x98")
    if not any(signature in line for signature in signatures):
        return None
    try:
        raw = bytearray()
        for character in line:
            ordinal = ord(character)
            if 128 <= ordinal <= 159:
                raw.append(ordinal)
            else:
                raw.extend(character.encode("cp1251"))
        text = raw.decode("utf-8", "strict")
    except (UnicodeEncodeError, UnicodeDecodeError):
        return None
    return text if text != line else None


def work_record_reason(path: Path, text: str) -> str | None:
    """Reject agent work records, not product protocols or reproducible test data."""
    # Only repository work roots have this meaning. Product reference topics
    # may legitimately use the same word at any deeper level.
    if len(path.parts) > 1 and path.parts[0].casefold() in {"plans", "handoffs", "worklogs"}:
        return "agent work directory"
    # A plan remains a plan after a rename. Recognize its owned schema rather
    # than a list of old filenames or ordinary words such as 'dependency plan'.
    if text.startswith("---\n"):
        frontmatter = text.split("\n---", 1)[0]
        keys = set(re.findall(r"^([a-z][a-z_-]*):", frontmatter, re.MULTILINE))
        if {"track", "revision", "owns"} <= keys:
            return "agent plan frontmatter"
    fields = {
        key.strip().casefold()
        for key in re.findall(r"^\*\*([^*\n]+):\*\*", text, re.MULTILINE)
    }
    if {"status", "branch"} <= fields or {"baseline sha", "head at validation"} <= fields:
        return "work-session status or review report"
    return None


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

    def test_rust_comments_are_readable_utf8_without_double_encoding(self):
        output = subprocess.run(
            ["git", "ls-files", "-z", "--", "crates"],
            cwd=ROOT, check=True, capture_output=True,
        )
        filenames = [
            ROOT / Path(raw.decode("utf-8", "strict"))
            for raw in output.stdout.split(b"\0")
            if raw and Path(raw.decode("utf-8", "strict")).suffix == ".rs"
        ]
        self.assertTrue(filenames)
        for path in filenames:
            with self.subTest(path=path.relative_to(ROOT).as_posix()):
                self.assertFalse(path.is_symlink())
                for index, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
                    if not line.lstrip().startswith(("//", "*")):
                        continue
                    self.assertFalse(
                        any(128 <= ord(character) <= 159 for character in line),
                        f"control character in Rust comment at line {index}",
                    )
                    self.assertIsNone(
                        recovered_cyrillic_comment(line),
                        f"double-encoded Rust comment at line {index}",
                    )

    def test_mojibake_detector_distinguishes_corruption_from_valid_cyrillic(self):
        readable = "/// Строковая проекция принадлежит автору."
        corrupted = readable.encode("utf-8").decode("cp1251")
        self.assertEqual(recovered_cyrillic_comment(corrupted), readable)
        self.assertIsNone(recovered_cyrillic_comment(readable))
        self.assertIsNone(recovered_cyrillic_comment("/// Рёбра задают порядок вычислений."))

    def test_product_documents_exclude_agent_work_records(self):
        failures = []
        for path in tracked_documents():
            if path.suffix.lower() not in {".md", ".mdx", ".rst"}:
                continue
            relative = path.relative_to(ROOT)
            reason = work_record_reason(relative, path.read_text(encoding="utf-8-sig"))
            if reason:
                failures.append(f"{relative.as_posix()}: {reason}")
        self.assertEqual(failures, [], "agent plans and run reports do not belong to the product")

    def test_work_record_check_distinguishes_product_contracts(self):
        rejected = (
            ("docs/innocent-name.md", "---\ntrack: colors\nrevision: 9\nowns: [core]\n---\n# Work"),
            ("docs/reference.md", "# Projection\n**Status:** COMPLETE\n**Branch:** work/example"),
            ("docs/check.md", "# Review\n**Baseline SHA:** aaaa\n**HEAD at validation:** bbbb"),
            ("plans/next.md", "# Next implementation steps"),
        )
        for name, text in rejected:
            with self.subTest(path=name):
                self.assertIsNotNone(work_record_reason(Path(name), text))
        allowed = (
            ("docs/contracts/dependency-plan.md", "# Compiled dependency plan\nA plan is a runtime graph."),
            ("docs/contracts/plans/dependency-plan.md", "# Compiled dependency plan\nA plan is a runtime graph."),
            ("docs/contracts/handoffs/transport.md", "# Handoff transport\nThe envelope carries a sequence number."),
            ("docs/decisions/0010-solver.md", "# Solver\n**Status:** Accepted\n## Decision"),
            ("proof/region/v1/PROTOCOL.md", "# Protocol\nThe status field identifies a terminal outcome."),
            ("crates/labcolors-core/tests/data/README.md", "# Measured fixtures\nSource snapshot: 2026-07-02."),
        )
        for name, text in allowed:
            with self.subTest(path=name):
                self.assertIsNone(work_record_reason(Path(name), text))

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
