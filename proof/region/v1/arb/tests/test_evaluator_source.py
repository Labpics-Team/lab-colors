#!/usr/bin/env python3
"""Hostile source contract for the standalone Arb evaluator."""

from __future__ import annotations

import hashlib
import os
import struct
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
# CI watchdogs bound broken test processes; they are not performance claims.
# Change them only with a measured exact native-gate workload and its job budget.
GENERATOR_TIMEOUT_SECONDS = 60
EVALUATOR_TIMEOUT_SECONDS = 300
sys.path.insert(0, str(REPO / "proof/region/v1"))

from arb import runtime as arb_runtime  # noqa: E402
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
            stdin=subprocess.DEVNULL,
            timeout=GENERATOR_TIMEOUT_SECONDS,
            env={
                "PATH": os.environ.get("PATH", ""),
                "PYTHONDONTWRITEBYTECODE": "1",
                "PYTHONHASHSEED": "0",
            },
        )


def run_evaluator(
    command: list[str] | tuple[str, ...],
    stdin: bytes,
) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        command,
        input=stdin,
        check=False,
        capture_output=True,
        timeout=EVALUATOR_TIMEOUT_SECONDS,
    )


def runtime_invocation() -> tuple[str, ...]:
    return (
        os.environ["LABCOLORS_ARB_EVALUATOR"],
        "--manifest-identity",
        "ab" + "00" * 31,
        "--job",
        "/dev/stdin",
    )


