#!/usr/bin/env python3
"""Исполняемая матрица артефактов: закреплённый инвентарь × обязательный CI.

Две закреплённые записи в `proof/artifact/`; обе коммитятся и в CI только
пересчитываются и сравниваются каноническими байтами:

* `tree.json` — `path -> git blob id` для объявленных корней. Blob id считает
  Git с применением `.gitattributes`, поэтому CRLF-чекаут и порядок сортировки
  ОС не влияют. Добавленный, удалённый или изменённый файл — drift. Это
  закрывает omission/decoy/parser на уровне байтов без regex-парсеров.
* `tests.json` — инвентарь тестов (запись платформенная: закрепляется
  состояние Linux CI toolchain, `refresh` выполняется в нём; Windows не
  импортирует Linux-only модули proof и потому не является источником записи).
  Сила по экосистемам различна и названа честно:
  Rust — compiler-resolved: `cargo test -- --list` и `-- --ignored --list`;
  новый `#[ignore]` или `cfg`-скрытый модуль меняет инвентарь и называет тест.
  Python — loader-resolved: имена тестов и skip, истинные на момент импорта
  в среде refresh (`__unittest_skip__` на классе/методе, включая `skipIf/
  skipUnless` с истинным условием); `self.skipTest`, `SkipTest` в `setUp*`
  и `load_tests` невидимы; ошибка импорта модуля — отдельное поле.
  Node — только текстовые сайты отключения (`x.skip(`, `x.todo(`,
  `{ skip: … }`); инвентаря тестов Node нет. Это закрывает disabled-test для
  Rust полностью, для Python — статически, для Node — по форме записи.

Инвентарь — не семантическое доказательство (INV-03): матрица говорит, что
существует и исполняется, а не что истинно. `refresh` переписывает записи
текущим состоянием и допустим только в PR, который меняет артефакты; diff
записей — часть review.

Коды: 0 OK; 1 drift; 64 порча записи / неверный вызов; 65 отказ инструмента.
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
ARTIFACT_DIR = REPO_ROOT / "proof" / "artifact"

# Объявленные корни production/proof/CI артефактов. Файл вне корней не
# существует для матрицы; новый корень = правка списка + refresh.
TREE_ROOTS: tuple[str, ...] = (
    ".github/workflows",
    "Cargo.toml",
    "Cargo.lock",
    "crates",
    "conformance",
    "bindings/swift",
    "packages/colors",
    "proof",
    "scripts",
)
# Внутри корней исключаются только каталоги результатов сборки/зависимостей.
TREE_EXCLUDE_DIRS: frozenset[str] = frozenset({"node_modules", "target", "pkg", "__pycache__", ".build"})

PYTHON_SUITES: tuple[tuple[str, str], ...] = (
    ("proof/region/v1/tests", "test_*.py"),
    ("scripts", "test_*.py"),
)
NODE_TEST_GLOBS: tuple[str, ...] = ("packages/colors/test/*.test.mjs",)
# Формы отключения node:test: `x.skip(`, `x.todo(`, опция `{ skip: ... }` / `{ todo: ... }`.
NODE_SKIP_RE = re.compile(r"\b(?:t|context|test|it|describe|suite)\.(?:skip|todo)\(|[{,]\s*(?:skip|todo)\s*:")

EXIT_OK, EXIT_DRIFT, EXIT_USAGE, EXIT_TOOL = 0, 1, 64, 65


class MalformedRecord(Exception):
    """Запись не разбирается как объявленная; это порча, не drift."""


def git(*args: str, stdin: bytes | None = None) -> bytes:
    result = subprocess.run(["git", *args], input=stdin, capture_output=True, cwd=REPO_ROOT)
    if result.returncode != 0:
        raise RuntimeError(f"git {' '.join(args[:2])}: {result.stderr.decode(errors='replace').strip()}")
    return result.stdout


def canonical(record: dict) -> bytes:
    body = {key: value for key, value in record.items() if key != "record_sha256"}
    return json.dumps(body, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def seal(record: dict) -> dict:
    record = {key: value for key, value in record.items() if key != "record_sha256"}
    record["record_sha256"] = hashlib.sha256(canonical(record)).hexdigest()
    return record


def parse_record(raw: bytes, expected_class: str) -> dict:
    try:
        record = json.loads(raw.decode("utf-8-sig"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise MalformedRecord(f"not JSON: {error}") from error
    if not isinstance(record, dict):
        raise MalformedRecord("record must be a JSON object")
    for key in ("class", "schema_version", "record_sha256"):
        if key not in record:
            raise MalformedRecord(f"missing key {key!r}")
    if record["class"] != expected_class:
        raise MalformedRecord(f"class {record['class']!r} != expected {expected_class!r}")
    if type(record["schema_version"]) is not int:
        raise MalformedRecord("schema_version must be an integer")
    digest = hashlib.sha256(canonical(record)).hexdigest()
    if record["record_sha256"] != digest:
        raise MalformedRecord(f"record_sha256 {record['record_sha256'][:16]}… != body {digest[:16]}…")
    return record


# ---------------------------------------------------------------- tree ----

def _within_roots(path: str) -> bool:
    # Записи матрицы не описывают сами себя: иначе refresh никогда не сходится.
    if path.startswith("proof/artifact/"):
        return False
    if any(part in TREE_EXCLUDE_DIRS for part in path.split("/")):
        return False
    return any(path == root or path.startswith(root + "/") for root in TREE_ROOTS)


def extract_tree() -> dict:
    """path -> blob id для файлов рабочего дерева под корнями.

    Набор путей — индекс плюс untracked (не ignored): decoy-файл, ещё не
    добавленный в индекс, обязан быть drift, а не невидимкой. Blob id всегда
    считается по байтам рабочего дерева через `hash-object --stdin-paths` —
    с применением `.gitattributes`, как при `git add`, поэтому eol-конверсия
    чекаута не влияет; tracked файл, удалённый из рабочего дерева, исчезает из
    записи (drift), а изменённый меняет blob. Symlink записывается blob цели из
    индекса: рабочее дерево на Windows его не воспроизводит. Режим файла в запись
    не входит: он зависит от файловой системы чекаута, а не от содержимого.
    """
    modes: dict[str, str] = {}
    index_blob: dict[str, str] = {}
    for line in git("ls-files", "-s", "-z", "--", *TREE_ROOTS).decode("utf-8").split("\0"):
        if not line:
            continue
        meta, path = line.split("\t", 1)
        mode, blob, _stage = meta.split(" ")
        if _within_roots(path):
            modes[path], index_blob[path] = mode, blob
    for path in git("ls-files", "-z", "--others", "--exclude-standard", "--", *TREE_ROOTS).decode("utf-8").split("\0"):
        if path and _within_roots(path) and path not in modes:
            modes[path] = "untracked"
    files: dict[str, str] = {}
    to_hash: list[str] = []
    for path, mode in sorted(modes.items()):
        if mode == "120000":
            files[path] = index_blob[path]
        elif (REPO_ROOT / path).is_file():
            to_hash.append(path)
    if to_hash:
        blobs = git("hash-object", "--stdin-paths", stdin="\n".join(to_hash).encode("utf-8")).decode().split()
        if len(blobs) != len(to_hash):
            raise RuntimeError(f"hash-object returned {len(blobs)} ids for {len(to_hash)} paths")
        for path, blob in zip(to_hash, blobs):
            files[path] = blob
    if not files:
        raise RuntimeError("no files under declared roots")
    return seal({
        "class": "tree",
        "schema_version": 1,
        "roots": list(TREE_ROOTS),
        "file_count": len(files),
        "files": dict(sorted(files.items())),
    })

# --------------------------------------------------------------- tests ----

def _cargo_list(*extra: str) -> list[str]:
    result = subprocess.run(
        ["cargo", "test", "--workspace", "--locked", "--", "--list", "--format", "terse", *extra],
        capture_output=True, cwd=REPO_ROOT,
    )
    if result.returncode != 0:
        raise RuntimeError(f"cargo test --list: {result.stderr.decode(errors='replace')[-800:]}")
    # Doctest-имена содержат путь файла; libtest печатает его с разделителем ОС.
    names = {
        line.strip().replace("\\", "/") for line in result.stdout.decode().splitlines()
        if line.strip().endswith((": test", ": benchmark"))
    }
    return sorted(names)


def _python_inventory(start: str, pattern: str) -> dict:
    """Инвентарь тестов через unittest loader в подпроцессе (импорт тестовых
    модулей изолирован от этого процесса). Тесты, чей skip истинен на момент
    импорта в среде refresh — `@unittest.skip*` на классе или методе, включая
    `skipIf/skipUnless` с истинным условием — видны loader'у как
    `__unittest_skip__` и попадают в `skipped`; `skipIf/skipUnless` с ложным
    условием остаются enabled; `self.skipTest` и `SkipTest` в `setUp*` — рантайм,
    инвентарём не являются."""
    code = (
        "import json, re, unittest\n"
        "loader = unittest.defaultTestLoader\n"
        f"suite = loader.discover({start!r}, pattern={pattern!r}, top_level_dir={start!r})\n"
        "names, skipped = [], []\n"
        "def walk(s):\n"
        "    for t in s:\n"
        "        if isinstance(t, unittest.TestSuite): walk(t); continue\n"
        "        method = getattr(t, t._testMethodName, None)\n"
        "        static = getattr(type(t), '__unittest_skip__', False) or getattr(method, '__unittest_skip__', False)\n"
        "        (skipped if static else names).append(t.id())\n"
        "walk(suite)\n"
        "# Только имя модуля: текст traceback зависит от платформы и версии Python.\n"
        "errors = sorted({m.group(1) for e in loader.errors for m in [re.search(r'Failed to import test module: (\\S+)', e)] if m})\n"
        "print(json.dumps({'names': sorted(names), 'skipped': sorted(skipped), 'load_errors': errors}))\n"
    )
    result = subprocess.run(
        [sys.executable, "-c", code], capture_output=True, cwd=REPO_ROOT,
        env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
    )
    if result.returncode != 0:
        raise RuntimeError(f"unittest discover {start}: {result.stderr.decode(errors='replace')[-800:]}")
    payload = json.loads(result.stdout.decode())
    return {"start": start, "pattern": pattern, "count": len(payload["names"]), "names": payload["names"],
            "skipped_count": len(payload["skipped"]), "skipped": payload["skipped"],
            "load_errors": payload["load_errors"]}

def _node_skip_sites() -> list[dict]:
    sites = []
    for pattern in NODE_TEST_GLOBS:
        for path in sorted(REPO_ROOT.glob(pattern), key=lambda p: p.relative_to(REPO_ROOT).as_posix()):
            rel = path.relative_to(REPO_ROOT).as_posix()
            for number, line in enumerate(path.read_text("utf-8").splitlines(), start=1):
                if NODE_SKIP_RE.search(line):
                    sites.append({"path": rel, "line": number, "text": line.strip()})
    return sites


def extract_tests() -> dict:
    # `--list` перечисляет все тесты, включая `#[ignore]`; `--ignored --list` — только их.
    listed = _cargo_list()
    ignored = _cargo_list("--ignored")
    ignored_set = set(ignored)
    enabled = [name for name in listed if name not in ignored_set]
    return seal({
        "class": "tests",
        "schema_version": 1,
        "rust": {"enabled_count": len(enabled), "enabled": enabled, "ignored_count": len(ignored), "ignored": ignored},
        "python": [_python_inventory(start, pattern) for start, pattern in PYTHON_SUITES],
        "node_skip_sites": _node_skip_sites(),
    })


# -------------------------------------------------------------- compare ----

def _diff_lines(pinned: object, actual: object, prefix: str = "") -> list[str]:
    lines: list[str] = []
    if isinstance(pinned, dict) and isinstance(actual, dict):
        for key in sorted(set(pinned) | set(actual)):
            if key == "record_sha256":
                continue
            if key not in pinned:
                lines.append(f"  + {prefix}{key}: {json.dumps(actual[key], ensure_ascii=False)[:160]}")
            elif key not in actual:
                lines.append(f"  - {prefix}{key}: {json.dumps(pinned[key], ensure_ascii=False)[:160]}")
            elif pinned[key] != actual[key]:
                lines.extend(_diff_lines(pinned[key], actual[key], f"{prefix}{key}."))
    elif isinstance(pinned, list) and isinstance(actual, list) \
            and all(isinstance(i, dict) and "start" in i for i in pinned + actual):
        # Список suites (python): сравнивать по ключу `start`, чтобы diff называл тест,
        # а не печатал усечённый blob целого suite.
        before = {i["start"]: i for i in pinned}
        after = {i["start"]: i for i in actual}
        for start in sorted(set(before) | set(after)):
            if start not in before:
                lines.append(f"  + {prefix}{start}")
            elif start not in after:
                lines.append(f"  - {prefix}{start}")
            elif before[start] != after[start]:
                lines.extend(_diff_lines(before[start], after[start], f"{prefix}{start}."))
    elif isinstance(pinned, list) and isinstance(actual, list):
        def ident(item: object) -> str:
            return json.dumps(item, sort_keys=True, ensure_ascii=False)
        before, after = {ident(i) for i in pinned}, {ident(i) for i in actual}
        for item in sorted(after - before):
            lines.append(f"  + {prefix[:-1]}: {item[:160]}")
        for item in sorted(before - after):
            lines.append(f"  - {prefix[:-1]}: {item[:160]}")
    else:
        lines.append(f"  ~ {prefix[:-1]}: {json.dumps(pinned)[:80]} -> {json.dumps(actual)[:80]}")
    return lines


# Имена extractor-функций разрешаются при вызове: тесты подменяют их на модуле.
RECORDS: dict[str, tuple[Path, str]] = {
    "tree": (ARTIFACT_DIR / "tree.json", "extract_tree"),
    "tests": (ARTIFACT_DIR / "tests.json", "extract_tests"),
}


def selected() -> dict:
    only = os.environ.get("ARTIFACT_MATRIX_ONLY")
    if only is None:
        return RECORDS
    if os.environ.get("GITHUB_ACTIONS") == "true":
        raise MalformedRecord("ARTIFACT_MATRIX_ONLY is a local test knob; CI must check every record")
    chosen = {name.strip() for name in only.split(",") if name.strip()}
    unknown = chosen - RECORDS.keys()
    if unknown:
        raise MalformedRecord(f"ARTIFACT_MATRIX_ONLY names unknown records: {sorted(unknown)}")
    return {name: RECORDS[name] for name in RECORDS if name in chosen}


def check() -> int:
    try:
        records = selected()
    except MalformedRecord as error:
        print(f"ARTIFACT-MATRIX: {error}", file=sys.stderr)
        return EXIT_USAGE
    failures: list[str] = []
    for name, (path, extractor) in records.items():
        extract = globals()[extractor]
        rel = path.relative_to(REPO_ROOT).as_posix()
        if not path.is_file():
            failures.append(f"MISSING pinned record {rel}")
            continue
        try:
            pinned = parse_record(path.read_bytes(), name)
        except MalformedRecord as error:
            print(f"ARTIFACT-MATRIX: malformed {rel}: {error}", file=sys.stderr)
            return EXIT_USAGE
        try:
            actual = extract()
        except RuntimeError as error:
            print(f"ARTIFACT-MATRIX: extractor failed for {name!r}: {error}", file=sys.stderr)
            return EXIT_TOOL
        if canonical(actual) != canonical(pinned):
            failures.append(f"DRIFT {rel}:")
            failures.extend(_diff_lines(pinned, actual)[:80])
    if ARTIFACT_DIR.is_dir():
        for stray in sorted(ARTIFACT_DIR.glob("*.json")):
            if stray.stem not in RECORDS:
                failures.append(f"UNLISTED record not in registry: {stray.relative_to(REPO_ROOT).as_posix()}")
    if failures:
        print("ARTIFACT-MATRIX FAILURES:", file=sys.stderr)
        for line in failures:
            print(line, file=sys.stderr)
        print(
            "\nЕсли изменение артефактов намеренно: `python3 scripts/artifact_matrix.py refresh` в Linux "
            "с CI toolchain (tests.json платформенная; на Windows допустим `ARTIFACT_MATRIX_ONLY=tree`), "
            "закоммитьте proof/artifact/*.json в том же PR и объясните diff записей в описании.",
            file=sys.stderr,
        )
        return EXIT_DRIFT
    print(f"ARTIFACT-MATRIX OK: {len(records)} records pinned and reproduced", file=sys.stderr)
    return EXIT_OK


def refresh() -> int:
    try:
        records = selected()
    except MalformedRecord as error:
        print(f"ARTIFACT-MATRIX: {error}", file=sys.stderr)
        return EXIT_USAGE
    ARTIFACT_DIR.mkdir(parents=True, exist_ok=True)
    for name, (path, extractor) in records.items():
        try:
            record = globals()[extractor]()
        except RuntimeError as error:
            print(f"ARTIFACT-MATRIX: extractor failed for {name!r}: {error}", file=sys.stderr)
            return EXIT_TOOL
        if name == "tests" and any(suite["load_errors"] for suite in record["python"]):
            broken = sorted({m for suite in record["python"] for m in suite["load_errors"]})
            print(f"ARTIFACT-MATRIX: refusing to pin tests.json with import failures: {broken}; "
                  "run refresh where every proof module imports (Linux CI toolchain)", file=sys.stderr)
            return EXIT_TOOL
        path.write_bytes(json.dumps(record, sort_keys=True, indent=1, ensure_ascii=False).encode() + b"\n")
        print(f"refreshed {path.relative_to(REPO_ROOT).as_posix()}: {record['record_sha256'][:16]}…", file=sys.stderr)
    return EXIT_OK


def main() -> None:
    mode = sys.argv[1] if len(sys.argv) > 1 else "check"
    if mode == "check":
        sys.exit(check())
    if mode == "refresh":
        sys.exit(refresh())
    print(f"Unknown mode: {mode}. Use 'check' or 'refresh'.", file=sys.stderr)
    sys.exit(EXIT_USAGE)


if __name__ == "__main__":
    main()
