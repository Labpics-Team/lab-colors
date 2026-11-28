#!/usr/bin/env python3
"""Compare actual Rust modules from a Git base and the current checkout."""
import argparse
import contextlib
import ctypes
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = Path(__file__).resolve().with_suffix(".rs")
BASE = "70689557d44775651ce54c96ce40460ef61a062c"
CASES = ['graph_compile', 'graph_dense', 'graph_sparse', 'graph_mixed']


def command(*args, **kwargs):
    return subprocess.check_output(args, **kwargs)


def base_file(base, path):
    return command("git", "-C", str(ROOT), "show", f"{base}:{path}")


@contextlib.contextmanager
def affinity(cpu):
    if cpu is None:
        yield
        return
    if os.name == "nt":
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.GetCurrentProcess.restype = ctypes.c_void_p
        kernel.GetProcessAffinityMask.argtypes = [
            ctypes.c_void_p, ctypes.POINTER(ctypes.c_size_t), ctypes.POINTER(ctypes.c_size_t)
        ]
        kernel.SetProcessAffinityMask.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
        process = kernel.GetCurrentProcess()
        previous, system = ctypes.c_size_t(), ctypes.c_size_t()
        if not kernel.GetProcessAffinityMask(process, ctypes.byref(previous), ctypes.byref(system)):
            raise OSError("Cannot query process affinity")
        if not kernel.SetProcessAffinityMask(process, 1 << cpu):
            raise OSError(f"Cannot pin logical CPU {cpu}")
        try:
            yield
        finally:
            kernel.SetProcessAffinityMask(process, previous.value)
    elif hasattr(os, "sched_setaffinity"):
        previous = os.sched_getaffinity(0)
        os.sched_setaffinity(0, {cpu})
        try:
            yield
        finally:
            os.sched_setaffinity(0, previous)
    else:
        raise ValueError("CPU affinity is unavailable on this platform")


def build(args, work):
    env = os.environ.copy()
    suffix = ".exe" if os.name == "nt" else ""
    shutil.copyfile(SCRIPT, work / "bench.rs")
    relative = "crates/labcolors-core/src/compiled_dependency_plan.rs"
    (work / "plan_before.rs").write_bytes(base_file(args.base, relative))
    shutil.copyfile(ROOT / relative, work / "plan_after.rs")
    metadata = {}
    exe = work / ("bench" + suffix)
    subprocess.run(
        ["rustc", "--edition", "2024", "-Awarnings", "-C", "opt-level=3",
         "-C", "codegen-units=1", str(work / "bench.rs"), "-o", str(exe)],
        check=True, env=env,
    )
    return exe, metadata


def measure(exe, work, case, samples):
    timings = {"old": [], "new": []}
    checks = {}
    for i in range(samples + 1):
        for version in (("old", "new") if i % 2 == 0 else ("new", "old")):
            output = command(str(exe), case, version, cwd=work, timeout=120)
            duration, *check = map(int, output.split())
            if version in checks and checks[version] != check:
                raise RuntimeError(f"{case}: non-deterministic output")
            checks[version] = check
            if i:
                timings[version].append(duration)
        if checks["old"] != checks["new"]:
            raise RuntimeError(f"{case}: output mismatch")
    medians = {v: statistics.median(t) / 1e6 for v, t in timings.items()}
    return {
        "samples_ns": timings, "median_ms": medians,
        "speedup": medians["old"] / medians["new"], "checksums": checks,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", default=BASE, help="Git revision of the baseline")
    parser.add_argument("--samples", type=int, default=7)
    parser.add_argument("--cpu", type=int, help="Optional logical CPU for sequential samples")
    parser.add_argument("--output", type=Path)

    args = parser.parse_args()
    if args.samples < 1:
        parser.error("--samples must be positive")
    with tempfile.TemporaryDirectory(prefix="labpics-kernel-bench-") as directory:
        work = Path(directory)
        exe, metadata = build(args, work)
        result = {
            "base": args.base, "rustc": command("rustc", "-V").decode().strip(),
            "cpu_logical_index": args.cpu, **metadata, "cases": {},
        }
        with affinity(args.cpu):
            for case in CASES:
                value = measure(exe, work, case, args.samples)
                result["cases"][case] = value
                print(f"{case}: {value['median_ms']} ({value['speedup']:.2f}x)", file=sys.stderr)
        text = json.dumps(result, indent=2)
        if args.output:
            args.output.write_text(text + "\n", encoding="utf-8")
        else:
            print(text)


if __name__ == "__main__":
    main()
