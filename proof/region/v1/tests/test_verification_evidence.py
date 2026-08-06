#!/usr/bin/env python3
"""Hostile contract for the verification evidence export (V5b2d-3e).

A source-bound engine RUN must hand the verification lanes everything the
semantic assembly consumes: the job bytes the evaluator ran under, the
comparator bundle the lane runner replays under, and the engine's sealed run
coordinates — the decision transcript and the run claim — because
`assemble_semantic_verification_v1` binds the lane-reassembled replay to the
engine transcript through exactly those two wire objects.  The receipt
already carries all four; the export writes them verbatim with the binding
checked, so nothing is re-derived and no second source of truth is created.
"""

from __future__ import annotations

import hashlib
import sys
import tempfile
import unittest
from dataclasses import dataclass, fields
from pathlib import Path
from types import SimpleNamespace

PROOF = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PROOF))

import corpus_lane  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402

FIXTURE_JOB_V1 = PROOF / "fixtures" / "proof-job-v1.bin"
PREIMAGE_FIELDS = (
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
)


def _digest(label: object) -> bytes:
    return hashlib.sha256(f"verification-evidence-{label}".encode("ascii")).digest()


@dataclass(frozen=True)
class EvidencePreimages:
    engine_release: bytes
    upstream_source: bytes
    arithmetic_input_set: bytes
    wrapper_source: bytes
    evaluator_source: bytes
    build_identity: bytes
    operation_allowlist: bytes
    test_observation: bytes
    legal_file_set: bytes
    exclusions: bytes


def _preimages(tag: str) -> EvidencePreimages:
    return EvidencePreimages(
        *(f"{tag}-{name}".encode("ascii") for name in PREIMAGE_FIELDS)
    )


def _contents(preimages: EvidencePreimages) -> dict[bytes, bytes]:
    values = tuple(getattr(preimages, field.name) for field in fields(preimages))
    return {hashlib.sha256(value).digest(): value for value in values}


def _comparator(tag: str) -> SimpleNamespace:
    preimages = _preimages(tag)
    contents = _contents(preimages)
    manifest = protocol.ComparatorManifestV2(
        protocol.ComparatorKindV1.ARB,
        *(
            hashlib.sha256(getattr(preimages, field.name)).digest()
            for field in fields(preimages)
        ),
    )
    resolved = protocol.ContentResolvedComparatorManifestV2.admit(
        manifest, contents.get
    )
    return SimpleNamespace(preimages=preimages, manifest=resolved)


