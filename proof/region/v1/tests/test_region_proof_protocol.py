#!/usr/bin/env python3
"""Hostile contract canonical offline proof protocol V1."""

from __future__ import annotations

import dataclasses
import hashlib
import io
import itertools
import json
import struct
import sys
import tempfile
import unittest
from contextlib import redirect_stderr
from dataclasses import fields as dataclass_fields
from dataclasses import make_dataclass, replace
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

import region_proof_protocol as protocol  # noqa: E402
import controller as fixture_controller  # noqa: E402
from controller import (  # noqa: E402
    ControllerErrorV1,
    FrozenInputV1,
    _read_frozen,
    verify_fixtures,
)

from region_proof_protocol import (  # noqa: E402
    FORMULA_RELEASE_DOMAIN_V1,
    ComparatorBudgetV1,
    ComparatorKindV1,
    BUILD_OBSERVATION_COORDINATES_V2,
    ComparatorManifestV2,
    BoundaryUnprovenWitnessV1,
    ContextualRegionDefinitionV1,
    DecisionTranscriptV1,
    DecisionV1,
    DualComparisonCandidateV1,
    DualComparisonClaimV1,
    ExactZeroSignalTraceV1,
    ProofJobV1,
    ProofPolicyV1,
    ProtocolErrorV1,
    ProtocolReasonV1,
    ReducedDomainManifestV1,
    ResourceLimitWitnessV1,
    RunClaimV1,
    WitnessStoreV1,
    ContentResolvedComparatorManifestV2,
    compare_dual_transcripts,
    source_bound_coordinates_v2,
    encode_contextual_definition_fields_v1,
)


FIXTURES = ROOT / "fixtures"
REPO = ROOT.parents[2]
FORMULA = REPO / "crates/labcolors-core/contracts/contextual-region-formula-v1.lcir"
DEFINITION_DIGEST = bytes.fromhex(
    "0a8d1c3d2f0052be84b5783071699861aad0ac83dae62de3275267754681cdc9"
)
FORMULA_RELEASE = bytes.fromhex(
    "2c626d8ee60eeb62ae4db53660d61bbc25e0efd4e557f0dc1e77565c130b6e52"
)
DOMAIN_IDENTITY = bytes.fromhex(
    "2d9eabe87d53106f1c5e226e8ab8cf14f4811af0318ce2a09bf89c55c69ec513"
)
POLICY_IDENTITY = bytes.fromhex(
    "3497bf0f8b3ec6c315c9bd58f0701f206595ac88fa1cff2c8235109ef9f44662"
)
JOB_IDENTITY = bytes.fromhex(
    "6e493856d3c81f0d5b12bf1221985c66210ae98c8b6c79c7c5b4aabf243c0116"
)
MANIFEST_IDENTITY = bytes.fromhex(
    "805c3710b9b38189f4b9c0bb69aaf429c944637a4ee38d1ffa56ee2d72ec09d9"
)
TRANSCRIPT_IDENTITY = bytes.fromhex(
    "0eabb02a15ca0b0773d3391b31ec2760c453d1c2d9d8351781927c0507632a09"
)
RUN_CLAIM_IDENTITY = bytes.fromhex(
    "fa5acb5c57da5e6d8c2321e5c0b9ee0c0646e916bdcd264e12fa457d4be3798f"
)
COMPARISON_IDENTITY = bytes.fromhex(
    "f3a3245c4ed5f00b0f83cfb516ec3d59df7fb4df6a6adb2aefd523860c93b278"
)


class ComparatorSourceIdentityTests(unittest.TestCase):
    """Every manifest coordinate lands on exactly one side of the split."""

    def _manifest(self, **changes: bytes) -> ComparatorManifestV2:
        base = manifest(ComparatorKindV1.ARB, 300).manifest
        values = {
            item.name: changes.get(item.name, getattr(base, item.name))
            for item in dataclasses.fields(base)
            if item.name != "kind"
        }
        return ComparatorManifestV2(base.kind, **values)

    def test_the_split_covers_the_manifest_exactly(self) -> None:
        coordinates = {
            item.name
            for item in dataclasses.fields(ComparatorManifestV2)
            if item.name != "kind"
        }
        source = set(source_bound_coordinates_v2())
        observation = set(BUILD_OBSERVATION_COORDINATES_V2)
        self.assertEqual(source | observation, coordinates)
        self.assertEqual(source & observation, set())
        self.assertTrue(observation.issubset(coordinates))
        # Anti-vacuity: an empty observation set would satisfy the partition
        # above while turning the loop over observation coordinates into zero
        # subTests, so the split would look proven and test nothing.
        self.assertTrue(observation)
        self.assertTrue(source)

    def test_every_source_coordinate_moves_the_source_identity(self) -> None:
        # Without this, a coordinate dropped from the fold would silently stop
        # distinguishing comparators and no test would notice.
        base = self._manifest()
        for name in source_bound_coordinates_v2():
            with self.subTest(coordinate=name):
                drifted = self._manifest(
                    **{name: hashlib.sha256(name.encode()).digest()}
                )
                self.assertNotEqual(base.source_identity, drifted.source_identity)
                self.assertNotEqual(base.identity, drifted.identity)

    def test_the_observation_coordinates_move_only_the_full_identity(self) -> None:
        base = self._manifest()
        for name in BUILD_OBSERVATION_COORDINATES_V2:
            with self.subTest(coordinate=name):
                drifted = self._manifest(
                    **{name: hashlib.sha256(b"observed-" + name.encode()).digest()}
                )
                self.assertEqual(base.source_identity, drifted.source_identity)
                self.assertNotEqual(base.identity, drifted.identity)

    def test_the_engine_kind_is_bound(self) -> None:
        # A lane admitted under the other engine's budget would replay the
        # wrong ladder, so the fold must separate the engines.
        arb = manifest(ComparatorKindV1.ARB, 300).manifest
        mpfi = ComparatorManifestV2(
            ComparatorKindV1.MPFI,
            *(
                getattr(arb, item.name)
                for item in dataclasses.fields(arb)
                if item.name != "kind"
            ),
        )
        self.assertNotEqual(arb.source_identity, mpfi.source_identity)


class ForeignTuple(tuple):
    pass


def digest(label: int) -> bytes:
    return hashlib.sha256(f"protocol-test-{label}".encode("ascii")).digest()


SYNTHETIC_CONTENT = {
    digest(index): f"protocol-test-{index}".encode("ascii")
    for index in range(1_000)
}


def admit_manifest(
    value: ComparatorManifestV2,
) -> ContentResolvedComparatorManifestV2:
    return ContentResolvedComparatorManifestV2.admit(
        value,
        SYNTHETIC_CONTENT.get,
    )


