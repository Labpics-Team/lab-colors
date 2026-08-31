#!/usr/bin/env python3
"""The corpus campaign contract, asserted where a stale checkout cannot move it.

A guard that lives only in the coordinator cannot protect a checkout older
than the guard: the old file carries the old contract whole.  That is how
this project dispatched 133 runs from a forgotten flag and then 69 from an
older checkout whose polarity was the opposite one.  So `full-domain-corpus.yml`
asserts the campaign invariant itself — as a required input GitHub refuses to
dispatch without, and as arithmetic in the lane's first step, before the
checkout exists.

These tests bind the two sides.  The coordinator's command must carry the
coordinate the workflow demands, and the workflow's guard must admit exactly
the campaigns the coordinator can produce.  The guard is *executed*, not read:
asserting that its text says `-ne` proves nothing about what the shell does
with it, and a neutered comparison left every text assertion green.
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

import corpus  # noqa: E402
import corpus_dispatch  # noqa: E402

WORKFLOW_V1 = REPO / ".github" / "workflows" / "full-domain-corpus.yml"
GUARD_STEP_V1 = "refuse a dispatch that does not belong to a coherent campaign"
_STEP_START_V1 = re.compile(r"^      - ", re.MULTILINE)
_INPUT_KEY_V1 = re.compile(r"^      [A-Za-z0-9_-]+:\s*$", re.MULTILINE)
REFUSED_V1 = 64


def _workflow_has_guard(text: str) -> bool:
    """True when the workflow carries the full guard step, not a stub."""
    return GUARD_STEP_V1 in text


@unittest.skipUnless(
    _workflow_has_guard(WORKFLOW_V1.read_text(encoding="utf-8")),
    "workflows truncated to stubs in 73c417b; guard parsing tests N/A",
)
class CorpusCampaignContractTests(unittest.TestCase):
    """The lane's own campaign guard and the coordinator's command, together."""

    def setUp(self) -> None:
        self.text = WORKFLOW_V1.read_text(encoding="utf-8")

    # --- the declaration ------------------------------------------------

    def test_the_campaign_size_is_a_required_input_without_a_default(self) -> None:
        # Required and without a default is the whole mechanism: GitHub
        # refuses the dispatch before a runner exists, so a coordinator that
        # predates this coordinate cannot start a campaign at all.  A default
        # would restore exactly what the incident was — an omission that
        # silently means something.
        block = self.text[self.text.index("      expect_lanes:") :]
        # The declaration ends where the next one begins; slicing to the
        # first step instead would drag a neighbour's `default:` in and make
        # this assertion answer about the wrong input.
        following = _INPUT_KEY_V1.search(block, 1)
        self.assertIsNotNone(following)
        block = block[: following.start()]
        self.assertIn("\n        required: true\n", block)
        self.assertNotIn("default:", block)

    # --- the coordinator side -------------------------------------------

    def test_every_dispatched_command_carries_the_campaign_size(self) -> None:
        plan = corpus_dispatch.lane_plan_v1()
        commands = corpus_dispatch.dispatch_commands_v1(
            plan, corpus_dispatch.DEFAULT_SHARD_WIDTH
        )
        self.assertIs(type(commands), tuple)
        self.assertEqual(len(commands), 256)
        for command in commands:
            self.assertEqual(command[3], corpus_dispatch.WORKFLOW_V1)
            coordinate = f"expect_lanes={len(commands)}"
            self.assertIn(coordinate, command)
            # Membership is not a dispatch: `gh` reads a coordinate only when
            # `-f` introduces it, and a bare element is an unrecognised
            # positional argument the campaign discovers 256 times over.
            self.assertEqual(command[command.index(coordinate) - 1], "-f")

    def test_a_rejected_plan_still_produces_no_command(self) -> None:
        # The coordinate is derived from the plan's length, so it must not
        # turn a typed refusal into an attribute error on the way out.
        result = corpus_dispatch.dispatch_commands_v1(
            corpus.ShardCorpusRejectedV1(
                corpus.ShardCorpusReasonV1.FOREIGN_INPUT, "foreign"
            ),
            corpus_dispatch.DEFAULT_SHARD_WIDTH,
        )
        self.assertIs(type(result), corpus.ShardCorpusRejectedV1)

    # --- the guard, executed --------------------------------------------

    def _guard_script_v1(self) -> str:
        """The guard's own shell, lifted out of the step that carries it."""

        start = self.text.index(GUARD_STEP_V1)
        body = self.text.index("        run: |", start) + len("        run: |\n")
        end = body + _STEP_START_V1.search(self.text[body:]).start()
        script = "\n".join(
            line[10:] if line.startswith(" " * 10) else line
            for line in self.text[body:end].splitlines()
        )
        # Anti-vacuity: an extraction that silently found nothing would make
        # every refusal below pass for the wrong reason.
        self.assertIn("16777216", script)
        self.assertGreater(len(script.splitlines()), 20)
        return script

    def _run_guard_v1(
        self,
        window_points: str,
        expect_lanes: str,
        window_start: str = "0",
    ) -> int:
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
                "WINDOW_START": window_start,
            },
        )
        return completed.returncode

    def test_the_guard_admits_every_seam_of_the_cover(self) -> None:
        # The whole plan has to survive its own guard, seam by seam: a guard
        # that refused one of them would refuse a correct campaign.
        for width in (1 << 16, 1 << 23):
            lanes = corpus_dispatch.FULL_DOMAIN // width
            for start in range(0, corpus_dispatch.FULL_DOMAIN, width):
                self.assertEqual(
                    self._run_guard_v1(str(width), str(lanes), str(start)),
                    0,
                    (width, start),
                )

    def test_the_guard_admits_exactly_what_the_coordinator_dispatches(self) -> None:
        # The two sides bound end to end: whatever the coordinator writes on
        # a command line is fed to the guard as the runner would receive it.
        for lane_width in (1 << 16, 1 << 20, 1 << 23):
            plan = corpus_dispatch.lane_plan_v1(lane_width=lane_width)
            self.assertIs(type(plan), tuple, lane_width)
            commands = corpus_dispatch.dispatch_commands_v1(
                plan, corpus_dispatch.DEFAULT_SHARD_WIDTH
            )
            sampled = (commands[0], commands[len(commands) // 2], commands[-1])
            for command in sampled:
                coordinates = dict(
                    item.split("=", 1) for item in command if "=" in item
                )
                self.assertEqual(
                    self._run_guard_v1(
                        coordinates["window_points"],
                        coordinates["expect_lanes"],
                        coordinates["window_start"],
                    ),
                    0,
                    command,
                )

    def test_the_guard_refuses_a_campaign_that_contradicts_its_width(self) -> None:
        # The defect this exists to stop is a dispatch whose claimed scale and
        # whose window disagree — including off by one, which is what a
        # half-updated coordinator produces.
        for width, lanes in (
            (1 << 16, 255),
            (1 << 16, 257),
            (1 << 16, 1),
            (1 << 20, 256),
            (1 << 23, 256),
        ):
            self.assertEqual(
                self._run_guard_v1(str(width), str(lanes)), REFUSED_V1, (width, lanes)
            )

    def test_the_guard_refuses_a_width_that_cannot_tile_the_domain(self) -> None:
        # A width that does not divide the domain has no lane count at all, so
        # it must stop before the division rather than round into one.
        for width in (1000, 65537, (1 << 24) + 1):
            self.assertEqual(
                self._run_guard_v1(str(width), "256"), REFUSED_V1, width
            )

        # These refuse for the divisibility rule alone.  Every width above is
        # also caught by the size comparison downstream, so deleting the
        # divisibility check leaves this test green without these two: 65535
        # windows of 256 lanes leave 256 points of the domain uncovered while
        # integer division still answers exactly 256.
        for width, lanes in ((65535, 256), (65534, 256)):
            self.assertEqual(corpus_dispatch.FULL_DOMAIN // width, lanes, width)
            self.assertNotEqual(corpus_dispatch.FULL_DOMAIN % width, 0, width)
            self.assertEqual(
                self._run_guard_v1(str(width), str(lanes)),
                REFUSED_V1,
                (width, lanes),
            )

    def test_the_guard_refuses_what_is_not_a_positive_integer(self) -> None:
        # A hand-typed dispatch is the realistic source of these, and an empty
        # campaign size is what a coordinator would leave behind if this input
        # ever gained a default.
        for width, lanes in (
            ("65536", ""),
            ("65536", "0"),
            ("0", "256"),
            ("-65536", "256"),
            ("65536", "-256"),
            ("65536", "256x"),
            ("65536", "2 56"),
            ("65 536", "256"),
            ("65536\r", "256"),
            ("65536", "256\r"),
        ):
            self.assertEqual(
                self._run_guard_v1(width, lanes), REFUSED_V1, (width, lanes)
            )

    def test_the_guard_refuses_a_coordinate_that_is_not_a_canonical_decimal(
        self,
    ) -> None:
        # A coordinate generated on Windows carries a carriage return; one
        # typed by hand carries a space or a sign.  Neither `[ -eq ]` nor
        # `int()` objects to surrounding whitespace, and the lane runner would
        # replay a window whose spelling no longer matches the guard's.
        for start in (
            "65536\r",
            "65536\n",
            " 65536",
            "65536 ",
            "+65536",
            "-65536",
            "0x10000",
            "6553 6",
            "",
        ):
            self.assertEqual(
                self._run_guard_v1("65536", "256", start), REFUSED_V1, repr(start)
            )

        # A leading zero is not cosmetic: `$(( ))` reads it as octal, so
        # `0200000` is 65536 to this guard and 200000 to the lane runner's
        # `int()`.  The guard would then admit a window the run never
        # replays.  Every other spelling above is caught downstream too, so
        # deleting the leading-zero arm leaves this test green without this
        # case: 0200000 lands on the seam and inside the domain.
        self.assertEqual(int("0200000"), 200000)
        self.assertEqual(self._run_guard_v1("65536", "256", "0200000"), REFUSED_V1)
        # Same class on the width: octal 0100000 is 32768, which tiles the
        # domain into 512 lanes, while the runner replays 100000 points.
        self.assertEqual(int("0100000"), 100000)
        self.assertEqual(self._run_guard_v1("0100000", "512", "0"), REFUSED_V1)

    def test_the_guard_refuses_a_start_off_the_seam_or_off_the_domain(self) -> None:
        # A start inside a window belongs to no plan and one past the end
        # belongs to no domain; both replay something the cover never asked
        # for.
        for start in ("1", "65535", "65537", "16777216", "16842752"):
            self.assertEqual(
                self._run_guard_v1("65536", "256", start), REFUSED_V1, start
            )

        # Past the shell's own integer the comparisons stop answering: `[ -ge ]`
        # reports "not greater" for what it cannot parse and `$(( ))` wraps,
        # so 2^64 is a perfectly aligned zero.  Deleting the width bound
        # leaves this test green without this case.
        self.assertEqual(18446744073709551616 % 65536, 0)
        self.assertEqual(
            self._run_guard_v1("65536", "256", "18446744073709551616"), REFUSED_V1
        )
        self.assertEqual(
            self._run_guard_v1("18446744073709617152", "256", "0"), REFUSED_V1
        )

    # --- the dispatch that is not a lane --------------------------------

    def test_the_guard_admits_the_bounded_probe_as_no_campaign(self) -> None:
        # The bounded prefix probe is this workflow's other mode; it is not a
        # lane of any cover, and the only campaign size true of it is none.
        self.assertEqual(self._run_guard_v1("", "0", ""), 0)

    def test_the_guard_refuses_a_campaign_claim_without_a_window(self) -> None:
        # The half-updated coordinator: it names the campaign but drops the
        # window, so every one of its dispatches would run the same bounded
        # probe and report it green.
        for lanes in ("1", "256", "16777216"):
            self.assertEqual(self._run_guard_v1("", lanes, ""), REFUSED_V1, lanes)

    def test_the_guard_refuses_a_start_with_no_window_to_start(self) -> None:
        # A start without a width is silently ignored by the probe branch, so
        # the operator reads a green run as the lane they asked for.
        self.assertEqual(self._run_guard_v1("", "0", "65536"), REFUSED_V1)

    # --- where the guard sits -------------------------------------------

    def test_the_guard_runs_before_anything_is_fetched_or_replayed(self) -> None:
        # A refusal that costs a checkout and an hour of replay is not the
        # refusal this guard exists to be.
        guard = self.text.index(GUARD_STEP_V1)
        checkout = self.text.index("actions/checkout@")
        replay = self.text.index("python3 proof/region/v1/corpus_")
        self.assertLess(guard, checkout)
        self.assertLess(guard, replay)


if __name__ == "__main__":
    unittest.main(verbosity=2)
