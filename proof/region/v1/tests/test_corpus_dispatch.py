#!/usr/bin/env python3
"""Hostile contract for the full-domain lane dispatch coordinator (V5b2d-1e).

The full 2^24 RUN executes as one independent dispatch per lane window of
the exact full manifest.  The coordinator must derive a deterministic lane
plan whose windows cover [0, 2^24) exactly — contiguous, packing-aligned,
no gaps, no overlaps — and must turn that plan into one `gh workflow run`
invocation per lane.  Any width that cannot produce an exact aligned cover
is a typed rejection before any dispatch exists, and a dry run must emit
the dispatch commands without touching the network.
"""

from __future__ import annotations

import io
import json
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stderr
from pathlib import Path

PROOF = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PROOF))

import corpus  # noqa: E402
import corpus_dispatch  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402

FULL_DOMAIN = protocol.OUTPUT_CARDINALITY_V1
ALIGNMENT = corpus.CORPUS_SHARD_ALIGNMENT_V1

ARB_ARTIFACT = "verification-evidence-arb"
MPFI_ARTIFACT = "verification-evidence-mpfi"
EVIDENCE_RUN = 4242424242
FOREIGN_EVIDENCE_RUN = 5353535353


def lane_run_name(
    artifact: str,
    window_start: int,
    window_points: int,
    evidence_run_id: int,
) -> str:
    """The exact title `verification-lanes.yml` renders for one lane run.

    The workflow's folded `run-name` scalar

        lane ${{ inputs.evidence_artifact }}
        ${{ inputs.window_start }}+${{ inputs.window_points }}
        of ${{ inputs.evidence_run_id }}

    collapses to one line of single-space-separated coordinates, so the
    collector's parser is tested against that literal shape and not against a
    convenient invention.
    """

    return f"lane {artifact} {window_start}+{window_points} of {evidence_run_id}"


def observation(
    run_id: int,
    artifact: str = ARB_ARTIFACT,
    window_start: int = 0,
    window_points: int = 65536,
    evidence_run_id: int = EVIDENCE_RUN,
    conclusion: str = "success",
) -> object:
    return corpus_dispatch.LaneRunObservationV1(
        run_id,
        lane_run_name(artifact, window_start, window_points, evidence_run_id),
        conclusion,
    )


def cover(
    plan: tuple[tuple[int, int], ...],
    artifact: str = ARB_ARTIFACT,
    evidence_run_id: int = EVIDENCE_RUN,
    first_run_id: int = 900000,
) -> list[object]:
    """One successful lane run per plan window, in reverse plan order.

    GitHub lists runs newest first, so a collector that returned observation
    order instead of plan order would look right only by accident; the fixture
    makes that accident impossible.
    """

    return [
        observation(
            first_run_id + index,
            artifact,
            start,
            points,
            evidence_run_id,
        )
        for index, (start, points) in reversed(list(enumerate(plan)))
    ]