def expect_reason(
    testcase: unittest.TestCase,
    reason: ProtocolReasonV1,
    function,
) -> None:
    with testcase.assertRaises(ProtocolErrorV1) as caught:
        function()
    testcase.assertEqual(caught.exception.reason, reason)


def fixture_definition() -> ContextualRegionDefinitionV1:
    return ContextualRegionDefinitionV1.parse(
        (FIXTURES / "v5b2b-definition-0a8d1c3d.bin").read_bytes()
    )


def fixture_domain() -> ReducedDomainManifestV1:
    return ReducedDomainManifestV1.parse(
        (FIXTURES / "reduced-domain-srgb8-seams-v1.bin").read_bytes()
    )


def fixture_policy() -> ProofPolicyV1:
    return ProofPolicyV1.parse(
        (FIXTURES / "proof-policy-protocol-v1.bin").read_bytes()
    )


def manifest(kind: ComparatorKindV1, seed: int) -> ContentResolvedComparatorManifestV2:
    return admit_manifest(
        ComparatorManifestV2(
            kind=kind,
            engine_release=digest(seed),
            upstream_source=digest(seed + 1),
            arithmetic_input_set=digest(seed + 2),
            wrapper_source=digest(seed + 3),
            evaluator_source=digest(seed + 4),
            build_identity=digest(seed + 5),
            operation_allowlist=digest(seed + 6),
            test_observation=digest(seed + 7),
            legal_file_set=digest(seed + 8),
            exclusions=digest(seed + 9),
        )
    )


def witness_store(*witnesses) -> WitnessStoreV1:
    return WitnessStoreV1.from_witnesses(witnesses)


