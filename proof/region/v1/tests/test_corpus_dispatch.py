#!/usr/bin/env python3
"""Hostile contract for the full-domain lane dispatch coordinator (V5b2d-1e).

The full 2^24 RUN executes as one independent dispatch per lane window of
the exact full manifest.  The coordinator must derive a deterministic lane
plan whose windows cover [0, 2^24) exactly — contiguous, packing-aligned,
no gaps, no overlaps — and must turn that plan into one `gh workflow run`
invocation per lane.  Any width that cannot produce an exact aligned cover
is a typed rejection before any dispatch exists.  Printing the commands is
the default and dispatching is the opt-in: the incident this coordinator
exists to prevent was a forgotten flag.
"""

from __future__ import annotations

import contextlib
import io
import json
import subprocess
import sys
import tempfile
import unittest
import unittest.mock
from pathlib import Path

PROOF = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PROOF))

import corpus  # noqa: E402
import corpus_dispatch  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402

FULL_DOMAIN = protocol.OUTPUT_CARDINALITY_V1
ALIGNMENT = corpus.CORPUS_SHARD_ALIGNMENT_V1


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


class DefaultIsPrintingTests(unittest.TestCase):
    def test_the_default_run_never_touches_the_network(self) -> None:
        calls: list[list[str]] = []

        def boom(*args: object, **kwargs: object) -> None:
            calls.append([args, kwargs])
            raise AssertionError("printing must not invoke subprocess")

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
                 "256", "--out", out],
            )
            for argv in invalid:
                self.assertEqual(corpus_dispatch.main(argv), 64, argv)
            self.assertEqual(list(Path(out).iterdir()), [])

    def test_unknown_mode_is_rejected(self) -> None:
        with self.assertRaises(SystemExit):
            corpus_dispatch.main(["--mode", "launch"])


class EvidenceAdmissionTests(unittest.TestCase):
    """A dispatch names a run that must actually carry the artifact.

    The lane workflow downloads the named artifact from the named run, so a
    run that does not exist, or exists without that artifact, produces one
    doomed job per lane — 256 of them, each failing seconds in.  The
    coordinator refuses before any dispatch exists instead of discovering it
    256 times over.
    """

    def test_a_run_without_the_artifact_is_a_typed_rejection(self) -> None:
        result = corpus_dispatch.admit_evidence_artifact_v1(
            424242,
            "verification-evidence-arb",
            lambda run_id: (("verification-evidence-mpfi", False),),
        )
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
        self.assertEqual(result.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT)

    def test_an_unreachable_run_is_a_typed_rejection_not_a_crash(self) -> None:
        def unreachable(run_id: int):
            raise OSError("no such run")

        result = corpus_dispatch.admit_evidence_artifact_v1(
            424242, "verification-evidence-arb", unreachable
        )
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
        self.assertEqual(result.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT)

    def test_a_run_carrying_the_artifact_is_admitted(self) -> None:
        result = corpus_dispatch.admit_evidence_artifact_v1(
            424242,
            "verification-evidence-arb",
            lambda run_id: (
                ("verification-evidence-arb", False),
                ("verification-evidence-mpfi", False),
            ),
        )
        self.assertIsNone(result)

    def test_an_expired_artifact_is_not_evidence(self) -> None:
        # An expired artifact is still listed by GitHub but can no longer be
        # downloaded, so admitting it would send every lane to a download
        # that cannot succeed — the incident class, through the stale-run
        # path instead of the missing-run one.
        result = corpus_dispatch.admit_evidence_artifact_v1(
            424242,
            "verification-evidence-arb",
            lambda run_id: (("verification-evidence-arb", True),),
        )
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
        self.assertEqual(result.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT)

    def test_a_live_artifact_beside_an_expired_one_is_admitted(self) -> None:
        # Anti-vacuity for the rule above: expiry must filter, not blanket-refuse.
        result = corpus_dispatch.admit_evidence_artifact_v1(
            424242,
            "verification-evidence-arb",
            lambda run_id: (
                ("verification-evidence-mpfi", True),
                ("verification-evidence-arb", False),
            ),
        )
        self.assertIsNone(result)

    def test_the_observer_is_asked_about_the_named_run(self) -> None:
        # Anti-vacuity: an observer that ignores its argument would let a
        # dispatch bind one run while its artifacts were read from another.
        seen: list[int] = []

        def observer(run_id: int):
            seen.append(run_id)
            return (("verification-evidence-arb", False),)

        corpus_dispatch.admit_evidence_artifact_v1(
            99, "verification-evidence-arb", observer
        )
        self.assertEqual(seen, [99])


