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

import re
import unittest
from pathlib import Path

PROOF = Path(__file__).resolve().parents[1]
REPO = PROOF.parents[2]
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
            if line.strip() and not line.startswith("        "):
                inside = False
    return frozenset(names)


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


if __name__ == "__main__":
    unittest.main(verbosity=2)