class DefinitionAndJobTests(unittest.TestCase):
    def test_controller_admits_real_nested_fixtures_and_rejects_corruption(self) -> None:
        evidence = verify_fixtures(REPO)
        self.assertEqual(evidence["admitted_files"], 5)
        self.assertEqual(evidence["definition_fields"], 30)
        self.assertEqual(evidence["domain_points"], 512)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / "fixture.bin"
            path.write_bytes(b"bad")
            frozen = FrozenInputV1(
                "fixture.bin",
                3,
                hashlib.sha256(b"good").hexdigest(),
            )
            with self.assertRaises(ControllerErrorV1):
                _read_frozen(root, frozen)
            target = root / "target.bin"
            target.write_bytes(b"bad")
            path.unlink()
            path.symlink_to(target)
            with self.assertRaises(ControllerErrorV1):
                _read_frozen(root, frozen)

            actual_directory = root / "actual-directory"
            actual_directory.mkdir()
            nested = actual_directory / "nested.bin"
            nested.write_bytes(b"good")
            directory_alias = root / "directory-alias"
            directory_alias.symlink_to(actual_directory, target_is_directory=True)
            with self.assertRaises(ControllerErrorV1):
                _read_frozen(
                    root,
                    FrozenInputV1(
                        "directory-alias/nested.bin",
                        4,
                        hashlib.sha256(b"good").hexdigest(),
                    ),
                )

            missing = FrozenInputV1(
                "missing.bin",
                0,
                hashlib.sha256(b"").hexdigest(),
            )
            with self.assertRaises(ControllerErrorV1):
                _read_frozen(root, missing)

            actual_root = root / "actual"
            actual_root.mkdir()
            actual_fixture = actual_root / "fixture.bin"
            actual_fixture.write_bytes(b"good")
            root_alias = root / "root-alias"
            root_alias.symlink_to(actual_root, target_is_directory=True)
            aliased = FrozenInputV1(
                "fixture.bin",
                4,
                hashlib.sha256(b"good").hexdigest(),
            )
            self.assertEqual(_read_frozen(root_alias, aliased), b"good")

            with patch.object(
                fixture_controller.os,
                "open",
                side_effect=PermissionError("host detail must not escape"),
            ):
                with self.assertRaises(ControllerErrorV1):
                    _read_frozen(actual_root, aliased)

            values = (0, 1, 10, 11, 127, 128, 254, 255)
            seam_cube = tuple(
                (red << 16) | (green << 8) | blue
                for red, green, blue in itertools.product(values, repeat=3)
            )

            class ShortDomain:
                point_count = 512

                @staticmethod
                def iter_ordinals():
                    return iter(seam_cube[:-1])

            class LongDomain:
                point_count = 512

                @staticmethod
                def iter_ordinals():
                    return iter((*seam_cube, seam_cube[-1] + 1))

            admitted_job = ProofJobV1.parse(
                (FIXTURES / "proof-job-v1.bin").read_bytes()
            )
            for drifted_domain in (ShortDomain(), LongDomain()):
                with (
                    patch.object(
                        fixture_controller.ReducedDomainManifestV1,
                        "parse",
                        return_value=drifted_domain,
                    ),
                    patch.object(
                        fixture_controller.ProofJobV1,
                        "parse",
                        return_value=admitted_job,
                    ),
                ):
                    with self.assertRaises(ControllerErrorV1):
                        verify_fixtures(REPO)

            stderr = io.StringIO()
            with (
                patch.object(
                    fixture_controller,
                    "verify_fixtures",
                    side_effect=ControllerErrorV1("fixture rejected"),
                ),
                redirect_stderr(stderr),
            ):
                self.assertEqual(
                    fixture_controller.main(
                        ["verify-fixtures", "--repo-root", str(root)]
                    ),
                    1,
                )
            self.assertEqual(
                json.loads(stderr.getvalue()),
                {"error": "fixture rejected", "status": "protocol-fixtures-rejected"},
            )

    def test_v5b2b_definition_fixture_reencodes_byte_identically(self) -> None:
        raw = (FIXTURES / "v5b2b-definition-0a8d1c3d.bin").read_bytes()
        definition = ContextualRegionDefinitionV1.parse(raw)

        self.assertEqual(len(raw), 451)
        self.assertEqual(definition.encode(), raw)
        self.assertEqual(definition.definition_digest, DEFINITION_DIGEST)
        self.assertEqual(definition.formula_release, FORMULA_RELEASE)
        self.assertEqual(len(definition.fields), 22 + 4 * definition.knot_count)

        for knot_count, knots in (
            (1, definition.fields[22:26]),
            (
                3,
                definition.fields[22:30]
                + (
                    struct.pack(">d", 3.0),
                    definition.fields[27],
                    definition.fields[28],
                    definition.fields[29],
                ),
            ),
        ):
            fields = list(definition.fields[:22])
            fields[21] = knot_count.to_bytes(8, "big")
            fields.extend(knots)
            encoded = encode_contextual_definition_fields_v1(tuple(fields))
            parsed = ContextualRegionDefinitionV1.parse(encoded)
            self.assertEqual(parsed.knot_count, knot_count)
            self.assertEqual(parsed.encode(), encoded)
            self.assertEqual(len(parsed.fields), 22 + 4 * knot_count)

    def test_definition_rejects_hardcoded_count_trailing_and_noncanonical_shape(self) -> None:
        raw = (FIXTURES / "v5b2b-definition-0a8d1c3d.bin").read_bytes()
        expect_reason(
            self,
            ProtocolReasonV1.TRAILING_BYTES,
            lambda: ContextualRegionDefinitionV1.parse(raw + b"\0"),
        )
        fields = list(fixture_definition().fields)
        fields[21] = (3).to_bytes(8, "big")
        expect_reason(
            self,
            ProtocolReasonV1.COUNT_MISMATCH,
            lambda: ContextualRegionDefinitionV1.parse(
                encode_contextual_definition_fields_v1(tuple(fields))
            ),
        )
        definition = fixture_definition()
        expect_reason(
            self,
            ProtocolReasonV1.COUNT_MISMATCH,
            lambda: ContextualRegionDefinitionV1(
                ForeignTuple(definition.fields),  # type: ignore[arg-type]
                definition.knot_count,
            ),
        )

    def test_definition_accepts_all_surrounds_and_enforces_context_domains(self) -> None:
        baseline = list(fixture_definition().fields)
        for surround in (1, 2, 3):
            fields = list(baseline)
            fields[13] = bytes((surround,))
            parsed = ContextualRegionDefinitionV1.parse(
                encode_contextual_definition_fields_v1(tuple(fields))
            )
            self.assertEqual(parsed.fields[13], bytes((surround,)))

        for field_index, value in (
            (11, 0.0),
            (11, float("inf")),
            (12, 0.0),
            (12, 1.0000000000000002),
        ):
            fields = list(baseline)
            fields[field_index] = struct.pack(">d", value)
            expect_reason(
                self,
                ProtocolReasonV1.INVALID_DEFINITION,
                lambda fields=fields: ContextualRegionDefinitionV1.parse(
                    encode_contextual_definition_fields_v1(tuple(fields))
                ),
            )

    def test_definition_knot_cardinality_is_bounded_by_bytes_not_an_ad_hoc_cap(self) -> None:
        knot_count = 65_537
        fields = list(fixture_definition().fields[:22])
        fields[21] = knot_count.to_bytes(8, "big")
        zero = struct.pack(">d", 0.0)
        for tone in range(knot_count):
            fields.extend((struct.pack(">d", float(tone)), zero, zero, zero))
        encoded = encode_contextual_definition_fields_v1(tuple(fields))
        parsed = ContextualRegionDefinitionV1.parse(encoded)

        self.assertEqual(parsed.knot_count, knot_count)
        self.assertEqual(len(parsed.fields), 22 + 4 * knot_count)
        self.assertEqual(parsed.encode(), encoded)
        fields = list(fixture_definition().fields)
        fields[18] = (0).to_bytes(8, "big")
        expect_reason(
            self,
            ProtocolReasonV1.INVALID_DEFINITION,
            lambda: ContextualRegionDefinitionV1.parse(
                encode_contextual_definition_fields_v1(tuple(fields))
            ),
        )
        direct_fields = fixture_definition().fields[:26]
        expect_reason(
            self,
            ProtocolReasonV1.COUNT_MISMATCH,
            lambda: ContextualRegionDefinitionV1(direct_fields, knot_count=1),
        )

    def test_committed_job_fixture_reencodes_and_replays_nested_digests(self) -> None:
        raw = (FIXTURES / "proof-job-v1.bin").read_bytes()
        sha256 = protocol.hashlib.sha256
        formula_release_calls = 0

        def count_formula_release(data=b"", *args, **kwargs):
            nonlocal formula_release_calls
            if data.startswith(FORMULA_RELEASE_DOMAIN_V1):
                formula_release_calls += 1
            return sha256(data, *args, **kwargs)

        with patch.object(protocol.hashlib, "sha256", new=count_formula_release):
            job = ProofJobV1.parse(raw)
        self.assertEqual(formula_release_calls, 1)

        definition_length = int.from_bytes(raw[40:48], "big")
        formula_offset = 48 + definition_length + 32 + 8
        corrupted = bytearray(raw)
        corrupted[formula_offset] ^= 1
        formula_release_calls = 0
        with patch.object(protocol.hashlib, "sha256", new=count_formula_release):
            expect_reason(
                self,
                ProtocolReasonV1.DIGEST_MISMATCH,
                lambda: ProofJobV1.parse(bytes(corrupted)),
            )
        self.assertEqual(formula_release_calls, 1)
        formula = FORMULA.read_bytes()

        self.assertEqual(job.encode(), raw)
        self.assertEqual(job.definition.definition_digest, DEFINITION_DIGEST)
        self.assertEqual(job.formula_spec, formula)
        self.assertEqual(
            hashlib.sha256(
                FORMULA_RELEASE_DOMAIN_V1
                + len(formula).to_bytes(8, "big")
                + formula
            ).digest(),
            FORMULA_RELEASE,
        )
        self.assertEqual(job.domain.point_count, 512)
        self.assertEqual(job.domain.identity, DOMAIN_IDENTITY)
        self.assertEqual(job.policy.identity, POLICY_IDENTITY)
        self.assertEqual(job.identity, JOB_IDENTITY)
        self.assertTrue(all(item.global_pregrant == 0 for item in job.policy.comparators))

        class DomainSubclass(ReducedDomainManifestV1):
            pass

        class PolicySubclass(ProofPolicyV1):
            pass

        for foreign_domain, foreign_policy in (
            (
                DomainSubclass(job.domain.ranges, job.domain.point_count),
                job.policy,
            ),
            (
                job.domain,
                PolicySubclass(
                    job.policy.equality_release,
                    job.policy.comparators,
                ),
            ),
        ):
            expect_reason(
                self,
                ProtocolReasonV1.INVALID_DEFINITION,
                lambda foreign_domain=foreign_domain, foreign_policy=foreign_policy: ProofJobV1(
                    job.definition,
                    job.formula_spec,
                    foreign_domain,
                    foreign_policy,
                ),
            )

    def test_job_rejects_huge_declared_blob_before_slice_and_any_trailing_byte(self) -> None:
        raw = bytearray((FIXTURES / "proof-job-v1.bin").read_bytes())
        raw[8 + 32 : 8 + 32 + 8] = (2**63).to_bytes(8, "big")
        expect_reason(
            self,
            ProtocolReasonV1.LENGTH_OUT_OF_BOUNDS,
            lambda: ProofJobV1.parse(bytes(raw)),
        )
        expect_reason(
            self,
            ProtocolReasonV1.TRAILING_BYTES,
            lambda: ProofJobV1.parse(
                (FIXTURES / "proof-job-v1.bin").read_bytes() + b"\0"
            ),
        )