class ArtifactListingWireTests(unittest.TestCase):
    """Reading `expired` off the wire is a rule, and rules get tests.

    Moving the expiry filter out of the query put the decision where a test
    can reach it, but the decoding that produces `expired` stayed behind the
    network — so a listing whose shape drifted read as "live" and admitted a
    stale run.  The captured form below is what `gh` actually emits for the
    query this module sends.
    """

    CAPTURED_V1 = (
        "verification-evidence-arb\tfalse\n"
        "verification-evidence-mpfi\tfalse\n"
        "corpus-lane-0-65536\ttrue\n"
    )

    def test_the_captured_wire_form_decodes_to_names_and_expiry(self) -> None:
        self.assertEqual(
            corpus_dispatch.parse_artifact_listing_v1(self.CAPTURED_V1),
            (
                ("verification-evidence-arb", False),
                ("verification-evidence-mpfi", False),
                ("corpus-lane-0-65536", True),
            ),
        )

    def test_an_unrecognised_line_refuses_instead_of_reading_as_live(self) -> None:
        for hostile in (
            "verification-evidence-arb\n",
            "verification-evidence-arb\tfalse\textra\n",
            "\tfalse\n",
            "verification-evidence-arb\tno\n",
            "verification-evidence-arb\t\n",
            "verification-evidence-arb\t1\n",
        ):
            with self.subTest(hostile=hostile):
                with self.assertRaises(ValueError):
                    corpus_dispatch.parse_artifact_listing_v1(hostile)

    def test_an_unreadable_listing_becomes_a_typed_refusal(self) -> None:
        # The decode raises; the admission is what turns that into a refusal,
        # so the two are proven together rather than separately assumed.
        def observer(_run_id: int) -> tuple[tuple[str, bool], ...]:
            return corpus_dispatch.parse_artifact_listing_v1("garbage\n")

        result = corpus_dispatch.admit_evidence_artifact_v1(
            1, "verification-evidence-arb", observer
        )
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)

    def test_the_refusal_carries_the_text_that_tells_causes_apart(self) -> None:
        # `repr` of a CalledProcessError drops stderr, and stderr is the only
        # place "no such run" reads differently from "no token". Without it
        # the operator debugs the wrong axis — which is what the refusal is
        # supposed to prevent.
        def observer(_run_id: int) -> tuple[tuple[str, bool], ...]:
            raise subprocess.CalledProcessError(
                1, "gh", stderr="gh: Not Found (HTTP 404)"
            )

        result = corpus_dispatch.admit_evidence_artifact_v1(
            99999999, "verification-evidence-arb", observer
        )
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
        self.assertIn("Not Found", result.detail)

    def test_the_query_asks_for_every_page_and_for_expiry(self) -> None:
        # Behavioural, not a look at the source: a mutant that drops the flag
        # from argv while leaving the words in a comment reads identically.
        # A run carrying more than one page of artifacts would otherwise lose
        # the evidence name and refuse a healthy campaign.
        seen: list[tuple[str, ...]] = []

        class _Completed:
            stdout = "verification-evidence-arb\tfalse\n"
            returncode = 0

        def record(command: tuple[str, ...], **kwargs: object) -> object:
            seen.append(tuple(command))
            assert kwargs.get("check") is True
            assert kwargs.get("capture_output") is True
            return _Completed()

        with unittest.mock.patch.object(corpus_dispatch.subprocess, "run", record):
            observed = corpus_dispatch.gh_run_artifacts_v1(31116022208)

        self.assertEqual(observed, (("verification-evidence-arb", False),))
        self.assertEqual(len(seen), 1)
        argv = seen[0]
        self.assertEqual(argv[:3], ("gh", "api", "--paginate"))
        self.assertIn("repos/{owner}/{repo}/actions/runs/31116022208/artifacts", argv)
        self.assertIn("--jq", argv)
        self.assertIn(
            ".artifacts[] | [.name, (.expired // false)] | @tsv",
            argv[argv.index("--jq") + 1],
        )


