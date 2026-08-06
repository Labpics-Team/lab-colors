#!/usr/bin/env python3
"""One source-bound full-domain MPFI BUILD→RUN receipt and its evidence.

The MPFI half of the dual proof's full-domain gate: a controller-observed
native execution of the exact full 2^24 domain under the MPFI engine's own
BUILD-derived comparator, symmetric to the Arb lane in
`arb/tests/full_domain_receipt.py`.  The module stays outside the fast
`test_*.py` inventory, and `native_gate.py` pins its exact inventory under
the `full-domain-receipt` mode.
"""

from __future__ import annotations

import os
import sys
import tempfile
import unittest
from pathlib import Path


PROOF = Path(__file__).resolve().parents[2]
TESTS = PROOF / "tests"
MPFI_TESTS = Path(__file__).resolve().parent
# MPFI_TESTS leads so `test_receipt` resolves to the MPFI receipt fixture;
# the Arb test directory carries a same-named module and must not shadow it.
sys.path[:0] = [str(MPFI_TESTS), str(PROOF), str(TESTS)]

import corpus  # noqa: E402
import corpus_lane  # noqa: E402
import executor  # noqa: E402
import provenance  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402
from mpfi import receipt, runtime as mpfi_runtime  # noqa: E402
from test_mpfi_build import _limits_for_bundle  # noqa: E402
from test_receipt import _request  # noqa: E402


# The full 2^24 RUN is one continuous native execution: its wall envelope is
# an operational coordinate of this gate, not a property of the definition.
FULL_DOMAIN_WALL_TIMEOUT_NS_V1 = 6 * 60 * 60 * 1_000_000_000
FULL_DOMAIN_MEMORY_MAX_BYTES_V1 = 4 * 1024 * 1024 * 1024
EVIDENCE_OUT_ENV_V1 = "LABCOLORS_FULL_DOMAIN_EVIDENCE_OUT"


def _full_domain_binding() -> mpfi_runtime.MpfiRuntimeBindingV1:
    return mpfi_runtime.MpfiRuntimeBindingV1(
        mpfi_runtime.mpfi_runtime_profile_v1(),
        executor.ExecutionLimitsV1(
            16 * 1024 * 1024,
            16 * 1024 * 1024,
            4096,
            16 * 1024 * 1024,
            64 * 1024,
            FULL_DOMAIN_WALL_TIMEOUT_NS_V1,
            FULL_DOMAIN_MEMORY_MAX_BYTES_V1,
            1,
        ),
    )


@unittest.skipUnless(
    sys.platform == "linux"
    and os.environ.get("LABCOLORS_MPFI_DOCKER")
    and os.environ.get("LABCOLORS_EXECUTOR_CGROUP_V1")
    and os.environ.get("LABCOLORS_GMP_ARCHIVE")
    and os.environ.get("LABCOLORS_MPFR_ARCHIVE")
    and os.environ.get("LABCOLORS_MPFI_ARCHIVE"),
    "requires Linux, Docker, a delegated cgroup, and all three exact MPFI source archives",
)
class NativeMpfiSourceBoundFullDomainReceiptIntegrationTests(unittest.TestCase):
    def test_full_domain_build_run_seal_and_verification_evidence(self) -> None:
        source_lock = provenance.mpfi_source_lock_v1()
        archive_names = (
            "LABCOLORS_GMP_ARCHIVE",
            "LABCOLORS_MPFR_ARCHIVE",
            "LABCOLORS_MPFI_ARCHIVE",
        )
        safe = tuple(
            provenance.admit_source_archive(lock, Path(os.environ[name]).read_bytes())
            for lock, name in zip(source_lock.sources, archive_names, strict=True)
        )
        admitted = provenance.admit_mpfi_sources(source_lock, safe)
        base = _request()
        full_job = corpus.full_domain_job_v1(base.job)
        request = receipt.MpfiPipelineRequestV1(
            source_lock,
            admitted,
            base.build_sources,
            base.generated_formula,
            _limits_for_bundle(
                source_lock,
                admitted,
                base.build_sources,
                base.generated_formula,
            ),
            full_job,
            _full_domain_binding(),
        )
        result = receipt.MpfiSourceBoundControllerV1(
            Path(os.environ["LABCOLORS_MPFI_DOCKER"]),
            Path(os.environ["LABCOLORS_EXECUTOR_CGROUP_V1"]),
        ).execute(request)

        self.assertIs(type(result), receipt.MpfiSourceBoundEvaluatorReceiptV1, result)
        self.assertTrue(receipt.replay_mpfi_evidence_is_well_bound_v1(result.evidence))
        self.assertEqual(result.job.encode(), full_job.encode())
        transcript = result.transcript
        self.assertEqual(transcript.point_count, protocol.OUTPUT_CARDINALITY_V1)
        self.assertEqual(transcript.job_identity, full_job.identity)
        self.assertEqual(
            transcript.domain_identity,
            protocol.exact_full_domain_manifest_v1().identity,
        )
        # The engine echoes the coordinate it was told, and it is told the
        # comparator's source identity: the decision chain must stay portable
        # across runs whose build observations differ.
        self.assertEqual(
            transcript.comparator_identity,
            result.comparator.manifest.source_identity,
        )
        self.assertNotEqual(
            result.comparator.manifest.source_identity,
            result.comparator.manifest.identity,
        )

        evidence_out = os.environ.get(EVIDENCE_OUT_ENV_V1)
        if evidence_out:
            self._export_and_verify(result, full_job, Path(evidence_out))
        else:
            with tempfile.TemporaryDirectory(
                prefix="labcolors-mpfi-full-domain-evidence-"
            ) as temporary:
                self._export_and_verify(result, full_job, Path(temporary))

    def _export_and_verify(
        self,
        result: receipt.MpfiSourceBoundEvaluatorReceiptV1,
        full_job: protocol.ProofJobV1,
        out: Path,
    ) -> None:
        corpus_lane.write_verification_evidence_v1(result, out)
        self.assertEqual((out / "job.bin").read_bytes(), full_job.encode())
        loaded = corpus_lane.load_comparator_bundle_v1(out / "comparator-bundle")
        self.assertEqual(loaded.identity, result.comparator.manifest.identity)
        self.assertEqual(
            (out / "transcript.bin").read_bytes(), result.transcript.encode()
        )
        self.assertEqual(
            (out / "run-claim.bin").read_bytes(), result.run_claim.encode()
        )
        claim = protocol.RunClaimV1.parse((out / "run-claim.bin").read_bytes())
        self.assertEqual(claim.transcript_identity, result.transcript.identity)


if __name__ == "__main__":
    unittest.main(verbosity=2)
