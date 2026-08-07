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
        kwargs_seen: list[dict] = []

        class _Completed:
            stdout = "verification-evidence-arb\tfalse\n"
            returncode = 0

        # Recorded and asserted outside the stub: `assert` inside it would
        # vanish under PYTHONOPTIMIZE=2, and the CI worker runs the suite
        # under exactly that.
        def record(command: tuple[str, ...], **kwargs: object) -> object:
            seen.append(tuple(command))
            kwargs_seen.append(dict(kwargs))
            return _Completed()

        with unittest.mock.patch.object(corpus_dispatch.subprocess, "run", record):
            observed = corpus_dispatch.gh_run_artifacts_v1(31116022208)

        self.assertEqual(observed, (("verification-evidence-arb", False),))
        self.assertEqual(len(seen), 1)
        self.assertIs(kwargs_seen[0].get("check"), True)
        self.assertIs(kwargs_seen[0].get("capture_output"), True)
        # Without text=True the reply arrives as bytes, the decode raises,
        # and every run is refused — a gate that looks alive, admits nobody.
        self.assertIs(kwargs_seen[0].get("text"), True)
        # A hung `gh` would otherwise stall the one call standing between an
        # operator and 256 dispatches, indistinguishable from work in
        # progress.
        self.assertEqual(
            kwargs_seen[0].get("timeout"),
            corpus_dispatch.OBSERVATION_TIMEOUT_SECONDS_V1,
        )
        argv = seen[0]
        self.assertEqual(argv[:3], ("gh", "api", "--paginate"))
        self.assertIn("repos/{owner}/{repo}/actions/runs/31116022208/artifacts", argv)
        self.assertIn("--jq", argv)
        self.assertIn(
            ".artifacts[] | [.name, (.expired // false)] | @tsv",
            argv[argv.index("--jq") + 1],
        )


ADMITTED_SHA_V1 = "3f0f0a1b2c3d4e5f60718293a4b5c6d7e8f90a1b"


def _admitted_provenance(**overrides: str) -> object:
    """The run an operator is allowed to replay, with one field disturbed."""

    fields = dict(
        path=".github/workflows/full-domain-run.yml",
        event="workflow_dispatch",
        status="completed",
        conclusion="success",
        head_sha=ADMITTED_SHA_V1,
    )
    fields.update(overrides)
    return corpus_dispatch.RunProvenanceV1(**fields)


class RunProvenanceWireTests(unittest.TestCase):
    """Observing a run means reading named fields, never a raw reply.

    The artifact listing proves a name exists inside some run; it says
    nothing about which workflow produced that run, what triggered it,
    whether it finished, or which commit it stands on.  Asking GitHub for the
    whole run object would drag a large reply of unknown shape through this
    process — the project's standing hazard — so the query projects exactly
    the fields the rule reads, and the decode refuses anything that is not
    that shape rather than inventing a field.
    """

    CAPTURED_V1 = (
        ".github/workflows/full-domain-run.yml\tworkflow_dispatch\tcompleted\t"
        f"success\t{ADMITTED_SHA_V1}\n"
    )

    def test_the_captured_wire_form_decodes_to_the_named_fields(self) -> None:
        self.assertEqual(
            corpus_dispatch.parse_run_provenance_v1(self.CAPTURED_V1),
            _admitted_provenance(),
        )

    def test_a_reply_that_is_not_one_five_column_record_refuses(self) -> None:
        # An empty or drifted reply read as a default record would admit the
        # exact runs this observation exists to refuse, so shape drift is a
        # decode failure — which the admission turns into a typed refusal.
        for hostile in (
            "",
            "\n",
            ".github/workflows/full-domain-run.yml\tworkflow_dispatch\tcompleted"
            f"\t{ADMITTED_SHA_V1}\n",
            self.CAPTURED_V1.replace("\n", "\textra\n"),
            self.CAPTURED_V1 + self.CAPTURED_V1,
            f"{{\"path\": \".github/workflows/full-domain-run.yml\"}}\n",
        ):
            with self.subTest(hostile=hostile):
                with self.assertRaises(ValueError):
                    corpus_dispatch.parse_run_provenance_v1(hostile)

    def test_the_query_projects_the_fields_and_not_the_whole_reply(self) -> None:
        # Behavioural, not a look at the source, and load-bearing twice: the
        # run object carries far more than this module reads, and printing an
        # unknown reply is the known way this project leaks.
        seen: list[tuple[str, ...]] = []

        class _Completed:
            stdout = RunProvenanceWireTests.CAPTURED_V1
            returncode = 0

        def record(command: tuple[str, ...], **kwargs: object) -> object:
            seen.append(tuple(command))
            assert kwargs.get("check") is True
            assert kwargs.get("capture_output") is True
            # Without this the reply arrives as bytes, the decode raises, and
            # every run is refused — a gate that looks alive and admits nobody.
            assert kwargs.get("text") is True
            return _Completed()

        with unittest.mock.patch.object(corpus_dispatch.subprocess, "run", record):
            observed = corpus_dispatch.gh_run_provenance_v1(31116022208)

        self.assertEqual(observed, _admitted_provenance())
        self.assertEqual(len(seen), 1)
        argv = seen[0]
        self.assertEqual(argv[:2], ("gh", "api"))
        self.assertIn("repos/{owner}/{repo}/actions/runs/31116022208", argv)
        self.assertIn("--jq", argv)
        query = argv[argv.index("--jq") + 1]
        for field in (".path", ".event", ".status", ".conclusion", ".head_sha"):
            self.assertIn(field, query)
        self.assertIn("@tsv", query)
        # Exactly five selectors: a bare `.` or a dropped projection would
        # pull the whole run object through this process.
        self.assertEqual(query.count("."), 5)