class LanePlanCoverTests(unittest.TestCase):
    def test_default_plan_covers_the_full_domain_exactly(self) -> None:
        plan = corpus_dispatch.lane_plan_v1()
        self.assertIs(type(plan), tuple)
        self.assertEqual(len(plan), 256)
        cursor = 0
        for start, points in plan:
            self.assertEqual(start, cursor)
            self.assertEqual(points, 65536)
            self.assertEqual(start % ALIGNMENT, 0)
            self.assertEqual(points % ALIGNMENT, 0)
            cursor += points
        self.assertEqual(cursor, FULL_DOMAIN)

    def test_plan_follows_the_requested_lane_width(self) -> None:
        plan = corpus_dispatch.lane_plan_v1(lane_width=1024, shard_width=256)
        self.assertIs(type(plan), tuple)
        self.assertEqual(len(plan), FULL_DOMAIN // 1024)
        self.assertEqual(plan[0], (0, 1024))
        self.assertEqual(plan[-1], (FULL_DOMAIN - 1024, 1024))
        self.assertEqual(sum(points for _, points in plan), FULL_DOMAIN)

    def test_plan_is_deterministic(self) -> None:
        first = corpus_dispatch.plan_json_v1(lane_width=2048, shard_width=512)
        second = corpus_dispatch.plan_json_v1(lane_width=2048, shard_width=512)
        self.assertEqual(first, second)
        decoded = json.loads(first)
        self.assertEqual(decoded["schema"], "corpus-dispatch-plan-v1")
        self.assertEqual(decoded["lane_width"], 2048)
        self.assertEqual(decoded["shard_width"], 512)
        self.assertEqual(decoded["lane_count"], FULL_DOMAIN // 2048)
        self.assertEqual(decoded["domain_points"], FULL_DOMAIN)
        self.assertEqual(len(decoded["lanes"]), decoded["lane_count"])

    def test_plan_rejects_every_width_that_cannot_cover_exactly(self) -> None:
        cases = (
            # zero / negative widths
            dict(lane_width=0, shard_width=16384),
            dict(lane_width=-65536, shard_width=16384),
            dict(lane_width=65536, shard_width=0),
            # breaks the packing alignment
            dict(lane_width=65534, shard_width=16384),
            dict(lane_width=65536, shard_width=16382),
            # shard width does not divide the lane width
            dict(lane_width=65536, shard_width=12288),
            # shard wider than the lane
            dict(lane_width=1024, shard_width=2048),
            # lane wider than the full domain
            dict(lane_width=FULL_DOMAIN * 2, shard_width=16384),
            # aligned but does not divide the domain, leaving a ragged tail
            dict(lane_width=12, shard_width=4),
            # foreign types
            dict(lane_width=65536.0, shard_width=16384),
            dict(lane_width="65536", shard_width=16384),
        )
        for case in cases:
            result = corpus_dispatch.lane_plan_v1(**case)
            self.assertIs(type(result), corpus.ShardCorpusRejectedV1, case)
            self.assertEqual(
                result.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT, case
            )


class DispatchCommandTests(unittest.TestCase):
    def test_dispatch_commands_cover_the_plan_exactly(self) -> None:
        plan = corpus_dispatch.lane_plan_v1(lane_width=1024, shard_width=256)
        self.assertIs(type(plan), tuple)
        commands = corpus_dispatch.dispatch_commands_v1(plan, 256)
        self.assertEqual(len(commands), len(plan))
        for (start, points), command in zip(plan, commands):
            self.assertEqual(command[0], "gh")
            self.assertEqual(command[1], "workflow")
            self.assertEqual(command[2], "run")
            self.assertEqual(command[3], "full-domain-corpus.yml")
            self.assertIn(f"-f", command)
            joined = " ".join(command)
            self.assertIn(f"-f window_start={start}", joined)
            self.assertIn(f"-f window_points={points}", joined)
            self.assertIn(f"-f shard_points=256", joined)

    def test_dispatch_commands_reject_a_rejected_plan(self) -> None:
        result = corpus_dispatch.dispatch_commands_v1(
            corpus.ShardCorpusRejectedV1(
                corpus.ShardCorpusReasonV1.FOREIGN_INPUT, "foreign"
            ),
            256,
        )
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)


class DryRunTests(unittest.TestCase):
    def test_dry_run_never_touches_the_network(self) -> None:
        calls: list[list[str]] = []

        def boom(*args: object, **kwargs: object) -> None:
            calls.append([args, kwargs])
            raise AssertionError("dry run must not invoke subprocess")

        original = subprocess.run
        subprocess.run = boom  # type: ignore[assignment]
        try:
            with tempfile.TemporaryDirectory() as out:
                status = corpus_dispatch.main(
                    [
                        "--mode",
                        "dispatch",
                        "--lane-width",
                        "1024",
                        "--shard-width",
                        "256",
                        "--dry-run",
                        "--out",
                        out,
                    ]
                )
        finally:
            subprocess.run = original  # type: ignore[assignment]
        self.assertEqual(status, 0)
        self.assertEqual(calls, [])


class DispatchCliTests(unittest.TestCase):
    def test_plan_mode_writes_the_deterministic_plan(self) -> None:
        with tempfile.TemporaryDirectory() as out:
            status = corpus_dispatch.main(
                [
                    "--mode",
                    "plan",
                    "--lane-width",
                    "2048",
                    "--shard-width",
                    "512",
                    "--out",
                    out,
                ]
            )
            self.assertEqual(status, 0)
            plan = json.loads((Path(out) / "dispatch-plan.json").read_text())
            self.assertEqual(plan["schema"], "corpus-dispatch-plan-v1")
            self.assertEqual(plan["lane_count"], FULL_DOMAIN // 2048)
            self.assertEqual(plan["lanes"][0], {"window_start": 0, "window_points": 2048})
            self.assertEqual(
                plan["lanes"][-1],
                {
                    "window_start": FULL_DOMAIN - 2048,
                    "window_points": 2048,
                },
            )

    def test_invalid_widths_exit_64_before_any_dispatch(self) -> None:
        with tempfile.TemporaryDirectory() as out:
            invalid = (
                ["--mode", "plan", "--lane-width", "12", "--shard-width", "4",
                 "--out", out],
                ["--mode", "plan", "--lane-width", "65536", "--shard-width",
                 "12288", "--out", out],
                ["--mode", "dispatch", "--lane-width", "0", "--shard-width",
                 "256", "--dry-run", "--out", out],
            )
            for argv in invalid:
                self.assertEqual(corpus_dispatch.main(argv), 64, argv)
            self.assertEqual(list(Path(out).iterdir()), [])

    def test_unknown_mode_is_rejected(self) -> None:
        with self.assertRaises(SystemExit):
            corpus_dispatch.main(["--mode", "launch"])


class LaneRunNameTests(unittest.TestCase):
    """The parser is the only thing that turns a run title into coordinates."""

    def test_parses_the_exact_workflow_run_name_form(self) -> None:
        parsed = corpus_dispatch.parse_lane_run_name_v1(
            "lane verification-evidence-arb 0+65536 of 4242424242"
        )
        self.assertIs(type(parsed), corpus_dispatch.LaneRunNameV1)
        self.assertEqual(parsed.evidence_artifact, ARB_ARTIFACT)
        self.assertEqual(parsed.window_start, 0)
        self.assertEqual(parsed.window_points, 65536)
        self.assertEqual(parsed.evidence_run_id, 4242424242)

        last = corpus_dispatch.parse_lane_run_name_v1(
            lane_run_name(MPFI_ARTIFACT, FULL_DOMAIN - 65536, 65536, 7)
        )
        self.assertIs(type(last), corpus_dispatch.LaneRunNameV1)
        self.assertEqual(last.evidence_artifact, MPFI_ARTIFACT)
        self.assertEqual(last.window_start, FULL_DOMAIN - 65536)
        self.assertEqual(last.window_points, 65536)
        self.assertEqual(last.evidence_run_id, 7)

    def test_every_noncanonical_title_is_not_a_lane_run(self) -> None:
        titles = (
            # not a lane run at all
            "",
            "Verification lane replay",
            "lane",
            "full-domain lane 0+65536 of 7",
            # arity drift
            "lane verification-evidence-arb 0+65536 of",
            "lane verification-evidence-arb 0+65536 of 7 rerun",
            "lane verification-evidence-arb 0+65536 7",
            # whitespace drift: a folded scalar emits exactly one space
            "lane  verification-evidence-arb 0+65536 of 7",
            "lane verification-evidence-arb 0+65536 of 7 ",
            " lane verification-evidence-arb 0+65536 of 7",
            "lane verification-evidence-arb 0+65536\tof 7",
            # window drift
            "lane verification-evidence-arb 0-65536 of 7",
            "lane verification-evidence-arb 65536 of 7",
            "lane verification-evidence-arb 0+65536+7 of 7",
            "lane verification-evidence-arb 0+0 of 7",
            # noncanonical ordinals that int() would happily swallow
            "lane verification-evidence-arb 00+65536 of 7",
            "lane verification-evidence-arb +0+65536 of 7",
            "lane verification-evidence-arb 0+6_5536 of 7",
            "lane verification-evidence-arb 0+65536 of 007",
            "lane verification-evidence-arb ٠+65536 of 7",
            "lane verification-evidence-arb 0+65536 of -7",
            "lane verification-evidence-arb 0+65536 of 0",
            # foreign types
            None,
            7,
            b"lane verification-evidence-arb 0+65536 of 7",
        )
        for title in titles:
            self.assertIsNone(corpus_dispatch.parse_lane_run_name_v1(title), title)


class LaneRunMatchTests(unittest.TestCase):
    """Matching names against the plan is pure: no network reaches it."""

    def test_a_complete_cover_yields_every_run_id_in_plan_order(self) -> None:
        plan = corpus_dispatch.lane_plan_v1()
        self.assertIs(type(plan), tuple)
        collected = corpus_dispatch.match_lane_runs_v1(
            plan, EVIDENCE_RUN, ARB_ARTIFACT, cover(plan)
        )
        self.assertIs(type(collected), corpus_dispatch.LaneRunCollectionV1)
        self.assertEqual(collected.evidence_run_id, EVIDENCE_RUN)
        self.assertEqual(collected.evidence_artifact, ARB_ARTIFACT)
        self.assertEqual(len(collected.lanes), len(plan))
        self.assertEqual(
            collected.lanes,
            tuple(
                (start, points, 900000 + index)
                for index, (start, points) in enumerate(plan)
            ),
        )

    def test_a_missing_window_is_a_typed_refusal_naming_the_hole(self) -> None:
        plan = corpus_dispatch.lane_plan_v1()
        self.assertIs(type(plan), tuple)
        observations = [
            seen
            for seen in cover(plan)
            if f" {65536 * 3}+65536 " not in seen.display_title
        ]
        self.assertEqual(len(observations), len(plan) - 1)
        refusal = corpus_dispatch.match_lane_runs_v1(
            plan, EVIDENCE_RUN, ARB_ARTIFACT, observations
        )
        self.assertIs(type(refusal), corpus_dispatch.LaneRunCollectionRejectedV1)
        self.assertEqual(refusal.reason, corpus.ShardCorpusReasonV1.INCOMPLETE_COVER)
        self.assertEqual(refusal.missing, ((65536 * 3, 65536),))
        self.assertEqual(refusal.duplicated, ())
        self.assertIn("196608+65536", refusal.detail)

    def test_a_window_in_two_runs_is_a_typed_refusal_naming_the_duplicate(
        self,
    ) -> None:
        plan = ((0, 65536), (65536, 65536), (131072, 65536))
        observations = cover(plan) + [observation(777, ARB_ARTIFACT, 65536, 65536)]
        refusal = corpus_dispatch.match_lane_runs_v1(
            plan, EVIDENCE_RUN, ARB_ARTIFACT, observations
        )
        self.assertIs(type(refusal), corpus_dispatch.LaneRunCollectionRejectedV1)
        self.assertEqual(refusal.reason, corpus.ShardCorpusReasonV1.INCOMPLETE_COVER)
        self.assertEqual(refusal.missing, ())
        self.assertEqual(refusal.duplicated, ((65536, 65536),))
        self.assertIn("65536+65536", refusal.detail)

    def test_a_foreign_engine_run_never_fills_this_engines_window(self) -> None:
        plan = ((0, 65536), (65536, 65536), (131072, 65536))
        # The other engine's lane covers the same window of the same evidence
        # run: only the artifact tells the two campaigns apart.
        observations = [
            seen for seen in cover(plan) if " 65536+65536 " not in seen.display_title
        ] + [observation(555, MPFI_ARTIFACT, 65536, 65536)]
        refusal = corpus_dispatch.match_lane_runs_v1(
            plan, EVIDENCE_RUN, ARB_ARTIFACT, observations
        )
        self.assertIs(type(refusal), corpus_dispatch.LaneRunCollectionRejectedV1)
        self.assertEqual(refusal.missing, ((65536, 65536),))

        # And a complete Arb cover is not disturbed by the MPFI campaign
        # running beside it.
        collected = corpus_dispatch.match_lane_runs_v1(
            plan,
            EVIDENCE_RUN,
            ARB_ARTIFACT,
            cover(plan) + cover(plan, MPFI_ARTIFACT, first_run_id=800000),
        )
        self.assertIs(type(collected), corpus_dispatch.LaneRunCollectionV1)
        self.assertEqual(
            collected.lanes, ((0, 65536, 900000), (65536, 65536, 900001), (131072, 65536, 900002))
        )

    def test_a_run_of_another_campaign_never_fills_this_ones_window(self) -> None:
        plan = ((0, 65536), (65536, 65536))
        observations = [
            seen for seen in cover(plan) if " 0+65536 " not in seen.display_title
        ] + [
            observation(444, ARB_ARTIFACT, 0, 65536, FOREIGN_EVIDENCE_RUN),
        ]
        refusal = corpus_dispatch.match_lane_runs_v1(
            plan, EVIDENCE_RUN, ARB_ARTIFACT, observations
        )
        self.assertIs(type(refusal), corpus_dispatch.LaneRunCollectionRejectedV1)
        self.assertEqual(refusal.missing, ((0, 65536),))

    def test_an_unsuccessful_run_never_covers_its_window(self) -> None:
        plan = ((0, 65536), (65536, 65536))
        for conclusion in ("failure", "cancelled", "timed_out", "skipped", ""):
            observations = [
                seen for seen in cover(plan) if " 0+65536 " not in seen.display_title
            ] + [observation(333, ARB_ARTIFACT, 0, 65536, conclusion=conclusion)]
            refusal = corpus_dispatch.match_lane_runs_v1(
                plan, EVIDENCE_RUN, ARB_ARTIFACT, observations
            )
            self.assertIs(
                type(refusal), corpus_dispatch.LaneRunCollectionRejectedV1, conclusion
            )
            self.assertEqual(refusal.missing, ((0, 65536),), conclusion)

    def test_a_failed_run_beside_the_successful_rerun_is_not_a_duplicate(self) -> None:
        plan = ((0, 65536),)
        collected = corpus_dispatch.match_lane_runs_v1(
            plan,
            EVIDENCE_RUN,
            ARB_ARTIFACT,
            cover(plan) + [observation(222, ARB_ARTIFACT, 0, 65536, conclusion="failure")],
        )
        self.assertIs(type(collected), corpus_dispatch.LaneRunCollectionV1)
        self.assertEqual(collected.lanes, ((0, 65536, 900000),))

    def test_gaps_and_duplicates_are_reported_together(self) -> None:
        plan = ((0, 65536), (65536, 65536), (131072, 65536))
        observations = [
            seen for seen in cover(plan) if " 0+65536 " not in seen.display_title
        ] + [observation(111, ARB_ARTIFACT, 131072, 65536)]
        refusal = corpus_dispatch.match_lane_runs_v1(
            plan, EVIDENCE_RUN, ARB_ARTIFACT, observations
        )
        self.assertIs(type(refusal), corpus_dispatch.LaneRunCollectionRejectedV1)
        self.assertEqual(refusal.missing, ((0, 65536),))
        self.assertEqual(refusal.duplicated, ((131072, 65536),))

    def test_foreign_inputs_are_typed_refusals(self) -> None:
        plan = ((0, 65536),)
        cases = (
            (corpus.ShardCorpusRejectedV1(
                corpus.ShardCorpusReasonV1.FOREIGN_INPUT, "foreign"
            ), EVIDENCE_RUN, ARB_ARTIFACT, cover(plan)),
            (((0, 65536), (65536,)), EVIDENCE_RUN, ARB_ARTIFACT, cover(plan)),
            # Overlap is what the guard exists for, and a plan short of one
            # window kills only the tuple-shape branch beside it.
            (((0, 65536), (32768, 65536)), EVIDENCE_RUN, ARB_ARTIFACT, cover(plan)),
            ((), EVIDENCE_RUN, ARB_ARTIFACT, ()),
            (plan, 0, ARB_ARTIFACT, cover(plan)),
            (plan, -1, ARB_ARTIFACT, cover(plan)),
            (plan, "4242424242", ARB_ARTIFACT, cover(plan)),
            (plan, True, ARB_ARTIFACT, cover(plan)),
            (plan, EVIDENCE_RUN, "verification-evidence", cover(plan)),
            (plan, EVIDENCE_RUN, None, cover(plan)),
            (plan, EVIDENCE_RUN, ARB_ARTIFACT, None),
            (plan, EVIDENCE_RUN, ARB_ARTIFACT, 7),
            (plan, EVIDENCE_RUN, ARB_ARTIFACT, ["lane verification-evidence-arb 0+65536 of 4242424242"]),
        )
        for case in cases:
            refusal = corpus_dispatch.match_lane_runs_v1(*case)
            self.assertIs(
                type(refusal), corpus_dispatch.LaneRunCollectionRejectedV1, case
            )
            self.assertEqual(
                refusal.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT, case
            )

    def test_the_collection_is_immutable_and_deterministic(self) -> None:
        plan = ((0, 65536), (65536, 65536))
        first = corpus_dispatch.match_lane_runs_v1(
            plan, EVIDENCE_RUN, ARB_ARTIFACT, cover(plan)
        )
        second = corpus_dispatch.match_lane_runs_v1(
            plan, EVIDENCE_RUN, ARB_ARTIFACT, cover(plan)
        )
        self.assertEqual(
            corpus_dispatch.lane_runs_json_v1(first),
            corpus_dispatch.lane_runs_json_v1(second),
        )
        decoded = json.loads(corpus_dispatch.lane_runs_json_v1(first))
        self.assertEqual(decoded["schema"], "corpus-lane-runs-v1")
        self.assertEqual(decoded["evidence_run_id"], EVIDENCE_RUN)
        self.assertEqual(decoded["evidence_artifact"], ARB_ARTIFACT)
        self.assertEqual(decoded["lane_count"], 2)
        self.assertEqual(
            decoded["lanes"][0],
            {"window_start": 0, "window_points": 65536, "run_id": 900000},
        )
        with self.assertRaises(Exception):
            first.lanes = ()


class GhObservationWhitelistTests(unittest.TestCase):
    """The query itself is the whitelist: nothing else may enter the process."""

    def _observe(self, stdout: str) -> tuple[object, list[str]]:
        argv: list[str] = []

        class Completed:
            def __init__(self, out: str) -> None:
                self.stdout = out

        def fake_run(command: tuple[str, ...], **kwargs: object) -> object:
            argv.extend(command)
            self.assertEqual(kwargs.get("check"), True)
            self.assertEqual(kwargs.get("capture_output"), True)
            return Completed(stdout)

        original = subprocess.run
        subprocess.run = fake_run  # type: ignore[assignment]
        try:
            return corpus_dispatch.gh_lane_runs_v1(7), argv
        finally:
            subprocess.run = original  # type: ignore[assignment]

    def test_the_query_projects_exactly_three_fields(self) -> None:
        observed, argv = self._observe(
            "31\tlane verification-evidence-arb 0+65536 of 4242424242\tsuccess\n"
            "30\tlane verification-evidence-mpfi 0+65536 of 4242424242\tfailure\n"
            "29\tsome other run\t\n"
        )
        joined = " ".join(argv)
        self.assertIn("gh run list --workflow verification-lanes.yml", joined)
        self.assertIn("--limit 7", joined)
        self.assertIn("--json databaseId,displayTitle,conclusion", joined)
        self.assertIn(
            "--jq .[] | [.databaseId, .displayTitle, .conclusion] | @tsv", joined
        )
        self.assertEqual(
            observed,
            (
                corpus_dispatch.LaneRunObservationV1(
                    31, "lane verification-evidence-arb 0+65536 of 4242424242", "success"
                ),
                corpus_dispatch.LaneRunObservationV1(
                    30,
                    "lane verification-evidence-mpfi 0+65536 of 4242424242",
                    "failure",
                ),
                corpus_dispatch.LaneRunObservationV1(29, "some other run", ""),
            ),
        )

    def test_an_unreadable_record_is_never_guessed(self) -> None:
        for stdout in (
            "31\tlane verification-evidence-arb 0+65536 of 42\n",
            "31\tlane\twith\ttab\tsuccess\n",
            "not-an-id\tlane verification-evidence-arb 0+65536 of 42\tsuccess\n",
        ):
            with self.assertRaises(ValueError, msg=stdout):
                self._observe(stdout)


class LaneRunObservationBoundaryTests(unittest.TestCase):
    def test_an_observer_failure_is_a_refusal_not_a_crash(self) -> None:
        def boom(limit: int) -> tuple[object, ...]:
            raise subprocess.CalledProcessError(1, ("gh",), stderr="gh: Not Found")

        refusal = corpus_dispatch.collect_lane_runs_v1(
            ((0, 65536),), EVIDENCE_RUN, ARB_ARTIFACT, observer=boom
        )
        self.assertIs(type(refusal), corpus_dispatch.LaneRunCollectionRejectedV1)
        self.assertEqual(refusal.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT)
        self.assertIn("Not Found", refusal.detail)

    def test_collection_matches_exactly_what_the_observer_reported(self) -> None:
        plan = ((0, 65536), (65536, 65536))
        seen: list[int] = []

        def observer(limit: int) -> tuple[object, ...]:
            seen.append(limit)
            return tuple(cover(plan))

        collected = corpus_dispatch.collect_lane_runs_v1(
            plan, EVIDENCE_RUN, ARB_ARTIFACT, observer=observer, limit=13
        )
        self.assertIs(type(collected), corpus_dispatch.LaneRunCollectionV1)
        self.assertEqual(collected.lanes, ((0, 65536, 900000), (65536, 65536, 900001)))
        self.assertEqual(seen, [13])


class CollectCliTests(unittest.TestCase):
    def _with_observer(self, observer: object, argv: list[str]) -> int:
        original = corpus_dispatch.gh_lane_runs_v1
        corpus_dispatch.gh_lane_runs_v1 = observer  # type: ignore[assignment]
        try:
            return corpus_dispatch.main(argv)
        finally:
            corpus_dispatch.gh_lane_runs_v1 = original  # type: ignore[assignment]

    def test_collect_mode_writes_the_machine_readable_run_ids(self) -> None:
        plan = corpus_dispatch.lane_plan_v1()
        with tempfile.TemporaryDirectory() as out:
            status = self._with_observer(
                lambda limit: tuple(cover(plan)),
                [
                    "--mode",
                    "collect",
                    "--evidence-run-id",
                    str(EVIDENCE_RUN),
                    "--evidence-artifact",
                    ARB_ARTIFACT,
                    "--out",
                    out,
                ],
            )
            self.assertEqual(status, 0)
            decoded = json.loads((Path(out) / "lane-runs.json").read_text())
            self.assertEqual(decoded["schema"], "corpus-lane-runs-v1")
            self.assertEqual(decoded["lane_count"], 256)
            self.assertEqual(len(decoded["lanes"]), 256)
            self.assertEqual(
                [lane["run_id"] for lane in decoded["lanes"]],
                [900000 + index for index in range(256)],
            )

    def test_one_run_listed_twice_is_still_one_run(self) -> None:
        # A repetitive listing must not manufacture a duplicate-cover refusal:
        # the same run id twice is the same run, and the collection has to say
        # so rather than report the campaign broken.
        plan = corpus_dispatch.lane_plan_v1()
        listing = cover(plan)
        doubled = listing + listing
        with tempfile.TemporaryDirectory() as out:
            status = self._with_observer(
                lambda limit: tuple(doubled),
                [
                    "--mode",
                    "collect",
                    "--evidence-run-id",
                    str(EVIDENCE_RUN),
                    "--evidence-artifact",
                    ARB_ARTIFACT,
                    "--out",
                    out,
                ],
            )
            self.assertEqual(status, 0)
            decoded = json.loads((Path(out) / "lane-runs.json").read_text())
            self.assertEqual(len(decoded["lanes"]), 256)
            self.assertEqual(
                [lane["run_id"] for lane in decoded["lanes"]],
                [900000 + index for index in range(256)],
            )

    def test_the_second_engine_is_collected_when_both_campaigns_are_listed(
        self,
    ) -> None:
        # Both engines replay the same evidence build, so a listing carries
        # both campaigns.  A collector that hardcoded the first artifact would
        # answer the MPFI request with Arb's run ids and exit 0 — half the
        # dual proof, silently wrong.
        plan = corpus_dispatch.lane_plan_v1()
        listing = cover(plan, artifact=ARB_ARTIFACT, first_run_id=900000) + cover(
            plan, artifact=MPFI_ARTIFACT, first_run_id=700000
        )
        with tempfile.TemporaryDirectory() as out:
            status = self._with_observer(
                lambda limit: tuple(listing),
                [
                    "--mode",
                    "collect",
                    "--evidence-run-id",
                    str(EVIDENCE_RUN),
                    "--evidence-artifact",
                    MPFI_ARTIFACT,
                    "--out",
                    out,
                ],
            )
            self.assertEqual(status, 0)
            decoded = json.loads((Path(out) / "lane-runs.json").read_text())
            self.assertEqual(decoded["evidence_artifact"], MPFI_ARTIFACT)
            self.assertEqual(
                [lane["run_id"] for lane in decoded["lanes"]],
                [700000 + index for index in range(256)],
            )

    def test_an_incomplete_collection_writes_nothing_and_exits_64(self) -> None:
        plan = corpus_dispatch.lane_plan_v1()
        holed = [
            seen for seen in cover(plan) if " 65536+65536 " not in seen.display_title
        ]
        with tempfile.TemporaryDirectory() as out:
            status = self._with_observer(
                lambda limit: tuple(holed),
                [
                    "--mode",
                    "collect",
                    "--evidence-run-id",
                    str(EVIDENCE_RUN),
                    "--evidence-artifact",
                    ARB_ARTIFACT,
                    "--out",
                    out,
                ],
            )
            self.assertEqual(status, 64)
            self.assertEqual(list(Path(out).iterdir()), [])

    def _refuse(self, argv: list[str]) -> tuple[int, str, list[int]]:
        """Run the CLI, recording every query the arguments would have fired.

        The observer answers with an empty listing rather than raising: a
        raising observer is swallowed into a typed refusal that also exits 64,
        so it would make "the query never happened" untestable.  Here an
        argument that reaches the query produces the *cover* refusal — same
        exit code, different text — which is exactly why these tests read the
        text and the recorded queries, never the exit code alone.
        """

        observed: list[int] = []

        def observer(limit: int) -> tuple[object, ...]:
            observed.append(limit)
            return ()

        stderr = io.StringIO()
        with redirect_stderr(stderr):
            status = self._with_observer(observer, argv)
        return status, stderr.getvalue(), observed

    def _assert_names_only(self, stderr: str, argument: str, context: object) -> None:
        """The refusal names the argument at fault and no other."""

        self.assertIn(argument, stderr, context)
        for other in ("--evidence-run-id", "--evidence-artifact", "--run-limit"):
            if other != argument:
                self.assertNotIn(other, stderr, context)

    def test_a_foreign_evidence_run_id_is_named_and_never_queried(self) -> None:
        # Every neighbouring refusal also exits 64, so the exit code cannot
        # tell an operator which argument to fix; the text has to.  And the
        # query must not happen at all: fired with a foreign coordinate it can
        # only come back as a refusal that blames `verification-lanes.yml`.
        for value in (None, "0", "-1"):
            argv = ["--mode", "collect", "--evidence-artifact", ARB_ARTIFACT]
            if value is not None:
                argv += ["--evidence-run-id", value]
            with tempfile.TemporaryDirectory() as out:
                status, stderr, observed = self._refuse(argv + ["--out", out])
                self.assertEqual(status, 64, value)
                self._assert_names_only(stderr, "--evidence-run-id", value)
                self.assertEqual(observed, [], value)
                self.assertEqual(list(Path(out).iterdir()), [], value)

    def test_a_foreign_evidence_artifact_is_named_and_never_queried(self) -> None:
        for value in (
            None,
            "",
            "verification-evidence",
            "verification-evidence-flint",
            "verification-evidence-arb/",
        ):
            argv = ["--mode", "collect", "--evidence-run-id", str(EVIDENCE_RUN)]
            if value is not None:
                argv += ["--evidence-artifact", value]
            with tempfile.TemporaryDirectory() as out:
                status, stderr, observed = self._refuse(argv + ["--out", out])
                self.assertEqual(status, 64, value)
                self._assert_names_only(stderr, "--evidence-artifact", value)
                # The operator learns the admissible set from the refusal, and
                # it is the module's allowlist rather than a second copy.
                for artifact in corpus_dispatch.EVIDENCE_ARTIFACTS_V1:
                    self.assertIn(artifact, stderr, value)
                self.assertEqual(observed, [], value)
                self.assertEqual(list(Path(out).iterdir()), [], value)

    def test_a_nonpositive_run_limit_is_named_and_never_queried(self) -> None:
        # `gh run list --limit 0` is not a query anyone can act on, and a
        # negative limit reaches the network verbatim.
        for value in ("0", "-5"):
            with tempfile.TemporaryDirectory() as out:
                status, stderr, observed = self._refuse(
                    [
                        "--mode",
                        "collect",
                        "--evidence-run-id",
                        str(EVIDENCE_RUN),
                        "--evidence-artifact",
                        ARB_ARTIFACT,
                        "--run-limit",
                        value,
                        "--out",
                        out,
                    ]
                )
                self.assertEqual(status, 64, value)
                self._assert_names_only(stderr, "--run-limit", value)
                self.assertEqual(observed, [], value)
                self.assertEqual(list(Path(out).iterdir()), [], value)

    def test_the_admissible_arguments_reach_the_query_unchanged(self) -> None:
        # The guard above must refuse foreign arguments, not narrow the
        # admissible ones: every allowlisted artifact and a positive limit
        # still reach the observer exactly as given.
        plan = corpus_dispatch.lane_plan_v1()
        for artifact in corpus_dispatch.EVIDENCE_ARTIFACTS_V1:
            observed: list[int] = []

            def observer(limit: int) -> tuple[object, ...]:
                observed.append(limit)
                return tuple(cover(plan, artifact=artifact))

            with tempfile.TemporaryDirectory() as out:
                status = self._with_observer(
                    observer,
                    [
                        "--mode",
                        "collect",
                        "--evidence-run-id",
                        str(EVIDENCE_RUN),
                        "--evidence-artifact",
                        artifact,
                        "--run-limit",
                        "1",
                        "--out",
                        out,
                    ],
                )
                self.assertEqual(status, 0, artifact)
                self.assertEqual(observed, [1], artifact)
                decoded = json.loads((Path(out) / "lane-runs.json").read_text())
                self.assertEqual(decoded["evidence_artifact"], artifact)


if __name__ == "__main__":
    unittest.main()
