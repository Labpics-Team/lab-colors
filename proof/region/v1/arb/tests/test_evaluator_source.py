#!/usr/bin/env python3
"""Hostile source contract for the standalone Arb evaluator."""

from __future__ import annotations

import hashlib
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ARB = Path(__file__).resolve().parents[1]
EVALUATOR = ARB / "evaluator"
REPO = ARB.parents[3]
FORMULA = REPO / "crates/labcolors-core/contracts/contextual-region-formula-v1.lcir"
GENERATOR = EVALUATOR / "formula.py"
sys.path.insert(0, str(REPO / "proof/region/v1"))

from region_proof_protocol import (  # noqa: E402
    BoundaryUnprovenWitnessV1,
    ComparatorBudgetV1,
    ComparatorKindV1,
    ComparatorManifestV2,
    ContextualRegionDefinitionV1,
    DecisionTranscriptV1,
    DecisionV1,
    ExactZeroSignalTraceV1,
    ProofJobV1,
    ProofPolicyV1,
    ReducedDomainManifestV1,
    ResourceLimitWitnessV1,
)


def generate(source: bytes) -> subprocess.CompletedProcess[bytes]:
    with tempfile.TemporaryDirectory() as temporary:
        formula = Path(temporary) / "formula.lcir"
        formula.write_bytes(source)
        return subprocess.run(
            [sys.executable, str(GENERATOR), str(formula)],
            check=False,
            capture_output=True,
            env={
                "PATH": os.environ.get("PATH", ""),
                "PYTHONDONTWRITEBYTECODE": "1",
                "PYTHONHASHSEED": "0",
            },
        )


def assert_transcript_wire_coordinates(
    case: unittest.TestCase,
    wire: bytes,
    transcript: DecisionTranscriptV1,
    manifest_identity: bytes,
) -> None:
    decision_bits = transcript.decision_bits
    accounting_digest = transcript.accounting_digest
    case.assertEqual(wire[:8], b"LCTRN1\0\0")
    case.assertEqual(wire[72:104], manifest_identity)
    accounting_offset = 160 + len(decision_bits)
    case.assertEqual(
        wire[accounting_offset : accounting_offset + 32],
        accounting_digest,
    )