def _transcript(
    job: protocol.ProofJobV1,
    comparator_manifest: protocol.ContentResolvedComparatorManifestV2,
) -> protocol.DecisionTranscriptV1:
    point_count = job.domain.point_count
    return protocol.DecisionTranscriptV1(
        job.identity,
        job.domain.identity,
        comparator_manifest.source_identity,
        point_count,
        b"\x00" * ((point_count * 2 + 7) // 8),
        (point_count, 0, 0, 0),
        0,
        _digest("accounting"),
        protocol.WitnessStoreV1(b"", 0),
    )


def _run_claim(
    job: protocol.ProofJobV1,
    comparator_manifest: protocol.ContentResolvedComparatorManifestV2,
    transcript: protocol.DecisionTranscriptV1,
) -> protocol.RunClaimV1:
    return protocol.RunClaimV1.for_transcript(
        job,
        comparator_manifest,
        transcript,
        _digest("binary"),
        _digest("invocation"),
        _digest("platform"),
    )


def _receipt(tag: str) -> SimpleNamespace:
    job = protocol.ProofJobV1.parse(FIXTURE_JOB_V1.read_bytes())
    comparator = _comparator(tag)
    transcript = _transcript(job, comparator.manifest)
    run_claim = _run_claim(job, comparator.manifest, transcript)
    return SimpleNamespace(
        job=job, comparator=comparator, transcript=transcript, run_claim=run_claim
    )


class VerificationEvidenceExportTests(unittest.TestCase):
    def test_export_writes_job_bytes_and_the_comparator_bundle(self) -> None:
        receipt = _receipt("evidence-honest")
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "evidence"
            corpus_lane.write_verification_evidence_v1(receipt, out)
            self.assertEqual(
                (out / "job.bin").read_bytes(), receipt.job.encode()
            )
            loaded = corpus_lane.load_comparator_bundle_v1(
                out / "comparator-bundle"
            )
            self.assertEqual(loaded.identity, receipt.comparator.manifest.identity)
            contents = _contents(receipt.comparator.preimages)
            for address, content in contents.items():
                self.assertEqual(
                    (out / "comparator-bundle" / "content" / address.hex()).read_bytes(),
                    content,
                )

    def test_export_writes_the_run_transcript_and_run_claim_verbatim(self) -> None:
        receipt = _receipt("evidence-run")
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "evidence"
            corpus_lane.write_verification_evidence_v1(receipt, out)
            self.assertEqual(
                (out / "transcript.bin").read_bytes(), receipt.transcript.encode()
            )
            self.assertEqual(
                (out / "run-claim.bin").read_bytes(), receipt.run_claim.encode()
            )
            transcript = protocol.DecisionTranscriptV1.parse(
                (out / "transcript.bin").read_bytes()
            )
            self.assertEqual(transcript.job_identity, receipt.job.identity)
            # The engine records the comparator's source identity; the
            # bundle beside it still carries the full manifest, so the
            # environment record travels with the evidence.
            self.assertEqual(
                transcript.comparator_identity,
                receipt.comparator.manifest.source_identity,
            )
            # Anti-vacuity: the two identities really differ, so the
            # assertion above is not comparing a value with itself.
            self.assertNotEqual(
                receipt.comparator.manifest.source_identity,
                receipt.comparator.manifest.identity,
            )
            claim = protocol.RunClaimV1.parse((out / "run-claim.bin").read_bytes())
            self.assertEqual(claim.job_identity, receipt.job.identity)
            self.assertEqual(
                claim.comparator_identity,
                receipt.comparator.manifest.source_identity,
            )
            self.assertEqual(claim.transcript_identity, transcript.identity)

    def test_foreign_receipt_shapes_are_typed_rejections(self) -> None:
        honest = _receipt("evidence-foreign")
        foreign_shapes = (
            object(),
            SimpleNamespace(job=honest.job),
            SimpleNamespace(job=honest.job, comparator=object()),
            SimpleNamespace(
                job=honest.job,
                comparator=SimpleNamespace(manifest=honest.comparator.manifest),
            ),
            SimpleNamespace(job=object(), comparator=honest.comparator),
            # The run coordinates are part of the wire evidence: a receipt
            # without them cannot feed the semantic assembly.
            SimpleNamespace(job=honest.job, comparator=honest.comparator),
            SimpleNamespace(
                job=honest.job,
                comparator=honest.comparator,
                transcript=object(),
            ),
            SimpleNamespace(
                job=honest.job,
                comparator=honest.comparator,
                transcript=honest.transcript,
            ),
            SimpleNamespace(
                job=honest.job,
                comparator=honest.comparator,
                transcript=honest.transcript,
                run_claim=object(),
            ),
            SimpleNamespace(
                job=honest.job,
                comparator=honest.comparator,
                transcript=object(),
                run_claim=honest.run_claim,
            ),
        )
        for foreign in foreign_shapes:
            with tempfile.TemporaryDirectory() as tmp:
                with self.assertRaises(
                    protocol.ProtocolErrorV1,
                    msg=f"foreign receipt shape was not rejected: {foreign!r}",
                ):
                    corpus_lane.write_verification_evidence_v1(
                        foreign, Path(tmp) / "evidence"
                    )

    def test_preimages_that_do_not_bind_the_manifest_refuse(self) -> None:
        receipt = _receipt("evidence-drift")
        drifted = EvidencePreimages(
            *(
                b"surrogate" if index == 0 else getattr(receipt.comparator.preimages, name)
                for index, name in enumerate(PREIMAGE_FIELDS)
            )
        )
        receipt.comparator.preimages = drifted
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(protocol.ProtocolErrorV1):
                corpus_lane.write_verification_evidence_v1(
                    receipt, Path(tmp) / "evidence"
                )

    def test_a_transcript_not_bound_to_the_receipt_refuses(self) -> None:
        receipt = _receipt("evidence-unbound-transcript")
        job = receipt.job
        point_count = job.domain.point_count
        unbound = (
            # foreign job identity
            protocol.DecisionTranscriptV1(
                _digest("foreign-job"),
                job.domain.identity,
                receipt.comparator.manifest.source_identity,
                point_count,
                b"\x00" * ((point_count * 2 + 7) // 8),
                (point_count, 0, 0, 0),
                0,
                _digest("accounting"),
                protocol.WitnessStoreV1(b"", 0),
            ),
            # foreign comparator identity
            protocol.DecisionTranscriptV1(
                job.identity,
                job.domain.identity,
                _digest("foreign-comparator"),
                point_count,
                b"\x00" * ((point_count * 2 + 7) // 8),
                (point_count, 0, 0, 0),
                0,
                _digest("accounting"),
                protocol.WitnessStoreV1(b"", 0),
            ),
        )
        for transcript in unbound:
            receipt.transcript = transcript
            receipt.run_claim = protocol.RunClaimV1(
                job.identity,
                receipt.comparator.manifest.source_identity,
                _digest("binary"),
                _digest("invocation"),
                _digest("platform"),
                transcript.identity,
            )
            with tempfile.TemporaryDirectory() as tmp:
                with self.assertRaises(protocol.ProtocolErrorV1):
                    corpus_lane.write_verification_evidence_v1(
                        receipt, Path(tmp) / "evidence"
                    )

    def test_a_run_claim_not_bound_to_the_transcript_refuses(self) -> None:
        receipt = _receipt("evidence-unbound-claim")
        receipt.run_claim = protocol.RunClaimV1(
            receipt.job.identity,
            receipt.comparator.manifest.source_identity,
            _digest("binary"),
            _digest("invocation"),
            _digest("platform"),
            _digest("foreign-transcript"),
        )
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(protocol.ProtocolErrorV1):
                corpus_lane.write_verification_evidence_v1(
                    receipt, Path(tmp) / "evidence"
                )


if __name__ == "__main__":
    unittest.main()