class DomainAndPolicyTests(unittest.TestCase):
    def test_reduced_domain_is_exact_seam_cube_and_canonical(self) -> None:
        values = (0, 1, 10, 11, 127, 128, 254, 255)
        ordinals = sorted((red << 16) | (green << 8) | blue for red, green, blue in itertools.product(values, repeat=3))
        domain = ReducedDomainManifestV1.from_ordinals(ordinals)
        raw = (FIXTURES / "reduced-domain-srgb8-seams-v1.bin").read_bytes()

        self.assertEqual(domain.point_count, 512)
        self.assertEqual(domain.encode(), raw)
        self.assertEqual(ReducedDomainManifestV1.parse(raw), domain)
        self.assertTrue(all(left[1] < right[0] for left, right in zip(domain.ranges, domain.ranges[1:])))

    def test_domain_rejects_empty_reorder_overlap_adjacency_and_wrong_count(self) -> None:
        expect_reason(
            self,
            ProtocolReasonV1.EMPTY_DOMAIN,
            lambda: ReducedDomainManifestV1.from_ordinals(()),
        )
        for ranges, count, reason in (
            ((), 0, ProtocolReasonV1.EMPTY_DOMAIN),
            (((10, 12), (1, 2)), 3, ProtocolReasonV1.NONCANONICAL_ORDER),
            (((1, 4), (3, 6)), 6, ProtocolReasonV1.OVERLAPPING_RANGE),
            (((1, 4), (4, 6)), 5, ProtocolReasonV1.ADJACENT_RANGE),
            (((1, 4), (6, 8)), 6, ProtocolReasonV1.COUNT_MISMATCH),
        ):
            expect_reason(
                self,
                reason,
                lambda ranges=ranges, count=count: ReducedDomainManifestV1(
                    ranges=ranges,
                    point_count=count,
                ),
            )
        expect_reason(
            self,
            ProtocolReasonV1.INVALID_RANGE,
            lambda: ReducedDomainManifestV1([(0, 1)], 1),  # type: ignore[arg-type]
        )
        expect_reason(
            self,
            ProtocolReasonV1.INVALID_RANGE,
            lambda: ReducedDomainManifestV1(  # type: ignore[arg-type]
                ForeignTuple((((0, 1)),)),
                1,
            ),
        )

        impossible_dense_header = b"".join(
            (
                protocol.DOMAIN_MAGIC_V1,
                b"\x01",
                protocol.OUTPUT_CARDINALITY_V1.to_bytes(8, "big"),
                (2).to_bytes(8, "big"),
            )
        )
        with patch.object(
            protocol._Reader,
            "u32",
            side_effect=AssertionError("impossible range count reached records"),
        ):
            expect_reason(
                self,
                ProtocolReasonV1.LENGTH_OUT_OF_BOUNDS,
                lambda: ReducedDomainManifestV1.parse(impossible_dense_header),
            )

        invalid_first_record = b"".join(
            (
                protocol.DOMAIN_MAGIC_V1,
                b"\x01",
                (3).to_bytes(8, "big"),
                (2).to_bytes(8, "big"),
                (1).to_bytes(4, "big"),
                (1).to_bytes(4, "big"),
                (2).to_bytes(4, "big"),
                (3).to_bytes(4, "big"),
            )
        )
        original_u32 = protocol._Reader.u32
        reads = 0

        def reject_third_record_read(reader) -> int:
            nonlocal reads
            reads += 1
            if reads > 2:
                raise AssertionError("parser read past invalid first range")
            return original_u32(reader)

        with patch.object(protocol._Reader, "u32", new=reject_third_record_read):
            expect_reason(
                self,
                ProtocolReasonV1.INVALID_RANGE,
                lambda: ReducedDomainManifestV1.parse(invalid_first_record),
            )
        self.assertEqual(reads, 2)

        expect_reason(
            self,
            ProtocolReasonV1.TRUNCATED,
            lambda: ReducedDomainManifestV1.parse(
                (FIXTURES / "reduced-domain-srgb8-seams-v1.bin").read_bytes()[:-1]
            ),
        )

    def test_policy_requires_strict_ladders_and_accepts_zero_grant(self) -> None:
        policy = fixture_policy()
        self.assertEqual(ProofPolicyV1.parse(policy.encode()), policy)
        self.assertTrue(all(item.global_pregrant == 0 for item in policy.comparators))
        expect_reason(
            self,
            ProtocolReasonV1.NONCANONICAL_ORDER,
            lambda: ProofPolicyV1(
                equality_release=1,
                comparators=(
                    ComparatorBudgetV1(ComparatorKindV1.ARB, (128, 128), 1, 0),
                    ComparatorBudgetV1(ComparatorKindV1.MPFI, (128, 256), 1, 0),
                ),
            ),
        )
        expect_reason(
            self,
            ProtocolReasonV1.INVALID_POLICY,
            lambda: ComparatorBudgetV1(1, (64,), 0, 0),  # type: ignore[arg-type]
        )
        expect_reason(
            self,
            ProtocolReasonV1.INVALID_POLICY,
            lambda: ComparatorBudgetV1(  # type: ignore[arg-type]
                ComparatorKindV1.ARB,
                ForeignTuple((64, 128)),
                0,
                0,
            ),
        )

        class BudgetSubclass(ComparatorBudgetV1):
            pass

        first = policy.comparators[0]
        foreign_budget = BudgetSubclass(
            first.kind,
            first.precision_ladder,
            first.per_point_work,
            first.global_pregrant,
        )
        expect_reason(
            self,
            ProtocolReasonV1.INVALID_POLICY,
            lambda: ProofPolicyV1(
                1,
                (foreign_budget, policy.comparators[1]),
            ),
        )
        expect_reason(
            self,
            ProtocolReasonV1.INVALID_POLICY,
            lambda: ProofPolicyV1(  # type: ignore[arg-type]
                1,
                ForeignTuple(policy.comparators),
            ),
        )

    def test_policy_rejects_first_ladder_count_before_it_can_consume_second_record(self) -> None:
        raw = bytearray((FIXTURES / "proof-policy-protocol-v1.bin").read_bytes())
        raw[11:15] = (12).to_bytes(4, "big")
        expect_reason(
            self,
            ProtocolReasonV1.LENGTH_OUT_OF_BOUNDS,
            lambda: ProofPolicyV1.parse(bytes(raw)),
        )