class FormulaGeneratorTests(unittest.TestCase):
    def test_registered_formula_generates_one_deterministic_c_program(self) -> None:
        source = FORMULA.read_bytes()
        first = generate(source)
        second = generate(source)

        self.assertEqual(first.returncode, 0, first.stderr.decode())
        self.assertEqual(second.returncode, 0, second.stderr.decode())
        self.assertEqual(first.stdout, second.stdout)
        self.assertEqual(
            hashlib.sha256(first.stdout).hexdigest(),
            "9958f20c8ca598625db0593a45f8f8bc79e4b2f22b53263b6c32d78a5e1d2693",
        )
        self.assertIn(b"lc_formula_point", first.stdout)
        self.assertIn(b"lc_formula_segment", first.stdout)
        self.assertIn(b"lc_formula_singleton", first.stdout)
        self.assertNotIn(b"double", first.stdout)

    def test_generator_rejects_canonical_semantic_and_driver_mutations(self) -> None:
        source = FORMULA.read_bytes()
        mutations = (
            (b"labcolors_exact_real_ssa 1", b"labcolors_exact_real_ssa 2"),
            (b"operator add 2 real exact_x_plus_y", b"operator add 2 real exact_x_minus_y"),
            (b"node xyz_x_r real mul srgb_m00 linear_r", b"node xyz_x_r real add srgb_m00 linear_r"),
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

    def test_generator_is_independent_from_python_protocol_and_controller(self) -> None:
        source = GENERATOR.read_text(encoding="utf-8")
        for forbidden in (
            "region_proof_protocol",
            "controller",
            "import numpy",
            "import scipy",
        ):
            self.assertNotIn(forbidden, source)


class StandaloneSourceTests(unittest.TestCase):
    def test_evaluator_has_an_independent_wire_hash_interval_and_region_path(self) -> None:
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
        )
        for name in required:
            with self.subTest(name=name):
                self.assertTrue((EVALUATOR / name).is_file(), name)

        joined = "\n".join(
            (EVALUATOR / name).read_text(encoding="utf-8")
            for name in required
        )
        for forbidden in (
            "region_proof_protocol",
            "controller.py",
            "arb_set_d(",
            "strtod(",
            "#include <math.h>",
            " pow(",
            " sqrt(",
            "epsilon",
            "midpoint",
            "fallback",
        ):
            self.assertNotIn(forbidden, joined)
        self.assertIn("arb_set_fmpz_2exp", joined)
        self.assertIn("arb_get_interval_fmpz_2exp", joined)
        self.assertIn("LCTRN1", joined)
        self.assertNotIn("LCARO1", joined)
        self.assertIn("--manifest-identity", joined)

    def test_closed_boundary_and_typed_unresolved_states_are_structural(self) -> None:
        region = (EVALUATOR / "region.c").read_text(encoding="utf-8")
        header = (EVALUATOR / "region.h").read_text(encoding="utf-8")

        for outcome in (
            "LC_REGION_INSIDE",
            "LC_REGION_OUTSIDE",
            "LC_REGION_BOUNDARY_UNPROVEN",
            "LC_REGION_RESOURCE_LIMIT_REACHED",
        ):
            self.assertIn(outcome, header)
        self.assertIn("arb_is_nonpositive", region)
        self.assertIn("arb_is_positive", region)
        self.assertIn("arb_intersection", region)
        self.assertNotIn("arb_contains_zero(f)", region)

    def test_subminimum_flint_precision_never_enters_the_formula(self) -> None:
        region = (EVALUATOR / "region.c").read_text(encoding="utf-8")
        evaluator = region[region.index("lc_region_evaluate_rgb(") :]

        guard = evaluator.index("if (precision < 2)")
        formula_call = evaluator.index("lc_formula_point(")
        self.assertLess(guard, formula_call)
        self.assertIn("minimum working precision", evaluator[:formula_call])

        decision = region[
            region.index("lc_region_decide(") : region.index("lc_region_evaluate_rgb(")
        ]
        decision_guard = decision.index("if (precision < 2)")
        singleton_dispatch = decision.index("if (region->knot_count == 1)")
        self.assertLess(decision_guard, singleton_dispatch)

    def test_sha256_has_literal_standard_vectors_and_no_external_crypto(self) -> None:
        source = (EVALUATOR / "hash.c").read_text(encoding="utf-8")
        header = (EVALUATOR / "hash.h").read_text(encoding="utf-8")
        self.assertIn("lc_sha256", header)
        self.assertIn("0x6a09e667", source)
        self.assertNotIn("openssl", source.lower())


