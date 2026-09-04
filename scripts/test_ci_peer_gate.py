#!/usr/bin/env python3
"""Adversarial tests for the fail-closed CI peer gate."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import re
import subprocess
import sys
import unittest


REPO = Path(__file__).resolve().parents[1]
SCRIPT = Path(__file__).with_name("ci_peer_gate.py")
SPEC = importlib.util.spec_from_file_location("ci_peer_gate", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
gate = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = gate
SPEC.loader.exec_module(gate)


SHA = "a" * 40
CI = ".github/workflows/ci.yml"
NATIVE = ".github/workflows/native-conformance.yml"


def run(
    path: str,
    *,
    status: str = "completed",
    conclusion: str | None = "success",
    head_sha: str = SHA,
    event: str = "pull_request",
    run_id: int = 1,
    run_attempt: int = 1,
) -> dict[str, object]:
    return {
        "path": path,
        "status": status,
        "conclusion": conclusion,
        "head_sha": head_sha,
        "event": event,
        "id": run_id,
        "run_attempt": run_attempt,
    }


class PeerGateTest(unittest.TestCase):
    def evaluate(self, runs: list[dict], event: str = "pull_request"):
        return gate.evaluate_runs(runs, head_sha=SHA, event=event)

    def test_exact_required_successes_are_the_only_green_state(self) -> None:
        decision = self.evaluate([run(CI), run(NATIVE)])
        self.assertEqual(decision.state, "success")

    def test_empty_run_set_waits_and_can_never_turn_green(self) -> None:
        decision = self.evaluate([])
        self.assertEqual(decision.state, "wait")
        self.assertEqual(len(decision.reasons), 2)

    def test_wrong_head_and_wrong_event_do_not_satisfy_admission(self) -> None:
        for mutant in (
            [run(CI, head_sha="b" * 40), run(NATIVE)],
            [run(CI, event="merge_group"), run(NATIVE)],
        ):
            with self.subTest(mutant=mutant):
                self.assertEqual(self.evaluate(mutant).state, "wait")

    def test_skipped_neutral_cancelled_and_failure_are_red(self) -> None:
        for conclusion in ("skipped", "neutral", "cancelled", "failure", None):
            with self.subTest(conclusion=conclusion):
                decision = self.evaluate(
                    [run(CI, conclusion=conclusion), run(NATIVE)]
                )
                self.assertEqual(decision.state, "fail")

    def test_pending_required_run_waits(self) -> None:
        decision = self.evaluate(
            [run(CI, status="in_progress", conclusion=None), run(NATIVE)]
        )
        self.assertEqual(decision.state, "wait")

    def test_unknown_status_is_red_not_pending(self) -> None:
        decision = self.evaluate(
            [run(CI, status="invented", conclusion=None), run(NATIVE)]
        )
        self.assertEqual(decision.state, "fail")

    def test_latest_attempt_owns_the_result(self) -> None:
        decision = self.evaluate(
            [
                run(CI, conclusion="failure", run_id=10, run_attempt=1),
                run(CI, run_id=10, run_attempt=2),
                run(NATIVE),
            ]
        )
        self.assertEqual(decision.state, "success")

    def test_new_run_supersedes_an_older_rerun(self) -> None:
        decision = self.evaluate(
            [
                run(CI, run_id=10, run_attempt=2),
                run(CI, conclusion="failure", run_id=11, run_attempt=1),
                run(NATIVE),
            ]
        )
        self.assertEqual(decision.state, "fail")

    def test_unadmitted_peer_is_red(self) -> None:
        decision = self.evaluate(
            [run(CI), run(NATIVE), run(".github/workflows/new-peer.yml")]
        )
        self.assertEqual(decision.state, "fail")

    def test_gate_run_is_ignored_without_counting_as_evidence(self) -> None:
        decision = self.evaluate(
            [run(gate.GATE_WORKFLOW), run(CI), run(NATIVE)]
        )
        self.assertEqual(decision.state, "success")

    def test_merge_queue_requires_the_same_complete_evidence(self) -> None:
        decision = self.evaluate(
            [run(CI, event="merge_group"), run(NATIVE, event="merge_group")],
            event="merge_group",
        )
        self.assertEqual(decision.state, "success")

    def test_unsupported_event_fails_closed(self) -> None:
        self.assertEqual(self.evaluate([], event="workflow_dispatch").state, "fail")

    def test_ci_caller_pin_contains_the_checked_in_worker_bytes(self) -> None:
        caller = (REPO / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        matches = re.findall(
            r"Labpics-Team/lab-colors/\.github/workflows/ci-worker\.yml@([0-9a-f]{40})",
            caller,
        )
        self.assertEqual(len(matches), 1, "CI caller must have one immutable worker pin")
        committed = subprocess.run(
            ["git", "show", f"{matches[0]}:.github/workflows/ci-worker.yml"],
            cwd=REPO,
            check=True,
            capture_output=True,
        ).stdout
        current = (REPO / ".github/workflows/ci-worker.yml").read_bytes()
        self.assertEqual(
            committed,
            current,
            "CI caller pin does not execute the reviewed worker in this checkout",
        )


if __name__ == "__main__":
    unittest.main()
