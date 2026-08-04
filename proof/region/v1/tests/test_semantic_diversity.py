#!/usr/bin/env python3
"""Hostile contract for third-verifier diversity and admission shapes."""

from __future__ import annotations

import ast
import hashlib
import re
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from semantic.receipt import (  # noqa: E402
    SemanticVerificationReceiptV1,
)


SEMANTIC_SOURCES = tuple(sorted((ROOT / "semantic").glob("*.py")))
FORBIDDEN_MODULE_ROOTS = ("arb", "mpfi", "build", "executor", "provenance")
FORBIDDEN_PATH_HINTS = (
    re.compile(r"\barb[/\\]\w"),
    re.compile(r"\bmpfi[/\\]\w"),
    re.compile(r"\bevaluator\b"),
)


def _imported_roots(tree: ast.AST) -> set[str]:
    roots: set[str] = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            roots.update(alias.name.split(".")[0] for alias in node.names)
        elif isinstance(node, ast.ImportFrom) and node.module:
            roots.add(node.module.split(".")[0])
    return roots


class DiversityBoundaryTests(unittest.TestCase):
    def test_semantic_sources_exist(self) -> None:
        names = {path.name for path in SEMANTIC_SOURCES}
        self.assertIn("__init__.py", names)
        self.assertIn("receipt.py", names)
        self.assertIn("verifier.py", names)

    def test_semantic_package_imports_neither_evaluator_path(self) -> None:
        # The third verifier may inherit only the canonical protocol and the
        # standard library; any Arb/MPFI/pipeline import destroys diversity.
        for path in SEMANTIC_SOURCES:
            source = path.read_text(encoding="utf-8")
            tree = ast.parse(source)
            roots = _imported_roots(tree)
            for root in FORBIDDEN_MODULE_ROOTS:
                self.assertNotIn(
                    root,
                    roots,
                    f"{path.name} imports forbidden module root {root}",
                )
            self.assertNotIn("__import__", source, f"{path.name} hides dynamic imports")
            if path.name in ("__init__.py", "intervalmath.py"):
                # The facade only re-exports the verifier boundary; the
                # interval kernel is pure mathematics with no wire surface.
                continue
            self.assertIn("region_proof_protocol", roots, f"{path.name} lost the protocol binding")

    def test_semantic_package_never_reads_evaluator_artifacts(self) -> None:
        for path in SEMANTIC_SOURCES:
            source = path.read_text(encoding="utf-8")
            for pattern in FORBIDDEN_PATH_HINTS:
                self.assertIsNone(
                    pattern.search(source),
                    f"{path.name} references evaluator artifacts",
                )


def digest(label: int) -> bytes:
    return hashlib.sha256(f"semantic-diversity-{label}".encode("ascii")).digest()


class DegenerateVerifierShapeTests(unittest.TestCase):
    """Hash-compare, no-op and saturate-all shapes must not mint receipts."""

    def _hash_comparer_receipt(self) -> SemanticVerificationReceiptV1:
        return SemanticVerificationReceiptV1(
            digest(1), digest(2), digest(3), digest(4), digest(5)
        )

    def _no_op_receipt(self) -> SemanticVerificationReceiptV1:
        return SemanticVerificationReceiptV1(
            digest(6), digest(7), digest(8), digest(9), digest(10)
        )

    def _saturate_all_receipt(self) -> SemanticVerificationReceiptV1:
        return SemanticVerificationReceiptV1(
            digest(11), digest(12), digest(13), digest(14), digest(15)
        )

    def test_degenerate_shapes_cannot_create_receipts(self) -> None:
        for shape in (
            self._hash_comparer_receipt,
            self._no_op_receipt,
            self._saturate_all_receipt,
        ):
            with self.assertRaises(TypeError):
                shape()

    def test_foreign_token_cannot_open_the_seal(self) -> None:
        with self.assertRaises(TypeError):
            SemanticVerificationReceiptV1(
                digest(1), digest(2), digest(3), digest(4), digest(5),
                _token=object(),
            )
        with self.assertRaises(TypeError):
            SemanticVerificationReceiptV1(
                digest(1), digest(2), digest(3), digest(4), digest(5),
                _token="verifier",
            )


if __name__ == "__main__":
    unittest.main()