class ManifestTranscriptComparisonTests(unittest.TestCase):
    def test_protocol_slice_cannot_name_structural_agreement_a_proof(self) -> None:
        self.assertFalse(hasattr(protocol, "DualProofReceiptV1"))

    def test_manifest_v1_surface_is_hard_deleted(self) -> None:
        for name in (
            "MANIFEST_MAGIC_V1",
            "MANIFEST_ID_LABEL_V1",
            "ComparatorManifestV1",
            "ContentResolvedComparatorManifestV1",
        ):
            with self.subTest(name=name):
                self.assertFalse(hasattr(protocol, name))

    def test_manifest_v2_rejects_the_historical_v1_wire_domain(self) -> None:
        coordinates = tuple(digest(index) for index in range(10, 20))
        legacy_wire = (
            b"LCMAN1\0\0"
            + bytes((int(ComparatorKindV1.ARB),))
            + b"".join(coordinates)
        )

        expect_reason(
            self,
            ProtocolReasonV1.BAD_MAGIC,
            lambda: protocol.ComparatorManifestV2.parse(legacy_wire),
        )

    def test_manifest_v2_has_a_distinct_wire_and_identity_domain(self) -> None:
        current = protocol.ComparatorManifestV2(
            ComparatorKindV1.ARB,
            *(digest(index) for index in range(10, 20)),
        )
        encoded = current.encode()

        self.assertEqual(encoded[:8], b"LCMAN2\0\0")
        self.assertEqual(
            current.identity,
            hashlib.sha256(
                b"labcolors.proof-region.comparator-manifest.v2\0"
                + len(encoded).to_bytes(8, "big")
                + encoded
            ).digest(),
        )
        legacy_wire = b"LCMAN1\0\0" + encoded[8:]
        legacy_identity = hashlib.sha256(
            b"labcolors.proof-region.comparator-manifest.v1\0"
            + len(legacy_wire).to_bytes(8, "big")
            + legacy_wire
        ).digest()
        self.assertNotEqual(current.identity, legacy_identity)

    def test_manifest_is_content_resolved_and_each_field_changes_identity(self) -> None:
        base = manifest(ComparatorKindV1.ARB, 10)
        self.assertEqual(
            admit_manifest(
                ComparatorManifestV2.parse(base.manifest.encode())
            ).identity,
            base.identity,
        )
        for field in (
            "engine_release",
            "upstream_source",
            "arithmetic_input_set",
            "wrapper_source",
            "evaluator_source",
            "build_identity",
            "operation_allowlist",
            "test_observation",
            "legal_file_set",
            "exclusions",
        ):
            changed = admit_manifest(
                replace(
                    base.manifest,
                    **{field: digest(100 + len(field))},
                )
            )
            self.assertNotEqual(changed.identity, base.identity, field)
            coordinate = getattr(base.manifest, field)
            expect_reason(
                self,
                ProtocolReasonV1.INVALID_MANIFEST,
                lambda coordinate=coordinate: ContentResolvedComparatorManifestV2.admit(
                    base.manifest,
                    lambda current: (
                        None
                        if current == coordinate
                        else SYNTHETIC_CONTENT.get(current)
                    ),
                ),
            )
            expect_reason(
                self,
                ProtocolReasonV1.DIGEST_MISMATCH,
                lambda coordinate=coordinate: ContentResolvedComparatorManifestV2.admit(
                    base.manifest,
                    lambda current: (
                        b"wrong"
                        if current == coordinate
                        else SYNTHETIC_CONTENT.get(current)
                    ),
                ),
            )

        expect_reason(
            self,
            ProtocolReasonV1.UNKNOWN_RELEASE,
            lambda: ComparatorManifestV2(
                3,  # type: ignore[arg-type]
                *(digest(index) for index in range(300, 310)),
            ),
        )
        lookalike_type = make_dataclass(
            "ComparatorManifestLookalike",
            tuple((field.name, object) for field in dataclass_fields(base.manifest)),
            frozen=True,
        )
        lookalike = lookalike_type(
            *(getattr(base.manifest, field.name) for field in dataclass_fields(base.manifest))
        )
        expect_reason(
            self,
            ProtocolReasonV1.INVALID_MANIFEST,
            lambda: ContentResolvedComparatorManifestV2.admit(  # type: ignore[arg-type]
                lookalike,
                SYNTHETIC_CONTENT.get,
            ),
        )
        with self.assertRaises(TypeError):
            ContentResolvedComparatorManifestV2()  # type: ignore[call-arg]
        class ForeignBytes(bytes):
            pass

        expect_reason(
            self,
            ProtocolReasonV1.INVALID_DIGEST,
            lambda: replace(
                base.manifest,
                engine_release=ForeignBytes(base.manifest.engine_release),
            ),
        )
        for invalid_content in (True, 7, object()):
            expect_reason(
                self,
                ProtocolReasonV1.INVALID_MANIFEST,
                lambda invalid_content=invalid_content: ContentResolvedComparatorManifestV2.admit(
                    base.manifest,
                    lambda _coordinate: invalid_content,  # type: ignore[return-value]
                ),
            )

    def test_transcript_msb_bits_counters_and_exact_witness_are_nonvacuous(self) -> None:
        job = ProofJobV1.parse((FIXTURES / "proof-job-v1.bin").read_bytes())
        arb = manifest(ComparatorKindV1.ARB, 20)
        decisions = (DecisionV1.INSIDE, DecisionV1.OUTSIDE) * 256
        equality = ExactZeroSignalTraceV1(
            ordinal=next(job.domain.iter_ordinals()),
            trace_digest=digest(200),
        )
        transcript = DecisionTranscriptV1.from_decisions(
            job=job,
            comparator=arb,
            decisions=decisions,
            witnesses=(equality,),
            accounting_digest=digest(201),
        )
        encoded = transcript.encode()
        reparsed = DecisionTranscriptV1.parse(encoded)

        self.assertEqual(reparsed, transcript)
        self.assertEqual(transcript.decision_bits[0], 0b00010001)
        self.assertEqual(transcript.counters, (256, 256, 0, 0))
        self.assertEqual(transcript.exact_equality_count, 1)
        self.assertFalse(hasattr(transcript, "witnesses"))
        self.assertEqual(tuple(reparsed.iter_witnesses()), (equality,))
        self.assertEqual(
            transcript.identity,
            hashlib.sha256(
                protocol.TRANSCRIPT_ID_LABEL_V1
                + len(encoded).to_bytes(8, "big")
                + encoded
            ).digest(),
        )
        self.assertEqual(
            tuple(reparsed.iter_decisions()),
            decisions,
        )
        expect_reason(
            self,
            ProtocolReasonV1.MISSING_EQUALITY_WITNESS,
            lambda: DecisionTranscriptV1.from_decisions(
                job=job,
                comparator=arb,
                decisions=decisions,
                witnesses=(),
                accounting_digest=digest(201),
                exact_equality_count=1,
            ),
        )

    def test_transcript_rejects_padding_and_witness_counts_before_records(self) -> None:
        fixture_job = ProofJobV1.parse((FIXTURES / "proof-job-v1.bin").read_bytes())
        single_job = ProofJobV1(
            fixture_job.definition,
            fixture_job.formula_spec,
            ReducedDomainManifestV1(((0, 1),), 1),
            fixture_job.policy,
        )
        arb = manifest(ComparatorKindV1.ARB, 22)
        transcript = DecisionTranscriptV1.from_decisions(
            single_job,
            arb,
            (DecisionV1.INSIDE,),
            (),
            digest(202),
        )
        expect_reason(
            self,
            ProtocolReasonV1.INVALID_TRANSCRIPT,
            lambda: DecisionTranscriptV1(
                transcript.job_identity,
                transcript.domain_identity,
                transcript.comparator_identity,
                1,
                b"\x01",
                transcript.counters,
                transcript.exact_equality_count,
                transcript.accounting_digest,
                witness_store(),
            ),
        )
        expect_reason(
            self,
            ProtocolReasonV1.INVALID_TRANSCRIPT,
            lambda: DecisionTranscriptV1(
                transcript.job_identity,
                transcript.domain_identity,
                transcript.comparator_identity,
                1,
                b"\x00",
                ForeignTuple(transcript.counters),  # type: ignore[arg-type]
                transcript.exact_equality_count,
                transcript.accounting_digest,
                witness_store(),
            ),
        )
        expect_reason(
            self,
            ProtocolReasonV1.INVALID_TRANSCRIPT,
            lambda: DecisionTranscriptV1(
                transcript.job_identity,
                transcript.domain_identity,
                transcript.comparator_identity,
                1,
                bytearray(b"\x00"),  # type: ignore[arg-type]
                transcript.counters,
                transcript.exact_equality_count,
                transcript.accounting_digest,
                witness_store(),
            ),
        )
        expect_reason(
            self,
            ProtocolReasonV1.INVALID_TRANSCRIPT,
            lambda: ExactZeroSignalTraceV1(0.5, digest(209)),  # type: ignore[arg-type]
        )
        expect_reason(
            self,
            ProtocolReasonV1.INVALID_TRANSCRIPT,
            lambda: WitnessStoreV1(memoryview(b""), 0),  # type: ignore[arg-type]
        )

        encoded = bytearray(transcript.encode())
        witness_count_offset = len(encoded) - 8
        encoded[witness_count_offset : witness_count_offset + 8] = (1).to_bytes(8, "big")
        encoded.extend(b"\0" * 22)
        with patch.object(
            protocol,
            "_scan_witness_body_v1",
            side_effect=AssertionError("record parser ran before count preflight"),
        ):
            expect_reason(
                self,
                ProtocolReasonV1.COUNT_MISMATCH,
                lambda: DecisionTranscriptV1.parse(bytes(encoded)),
            )

        expect_reason(
            self,
            ProtocolReasonV1.MISSING_EQUALITY_WITNESS,
            lambda: DecisionTranscriptV1(
                digest(210),
                digest(211),
                digest(212),
                2,
                b"\x10",
                (1, 1, 0, 0),
                2,
                digest(213),
                witness_store(
                    ExactZeroSignalTraceV1(0, digest(214)),
                    ExactZeroSignalTraceV1(1, digest(215)),
                ),
            ),
        )
        expect_reason(
            self,
            ProtocolReasonV1.COUNT_MISMATCH,
            lambda: DecisionTranscriptV1(
                digest(216),
                digest(217),
                digest(218),
                1,
                b"\x80",
                (0, 0, 1, 0),
                0,
                digest(219),
                witness_store(
                    ResourceLimitWitnessV1(0, scope=1, granted=0, consumed=0)
                ),
            ),
        )

        for witnesses in (
            (
                ExactZeroSignalTraceV1(0, digest(220)),
                ExactZeroSignalTraceV1(0, digest(221)),
            ),
            (
                ExactZeroSignalTraceV1(1, digest(222)),
                ExactZeroSignalTraceV1(0, digest(223)),
            ),
        ):
            expect_reason(
                self,
                ProtocolReasonV1.NONCANONICAL_ORDER,
                lambda witnesses=witnesses: WitnessStoreV1.from_witnesses(
                    witnesses
                ),
            )
        expect_reason(
            self,
            ProtocolReasonV1.UNKNOWN_RELEASE,
            lambda: WitnessStoreV1.from_witnesses(
                (object(),)  # type: ignore[arg-type]
            ),
        )

        resource = DecisionTranscriptV1.from_decisions(
            single_job,
            arb,
            (DecisionV1.RESOURCE_LIMIT_REACHED,),
            (ResourceLimitWitnessV1(0, scope=1, granted=0, consumed=0),),
            digest(232),
        )
        boundary = DecisionTranscriptV1.from_decisions(
            single_job,
            arb,
            (DecisionV1.BOUNDARY_UNPROVEN,),
            (BoundaryUnprovenWitnessV1(0, digest(233)),),
            digest(234),
        )
        resource_wire = resource.encode()
        parsed_resource = DecisionTranscriptV1.parse(resource_wire)
        self.assertIsNone(parsed_resource.witness_store._hash)
        self.assertEqual(hash(resource), hash(parsed_resource))
        cached_hash = parsed_resource.witness_store._hash
        self.assertIsNotNone(cached_hash)
        with patch.object(
            WitnessStoreV1,
            "body_view",
            side_effect=AssertionError("cached hash rescanned witness body"),
        ):
            self.assertEqual(hash(parsed_resource.witness_store), cached_hash)
        with patch.object(
            protocol,
            "ResourceLimitWitnessV1",
            side_effect=AssertionError("transcript parse materialised witness objects"),
        ):
            self.assertEqual(
                DecisionTranscriptV1.parse(resource_wire).encode(),
                resource_wire,
            )
        for hostile_wire, reason in (
            (boundary.encode()[:-1], ProtocolReasonV1.TRUNCATED),
            (resource_wire + b"\0", ProtocolReasonV1.TRAILING_BYTES),
        ):
            with patch.object(
                protocol,
                "_scan_witness_body_v1",
                side_effect=AssertionError("record parser ran before body preflight"),
            ):
                expect_reason(
                    self,
                    reason,
                    lambda hostile_wire=hostile_wire: DecisionTranscriptV1.parse(
                        hostile_wire
                    ),
                )
        encoded = bytearray(resource.encode())
        encoded[-22] = 9
        with self.assertRaises(ProtocolErrorV1) as caught:
            DecisionTranscriptV1.parse(bytes(encoded))
        self.assertEqual(caught.exception.reason, ProtocolReasonV1.UNKNOWN_RELEASE)
        self.assertEqual(caught.exception.artifact, "transcript-v1")
        self.assertEqual(caught.exception.offset, len(encoded) - 22)

        encoded = bytearray(transcript.encode())
        decision_offset = len(protocol.TRANSCRIPT_MAGIC_V1) + 3 * 32 + 8 + 8
        counters_offset = decision_offset + 1
        equality_offset = counters_offset + 4 * 8
        witness_count_offset = equality_offset + 8 + 32
        encoded[decision_offset] = 0b1100_0000
        encoded[counters_offset : counters_offset + 4 * 8] = (
            (0).to_bytes(8, "big") * 3 + (1).to_bytes(8, "big")
        )
        encoded[equality_offset : equality_offset + 8] = (1).to_bytes(8, "big")
        encoded[witness_count_offset : witness_count_offset + 8] = (2).to_bytes(8, "big")
        encoded.extend(b"\0" * 44)
        expect_reason(
            self,
            ProtocolReasonV1.COUNT_MISMATCH,
            lambda: DecisionTranscriptV1.parse(bytes(encoded)),
        )

    def test_unresolved_alignment_is_single_pass_and_extra_witness_is_rejected(self) -> None:
        job = ProofJobV1.parse((FIXTURES / "proof-job-v1.bin").read_bytes())

        class CountingDomain:
            def __init__(self, wrapped: ReducedDomainManifestV1):
                self.wrapped = wrapped
                self.calls = 0
                self.point_count = wrapped.point_count
                self.identity = wrapped.identity
                self.ranges = wrapped.ranges

            def index_of(self, ordinal: int) -> int | None:
                return self.wrapped.index_of(ordinal)

            def iter_ordinals(self):
                self.calls += 1
                if self.calls > 1:
                    raise AssertionError("domain was materialised or rescanned")
                yield from self.wrapped.iter_ordinals()

        domain = CountingDomain(job.domain)

        class CountingJob:
            identity = job.identity

            def __init__(self):
                self.domain = domain

        decisions = (DecisionV1.RESOURCE_LIMIT_REACHED,) * job.domain.point_count
        class OneShotWitnesses:
            def __init__(self):
                self.consumed = False

            def __iter__(self):
                if self.consumed:
                    raise AssertionError("witness iterable was consumed twice")
                self.consumed = True
                for ordinal in job.domain.iter_ordinals():
                    yield ResourceLimitWitnessV1(
                        ordinal,
                        scope=1,
                        granted=0,
                        consumed=0,
                    )

        witnesses = OneShotWitnesses()
        with patch.object(
            WitnessStoreV1,
            "iter_witnesses",
            side_effect=AssertionError("alignment materialised typed witnesses"),
        ):
            transcript = DecisionTranscriptV1.from_decisions(
                CountingJob(),
                manifest(ComparatorKindV1.ARB, 24),
                decisions,
                witnesses,
                digest(203),
            )
        self.assertTrue(witnesses.consumed)
        self.assertEqual(domain.calls, 0)
        self.assertEqual(transcript.counters, (0, 0, 0, 512))

        first = next(job.domain.iter_ordinals())
        extra = ResourceLimitWitnessV1(first, scope=1, granted=0, consumed=0)
        expect_reason(
            self,
            ProtocolReasonV1.COUNT_MISMATCH,
            lambda: DecisionTranscriptV1.from_decisions(
                job,
                manifest(ComparatorKindV1.ARB, 25),
                (DecisionV1.INSIDE,) * job.domain.point_count,
                (extra,),
                digest(204),
            ),
        )

    def test_unresolved_or_shared_diversity_paths_cannot_produce_comparison(self) -> None:
        job = ProofJobV1.parse((FIXTURES / "proof-job-v1.bin").read_bytes())
        arb = manifest(ComparatorKindV1.ARB, 30)
        mpfi = manifest(ComparatorKindV1.MPFI, 50)
        decisions = (DecisionV1.INSIDE,) * 511 + (DecisionV1.RESOURCE_LIMIT_REACHED,)
        ordinal = job.domain.ranges[-1][1] - 1
        witness = ResourceLimitWitnessV1(ordinal=ordinal, scope=1, granted=0, consumed=0)
        ta = DecisionTranscriptV1.from_decisions(
            job, arb, decisions, (witness,), digest(300)
        )
        tb = DecisionTranscriptV1.from_decisions(
            job, mpfi, decisions, (witness,), digest(301)
        )
        ra = RunClaimV1.for_transcript(job, arb, ta, digest(302), digest(303), digest(304))
        rb = RunClaimV1.for_transcript(job, mpfi, tb, digest(305), digest(306), digest(307))
        expect_reason(
            self,
            ProtocolReasonV1.UNRESOLVED_TRANSCRIPT,
            lambda: compare_dual_transcripts(job, arb, ta, ra, mpfi, tb, rb),
        )

        for coordinate in (
            "engine_release",
            "upstream_source",
            "wrapper_source",
            "evaluator_source",
        ):
            shared_coordinate = admit_manifest(
                replace(
                    mpfi.manifest,
                    **{coordinate: getattr(arb.manifest, coordinate)},
                )
            )
            expect_reason(
                self,
                ProtocolReasonV1.SHARED_DIVERSITY_COORDINATE,
                lambda shared_coordinate=shared_coordinate: compare_dual_transcripts(
                    job, arb, ta, ra, shared_coordinate, tb, rb
                ),
            )

        shared_binary = RunClaimV1.for_transcript(
            job,
            mpfi,
            tb,
            ra.binary_identity,
            digest(306),
            digest(307),
        )
        expect_reason(
            self,
            ProtocolReasonV1.SHARED_DIVERSITY_COORDINATE,
            lambda: compare_dual_transcripts(
                job, arb, ta, ra, mpfi, tb, shared_binary
            ),
        )

    def test_dual_admission_rejects_every_foreign_binding_and_reverse_order(self) -> None:
        job = ProofJobV1.parse((FIXTURES / "proof-job-v1.bin").read_bytes())
        arb = manifest(ComparatorKindV1.ARB, 60)
        mpfi = manifest(ComparatorKindV1.MPFI, 80)
        decisions = (DecisionV1.INSIDE, DecisionV1.OUTSIDE) * 256
        ta = DecisionTranscriptV1.from_decisions(job, arb, decisions, (), digest(320))
        tb = DecisionTranscriptV1.from_decisions(job, mpfi, decisions, (), digest(321))
        ra = RunClaimV1.for_transcript(
            job, arb, ta, digest(322), digest(323), digest(324)
        )
        rb = RunClaimV1.for_transcript(
            job, mpfi, tb, digest(325), digest(326), digest(327)
        )

        for foreign_transcript, foreign_run in (
            (replace(ta, job_identity=digest(330)), ra),
            (replace(ta, domain_identity=digest(331)), ra),
            (replace(ta, comparator_identity=digest(332)), ra),
            (ta, replace(ra, job_identity=digest(333))),
            (ta, replace(ra, comparator_identity=digest(334))),
            (ta, replace(ra, transcript_identity=digest(335))),
        ):
            expect_reason(
                self,
                ProtocolReasonV1.FOREIGN_BINDING,
                lambda foreign_transcript=foreign_transcript, foreign_run=foreign_run: compare_dual_transcripts(
                    job,
                    arb,
                    foreign_transcript,
                    foreign_run,
                    mpfi,
                    tb,
                    rb,
                ),
            )

        expect_reason(
            self,
            ProtocolReasonV1.NONCANONICAL_ORDER,
            lambda: compare_dual_transcripts(job, mpfi, tb, rb, arb, ta, ra),
        )

    def test_synthetic_resolved_transcripts_produce_only_structural_comparison(self) -> None:
        job = ProofJobV1.parse((FIXTURES / "proof-job-v1.bin").read_bytes())
        arb = manifest(ComparatorKindV1.ARB, 70)
        mpfi = manifest(ComparatorKindV1.MPFI, 90)
        decisions = (DecisionV1.INSIDE, DecisionV1.OUTSIDE) * 256
        ta = DecisionTranscriptV1.from_decisions(job, arb, decisions, (), digest(400))
        tb = DecisionTranscriptV1.from_decisions(job, mpfi, decisions, (), digest(401))
        ra = RunClaimV1.for_transcript(job, arb, ta, digest(402), digest(403), digest(404))
        rb = RunClaimV1.for_transcript(job, mpfi, tb, digest(405), digest(406), digest(407))

        with patch.object(
            WitnessStoreV1,
            "iter_witnesses",
            side_effect=AssertionError("dual comparison materialised typed witnesses"),
        ):
            candidate = compare_dual_transcripts(job, arb, ta, ra, mpfi, tb, rb)
        # These independent constants pin each domain-separated identity label;
        # deriving the oracle through protocol constants would let all labels drift.
        for artifact, length, wire_sha256, identity in (
            (
                arb.manifest,
                329,
                "884625d5983131234d0570f90b482e0e9de2801b773f7f03e34eb2138b104c30",
                MANIFEST_IDENTITY,
            ),
            (
                ta,
                328,
                "18cf0246ff4d655e7cc3bd2bbab51eaf1bc7a5fde192d58556c90aa3404edf3d",
                TRANSCRIPT_IDENTITY,
            ),
            (
                ra,
                200,
                "ff2b8be1848e61dbfbcd552881824b5eaec0c06c0b65111350fef8d7da549fee",
                RUN_CLAIM_IDENTITY,
            ),
            (
                candidate,
                368,
                "4834b22578490ec6005ddd4b30ba2b008adb3a44a928a175a92dcabed3af24b2",
                COMPARISON_IDENTITY,
            ),
        ):
            encoded = artifact.encode()
            self.assertEqual(len(encoded), length)
            self.assertEqual(hashlib.sha256(encoded).hexdigest(), wire_sha256)
            self.assertEqual(artifact.identity, identity)
        raw_claim = DualComparisonClaimV1.parse(candidate.encode())
        self.assertEqual(raw_claim, candidate.claim)
        self.assertIs(type(candidate), DualComparisonCandidateV1)
        self.assertIsNot(type(raw_claim), DualComparisonCandidateV1)
        self.assertFalse(hasattr(DualComparisonCandidateV1, "parse"))
        with self.assertRaises(TypeError):
            DualComparisonCandidateV1(raw_claim)  # type: ignore[call-arg]
        with self.assertRaises(TypeError):
            DualComparisonCandidateV1()  # type: ignore[call-arg]
        self.assertEqual(candidate.claim.domain_identity, job.domain.identity)
        self.assertEqual(candidate.claim.domain_point_count, 512)
        self.assertFalse(hasattr(candidate, "family_image"))
        self.assertFalse(hasattr(candidate, "bitmap"))
        expect_reason(
            self,
            ProtocolReasonV1.COUNT_MISMATCH,
            lambda: DualComparisonClaimV1(
                candidate.claim.job_identity,
                candidate.claim.definition_digest,
                candidate.claim.domain_identity,
                candidate.claim.policy_identity,
                candidate.claim.domain_point_count,
                (candidate.claim.comparator_identities[0],),  # type: ignore[arg-type]
                candidate.claim.run_claim_identities,
                candidate.claim.transcript_identities,
                candidate.claim.decision_digest,
            ),
        )
        expect_reason(
            self,
            ProtocolReasonV1.COUNT_MISMATCH,
            lambda: DualComparisonClaimV1(
                candidate.claim.job_identity,
                candidate.claim.definition_digest,
                candidate.claim.domain_identity,
                candidate.claim.policy_identity,
                candidate.claim.domain_point_count,
                ForeignTuple(  # type: ignore[arg-type]
                    candidate.claim.comparator_identities
                ),
                candidate.claim.run_claim_identities,
                candidate.claim.transcript_identities,
                candidate.claim.decision_digest,
            ),
        )
        expect_reason(
            self,
            ProtocolReasonV1.LENGTH_OUT_OF_BOUNDS,
            lambda: DualComparisonClaimV1(
                candidate.claim.job_identity,
                candidate.claim.definition_digest,
                candidate.claim.domain_identity,
                candidate.claim.policy_identity,
                1.5,  # type: ignore[arg-type]
                candidate.claim.comparator_identities,
                candidate.claim.run_claim_identities,
                candidate.claim.transcript_identities,
                candidate.claim.decision_digest,
            ),
        )

        divergent = DecisionTranscriptV1.from_decisions(
            job,
            mpfi,
            (DecisionV1.OUTSIDE,) + decisions[1:],
            (),
            digest(408),
        )
        divergent_run = RunClaimV1.for_transcript(
            job, mpfi, divergent, digest(405), digest(406), digest(407)
        )
        expect_reason(
            self,
            ProtocolReasonV1.DISAGREEMENT,
            lambda: compare_dual_transcripts(
                job, arb, ta, ra, mpfi, divergent, divergent_run
            ),
        )

        equality_ordinal = next(job.domain.iter_ordinals())
        equality_a = ExactZeroSignalTraceV1(equality_ordinal, digest(410))
        equality_b = ExactZeroSignalTraceV1(equality_ordinal, digest(411))
        equality_ta = DecisionTranscriptV1.from_decisions(
            job, arb, decisions, (equality_a,), digest(412)
        )
        equality_tb = DecisionTranscriptV1.from_decisions(
            job, mpfi, decisions, (equality_b,), digest(413)
        )
        equality_ra = RunClaimV1.for_transcript(
            job, arb, equality_ta, digest(414), digest(415), digest(416)
        )
        equality_rb = RunClaimV1.for_transcript(
            job, mpfi, equality_tb, digest(417), digest(418), digest(419)
        )
        expect_reason(
            self,
            ProtocolReasonV1.DISAGREEMENT,
            lambda: compare_dual_transcripts(
                job,
                arb,
                equality_ta,
                equality_ra,
                mpfi,
                equality_tb,
                equality_rb,
            ),
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
