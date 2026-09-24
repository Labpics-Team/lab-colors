#!/usr/bin/env python3
"""AUTH-01: целевые семантические мутации обязаны делать целевой тест Core красным."""

from __future__ import annotations

from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "crates/labcolors-core/src/authority.rs"
COMMAND = [
    "cargo",
    "test",
    "-p",
    "labcolors-core",
    "authority::tests",
    "--lib",
    "--locked",
]

COMPILE_KILLED = frozenset({"open-authority-id"})

MUTANTS = {
    "open-authority-id": (
        "    HumanCleanEvidence,\n}",
        "    HumanCleanEvidence,\n    Other,\n}",
    ),
    "missing-as-success": (
        "            return Err(AuthorityRequireErrorV1::Missing);",
        "            return Ok(());",
    ),
    "lane-substitution": (
        "            Self::HumanCleanEvidence => 2,",
        "            Self::HumanCleanEvidence => 1,",
    ),
    "aggregate-compensation": (
        "        self.slots[id.slot()]",
        "        self.slots[id.slot()].or(self.slots[0]).or(self.slots[1]).or(self.slots[2])",
    ),
    "skip-applicability": (
        "    if expected.applicability != current.applicability {",
        "    if false && expected.applicability != current.applicability {",
    ),
    "skip-provenance": (
        "    if expected.provenance != current.provenance {",
        "    if false && expected.provenance != current.provenance {",
    ),
    "blind-replacement": (
        "                ensure_expected_current(current, expected)?;",
        "                let _ = (current, expected);",
    ),
    "saved-descriptor-rollback": (
        "    if owner_current.descriptor.release != next.release {",
        "    if false && owner_current.descriptor.release != next.release {",
    ),
    "changed-release-replay": (
        "        if current.release != expected.release {",
        "        if false && current.release != expected.release {",
    ),
    "unverified-as-observed": (
        "        (RendererObservationRequirementV1::Required, ProgramRendererProvenanceV1::Unverified) => {\n            return Err(AuthorityPermitErrorV1::RendererObservationRequired);\n        }",
        "        (RendererObservationRequirementV1::Required, ProgramRendererProvenanceV1::Unverified) => {}",
    ),
}


def run_mutant(name: str, before: str, after: str, original: str) -> None:
    """Require one targeted source mutation to make the focused AUTH suite red."""
    count = original.count(before)
    if count != 1:
        raise SystemExit(f"{name}: mutation anchor count is {count}, expected 1")
    SOURCE.write_text(original.replace(before, after, 1), encoding="utf-8")
    try:
        result = subprocess.run(
            COMMAND,
            cwd=ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )
    finally:
        SOURCE.write_text(original, encoding="utf-8")
    if result.returncode == 0:
        sys.stderr.write(result.stdout)
        raise SystemExit(f"{name}: mutant survived focused AUTH gate")
    expected_marker = "error[E" if name in COMPILE_KILLED else "test result: FAILED"
    if expected_marker not in result.stdout:
        sys.stderr.write(result.stdout)
        raise SystemExit(
            f"{name}: unexpected failure; expected marker {expected_marker!r}"
        )
    print(f"caught {name}")


def main() -> None:
    """Run the bounded AUTH-01 semantic mutation matrix and restore the source."""
    original = SOURCE.read_text(encoding="utf-8")
    for name, (before, after) in MUTANTS.items():
        run_mutant(name, before, after, original)
    if SOURCE.read_text(encoding="utf-8") != original:
        raise SystemExit("authority source was not restored")
    print(f"AUTH-01 mutation gate caught {len(MUTANTS)} semantic mutants")


if __name__ == "__main__":
    main()