class DispatchSeamTests(unittest.TestCase):
    """The gate has to be wired in, not merely defined.

    Four unit tests over the admission prove the decision; none of them
    prove that `main` asks.  Deleting the call would leave those green while
    the live path dispatched blind again, so the seam gets its own contract:
    a refusal must stop the campaign before the first invocation, and an
    admission must let it through.
    """

    def _argv(self, out: Path, *, live: bool = True) -> list[str]:
        argv = [
            "--mode",
            "verification-dispatch",
            "--evidence-run-id",
            "424242",
            "--evidence-artifact",
            "verification-evidence-arb",
            "--out",
            str(out),
        ]
        # Dispatching is the opt-in, and the scale is declared with it: the
        # seam only exists on the live path.
        return [*argv, "--execute", "--expect-lanes", "256"] if live else argv

    def test_a_refused_evidence_run_dispatches_nothing(self) -> None:
        launched: list[tuple[str, ...]] = []
        with tempfile.TemporaryDirectory() as tmp:
            with unittest.mock.patch.object(
                corpus_dispatch,
                "gh_run_artifacts_v1",
                lambda run_id: (("verification-evidence-arb", False),)
                if run_id == 999
                else (),
            ), unittest.mock.patch.object(
                corpus_dispatch.subprocess,
                "run",
                lambda command, **kwargs: launched.append(tuple(command)),
            ):
                code = corpus_dispatch.main(self._argv(Path(tmp) / "out.txt"))
        self.assertEqual(code, 64)
        self.assertEqual(launched, [])

    def test_an_admitted_evidence_run_dispatches_every_lane(self) -> None:
        launched: list[tuple[str, ...]] = []

        class _Completed:
            returncode = 0

        with tempfile.TemporaryDirectory() as tmp:
            with unittest.mock.patch.object(
                corpus_dispatch,
                "gh_run_artifacts_v1",
                # Sensitive to the run id on purpose: an observer that ignores
                # it lets a mutant hardcode the id at the admission call site
                # and still pass — and the run id is the exact coordinate the
                # 99999999 incident turned on.
                lambda run_id: (("verification-evidence-arb", False),)
                if run_id == 424242
                else (),
            ), unittest.mock.patch.object(
                corpus_dispatch.subprocess,
                "run",
                lambda command, **kwargs: (
                    launched.append(tuple(command)) or _Completed()
                ),
            ):
                code = corpus_dispatch.main(self._argv(Path(tmp) / "out.txt"))
        self.assertEqual(code, 0)
        self.assertEqual(len(launched), 256)
        self.assertTrue(
            all(command[:3] == ("gh", "workflow", "run") for command in launched)
        )

    def test_the_admission_asks_about_the_artifact_the_operator_named(self) -> None:
        # Mirror of the run-id pin: an observer blind to the artifact lets a
        # mutant hardcode one engine's name at the call site and stay green.
        # The producer runs the two engines as independent jobs, so a run
        # really can carry one artifact and not the other — and admitting the
        # wrong one buys 256 doomed lanes, which is the incident again.
        launched: list[tuple[str, ...]] = []
        argv = [
            "--mode",
            "verification-dispatch",
            "--evidence-run-id",
            "424242",
            "--evidence-artifact",
            "verification-evidence-mpfi",
            "--out",
        ]
        with tempfile.TemporaryDirectory() as tmp:
            with unittest.mock.patch.object(
                corpus_dispatch,
                "gh_run_artifacts_v1",
                lambda run_id: (("verification-evidence-arb", False),),
            ), unittest.mock.patch.object(
                corpus_dispatch.subprocess,
                "run",
                lambda command, **kwargs: launched.append(tuple(command)),
            ):
                code = corpus_dispatch.main(
                    [
                        *argv,
                        str(Path(tmp) / "out.txt"),
                        "--execute",
                        "--expect-lanes",
                        "256",
                    ]
                )
        self.assertEqual(code, 64)
        self.assertEqual(launched, [])

    def test_a_campaign_of_an_unexpected_size_is_refused_before_it_starts(
        self,
    ) -> None:
        # Swapping the two widths is a realistic slip and silently means a
        # different campaign; the operator declares the scale so reality can
        # disagree out loud.
        launched: list[tuple[str, ...]] = []
        with tempfile.TemporaryDirectory() as tmp:
            with unittest.mock.patch.object(
                corpus_dispatch,
                "gh_run_artifacts_v1",
                lambda run_id: (("verification-evidence-arb", False),),
            ), unittest.mock.patch.object(
                corpus_dispatch.subprocess,
                "run",
                lambda command, **kwargs: launched.append(tuple(command)),
            ):
                argv = self._argv(Path(tmp) / "out.txt", live=False)
                code = corpus_dispatch.main(
                    [*argv, "--execute", "--expect-lanes", "16"]
                )
        self.assertEqual(code, 64)
        self.assertEqual(launched, [])

    def test_without_execute_the_campaign_only_prints(self) -> None:
        # The polarity is the guard: the incident was a forgotten flag, so
        # forgetting one now costs nothing.
        launched: list[tuple[str, ...]] = []
        with tempfile.TemporaryDirectory() as tmp:
            with unittest.mock.patch.object(
                corpus_dispatch.subprocess,
                "run",
                lambda command, **kwargs: launched.append(tuple(command)),
            ):
                code = corpus_dispatch.main(
                    self._argv(Path(tmp) / "out.txt", live=False)
                )
        self.assertEqual(code, 0)
        self.assertEqual(launched, [])

    def test_the_admission_asks_about_the_run_the_operator_named(self) -> None:
        # The other seam tests all pass 424242, so a call site that hardcoded
        # that very number would be indistinguishable from one that reads the
        # argument.  This one names a different run and records what the
        # observer was actually asked.
        observed: list[int] = []
        launched: list[tuple[str, ...]] = []

        class _Completed:
            returncode = 0

        with tempfile.TemporaryDirectory() as tmp:
            argv = self._argv(Path(tmp) / "out.txt")
            argv[argv.index("424242")] = "31116022208"
            with unittest.mock.patch.object(
                corpus_dispatch,
                "gh_run_artifacts_v1",
                lambda run_id: (
                    observed.append(run_id)
                    or (("verification-evidence-arb", False),)
                ),
            ), unittest.mock.patch.object(
                corpus_dispatch.subprocess,
                "run",
                lambda command, **kwargs: (
                    launched.append(tuple(command)) or _Completed()
                ),
            ):
                code = corpus_dispatch.main(argv)
        self.assertEqual(code, 0)
        self.assertEqual(observed, [31116022208])
        self.assertEqual(len(launched), 256)

    def test_execute_without_a_declared_scale_dispatches_nothing(self) -> None:
        # The scale declaration is the guard, so it has to be required — an
        # optional one restores the forgotten-flag class it replaced.
        launched: list[tuple[str, ...]] = []
        with tempfile.TemporaryDirectory() as tmp:
            with unittest.mock.patch.object(
                corpus_dispatch,
                "gh_run_artifacts_v1",
                lambda run_id: (("verification-evidence-arb", False),),
            ), unittest.mock.patch.object(
                corpus_dispatch.subprocess,
                "run",
                lambda command, **kwargs: launched.append(tuple(command)),
            ):
                argv = self._argv(Path(tmp) / "out.txt", live=False)
                errors = io.StringIO()
                with contextlib.redirect_stderr(errors):
                    code = corpus_dispatch.main([*argv, "--execute"])
        self.assertEqual(code, 64)
        self.assertEqual(launched, [])
        # The refusal has to name what is missing: falling through to the
        # mismatch comparison refuses too, and tells the operator nothing.
        self.assertIn("--expect-lanes", errors.getvalue())
        self.assertNotIn("but the plan has", errors.getvalue())

    def test_a_wrong_scale_is_refused_before_the_network_is_asked(self) -> None:
        # The cheap local check must precede the authenticated call: a
        # mistyped width should not spend one, and the operator should read
        # "wrong scale" rather than a diagnosis about the evidence run.
        observed: list[int] = []
        with tempfile.TemporaryDirectory() as tmp:
            with unittest.mock.patch.object(
                corpus_dispatch,
                "gh_run_artifacts_v1",
                lambda run_id: observed.append(run_id) or (),
            ):
                argv = self._argv(Path(tmp) / "out.txt", live=False)
                code = corpus_dispatch.main(
                    [*argv, "--execute", "--expect-lanes", "16"]
                )
        self.assertEqual(code, 64)
        self.assertEqual(observed, [])

    def test_plan_mode_refuses_execute_instead_of_ignoring_it(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            code = corpus_dispatch.main(
                ["--mode", "plan", "--out", tmp, "--execute", "--expect-lanes", "256"]
            )
        self.assertEqual(code, 64)

    def test_a_campaign_that_dies_reports_how_far_it_got(self) -> None:
        # Without this the failure is a traceback: the operator cannot tell a
        # retry from a duplicate of everything already dispatched.
        launched: list[tuple[str, ...]] = []

        class _Completed:
            returncode = 0

        def flaky(command: tuple[str, ...], **_kwargs: object) -> object:
            if len(launched) == 3:
                raise subprocess.CalledProcessError(1, list(command))
            launched.append(tuple(command))
            return _Completed()

        with tempfile.TemporaryDirectory() as tmp:
            with unittest.mock.patch.object(
                corpus_dispatch,
                "gh_run_artifacts_v1",
                lambda run_id: (("verification-evidence-arb", False),),
            ), unittest.mock.patch.object(
                corpus_dispatch.subprocess, "run", flaky
            ):
                errors = io.StringIO()
                with contextlib.redirect_stderr(errors):
                    code = corpus_dispatch.main(self._argv(Path(tmp) / "out.txt"))
        self.assertEqual(code, 64)
        # Exactly the ones that succeeded, and no further attempt after.
        self.assertEqual(len(launched), 3)
        # The count is the whole point: a retry that cannot tell 3 from 0
        # duplicates everything already dispatched.
        self.assertIn("after 3 of 256", errors.getvalue())

    def test_a_missing_gh_is_a_typed_stop_not_a_traceback(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            with unittest.mock.patch.object(
                corpus_dispatch,
                "gh_run_artifacts_v1",
                lambda run_id: (("verification-evidence-arb", False),),
            ), unittest.mock.patch.object(
                corpus_dispatch.subprocess,
                "run",
                lambda command, **kwargs: (_ for _ in ()).throw(
                    FileNotFoundError("gh")
                ),
            ):
                code = corpus_dispatch.main(self._argv(Path(tmp) / "out.txt"))
        self.assertEqual(code, 64)

    def test_printing_stays_offline_and_asks_nobody(self) -> None:
        # Printing is documented as offline: it must not even observe.
        observed: list[int] = []
        launched: list[tuple[str, ...]] = []
        with tempfile.TemporaryDirectory() as tmp:
            with unittest.mock.patch.object(
                corpus_dispatch,
                "gh_run_artifacts_v1",
                lambda run_id: observed.append(run_id) or (),
            ), unittest.mock.patch.object(
                corpus_dispatch.subprocess,
                "run",
                lambda command, **kwargs: launched.append(tuple(command)),
            ):
                code = corpus_dispatch.main(
                    self._argv(Path(tmp) / "out.txt", live=False)
                )
        self.assertEqual(code, 0)
        self.assertEqual(observed, [])
        self.assertEqual(launched, [])

    def test_a_rejected_command_set_is_exit_64_not_a_type_error(self) -> None:
        # A foreign artifact name is refused by the pure builder; that
        # refusal must leave through the typed exit, never as a TypeError
        # raised by iterating a rejection.
        with tempfile.TemporaryDirectory() as tmp:
            argv = self._argv(Path(tmp) / "out.txt", live=False)
            argv[argv.index("verification-evidence-arb")] = "verification-evidence-foreign"
            code = corpus_dispatch.main(argv)
        self.assertEqual(code, 64)


if __name__ == "__main__":
    unittest.main()
