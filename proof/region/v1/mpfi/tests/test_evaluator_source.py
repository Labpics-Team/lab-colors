#!/usr/bin/env python3
"""Hostile source and runtime contract for the independent MPFI evaluator."""

from __future__ import annotations

import hashlib
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MPFI = Path(__file__).resolve().parents[1]
EVALUATOR = MPFI / "evaluator"
REPO = MPFI.parents[3]
FORMULA = REPO / "crates/labcolors-core/contracts/contextual-region-formula-v1.lcir"
GENERATOR = EVALUATOR / "formula.py"
sys.path.insert(0, str(MPFI.parent))

from mpfi import operations  # noqa: E402


def generate(source: bytes) -> subprocess.CompletedProcess[bytes]:
    with tempfile.TemporaryDirectory() as temporary:
        path = Path(temporary) / "formula.lcir"
        path.write_bytes(source)
        return subprocess.run(
            [sys.executable, str(GENERATOR), str(path)],
            check=False,
            capture_output=True,
            stdin=subprocess.DEVNULL,
            timeout=60,
            env={
                "PATH": os.environ.get("PATH", ""),
                "PYTHONDONTWRITEBYTECODE": "1",
                "PYTHONHASHSEED": "0",
            },
        )


class FormulaSourceTests(unittest.TestCase):
    def test_registered_formula_is_deterministic_and_has_no_binary64_path(self) -> None:
        source = FORMULA.read_bytes()
        first = generate(source)
        second = generate(source)

        self.assertEqual(first.returncode, 0, first.stderr.decode())
        self.assertEqual(second.returncode, 0, second.stderr.decode())
        self.assertEqual(first.stdout, second.stdout)
        self.assertEqual(
            hashlib.sha256(first.stdout).hexdigest(),
            "a8df7529261ba68e8fbf591cff283ec88a35cb98958b293bc7885d9fb4dd0fb6",
        )
        self.assertIn(b"lc_mpfi_formula_point", first.stdout)
        self.assertIn(b"lc_mpfi_formula_segment", first.stdout)
        self.assertIn(b"lc_mpfi_formula_singleton", first.stdout)
        self.assertNotIn(b"double", first.stdout)

    def test_formula_mutations_are_rejected_before_c_output(self) -> None:
        source = FORMULA.read_bytes()
        mutations = (
            (b"labcolors_exact_real_ssa 1", b"labcolors_exact_real_ssa 2"),
            (b"operator add 2 real exact_x_plus_y", b"operator add 2 real exact_x_minus_y"),
            (b"literal p1_7 3ffb333333333333", b"literal p1_7 3ffb333333333334"),
            (b"rule boundary inclusive", b"rule boundary exclusive"),
            (b"point_nodes 226", b"point_nodes 225"),
        )
        for needle, replacement in mutations:
            with self.subTest(replacement=replacement):
                self.assertIn(needle, source)
                result = generate(source.replace(needle, replacement, 1))
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, b"")
        for mutant in (source + b"\n", source.replace(b"\n", b"\r\n", 1)):
            result = generate(mutant)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, b"")

    def test_generator_does_not_import_the_protocol_or_the_other_engine(self) -> None:
        source = GENERATOR.read_text(encoding="utf-8")
        for forbidden in (
            "region_proof_protocol",
            "controller",
            "import arb",
            "import flint",
            "import numpy",
            "import scipy",
        ):
            self.assertNotIn(forbidden, source)


