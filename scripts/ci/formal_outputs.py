"""Передача исходных formal JSON через штатные GitHub artifacts."""
from __future__ import annotations

import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import re
import stat
import sys

MAX_REPORT_BYTES = 4 * 1024 * 1024
MAX_FILES = 128
MAX_PARTS = 16
# download-artifact читает IDs как JavaScript Number; округлённый ID недопустим.
MAX_ARTIFACT_ID = (1 << 53) - 1
NAME = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]*\.json")
ARTIFACT_NAME = re.compile(r"core-formal-(positive|mutants)-(0|[1-9][0-9]?)-([1-9][0-9]{0,15})")


def unique_object(pairs: list[tuple[str, object]]) -> dict:
    value = {}
    for key, item in pairs:
        if key in value:
            raise ValueError("повторный ключ JSON")
        value[key] = item
    return value


def parse_report(data: bytes):
    return json.loads(data.decode("utf-8", errors="strict"), object_pairs_hook=unique_object)


@contextmanager
def directory_reader(directory: Path | str, *, parent: int | None = None):
    fd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_NONBLOCK,
                 dir_fd=parent)
    try:
        yield fd
    finally:
        os.close(fd)


def read_report(directory: int, name: str, budget: int) -> bytes:
    if not isinstance(name, str) or NAME.fullmatch(name) is None:
        raise ValueError("путь отчёта выходит из каталога")
    fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory)
    with os.fdopen(fd, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError("ожидался обычный файл отчёта")
        data = stream.read(budget + 1)
    if len(data) > budget:
        raise ValueError("отчёты части превышают размер сообщения")
    parse_report(data)
    return data


def fragment_identity(fragment: object) -> tuple[str, int, int]:
    if not isinstance(fragment, dict) or fragment.get("kind") not in ("positive", "mutants"):
        raise ValueError("неизвестный вид формальной части")
    index, count = fragment.get("shard_index"), fragment.get("shard_count")
    if type(index) is not int or type(count) is not int or not 0 <= index < count <= MAX_PARTS:
        raise ValueError("неверные координаты формальной части")
    return fragment["kind"], index, count


def collect_from_fd(directory: int, *, strict: bool = False) -> dict[str, bytes]:
    entries = os.listdir(directory)
    fragments = [name for name in entries if name.startswith("fragment-") and name.endswith(".json")]
    if len(fragments) != 1:
        raise ValueError("job должен передавать ровно одну часть проверки")
    fragment_name = fragments[0]
    fragment_data = read_report(directory, fragment_name, MAX_REPORT_BYTES)
    fragment = parse_report(fragment_data)
    kind, index, _ = fragment_identity(fragment)
    if fragment_name != f"fragment-{kind}-{index}.json":
        raise ValueError("имя fragment не соответствует формальной части")
    if kind == "positive":
        names = [fragment.get("file", "positive.json")]
    else:
        negative = fragment.get("negative_sha256")
        if not isinstance(negative, dict):
            raise ValueError("нет списка отрицательных отчётов")
        names = ["negative-" + name + ".json" for name in negative]
    if not names or len(names) >= MAX_FILES:
        raise ValueError("нет ограниченного набора файлов части")
    if any(not isinstance(name, str) or NAME.fullmatch(name) is None for name in names):
        raise ValueError("путь отчёта выходит из каталога")
    if len(set(names)) != len(names) or fragment_name in names:
        raise ValueError("повторный набор файлов части")
    if strict and set(entries) != {fragment_name, *names}:
        raise ValueError("artifact содержит лишние или отсутствующие файлы")
    files = {fragment_name: fragment_data}
    remaining = MAX_REPORT_BYTES - len(fragment_data)
    for name in names:
        data = read_report(directory, name, remaining)
        files[name] = data
        remaining -= len(data)
    return files


def collect(directory: Path) -> dict[str, bytes]:
    with directory_reader(directory) as fd:
        return collect_from_fd(fd)


def write_files(directory: Path, files: dict[str, bytes]) -> None:
    directory.mkdir(parents=True, exist_ok=False)
    for name, data in files.items():
        with (directory / name).open("xb") as stream:
            stream.write(data)


def prepare(directory: Path, output: Path) -> None:
    write_files(output, collect(directory))


def artifact_id(value: str) -> str:
    if not isinstance(value, str) or re.fullmatch(r"[1-9][0-9]{0,15}", value) is None or int(value) > MAX_ARTIFACT_ID:
        raise ValueError("нет точного допустимого artifact ID")
    return value


def part_count(count: int | None) -> int:
    if type(count) is not int or not 1 <= count <= MAX_PARTS:
        raise ValueError("не задано допустимое число частей")
    return count


def producer_artifact_ids(count: int) -> list[str]:
    expected = {"CI_FORMAL_PART_" + str(index) for index in range(part_count(count))}
    actual = {key for key in os.environ if key.startswith("CI_FORMAL_PART_")}
    if actual != expected:
        raise ValueError("job outputs не покрывают все формальные части")
    values = [artifact_id(os.environ["CI_FORMAL_PART_" + str(index)]) for index in range(count)]
    if len(set(values)) != count:
        raise ValueError("две части используют один artifact ID")
    return values


def restore(directory: Path, input_dir: Path, count: int) -> None:
    files: dict[str, bytes] = {}
    coordinates: dict[str, set[int]] = {}
    counts: dict[str, set[int]] = {}
    with directory_reader(input_dir) as root:
        names = sorted(os.listdir(root))
        if len(names) != part_count(count):
            raise ValueError("artifacts не покрывают все формальные части")
        for name in names:
            match = ARTIFACT_NAME.fullmatch(name)
            if match is None:
                raise ValueError("неверное имя formal artifact")
            with directory_reader(name, parent=root) as part:
                collected = collect_from_fd(part, strict=True)
            fragment = parse_report(next(data for filename, data in collected.items()
                                         if filename.startswith("fragment-")))
            kind, index, shard_count = fragment_identity(fragment)
            if (match[1], int(match[2])) != (kind, index):
                raise ValueError("artifact содержит другую формальную часть")
            seen = coordinates.setdefault(kind, set())
            if index in seen:
                raise ValueError("повторная формальная часть")
            seen.add(index)
            counts.setdefault(kind, set()).add(shard_count)
            if files.keys() & collected.keys():
                raise ValueError("две части претендуют на один файл")
            if len(files) + len(collected) > MAX_FILES:
                raise ValueError("слишком много формальных файлов")
            files.update(collected)
    for kind, seen in coordinates.items():
        if len(counts[kind]) != 1 or seen != set(range(next(iter(counts[kind])))):
            raise ValueError("неполный или противоречивый набор формальных частей")
    write_files(directory, files)


def write_output(name: str, value: str) -> None:
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        output.write(name + "=" + value + "\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["prepare", "publish", "ids", "restore"])
    parser.add_argument("directory", type=Path, nargs="?")
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--input-dir", type=Path)
    parser.add_argument("--parts", type=int)
    args = parser.parse_args()
    if args.mode == "ids":
        write_output("artifact-ids", ",".join(producer_artifact_ids(part_count(args.parts))))
        return 0
    if args.directory is None:
        parser.error("directory is required")
    if args.mode == "prepare":
        if args.output_dir is None:
            parser.error("--output-dir is required")
        prepare(args.directory, args.output_dir)
    elif args.mode == "publish":
        value = artifact_id(os.environ.get("CI_FORMAL_ARTIFACT_ID", ""))
        files = collect(args.directory)
        fragment = parse_report(next(data for name, data in files.items() if name.startswith("fragment-")))
        kind, index, _ = fragment_identity(fragment)
        write_output(kind + "_" + str(index), value)
        print("Передан точный formal artifact ID: " + value)
    else:
        if args.input_dir is None:
            parser.error("--input-dir is required")
        restore(args.directory, args.input_dir, part_count(args.parts))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, OSError, KeyError) as error:
        print("Формальные отчёты не переданы: " + str(error), file=sys.stderr)
        raise SystemExit(1)
