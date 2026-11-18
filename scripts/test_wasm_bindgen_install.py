#!/usr/bin/env python3
"""Exercise the locked CLI installer without registry or release network access."""
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest


class LockedBindgenInstallTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "scripts").mkdir()
        (self.root / "bin").mkdir()
        self.script = self.root / "scripts/install-wasm-bindgen.sh"
        shutil.copyfile(Path(__file__).with_name("install-wasm-bindgen.sh"), self.script)
        self.lock = self.root / "Cargo.lock"
        self.package = ('[[package]]\nname = "wasm-bindgen"\nversion = "0.2.999"\n'
                        'source = "registry+https://github.com/rust-lang/crates.io-index"\n')
        self.lock.write_text(self.package)
        self.log = self.root / "calls.jsonl"
        self.asset = self.root / "release.tar.gz"
        with tarfile.open(self.asset, "w:gz") as archive:
            for name in ("wasm-bindgen", "wasm-bindgen-test-runner"):
                content = b'#!/bin/sh\nprintf "wasm-bindgen 0.2.999\\n"\n'
                entry = tarfile.TarInfo("upstream/" + name)
                entry.mode = 0o755
                entry.size = len(content)
                archive.addfile(entry, io.BytesIO(content))
        stub = '''#!/usr/bin/env python3
import json, os, pathlib, shutil, sys
name = pathlib.Path(sys.argv[0]).name
with open(os.environ["CALLS"], "a") as out:
    out.write(json.dumps([name, *sys.argv[1:]]) + "\\n")
if name == "curl":
    shutil.copyfile(os.environ["ASSET"], sys.argv[sys.argv.index("--output") + 1])
else:
    sys.exit(int(os.environ.get("VERIFY_EXIT", "0")))
'''
        for name in ("curl", "gh"):
            path = self.root / "bin" / name
            path.write_text(stub)
            path.chmod(0o755)
        self.environment = dict(os.environ, PATH=str(self.root / "bin") + os.pathsep + os.environ["PATH"],
                                RUNNER_TEMP=str(self.root), WASM_PACK_CACHE=str(self.root / "cache"),
                                GITHUB_PATH=str(self.root / "github-path"), CALLS=str(self.log), ASSET=str(self.asset))

    def run_install(self, **environment):
        return subprocess.run(["bash", str(self.script)], env=dict(self.environment, **environment),
                              capture_output=True, text=True, timeout=10)

    def test_selects_lockfile_version_and_verifies_before_materializing_cache(self):
        result = self.run_install()
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = [json.loads(line) for line in self.log.read_text().splitlines()]
        self.assertEqual([call[0] for call in calls], ["curl", "gh"])
        self.assertIn("/0.2.999/wasm-bindgen-0.2.999-x86_64-unknown-linux-musl.tar.gz", " ".join(calls[0]))
        self.assertEqual(calls[1][1:4], ["release", "verify-asset", "0.2.999"])
        self.assertEqual(calls[1][-2:], ["--repo", "github.com/wasm-bindgen/wasm-bindgen"])
        cache = self.root / "cache/wasm-bindgen-cargo-install-0.2.999"
        self.assertEqual(set(path.name for path in cache.iterdir()), {"wasm-bindgen", "wasm-bindgen-test-runner"})
        self.assertEqual((self.root / "github-path").read_text(), str(cache) + "\n")
        self.assertEqual(list(self.root.glob("wasm-bindgen.*")), [])

    def test_rejected_attestation_never_extracts_or_exposes_the_cli(self):
        self.asset.write_bytes(b"not an archive; extraction would fail differently")
        result = self.run_install(VERIFY_EXIT="23")
        self.assertEqual(result.returncode, 23, result.stderr)
        self.assertFalse((self.root / "github-path").exists())
        self.assertFalse((self.root / "cache").exists())
        self.assertEqual(list(self.root.glob("wasm-bindgen.*")), [])

    def test_missing_ambiguous_or_non_registry_lock_never_downloads(self):
        for contents in (self.package.replace('name = "wasm-bindgen"', 'name = "another"'),
                         self.package + self.package,
                         self.package.replace('registry+https://github.com/rust-lang/crates.io-index', 'git+https://example.invalid/fork')):
            with self.subTest(contents=contents):
                self.lock.write_text(contents)
                result = self.run_install()
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(self.log.exists())


if __name__ == "__main__":
    unittest.main()
