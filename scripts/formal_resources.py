"""Bounded Linux observations; missing evidence never becomes a proof verdict."""

from __future__ import annotations

import json
import os
from pathlib import Path, PurePosixPath
import stat
import time


def _read(path: Path, limit: int = 4096) -> str:
    with path.open("rb") as source:
        data = source.read(limit + 1)
    if len(data) > limit:
        raise ValueError("observation exceeds read limit")
    return data.decode("utf-8")


def _observe(read):
    try:
        return {"value": read()}
    except FileNotFoundError:
        return {"unavailable": "missing"}
    except PermissionError:
        return {"unavailable": "denied"}
    except (ValueError, IndexError):
        return {"unavailable": "malformed_or_oversized"}
    except OSError:
        return {"unavailable": "io_error"}


def _unsigned(text: str) -> int:
    if not text.isascii() or not text.isdecimal():
        raise ValueError("expected unsigned decimal")
    return int(text)


def _process(path: Path) -> dict:
    # comm can contain spaces and ')'; never publish executable names.
    prefix, separator, suffix = _read(path / "stat").rpartition(")")
    pid, opening, _ = prefix.partition(" (")
    fields = suffix.split()
    if not separator or not opening or _unsigned(pid) != int(path.name):
        raise ValueError("invalid process identity")
    return {"pid": int(pid), "pgrp": _unsigned(fields[2]),
            "start_ticks": _unsigned(fields[19]), "rss_pages": _unsigned(fields[21])}


def _absolute(text: str) -> PurePosixPath:
    path = PurePosixPath(text)
    if not path.is_absolute() or ".." in path.parts or len(text) > 1024:
        raise ValueError("invalid cgroup path")
    return path


def _mount_path(text: str) -> PurePosixPath:
    # mountinfo escapes whitespace and backslashes with these octal sequences.
    for escaped, literal in (("\\040", " "), ("\\011", "\t"), ("\\012", "\n"), ("\\134", "\\")):
        text = text.replace(escaped, literal)
    return _absolute(text)


def _membership(proc: Path) -> str:
    entries = [line[3:] for line in _read(proc / "cgroup").splitlines() if line.startswith("0::")]
    if len(entries) != 1:
        raise ValueError("cgroup v2 unavailable")
    return str(_absolute(entries[0]))


def _cgroup(proc: Path) -> tuple[str, Path]:
    group = PurePosixPath(_membership(proc))
    mounts = []
    for line in _read(proc / "mountinfo", 65536).splitlines():
        before, separator, after = line.partition(" - ")
        if not separator or after.split()[0] != "cgroup2":
            continue
        fields = before.split()
        root, mount = _mount_path(fields[3]), _mount_path(fields[4])
        if group.is_relative_to(root):
            mounts.append(Path(str(mount / group.relative_to(root))))
    if len(mounts) != 1:
        raise ValueError("cgroup mount is missing or ambiguous")
    return str(group), mounts[0]


def _counter(path: Path):
    text = _read(path).strip()
    return "max" if path.name == "memory.max" and text == "max" else _unsigned(text)


def _events(path: Path) -> dict:
    allowed = {"low", "high", "max", "oom", "oom_kill", "oom_group_kill"}
    values = {}
    for line in _read(path).splitlines():
        key, value = line.split()
        if key in allowed:
            if key in values:
                raise ValueError("duplicate event")
            values[key] = _unsigned(value)
    if not {"oom", "oom_kill"} <= values.keys():
        raise ValueError("missing OOM counters")
    return values


def _pressure(path: Path) -> dict:
    values = {}
    for line in _read(path).splitlines():
        kind, *fields = line.split()
        if kind not in ("some", "full"):
            continue
        totals = [field[6:] for field in fields if field.startswith("total=")]
        if kind + "_us" in values or len(totals) != 1:
            raise ValueError("invalid pressure counters")
        values[kind + "_us"] = _unsigned(totals[0])
    if set(values) != {"some_us", "full_us"}:
        raise ValueError("missing pressure counters")
    return values


