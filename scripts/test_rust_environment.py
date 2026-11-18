#!/usr/bin/env python3
"""Exercise compiler selection across CI and standalone fixture boundaries."""

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/ci/rust_environment.py"


class RustEnvironmentTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="rust-environment-")
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.bin = self.base / "bin"
        self.bin.mkdir()
        self.env = dict(os.environ)
        self.env.pop("CARGO_HOME", None)
        self.env.pop("RUSTUP_TOOLCHAIN", None)
        self.env.update({
            "PATH": str(self.bin) + os.pathsep + self.env["PATH"],
            "HOME": str(self.base / "home"),
            "GITHUB_ENV": str(self.base / "environment"),
            "SOURCE_ROOT": str(ROOT),
        })
        self.tool("rustup", '''#!/usr/bin/env python3
import os
import sys
from pathlib import Path
assert sys.argv[1:] == ["show", "active-toolchain"]
assert Path.cwd() == Path(os.environ["SOURCE_ROOT"])
if os.environ.get("FAIL_SELECTION"):
    sys.exit(17)
print(os.environ["SELECTED_TOOLCHAIN"] + " (overridden by source tree)")
''')
        self.tool("cargo", '''#!/usr/bin/env python3
import os
print(os.environ.get("RUSTUP_TOOLCHAIN", "runner-default"))
print(os.environ.get("CARGO_HOME", "missing"))
''')

    def tool(self, name, content):
        path = self.bin / name
        path.write_text(content)
        path.chmod(0o755)

    def test_selected_compiler_and_cargo_home_follow_external_builds(self):
        for compiler in ("1.2.3-test-host", "1.85.0-test-host"):
            for cargo_home in (None, str(self.base / "configured cargo")):
                with self.subTest(compiler=compiler, cargo_home=cargo_home):
                    env = dict(self.env, SELECTED_TOOLCHAIN=compiler)
                    if cargo_home:
                        env["CARGO_HOME"] = cargo_home
                    env_file = Path(env["GITHUB_ENV"])
                    env_file.unlink(missing_ok=True)
                    subprocess.run([sys.executable, str(SCRIPT)], cwd=self.base,
                                   env=env, check=True, capture_output=True)
                    exported = dict(line.split("=", 1) for line in env_file.read_text().splitlines())
                    expected_home = cargo_home or str(Path(env["HOME"]) / ".cargo")
                    self.assertEqual(exported, {"RUSTUP_TOOLCHAIN": compiler, "CARGO_HOME": expected_home})
                    # A fixture without rust-toolchain.toml must not use the runner default.
                    fixture = subprocess.check_output(["cargo", "--version"], cwd=self.base,
                                                      env=dict(env, **exported), text=True)
                    self.assertEqual(fixture.splitlines(), [compiler, expected_home])

    def test_failed_selection_does_not_publish_an_environment(self):
        result = subprocess.run([sys.executable, str(SCRIPT)], cwd=self.base,
                                env=dict(self.env, FAIL_SELECTION="1"), capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(Path(self.env["GITHUB_ENV"]).exists())


if __name__ == "__main__":
    unittest.main()
