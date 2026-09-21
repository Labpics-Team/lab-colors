#!/usr/bin/env python3
"""Fail-closed controls for the point-support retained-surplus proof."""

from __future__ import annotations

import hashlib
import json
import runpy
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
VERIFIER = REPO_ROOT / "scripts/verify_point_support_surplus.py"
PROOF = (
    REPO_ROOT
    / "crates/labcolors-core/contracts/point-support-reference-surplus-q55-bps-proof-v1.json"
)


class PointSupportSurplusSourceBindingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.verifier = runpy.run_path(str(VERIFIER), run_name="point_support_surplus_test")
        cls.sources = cls.verifier["read_source_cone"]()
        cls.point_path = cls.verifier["POINT_SOURCE"]
        cls.observation_path = cls.verifier["OBSERVATION_SOURCE"]
        cls.numerics_path = cls.verifier["NUMERICS_SOURCE"]
        cls.session_path = cls.verifier["SESSION_SOURCE"]
        cls.proof = json.loads(PROOF.read_text(encoding="utf-8-sig"))

    def test_committed_proof_is_canonical_and_replays(self) -> None:
        replayed = self.verifier["canonical_proof"]()
        self.assertEqual(replayed, self.proof)
        payload = dict(self.proof)
        payload_digest = payload.pop("proof_payload_sha256")
        canonical = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
        self.assertEqual(hashlib.sha256(canonical).hexdigest(), payload_digest)
        self.assertEqual(
            hashlib.sha256(VERIFIER.read_bytes()).hexdigest(),
            self.proof["verifier_sha256"],
        )

    def test_complete_production_dependency_cone_is_bound(self) -> None:
        digest = self.verifier["source_closure_digest"]
        mutate = self.verifier["mutate_source"]
        baseline = digest(self.sources)
        self.assertEqual(baseline, self.proof["source_closure_sha256"])

        regressions = (
            (
                self.point_path,
                b"matches!(self.decision, PointSupportStabilityDecisionV1::NotRetained)",
                b"false",
            ),
            (
                self.observation_path,
                b"        self.backing.set().values(case_index)\n",
                b"        None\n",
            ),
            (
                self.session_path,
                b"            Self::Observed(observation) => ObservationHeadViewV1::Observed(observation),\n",
                b"            Self::Observed(_) => ObservationHeadViewV1::Empty,\n",
            ),
            (
                self.numerics_path,
                b"proof_ids: [NumericalProofIdV2::PointSupportReferenceSurplusIntegerV1],\n            bound_status: Available",
                b"proof_ids: [NumericalProofIdV2::PointSupportReferenceSurplusIntegerV1],\n            bound_status: Unavailable",
            ),
        )
        for path, old, new in regressions:
            with self.subTest(path=path.name, old=old):
                self.assertNotEqual(digest(mutate(self.sources, path, old, new)), baseline)

        verified_digest, controls = self.verifier["verify_source_binding"]()
        self.assertEqual(verified_digest, baseline)
        self.assertEqual(controls, self.proof["source_negative_controls"])

    def test_complete_files_fail_closed_on_addition_removal_and_unknown_override(self) -> None:
        digest = self.verifier["source_closure_digest"]
        baseline = digest(self.sources)
        self.assertEqual(digest(dict(reversed(tuple(self.sources.items())))), baseline)
        for path in self.verifier["SOURCE_CONE_PATHS"]:
            with self.subTest(path=path.name):
                removed = dict(self.sources)
                removed[path] = b""
                self.assertNotEqual(digest(removed), baseline)
                appended = dict(self.sources)
                appended[path] += b"\nfn unbound_redirect() {}\n"
                self.assertNotEqual(digest(appended), baseline)

        unknown = dict(self.sources)
        unknown[REPO_ROOT / "unlisted.rs"] = b"fn redirect() {}\n"
        with self.assertRaises(AssertionError):
            digest(unknown)

        source_files = self.verifier["source_files"]
        globals_ = source_files.__globals__
        original_paths = globals_["SOURCE_CONE_PATHS"]
        try:
            globals_["SOURCE_CONE_PATHS"] = tuple(reversed(original_paths))
            self.assertEqual(digest(), baseline)
            globals_["SOURCE_CONE_PATHS"] = original_paths + (original_paths[0],)
            with self.assertRaises(AssertionError):
                source_files()
            globals_["SOURCE_CONE_PATHS"] = original_paths[:-1]
            self.assertNotEqual(digest(), baseline)
            globals_["SOURCE_CONE_PATHS"] = original_paths + (
                REPO_ROOT / ".." / "outside-semantic-cone.rs",
            )
            with self.assertRaises(AssertionError):
                source_files()
        finally:
            globals_["SOURCE_CONE_PATHS"] = original_paths

    def test_claim_is_lower_bound_specific_and_q55_dependency_is_exact(self) -> None:
        self.assertEqual(
            self.proof["certified_claim"],
            "for every successfully evaluated enabled stability cell, decision is Retained iff current_lower_surplus >= (10000-drop_bps)/10000 * max(baseline_lower_surplus,0); the declared anchor remains a separate hard floor",
        )
        self.assertIn("does not certify", self.proof["excluded_claim"])
        dependency = self.verifier["verify_q55_dependency"]()
        self.assertEqual(dependency, self.proof["q55_dependency"])
        self.assertEqual(dependency["maximum_luminance_upper"], (1 << 55) + 3)

    def test_universal_certificate_separates_scales_and_rejects_symbolic_mutants(
        self,
    ) -> None:
        algebra = self.proof["universal_algebraic_certificate"]
        self.assertEqual(algebra["basis_point_scale_instantiation"], 10_000)
        self.assertIn("Q55 scale Q>0", algebra["domain"])
        self.assertIn("basis-point scale B>0 instantiated as 10000", algebra["domain"])
        self.assertNotIn("Q55 scale S", algebra["domain"])
        self.assertEqual(
            algebra["symbolic_mutation_controls"],
            {
                "anchor_coefficients_and_denominator": 6,
                "retained_cross_product": 5,
            },
        )
        self.assertIn(
            "positive-baseline retained threshold is p*(B-drop)/(q*B)",
            algebra["identities"],
        )
        self.assertIn(
            "a/b >= p*(B-drop)/(q*B) iff a*q*B >= p*(B-drop)*b",
            algebra["identities"],
        )


class PointSupportSurplusCliTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        verifier = runpy.run_path(str(VERIFIER), run_name="point_support_cli_test")
        cls.snapshot = {
            path.relative_to(REPO_ROOT): path.read_bytes()
            for path in (*verifier["SOURCE_CONE_PATHS"], VERIFIER, PROOF)
        }

    def assert_cli(
        self,
        expected_success: bool,
        changes: dict[Path, bytes | None] | None = None,
    ) -> None:
        # Настоящий CLI читает изолированный filesystem snapshot: подмена
        # digest-helper не обнаружила бы прежний обход проверки в main().
        with tempfile.TemporaryDirectory(prefix="point-support-cli-") as directory:
            root = Path(directory)
            files = {**self.snapshot, **(changes or {})}
            for relative, content in files.items():
                if content is None:
                    continue
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(content)
            proof_path = root / PROOF.relative_to(REPO_ROOT)
            proof_before = proof_path.read_bytes()
            result = subprocess.run(
                [sys.executable, str(root / VERIFIER.relative_to(REPO_ROOT))],
                cwd=root,
                capture_output=True,
                text=True,
                timeout=30,
                check=False,
            )
            self.assertEqual(proof_path.read_bytes(), proof_before)
            if expected_success:
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.encode("utf-8"), proof_before)
            else:
                self.assertNotEqual(result.returncode, 0, result.stderr)
                self.assertNotIn("verification: PASS", result.stderr)

    def test_cli_replays_exact_source_without_writing_the_receipt(self) -> None:
        self.assert_cli(True)

    def test_cli_rejects_a_changed_retention_law_with_the_old_receipt(self) -> None:
        path = Path("crates/labcolors-core/src/point_support.rs")
        original = self.snapshot[path]
        old = b"DROP_BASIS_POINTS_SCALE - self.0"
        self.assertEqual(original.count(old), 1)
        self.assert_cli(False, {path: original.replace(old, b"self.0", 1)})

    def test_cli_rejects_a_self_consistent_but_false_numerical_receipt(self) -> None:
        path = PROOF.relative_to(REPO_ROOT)
        forged = json.loads(self.snapshot[path])
        forged["integer_replay_envelope"]["required_numerator_max"] = 0
        forged.pop("proof_payload_sha256")
        canonical = json.dumps(forged, sort_keys=True, separators=(",", ":"))
        forged["proof_payload_sha256"] = hashlib.sha256(canonical.encode("utf-8")).hexdigest()
        encoded = json.dumps(forged, sort_keys=True, separators=(",", ":")) + "\n"
        self.assert_cli(False, {path: encoded.encode("utf-8")})

    def test_cli_rejects_verifier_revision_drift(self) -> None:
        path = VERIFIER.relative_to(REPO_ROOT)
        self.assert_cli(False, {path: self.snapshot[path] + b"\n# revision drift\n"})

    def test_cli_requires_the_recorded_source_files(self) -> None:
        self.assert_cli(False, {Path("crates/labcolors-core/src/point_support.rs"): None})


if __name__ == "__main__":
    unittest.main(verbosity=2)
