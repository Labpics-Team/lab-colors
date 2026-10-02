#!/usr/bin/env python3
"""AUTH/TQ/CC/EVAL: целевые семантические мутации обязаны делать целевой тест Core красным."""

from __future__ import annotations

from pathlib import Path
import argparse
import json
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
    expected_marker = COMPILE_KILLED.get(name, (LIFECYCLE_FAILURES | POINT_FAILURES | RASTER_FAILURES | HANDOFF_FAILURES | CC_FAILURES | EVAL_FAILURES).get(name, "test result: FAILED"))
    if expected_marker not in result.stdout:
        sys.stderr.write(result.stdout)
        raise SystemExit(
            f"{name}: unexpected failure; expected marker {expected_marker!r}"
        )
    print(f"caught {name}")


TQ_MUTANTS = {
    "tq-stale-point-binding": (
        AUTH_SOURCE,
        TQ_COMMAND,
        "    hasher.update(&materialization.revision().to_be_bytes());\n    hasher.update(&sink_stamp.sequence().to_be_bytes());",
        "    hasher.update(&0_u64.to_be_bytes());\n    hasher.update(&0_u64.to_be_bytes());",
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


# Проверенный writer не заменяет проверку его применения настоящим Attachment.
HANDOFF_COMMAND = ["cargo", "test", "-p", "labcolors-core", "--lib", "--locked",
                   "program::attachment::handoff::tests::exhaustive_handoff_histories_keep_host_session_and_authority_in_sync",
                   "--", "--exact"]
HANDOFF_SOURCE = ROOT / "crates/labcolors-core/src/program/attachment.rs"
HANDOFF_MUTANTS = {
    "handoff-uncommitted-session": (
        HANDOFF_SOURCE, HANDOFF_COMMAND,
        "        let _view = transition.commit_deferred();", "        drop(transition);",
    ),
    "handoff-lost-publication-stamp": (
        HANDOFF_SOURCE, HANDOFF_COMMAND,
        "        self.expected_sink_stamp = desired_sink_stamp;", "        let _ = desired_sink_stamp;",
    ),
}
HANDOFF_FAILURES = {
    name: "test program::attachment::handoff::tests::exhaustive_handoff_histories_keep_host_session_and_authority_in_sync ... FAILED"
    for name in HANDOFF_MUTANTS
}


# CC проверяется через настоящий attachment; старые TQ/AUTH фальсификаторы сохранены.
CC_SOURCE = ROOT / "crates/labcolors-core/src/authority/clean_convention.rs"
CC_COMMAND = ["cargo", "test", "-p", "labcolors-core", "--lib", "--locked",
              "authority::clean_convention::tests"]
CC_MUTANTS = {
    "cc-foreign-release": (CC_SOURCE, CC_COMMAND,
        "        if release != EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V1 {",
        "        if false {"),
    "cc-unsupported-scope": (CC_SOURCE, CC_COMMAND,
        "        if scope != CleanConventionScopeV1::ModeledSrgb8Point {",
        "        if false {"),
    "cc-unearned-admission": (CC_SOURCE, CC_COMMAND,
        "        if admission != CleanConventionAdmissionKindV1::DeclaredPackagePolicyCandidate {",
        "        if false {"),
    "cc-hidden-default": (CC_SOURCE, CC_COMMAND,
        "        let selection = selection.ok_or(CleanConventionErrorV1::SelectionRequired)?;",
        "        let selection = selection.unwrap_or(CleanConventionSelectionV1 { release: EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V1, scope: CleanConventionScopeV1::ModeledSrgb8Point, admission: CleanConventionAdmissionKindV1::DeclaredPackagePolicyCandidate });"),
    "cc-source-instead-of-composite": (CC_SOURCE, CC_COMMAND,
        "        let composite = materialization.terminal_composite();",
        "        let composite = materialization.output().source();"),
    "cc-ignore-classifier": (CC_SOURCE, CC_COMMAND,
        "            ExactNominalSrgb8CleanSetV1.classify(composite)",
        "            ExactNominalSrgb8CleanSetV1.classify(Srgb8::new([0; 3]))"),
    "cc-reject-all": (CC_SOURCE, CC_COMMAND,
        "        let composite = materialization.terminal_composite();",
        "        return Err(CleanConventionErrorV1::UnsupportedScope);\n        let composite = materialization.terminal_composite();"),
    "cc-foreign-lane": (CC_SOURCE, CC_COMMAND,
        "            AuthorityIdV1::CleanConvention,",
        "            AuthorityIdV1::TechnicalQuality,"),
    "cc-unbound-technical-receipt": (CC_SOURCE, CC_COMMAND,
        "        provenance.update(TECHNICAL_RECEIPT_PIN);",
        "        provenance.update(&[]);"),
    "cc-unbound-materialization": (CC_SOURCE, CC_COMMAND,
        "        hash_modeled_point_identity(self.materialization, &mut provenance);",
        "        let _ = self.materialization;"),
}
CC_FAILURES = {
    name: "test authority::clean_convention::tests::" + test + " ... FAILED"
    for name, test in {
        "cc-foreign-release": "explicit_selection_rejects_foreign_release_scope_and_unearned_admission",
        "cc-unsupported-scope": "explicit_selection_rejects_foreign_release_scope_and_unearned_admission",
        "cc-unearned-admission": "explicit_selection_rejects_foreign_release_scope_and_unearned_admission",
        "cc-hidden-default": "missing_selection_and_non_ready_materialization_cannot_mint_authority",
        "cc-source-instead-of-composite": "accepted_source_does_not_admit_rejected_terminal_composite",
        "cc-ignore-classifier": "accepted_source_does_not_admit_rejected_terminal_composite",
        "cc-reject-all": "rejected_source_can_admit_accepted_terminal_composite_without_other_authorities",
        "cc-foreign-lane": "rejected_source_can_admit_accepted_terminal_composite_without_other_authorities",
        "cc-unbound-technical-receipt": "receipt_identity_binds_selection_technical_pin_and_every_materialization_coordinate",
        "cc-unbound-materialization": "receipt_identity_binds_selection_technical_pin_and_every_materialization_coordinate",
    }.items()
}


# Совместный EVAL должен сохранять оба обязательства, а не только дать общий PASS.
EVAL_SOURCE = ROOT / "crates/labcolors-core/src/authority/evaluation.rs"
EVAL_COMMAND = ["cargo", "test", "-p", "labcolors-core", "--lib", "--locked",
                "authority::evaluation::tests"]
EVAL_MUTANTS = {
    "eval-skip-technical": (EVAL_SOURCE, EVAL_COMMAND,
        "        self.require(technical)",
        "        Ok::<(), AuthorityRequireErrorV1>(())"),
    "eval-skip-convention": (EVAL_SOURCE, EVAL_COMMAND,
        "        self.require(convention)",
        "        Ok::<(), AuthorityRequireErrorV1>(())"),
    "eval-trust-old-technical": (EVAL_SOURCE, EVAL_COMMAND,
        "        let technical = fresh.read(AuthorityIdV1::TechnicalQuality).ok_or(",
        "        let technical = self.read(AuthorityIdV1::TechnicalQuality).ok_or("),
    "eval-trust-old-convention": (EVAL_SOURCE, EVAL_COMMAND,
        "        let convention = checked.descriptor();",
        "        let convention = self.read(AuthorityIdV1::CleanConvention).unwrap_or(checked.descriptor());"),
    "eval-return-one-branch-twice": (EVAL_SOURCE, EVAL_COMMAND,
        "            technical,\n            convention,",
        "            technical: convention,\n            convention,"),
    "eval-reject-all": (EVAL_SOURCE, EVAL_COMMAND,
        "        let profile = profile.ok_or(PointEvaluationErrorV1::ProfileRequired)?;",
        "        return Err(PointEvaluationErrorV1::ProfileRequired);\n        let profile = profile.ok_or(PointEvaluationErrorV1::ProfileRequired)?;"),
    "eval-unbound-profile-release": (EVAL_SOURCE, EVAL_COMMAND,
        "        hasher.update(&self.convention.release());",
        "        hasher.update(&[0_u8; 32]);"),
}
EVAL_FAILURES = {
    name: "test authority::evaluation::tests::" + test + " ... FAILED"
    for name, test in {
        "eval-skip-technical": "explicit_profile_and_both_installed_authorities_are_required",
        "eval-skip-convention": "explicit_profile_and_both_installed_authorities_are_required",
        "eval-trust-old-technical": "same_rgb_new_revision_rejects_each_stale_branch_and_recovers",
        "eval-trust-old-convention": "same_rgb_new_revision_rejects_each_stale_branch_and_recovers",
        "eval-return-one-branch-twice": "result_preserves_one_subject_profile_and_distinct_current_branches",
        "eval-reject-all": "result_preserves_one_subject_profile_and_distinct_current_branches",
        "eval-unbound-profile-release": "result_preserves_one_subject_profile_and_distinct_current_branches",
    }.items()
}


def verify_evaluation_borrows() -> None:
    """Реальный Rust-тип удерживает оба источника, но освобождает их после использования."""
    original = EVAL_SOURCE.read_text(encoding="utf-8")
    use = "    let _ = result.materialization();\n"
    mutate_attachment = "    let _ = attachment.update_unknown(2, 1);\n"
    mutate_authority = "    let _ = state.admit_modeled_point_technical_quality(attachment, AuthorityExpectedCurrentV1::Vacant);\n"

    def probe(source: str, body: str, expected_error: bool, name: str) -> None:
        code = """
#[cfg(test)]
fn evaluation_borrow_contract_probe(
    state: &mut AuthorityStateV1,
    attachment: &mut ProgramAttachmentV1<super::test_support::Host>,
    profile: PointQualityProfileV1,
) {
    let result = state.evaluate_declared_modeled_point(attachment, Some(profile)).unwrap();
""" + body + "}\n"
        try:
            EVAL_SOURCE.write_text(source + code, encoding="utf-8")
            completed = subprocess.run(
                ["cargo", "check", "-p", "labcolors-core", "--tests", "--locked", "--message-format=json"],
                cwd=ROOT, text=True, capture_output=True, check=False,
            )
        finally:
            EVAL_SOURCE.write_text(original, encoding="utf-8")
        diagnostics = []
        for line in completed.stdout.splitlines():
            try:
                item = json.loads(line)
            except json.JSONDecodeError:
                continue
            if item.get("reason") == "compiler-message" and item["message"]["level"] == "error":
                diagnostics.append(item["message"])
        qualified = completed.returncode == 0 and not diagnostics
        if expected_error:
            qualified = completed.returncode != 0 and bool(diagnostics) and all(
                (message.get("code") or {}).get("code") == "E0502"
                and any(span["file_name"].endswith("authority/evaluation.rs") for span in message["spans"])
                for message in diagnostics
            )
        if not qualified:
            sys.stderr.write(completed.stdout + completed.stderr)
            raise SystemExit(f"{name}: evaluation borrow contract did not produce its required compiler outcome")
        print(f"verified {name}")

    probe(original, use + mutate_attachment + mutate_authority, False, "eval-release-after-use")
    probe(original, mutate_attachment + use, True, "eval-keeps-attachment-borrowed")
    probe(original, mutate_authority + use, True, "eval-keeps-authority-borrowed")
    # Различающий отрицательный контроль: отвязанное время жизни действительно
    # разрешает запрещённое действие. Ошибка компиляции не засчитывается за него.
    before = "-> Result<CurrentPointEvaluationV1<'a>, PointEvaluationErrorV1>"
    after = "-> Result<CurrentPointEvaluationV1<'static>, PointEvaluationErrorV1>"
    validate_anchor("eval-detached-result", original, before, after)
    probe(original.replace(before, after), mutate_attachment + mutate_authority + use,
          False, "eval-detached-result-counterexample")
    if EVAL_SOURCE.read_text(encoding="utf-8") != original:
        raise SystemExit("evaluation source was not restored")


CC_SELECTION_MUTANTS = (
    "cc-foreign-release",
    "cc-unsupported-scope",
    "cc-unearned-admission",
    "cc-hidden-default",
    "cc-source-instead-of-composite",
)
CC_BINDING_MUTANTS = (
    "cc-ignore-classifier",
    "cc-reject-all",
    "cc-foreign-lane",
    "cc-unbound-technical-receipt",
    "cc-unbound-materialization",
)

REQUIRED_SCOPES = (
    "authority",
    "tq",
    "lifecycle-geometry",
    "cc-selection",
    "cc-binding",
    "eval",
    "science",
    "cli",
)

MUTATION_SCOPES = {
    "authority": {
        "authority": tuple(AUTH_MUTANTS),
        "bounded": (),
    },
    "tq": {
        "authority": (),
        "bounded": tuple(TQ_MUTANTS),
    },
    "lifecycle-geometry": {
        "authority": (),
        "bounded": tuple(
            LIFECYCLE_MUTANTS | POINT_MUTANTS | RASTER_MUTANTS | HANDOFF_MUTANTS
        ),
    },
    "cc-selection": {
        "authority": (),
        "bounded": CC_SELECTION_MUTANTS,
    },
    "cc-binding": {
        "authority": (),
        "bounded": CC_BINDING_MUTANTS,
    },
    "eval": {
        "authority": (),
        "bounded": tuple(EVAL_MUTANTS),
    },
}


def validate_scope_partition() -> None:
    """Every semantic mutant belongs to exactly one required CI shard."""
    expected_authority = set(AUTH_MUTANTS)
    expected_bounded = set(
        TQ_MUTANTS | LIFECYCLE_MUTANTS | POINT_MUTANTS | RASTER_MUTANTS
        | HANDOFF_MUTANTS | CC_MUTANTS | EVAL_MUTANTS
    )
    seen_authority: list[str] = []
    seen_bounded: list[str] = []
    for scope in REQUIRED_SCOPES:
        partition = MUTATION_SCOPES.get(scope)
        if partition is None:
            continue
        seen_authority.extend(partition["authority"])
        seen_bounded.extend(partition["bounded"])
    if set(seen_authority) != expected_authority or len(seen_authority) != len(expected_authority):
        raise SystemExit("authority mutation shard partition is incomplete or duplicated")
    if set(seen_bounded) != expected_bounded or len(seen_bounded) != len(expected_bounded):
        raise SystemExit("bounded mutation shard partition is incomplete or duplicated")


def run_mutation_scope(scope: str) -> None:
    """Execute one disjoint in-process mutant partition and restore every source."""
    partition = MUTATION_SCOPES[scope]
    authority_names = partition["authority"]
    bounded_names = partition["bounded"]
    bounded_mutants = (
        TQ_MUTANTS | LIFECYCLE_MUTANTS | POINT_MUTANTS | RASTER_MUTANTS
        | HANDOFF_MUTANTS | CC_MUTANTS | EVAL_MUTANTS
    )
    auth_original = AUTH_SOURCE.read_text(encoding="utf-8")
    selected = {name: bounded_mutants[name] for name in bounded_names}
    originals = {
        source: source.read_text(encoding="utf-8")
        for source, _command, _before, _after in selected.values()
    }

    for name in authority_names:
        before, after = AUTH_MUTANTS[name]
        validate_anchor(name, auth_original, before, after)
    for name, (source, _command, before, after) in selected.items():
        validate_anchor(name, originals[source], before, after)

    for name in authority_names:
        before, after = AUTH_MUTANTS[name]
        run_mutant(name, AUTH_SOURCE, AUTH_COMMAND, before, after, auth_original)
    if AUTH_SOURCE.read_text(encoding="utf-8") != auth_original:
        raise SystemExit("authority source was not restored")

    for name, (source, command, before, after) in selected.items():
        run_mutant(name, source, command, before, after, originals[source])
    for source, original in originals.items():
        if source.read_text(encoding="utf-8") != original:
            raise SystemExit(f"{source}: source was not restored")

    if scope == "eval":
        verify_evaluation_borrows()
    print(f"mutation shard {scope}: caught {len(authority_names) + len(bounded_names)} semantic mutants")


def run_external_gate(script: str, label: str) -> None:
    """Execute one existing standalone semantic gate as its own CI shard."""
    subprocess.run([sys.executable, str(ROOT / "scripts" / script)], cwd=ROOT, check=True)
    print(f"mutation shard {label}: passed")


def run_scope(scope: str) -> None:
    """Dispatch a validated required scope to its canonical owner."""
    if scope in MUTATION_SCOPES:
        run_mutation_scope(scope)
    elif scope == "science":
        run_external_gate("science_certificate_gate.py", scope)
    elif scope == "cli":
        run_external_gate("evaluation_cli_gate.py", scope)
    else:
        raise SystemExit(f"unknown mutation scope: {scope}")


def main(argv: list[str] | None = None) -> None:
    """Run all semantic proof shards or one independently schedulable shard."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scope", choices=("all", *REQUIRED_SCOPES), default="all")
    args = parser.parse_args([] if argv is None else argv)
    validate_scope_partition()
    scopes = REQUIRED_SCOPES if args.scope == "all" else (args.scope,)
    for scope in scopes:
        run_scope(scope)
    if args.scope == "all":
        bounded = (
            TQ_MUTANTS | LIFECYCLE_MUTANTS | POINT_MUTANTS | RASTER_MUTANTS
            | HANDOFF_MUTANTS | CC_MUTANTS | EVAL_MUTANTS
        )
        print(f"AUTH/TQ/CC/EVAL mutation gate caught {len(AUTH_MUTANTS) + len(bounded)} semantic mutants")


if __name__ == "__main__":
    main(sys.argv[1:])