class RunProvenanceRuleTests(unittest.TestCase):
    """Which observed run may be replayed is a rule, so it gets tests.

    A run id plus an artifact name is not provenance.  A stale run of an old
    commit, a run of a different workflow, and a run a fork's pull request
    produced all list an artifact of exactly the right name, and all three
    send 256 lanes to replay under something the operator did not intend.
    """

    def test_the_producer_run_an_operator_dispatched_is_admitted(self) -> None:
        self.assertIsNone(
            corpus_dispatch.admit_run_provenance_v1(_admitted_provenance())
        )

    def test_a_run_of_another_workflow_is_refused(self) -> None:
        # The path, not the file name: a workflow of the same basename in a
        # foreign directory is a different producer.
        for path in (
            ".github/workflows/verification-lanes.yml",
            ".github/workflows/ci.yml",
            "full-domain-run.yml",
            "vendor/.github/workflows/full-domain-run.yml",
            "",
        ):
            with self.subTest(path=path):
                result = corpus_dispatch.admit_run_provenance_v1(
                    _admitted_provenance(path=path)
                )
                self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
                self.assertEqual(
                    result.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT
                )

    def test_a_run_a_pull_request_produced_is_refused(self) -> None:
        # A fork's pull request can run the producer workflow and upload an
        # artifact of exactly the allowlisted name carrying a comparator
        # bundle nobody reviewed.  Only an operator's own dispatch counts.
        for event in ("pull_request", "pull_request_target", "push", "schedule", ""):
            with self.subTest(event=event):
                result = corpus_dispatch.admit_run_provenance_v1(
                    _admitted_provenance(event=event)
                )
                self.assertIs(type(result), corpus.ShardCorpusRejectedV1)

    def test_a_run_that_did_not_finish_successfully_is_refused(self) -> None:
        for status, conclusion in (
            ("completed", "failure"),
            ("completed", "cancelled"),
            ("completed", "timed_out"),
            # A run still in flight reports a null conclusion, which the wire
            # form renders as an empty column.
            ("in_progress", ""),
            ("queued", ""),
            # Hostile rather than observed: a finished-looking conclusion
            # under an unfinished status must not pass on the conclusion
            # alone.
            ("in_progress", "success"),
        ):
            with self.subTest(status=status, conclusion=conclusion):
                result = corpus_dispatch.admit_run_provenance_v1(
                    _admitted_provenance(status=status, conclusion=conclusion)
                )
                self.assertIs(type(result), corpus.ShardCorpusRejectedV1)

    def test_a_run_without_a_commit_sha_is_refused(self) -> None:
        # The sha is what the operator checks the campaign against, so a run
        # that does not carry one cannot be admitted merely for being green.
        for head_sha in (
            "",
            "3f0f0a1b",
            ADMITTED_SHA_V1[:-1],
            ADMITTED_SHA_V1 + "0",
            ADMITTED_SHA_V1[:-1] + "g",
            ADMITTED_SHA_V1.upper(),
        ):
            with self.subTest(head_sha=head_sha):
                result = corpus_dispatch.admit_run_provenance_v1(
                    _admitted_provenance(head_sha=head_sha)
                )
                self.assertIs(type(result), corpus.ShardCorpusRejectedV1)

    def test_a_record_that_was_never_observed_is_refused_not_crashed(self) -> None:
        for foreign in (
            None,
            (
                ".github/workflows/full-domain-run.yml",
                "workflow_dispatch",
                "completed",
                "success",
                ADMITTED_SHA_V1,
            ),
            {"path": ".github/workflows/full-domain-run.yml"},
            "completed",
            object(),
        ):
            with self.subTest(foreign=foreign):
                result = corpus_dispatch.admit_run_provenance_v1(foreign)
                self.assertIs(type(result), corpus.ShardCorpusRejectedV1)


