#!/usr/bin/env python3
"""Предметные подмены внешнего порта с настоящей identity чистого источника.

Переиспользует Git/процессные границы уже существующего SCI-гейта. Каждая
подмена получает локальный коммит в собственном временном worktree: ошибка
незакоммиченного источника не засчитывается как найденный дефект оценщика.
"""
from pathlib import Path
import tempfile
import json
import subprocess
from science_certificate_gate import git, run

ROOT = Path(__file__).resolve().parents[1]
CORE = "crates/labcolors-core/src/point_evaluation.rs"
APP = "crates/labcolors-evaluate-cli/src/app.rs"
CORE_COMMAND = ["cargo", "test", "-p", "labcolors-core", "--lib", "--locked"]
CLI_COMMAND = ["cargo", "test", "-p", "labcolors-evaluate-cli", "--bin", "labcolors-evaluate", "--locked"]
PROCESS_COMMAND = ["cargo", "test", "-p", "labcolors-evaluate-cli", "--test", "cli", "--locked"]
MUTANTS = (
    ("envelope-bound-before-allocation", "crates/labcolors-core/src/certificate.rs", CORE_COMMAND,
     "    if envelope_length > MAX_ENVELOPE_BYTES_V1 {", "    if false {",
     "certificate::tests::complete_envelope_size_is_bounded_before_allocation_and_finalization"),
    ("cli-main-drops-report", "crates/labcolors-evaluate-cli/src/main.rs", PROCESS_COMMAND,
     "        stdout.lock(),", "        std::io::sink(),",
     "binary_stdin_file_and_jsonl_equal_the_direct_fresh_process"),
    ("point-report-without-tq", CORE, CORE_COMMAND,
     "    authority\n        .admit_modeled_point_technical_quality(attachment, AuthorityExpectedCurrentV1::Vacant)\n        .map_err(technical_failure)?;",
     "    let _ = &attachment;",
     "point_evaluation::tests::public_report_equals_direct_owner_chain_on_the_same_attachment"),
    ("point-report-without-cc", CORE, CORE_COMMAND,
     "    authority\n        .admit_modeled_point_clean_convention(\n            attachment,\n            Some(selection),\n            AuthorityExpectedCurrentV1::Vacant,\n        )\n        .map_err(convention_failure)?;",
     "    let _ = &selection;",
     "point_evaluation::tests::public_report_equals_direct_owner_chain_on_the_same_attachment"),
    ("point-report-source-instead-of-final", CORE, CORE_COMMAND,
     "        rgb: current.materialization().terminal_composite(),",
     "        rgb: current.materialization().output().source(),",
     "point_evaluation::tests::public_report_equals_direct_owner_chain_on_the_same_attachment"),
    ("point-report-hidden-release", CORE, CORE_COMMAND,
     "        convention_release,\n        CleanConventionScopeV1::ModeledSrgb8Point,",
     "        crate::clean_set::EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2,\n        CleanConventionScopeV1::ModeledSrgb8Point,",
     "point_evaluation::tests::public_entry_checks_final_composite_and_never_defaults_the_release"),
    ("cli-skip-evaluation", APP, CLI_COMMAND,
     "        let report = evaluate_request(&request)?;",
     "        return Ok(());\n        let report = evaluate_request(&request)?;",
     "app::tests::documented_request_returns_only_declared_modeled_report_and_lcen"),
    ("cli-unearned-scope", APP, CLI_COMMAND,
     "    if profile.scope != SCOPE {", "    if false {",
     "app::tests::unsupported_profile_and_release_do_not_select_defaults"),
    ("cli-success-on-refusal", APP, CLI_COMMAND,
     "        Err(err) => finish_error(err, &mut stderr),", "        Err(_) => 0,",
     "app::tests::final_composite_rejection_never_leaks_partial_success"),
    ("cli-ignore-write-failure", APP, CLI_COMMAND,
     "        stdout\n            .write_all(&output)\n            .map_err(|_| Error::io(\"write_failed\"))",
     "        let _ = stdout.write_all(&output);\n        Ok(())",
     "app::tests::stream_failure_cannot_report_success_or_publish_a_certificate"),
    ("cli-over-input-limit", APP, CLI_COMMAND,
     "        if output.len() + read > MAX_INPUT_BYTES {",
     "        if output.len() + read > MAX_INPUT_BYTES + 1 {",
     "app::tests::reader_consumes_at_most_the_limit_plus_one_and_accepts_exact_boundary"),
)


