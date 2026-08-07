#!/usr/bin/env python3
"""A workflow that reads an input it never declared, caught before it runs.

`${{ inputs.x }}` on an undeclared input is not an error in GitHub Actions —
it expands to the empty string.  So a one-letter drift between the input's
declaration and its use costs a whole dispatched run to discover, and the
proof workflows are the expensive kind: the dual-proof job would have spent
two hours of native execution before noticing its lane cover was empty.

The dispatch workflows are pinned elsewhere by naming the coordinates they
must carry (`test_verification_dispatch.py`).  That catches a missing
coordinate but not a misspelt one, which is the defect that actually
happened.  This gate closes the class for every workflow at once, without a
YAML parser the proof tree does not have.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import unittest
from pathlib import Path

PROOF = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PROOF))
REPO = PROOF.parents[2]

import corpus_dispatch  # noqa: E402
WORKFLOWS = REPO / ".github" / "workflows"

_INPUT_REFERENCE_V1 = re.compile(r"\$\{\{\s*(?:github\.event\.)?inputs\.([A-Za-z0-9_-]+)")
_DECLARED_INPUT_V1 = re.compile(r"^      ([A-Za-z0-9_-]+):\s*$")
_INPUTS_BLOCK_V1 = re.compile(r"^    inputs:\s*$")


def _declared_inputs_v1(text: str) -> frozenset[str]:
    """Input names under `workflow_dispatch:`/`workflow_call:`.

    Read by indentation rather than parsed: the proof tree carries no YAML
    dependency, and every workflow here is written at the canonical depth.
    """

    names = []
    inside = False
    for line in text.splitlines():
        if _INPUTS_BLOCK_V1.match(line):
            inside = True
            continue
        if inside:
            match = _DECLARED_INPUT_V1.match(line)
            if match:
                names.append(match.group(1))
                continue
            # A comment is prose, not structure.  Treating one as a
            # dedent ended the block early and dropped every input declared
            # below it — silently, because a shorter set still satisfies
            # every assertion that only reads the names it did find.
            if line.lstrip().startswith("#"):
                continue
            if line.strip() and not line.startswith("        "):
                inside = False
    return frozenset(names)


class DeclaredInputReaderTests(unittest.TestCase):
    """The reader itself, on text whose shape it must survive."""

    def test_a_comment_does_not_end_the_input_block(self) -> None:
        # The reader used to treat any line short of eight spaces as a
        # dedent, so one comment hid every input below it.  Nothing failed:
        # a truncated set of names still satisfies assertions about the
        # names it kept.
        text = "\n".join(
            (
                "on:",
                "  workflow_dispatch:",
                "    inputs:",
                "      before:",
                "        required: true",
                "      # a note about the next one",
                "      after:",
                "        required: true",
                "jobs:",
            )
        )
        self.assertEqual(_declared_inputs_v1(text), frozenset({"before", "after"}))

    def test_a_real_dedent_still_ends_the_input_block(self) -> None:
        # The fix must not turn the reader into one that never stops: a key
        # outside the block is not an input.
        text = "\n".join(
            (
                "on:",
                "  workflow_dispatch:",
                "    inputs:",
                "      inside:",
                "        required: true",
                "permissions:",
                "      outside:",
                "jobs:",
            )
        )
        self.assertEqual(_declared_inputs_v1(text), frozenset({"inside"}))


class WorkflowInputReferenceTests(unittest.TestCase):
    def test_every_referenced_input_is_declared(self) -> None:
        offenders = []
        checked = 0
        for workflow in sorted(WORKFLOWS.glob("*.yml")):
            text = workflow.read_text(encoding="utf-8")
            referenced = frozenset(_INPUT_REFERENCE_V1.findall(text))
            if not referenced:
                continue
            checked += 1
            for name in sorted(referenced - _declared_inputs_v1(text)):
                offenders.append(f"{workflow.name}: {name}")
        self.assertEqual(offenders, [], f"undeclared workflow inputs: {offenders}")
        # Anti-vacuity: a gate that checked nothing would also pass.
        self.assertGreater(checked, 0)

    def test_the_gate_sees_a_misspelt_reference(self) -> None:
        # The exact drift that happened: the input was declared plural and
        # read singular, so the job silently received an empty string.
        drifted = (
            "on:\n"
            "  workflow_dispatch:\n"
            "    inputs:\n"
            "      lane_run_ids:\n"
            "        required: true\n"
            "jobs:\n"
            "  one:\n"
            "    steps:\n"
            "      - run: echo ${{ inputs.lane_run_id }}\n"
        )
        self.assertEqual(_declared_inputs_v1(drifted), frozenset({"lane_run_ids"}))
        self.assertEqual(
            frozenset(_INPUT_REFERENCE_V1.findall(drifted)), frozenset({"lane_run_id"})
        )

        healthy = drifted.replace("inputs.lane_run_id }}", "inputs.lane_run_ids }}")
        self.assertEqual(
            frozenset(_INPUT_REFERENCE_V1.findall(healthy))
            - _declared_inputs_v1(healthy),
            frozenset(),
        )


class DualProofContainmentContractTests(unittest.TestCase):
    """The dual-proof job must not share one observer subtree between engines."""

    def setUp(self) -> None:
        workflow = WORKFLOWS / "dual-proof.yml"
        self.assertTrue(workflow.is_file(), "dual-proof.yml is missing")
        self.text = workflow.read_text(encoding="utf-8")

    def test_each_engine_gets_its_own_observer_subtree(self) -> None:
        # A shared subtree admits two tasks, and the controller stays in the
        # observer it entered: the second engine's BUILD would fork into a
        # full budget and die an hour into the run.
        for group in ("proof-arb", "proof-mpfi"):
            self.assertIn(f"{group}/observer", self.text)
        self.assertIn("LABCOLORS_DUAL_PROOF_CGROUP_TASKS", self.text)
        self.assertNotIn("LABCOLORS_EXECUTOR_CGROUP_V1=", self.text)

    def test_the_cover_is_gathered_from_many_runs(self) -> None:
        # One lane is one dispatch is one run, so a full-domain cover never
        # lives in a single run id.
        self.assertIn("lane_run_ids", self.text)
        self.assertIn("verification-lane-*", self.text)
        self.assertNotIn("run-id:", self.text)

    def test_two_runs_cannot_quietly_claim_one_lane(self) -> None:
        # Downloading straight into one directory lets a re-run of the same
        # window overwrite the evidence already there: the cover still looks
        # exact while one of two answers silently won.
        self.assertIn("staged/", self.text)
        self.assertIn("one would silently win", self.text)

    def test_a_lane_run_id_that_is_not_a_number_is_refused(self) -> None:
        # An element starting with `-` would be read by `gh` as a flag.
        self.assertIn("''|*[!0-9]*)", self.text)

    def test_the_cover_is_checked_before_anything_is_built(self) -> None:
        cover_check = self.text.index("refuse an incomplete cover")
        first_build = self.text.index("seal the full-domain dual proof")
        self.assertLess(cover_check, first_build)


class LaneCampaignContractTests(unittest.TestCase):
    """The lane's campaign guard and the coordinator's command must agree.

    A guard that lives only in the coordinator cannot protect a checkout
    older than the guard — that is how this project dispatched 133 runs and
    then 69.  The lane therefore asserts the campaign invariant itself, and
    these tests bind the two sides so they cannot drift into a state where
    the coordinator dispatches something the lane refuses, or the lane admits
    something no coordinator would send.
    """

    def setUp(self) -> None:
        self.text = (WORKFLOWS / "verification-lanes.yml").read_text("utf-8")

    def test_the_campaign_size_is_a_required_input(self) -> None:
        # Required and without a default: GitHub refuses the dispatch before
        # a runner exists, which is the whole point — an older coordinator
        # cannot send this coordinate at all.
        declared = _declared_inputs_v1(self.text)
        self.assertIn("expect_lanes", declared)
        block = self.text[self.text.index("      expect_lanes:") :]
        block = block[: block.index("\njobs:")]
        self.assertIn("required: true", block)
        self.assertNotIn("default:", block)

    def test_every_dispatched_command_carries_the_campaign_size(self) -> None:
        plan = corpus_dispatch.lane_plan_v1()
        commands = corpus_dispatch.verification_dispatch_commands_v1(
            plan,
            corpus_dispatch.DEFAULT_SHARD_WIDTH,
            31000000001,
            corpus_dispatch.EVIDENCE_ARTIFACTS_V1[0],
        )
        self.assertIs(type(commands), tuple)
        self.assertEqual(len(commands), 256)
        for command in commands:
            coordinate = f"expect_lanes={len(commands)}"
            self.assertIn(coordinate, command)
            # Membership is not a dispatch: `gh` reads a coordinate only when
            # `-f` introduces it, and a bare element would be an unrecognised
            # positional argument the campaign discovers 256 times.
            self.assertEqual(command[command.index(coordinate) - 1], "-f")

    def test_the_size_the_coordinator_sends_satisfies_the_lane_arithmetic(
        self,
    ) -> None:
        # The lane divides the domain by its own window width and compares.
        # Whatever widths the plan admits, the pair must survive that check,
        # or a correct campaign would refuse itself.
        for lane_width in (1 << 16, 1 << 20, 1 << 23):
            plan = corpus_dispatch.lane_plan_v1(lane_width=lane_width)
            self.assertIs(type(plan), tuple, lane_width)
            commands = corpus_dispatch.verification_dispatch_commands_v1(
                plan,
                corpus_dispatch.DEFAULT_SHARD_WIDTH,
                1,
                corpus_dispatch.EVIDENCE_ARTIFACTS_V1[0],
            )
            for command in commands:
                points = int(
                    next(
                        item for item in command if item.startswith("window_points=")
                    ).split("=")[1]
                )
                lanes = int(
                    next(
                        item for item in command if item.startswith("expect_lanes=")
                    ).split("=")[1]
                )
                self.assertEqual(
                    corpus_dispatch.FULL_DOMAIN % points, 0, lane_width
                )
                self.assertEqual(
                    corpus_dispatch.FULL_DOMAIN // points, lanes, lane_width
                )

    def _guard_script_v1(self) -> str:
        """The guard's own shell, lifted out of the step that carries it.

        Asserting the text says `-ne` proves nothing about what the shell
        does with it: the comparison was neutered in a mutation and every
        text assertion here stayed green.  So the test runs the script.
        """

        start = self.text.index("      - name: refuse a lane that does not belong")
        body = self.text.index("        run: |", start) + len("        run: |\n")
        end = self.text.index("\n      - uses:", body)
        return "\n".join(
            line[10:] if line.startswith(" " * 10) else line
            for line in self.text[body:end].splitlines()
        )

    def _run_guard_v1(self, window_points: str, expect_lanes: str) -> int:
        bash = shutil.which("bash")
        if bash is None:
            self.skipTest("the lane guard is shell and needs a shell to run")
        completed = subprocess.run(
            (bash, "-c", self._guard_script_v1()),
            capture_output=True,
            text=True,
            env={
                **os.environ,
                "WINDOW_POINTS": window_points,
                "EXPECT_LANES": expect_lanes,
            },
        )
        return completed.returncode

    def test_the_guard_admits_exactly_the_coherent_campaigns(self) -> None:
        # Every width the plan admits, paired with the count that width
        # implies: a guard that refused these would refuse a correct run.
        for width in (1 << 16, 1 << 20, 1 << 23, 1 << 24):
            lanes = corpus_dispatch.FULL_DOMAIN // width
            self.assertEqual(self._run_guard_v1(str(width), str(lanes)), 0, width)

    def test_the_guard_refuses_a_campaign_that_contradicts_its_width(self) -> None:
        # The defect this exists to stop is a dispatch whose claimed scale
        # and whose window disagree — including off-by-one, which is what a
        # half-updated coordinator would produce.
        for width, lanes in (
            (1 << 16, 255),
            (1 << 16, 257),
            (1 << 16, 1),
            (1 << 20, 256),
            (1 << 23, 256),
        ):
            self.assertEqual(
                self._run_guard_v1(str(width), str(lanes)), 64, (width, lanes)
            )

    def test_the_guard_refuses_what_is_not_a_positive_integer(self) -> None:
        # An empty value is what an older coordinator's dispatch would leave
        # behind if the input ever gained a default, and a negative or
        # non-numeric one is what a hand-typed dispatch produces.
        for width, lanes in (
            ("", "256"),
            ("65536", ""),
            ("0", "256"),
            ("65536", "0"),
            ("-65536", "256"),
            ("65536", "-256"),
            ("65536", "256x"),
            ("65 536", "256"),
        ):
            self.assertEqual(
                self._run_guard_v1(width, lanes), 64, (width, lanes)
            )

    def test_the_guard_refuses_a_width_that_cannot_tile_the_domain(self) -> None:
        # A width that does not divide the domain has no lane count at all,
        # so it must stop before the division rather than round into one.
        for width in (1000, 65537, (1 << 24) + 1):
            self.assertEqual(self._run_guard_v1(str(width), "256"), 64, width)

        # These two refuse for the divisibility rule alone.  Every other
        # width above is also caught by the size comparison downstream, so
        # deleting the divisibility check left this test green: 65535 windows
        # of 256 lanes leave 256 points of the domain uncovered while
        # integer division still answers exactly 256.
        for width, lanes in ((65535, 256), (65534, 256)):
            self.assertEqual(corpus_dispatch.FULL_DOMAIN // width, lanes, width)
            self.assertNotEqual(corpus_dispatch.FULL_DOMAIN % width, 0, width)
            self.assertEqual(
                self._run_guard_v1(str(width), str(lanes)), 64, (width, lanes)
            )

    def test_the_guard_runs_before_anything_is_downloaded(self) -> None:
        # A refusal that costs a checkout and an artifact download is not the
        # refusal this guard exists to be.
        guard = self.text.index("refuse a lane that does not belong")
        checkout = self.text.index("actions/checkout@")
        download = self.text.index("download the engine verification evidence")
        self.assertLess(guard, checkout)
        self.assertLess(guard, download)


if __name__ == "__main__":
    unittest.main(verbosity=2)