class EvidenceRunAdmissionTests(unittest.TestCase):
    """The impure half: observe, refuse on any failure, report the sha.

    Nothing here decides which run is acceptable — that is the pure rule
    above.  What is proven here is that observation failures land as typed
    refusals carrying their cause, that the named run is the one observed,
    and that an admitted run's commit reaches the operator.
    """

    def test_an_unreachable_run_is_a_typed_refusal_carrying_the_cause(self) -> None:
        def observer(_run_id: int) -> object:
            raise subprocess.CalledProcessError(
                1, "gh", stderr="gh: Not Found (HTTP 404)"
            )

        result = corpus_dispatch.admit_evidence_run_v1(99999999, observer)
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
        self.assertIn("Not Found", result.detail)

    def test_an_undecodable_reply_becomes_a_refusal_not_a_traceback(self) -> None:
        def observer(_run_id: int) -> object:
            return corpus_dispatch.parse_run_provenance_v1("garbage\n")

        result = corpus_dispatch.admit_evidence_run_v1(1, observer)
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)

    def test_the_observer_is_asked_about_the_named_run(self) -> None:
        # Anti-vacuity: an observer blind to its argument would let one run be
        # bound while another was vouched for.
        seen: list[int] = []

        def observer(run_id: int) -> object:
            seen.append(run_id)
            return _admitted_provenance()

        errors = io.StringIO()
        with contextlib.redirect_stderr(errors):
            self.assertIsNone(corpus_dispatch.admit_evidence_run_v1(77, observer))
        self.assertEqual(seen, [77])

    def test_the_admitted_commit_reaches_the_operator(self) -> None:
        # A green producer run of last week's comparator is admissible by
        # every rule here and still wrong.  Nothing in this process can tell
        # which commit the operator meant, so the one coordinate that decides
        # it is put in front of them at the moment of admission.
        errors = io.StringIO()
        with contextlib.redirect_stderr(errors):
            result = corpus_dispatch.admit_evidence_run_v1(
                424242, lambda run_id: _admitted_provenance()
            )
        self.assertIsNone(result)
        self.assertIn(ADMITTED_SHA_V1, errors.getvalue())

    def test_a_refused_run_reports_no_commit(self) -> None:
        # The report belongs to admission: a sha printed beside a refusal
        # reads as a cleared campaign.
        errors = io.StringIO()
        with contextlib.redirect_stderr(errors):
            result = corpus_dispatch.admit_evidence_run_v1(
                424242, lambda run_id: _admitted_provenance(event="pull_request")
            )
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)
        self.assertNotIn(ADMITTED_SHA_V1, errors.getvalue())


