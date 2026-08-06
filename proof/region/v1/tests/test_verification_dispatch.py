#!/usr/bin/env python3
"""Hostile contract for the verification lane dispatch coordinator.

The semantic verification of an engine RUN transcript replays the full 2^24
domain as independent window lanes under the engine's own comparator, so the
dispatch surface must mirror the corpus RUN dispatch: one exact aligned lane
plan, one workflow invocation per lane, and the run that carries the engine
evidence (job bytes + comparator bundle) bound as a dispatch coordinate.

A single evidence run carries one artifact per engine, so every lane must
also bind the exact engine artifact by name.  A lane that picked the first
artifact of a sorted directory listing would silently replay the foreign
engine's bundle, so the dispatch names the artifact explicitly, the workflow
downloads exactly that artifact, and foreign artifact names are typed
rejections before any dispatch exists.  Plans that cannot cover the domain
exactly and evidence run ids that are not positive integers are likewise
typed rejections.
"""

from __future__ import annotations

import io
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

PROOF = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PROOF))

import corpus  # noqa: E402
import corpus_dispatch  # noqa: E402

REPO = PROOF.parents[2]
ARB_ARTIFACT = "verification-evidence-arb"
MPFI_ARTIFACT = "verification-evidence-mpfi"


class EvidenceArtifactAllowlistTests(unittest.TestCase):
    def test_the_allowlist_is_exactly_the_two_engine_artifacts(self) -> None:
        self.assertEqual(
            corpus_dispatch.EVIDENCE_ARTIFACTS_V1,
            (ARB_ARTIFACT, MPFI_ARTIFACT),
        )


class VerificationDispatchCommandTests(unittest.TestCase):
    def test_one_command_per_lane_bound_to_run_and_artifact(self) -> None:
        plan = corpus_dispatch.lane_plan_v1(
            lane_width=1 << 23, shard_width=1 << 14
        )
        self.assertIs(type(plan), tuple)
        self.assertEqual(len(plan), 2)
        commands = corpus_dispatch.verification_dispatch_commands_v1(
            plan, 1 << 14, 31000000001, ARB_ARTIFACT
        )
        self.assertIs(type(commands), tuple)
        self.assertEqual(len(commands), 2)
        for command, (start, points) in zip(commands, plan):
            self.assertIs(type(command), tuple)
            self.assertEqual(
                command[:4],
                (
                    "gh",
                    "workflow",
                    "run",
                    corpus_dispatch.VERIFICATION_WORKFLOW_V1,
                ),
            )
            self.assertIn("evidence_run_id=31000000001", command)
            self.assertIn(f"evidence_artifact={ARB_ARTIFACT}", command)
            self.assertIn(f"window_start={start}", command)
            self.assertIn(f"window_points={points}", command)
            self.assertIn("shard_points=16384", command)

    def test_every_allowlisted_artifact_dispatches(self) -> None:
        plan = corpus_dispatch.lane_plan_v1(
            lane_width=1 << 23, shard_width=1 << 14
        )
        for artifact in corpus_dispatch.EVIDENCE_ARTIFACTS_V1:
            commands = corpus_dispatch.verification_dispatch_commands_v1(
                plan, 1 << 14, 1, artifact
            )
            self.assertIs(type(commands), tuple)
            for command in commands:
                self.assertIn(f"evidence_artifact={artifact}", command)

    def test_full_domain_plan_yields_256_verification_lanes(self) -> None:
        plan = corpus_dispatch.lane_plan_v1()
        self.assertIs(type(plan), tuple)
        commands = corpus_dispatch.verification_dispatch_commands_v1(
            plan, corpus_dispatch.DEFAULT_SHARD_WIDTH, 1, MPFI_ARTIFACT
        )
        self.assertEqual(len(commands), 256)

    def test_rejected_plan_passes_through(self) -> None:
        rejection = corpus_dispatch.lane_plan_v1(lane_width=1000)
        self.assertIs(type(rejection), corpus.ShardCorpusRejectedV1)
        result = corpus_dispatch.verification_dispatch_commands_v1(
            rejection, corpus_dispatch.DEFAULT_SHARD_WIDTH, 1, ARB_ARTIFACT
        )
        self.assertIs(result, rejection)

    def test_foreign_evidence_run_id_is_a_typed_rejection(self) -> None:
        plan = corpus_dispatch.lane_plan_v1(
            lane_width=1 << 23, shard_width=1 << 14
        )
        for foreign in (0, -1, "31000000001", 3.5, None, object()):
            result = corpus_dispatch.verification_dispatch_commands_v1(
                plan, 1 << 14, foreign, ARB_ARTIFACT
            )
            self.assertIs(
                type(result),
                corpus.ShardCorpusRejectedV1,
                f"evidence run id {foreign!r} was not rejected",
            )
            self.assertEqual(
                result.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT
            )

    def test_foreign_evidence_artifact_is_a_typed_rejection(self) -> None:
        plan = corpus_dispatch.lane_plan_v1(
            lane_width=1 << 23, shard_width=1 << 14
        )
        for foreign in (
            "",
            "verification-evidence-flint",
            "verification-evidence-*",
            "verification-evidence-arb/",
            ARB_ARTIFACT.encode("ascii"),
            1,
            None,
            object(),
        ):
            result = corpus_dispatch.verification_dispatch_commands_v1(
                plan, 1 << 14, 1, foreign
            )
            self.assertIs(
                type(result),
                corpus.ShardCorpusRejectedV1,
                f"evidence artifact {foreign!r} was not rejected",
            )
            self.assertEqual(
                result.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT
            )


