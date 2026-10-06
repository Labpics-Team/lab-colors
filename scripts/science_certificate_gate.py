#!/usr/bin/env python3
"""Различающие пробы научного сертификата с действительным build-дескриптором.

Изменённый незакоммиченный Core намеренно не имеет producer identity. Поэтому
каждый экземпляр подмены получает отдельный локальный Git-коммит в собственном
временном worktree. Это тестовый артефакт, он не публикуется и не меняет HEAD
проверяемого репозитория. Отказ source-identity не засчитывается как пойманный дефект.
"""
from __future__ import annotations

import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = "crates/labcolors-core/src/certificate/science.rs"
EVAL = "crates/labcolors-core/src/authority/evaluation.rs"
CERT = "crates/labcolors-core/src/certificate.rs"
TEST_PREFIX = "certificate::science::tests::"
MUTANTS = (
    ("science-skip-current-tuple", SOURCE,
     "        compare_expected(decoded.admission_key(), &key)?;", "        let _ = &key;",
     "forged_producer_context_and_subject_are_refused_after_integrity_validation"),
    ("science-trust-any-integral-body", SOURCE,
     "        if payload != expected.0 {", "        if false {",
     "every_payload_byte_with_recomputed_hashes_is_rejected"),
    ("science-forget-subject", SOURCE,
     "        let subject = value.subject_identity();", "        let subject = [0_u8; 32];",
     "independent_wire_binds_exact_profile_subject_both_branches_and_color"),
    ("science-forget-technical-proof", SOURCE,
     "            tq_provenance.as_bytes(),", "            &[0_u8; 32],",
     "independent_wire_binds_exact_profile_subject_both_branches_and_color"),
    ("science-forget-convention-proof", SOURCE,
     "            cc_provenance.as_bytes(),", "            &[0_u8; 32],",
     "independent_wire_binds_exact_profile_subject_both_branches_and_color"),
    ("science-reject-all", SOURCE,
     "        let payload = DeclaredPointPayloadV1::from_evaluation(evaluation)?;",
     "        return Err(PointCertificateErrorV1::InvalidPointPayload);\n        let payload = DeclaredPointPayloadV1::from_evaluation(evaluation)?;",
     "independent_wire_binds_exact_profile_subject_both_branches_and_color"),
    ("science-public-transport-upgrade", CERT,
     "        Self::decode_class::<false>(bytes)",
     "        Self::decode_class::<true>(bytes)",
     "generic_transport_and_semantic_certificate_never_change_classes_implicitly"),
)


def git(cwd: Path, *args: str) -> str:
    return subprocess.check_output(["git", "-C", str(cwd), *args], text=True, stderr=subprocess.STDOUT, timeout=60)