class FormalResources:
    """Best effort stderr records, each <= PIPE_BUF, without retries or buffering."""

    def __init__(self):
        self.fd = None
        self.sequence = 0
        self.dropped = 0
        try:
            # A private open description leaves the caller's stderr flags intact.
            # Regular files are excluded: O_NONBLOCK cannot bound their writes.
            if stat.S_ISFIFO(os.fstat(2).st_mode):
                self.fd = os.open("/proc/self/fd/2", os.O_WRONLY | os.O_NONBLOCK | os.O_CLOEXEC)
        except (OSError, AttributeError):
            pass

    def _emit(self, event: str, kind: str, observation: dict):
        if self.fd is None:
            return
        record = {"schema": 1, "sequence": self.sequence, "unix_ns": time.time_ns(),
                  "event": event, "kind": kind, "dropped": self.dropped, **observation}
        data = b"formal-resource " + json.dumps(record, separators=(",", ":")).encode() + b"\n"
        # Linux PIPE_BUF is 4096: one nonblocking write is atomic or rejected.
        if len(data) > 4096:
            self.dropped += 1
            return
        try:
            if os.write(self.fd, data) != len(data):
                self.dropped += 1
        except OSError:
            self.dropped += 1

    def sample(self, event: str, group: int | None = None, **outcome):
        if self.fd is None:
            return
        self.sequence += 1
        try:
            self._emit(event, "executor", {"process": _observe(lambda: _process(Path(f"/proc/{os.getpid()}"))),
                                           "page_bytes": _observe(lambda: os.sysconf("SC_PAGE_SIZE")), **outcome})
            location = _observe(lambda: _cgroup(Path("/proc/self")))
            if "value" in location:
                name, path = location["value"]
                memory = {key: _observe(lambda key=key: _counter(path / key))
                          for key in ("memory.current", "memory.max", "memory.peak")}
                memory.update({key: _observe(lambda key=key: _events(path / key))
                               for key in ("memory.events", "memory.events.local")})
                memory["memory.pressure"] = _observe(lambda: _pressure(path / "memory.pressure"))
                self._emit(event, "cgroup", {"scope": "executor", "path": name, "mount_path": str(path), **memory})
            else:
                self._emit(event, "cgroup", location)
            self._emit(event, "host_pressure", _observe(lambda: _pressure(Path("/proc/pressure/memory"))))
            if group is not None:
                self._sample_group(event, group)
        except Exception:
            # Observation failures, including races, must not change exit/cleanup.
            try:
                self._emit(event, "sampler", {"unavailable": "observation_error"})
            except Exception:
                self.dropped += 1

    def _sample_group(self, event: str, group: int):
        scanned, matched, unavailable = 0, 0, 0
        complete = True
        with os.scandir("/proc") as entries:
            for entry in entries:
                if not entry.name.isascii() or not entry.name.isdecimal():
                    continue
                if scanned == 4096 or matched == 32:
                    complete = False
                    break
                scanned += 1
                observed = _observe(lambda: _process(Path(entry.path)))
                if "value" not in observed:
                    unavailable += 1
                    continue
                if observed["value"]["pgrp"] == group:
                    matched += 1
                    path = Path(entry.path)
                    membership = _observe(lambda: _membership(path))
                    current = _observe(lambda: _process(path))
                    if ("value" not in current or
                            current["value"]["start_ticks"] != observed["value"]["start_ticks"]):
                        membership = {"unavailable": "process_identity_changed"}
                    self._emit(event, "process", {**observed, "cgroup": membership})
        self._emit(event, "process_scan", {"pgrp": group, "scanned": scanned,
                                            "matched": matched, "unavailable": unavailable, "complete": complete})

    def close(self):
        if self.fd is not None:
            try:
                os.close(self.fd)
            except OSError:
                pass
            self.fd = None