class DispatchSeamTests(unittest.TestCase):
    """The gates have to be wired in, not merely defined.

    The unit tests above prove what each admission decides; none of them
    prove that `main` asks.  Deleting either call would leave them green
    while the live path dispatched blind again, so both seams get their own
    contract: a refusal must stop the campaign before the first invocation,
    an admission must let it through, and the admitted commit must reach the
    operator before the lanes do.
    """

    def _provenance_patch(self, observer: object = None) -> object:
        """Patch the run observation the live path performs before anything else."""

        return unittest.mock.patch.object(
            corpus_dispatch,
            "gh_run_provenance_v1",
            observer
            if observer is not None
            else (lambda run_id: _admitted_provenance()),
        )


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
            with self._provenance_patch(), unittest.mock.patch.object(
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
            with self._provenance_patch(), unittest.mock.patch.object(
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
            with self._provenance_patch(), unittest.mock.patch.object(
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

    def test_an_overdeclared_scale_is_refused_too(self) -> None:
        # The declaration must equal reality in both directions.  Weakening
        # `!=` to `<` survived every test: an operator declaring 512 where the
        # plan holds 256 would dispatch silently, and a declaration that does
        # not match is a wrong mental model regardless of its sign.
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
                    [*argv, "--execute", "--expect-lanes", "512"]
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
            with self._provenance_patch(), unittest.mock.patch.object(
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
            with self._provenance_patch(
                lambda run_id: observed.append(run_id) or _admitted_provenance()
            ), unittest.mock.patch.object(
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
            with self._provenance_patch(), unittest.mock.patch.object(
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
            with self._provenance_patch(), unittest.mock.patch.object(
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

    def test_the_origin_is_asked_before_the_artifact(self) -> None:
        # A run of the wrong workflow lists an artifact of exactly the right
        # name, so asking about the artifact first clears a run that should
        # never have been considered — and spends a call to do it.  The order
        # is the claim, so the order is what this pins.
        asked: list[str] = []
        with tempfile.TemporaryDirectory() as tmp:
            with unittest.mock.patch.object(
                corpus_dispatch,
                "gh_run_provenance_v1",
                lambda run_id: asked.append("origin")
                or corpus_dispatch.RunProvenanceV1(
                    corpus_dispatch.EVIDENCE_WORKFLOW_PATH_V1,
                    "pull_request",
                    "completed",
                    "success",
                    "0" * 40,
                ),
            ), unittest.mock.patch.object(
                corpus_dispatch,
                "gh_run_artifacts_v1",
                lambda run_id: asked.append("artifact")
                or (("verification-evidence-arb", False),),
            ), unittest.mock.patch.object(
                corpus_dispatch.subprocess,
                "run",
                lambda command, **kwargs: None,
            ):
                code = corpus_dispatch.main(self._argv(Path(tmp) / "out.txt"))
        # The wrong trigger is refused, and the artifact was never asked about.
        self.assertEqual(code, 64)
        self.assertEqual(asked, ["origin"])

    def test_printing_stays_offline_and_asks_nobody(self) -> None:
        # Printing is documented as offline: it must not even observe.
        observed: list[int] = []
        launched: list[tuple[str, ...]] = []
        with tempfile.TemporaryDirectory() as tmp:
            with self._provenance_patch(
                lambda run_id: observed.append(run_id) or _admitted_provenance()
            ), unittest.mock.patch.object(
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

    def test_a_run_of_a_foreign_workflow_dispatches_nothing(self) -> None:
        # The artifact name is right, the artifact is live, and the run is
        # still not the producer's.  Without the provenance seam this is 256
        # lanes replaying whatever a foreign workflow happened to upload.
        launched: list[tuple[str, ...]] = []
        with tempfile.TemporaryDirectory() as tmp:
            with self._provenance_patch(
                lambda run_id: _admitted_provenance(
                    path=".github/workflows/ci.yml"
                )
            ), unittest.mock.patch.object(
                corpus_dispatch,
                "gh_run_artifacts_v1",
                lambda run_id: (("verification-evidence-arb", False),),
            ), unittest.mock.patch.object(
                corpus_dispatch.subprocess,
                "run",
                lambda command, **kwargs: launched.append(tuple(command)),
            ):
                errors = io.StringIO()
                with contextlib.redirect_stderr(errors):
                    code = corpus_dispatch.main(self._argv(Path(tmp) / "out.txt"))
        self.assertEqual(code, 64)
        self.assertEqual(launched, [])

    def test_a_run_a_pull_request_produced_dispatches_nothing(self) -> None:
        # A fork's pull request can run the producer workflow and publish an
        # artifact of exactly the allowlisted name; every earlier rule admits
        # it, and the lanes then replay a comparator bundle nobody reviewed.
        launched: list[tuple[str, ...]] = []
        with tempfile.TemporaryDirectory() as tmp:
            with self._provenance_patch(
                lambda run_id: _admitted_provenance(event="pull_request")
            ), unittest.mock.patch.object(
                corpus_dispatch,
                "gh_run_artifacts_v1",
                lambda run_id: (("verification-evidence-arb", False),),
            ), unittest.mock.patch.object(
                corpus_dispatch.subprocess,
                "run",
                lambda command, **kwargs: launched.append(tuple(command)),
            ):
                errors = io.StringIO()
                with contextlib.redirect_stderr(errors):
                    code = corpus_dispatch.main(self._argv(Path(tmp) / "out.txt"))
        self.assertEqual(code, 64)
        self.assertEqual(launched, [])

    def test_a_run_that_failed_dispatches_nothing(self) -> None:
        # A failed producer run can still have uploaded one engine's artifact
        # before the other job died; the artifact listing cannot tell.
        launched: list[tuple[str, ...]] = []
        with tempfile.TemporaryDirectory() as tmp:
            with self._provenance_patch(
                lambda run_id: _admitted_provenance(conclusion="failure")
            ), unittest.mock.patch.object(
                corpus_dispatch,
                "gh_run_artifacts_v1",
                lambda run_id: (("verification-evidence-arb", False),),
            ), unittest.mock.patch.object(
                corpus_dispatch.subprocess,
                "run",
                lambda command, **kwargs: launched.append(tuple(command)),
            ):
                errors = io.StringIO()
                with contextlib.redirect_stderr(errors):
                    code = corpus_dispatch.main(self._argv(Path(tmp) / "out.txt"))
        self.assertEqual(code, 64)
        self.assertEqual(launched, [])

    def test_the_admitted_commit_reaches_the_operator_before_the_campaign(
        self,
    ) -> None:
        # Which commit the evidence stands on is the one thing this process
        # cannot decide and the operator can.  It has to be on their terminal
        # before 256 lanes start, not discoverable afterwards in the run log.
        launched: list[tuple[str, ...]] = []

        class _Completed:
            returncode = 0

        with tempfile.TemporaryDirectory() as tmp:
            with self._provenance_patch(), unittest.mock.patch.object(
                corpus_dispatch,
                "gh_run_artifacts_v1",
                lambda run_id: (("verification-evidence-arb", False),),
            ), unittest.mock.patch.object(
                corpus_dispatch.subprocess,
                "run",
                lambda command, **kwargs: (
                    launched.append(tuple(command)) or _Completed()
                ),
            ):
                errors = io.StringIO()
                with contextlib.redirect_stderr(errors):
                    code = corpus_dispatch.main(self._argv(Path(tmp) / "out.txt"))
        self.assertEqual(code, 0)
        self.assertEqual(len(launched), 256)
        reported = errors.getvalue()
        self.assertIn(ADMITTED_SHA_V1, reported)
        # Before, not after: an operator reading it below "dispatching 256
        # lanes" learns the commit once the campaign is already gone.
        self.assertLess(
            reported.index(ADMITTED_SHA_V1), reported.index("dispatching 256")
        )

    def test_the_provenance_admission_asks_about_the_run_the_operator_named(
        self,
    ) -> None:
        # Mirror of the artifact pin: a call site that hardcoded a run id
        # would vouch for one run while the lanes replayed another.
        observed: list[int] = []
        launched: list[tuple[str, ...]] = []

        class _Completed:
            returncode = 0

        with tempfile.TemporaryDirectory() as tmp:
            argv = self._argv(Path(tmp) / "out.txt")
            argv[argv.index("424242")] = "31116022208"
            with self._provenance_patch(
                lambda run_id: observed.append(run_id) or _admitted_provenance()
            ), unittest.mock.patch.object(
                corpus_dispatch,
                "gh_run_artifacts_v1",
                lambda run_id: (("verification-evidence-arb", False),),
            ), unittest.mock.patch.object(
                corpus_dispatch.subprocess,
                "run",
                lambda command, **kwargs: (
                    launched.append(tuple(command)) or _Completed()
                ),
            ):
                errors = io.StringIO()
                with contextlib.redirect_stderr(errors):
                    code = corpus_dispatch.main(argv)
        self.assertEqual(code, 0)
        self.assertEqual(observed, [31116022208])
        self.assertEqual(len(launched), 256)

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
