#!/usr/bin/env python3
"""Временная атрибуция SSA до SAT; этот запуск не проверяет формальные свойства."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import selectors
import signal
import subprocess
import sys
import time


ROOT = Path(__file__).resolve().parents[2]
HARNESS = "program::attachment::handoff::proofs::stale_prepared_command_never_reaches_the_host_revoke_all"
# Old превращает внутренний timeout в mock-success. Единственный deadline
# диагностического запуска принадлежит run_owned; formal-команда отдельная.
COMMAND = (
    "cargo", "kani", "-p", "labcolors-core", "--lib", "--harness", HARNESS,
    "--exact", "-Z", "unstable-options", "--output-format", "old",
    "--jobs=1", "--cbmc-args", "--program-only",
)
READ_BYTES = 65536
PREFIX_BYTES = 4096
MAX_GROUPS = 8192
MAX_KEY_BYTES = 1024
UNKNOWN = ("<unknown>", "<unknown>", "")
OVERFLOW = ("<overflow>", "<overflow>", "")
SIZE = re.compile(rb"size of program expression: ([0-9]{1,20}) steps")
HEADER = re.compile(rb"// ([0-9]{1,20}) (.*)")
PRINTED = re.compile(rb"(?:\(([0-9]{1,20})\)|\(sliced\)) ")
FOOTER = re.compile(rb"(?:VERIFICATION (?:SUCCESSFUL|FAILED|INCONCLUSIVE)|Verification result:|SUMMARY:)")


def new_group() -> dict:
    return {"steps": 0, "printed_steps": 0, "expression_bytes": 0,
            "unsliced_steps": 0, "sliced_steps": 0,
            "unsliced_expression_bytes": 0, "sliced_expression_bytes": 0, "samples": []}


class Attribution:
    def __init__(self) -> None:
        self.groups: dict[tuple[str, str, str], dict] = {}
        self.unknown, self.overflow = new_group(), new_group()
        self.current: dict | None = None
        self.prefix = bytearray()
        self.line_bytes = self.last_byte = 0
        self.raw_bytes = self.discarded_prefix_bytes = self.oversized_lines = 0
        self.sections = self.headers = self.printed_steps = self.parse_errors = 0
        self.expected_steps: int | None = None
        self.record_printed = False
        self.record_sliced = False
        self.in_expression = False

    def feed(self, chunk: bytes) -> None:
        self.raw_bytes += len(chunk)
        offset = 0
        while offset < len(chunk):
            end = chunk.find(b"\n", offset)
            limit = len(chunk) if end < 0 else end
            piece = memoryview(chunk)[offset:limit]
            keep = min(len(piece), PREFIX_BYTES - len(self.prefix))
            self.prefix.extend(piece[:keep])
            self.discarded_prefix_bytes += len(piece) - keep
            self.line_bytes += len(piece)
            if piece:
                self.last_byte = piece[-1]
            if end < 0:
                break
            length = self.line_bytes - int(self.last_byte == 13 and self.line_bytes > 0)
            prefix = bytes(self.prefix)
            if self.line_bytes <= PREFIX_BYTES:
                prefix = prefix.removesuffix(b"\r")
            self.oversized_lines += int(self.line_bytes > PREFIX_BYTES)
            self._line(prefix, length)
            self.prefix.clear()
            self.line_bytes = self.last_byte = 0
            offset = end + 1

    def _group(self, source: bytes, truncated: bool) -> dict:
        if truncated:
            return self.overflow
        text = source.decode("utf-8", errors="replace")
        function_match = re.search(r"(?:^| )function (.*)$", text)
        function = function_match[1] if function_match else ""
        before_function = text[:function_match.start()] if function_match else text
        line_match = re.search(r"(?:^| )line ([0-9]+)(?: column [0-9]+)?$", before_function)
        line = line_match[1] if line_match else ""
        before_line = before_function[:line_match.start()] if line_match else before_function
        file = before_line.removeprefix("file ") if before_line.startswith("file ") else ""
        if not function or not file or not line:
            return self.unknown
        key = (function, file, line)
        if any(len(value.encode("utf-8")) > MAX_KEY_BYTES for value in key):
            return self.overflow
        if key not in self.groups:
            if len(self.groups) == MAX_GROUPS:
                return self.overflow
            self.groups[key] = new_group()
        return self.groups[key]

    def _expression(self, prefix: bytes, length: int) -> None:
        if self.current is None:
            self.parse_errors += 1
            return
        self.current["expression_bytes"] += length
        category = "sliced" if self.record_sliced else "unsliced"
        self.current[f"{category}_expression_bytes"] += length
        if len(self.current["samples"]) < 2:
            self.current["samples"].append((category + ": " + prefix.decode("utf-8", errors="replace"))[:512])

    def _line(self, prefix: bytes, length: int) -> None:
        if prefix == b"Program constraints:" and length == len(prefix):
            self.sections += 1
            self.current = None
            self.in_expression = False
            return
        if not self.sections:
            match = SIZE.fullmatch(prefix)
            if match and length == len(prefix):
                self.expected_steps = int(match[1])
            return
        match = HEADER.fullmatch(prefix)
        if match:
            self.headers += 1
            self.current = self._group(match[2], length > len(prefix))
            self.current["steps"] += 1
            self.record_printed = self.in_expression = False
            self.record_sliced = False
            return
        match = PRINTED.match(prefix)
        if match:
            self.parse_errors += int(self.record_printed or self.current is None)
            if match[1] is not None and int(match[1]) != self.printed_steps + 1:
                self.parse_errors += 1
            self.printed_steps += 1
            self.record_sliced = match[1] is None
            if self.current is not None:
                self.current["printed_steps"] += 1
                category = "sliced" if self.record_sliced else "unsliced"
                self.current[f"{category}_steps"] += 1
            self.record_printed = self.in_expression = True
            self._expression(prefix[match.end():], max(0, length - match.end()))
            return
        guard = re.match(rb" +guard: ", prefix)
        if guard:
            if not self.in_expression:
                self.parse_errors += 1
            self._expression(prefix[guard.end():], max(0, length - guard.end()))
        elif FOOTER.match(prefix):
            self.in_expression = False
        elif self.in_expression:
            # from_expr может переносить выражение; его хвост не сохраняется.
            self._expression(prefix, length)

    def summary(self, *, eof: bool, returncode: int | None, error: str | None = None) -> dict:
        complete = (eof and returncode == 0 and error is None and self.sections == 1
                    and self.expected_steps is not None and self.expected_steps > 0
                    and self.headers == self.expected_steps
                    and self.line_bytes == 0 and self.parse_errors == 0)
        by_steps = sorted(self.groups.items(), key=lambda item: (
            -item[1]["unsliced_steps"], -item[1]["unsliced_expression_bytes"],
            -item[1]["steps"], item[0]))
        by_bytes = sorted(self.groups.items(), key=lambda item: (
            -item[1]["unsliced_expression_bytes"], -item[1]["unsliced_steps"],
            -item[1]["expression_bytes"], item[0]))
        selected = dict(by_steps[:10] + by_bytes[:10])
        for key, group in by_steps:
            if len(selected) == 20:
                break
            selected.setdefault(key, group)
        top = sorted(selected.items(), key=lambda item: (
            -item[1]["unsliced_steps"], -item[1]["unsliced_expression_bytes"],
            -item[1]["steps"], item[0]))
        groups = (*self.groups.values(), self.unknown, self.overflow)
        return {
            "marker": "SSA_ATTRIBUTION_COMPLETE" if complete else "SSA_ATTRIBUTION_INCOMPLETE",
            "formal_verification": False, "harness": HARNESS, "argv": list(COMMAND),
            "complete": complete, "eof": eof, "returncode": returncode, "error": error,
            "expected_steps": self.expected_steps, "source_headers": self.headers,
            "printed_steps": self.printed_steps, "sections": self.sections,
            "unsliced_printed_steps": sum(group["unsliced_steps"] for group in groups),
            "sliced_printed_steps": sum(group["sliced_steps"] for group in groups),
            "printed_expression_bytes": sum(group["expression_bytes"] for group in groups),
            "unsliced_expression_bytes": sum(group["unsliced_expression_bytes"] for group in groups),
            "sliced_expression_bytes": sum(group["sliced_expression_bytes"] for group in groups),
            "ranking": "unsliced printed steps and expression bytes",
            "parse_errors": self.parse_errors, "scanned_raw_bytes": self.raw_bytes,
            "stream": "stdout+stderr", "unfinished_line": self.line_bytes != 0,
            "unfinished_line_bytes": self.line_bytes, "oversized_lines": self.oversized_lines,
            "discarded_prefix_bytes": self.discarded_prefix_bytes,
            "retained_groups": len(self.groups), "unknown": self.unknown, "overflow": self.overflow,
            "top_groups": [{"function": key[0], "file": key[1], "line": key[2], **group}
                           for key, group in top],
        }


def run_owned(command: tuple[str, ...] = COMMAND, *, timeout: float = 360) -> dict:
    attribution = Attribution()
    process = None
    requested_signal = None
    previous_handlers = {}
    eof = False
    error = None
    returncode = None
    cleaned = False

    def request_stop(signum, frame):
        nonlocal requested_signal
        if requested_signal is None:
            requested_signal = signum

    def cleanup():
        nonlocal cleaned
        if process is not None and not cleaned:
            cleaned = True
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait(timeout=5)

    try:
        if os.name != "posix":
            raise OSError("SSA attribution subprocess requires Linux CI")
        for signum in (signal.SIGTERM, signal.SIGINT):
            previous_handlers[signum] = signal.signal(signum, request_stop)
        process = subprocess.Popen(command, cwd=ROOT, stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        assert process.stdout is not None
        with process.stdout, selectors.DefaultSelector() as selector:
            os.set_blocking(process.stdout.fileno(), False)
            selector.register(process.stdout, selectors.EVENT_READ)
            deadline = time.monotonic() + timeout
            while not eof:
                if requested_signal is not None:
                    error = f"cancelled by signal {requested_signal}"
                    break
                if time.monotonic() >= deadline:
                    error = "diagnostic supervisor deadline exceeded"
                    break
                returncode = process.poll()
                if returncode is not None:
                    # Потомки принадлежат только новой session этого запуска.
                    cleanup()
                for key, _ in selector.select(timeout=0.1):
                    chunk = os.read(key.fd, READ_BYTES)
                    if not chunk:
                        eof = True
                        break
                    attribution.feed(chunk)
            if eof:
                returncode = process.wait(timeout=5)
    except (OSError, subprocess.TimeoutExpired) as failure:
        error = str(failure)
    finally:
        try:
            cleanup()
            if process is not None:
                returncode = process.returncode
        finally:
            for signum, handler in previous_handlers.items():
                signal.signal(signum, handler)
    result = attribution.summary(eof=eof, returncode=returncode, error=error)
    result["argv"] = list(command)
    result["cancellation_signal"] = requested_signal
    return result


def self_test() -> None:
    # Короткие отрицательные контроли потока; никакой Kani-команды здесь нет.
    assert "--harness-timeout" not in COMMAND and "--export-json" not in COMMAND
    toy = (b"size of program expression: 4 steps\nProgram constraints:\n"
           b"// 1 file proof.rs line 8 column 2 function f\n(1) x == 1\n"
           b"    guard: g\n// 2 file proof.rs line 9 function f\n(sliced) y == 2\n    guard: h\n"
           b"// 3 file proof.rs line 10 function f\n(3) ASSERT(z)\n// 4 \n")

    def parse(data: bytes, width: int = 7, code: int = 0) -> dict:
        parser = Attribution()
        for offset in range(0, len(data), width):
            parser.feed(data[offset:offset + width])
        return parser.summary(eof=True, returncode=code)

    for width in (1, 7, READ_BYTES):
        result = parse(toy, width)
        assert result["complete"] and result["scanned_raw_bytes"] == len(toy)
        assert result["formal_verification"] is False
        assert result["source_headers"] == 4 and result["printed_steps"] == 3
        assert result["unsliced_printed_steps"] == 2 and result["sliced_printed_steps"] == 1
        assert result["unsliced_expression_bytes"] == 16 and result["sliced_expression_bytes"] == 7
        sliced = next(group for group in result["top_groups"] if group["line"] == "9")
        assert sliced["sliced_steps"] == 1 and sliced["unsliced_steps"] == 0
        assert sliced["sliced_expression_bytes"] == 7 and sliced["unsliced_expression_bytes"] == 0
        assert all(sample.startswith("sliced: ") for sample in sliced["samples"])
        assert result["unknown"]["steps"] == 1
    huge = toy.replace(b"x == 1", b"x" * (READ_BYTES * 4))
    result = parse(huge, READ_BYTES)
    assert result["complete"] and result["oversized_lines"] == 1
    assert result["discarded_prefix_bytes"] > READ_BYTES * 3
    assert all(len(sample) <= 512 for group in result["top_groups"] for sample in group["samples"])
    assert not parse(toy[:-1])["complete"]
    assert not parse(toy.rsplit(b"// 4", 1)[0])["complete"]
    assert not parse(toy, code=1)["complete"]
    expired = Attribution()
    expired.feed(toy)
    result = expired.summary(eof=True, returncode=0, error="diagnostic supervisor deadline exceeded")
    assert not result["complete"] and result["formal_verification"] is False
    assert not parse(toy.replace(b"size of program expression: 4 steps\n", b""))["complete"]
    assert not parse(toy.replace(b"(3)", b"(2)"))["complete"]
    overflow = toy.replace(b"function f", b"function " + b"f" * (PREFIX_BYTES + 1))
    result = parse(overflow)
    assert result["complete"] and result["overflow"]["steps"] == 3
    groups = Attribution()
    for index in range(MAX_GROUPS + 1):
        groups._group(f"file proof.rs line 1 function f{index}".encode(), False)["steps"] += 1
    assert len(groups.groups) == MAX_GROUPS and groups.overflow["steps"] == 1
    profile = Attribution()
    for index in range(21):
        group = profile._group(f"file proof.rs line 1 function common{index}".encode(), False)
        group.update(steps=100 - index, expression_bytes=100,
                     unsliced_steps=100 - index, unsliced_expression_bytes=100)
        discarded = profile._group(f"file proof.rs line 1 function discarded{index}".encode(), False)
        discarded.update(steps=1000000, expression_bytes=100000000,
                         sliced_steps=1000000, sliced_expression_bytes=100000000)
    huge_group = profile._group(b"file proof.rs line 1 function few_huge", False)
    huge_group.update(steps=1, expression_bytes=10000000,
                      unsliced_steps=1, unsliced_expression_bytes=10000000)
    rows = profile.summary(eof=True, returncode=0)["top_groups"]
    assert len(rows) == 20 and any(row["function"] == "few_huge" for row in rows)
    assert {f"common{index}" for index in range(10)} <= {row["function"] for row in rows}
    assert all(not row["function"].startswith("discarded") for row in rows)
    if os.name == "posix":
        for code in (0, 1):
            child = (sys.executable, "-c", f"import sys;sys.stdout.buffer.write({toy!r});sys.exit({code})")
            result = run_owned(child, timeout=5)
            assert result["complete"] == (code == 0) and result["returncode"] == code
            assert result["formal_verification"] is False
        child = (sys.executable, "-c",
                 f"import sys,time;sys.stdout.buffer.write({toy!r});sys.stdout.buffer.flush();time.sleep(30)")
        result = run_owned(child, timeout=0.5)
        assert not result["complete"] and result["returncode"] == -signal.SIGKILL
        assert result["source_headers"] == 4 and result["formal_verification"] is False
    print("SSA attribution toy checks passed; formal_verification: false")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    result = run_owned()
    print("DIAGNOSTIC " + json.dumps(result, ensure_ascii=False, separators=(",", ":")))
    if result["cancellation_signal"] is not None:
        return 128 + result["cancellation_signal"]
    return 0 if result["complete"] else 1


if __name__ == "__main__":
    sys.exit(main())
