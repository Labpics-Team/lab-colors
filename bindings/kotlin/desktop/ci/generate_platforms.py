#!/usr/bin/env python3
"""Project the platform source into the statically inspectable release workflow."""
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[4]


def main(check=False):
    platforms = json.loads((ROOT / "bindings/kotlin/desktop/platforms.json").read_text(encoding="utf-8"))
    workflow = ROOT / ".github/workflows/desktop-conformance-worker.yml"
    before = workflow.read_text(encoding="utf-8")
    matrix = "      matrix: &desktop-platforms\n        include:\n"
    matrix += "".join("          - " + json.dumps(row) + "\n" for row in platforms)
    after, count = re.subn(r"(?<=      # BEGIN generated desktop platforms\n).*?(?=      # END generated desktop platforms)",
                          lambda _: matrix, before, flags=re.DOTALL)
    if count != 1:
        raise ValueError("the generated desktop matrix section is missing or ambiguous")
    if check:
        if before != after:
            raise ValueError("regenerate workflow with bindings/kotlin/desktop/ci/generate_platforms.py")
    else:
        workflow.write_text(after, encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main("--check" in sys.argv[1:])
