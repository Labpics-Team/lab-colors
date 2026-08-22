#!/usr/bin/env python3
"""A skip decorator that slipped off its test case, caught as source.

`unittest.skip*` only means something on a test: on a test case it gates the
case, and on a plain module-level function it turns every call into a raised
`SkipTest`.  So when an extraction inserts a helper between a module's env
gate and the class it guarded, two defects appear at once — the helper
becomes uncallable, and the native test loses the gate that kept it out of a
run that cannot host it.

Neither defect is visible to a syntax check, and the long native lanes live
outside the fast `test_*.py` inventory, so nothing else in the tree would
notice until a dispatched job failed an hour in.  This gate reads the tree as
source, like the arity gate next to it, so it also covers the modules that
refuse to import on this platform.
"""

from __future__ import annotations

import ast
import unittest
from pathlib import Path

PROOF = Path(__file__).resolve().parents[1]
SKIP_DECORATORS_V1 = frozenset({"skip", "skipIf", "skipUnless"})


def _sources_v1() -> list[Path]:
    return sorted(
        path for path in PROOF.rglob("*.py") if "__pycache__" not in path.parts
    )


def _is_unittest_skip_v1(decorator: ast.expr) -> bool:
    """True for `@unittest.skip*` and for a bare imported `@skipUnless`."""

    target = decorator.func if isinstance(decorator, ast.Call) else decorator
    if isinstance(target, ast.Attribute):
        return (
            target.attr in SKIP_DECORATORS_V1
            and isinstance(target.value, ast.Name)
            and target.value.id == "unittest"
        )
    return isinstance(target, ast.Name) and target.id in SKIP_DECORATORS_V1


class SkipDecoratorPlacementTests(unittest.TestCase):
    def test_no_module_level_function_carries_a_skip_decorator(self) -> None:
        offenders = []
        for path in _sources_v1():
            tree = ast.parse(path.read_text("utf-8"), str(path))
            # Only module level: inside a class body a decorated function is
            # a test method, where skipping is exactly the intended meaning.
            for node in tree.body:
                if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    continue
                for decorator in node.decorator_list:
                    if _is_unittest_skip_v1(decorator):
                        offenders.append(
                            f"{path.relative_to(PROOF)}:{node.lineno} {node.name}"
                        )
        self.assertEqual(offenders, [], f"skip decorator on a non-test: {offenders}")

    def test_the_gate_sees_the_defect_it_exists_for(self) -> None:
        # The exact shape that slipped through: an extracted helper placed
        # between the env gate and the class it was written to guard.
        defect = ast.parse(
            "import unittest\n"
            "@unittest.skipUnless(True, 'reason')\n"
            "def seal_v1():\n"
            "    return 1\n"
            "class T(unittest.TestCase):\n"
            "    pass\n"
        )
        functions = [
            node
            for node in defect.body
            if isinstance(node, ast.FunctionDef)
            and any(_is_unittest_skip_v1(item) for item in node.decorator_list)
        ]
        self.assertEqual([node.name for node in functions], ["seal_v1"])

        healthy = ast.parse(
            "import unittest\n"
            "def seal_v1():\n"
            "    return 1\n"
            "@unittest.skipUnless(True, 'reason')\n"
            "class T(unittest.TestCase):\n"
            "    @unittest.skipIf(False, 'reason')\n"
            "    def test_x(self):\n"
            "        pass\n"
        )
        self.assertEqual(
            [
                node.name
                for node in healthy.body
                if isinstance(node, ast.FunctionDef)
                and any(_is_unittest_skip_v1(item) for item in node.decorator_list)
            ],
            [],
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
