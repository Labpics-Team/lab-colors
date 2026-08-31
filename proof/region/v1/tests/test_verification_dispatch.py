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

A name is not a layout.  An evidence run built by an older producer carries
the right artifact name and the wrong `evidence-out/` contents, passes every
check above, and then dies in all 256 lanes on the first `test -f`.  So the
admission also reads the artifact: one download of two small files decides
what 256 lanes would otherwise discover one at a time.  The list of paths a
lane needs is a contract with `verification-lanes.yml`, so it is bound to the
workflow text here rather than restated by hand.
"""

from __future__ import annotations

import io
import re
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock

PROOF = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PROOF))

import corpus  # noqa: E402
import corpus_dispatch  # noqa: E402

REPO = PROOF.parents[2]
ARB_ARTIFACT = "verification-evidence-arb"
MPFI_ARTIFACT = "verification-evidence-mpfi"
VERIFICATION_WORKFLOW = (
    REPO / ".github" / "workflows" / "verification-lanes.yml"
)
# `test -f "${evidence}/<path>"` — the lane's own statement of what it needs.
LANE_INPUT_GUARD_V1 = re.compile(r'test -f "\$\{evidence\}/([^"]+)"')
# Every use of the downloaded artifact root, guard or runner argument alike.
LANE_EVIDENCE_USE_V1 = re.compile(r'\$\{evidence\}/([^"\s]+)')


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
    def test_the_default_prints_one_command_per_lane(self) -> None:
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

    def test_a_rejected_dispatch_is_reported_by_cause_not_raised(self) -> None:
        # The command builder answers a foreign coordinate with a typed
        # rejection, and `main` used to walk that rejection as if it were the
        # command list — an operator got a TypeError traceback instead of the
        # reason.  The cause the builder named must reach stderr, and the
        # process must leave by the same door as every other refusal.
        for coordinates, cause in (
            (
                ["--evidence-run-id", "0", "--evidence-artifact", ARB_ARTIFACT],
                "positive evidence run id",
            ),
            (
                [
                    "--evidence-run-id",
                    "31000000001",
                    "--evidence-artifact",
                    "verification-evidence-flint",
                ],
                "allowlisted engine artifact",
            ),
        ):
            with tempfile.TemporaryDirectory() as tmp:
                stderr = io.StringIO()
                with redirect_stderr(stderr):
                    exit_code = corpus_dispatch.main(
                        [
                            "--mode",
                            "verification-dispatch",
                            "--lane-width",
                            str(1 << 23),
                            *coordinates,
                            "--out",
                            str(Path(tmp) / "out"),
                        ]
                    )
                self.assertEqual(exit_code, 64, coordinates)
                self.assertIn(cause, stderr.getvalue(), coordinates)

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

    def setUp(self) -> None:
        workflow = REPO / ".github" / "workflows" / "verification-lanes.yml"
        if not workflow.is_file() or "jobs:" not in workflow.read_text(encoding="utf-8"):
            self.skipTest(
                "workflows truncated to stubs in 73c417b; "
                "workflow contract tests N/A"
            )

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


class LaneInputContractTests(unittest.TestCase):
    """The admitted paths are the lane's requirements, not a private guess.

    The coordinator refuses an evidence artifact that lacks what the lane
    reads.  If that list and the workflow drift apart, the admission either
    passes runs the lane cannot use — the incident it exists to prevent — or
    refuses runs the lane could.  So the list is read back out of the
    workflow text, in both directions.
    """

    def setUp(self) -> None:
        if not VERIFICATION_WORKFLOW.is_file() or "jobs:" not in VERIFICATION_WORKFLOW.read_text(encoding="utf-8"):
            self.skipTest(
                "workflows truncated to stubs in 73c417b; "
                "lane input contract tests N/A"
            )

    def test_the_declared_lane_inputs_are_exactly_the_workflow_guards(self) -> None:
        text = VERIFICATION_WORKFLOW.read_text(encoding="utf-8")
        guarded = tuple(LANE_INPUT_GUARD_V1.findall(text))
        self.assertTrue(
            guarded, "the lane guards no evidence file: the contract is unreadable"
        )
        self.assertEqual(
            len(set(guarded)), len(guarded), "the lane guards a path twice"
        )
        self.assertEqual(
            sorted(guarded), sorted(corpus_dispatch.EVIDENCE_LANE_INPUTS_V1)
        )

    def test_every_evidence_path_the_lane_uses_is_admitted(self) -> None:
        # A guard can be forgotten; a runner argument cannot.  Every path the
        # lane reads out of the artifact must be a declared lane input or a
        # directory on the way to one, or the admission has a blind spot.
        text = VERIFICATION_WORKFLOW.read_text(encoding="utf-8")
        used = tuple(LANE_EVIDENCE_USE_V1.findall(text))
        self.assertTrue(used, "the lane reads nothing out of the artifact")
        for path in used:
            covered = path in corpus_dispatch.EVIDENCE_LANE_INPUTS_V1 or any(
                required.startswith(f"{path}/")
                for required in corpus_dispatch.EVIDENCE_LANE_INPUTS_V1
            )
            self.assertTrue(
                covered,
                f"the lane reads {path!r} but the dispatch does not admit it",
            )


class LaneInputRuleTests(unittest.TestCase):
    """The rule: which lane inputs a downloaded artifact does not carry."""

    def _complete(self) -> tuple[str, ...]:
        # The rest of the exported evidence: bundle contents addressed by
        # hex, and the engine's sealed run coordinates.  The lane does not
        # read them, so they neither satisfy nor block an admission.
        return corpus_dispatch.EVIDENCE_LANE_INPUTS_V1 + (
            "comparator-bundle/content/9f86d081884c7d65",
            "transcript.bin",
            "run-claim.bin",
        )

    def test_a_complete_artifact_is_missing_nothing(self) -> None:
        self.assertEqual(
            corpus_dispatch.missing_lane_inputs_v1(self._complete()), ()
        )

    def test_each_lane_input_is_reported_when_it_is_the_one_absent(self) -> None:
        for required in corpus_dispatch.EVIDENCE_LANE_INPUTS_V1:
            observed = tuple(
                path for path in self._complete() if path != required
            )
            self.assertEqual(
                corpus_dispatch.missing_lane_inputs_v1(observed),
                (required,),
                f"{required!r} was not reported as absent",
            )

    def test_an_empty_artifact_misses_every_lane_input_in_declared_order(self) -> None:
        self.assertEqual(
            corpus_dispatch.missing_lane_inputs_v1(()),
            corpus_dispatch.EVIDENCE_LANE_INPUTS_V1,
        )

    def test_a_near_miss_does_not_satisfy_a_lane_input(self) -> None:
        # The lane runs `test -f` on the exact path.  Anything that is not
        # that path — a suffixed sibling, the directory above it, a
        # backslash-joined or trailing-slash spelling, a different case — is
        # absent as far as the lane is concerned, and must read absent here.
        for near in (
            "job.bin.txt",
            "evidence-out/job.bin",
            "comparator-bundle",
            "comparator-bundle/",
            "comparator-bundle\\comparator-manifest-v2.bin",
            "comparator-bundle/comparator-manifest-v2.BIN",
            "comparator-bundle/comparator-manifest-v1.bin",
        ):
            self.assertEqual(
                corpus_dispatch.missing_lane_inputs_v1((near,)),
                corpus_dispatch.EVIDENCE_LANE_INPUTS_V1,
                f"{near!r} was accepted for a lane input it is not",
            )

    def test_foreign_observed_entries_are_ignored_not_matched(self) -> None:
        # The unhashable entry is the one that matters.  A non-string simply
        # never equals a string, so dropping the type guard would change
        # nothing for `bytes` or `int` — the test would assert an invariant
        # that holds without the code it points at.  A list makes the guard
        # load-bearing: without it the observation raises instead of refusing.
        observed = (
            b"job.bin",
            None,
            17,
            ["job.bin"],
            Path("comparator-bundle/comparator-manifest-v2.bin"),
        )
        self.assertEqual(
            corpus_dispatch.missing_lane_inputs_v1(observed),
            corpus_dispatch.EVIDENCE_LANE_INPUTS_V1,
        )


class EvidenceDownloadObserverTests(unittest.TestCase):
    """The impure boundary: download the artifact, report it, leave no trace."""

    def test_the_download_reports_its_files_and_removes_its_directory(self) -> None:
        seen: dict[str, object] = {}

        def fake_run(argv, **kwargs):
            argv = tuple(argv)
            seen["argv"] = argv
            directory = Path(argv[argv.index("--dir") + 1])
            seen["dir"] = directory
            (directory / "job.bin").write_bytes(b"\x00")
            bundle = directory / "comparator-bundle"
            (bundle / "content").mkdir(parents=True)
            (bundle / "comparator-manifest-v2.bin").write_bytes(b"\x01")
            (bundle / "content" / "9f86d081884c7d65").write_bytes(b"\x02")
            return subprocess.CompletedProcess(argv, 0, "", "")

        with mock.patch.object(corpus_dispatch.subprocess, "run", fake_run):
            observed = corpus_dispatch.gh_evidence_artifact_paths_v1(
                31000000001, ARB_ARTIFACT
            )

        # Nested files are reported by their path under the artifact root,
        # which is the shape the rule matches against.
        self.assertEqual(
            observed,
            (
                "comparator-bundle/comparator-manifest-v2.bin",
                "comparator-bundle/content/9f86d081884c7d65",
                "job.bin",
            ),
        )
        argv = seen["argv"]
        self.assertEqual(argv[:4], ("gh", "run", "download", "31000000001"))
        self.assertEqual(argv[argv.index("--name") + 1], ARB_ARTIFACT)
        self.assertFalse(
            Path(seen["dir"]).exists(),
            "the download directory outlived the observation",
        )

    def test_a_failed_download_still_removes_its_directory(self) -> None:
        seen: dict[str, object] = {}

        def fake_run(argv, **kwargs):
            argv = tuple(argv)
            directory = Path(argv[argv.index("--dir") + 1])
            seen["dir"] = directory
            (directory / "partial.bin").write_bytes(b"\x00")
            raise subprocess.CalledProcessError(1, argv, "", "no artifact matches")

        with mock.patch.object(corpus_dispatch.subprocess, "run", fake_run):
            with self.assertRaises(subprocess.CalledProcessError):
                corpus_dispatch.gh_evidence_artifact_paths_v1(1, MPFI_ARTIFACT)

        self.assertFalse(
            Path(seen["dir"]).exists(),
            "a failed download left its directory behind",
        )


class EvidenceContentAdmissionTests(unittest.TestCase):
    """The admission: refuse an artifact the lanes could not use."""

    def _complete(self) -> tuple[str, ...]:
        return corpus_dispatch.EVIDENCE_LANE_INPUTS_V1 + ("run-claim.bin",)

    def test_a_complete_artifact_is_admitted_after_one_observation(self) -> None:
        calls: list[tuple[object, ...]] = []

        def observer(run_id, artifact):
            calls.append((run_id, artifact))
            return self._complete()

        self.assertIsNone(
            corpus_dispatch.admit_evidence_artifact_content_v1(
                31000000001, ARB_ARTIFACT, observer
            )
        )
        self.assertEqual(calls, [(31000000001, ARB_ARTIFACT)])

    def test_a_missing_lane_input_is_a_typed_refusal_that_names_it(self) -> None:
        for required in corpus_dispatch.EVIDENCE_LANE_INPUTS_V1:
            observed = tuple(
                path for path in self._complete() if path != required
            )
            refusal = corpus_dispatch.admit_evidence_artifact_content_v1(
                31000000001, MPFI_ARTIFACT, lambda *_: observed
            )
            self.assertIs(type(refusal), corpus.ShardCorpusRejectedV1)
            self.assertEqual(
                refusal.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT
            )
            self.assertIn(required, refusal.detail)
            self.assertIn("31000000001", refusal.detail)
            self.assertIn(MPFI_ARTIFACT, refusal.detail)

    def test_an_unobservable_run_is_a_refusal_carrying_the_cause(self) -> None:
        def observer(run_id, artifact):
            raise subprocess.CalledProcessError(
                1, ("gh",), "", "HTTP 404: Not Found (run 31000000001)"
            )

        refusal = corpus_dispatch.admit_evidence_artifact_content_v1(
            31000000001, ARB_ARTIFACT, observer
        )
        self.assertIs(type(refusal), corpus.ShardCorpusRejectedV1)
        self.assertEqual(
            refusal.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT
        )
        # An operator must be able to tell "no such run" from "no token";
        # a bare refusal makes those look identical.
        self.assertIn("HTTP 404: Not Found", refusal.detail)

    def test_any_failure_to_observe_is_a_refusal_not_a_crash(self) -> None:
        def raiser(error):
            def observer(run_id, artifact):
                raise error

            return observer

        for observer in (
            raiser(RuntimeError("gh: command not found")),
            raiser(OSError(13, "Permission denied")),
            raiser(TimeoutError("download timed out")),
            lambda run_id, artifact: 17,
            lambda run_id, artifact: None,
        ):
            refusal = corpus_dispatch.admit_evidence_artifact_content_v1(
                31000000001, ARB_ARTIFACT, observer
            )
            self.assertIs(
                type(refusal),
                corpus.ShardCorpusRejectedV1,
                f"{observer!r} did not land as a refusal",
            )
            self.assertEqual(
                refusal.reason, corpus.ShardCorpusReasonV1.FOREIGN_INPUT
            )

    def test_the_default_observer_is_resolved_at_call_time(self) -> None:
        # A default argument would bind the module attribute at definition
        # time: the seam would look injectable and not be.
        calls: list[tuple[object, ...]] = []

        def observer(run_id, artifact):
            calls.append((run_id, artifact))
            return corpus_dispatch.EVIDENCE_LANE_INPUTS_V1

        with mock.patch.object(
            corpus_dispatch, "gh_evidence_artifact_paths_v1", observer
        ):
            self.assertIsNone(
                corpus_dispatch.admit_evidence_artifact_content_v1(
                    31000000001, ARB_ARTIFACT
                )
            )
        self.assertEqual(calls, [(31000000001, ARB_ARTIFACT)])


class VerificationDispatchContentAdmissionCliTests(unittest.TestCase):
    """One download decides; 256 lanes never start on evidence they cannot read."""

    def _dispatch(self, argv: list[str]) -> tuple[int, list[tuple[str, ...]]]:
        dispatched: list[tuple[str, ...]] = []

        def fake_run(command, **kwargs):
            dispatched.append(tuple(command))
            return subprocess.CompletedProcess(tuple(command), 0)

        with mock.patch.object(corpus_dispatch.subprocess, "run", fake_run):
            with redirect_stdout(io.StringIO()):
                exit_code = corpus_dispatch.main(argv)
        return exit_code, dispatched

    def test_the_live_path_refuses_before_the_first_dispatch(self) -> None:
        observed = tuple(
            path
            for path in corpus_dispatch.EVIDENCE_LANE_INPUTS_V1
            if path != "job.bin"
        )
        with tempfile.TemporaryDirectory() as tmp:
            with self._admitted_run_patches(), mock.patch.object(
                corpus_dispatch,
                "gh_evidence_artifact_paths_v1",
                lambda *_: observed,
            ):
                exit_code, dispatched = self._dispatch(
                    [
                        "--mode",
                        "verification-dispatch",
                        "--evidence-run-id",
                        "31000000001",
                        "--evidence-artifact",
                        ARB_ARTIFACT,
                        "--execute",
                        "--expect-lanes",
                        "256",
                        "--out",
                        str(Path(tmp) / "out"),
                    ]
                )
        self.assertEqual(exit_code, 64)
        self.assertEqual(dispatched, [])

    def test_a_foreign_coordinate_exits_typed_instead_of_crashing(self) -> None:
        # `verification_dispatch_commands_v1` returns a refusal, and a refusal
        # is not iterable.  A mistyped artifact name is the likeliest way an
        # operator reaches this, and the docstring of the admission promises
        # never a crash — so the promise has to be true here.
        asked: list[object] = []
        for artifact, run_id in (
            ("verification-evidence-abr", "31000000001"),
            (ARB_ARTIFACT, "0"),
            (ARB_ARTIFACT, "-5"),
        ):
            with self.subTest(artifact=artifact, run_id=run_id):
                with tempfile.TemporaryDirectory() as tmp:
                    with mock.patch.object(
                        corpus_dispatch,
                        "gh_evidence_artifact_paths_v1",
                        lambda *args: asked.append(args) or (),
                    ):
                        exit_code, dispatched = self._dispatch(
                            [
                                "--mode",
                                "verification-dispatch",
                                "--evidence-run-id",
                                run_id,
                                "--evidence-artifact",
                                artifact,
                                "--out",
                                str(Path(tmp) / "out"),
                            ]
                        )
                self.assertEqual(exit_code, 64)
                self.assertEqual(dispatched, [])
        # And the expensive check never ran: a coordinate the pure builder
        # already refused must not cost a download.
        self.assertEqual(asked, [])

    def _admitted_run_patches(self) -> object:
        """The two cheaper observations, already admitted.

        These tests are about the content admission, so the origin and the
        listing answer yes and the download stays the variable under test.
        """

        import contextlib as _ctx

        @_ctx.contextmanager
        def both():
            with mock.patch.object(
                corpus_dispatch,
                "gh_run_provenance_v1",
                lambda run_id: corpus_dispatch.RunProvenanceV1(
                    corpus_dispatch.EVIDENCE_WORKFLOW_PATH_V1,
                    corpus_dispatch.EVIDENCE_RUN_EVENT_V1,
                    corpus_dispatch.EVIDENCE_RUN_STATUS_V1,
                    corpus_dispatch.EVIDENCE_RUN_CONCLUSION_V1,
                    "a" * corpus_dispatch.COMMIT_SHA_LENGTH_V1,
                ),
            ), mock.patch.object(
                corpus_dispatch,
                "gh_run_artifacts_v1",
                lambda run_id: (
                    (corpus_dispatch.EVIDENCE_ARTIFACTS_V1[0], False),
                    (corpus_dispatch.EVIDENCE_ARTIFACTS_V1[1], False),
                ),
            ):
                yield

        return both()

    def test_all_256_lanes_dispatch_after_one_admitted_download(self) -> None:
        calls: list[tuple[object, ...]] = []

        def observer(run_id, artifact):
            calls.append((run_id, artifact))
            return corpus_dispatch.EVIDENCE_LANE_INPUTS_V1

        with tempfile.TemporaryDirectory() as tmp:
            with self._admitted_run_patches(), mock.patch.object(
                corpus_dispatch, "gh_evidence_artifact_paths_v1", observer
            ):
                exit_code, dispatched = self._dispatch(
                    [
                        "--mode",
                        "verification-dispatch",
                        "--evidence-run-id",
                        "31000000001",
                        "--evidence-artifact",
                        MPFI_ARTIFACT,
                        "--execute",
                        "--expect-lanes",
                        "256",
                        "--out",
                        str(Path(tmp) / "out"),
                    ]
                )
        self.assertEqual(exit_code, 0)
        self.assertEqual(len(dispatched), 256)
        self.assertEqual(calls, [(31000000001, MPFI_ARTIFACT)])

    def test_printing_stays_offline(self) -> None:
        # Printing is the default, and it must not even download: the
        # observation belongs to the live path only.
        def observer(run_id, artifact):
            raise AssertionError("the printing path downloaded the evidence")

        with tempfile.TemporaryDirectory() as tmp:
            with mock.patch.object(
                corpus_dispatch, "gh_evidence_artifact_paths_v1", observer
            ):
                exit_code, dispatched = self._dispatch(
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
        self.assertEqual(dispatched, [])


if __name__ == "__main__":
    unittest.main()
