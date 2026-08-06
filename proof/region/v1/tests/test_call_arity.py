#!/usr/bin/env python3
"""Signature drift inside the proof tree, checked without importing it.

A changed signature is only caught by a test that actually runs, and part of
this tree cannot be imported on every platform: the pipeline harness needs
Unix-only modules, so `test_corpus_shards.py` and its neighbours silently
vanish from a Windows run.  A stale call in one of them stays green locally
and fails only in CI.

This gate closes that class by reading the tree as source instead of
importing it, so it runs everywhere — including where the modules themselves
refuse to load.

Resolution is exact rather than by bare name: a call is only checked when its
target is unambiguous, either `module.function(...)` through an alias bound by
a real `import` of a tree module, or `function(...)` bound by a real
`from <tree module> import function`.  Anything else — a method, a local, a
callable attribute — is skipped rather than guessed at, so the gate never
reports a call it cannot resolve.  It proves arity only, not types.

Modules are keyed by their dotted path, not by file name: the tree holds ten
colliding stems (`receipt.py`, `formula.py`, `gate.py` and friends live under
both `arb/` and `mpfi/`), and keying by stem would silently match a call
against the other engine's signatures — a gate that answers plausibly and
wrongly. A bare stem resolves only while it is unique across the tree.
"""

from __future__ import annotations

import ast
import sys
import unittest
from pathlib import Path

PROOF = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PROOF))

Signature = tuple[int, int, bool]


def _sources() -> list[Path]:
    return sorted(
        path for path in PROOF.rglob("*.py") if "__pycache__" not in path.parts
    )


def _module_key(path: Path) -> str:
    """Dotted module path relative to the proof tree root."""

    parts = path.relative_to(PROOF).with_suffix("").parts
    if parts[-1] == "__init__":
        parts = parts[:-1]
    return ".".join(parts)


def _signature(node: ast.FunctionDef | ast.AsyncFunctionDef) -> Signature:
    """Minimum and maximum positional arity, and whether keywords stay open."""

    positional = node.args.posonlyargs + node.args.args
    required = len(positional) - len(node.args.defaults)
    maximum = sys.maxsize if node.args.vararg else len(positional)
    open_keywords = node.args.kwarg is not None or bool(node.args.kwonlyargs)
    return required, maximum, open_keywords


def _module_level_functions(tree: ast.Module) -> dict[str, Signature]:
    return {
        node.name: _signature(node)
        for node in tree.body
        if type(node) in (ast.FunctionDef, ast.AsyncFunctionDef)
    }


def _lookup(
    name: str,
    defined: dict[str, dict[str, Signature]],
    by_stem: dict[str, str],
) -> str | None:
    """Resolve a written module name to one tree module, or nothing."""

    if name in defined:
        return name
    # A bare name is only resolvable while its stem is unique: these modules
    # reach each other through sys.path entries, not through the package.
    return by_stem.get(name)


def _bindings(
    tree: ast.Module,
    defined: dict[str, dict[str, Signature]],
    by_stem: dict[str, str],
) -> tuple[dict[str, str], dict[str, Signature]]:
    """Aliases of tree modules, and names imported directly from them."""

    aliases: dict[str, str] = {}
    direct: dict[str, Signature] = {}
    for node in ast.walk(tree):
        if type(node) is ast.Import:
            for item in node.names:
                module = _lookup(item.name, defined, by_stem)
                if module is None:
                    continue
                if item.asname is not None:
                    aliases[item.asname] = module
                elif "." not in item.name:
                    # `import a.b` binds only `a`; only a dotless import
                    # binds a name this resolver can match.
                    aliases[item.name] = module
        elif type(node) is ast.ImportFrom and node.module and not node.level:
            package = _lookup(node.module, defined, by_stem)
            for item in node.names:
                submodule = _lookup(f"{node.module}.{item.name}", defined, by_stem)
                if submodule is not None:
                    aliases[item.asname or item.name] = submodule
                    continue
                if package is None or item.asname is not None:
                    continue
                signature = defined[package].get(item.name)
                if signature is not None:
                    direct[item.name] = signature
    return aliases, direct