class EvaluatorSourceTests(unittest.TestCase):
    def test_source_tree_is_complete_and_operation_closed(self) -> None:
        required = (
            "main.c",
            "wire.c",
            "wire.h",
            "hash.c",
            "hash.h",
            "interval.c",
            "interval.h",
            "region.c",
            "region.h",
            "formula.h",
            "formula.py",
        )
        for name in required:
            with self.subTest(name=name):
                self.assertTrue((EVALUATOR / name).is_file(), name)
        self.assertEqual(operations.validate_sources(EVALUATOR), ())

    def test_mutating_an_allowed_call_to_a_forbidden_operation_is_red(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            copy = Path(temporary)
            for path in EVALUATOR.glob("*.c"):
                (copy / path.name).write_text(path.read_text(encoding="utf-8"), encoding="utf-8")
            for path in EVALUATOR.glob("*.h"):
                (copy / path.name).write_text(path.read_text(encoding="utf-8"), encoding="utf-8")
            interval = copy / "interval.c"
            interval.write_text(
                interval.read_text(encoding="utf-8").replace(
                    "mpfi_div(output, left, right)",
                    "mpfi_div_ext(output, left, right)",
                    1,
                ),
                encoding="utf-8",
            )
            errors = operations.validate_sources(copy)
            self.assertTrue(any("forbidden operation mpfi_div_ext" in error for error in errors))

    def test_build_recipe_is_closed_and_requires_clang_19(self) -> None:
        recipe = (MPFI / "build.sh").read_text(encoding="utf-8")
        self.assertIn("/usr/bin/clang-19", recipe)
        self.assertIn("^clang version 19\\.", recipe)
        self.assertIn("-fno-fast-math", recipe)
        self.assertIn("-ffp-contract=off", recipe)
        self.assertIn("-fno-lto", recipe)
        self.assertIn("mpfi-evaluator-v1", recipe)
        self.assertIn("readelf", recipe)
        self.assertNotIn("gcc", recipe.lower())

    def test_no_pre_run_receipt_or_arb_compatibility_layer_exists(self) -> None:
        self.assertFalse((MPFI / "receipt.py").exists())
        joined = "\n".join(
            path.read_text(encoding="utf-8")
            for path in EVALUATOR.glob("*.c")
        ).lower()
        for forbidden in ("arb", "flint", "fallback", "long double", "strtod"):
            self.assertNotIn(forbidden, joined)


class RuntimeTests(unittest.TestCase):
    @unittest.skipUnless(
        os.environ.get("LABCOLORS_MPFI_EVALUATOR"),
        "set LABCOLORS_MPFI_EVALUATOR to the controlled C17 binary",
    )
    def test_frozen_fixture_produces_a_canonical_transcript(self) -> None:
        executable = os.environ["LABCOLORS_MPFI_EVALUATOR"]
        fixture = (REPO / "proof/region/v1/fixtures/proof-job-v1.bin").read_bytes()
        manifest = bytes.fromhex("01" + "23" * 31)
        result = subprocess.run(
            [
                executable,
                "--manifest-identity",
                manifest.hex(),
                "--job",
                "/dev/stdin",
            ],
            input=fixture,
            check=False,
            capture_output=True,
            timeout=300,
        )
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        self.assertEqual(result.stderr, b"")
        sys.path.insert(0, str(REPO / "proof/region/v1"))
        from region_proof_protocol import DecisionTranscriptV1  # noqa: PLC0415

        transcript = DecisionTranscriptV1.parse(result.stdout)
        self.assertEqual(transcript.encode(), result.stdout)
        self.assertEqual(transcript.comparator_identity, manifest)

    @unittest.skipUnless(
        os.environ.get("LABCOLORS_MPFI_EVALUATOR"),
        "set LABCOLORS_MPFI_EVALUATOR to the controlled C17 binary",
    )
    def test_black_exact_zero_emits_the_canonical_trace_witness(self) -> None:
        sys.path.insert(0, str(REPO / "proof/region/v1"))
        from region_proof_protocol import (  # noqa: PLC0415
            ComparatorBudgetV1,
            ComparatorKindV1,
            ContextualRegionDefinitionV1,
            DecisionTranscriptV1,
            ExactZeroSignalTraceV1,
            ProofJobV1,
            ProofPolicyV1,
            ReducedDomainManifestV1,
        )

        registered = ContextualRegionDefinitionV1.parse(
            (REPO / "proof/region/v1/fixtures/v5b2b-definition-0a8d1c3d.bin").read_bytes()
        )
        zero = bytes(8)
        definition = ContextualRegionDefinitionV1(
            registered.fields[:21] + ((1).to_bytes(8, "big"),) + (zero,) * 4,
            1,
        )
        policy = ProofPolicyV1(
            1,
            (
                ComparatorBudgetV1(ComparatorKindV1.ARB, (128,), 1, 1),
                ComparatorBudgetV1(ComparatorKindV1.MPFI, (192,), 1, 1),
            ),
        )
        job = ProofJobV1(
            definition,
            FORMULA.read_bytes(),
            ReducedDomainManifestV1.from_ordinals((0,)),
            policy,
        )
        manifest = bytes.fromhex("ab" + "00" * 31)
        result = subprocess.run(
            [
                os.environ["LABCOLORS_MPFI_EVALUATOR"],
                "--manifest-identity",
                manifest.hex(),
                "--job",
                "/dev/stdin",
            ],
            input=job.encode(),
            check=False,
            capture_output=True,
            timeout=300,
        )
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        transcript = DecisionTranscriptV1.parse(result.stdout)
        self.assertEqual(tuple(transcript.iter_decisions()), (0,))
        self.assertEqual(transcript.counters, (1, 0, 0, 0))
        self.assertEqual(transcript.exact_equality_count, 1)
        witness = tuple(transcript.iter_witnesses())[0]
        self.assertIs(type(witness), ExactZeroSignalTraceV1)
        self.assertEqual(
            witness.trace_digest,
            hashlib.sha256(
                b"labcolors.proof-region.exact-zero-signal-trace.v1\0"
                + job.identity
                + (0).to_bytes(4, "big")
                + (0).to_bytes(8, "big")
            ).digest(),
        )


if __name__ == "__main__":
    unittest.main()
