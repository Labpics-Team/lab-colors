#!/usr/bin/env python3
"""Hostile contract for the verification evidence export (V5b2d-3a).

A source-bound engine RUN must hand the verification lanes exactly two wire
objects: the job bytes the evaluator consumed and the comparator bundle the
lane runner needs to replay under the engine's own comparator.  The receipt
already carries both — the comparator's ten preimage blobs ride on the
controller-derived comparator — so the export reuses the single comparator
bundle wire surface and adds the canonical job encoding; nothing is
re-derived, nothing is duplicated.
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


def _receipt(tag: str) -> SimpleNamespace:
    job = protocol.ProofJobV1.parse(FIXTURE_JOB_V1.read_bytes())
    return SimpleNamespace(job=job, comparator=_comparator(tag))


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
            SimpleNamespace(
                job=object(), comparator=honest.comparator
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


if __name__ == "__main__":
    unittest.main()
