#!/usr/bin/env python3
"""Проверка секретов в путях, которые Trivy по умолчанию исключает."""

from __future__ import annotations

from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys
import tempfile


TRIVY_VERSION = "0.74.0"
SECRET_CONFIG = (
    "disable-allow-rules:\n"
    "  - tests\n"
    "  - examples\n"
    "  - markdown\n"
    "skip-patterns: []\n"
)
PROBE_PATHS = frozenset(
    {"README.md", "tests/secret.txt", "examples/secret.txt", "package-lock.json", "tokens.json"}
)
MAX_REPORT_BYTES = 64 * 1024 * 1024


class IncompleteScan(Exception):
    """Сканер не предоставил согласованный полный результат."""


def _secret_paths(report: object, subject: Path, status: int, started: datetime) -> set[str]:
    if not isinstance(report, dict):
        raise IncompleteScan("Trivy вернул отчёт неверной формы")
    created = report.get("CreatedAt")
    try:
        timestamp = datetime.fromisoformat(created) if isinstance(created, str) else None
    except ValueError as error:
        raise IncompleteScan("Trivy вернул неверное время отчёта") from error
    if (
        report.get("SchemaVersion") != 2
        or report.get("ArtifactType") not in {"filesystem", "repository"}
        or report.get("ArtifactName") != str(subject)
        or not isinstance(report.get("Trivy"), dict)
        or report["Trivy"].get("Version") != TRIVY_VERSION
        or timestamp is None
        or timestamp.tzinfo is None
        or abs((timestamp - started).total_seconds()) > 600
    ):
        raise IncompleteScan("Trivy вернул отчёт другого предмета, версии или запуска")
    results = report.get("Results")
    if not isinstance(results, list):
        raise IncompleteScan("Trivy не вернул список результатов")
    paths: set[str] = set()
    count = 0
    for result in results:
        if not isinstance(result, dict) or result.get("Class") not in {
            "secret", "lang-pkgs", "os-pkgs", "config", "license", "license-file"
        }:
            raise IncompleteScan("Trivy вернул неожиданный класс результата")
        if result.get("Class") != "secret":
            if result.get("Secrets"):
                raise IncompleteScan("Trivy вернул секрет вне секретного сканера")
            continue
        target = result.get("Target")
        if not isinstance(target, str) or not target or target.startswith("/"):
            raise IncompleteScan("Trivy вернул неверный путь результата")
        normalized = PurePosixPath(target)
        if ".." in normalized.parts or str(normalized) != target:
            raise IncompleteScan("Trivy вернул путь вне предмета проверки")
        secrets = result.get("Secrets")
        if not isinstance(secrets, list) or not secrets:
            raise IncompleteScan("Trivy вернул пустой результат секрета")
        for finding in secrets:
            if not isinstance(finding, dict) or not isinstance(finding.get("RuleID"), str):
                raise IncompleteScan("Trivy вернул неверную находку секрета")
        paths.add(target)
        count += len(secrets)
    if status not in {0, 1} or (status == 0) != (count == 0):
        raise IncompleteScan("Код возврата Trivy не согласуется с находками")
    return paths


def _run_scan(binary: Path, subject: Path, config: Path, work: Path, label: str) -> set[str]:
    report = work / f"{label}.json"
    diagnostics = work / f"{label}.stderr"
    environment = {
        "PATH": os.environ.get("PATH", "/usr/local/bin:/usr/bin:/bin"),
        "HOME": str(work),
        "TMPDIR": str(work),
        "LANG": "C.UTF-8",
        "LC_ALL": "C.UTF-8",
        "NO_COLOR": "1",
    }
    command = [
        str(binary), "fs", "--config", os.devnull,
        "--secret-config", str(config), "--ignorefile", os.devnull,
        "--format", "json", "--exit-code", "1", "--scanners", "secret",
        "--severity", "UNKNOWN,LOW,MEDIUM,HIGH,CRITICAL",
        "--offline-scan", "--skip-db-update", "--skip-java-db-update",
        "--skip-check-update", "--skip-vex-repo-update",
        "--disable-telemetry", "--skip-version-check", "--no-progress", "--quiet",
        "--cache-dir", str(work / "cache"), str(subject),
    ]
    started = datetime.now(timezone.utc)
    try:
        with report.open("wb") as output, diagnostics.open("wb") as error_output:
            completed = subprocess.run(
                command, cwd=work, env=environment, stdin=subprocess.DEVNULL,
                stdout=output, stderr=error_output, timeout=360, check=False,
            )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise IncompleteScan("Trivy не завершил проверку") from error
    if report.stat().st_size > MAX_REPORT_BYTES:
        raise IncompleteScan("Отчёт Trivy превысил допустимый размер")
    try:
        raw = json.loads(report.read_bytes())
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        digest = hashlib.sha256(diagnostics.read_bytes()).hexdigest()
        raise IncompleteScan(f"Trivy не вернул JSON; status={completed.returncode}; diagnosticDigest={digest}") from error
    return _secret_paths(raw, subject, completed.returncode, started)


def _prepare_probe(work: Path) -> Path:
    probe = work / "probe"
    # Синтетический AWS-shaped маркер составляется при исполнении: в Git нет ключа.
    marker = "AKIA" + "ABCDEFGHIJKLMNOP"
    for relative in PROBE_PATHS:
        file = probe / relative
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text(f"access_key_id = {marker}\n", encoding="utf-8")
    return probe


def _verify_candidate(subject: Path, sha: str) -> None:
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        raise IncompleteScan("Не задан точный SHA кандидата")
    try:
        head = subprocess.run(
            ["git", "-C", str(subject), "rev-parse", "HEAD"],
            capture_output=True, text=True, timeout=10, check=False,
        )
        status = subprocess.run(
            ["git", "-C", str(subject), "status", "--porcelain", "--untracked-files=all"],
            capture_output=True, text=True, timeout=10, check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise IncompleteScan("Идентичность checkout кандидата недоступна") from error
    if head.returncode or head.stdout.strip() != sha or status.returncode or status.stdout.strip():
        raise IncompleteScan("Checkout кандидата не совпадает с SHA или изменён")


def run(binary: Path, subject: Path, sha: str, scratch: Path) -> int:
    subject = subject.resolve(strict=True)
    binary = binary.resolve(strict=True)
    scratch = scratch.resolve(strict=True)
    _verify_candidate(subject, sha)
    with tempfile.TemporaryDirectory(prefix="strict-secret-", dir=scratch) as temporary:
        work = Path(temporary)
        config = work / "secret.yaml"
        config.write_text(SECRET_CONFIG, encoding="utf-8")
        probe = _prepare_probe(work)
        if _run_scan(binary, probe, config, work, "probe") != PROBE_PATHS:
            raise IncompleteScan("Trivy не обнаружил все контрольные секреты")
        findings = _run_scan(binary, subject, config, work, "candidate")
        if findings:
            print(f"Проверка секретов отклонена: {len(findings)} файлов; совпадения скрыты", file=sys.stderr)
            return 1
    print(f"Секреты в кандидате {sha} не обнаружены в поддерживаемых Trivy файлах")
    return 0


if __name__ == "__main__":
    try:
        if len(sys.argv) != 5:
            raise IncompleteScan("usage: strict_secret_scan.py TRIVY SUBJECT SHA SCRATCH")
        raise SystemExit(run(Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3], Path(sys.argv[4])))
    except (IncompleteScan, OSError) as error:
        print(f"Проверка секретов не завершена: {error}", file=sys.stderr)
        raise SystemExit(2) from error