def main() -> None:
    head = git(ROOT, "rev-parse", "HEAD").strip()
    if git(ROOT, "status", "--porcelain").strip():
        raise RuntimeError("Для CLI-проб нужен чистый источник")
    # Все мишени различаются до сборки, не после нескольких минут работы.
    for name, path, _command, before, _after, _test in MUTANTS:
        if (ROOT / path).read_text().count(before) != 1:
            raise RuntimeError(f"{name}: отсутствующая или неоднозначная мишень")
    with tempfile.TemporaryDirectory(prefix="labcolors-cli-probes-") as temporary:
        specimen = Path(temporary) / "specimen"
        git(ROOT, "worktree", "add", "--detach", str(specimen), head)
        try:
            healthy = (
                (CORE_COMMAND + ["certificate::tests::complete_envelope_size_is_bounded_before_allocation_and_finalization", "--", "--exact"], "complete_envelope_size_is_bounded_before_allocation_and_finalization"),
                (CORE_COMMAND + ["point_evaluation::tests"], "public_report_equals_direct_owner_chain_on_the_same_attachment"),
                (CLI_COMMAND, "documented_request_returns_only_declared_modeled_report_and_lcen"),
                (PROCESS_COMMAND, "binary_stdin_file_and_jsonl_equal_the_direct_fresh_process"),
            )
            for command, required_test in healthy:
                positive = run(specimen, *command)
                if positive.returncode or required_test + " ... ok" not in positive.stdout:
                    raise RuntimeError(f"Здоровый исполняемый контроль отсутствует или не прошёл\n{positive.stdout}")
            # Именно опубликованные команды, а не только внутренние функции.
            for example, expected_exit in (("declared-point.json", 0), ("rejected-point.json", 4)):
                result = subprocess.run(
                    ["cargo", "run", "--quiet", "--locked", "-p", "labcolors-evaluate-cli", "--",
                     "crates/labcolors-evaluate-cli/examples/" + example], cwd=specimen,
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False, timeout=240,
                )
                if result.returncode != expected_exit:
                    raise RuntimeError(f"Пример {example} не прошёл: {result.stderr!r}")
                if expected_exit == 0:
                    document = json.loads(result.stdout)
                    if result.stderr or document.get("kind") != "labcolors-declared-point-report-v1" or document.get("terminalSrgb8") != [128,128,128]:
                        raise RuntimeError("Документированный успешный пример нарушил контракт")
                else:
                    document = json.loads(result.stderr)
                    if result.stdout or document.get("error") != {"domain":"clean-convention", "code":"rejected_by_convention"}:
                        raise RuntimeError("Документированный отказ нарушил контракт")
                print(f"verified documented {example}", flush=True)
            for name, relative, command, before, after, test in MUTANTS:
                git(specimen, "switch", "--detach", head)
                path = specimen / relative
                original = path.read_text()
                if original.count(before) != 1:
                    raise RuntimeError(f"{name}: мишень изменилась")
                path.write_text(original.replace(before, after))
                git(specimen, "add", relative)
                git(specimen, "-c", "user.name=Lab Colors Verification", "-c", "user.email=verification@localhost",
                    "commit", "-m", f"test: isolated {name}")
                if git(specimen, "status", "--porcelain").strip():
                    raise RuntimeError("Подмена не получила чистой identity")
                negative = run(specimen, *command, test, "--", "--exact")
                marker = f"test {test} ... FAILED"
                if not negative.returncode or marker not in negative.stdout or "producer_identity_unavailable" in negative.stdout:
                    raise RuntimeError(f"{name}: нет назначенного контрпримера\n{negative.stdout}")
                print(f"caught {name}", flush=True)
            git(specimen, "switch", "--detach", head)
            if git(specimen, "status", "--porcelain").strip():
                raise RuntimeError("Исходник пробы не восстановлен")
        finally:
            git(ROOT, "worktree", "remove", "--force", str(specimen))
    if git(ROOT, "rev-parse", "HEAD").strip() != head or git(ROOT, "status", "--porcelain").strip():
        raise RuntimeError("Исходный проверяемый артефакт изменился")
    print(f"Evaluation CLI gate caught {len(MUTANTS)} semantic mutants", flush=True)


if __name__ == "__main__":
    main()
