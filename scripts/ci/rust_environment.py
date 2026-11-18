#!/usr/bin/env python3
"""Carry the selected Rust compiler and Cargo home into sibling build roots."""

import os
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def selected_rust_environment():
    # rustup owns file/override resolution, including the explicit MSRV job.
    # Resolve inside the source tree before a caller enters a fixture/worktree.
    active = subprocess.check_output(
        ["rustup", "show", "active-toolchain"], cwd=ROOT, text=True
    ).split()
    if not active:
        raise RuntimeError("rustup did not report the selected toolchain")
    return {
        "CARGO_HOME": os.environ.get("CARGO_HOME") or str(Path.home() / ".cargo"),
        "RUSTUP_TOOLCHAIN": active[0],
    }


def main():
    environment = selected_rust_environment()
    with open(os.environ["GITHUB_ENV"], "a", encoding="utf-8") as output:
        for name, value in environment.items():
            output.write("{}={}\n".format(name, value))


if __name__ == "__main__":
    main()
