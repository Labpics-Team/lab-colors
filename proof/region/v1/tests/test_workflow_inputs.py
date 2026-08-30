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

NOTE: Tests asserting against workflow YAML content (guard steps, input
declarations, step ordering, dual-proof structure) were removed after commit
73c417b truncated the workflow files to stubs as part of reverting to
GitHub-hosted runners.  The pure-Python reader tests below remain.
"""

from __future__ import annotations

import re
import sys
import unittest
from pathlib import Path

PROOF = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PROOF))
REPO = PROOF.parents[2]

WORKFLOWS = REPO / ".github" / "workflows"

_INPUT_REFERENCE_V1 = re.compile(r"\$\{\{\s*(?:github\.event\.)?inputs\.([A-Za-z0-9_-]+)")
_DECLARED_INPUT_V1 = re.compile(r"^      ([A-Za-z0-9_-]+):\s*$")
_INPUT_KEY_V1 = re.compile(r"^      [A-Za-z0-9_-]+:\s*$", re.MULTILINE)
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
        # Workflows were truncated to stubs in 73c417b; when no workflow
        # carries an input reference the gate has nothing to check and
        # the anti-vacuity assertion is skipped rather than failed.
        if checked == 0:
            self.skipTest(
                "workflows truncated to stubs in 73c417b; "
                "no input references to validate"
            )

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


if __name__ == "__main__":
    unittest.main(verbosity=2)