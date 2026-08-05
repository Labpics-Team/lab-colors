#!/usr/bin/env python3
"""One source-bound full-domain BUILD→RUN receipt and its verification evidence.

The dual proof's full-domain gate needs exactly one thing the corpus lanes
cannot produce: a controller-observed native execution of the exact full
2^24 domain under the engine's own BUILD-derived comparator.  This module
carries that single long-running integration; it stays outside the fast
`test_*.py` inventory on purpose, and `native_gate.py` pins its exact
inventory under the `full-domain-receipt` mode.

The receipt's verification evidence is the lane runner's wire input — the
canonical job encoding and the comparator bundle — exported straight from
the receipt by `corpus_lane.write_verification_evidence_v1`, so the lanes
that later replay this RUN never re-derive a single coordinate.
"""

from __future__ import annotations

import os
import sys
import tempfile
import unittest
from pathlib import Path


PROOF = Path(__file__).resolve().parents[2]
ARB = PROOF / "arb"
TESTS = ARB / "tests"
sys.path[:0] = [str(PROOF), str(TESTS)]

import corpus  # noqa: E402
import corpus_lane  # noqa: E402
import provenance  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402
from arb import receipt  # noqa: E402
from test_pipeline import _job, _request, _runtime_binding  # noqa: E402


# The full 2^24 RUN is one continuous native execution: its wall envelope is
# an operational coordinate of this gate, not a property of the definition.
FULL_DOMAIN_WALL_TIMEOUT_NS_V1 = 6 * 60 * 60 * 1_000_000_000
FULL_DOMAIN_MEMORY_MAX_BYTES_V1 = 4 * 1024 * 1024 * 1024
EVIDENCE_OUT_ENV_V1 = "LABCOLORS_FULL_DOMAIN_EVIDENCE_OUT"


@unittest.skipUnless(
    sys.platform == "linux"
    and os.environ.get("LABCOLORS_ARB_PIPELINE_DOCKER")
    and os.environ.get("LABCOLORS_EXECUTOR_CGROUP_V1")
    and os.environ.get("LABCOLORS_GMP_ARCHIVE")
    and os.environ.get("LABCOLORS_MPFR_ARCHIVE")
    and os.environ.get("LABCOLORS_FLINT_ARCHIVE"),
    "requires Linux, Docker, a delegated cgroup, and all three exact source archives",
)
class NativeSourceBoundFullDomainReceiptIntegrationTests(unittest.TestCase):
    def test_full_domain_build_run_seal_and_verification_evidence(self) -> None:
        source_lock = provenance.arb_source_lock_v1()
        archive_names = (
            "LABCOLORS_GMP_ARCHIVE",
            "LABCOLORS_MPFR_ARCHIVE",
            "LABCOLORS_FLINT_ARCHIVE",
        )
        safe = tuple(
            provenance.admit_source_archive(lock, Path(os.environ[name]).read_bytes())
            for lock, name in zip(source_lock.sources, archive_names, strict=True)
        )
        admitted = provenance.admit_arb_sources(source_lock, safe)
        full_job = corpus.full_domain_job_v1(_job())
        request = _request(
            source_lock=source_lock,
            admitted_sources=admitted,
            job=full_job,
            runtime_binding=_runtime_binding(
                wall_timeout_ns=FULL_DOMAIN_WALL_TIMEOUT_NS_V1,
                memory_max_bytes=FULL_DOMAIN_MEMORY_MAX_BYTES_V1,
            ),
        )
        result = receipt.SourceBoundArbControllerV1(
            Path(os.environ["LABCOLORS_ARB_PIPELINE_DOCKER"]),
            Path(os.environ["LABCOLORS_EXECUTOR_CGROUP_V1"]),
        ).execute(request)

        self.assertIs(type(result), receipt.SourceBoundEvaluatorReceiptV1, result)
        self.assertTrue(receipt.replay_evidence_is_well_bound_v1(result.evidence))
        self.assertEqual(result.job.encode(), full_job.encode())
        transcript = result.transcript
        self.assertEqual(transcript.point_count, protocol.OUTPUT_CARDINALITY_V1)
        self.assertEqual(transcript.job_identity, full_job.identity)
        self.assertEqual(
            transcript.domain_identity,
            protocol.exact_full_domain_manifest_v1().identity,
        )
        self.assertEqual(
            transcript.comparator_identity,
            result.comparator.manifest.identity,
        )

        evidence_out = os.environ.get(EVIDENCE_OUT_ENV_V1)
        if evidence_out:
            self._export_and_verify(result, full_job, Path(evidence_out))
        else:
            with tempfile.TemporaryDirectory(
                prefix="labcolors-full-domain-evidence-"
            ) as temporary:
                self._export_and_verify(result, full_job, Path(temporary))

    def _export_and_verify(
        self,
        result: receipt.SourceBoundEvaluatorReceiptV1,
        full_job: protocol.ProofJobV1,
        out: Path,
    ) -> None:
        corpus_lane.write_verification_evidence_v1(result, out)
        self.assertEqual((out / "job.bin").read_bytes(), full_job.encode())
        loaded = corpus_lane.load_comparator_bundle_v1(out / "comparator-bundle")
        self.assertEqual(loaded.identity, result.comparator.manifest.identity)


if __name__ == "__main__":
    unittest.main(verbosity=2)