def _deliberate(tree: ast.Module) -> set[int]:
    """Calls that are written wrong on purpose, so they must not be reported.

    Hostile-input tests invoke a constructor with the wrong shape to prove it
    refuses.  Two idioms carry that intent here: a `lambda:` wrapping the call
    for deferred invocation, and the body of an `assertRaises` block.  Skipping
    them costs coverage, which the anti-vacuity floor keeps visible; reporting
    them would make the gate fire on correct code, which is how gates get
    switched off.
    """

    excluded: set[int] = set()

    def bury(node: ast.AST) -> None:
        for inner in ast.walk(node):
            if type(inner) is ast.Call:
                excluded.add(id(inner))

    for node in ast.walk(tree):
        if type(node) is ast.Lambda:
            bury(node.body)
        elif type(node) is ast.With:
            for item in node.items:
                call = item.context_expr
                if type(call) is ast.Call and "assertRaises" in ast.dump(call.func):
                    for statement in node.body:
                        bury(statement)
    return excluded


def _supplied(node: ast.Call) -> int | None:
    """Argument count, or None when unpacking makes it undecidable."""

    if any(type(arg) is ast.Starred for arg in node.args):
        return None
    if any(keyword.arg is None for keyword in node.keywords):
        return None
    return len(node.args) + len(node.keywords)


class CallArityGateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.trees = {
            path: ast.parse(path.read_text("utf-8"), filename=str(path))
            for path in _sources()
        }
        cls.defined = {
            _module_key(path): _module_level_functions(tree)
            for path, tree in cls.trees.items()
        }
        stems: dict[str, list[str]] = {}
        for key in cls.defined:
            stems.setdefault(key.rsplit(".", 1)[-1], []).append(key)
        cls.by_stem = {
            stem: keys[0] for stem, keys in stems.items() if len(keys) == 1
        }

    def _resolve(self, node: ast.Call, aliases, direct) -> Signature | None:
        func = node.func
        if type(func) is ast.Attribute and type(func.value) is ast.Name:
            module = aliases.get(func.value.id)
            if module is not None:
                return self.defined[module].get(func.attr)
            return None
        if type(func) is ast.Name:
            return direct.get(func.id)
        return None

    def _drift(self) -> list[str]:
        drift: list[str] = []
        for path, tree in self.trees.items():
            aliases, direct = _bindings(tree, self.defined, self.by_stem)
            if not aliases and not direct:
                continue
            deliberate = _deliberate(tree)
            for node in ast.walk(tree):
                if type(node) is not ast.Call or id(node) in deliberate:
                    continue
                signature = self._resolve(node, aliases, direct)
                if signature is None:
                    continue
                supplied = _supplied(node)
                if supplied is None:
                    continue
                required, maximum, open_keywords = signature
                if supplied < required or (not open_keywords and supplied > maximum):
                    drift.append(
                        f"{path.relative_to(PROOF)}:{node.lineno}: {ast.unparse(node.func)}"
                        f" called with {supplied} argument(s), definition takes"
                        f" {required}..{'*' if maximum == sys.maxsize else maximum}"
                    )
        return drift

    def test_the_gate_sees_the_tree_it_claims_to_guard(self) -> None:
        # Anti-vacuity: an empty parse, or a resolver that binds nothing,
        # would make the drift assertion trivially true.
        self.assertGreater(len(self.trees), 30)
        resolved = 0
        for tree in self.trees.values():
            aliases, direct = _bindings(tree, self.defined, self.by_stem)
            for node in ast.walk(tree):
                if type(node) is ast.Call and self._resolve(node, aliases, direct):
                    resolved += 1
        self.assertGreater(resolved, 100)

    def test_no_call_site_drifts_from_its_definition(self) -> None:
        self.assertEqual(self._drift(), [])

    def test_the_gate_catches_a_planted_drift(self) -> None:
        # Deliberate sabotage: a call one argument short of the definition
        # must be reported, otherwise the gate above is theatre.
        planted = ast.parse(
            "import corpus\ncorpus.decision_procedure_work_bound_v1(definition)"
        )
        aliases, direct = _bindings(planted, self.defined, self.by_stem)
        call = planted.body[1].value
        signature = self._resolve(call, aliases, direct)
        self.assertIsNotNone(signature)
        self.assertLess(_supplied(call), signature[0])


if __name__ == "__main__":
    unittest.main()
