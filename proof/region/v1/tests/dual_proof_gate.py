#!/usr/bin/env python3
"""The one place a full-domain dual proof can be sealed.

`join_dual_proof_v1` needs five live objects, and two of them — the engines'
source-bound receipts — have no wire form on purpose: a receipt that could be
parsed from bytes would let foreign code mint provenance.  So the join can
only happen inside a process that minted both receipts itself, which means
one job that builds and runs both engines back to back.

The other three come from disk.  The semantic receipts are re-sealed here
from the verification lanes, and that is only possible because a lane binds
the comparator's *source* identity: two runs of the same sources observe
different build environments, so a lane bound to the full identity would die
with the run that produced its evidence, and no single process could ever
hold both a fresh receipt and a lane cover.  That same source identity is
what sorts the lane cover into engines — the discriminator is derived from
the sources, never a name written into the artifact.

Holding both engines in one interpreter is what makes this module delicate:
the two test directories carry five same-named modules (`full_domain_receipt`,
`gate`, `native_gate`, `test_evaluator_source`, `test_receipt`), and an import
cached by the first engine would silently answer for the second.  Both lane
modules are therefore loaded by path under distinct names, and the load is
checked against the file it was meant to be.

This module stays outside the fast `test_*.py` inventory: it is one long
native integration, dispatched deliberately, never swept up by a quick gate.
"""

from __future__ import annotations

import importlib.util
import json
import os
import sys
import unittest
from pathlib import Path
from types import ModuleType


PROOF = Path(__file__).resolve().parents[1]
ARB_LANE_MODULE = PROOF / "arb" / "tests" / "full_domain_receipt.py"
MPFI_LANE_MODULE = PROOF / "mpfi" / "tests" / "full_domain_receipt.py"
sys.path.insert(0, str(PROOF))

import corpus_assembly  # noqa: E402
import dual_proof  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402
import verification_assembly  # noqa: E402
from semantic.receipt import SemanticVerificationReceiptV1  # noqa: E402

LANES_ENV_V1 = "LABCOLORS_DUAL_PROOF_LANES"
RECEIPT_OUT_ENV_V1 = "LABCOLORS_DUAL_PROOF_OUT"
REQUIRED_ENVIRONMENT_V1 = (
    "LABCOLORS_ARB_PIPELINE_DOCKER",
    "LABCOLORS_MPFI_DOCKER",
    "LABCOLORS_EXECUTOR_CGROUP_V1",
    LANES_ENV_V1,
)


def _load_lane_module_v1(name: str, path: Path) -> ModuleType:
    """Load one engine's lane module under a name that cannot collide."""

    specification = importlib.util.spec_from_file_location(name, path)
    if specification is None or specification.loader is None:
        raise AssertionError(f"cannot load {path}")
    module = importlib.util.module_from_spec(specification)
    sys.modules[name] = module
    specification.loader.exec_module(module)
    if Path(module.__file__ or "").resolve() != path:
        raise AssertionError(f"{name} resolved to {module.__file__}, not {path}")
    return module


def _lane_cover_v1(
    root: Path,
    job: protocol.ProofJobV1,
    comparator: protocol.ContentResolvedComparatorManifestV2,
) -> tuple[object, ...]:
    """Admit every lane under `root` that this comparator's sources produced.

    The cover is selected by the comparator's source identity rather than by
    directory name, so a lane from the other engine is not merely rejected
    later — it is never offered to this receipt in the first place.
    """

    wanted = comparator.source_identity.hex()
    lanes = []
    foreign = 0
    for directory in sorted(path for path in root.iterdir() if path.is_dir()):
        manifest_path = directory / "lane-manifest.json"
        if not manifest_path.is_file():
            raise AssertionError(f"not a lane directory: {directory}")
        manifest = json.loads(manifest_path.read_text("ascii"))
        if manifest["comparator_source_identity"] != wanted:
            foreign += 1
            continue
        lane = corpus_assembly.load_lane_v1(directory, job, comparator)
        if type(lane) is not corpus_assembly.AdmittedLaneV1:
            raise AssertionError(f"lane rejected: {directory.name} ({lane!r})")
        lanes.append(lane)
    if not lanes:
        raise AssertionError(f"no lane of this engine under {root} ({foreign} foreign)")
    lanes.sort(key=lambda lane: lane.window_start)
    return tuple(lanes)


