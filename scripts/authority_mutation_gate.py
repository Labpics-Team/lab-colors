#!/usr/bin/env python3
"""AUTH/TQ: целевые семантические мутации обязаны делать целевой тест Core красным."""

from __future__ import annotations

from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
AUTH_SOURCE = ROOT / "crates/labcolors-core/src/authority.rs"
TQ_SOURCE = ROOT / "crates/labcolors-core/src/authority/technical_quality.rs"
FIELD_SOURCE = ROOT / "crates/labcolors-core/src/field_effect.rs"
AUTH_COMMAND = [
    "cargo",
    "test",
    "-p",
    "labcolors-core",
    "authority::tests",
    "--lib",
    "--locked",
]
TQ_COMMAND = [
    "cargo",
    "test",
    "-p",
    "labcolors-core",
    "authority::technical_quality::tests",
    "--lib",
    "--locked",
]

COMPILE_KILLED = {
    "open-authority-id": "error[E0004]",  # Нарушена замкнутость перечисления.
    "tq-digest-as-proof": "error[E0599]",  # У сырых байтов нет методов доказательства.
}

AUTH_MUTANTS = {
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
        "        match self.slots[id.slot()] { Some(value) => Some(value), None => self.slots[0] }",
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


def run_mutant(
    name: str, source: Path, command: list[str], before: str, after: str, original: str
) -> None:
    """Require one targeted source mutation to make its focused suite red."""
    count = original.count(before)
    if count != 1:
        raise SystemExit(f"{name}: mutation anchor count is {count}, expected 1")
    source.write_text(original.replace(before, after, 1), encoding="utf-8")
    try:
        result = subprocess.run(
            command,
            cwd=ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )
    finally:
        source.write_text(original, encoding="utf-8")
    if result.returncode == 0:
        sys.stderr.write(result.stdout)
        raise SystemExit(f"{name}: mutant survived focused AUTH gate")
    expected_marker = COMPILE_KILLED.get(name, "test result: FAILED")
    if expected_marker not in result.stdout:
        sys.stderr.write(result.stdout)
        raise SystemExit(
            f"{name}: unexpected failure; expected marker {expected_marker!r}"
        )
    print(f"caught {name}")


TQ_MUTANTS = {
    "tq-stale-point-binding": (
        TQ_SOURCE,
        TQ_COMMAND,
        "        hasher.update(&materialization.revision().to_be_bytes());\n        hasher.update(&sink_stamp.sequence().to_be_bytes());",
        "        hasher.update(&0_u64.to_be_bytes());\n        hasher.update(&0_u64.to_be_bytes());",
    ),
    "tq-foreign-authority-lane": (
        TQ_SOURCE,
        TQ_COMMAND,
        "            AuthorityIdV1::TechnicalQuality,",
        "            AuthorityIdV1::CleanConvention,",
    ),
    "tq-requires-false-observation": (
        TQ_SOURCE,
        TQ_COMMAND,
        "            RendererObservationRequirementV1::NotRequired,",
        "            RendererObservationRequirementV1::Required,",
    ),
    "tq-weak-field-promotion": (
        FIELD_SOURCE,
        TQ_COMMAND,
        "    if certificate.evidence_class != FieldEvidenceClassV1::ExactReferenceWholeRaster {",
        "    if false && certificate.evidence_class != FieldEvidenceClassV1::ExactReferenceWholeRaster {",
    ),
    "tq-skip-field-replay": (
        FIELD_SOURCE,
        TQ_COMMAND,
        "    verify_certificate_replay(certificate, request)?;",
        "    let _ = (certificate, request);",
    ),
    "tq-stale-field-head": (
        FIELD_SOURCE,
        TQ_COMMAND,
        "    if request.scene_revision != current_scene {",
        "    if false && request.scene_revision != current_scene {",
    ),
    "tq-foreign-field-content": (
        FIELD_SOURCE,
        TQ_COMMAND,
        "    if certificate.request_digest != request_digest(request) {",
        "    if false && certificate.request_digest != request_digest(request) {",
    ),
    "tq-repeated-raster-hash": (
        FIELD_SOURCE,
        TQ_COMMAND,
        "pub(crate) const fn request_digest(request: &FieldEvaluationRequestV1<'_>) -> FieldRequestDigestV1 {\n    request.digest\n}",
        "pub(crate) fn request_digest(request: &FieldEvaluationRequestV1<'_>) -> FieldRequestDigestV1 {\n    let mut hasher = Hasher::new();\n    hash_operation(&mut hasher, &request.operation);\n    request.digest\n}",
    ),
    "tq-digest-as-proof": (
        TQ_SOURCE,
        TQ_COMMAND,
        "    ExactReferenceFieldV1(FieldExactReferenceReplayV1<'proof, 'input>),",
        "    ExactReferenceFieldV1([u8; 32]),",
    ),
}


def validate_anchor(name: str, original: str, before: str, after: str) -> None:
    """Отклоняет drift semantic-anchor до запуска дорогих mutant subprocess."""
    count = original.count(before)
    if count != 1:
        raise SystemExit(f"{name}: mutation anchor count is {count}, expected 1")
    if before == after:
        raise SystemExit(f"{name}: mutation replacement is identical to its anchor")


def main() -> None:
    """Run the bounded AUTH/TQ semantic mutation matrix and restore every source."""
    auth_original = AUTH_SOURCE.read_text(encoding="utf-8")
    tq_originals = {
        source: source.read_text(encoding="utf-8")
        for source, _command, _before, _after in TQ_MUTANTS.values()
    }

    # Сверяем всю матрицу до первого cargo subprocess: поздний stale anchor
    # должен падать сразу, а не после минут корректных мутантов. run_mutant
    # всё равно повторно сверяет anchor непосредственно перед подменой.
    for name, (before, after) in AUTH_MUTANTS.items():
        validate_anchor(name, auth_original, before, after)
    for name, (source, _command, before, after) in TQ_MUTANTS.items():
        validate_anchor(name, tq_originals[source], before, after)

    for name, (before, after) in AUTH_MUTANTS.items():
        run_mutant(name, AUTH_SOURCE, AUTH_COMMAND, before, after, auth_original)
    if AUTH_SOURCE.read_text(encoding="utf-8") != auth_original:
        raise SystemExit("authority source was not restored")

    for name, (source, command, before, after) in TQ_MUTANTS.items():
        run_mutant(name, source, command, before, after, tq_originals[source])
    for source, original in tq_originals.items():
        if source.read_text(encoding="utf-8") != original:
            raise SystemExit(f"{source}: source was not restored")
    total = len(AUTH_MUTANTS) + len(TQ_MUTANTS)
    print(f"AUTH/TQ mutation gate caught {total} semantic mutants")


if __name__ == "__main__":
    main()