def run(cwd: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(list(args), cwd=cwd, text=True, stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT, check=False, timeout=240)


def check_borrows(cwd: Path) -> None:
    path = cwd / SOURCE
    original = path.read_text()
    signature = '''
#[cfg(test)]
fn certificate_borrow_probe(
    state: &mut crate::authority::AuthorityStateV1,
    attachment: &mut crate::program_wire::ProgramAttachmentV1<crate::authority::test_support::Host>,
    profile: crate::authority::evaluation::PointQualityProfileV1,
    bytes: &mut Vec<u8>,
) {
    let evaluation = state.evaluate_declared_modeled_point(attachment, Some(profile)).unwrap();
'''
    issue = '    let issued = DeclaredPointCertificateV1::issue(&evaluation).unwrap();\n'
    verify = '    let verified = VerifiedPointCertificateV1::verify(bytes, &evaluation).unwrap();\n'
    mutation_a = '    let _ = attachment.update_unknown(2,71);\n'
    mutation_s = '    *state = crate::authority::AuthorityStateV1::new();\n'
    mutation_b = '    bytes.clear();\n'
    probes = (
        ("certificate-release-after-use", issue + '    let _ = issued.as_bytes();\n' + mutation_a + mutation_s, None),
        ("certificate-keeps-attachment", issue + mutation_a + '    let _ = issued.as_bytes();\n', 'attachment.update_unknown'),
        ("certificate-keeps-authority", issue + mutation_s + '    let _ = issued.as_bytes();\n', '*state ='),
        ("verified-keeps-input-bytes", verify + mutation_b + '    let _ = verified.as_bytes();\n', 'bytes.clear()'),
        ("verified-releases-input-after-use", verify + '    let _ = verified.as_bytes();\n' + mutation_b + mutation_a, None),
    )
    try:
        for name, body, expected_line in probes:
            path.write_text(original + signature + body + '}\n')
            outcome = run(cwd, 'cargo', 'check', '-p', 'labcolors-core', '--tests', '--locked', '--message-format=json')
            errors = []
            for line in outcome.stdout.splitlines():
                try:
                    item = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if item.get('reason') == 'compiler-message' and item['message']['level'] == 'error':
                    errors.append(item['message'])
            if expected_line is None:
                qualified = outcome.returncode == 0 and not errors
            else:
                qualified = outcome.returncode != 0 and bool(errors) and all(
                    (message.get('code') or {}).get('code') in {'E0502', 'E0506'} and any(
                        span.get('is_primary') and span['file_name'].endswith('certificate/science.rs')
                        and any(expected_line in text['text'] for text in span.get('text', []))
                        for span in message['spans']) for message in errors)
            if not qualified:
                raise RuntimeError(f'{name}: неверная причина/фаза компиляции\n{outcome.stdout}')
            print(f'verified {name}', flush=True)
    finally:
        path.write_text(original)


def main() -> None:
    head = git(ROOT, 'rev-parse', 'HEAD').strip()
    if git(ROOT, 'status', '--porcelain').strip():
        raise RuntimeError('Нужен чистый коммит для настоящего source producer')
    with tempfile.TemporaryDirectory(prefix='labcolors-science-probes-') as temporary:
        specimen = Path(temporary) / 'specimen'
        git(ROOT, 'worktree', 'add', '--detach', str(specimen), head)
        try:
            healthy = run(specimen, 'cargo', 'test', '-p', 'labcolors-core', '--lib', '--locked', TEST_PREFIX)
            if healthy.returncode:
                raise RuntimeError(f'Здоровый контроль не прошёл\n{healthy.stdout}')
            for name, relative, before, after, test in MUTANTS:
                git(specimen, 'switch', '--detach', head)
                path = specimen / relative
                content = path.read_text()
                if content.count(before) != 1:
                    raise RuntimeError(f'{name}: неоднозначная мишень')
                path.write_text(content.replace(before, after))
                git(specimen, 'add', relative)
                git(specimen, '-c', 'user.name=Lab Colors Verification', '-c', 'user.email=verification@localhost',
                    'commit', '-m', f'test: isolated {name}')
                if git(specimen, 'status', '--porcelain').strip():
                    raise RuntimeError('Подмена должна быть чистым проверяемым исходником')
                outcome = run(specimen, 'cargo', 'test', '-p', 'labcolors-core', '--lib', '--locked', TEST_PREFIX + test, '--', '--exact')
                marker = f'test {TEST_PREFIX}{test} ... FAILED'
                if not outcome.returncode or marker not in outcome.stdout or 'producer_identity_unavailable' in outcome.stdout:
                    raise RuntimeError(f'{name}: не целевой контрпример\n{outcome.stdout}')
                print(f'caught {name}', flush=True)
            git(specimen, 'switch', '--detach', head)
            check_borrows(specimen)
            if git(specimen, 'status', '--porcelain').strip():
                raise RuntimeError('Исходники проб не восстановлены')
        finally:
            git(ROOT, 'worktree', 'remove', '--force', str(specimen))
    if git(ROOT, 'rev-parse', 'HEAD').strip() != head or git(ROOT, 'status', '--porcelain').strip():
        raise RuntimeError('Исходный проверяемый артефакт изменился')
    print(f'Science certificate gate caught {len(MUTANTS)} semantic mutants; 5 borrow probes verified', flush=True)


if __name__ == '__main__':
    main()
