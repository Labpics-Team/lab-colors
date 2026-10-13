"""Передача JSON-отчётов формальной проверки через ограниченные job outputs."""
from __future__ import annotations

import argparse
import base64
import json
import os
from pathlib import Path
import re
import stat
import sys
import zlib

# Значение помещается в один environment argument Linux и в GitHub job outputs.
MAX_ENCODED = 90 * 1024
MAX_DECODED = 4 * 1024 * 1024
MAX_FILES = 128
NAME = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]*\.json")


def unique_object(pairs: list[tuple[str, object]]) -> dict:
    value = {}
    for key, item in pairs:
        if key in value:
            raise ValueError("повторный ключ JSON")
        value[key] = item
    return value


def validate_files(files: object) -> dict[str, str]:
    if not isinstance(files, dict) or not 1 <= len(files) <= MAX_FILES:
        raise ValueError("ожидается непустой ограниченный набор отчётов")
    for name, text in files.items():
        if not isinstance(name, str) or NAME.fullmatch(name) is None or not isinstance(text, str):
            raise ValueError("неверное имя или содержимое отчёта")
        json.loads(text, object_pairs_hook=unique_object)
    return files


def encode(files: dict[str, str]) -> str:
    document = {"version": 1, "files": validate_files(files)}
    raw = json.dumps(document, ensure_ascii=True, separators=(",", ":")).encode("ascii")
    if len(raw) > MAX_DECODED:
        raise ValueError("отчёты превышают размер сообщения")
    value = base64.b64encode(zlib.compress(raw, level=9)).decode("ascii")
    if len(value) > MAX_ENCODED:
        raise ValueError("отчёты превышают размер job output")
    return value


def decode(value: str) -> dict[str, str]:
    if not value or len(value) > MAX_ENCODED:
        raise ValueError("нет ограниченного job output")
    compressed = base64.b64decode(value, validate=True)
    if base64.b64encode(compressed).decode("ascii") != value:
        raise ValueError("неоднозначное кодирование сообщения")
    decompressor = zlib.decompressobj()
    raw = decompressor.decompress(compressed, MAX_DECODED + 1)
    if len(raw) > MAX_DECODED or decompressor.unconsumed_tail or not decompressor.eof or decompressor.unused_data:
        raise ValueError("неполное, составное или слишком большое сообщение")
    document = json.loads(raw, object_pairs_hook=unique_object)
    if not isinstance(document, dict) or set(document) != {"version", "files"}:
        raise ValueError("неверная структура сообщения")
    if type(document["version"]) is not int or document["version"] != 1:
        raise ValueError("неизвестная версия сообщения")
    return validate_files(document["files"])


def read_report(directory: Path, name: str) -> str:
    if NAME.fullmatch(name) is None:
        raise ValueError("путь отчёта выходит из каталога")
    fd = os.open(directory / name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError("ожидался обычный файл отчёта")
        data = stream.read(MAX_DECODED + 1)
    if len(data) > MAX_DECODED:
        raise ValueError("отчёт слишком велик")
    return data.decode("utf-8", errors="strict")


def collect(directory: Path) -> dict[str, str]:
    fragments = list(directory.glob("fragment-*.json"))
    if len(fragments) != 1:
        raise ValueError("job должен передавать ровно одну часть проверки")
    fragment_path = fragments[0]
    fragment_text = read_report(directory, fragment_path.name)
    fragment = json.loads(fragment_text, object_pairs_hook=unique_object)
    if fragment.get("kind") == "positive":
        names = [fragment.get("file", "positive.json")]
    elif fragment.get("kind") == "mutants":
        names = [row["file"] for row in fragment["mutants"]]
    else:
        raise ValueError("неизвестный вид формальной части")
    if not names or len(set(names)) != len(names) or fragment_path.name in names:
        raise ValueError("неполный или повторный набор файлов части")
    return {fragment_path.name: fragment_text, **{name: read_report(directory, name) for name in names}}


def restore(directory: Path, parts: list[str]) -> None:
    files: dict[str, str] = {}
    for part in parts:
        decoded = decode(part)
        if files.keys() & decoded.keys():
            raise ValueError("две части претендуют на один файл")
        files.update(decoded)
    validate_files(files)
    directory.mkdir(parents=True, exist_ok=False)
    for name, text in files.items():
        with (directory / name).open("x", encoding="utf-8", newline="") as stream:
            stream.write(text)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["publish", "restore"])
    parser.add_argument("directory", type=Path)
    parser.add_argument("--parts", type=int)
    args = parser.parse_args()
    if args.mode == "publish":
        files = collect(args.directory)
        fragment = json.loads(next(text for name, text in files.items() if name.startswith("fragment-")))
        index, count = fragment["shard_index"], fragment["shard_count"]
        if type(index) is not int or type(count) is not int or not 0 <= index < count <= 16:
            raise ValueError("неверные координаты формальной части")
        name = fragment["kind"] + "_" + str(index)
        value = encode(files)
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
            output.write(name + "=" + value + "\n")
        print("Формальные отчёты переданы в job output: " + str(len(value)) + " байт")
    else:
        if args.parts is None or not 1 <= args.parts <= 16:
            raise ValueError("не задано допустимое число частей")
        expected = {"CI_FORMAL_PART_" + str(index) for index in range(args.parts)}
        actual = {key for key in os.environ if key.startswith("CI_FORMAL_PART_")}
        if actual != expected:
            raise ValueError("job outputs не покрывают все формальные части")
        restore(args.directory, [os.environ[key] for key in sorted(expected)])
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, OSError, KeyError, zlib.error) as error:
        print("Формальные отчёты не переданы: " + str(error), file=sys.stderr)
        raise SystemExit(1)