def _sealed_semantic_receipt_v1(
    job: protocol.ProofJobV1,
    comparator: protocol.ContentResolvedComparatorManifestV2,
    transcript: protocol.DecisionTranscriptV1,
    run: protocol.RunClaimV1,
    root: Path,
) -> SemanticVerificationReceiptV1:
    """Re-seal one engine's semantic receipt from its own live coordinates.

    The lanes replayed an earlier run of the same sources; they admit here
    because they bind the comparator's source identity, which reproduces.
    """

    sealed = verification_assembly.assemble_semantic_verification_v1(
        job, comparator, transcript, run, _lane_cover_v1(root, job, comparator)
    )
    if type(sealed) is not SemanticVerificationReceiptV1:
        raise AssertionError(f"semantic verification rejected: {sealed!r}")
    return sealed


class NativeFullDomainDualProofIntegrationTests(unittest.TestCase):
    def test_one_process_seals_the_full_domain_dual_proof(self) -> None:
        # Deliberately a failure and never a skip: this module is only ever
        # invoked as its own gate, so an incomplete environment means the
        # proof did not happen — it must not read as a pass.
        missing = [name for name in REQUIRED_ENVIRONMENT_V1 if not os.environ.get(name)]
        self.assertEqual(missing, [], f"missing native environment: {missing}")
        self.assertEqual(sys.platform, "linux")

        arb_lane = _load_lane_module_v1("labcolors_arb_full_domain", ARB_LANE_MODULE)
        mpfi_lane = _load_lane_module_v1("labcolors_mpfi_full_domain", MPFI_LANE_MODULE)

        arb = arb_lane.seal_full_domain_receipt_v1()
        mpfi = mpfi_lane.seal_full_domain_receipt_v1()

        # One job, two engines: a mismatch here means the lanes drifted apart
        # before any proof could exist.
        self.assertEqual(arb.job.identity, mpfi.job.identity)

        lanes_root = Path(os.environ[LANES_ENV_V1])
        arb_semantic = _sealed_semantic_receipt_v1(
            arb.job, arb.comparator.manifest, arb.transcript, arb.run_claim, lanes_root
        )
        mpfi_semantic = _sealed_semantic_receipt_v1(
            mpfi.job,
            mpfi.comparator.manifest,
            mpfi.transcript,
            mpfi.evidence.run_claim,
            lanes_root,
        )
        # Distinct engines cannot seal the same semantic receipt; equality
        # here would mean one engine's cover answered for both.
        self.assertNotEqual(arb_semantic.identity, mpfi_semantic.identity)

        candidate = protocol.compare_dual_transcripts(
            arb.job,
            arb.comparator.manifest,
            arb.transcript,
            arb.run_claim,
            mpfi.comparator.manifest,
            mpfi.transcript,
            mpfi.evidence.run_claim,
        )
        self.assertIs(type(candidate), protocol.DualComparisonCandidateV1)
        self.assertIs(dual_proof.claim_spans_full_domain_v1(candidate.claim), True)

        sealed = dual_proof.join_dual_proof_v1(
            candidate, arb, mpfi, arb_semantic, mpfi_semantic
        )
        self.assertIs(type(sealed), dual_proof.DualProofReceiptV1, sealed)
        self.assertTrue(sealed.full_domain)

        out = os.environ.get(RECEIPT_OUT_ENV_V1)
        if out:
            # The receipt has no wire form by design, so what leaves the run
            # is the identity it sealed: a checkable trace that invents no
            # parseable provenance artifact.
            directory = Path(out)
            directory.mkdir(parents=True, exist_ok=True)
            (directory / "dual-proof-identity.txt").write_text(
                sealed.identity.hex() + "\n", encoding="ascii"
            )
        print(f"dual proof sealed identity={sealed.identity.hex()}")


def main() -> int:
    """Run the single dual-proof integration with no room to vanish.

    The engine gates pin a discovered inventory against drift; this one has a
    single named test, so the law that matters is only that it ran, was not
    skipped, and passed.
    """

    suite = unittest.defaultTestLoader.loadTestsFromTestCase(
        NativeFullDomainDualProofIntegrationTests
    )
    tests = unittest.defaultTestLoader.getTestCaseNames(
        NativeFullDomainDualProofIntegrationTests
    )
    if len(tests) != 1:
        print(f"dual proof gate must carry exactly one test, found {tests}", file=sys.stderr)
        return 1
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    if result.skipped:
        print(f"dual proof gate must not skip: {result.skipped!r}", file=sys.stderr)
        return 1
    if (
        result.failures
        or result.errors
        or result.expectedFailures
        or result.unexpectedSuccesses
        or not result.wasSuccessful()
    ):
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