class ExactBoundaryRuntimeTests(unittest.TestCase):
    @unittest.skipUnless(
        os.environ.get("LABCOLORS_ARB_EVALUATOR"),
        "set LABCOLORS_ARB_EVALUATOR to the controlled C17 binary",
    )
    def test_cli_requires_one_nonzero_lowercase_manifest_identity(self) -> None:
        executable = os.environ["LABCOLORS_ARB_EVALUATOR"]
        valid = "ab" + "00" * 31
        invalid_invocations = (
            (),
            ("--manifest-identity", "0" * 64, "--job", "/dev/stdin"),
            ("--manifest-identity", valid.upper(), "--job", "/dev/stdin"),
            ("--manifest-identity", "g" + valid[1:], "--job", "/dev/stdin"),
            ("--manifest-identity", valid[:-1], "--job", "/dev/stdin"),
            ("--manifest-identity", valid + "0", "--job", "/dev/stdin"),
            ("--manifest-identity", valid, "--job", "job.bin"),
            ("--manifest", valid, "--job", "/dev/stdin"),
            ("--manifest-identity", valid, "--job", "/dev/stdin", "extra"),
        )
        for arguments in invalid_invocations:
            with self.subTest(arguments=arguments):
                result = subprocess.run(
                    (executable, *arguments),
                    input=b"",
                    check=False,
                    capture_output=True,
                )
                self.assertEqual(result.returncode, 64)
                self.assertEqual(result.stdout, b"")

        accepted = subprocess.run(
            (
                executable,
                "--manifest-identity",
                valid,
                "--job",
                "/dev/stdin",
            ),
            input=b"",
            check=False,
            capture_output=True,
        )
        self.assertEqual(accepted.returncode, 1)
        self.assertEqual(accepted.stdout, b"")
        self.assertIn(b"job read failed", accepted.stderr)

    @unittest.skipUnless(
        os.environ.get("LABCOLORS_ARB_EVALUATOR"),
        "set LABCOLORS_ARB_EVALUATOR to the controlled C17 binary",
    )
    def test_black_exact_zero_runs_through_job_parser_formula_and_closed_driver(self) -> None:
        registered = ContextualRegionDefinitionV1.parse(
            (REPO / "proof/region/v1/fixtures/v5b2b-definition-0a8d1c3d.bin").read_bytes()
        )
        zero = bytes(8)
        fields = registered.fields[:21] + ((1).to_bytes(8, "big"),) + (zero,) * 4
        definition = ContextualRegionDefinitionV1(fields, 1)
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
        manifest = ComparatorManifestV2(
            ComparatorKindV1.ARB,
            *(hashlib.sha256(f"arb-manifest-{index}".encode()).digest() for index in range(10)),
        )
        executable = os.environ["LABCOLORS_ARB_EVALUATOR"]
        result = subprocess.run(
            [
                executable,
                "--manifest-identity",
                manifest.identity.hex(),
                "--job",
                "/dev/stdin",
            ],
            input=job.encode(),
            check=False,
            capture_output=True,
        )

        self.assertEqual(result.returncode, 0, result.stderr.decode())
        self.assertEqual(result.stderr, b"")
        transcript = DecisionTranscriptV1.parse(result.stdout)
        self.assertEqual(transcript.encode(), result.stdout)
        assert_transcript_wire_coordinates(
            self,
            result.stdout,
            transcript,
            manifest.identity,
        )
        self.assertEqual(transcript.job_identity, job.identity)
        self.assertEqual(transcript.domain_identity, job.domain.identity)
        self.assertEqual(transcript.comparator_identity, manifest.identity)
        self.assertEqual(tuple(transcript.iter_decisions()), (DecisionV1.INSIDE,))
        self.assertEqual(transcript.counters, (1, 0, 0, 0))
        self.assertEqual(transcript.exact_equality_count, 1)
        witnesses = tuple(transcript.iter_witnesses())
        self.assertEqual(len(witnesses), 1)
        self.assertIs(type(witnesses[0]), ExactZeroSignalTraceV1)
        self.assertEqual(witnesses[0].ordinal, 0)
        self.assertEqual(
            witnesses[0].trace_digest,
            hashlib.sha256(
                b"labcolors.proof-region.exact-zero-signal-trace.v1\0"
                + job.identity
                + (0).to_bytes(4, "big")
                + (0).to_bytes(8, "big")
            ).digest(),
        )

        alternate_manifest = ComparatorManifestV2(
            ComparatorKindV1.ARB,
            *(hashlib.sha256(f"arb-alternate-{index}".encode()).digest() for index in range(10)),
        )
        alternate = subprocess.run(
            [
                executable,
                "--manifest-identity",
                alternate_manifest.identity.hex(),
                "--job",
                "/dev/stdin",
            ],
            input=job.encode(),
            check=False,
            capture_output=True,
        )
        self.assertEqual(alternate.returncode, 0, alternate.stderr.decode())
        alternate_transcript = DecisionTranscriptV1.parse(alternate.stdout)
        self.assertEqual(
            tuple(alternate_transcript.iter_decisions()),
            tuple(transcript.iter_decisions()),
        )
        self.assertEqual(alternate_transcript.counters, transcript.counters)
        self.assertEqual(tuple(alternate_transcript.iter_witnesses()), witnesses)
        self.assertEqual(alternate_transcript.comparator_identity, alternate_manifest.identity)
        self.assertNotEqual(alternate_transcript.accounting_digest, transcript.accounting_digest)
        self.assertNotEqual(alternate.stdout, result.stdout)

        corrupted = bytearray(job.encode())
        corrupted[-1] ^= 1
        rejected = subprocess.run(
            [
                executable,
                "--manifest-identity",
                manifest.identity.hex(),
                "--job",
                "/dev/stdin",
            ],
            input=corrupted,
            check=False,
            capture_output=True,
        )
        self.assertNotEqual(rejected.returncode, 0)
        self.assertEqual(rejected.stdout, b"")

    @unittest.skipUnless(
        os.environ.get("LABCOLORS_ARB_EVALUATOR"),
        "set LABCOLORS_ARB_EVALUATOR to the controlled C17 binary",
    )
    def test_multisegment_exact_trace_selects_first_canonical_branch(self) -> None:
        registered = ContextualRegionDefinitionV1.parse(
            (REPO / "proof/region/v1/fixtures/v5b2b-definition-0a8d1c3d.bin").read_bytes()
        )
        zero = bytes(8)
        tones = tuple(
            bytes.fromhex(bits)
            for bits in (
                "c000000000000000",
                "bff0000000000000",
                "0000000000000000",
                "3ff0000000000000",
            )
        )
        knots = tuple(
            coordinate
            for tone in tones
            for coordinate in (tone, zero, zero, zero)
        )
        definition = ContextualRegionDefinitionV1(
            registered.fields[:21] + ((4).to_bytes(8, "big"),) + knots,
            4,
        )
        job = ProofJobV1(
            definition,
            FORMULA.read_bytes(),
            ReducedDomainManifestV1.from_ordinals((0,)),
            ProofPolicyV1(
                1,
                (
                    ComparatorBudgetV1(ComparatorKindV1.ARB, (128,), 2, 2),
                    ComparatorBudgetV1(ComparatorKindV1.MPFI, (192,), 2, 2),
                ),
            ),
        )
        manifest = ComparatorManifestV2(
            ComparatorKindV1.ARB,
            *(hashlib.sha256(f"arb-multisegment-{index}".encode()).digest() for index in range(10)),
        )
        result = subprocess.run(
            (
                os.environ["LABCOLORS_ARB_EVALUATOR"],
                "--manifest-identity",
                manifest.identity.hex(),
                "--job",
                "/dev/stdin",
            ),
            input=job.encode(),
            check=False,
            capture_output=True,
        )

        self.assertEqual(result.returncode, 0, result.stderr.decode())
        transcript = DecisionTranscriptV1.parse(result.stdout)
        self.assertEqual(tuple(transcript.iter_decisions()), (DecisionV1.INSIDE,))
        self.assertEqual(transcript.counters, (1, 0, 0, 0))
        self.assertEqual(transcript.exact_equality_count, 1)
        witnesses = tuple(transcript.iter_witnesses())
        self.assertEqual(len(witnesses), 1)
        self.assertIs(type(witnesses[0]), ExactZeroSignalTraceV1)
        self.assertEqual(
            witnesses[0].trace_digest,
            hashlib.sha256(
                b"labcolors.proof-region.exact-zero-signal-trace.v1\0"
                + job.identity
                + (0).to_bytes(4, "big")
                + (1).to_bytes(8, "big")
            ).digest(),
        )

    @unittest.skipUnless(
        os.environ.get("LABCOLORS_ARB_EVALUATOR"),
        "set LABCOLORS_ARB_EVALUATOR to the controlled C17 binary",
    )
    def test_frozen_seam_cube_resolves_one_inside_and_511_outside(self) -> None:
        frozen = ProofJobV1.parse(
            (REPO / "proof/region/v1/fixtures/proof-job-v1.bin").read_bytes()
        )
        budget = (
            ComparatorBudgetV1(ComparatorKindV1.ARB, (64, 128), 4, 2048),
            ComparatorBudgetV1(ComparatorKindV1.MPFI, (64, 128), 4, 2048),
        )
        job = ProofJobV1(
            frozen.definition,
            frozen.formula_spec,
            frozen.domain,
            ProofPolicyV1(1, budget),
        )
        manifest = ComparatorManifestV2(
            ComparatorKindV1.ARB,
            *(hashlib.sha256(f"arb-manifest-{index}".encode()).digest() for index in range(10)),
        )
        invocation = [
            os.environ["LABCOLORS_ARB_EVALUATOR"],
            "--manifest-identity",
            manifest.identity.hex(),
            "--job",
            "/dev/stdin",
        ]
        first = subprocess.run(
            invocation,
            input=job.encode(),
            check=False,
            capture_output=True,
        )
        second = subprocess.run(
            invocation,
            input=job.encode(),
            check=False,
            capture_output=True,
        )

        self.assertEqual(first.returncode, 0, first.stderr.decode())
        self.assertEqual(second.returncode, 0, second.stderr.decode())
        self.assertEqual(first.stdout, second.stdout)
        transcript = DecisionTranscriptV1.parse(first.stdout)
        self.assertEqual(transcript.encode(), first.stdout)
        assert_transcript_wire_coordinates(
            self,
            first.stdout,
            transcript,
            manifest.identity,
        )
        self.assertEqual(transcript.job_identity, job.identity)
        self.assertEqual(transcript.domain_identity, job.domain.identity)
        self.assertEqual(transcript.comparator_identity, manifest.identity)
        self.assertEqual(len(transcript.decision_bits), 128)
        self.assertEqual(transcript.counters, (1, 511, 0, 0))
        self.assertEqual(transcript.exact_equality_count, 0)
        self.assertEqual(tuple(transcript.iter_witnesses()), ())

        low_precision = ProofJobV1(
            frozen.definition,
            frozen.formula_spec,
            frozen.domain,
            ProofPolicyV1(
                1,
                (
                    ComparatorBudgetV1(ComparatorKindV1.ARB, (16,), 4, 2_048),
                    ComparatorBudgetV1(ComparatorKindV1.MPFI, (24,), 4, 2_048),
                ),
            ),
        )
        low_first = subprocess.run(
            invocation,
            input=low_precision.encode(),
            check=False,
            capture_output=True,
        )
        low_second = subprocess.run(
            invocation,
            input=low_precision.encode(),
            check=False,
            capture_output=True,
        )
        self.assertEqual(low_first.returncode, 0, low_first.stderr.decode())
        self.assertEqual(low_second.returncode, 0, low_second.stderr.decode())
        self.assertEqual(low_first.stdout, low_second.stdout)
        low_transcript = DecisionTranscriptV1.parse(low_first.stdout)
        self.assertEqual(low_transcript.encode(), low_first.stdout)
        assert_transcript_wire_coordinates(
            self,
            low_first.stdout,
            low_transcript,
            manifest.identity,
        )
        self.assertEqual(low_transcript.counters, (0, 501, 11, 0))
        low_witnesses = tuple(low_transcript.iter_witnesses())
        self.assertTrue(
            all(type(witness) is BoundaryUnprovenWitnessV1 for witness in low_witnesses)
        )
        self.assertEqual(
            tuple(witness.ordinal for witness in low_witnesses),
            (
                65_793,
                657_930,
                723_723,
                8_355_711,
                8_421_247,
                8_421_503,
                8_421_504,
                16_711_422,
                16_776_958,
                16_777_214,
                16_777_215,
            ),
        )

    @unittest.skipUnless(
        os.environ.get("LABCOLORS_ARB_EVALUATOR"),
        "set LABCOLORS_ARB_EVALUATOR to the controlled C17 binary",
    )
    def test_zero_grant_emits_canonical_resource_witnesses(self) -> None:
        frozen = ProofJobV1.parse(
            (REPO / "proof/region/v1/fixtures/proof-job-v1.bin").read_bytes()
        )
        zero_grant = (
            ComparatorBudgetV1(ComparatorKindV1.ARB, (64,), 0, 0),
            ComparatorBudgetV1(ComparatorKindV1.MPFI, (64,), 0, 0),
        )
        job = ProofJobV1(
            frozen.definition,
            frozen.formula_spec,
            frozen.domain,
            ProofPolicyV1(1, zero_grant),
        )
        manifest = ComparatorManifestV2(
            ComparatorKindV1.ARB,
            *(hashlib.sha256(f"arb-zero-grant-{index}".encode()).digest() for index in range(10)),
        )
        result = subprocess.run(
            (
                os.environ["LABCOLORS_ARB_EVALUATOR"],
                "--manifest-identity",
                manifest.identity.hex(),
                "--job",
                "/dev/stdin",
            ),
            input=job.encode(),
            check=False,
            capture_output=True,
        )

        self.assertEqual(result.returncode, 0, result.stderr.decode())
        transcript = DecisionTranscriptV1.parse(result.stdout)
        self.assertEqual(transcript.encode(), result.stdout)
        assert_transcript_wire_coordinates(
            self,
            result.stdout,
            transcript,
            manifest.identity,
        )
        self.assertEqual(transcript.counters, (0, 504, 0, 8))
        witnesses = tuple(transcript.iter_witnesses())
        self.assertEqual(
            tuple(witness.ordinal for witness in witnesses),
            (10, 11, 256, 257, 65_537, 65_546, 65_792, 65_793),
        )
        self.assertTrue(all(type(witness) is ResourceLimitWitnessV1 for witness in witnesses))
        self.assertTrue(
            all(
                (witness.scope, witness.granted, witness.consumed) == (1, 0, 0)
                for witness in witnesses
            )
        )

    @unittest.skipUnless(
        os.environ.get("LABCOLORS_ARB_EVALUATOR"),
        "set LABCOLORS_ARB_EVALUATOR to the controlled C17 binary",
    )
    def test_global_pregrant_is_never_transferred_between_points(self) -> None:
        frozen = ProofJobV1.parse(
            (REPO / "proof/region/v1/fixtures/proof-job-v1.bin").read_bytes()
        )
        job = ProofJobV1(
            frozen.definition,
            frozen.formula_spec,
            ReducedDomainManifestV1.from_ordinals((0, 65_793)),
            ProofPolicyV1(
                1,
                (
                    ComparatorBudgetV1(ComparatorKindV1.ARB, (32,), 1, 1),
                    ComparatorBudgetV1(ComparatorKindV1.MPFI, (40,), 1, 1),
                ),
            ),
        )
        manifest = ComparatorManifestV2(
            ComparatorKindV1.ARB,
            *(hashlib.sha256(f"arb-pregrant-{index}".encode()).digest() for index in range(10)),
        )
        result = subprocess.run(
            (
                os.environ["LABCOLORS_ARB_EVALUATOR"],
                "--manifest-identity",
                manifest.identity.hex(),
                "--job",
                "/dev/stdin",
            ),
            input=job.encode(),
            check=False,
            capture_output=True,
        )

        self.assertEqual(result.returncode, 0, result.stderr.decode())
        transcript = DecisionTranscriptV1.parse(result.stdout)
        self.assertEqual(
            tuple(transcript.iter_decisions()),
            (DecisionV1.OUTSIDE, DecisionV1.RESOURCE_LIMIT_REACHED),
        )
        self.assertEqual(transcript.counters, (0, 1, 0, 1))
        witnesses = tuple(transcript.iter_witnesses())
        self.assertEqual(
            witnesses,
            (ResourceLimitWitnessV1(65_793, scope=2, granted=0, consumed=0),),
        )

    @unittest.skipUnless(
        os.environ.get("LABCOLORS_ARB_EVALUATOR"),
        "set LABCOLORS_ARB_EVALUATOR to the controlled C17 binary",
    )
    def test_subminimum_precision_is_unresolved_and_a_later_valid_rung_recovers(self) -> None:
        frozen = ProofJobV1.parse(
            (REPO / "proof/region/v1/fixtures/proof-job-v1.bin").read_bytes()
        )
        domain = ReducedDomainManifestV1.from_ordinals((0, 65_793))
        manifest = ComparatorManifestV2(
            ComparatorKindV1.ARB,
            *(hashlib.sha256(f"arb-minimum-precision-{index}".encode()).digest() for index in range(10)),
        )
        invocation = (
            os.environ["LABCOLORS_ARB_EVALUATOR"],
            "--manifest-identity",
            manifest.identity.hex(),
            "--job",
            "/dev/stdin",
        )

        def run_with(arb_ladder: tuple[int, ...]) -> object:
            job = ProofJobV1(
                frozen.definition,
                frozen.formula_spec,
                domain,
                ProofPolicyV1(
                    1,
                    (
                        ComparatorBudgetV1(
                            ComparatorKindV1.ARB,
                            arb_ladder,
                            1,
                            2,
                        ),
                        ComparatorBudgetV1(
                            ComparatorKindV1.MPFI,
                            (32,),
                            1,
                            2,
                        ),
                    ),
                ),
            )
            result = subprocess.run(
                invocation,
                input=job.encode(),
                check=False,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr.decode())
            transcript = DecisionTranscriptV1.parse(result.stdout)
            self.assertEqual(transcript.encode(), result.stdout)
            return transcript

        unresolved = run_with((1,))
        direct = run_with((32,))
        recovered = run_with((1, 32))

        self.assertEqual(
            tuple(unresolved.iter_decisions()),
            (DecisionV1.BOUNDARY_UNPROVEN, DecisionV1.BOUNDARY_UNPROVEN),
        )
        self.assertEqual(unresolved.counters, (0, 0, 2, 0))
        self.assertEqual(
            tuple(recovered.iter_decisions()),
            tuple(direct.iter_decisions()),
        )
        self.assertEqual(recovered.counters, direct.counters)

    @unittest.skipUnless(
        os.environ.get("LABCOLORS_ARB_EVALUATOR"),
        "set LABCOLORS_ARB_EVALUATOR to the controlled C17 binary",
    )
    def test_resource_witness_accounts_for_work_consumed_on_earlier_rungs(self) -> None:
        frozen = ProofJobV1.parse(
            (REPO / "proof/region/v1/fixtures/proof-job-v1.bin").read_bytes()
        )
        job = ProofJobV1(
            frozen.definition,
            frozen.formula_spec,
            ReducedDomainManifestV1.from_ordinals((257,)),
            ProofPolicyV1(
                1,
                (
                    ComparatorBudgetV1(ComparatorKindV1.ARB, (12, 64), 1, 1),
                    ComparatorBudgetV1(ComparatorKindV1.MPFI, (20, 80), 1, 1),
                ),
            ),
        )
        manifest = ComparatorManifestV2(
            ComparatorKindV1.ARB,
            *(hashlib.sha256(f"arb-cross-rung-{index}".encode()).digest() for index in range(10)),
        )
        result = subprocess.run(
            (
                os.environ["LABCOLORS_ARB_EVALUATOR"],
                "--manifest-identity",
                manifest.identity.hex(),
                "--job",
                "/dev/stdin",
            ),
            input=job.encode(),
            check=False,
            capture_output=True,
        )

        self.assertEqual(result.returncode, 0, result.stderr.decode())
        transcript = DecisionTranscriptV1.parse(result.stdout)
        self.assertEqual(
            tuple(transcript.iter_decisions()),
            (DecisionV1.RESOURCE_LIMIT_REACHED,),
        )
        self.assertEqual(transcript.counters, (0, 0, 0, 1))
        self.assertEqual(
            tuple(transcript.iter_witnesses()),
            (ResourceLimitWitnessV1(257, scope=1, granted=1, consumed=1),),
        )

    @unittest.skipUnless(
        os.environ.get("LABCOLORS_ARB_EVALUATOR"),
        "set LABCOLORS_ARB_EVALUATOR to the controlled C17 binary",
    )
    def test_spd_admission_is_exact_across_the_full_binary64_exponent_range(self) -> None:
        frozen = ProofJobV1.parse(
            (REPO / "proof/region/v1/fixtures/proof-job-v1.bin").read_bytes()
        )
        fields = list(frozen.definition.fields)
        fields[18] = bytes.fromhex("3ff0000000000000")
        fields[19] = bytes.fromhex("0000000000000001")
        fields[20] = bytes.fromhex("3ff0000000000000")
        definition = ContextualRegionDefinitionV1(
            tuple(fields),
            frozen.definition.knot_count,
        )
        job = ProofJobV1(
            definition,
            frozen.formula_spec,
            ReducedDomainManifestV1.from_ordinals((0,)),
            ProofPolicyV1(
                1,
                (
                    ComparatorBudgetV1(ComparatorKindV1.ARB, (64,), 4, 4),
                    ComparatorBudgetV1(ComparatorKindV1.MPFI, (80,), 4, 4),
                ),
            ),
        )
        manifest = ComparatorManifestV2(
            ComparatorKindV1.ARB,
            *(hashlib.sha256(f"arb-exact-spd-{index}".encode()).digest() for index in range(10)),
        )
        result = subprocess.run(
            (
                os.environ["LABCOLORS_ARB_EVALUATOR"],
                "--manifest-identity",
                manifest.identity.hex(),
                "--job",
                "/dev/stdin",
            ),
            input=job.encode(),
            check=False,
            capture_output=True,
        )

        self.assertEqual(result.returncode, 0, result.stderr.decode())
        transcript = DecisionTranscriptV1.parse(result.stdout)
        self.assertEqual(transcript.encode(), result.stdout)
        self.assertEqual(transcript.job_identity, job.identity)


if __name__ == "__main__":
    unittest.main(verbosity=2)
