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
    expected_marker = COMPILE_KILLED.get(name, (LIFECYCLE_FAILURES | POINT_FAILURES | RASTER_FAILURES).get(name, "test result: FAILED"))
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
        "    ExactReferenceFieldV1(FieldExactReferenceReplayV1<'proof>),",
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


# Связка формально проверенного решения с настоящим lifecycle. Подмена
# самого caller не должна обходить доказанный helper и оставаться незамеченной.
LIFECYCLE_COMMAND = ["cargo", "test", "-p", "labcolors-core", "--lib", "--locked", "lifecycle_"]
LIFECYCLE_MUTANTS = {
    "session-stale-evidence-admission": (
        ROOT / "crates/labcolors-core/src/session.rs", LIFECYCLE_COMMAND,
        "            if !decision.observation().is_same_binding_as(&raw_observation) {",
        "            if false {",
    ),
    "observation-stale-order-bypass": (
        ROOT / "crates/labcolors-core/src/observation.rs", LIFECYCLE_COMMAND,
        "        if revision < current {",
        "        if false && revision < current {",
    ),
    "ledger-budget-helper-bypass": (
        ROOT / "crates/labcolors-core/src/certificate.rs", LIFECYCLE_COMMAND,
        "            self.records.len(),\n            self.accounted_bytes,",
        "            0,\n            0,",
    ),
}


LIFECYCLE_FAILURES = {
    "session-stale-evidence-admission": "test session::lifecycle_tests::lifecycle_rejects_saved_evidence_for_a_new_revision ... FAILED",
    "observation-stale-order-bypass": "test session::lifecycle_tests::lifecycle_observed_order_cancel_and_failure_matrix ... FAILED",
    "ledger-budget-helper-bypass": "test certificate::lifecycle_tests::lifecycle_replay_conflict_and_capacity_use_the_real_ledger ... FAILED",
}


# Полнота поиска и авторская минимальность проверяются на настоящем coordinator.
POINT_COMMAND = ["cargo", "test", "-p", "labcolors-core", "--lib", "--locked",
                 "point_representation::frontier_tests::crossed_rgb_frontiers_require_all_three_channels",
                 "--", "--exact"]
POINT_SOURCE = ROOT / "crates/labcolors-core/src/point_representation.rs"
POINT_MUTANTS = {
    "point-discard-minimal-selection": (
        POINT_SOURCE, POINT_COMMAND,
        "    let selected = feasible_lower;",
        "    let selected = domain.upper(); let _ = feasible_lower;",
    ),
    "point-feasible-on-any-channel": (
        POINT_SOURCE, POINT_COMMAND,
        "    (0..3).all(|channel| match target[channel].cmp(&backdrop[channel]) {",
        "    (0..3).any(|channel| match target[channel].cmp(&backdrop[channel]) {",
    ),
}
POINT_FAILURES = {
    name: "test point_representation::frontier_tests::crossed_rgb_frontiers_require_all_three_channels ... FAILED"
    for name in POINT_MUTANTS
}


# Поле проверяется через настоящий full/incremental путь и независимую 2D-свёртку.
RASTER_COMMAND = ["cargo", "test", "-p", "labcolors-core", "--lib", "--locked",
                  "field_effect_tests::raster_tests"]
RASTER_SOURCE = ROOT / "crates/labcolors-core/src/field_effect.rs"
RASTER_MUTANTS = {
    "field-missing-blur-halo": (
        RASTER_SOURCE, RASTER_COMMAND,
        "    let exact = dirty_input.expanded(request.operation.radius(), request.geometry().extent())?;",
        "    let exact = dirty_input.expanded(0, request.geometry().extent())?;",
    ),
    "field-hidden-dirty-change": (
        RASTER_SOURCE, RASTER_COMMAND,
        "    verify_incremental_change_scope(previous_request, request, dirty_input)?;",
        "    let _ = (previous_request, request, dirty_input);",
    ),
    "field-biased-blur-rounding": (
        RASTER_SOURCE, RASTER_COMMAND,
        "                    .checked_add(1_u128 << 63)",
        "                    .checked_add(0)",
    ),
}
RASTER_FAILURES = {
    "field-missing-blur-halo": "test field_effect_tests::raster_tests::every_small_dirty_rectangle_matches_independent_full_convolution ... FAILED",
    "field-hidden-dirty-change": "test field_effect_tests::raster_tests::rejected_dirty_scope_preserves_output_and_allows_correct_retry ... FAILED",
    "field-biased-blur-rounding": "test field_effect_tests::raster_tests::every_small_dirty_rectangle_matches_independent_full_convolution ... FAILED",
}


def main() -> None:
    """Run the bounded AUTH/TQ semantic mutation matrix and restore every source."""
    auth_original = AUTH_SOURCE.read_text(encoding="utf-8")
    bounded_mutants = TQ_MUTANTS | LIFECYCLE_MUTANTS | POINT_MUTANTS | RASTER_MUTANTS
    tq_originals = {
        source: source.read_text(encoding="utf-8")
        for source, _command, _before, _after in bounded_mutants.values()
    }

    # Сверяем всю матрицу до первого cargo subprocess: поздний stale anchor
    # должен падать сразу, а не после минут корректных мутантов. run_mutant
    # всё равно повторно сверяет anchor непосредственно перед подменой.
    for name, (before, after) in AUTH_MUTANTS.items():
        validate_anchor(name, auth_original, before, after)
    for name, (source, _command, before, after) in bounded_mutants.items():
        validate_anchor(name, tq_originals[source], before, after)

    for name, (before, after) in AUTH_MUTANTS.items():
        run_mutant(name, AUTH_SOURCE, AUTH_COMMAND, before, after, auth_original)
    if AUTH_SOURCE.read_text(encoding="utf-8") != auth_original:
        raise SystemExit("authority source was not restored")

    for name, (source, command, before, after) in bounded_mutants.items():
        run_mutant(name, source, command, before, after, tq_originals[source])
    for source, original in tq_originals.items():
        if source.read_text(encoding="utf-8") != original:
            raise SystemExit(f"{source}: source was not restored")
    total = len(AUTH_MUTANTS) + len(bounded_mutants)
    print(f"AUTH/TQ mutation gate caught {total} semantic mutants")


if __name__ == "__main__":
    main()