def runtime_profile_job(
    *,
    arb_ladder: tuple[int, ...] = (1,),
    mpfi_ladder: tuple[int, ...] = (1,),
    knot_count: int = 1,
) -> ProofJobV1:
    registered = ContextualRegionDefinitionV1.parse(
        (REPO / "proof/region/v1/fixtures/v5b2b-definition-0a8d1c3d.bin").read_bytes()
    )
    zero = bytes(8)
    knots = tuple(
        coordinate
        for index in range(knot_count)
        for coordinate in (struct.pack(">d", float(index)), zero, zero, zero)
    )
    definition = ContextualRegionDefinitionV1(
        registered.fields[:21] + (knot_count.to_bytes(8, "big"),) + knots,
        knot_count,
    )
    return ProofJobV1(
        definition,
        FORMULA.read_bytes(),
        ReducedDomainManifestV1.from_ordinals((0,)),
        ProofPolicyV1(
            1,
            (
                ComparatorBudgetV1(
                    ComparatorKindV1.ARB,
                    arb_ladder,
                    0,
                    0,
                ),
                ComparatorBudgetV1(
                    ComparatorKindV1.MPFI,
                    mpfi_ladder,
                    0,
                    0,
                ),
            ),
        ),
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

    def test_runtime_profile_bounds_input_and_transcript_before_allocation(self) -> None:
        header = (EVALUATOR / "wire.h").read_text(encoding="utf-8")
        wire = (EVALUATOR / "wire.c").read_text(encoding="utf-8")
        main = (EVALUATOR / "main.c").read_text(encoding="utf-8")
        self.assertEqual(arb_runtime.ARB_MAX_JOB_BYTES_V1, 16_777_216)
        self.assertEqual(arb_runtime.ARB_MAX_OUTPUT_BYTES_V1, 16_777_216)
        self.assertEqual(arb_runtime.ARB_MAX_PRECISION_BITS_V1, 4_096)
        self.assertEqual(arb_runtime.ARB_MAX_POLICY_RUNGS_V1, 32)
        self.assertEqual(arb_runtime.ARB_MAX_KNOTS_V1, 1_024)
        for declaration in (
            "#define LC_ARB_MAX_PRECISION_BITS_V1 UINT32_C(4096)",
            "#define LC_ARB_MAX_POLICY_RUNGS_V1 UINT32_C(32)",
            "#define LC_ARB_MAX_KNOTS_V1 UINT64_C(1024)",
            "#define LC_ARB_EXIT_USAGE_V1 64",
            "#define LC_ARB_EXIT_INPUT_REJECTED_V1 65",
            "#define LC_ARB_EXIT_INPUT_LIMIT_V1 66",
            "#define LC_ARB_EXIT_OUTPUT_LIMIT_V1 67",
            "#define LC_ARB_EXIT_RESOURCE_LIMIT_V1 68",
            "#define LC_ARB_EXIT_INTERNAL_V1 70",
            "#define LC_ARB_EXIT_IO_V1 74",
        ):
            with self.subTest(declaration=declaration):
                self.assertIn(declaration, header)
        self.assertIn("UINT64_C(16) * UINT64_C(1024) * UINT64_C(1024)", header)
        for name in (
            "LC_ARB_MAX_JOB_BYTES_V1",
            "LC_ARB_MAX_OUTPUT_BYTES_V1",
            "LC_ARB_MAX_PRECISION_BITS_V1",
            "LC_ARB_MAX_POLICY_RUNGS_V1",
            "LC_ARB_MAX_KNOTS_V1",
        ):
            with self.subTest(name=name):
                self.assertIn(name, header)
                self.assertIn(name, main + wire)
        self.assertIn("LC_ARB_EVALUATION_RESOURCE_LIMIT", main)
        self.assertIn("limit_exceeded", main)
        self.assertIn("input.maximum", main)
        self.assertIn("output.maximum", main)
        self.assertNotIn("byte_buffer decisions", main)
        self.assertNotIn("byte_buffer witnesses", main)
        self.assertIn("append_digest_witness(output", main)
        self.assertIn("append_resource_witness(\n                    output", main)
        digest_appender = main[
            main.index("append_digest_witness(") : main.index("append_resource_witness(")
        ]
        resource_appender = main[
            main.index("append_resource_witness(") : main.index("lesser_u64(")
        ]
        self.assertIn("uint8_t record[37]", digest_appender)
        self.assertIn("buffer_append(output, record, sizeof(record))", digest_appender)
        self.assertNotIn("buffer_u8", digest_appender)
        self.assertIn("uint8_t record[22]", resource_appender)
        self.assertIn("buffer_append(output, record, sizeof(record))", resource_appender)
        self.assertNotIn("buffer_u8", resource_appender)
        self.assertIn("output_limit", main)
        reserve = main[main.index("buffer_reserve(") : main.index("buffer_append(")]
        self.assertLess(
            reserve.index("required > buffer->maximum"),
            reserve.index("realloc(buffer->bytes"),
        )
        reader = main[main.index("read_stdin(") : main.index("digest_is_nonzero(")]
        self.assertLess(
            reader.index("input->maximum"),
            reader.index("buffer_append(input"),
        )
        self.assertLess(
            wire.index("knot_count > LC_ARB_MAX_KNOTS_V1"),
            wire.index("lc_region_init(&job->region"),
        )
        self.assertLess(
            wire.index("rung_count > LC_ARB_MAX_POLICY_RUNGS_V1"),
            wire.index("policy->precision_ladder = calloc"),
        )
        read_failure = main[
            main.index("if (read_status != LC_ARB_READ_OK)") :
            main.index("if (!lc_parse_job")
        ]
        parse_failure_start = main.index("if (!lc_parse_job")
        parse_failure = main[
            parse_failure_start :
            main.index("lc_arb_evaluation_status", parse_failure_start)
        ]
        self.assertIn("read_status == LC_ARB_READ_ALLOCATION_FAILED", read_failure)
        self.assertIn("status = LC_ARB_EXIT_INTERNAL_V1", read_failure)
        self.assertIn("error == LC_WIRE_ALLOCATION_FAILED", parse_failure)
        self.assertIn("status = LC_ARB_EXIT_INTERNAL_V1", parse_failure)

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
    def test_closed_stdout_is_a_versioned_io_exit_not_an_untyped_signal(self) -> None:
        read_descriptor, write_descriptor = os.pipe()
        os.close(read_descriptor)
        with os.fdopen(write_descriptor, "wb") as output:
            process = subprocess.Popen(
                runtime_invocation(),
                stdin=subprocess.PIPE,
                stdout=output,
                stderr=subprocess.PIPE,
            )
            _stdout, stderr = process.communicate(
                runtime_profile_job().encode(),
                timeout=EVALUATOR_TIMEOUT_SECONDS,
            )
        self.assertEqual(process.returncode, arb_runtime.ARB_EXIT_IO_V1)
        self.assertEqual(stderr, b"result write failed\n")

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
                result = run_evaluator((executable, *arguments), b"")
                self.assertEqual(result.returncode, arb_runtime.ARB_EXIT_USAGE_V1)
                self.assertEqual(result.stdout, b"")

        accepted = run_evaluator(
            (
                executable,
                "--manifest-identity",
                valid,
                "--job",
                "/dev/stdin",
            ),
            b"",
        )
        self.assertEqual(accepted.returncode, arb_runtime.ARB_EXIT_INPUT_REJECTED_V1)
        self.assertEqual(accepted.stdout, b"")
        self.assertEqual(accepted.stderr, b"job read failed: empty_input\n")

    @unittest.skipUnless(
        os.environ.get("LABCOLORS_ARB_EVALUATOR"),
        "set LABCOLORS_ARB_EVALUATOR to the controlled C17 binary",
    )
    def test_job_transport_limit_precedes_wire_parsing(self) -> None:
        at_limit = run_evaluator(
            runtime_invocation(),
            bytes(arb_runtime.ARB_MAX_JOB_BYTES_V1),
        )
        self.assertEqual(
            at_limit.returncode,
            arb_runtime.ARB_EXIT_INPUT_REJECTED_V1,
        )
        self.assertEqual(at_limit.stdout, b"")
        self.assertEqual(at_limit.stderr, b"job rejected: bad_magic\n")

        over_limit = run_evaluator(
            runtime_invocation(),
            bytes(arb_runtime.ARB_MAX_JOB_BYTES_V1 + 1),
        )
        self.assertEqual(over_limit.returncode, arb_runtime.ARB_EXIT_INPUT_LIMIT_V1)
        self.assertEqual(over_limit.stdout, b"")
        self.assertEqual(over_limit.stderr, b"job read failed: input_limit\n")

    @unittest.skipUnless(
        os.environ.get("LABCOLORS_ARB_EVALUATOR"),
        "set LABCOLORS_ARB_EVALUATOR to the controlled C17 binary",
    )
    def test_allocation_profile_boundaries_are_enforced_by_the_native_parser(self) -> None:
        accepted = (
            runtime_profile_job(
                arb_ladder=(arb_runtime.ARB_MAX_PRECISION_BITS_V1,),
            ),
            runtime_profile_job(
                mpfi_ladder=(arb_runtime.ARB_MAX_PRECISION_BITS_V1,),
            ),
            runtime_profile_job(
                arb_ladder=tuple(range(1, arb_runtime.ARB_MAX_POLICY_RUNGS_V1 + 1)),
            ),
            runtime_profile_job(
                mpfi_ladder=tuple(range(1, arb_runtime.ARB_MAX_POLICY_RUNGS_V1 + 1)),
            ),
            runtime_profile_job(knot_count=arb_runtime.ARB_MAX_KNOTS_V1),
        )
        for index, job in enumerate(accepted):
            with self.subTest(boundary=index):
                result = run_evaluator(runtime_invocation(), job.encode())
                self.assertEqual(result.returncode, 0, result.stderr.decode())
                self.assertEqual(result.stderr, b"")
                self.assertEqual(DecisionTranscriptV1.parse(result.stdout).encode(), result.stdout)

        over_rungs = tuple(range(1, arb_runtime.ARB_MAX_POLICY_RUNGS_V1 + 2))
        rejected = (
            runtime_profile_job(
                arb_ladder=(arb_runtime.ARB_MAX_PRECISION_BITS_V1 + 1,),
            ),
            runtime_profile_job(
                mpfi_ladder=(arb_runtime.ARB_MAX_PRECISION_BITS_V1 + 1,),
            ),
            runtime_profile_job(arb_ladder=over_rungs),
            runtime_profile_job(mpfi_ladder=over_rungs),
            runtime_profile_job(knot_count=arb_runtime.ARB_MAX_KNOTS_V1 + 1),
        )
        for index, job in enumerate(rejected):
            with self.subTest(over_limit=index):
                result = run_evaluator(runtime_invocation(), job.encode())
                self.assertEqual(
                    result.returncode,
                    arb_runtime.ARB_EXIT_RESOURCE_LIMIT_V1,
                )
                self.assertEqual(result.stdout, b"")
                self.assertEqual(result.stderr, b"job rejected: resource_limit\n")

    @unittest.skipUnless(
        os.environ.get("LABCOLORS_ARB_EVALUATOR"),
        "set LABCOLORS_ARB_EVALUATOR to the controlled C17 binary",
    )
    def test_aggregate_transcript_output_limit_is_exact(self) -> None:
        frozen = ProofJobV1.parse(
            (REPO / "proof/region/v1/fixtures/proof-job-v1.bin").read_bytes()
        )
        policy = ProofPolicyV1(
            1,
            (
                ComparatorBudgetV1(ComparatorKindV1.ARB, (1,), 0, 0),
                ComparatorBudgetV1(ComparatorKindV1.MPFI, (1,), 0, 0),
            ),
        )

        def job(point_count: int) -> ProofJobV1:
            return ProofJobV1(
                frozen.definition,
                frozen.formula_spec,
                ReducedDomainManifestV1(((0, point_count),), point_count),
                policy,
            )

        accepted_points = 450_389
        accepted = run_evaluator(runtime_invocation(), job(accepted_points).encode())
        decision_bytes = (accepted_points + 3) // 4
        counters_offset = 120 + decision_bytes
        self.assertEqual(accepted.returncode, 0, accepted.stderr.decode())
        self.assertEqual(accepted.stderr, b"")
        self.assertEqual(len(accepted.stdout), 16_777_191)
        self.assertEqual(
            tuple(
                int.from_bytes(
                    accepted.stdout[offset : offset + 8],
                    "big",
                )
                for offset in range(counters_offset, counters_offset + 32, 8)
            ),
            (0, 0, accepted_points, 0),
        )

        rejected = run_evaluator(runtime_invocation(), job(450_390).encode())
        self.assertEqual(rejected.returncode, arb_runtime.ARB_EXIT_OUTPUT_LIMIT_V1)
        self.assertEqual(rejected.stdout, b"")
        self.assertEqual(rejected.stderr, b"evaluation failed: output_limit\n")

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
        result = run_evaluator(
            [
                executable,
                "--manifest-identity",
                manifest.identity.hex(),
                "--job",
                "/dev/stdin",
            ],
            job.encode(),
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
        alternate = run_evaluator(
            [
                executable,
                "--manifest-identity",
                alternate_manifest.identity.hex(),
                "--job",
                "/dev/stdin",
            ],
            job.encode(),
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
        rejected = run_evaluator(
            [
                executable,
                "--manifest-identity",
                manifest.identity.hex(),
                "--job",
                "/dev/stdin",
            ],
            bytes(corrupted),
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
        result = run_evaluator(
            (
                os.environ["LABCOLORS_ARB_EVALUATOR"],
                "--manifest-identity",
                manifest.identity.hex(),
                "--job",
                "/dev/stdin",
            ),
            job.encode(),
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
        first = run_evaluator(invocation, job.encode())
        second = run_evaluator(invocation, job.encode())

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
        low_first = run_evaluator(invocation, low_precision.encode())
        low_second = run_evaluator(invocation, low_precision.encode())
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
        result = run_evaluator(
            (
                os.environ["LABCOLORS_ARB_EVALUATOR"],
                "--manifest-identity",
                manifest.identity.hex(),
                "--job",
                "/dev/stdin",
            ),
            job.encode(),
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
        result = run_evaluator(
            (
                os.environ["LABCOLORS_ARB_EVALUATOR"],
                "--manifest-identity",
                manifest.identity.hex(),
                "--job",
                "/dev/stdin",
            ),
            job.encode(),
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
            result = run_evaluator(invocation, job.encode())
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
        result = run_evaluator(
            (
                os.environ["LABCOLORS_ARB_EVALUATOR"],
                "--manifest-identity",
                manifest.identity.hex(),
                "--job",
                "/dev/stdin",
            ),
            job.encode(),
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
        result = run_evaluator(
            (
                os.environ["LABCOLORS_ARB_EVALUATOR"],
                "--manifest-identity",
                manifest.identity.hex(),
                "--job",
                "/dev/stdin",
            ),
            job.encode(),
        )

        self.assertEqual(result.returncode, 0, result.stderr.decode())
        transcript = DecisionTranscriptV1.parse(result.stdout)
        self.assertEqual(transcript.encode(), result.stdout)
        self.assertEqual(transcript.job_identity, job.identity)


if __name__ == "__main__":
    unittest.main(verbosity=2)
