#!/usr/bin/env python3
"""Anti-vacuum contract for the offline Arb dependency build."""

from __future__ import annotations

import os
import subprocess
import unittest
from pathlib import Path


ARB = Path(__file__).resolve().parents[1]
BUILD = ARB / "build.sh"


class ArbBuildRecipeTests(unittest.TestCase):
    def test_recipe_is_offline_static_and_platform_explicit(self) -> None:
        source = BUILD.read_text(encoding="utf-8")

        for required in (
            "/usr/bin/env -i",
            "LC_BUILD_ENV_V1=1",
            'require_directory "$inputs/gmp-6.3.0"',
            'require_directory "$inputs/mpfr-4.2.2"',
            'require_directory "$inputs/flint-3.6.0"',
            'require_regular "$workspace/proof/region/v1/arb/evaluator/formula.h"',
            "9958f20c8ca598625db0593a45f8f8bc79e4b2f22b53263b6c32d78a5e1d2693",
            "-I.",
            "--build=x86_64-pc-linux-gnu",
            "--host=x86_64-pc-linux-gnu",
            "--disable-shared",
            "--enable-static",
            "--disable-assembly",
            "--enable-formally-proven-code",
            "--disable-lto",
            "--enable-assert",
            "-fno-fast-math",
            "-ffp-contract=off",
            "-fno-lto",
            "-std=gnu17",
            "-march=x86-64",
            "-mtune=generic",
            "-Wl,--build-id=none",
            "make check",
            "readelf",
        ):
            with self.subTest(required=required):
                self.assertIn(required, source)

        for forbidden in (
            "curl ",
            "wget ",
            "git clone",
            "apt-get",
            "brew ",
            "tar --extract",
            "-ffast-math",
            "-march=native",
            "-flto",
        ):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, source)

    @unittest.skipUnless(BUILD.exists(), "RED until the controlled build recipe exists")
    def test_recipe_rejects_ambient_or_incomplete_invocation_before_build(self) -> None:
        result = subprocess.run(
            [str(BUILD)],
            check=False,
            capture_output=True,
            env={
                "PATH": os.environ.get("PATH", ""),
                "UNDECLARED": "must-not-be-observed",
            },
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, b"")


if __name__ == "__main__":
    unittest.main(verbosity=2)