class VerificationDispatchCliTests(unittest.TestCase):
    def test_dry_run_prints_one_command_per_lane(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            stdout = io.StringIO()
            with redirect_stdout(stdout):
                exit_code = corpus_dispatch.main(
                    [
                        "--mode",
                        "verification-dispatch",
                        "--lane-width",
                        str(1 << 23),
                        "--evidence-run-id",
                        "31000000001",
                        "--evidence-artifact",
                        ARB_ARTIFACT,
                        "--out",
                        str(Path(tmp) / "out"),
                    ]
                )
            self.assertEqual(exit_code, 0)
            lines = stdout.getvalue().splitlines()
            self.assertEqual(len(lines), 2)
            for line in lines:
                self.assertIn(corpus_dispatch.VERIFICATION_WORKFLOW_V1, line)
                self.assertIn("evidence_run_id=31000000001", line)
                self.assertIn(f"evidence_artifact={ARB_ARTIFACT}", line)

    def test_missing_evidence_run_id_exits_64(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            exit_code = corpus_dispatch.main(
                [
                    "--mode",
                    "verification-dispatch",
                    "--evidence-artifact",
                    ARB_ARTIFACT,
                    "--out",
                    str(Path(tmp) / "out"),
                ]
            )
            self.assertEqual(exit_code, 64)

    def test_missing_evidence_artifact_exits_64(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            exit_code = corpus_dispatch.main(
                [
                    "--mode",
                    "verification-dispatch",
                    "--evidence-run-id",
                    "31000000001",
                    "--out",
                    str(Path(tmp) / "out"),
                ]
            )
            self.assertEqual(exit_code, 64)


class VerificationWorkflowContractTests(unittest.TestCase):
    """The dispatch workflow must consume the engine evidence contract."""

    def test_workflow_declares_the_dispatch_coordinates(self) -> None:
        workflow = REPO / ".github" / "workflows" / "verification-lanes.yml"
        self.assertTrue(workflow.is_file(), "verification-lanes.yml is missing")
        text = workflow.read_text(encoding="utf-8")
        for coordinate in (
            "evidence_run_id",
            "evidence_artifact",
            "window_start",
            "window_points",
            "shard_points",
        ):
            self.assertIn(coordinate, text)
        # The lane must replay under the engine evidence, never under the
        # fixture coordinates.
        self.assertIn("--comparator-bundle", text)
        self.assertIn("--job", text)
        self.assertIn(corpus_dispatch.VERIFICATION_WORKFLOW_V1, workflow.name)

    def test_workflow_downloads_exactly_the_bound_artifact(self) -> None:
        # A glob download plus a sorted first-entry pick would silently bind a
        # foreign engine's bundle when one run carries both artifacts, so the
        # lane must download the bound artifact by exact name and refuse any
        # name outside the dispatch allowlist.
        workflow = REPO / ".github" / "workflows" / "verification-lanes.yml"
        text = workflow.read_text(encoding="utf-8")
        self.assertIn("--name", text)
        self.assertNotIn("verification-evidence-*", text)
        self.assertNotIn("head -n 1", text)
        for artifact in corpus_dispatch.EVIDENCE_ARTIFACTS_V1:
            self.assertIn(artifact, text)

    def test_producer_workflow_publishes_exactly_the_allowlisted_names(self) -> None:
        producer = REPO / ".github" / "workflows" / "full-domain-run.yml"
        self.assertTrue(producer.is_file(), "full-domain-run.yml is missing")
        text = producer.read_text(encoding="utf-8")
        for artifact in corpus_dispatch.EVIDENCE_ARTIFACTS_V1:
            self.assertIn(artifact, text)


if __name__ == "__main__":
    unittest.main()
