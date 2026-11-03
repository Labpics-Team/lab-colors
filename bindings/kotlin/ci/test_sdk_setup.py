"""SDK из объявленного job root работает без предустановленного latest."""
import ast
import importlib.util
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location("android_sdk_build", Path(__file__).with_name("build.py"))
owner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)


def require_sdk_setup(source, name):
    match = re.search(r"(?ms)^  " + name + r":\n.*?(?=^  [a-zA-Z0-9_-]+:\n|\Z)", source)
    if not match:
        raise AssertionError("Android job missing")
    block = match.group()
    steps = re.findall(r"(?ms)^      - .*?(?=^      - |\Z)", block)
    setup = [step for step in steps if "uses: android-actions/setup-android@" in step]
    if len(setup) != 1 or "\n        if:" in setup[0] or "continue-on-error:" in setup[0]:
        raise AssertionError("Android SDK setup is missing or optional")
    if "android-actions/setup-android@035682743ab223ec9c6169af09811dff8247e520" not in setup[0]:
        raise AssertionError("Android SDK setup lost its reviewed source pin")
    if "cmdline-tools-version: '15859902'" not in setup[0] or "/android-sdk" not in setup[0]:
        raise AssertionError("Android command-line tools lost version or job root")
    target = "bindings/kotlin/ci/build.py" if name == "android-build" else "bindings/kotlin/ci/test-consumer.py"
    if block.index(setup[0]) >= block.index(target):
        raise AssertionError("Android SDK setup occurs after its consumer")
    if name == "android-consumer" and "platforms;android-36 build-tools;36.0.0" not in setup[0]:
        raise AssertionError("Android compilation prerequisites absent before emulator setup")


class AndroidSdkSetupTests(unittest.TestCase):
    def tool(self, root, version):
        tool = root / "cmdline-tools" / version / "bin/sdkmanager"
        tool.parent.mkdir(parents=True)
        tool.write_text("#!/bin/sh\nexit 0\n")
        tool.chmod(0o700)
        return tool

    def test_versioned_and_latest_tools_use_the_declared_sdk(self):
        for version in ("22.0", "latest"):
            with self.subTest(version=version), tempfile.TemporaryDirectory(prefix="android-sdk-control-") as name:
                root = Path(name)
                tool = self.tool(root, version)
                with patch.dict(os.environ, {"PATH": str(tool.parent)}):
                    self.assertEqual(owner.sdk_manager(root), tool.resolve())

    def test_missing_or_foreign_tools_do_not_fall_back_to_a_guessed_latest(self):
        with tempfile.TemporaryDirectory(prefix="android-sdk-control-") as name:
            root = Path(name)
            sdk = root / "sdk"
            self.tool(sdk, "22.0")
            foreign = self.tool(root / "foreign", "22.0")
            for path in (str(root / "absent"), str(foreign.parent)):
                with self.subTest(path=path), patch.dict(os.environ, {"PATH": path}):
                    with self.assertRaises(RuntimeError):
                        owner.sdk_manager(sdk)

    def test_sdk_directory_symlink_cannot_redirect_to_another_sdk(self):
        with tempfile.TemporaryDirectory(prefix="android-sdk-control-") as name:
            root = Path(name)
            foreign = self.tool(root / "foreign", "22.0")
            sdk = root / "sdk"
            sdk.mkdir()
            (sdk / "cmdline-tools").symlink_to(root / "foreign/cmdline-tools", target_is_directory=True)
            with patch.dict(os.environ, {"PATH": str(foreign.parent)}):
                with self.assertRaises(RuntimeError):
                    owner.sdk_manager(sdk)

    def test_actual_consumer_sdk_call_uses_the_shared_owner(self):
        source = Path(__file__).with_name("prepare-consumer.py").read_text()
        tree = ast.parse(source)
        main = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "main")
        index = next(index for index, node in enumerate(main.body)
                     if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id == "sdkmanager" for target in node.targets))
        statements = main.body[index:index + 2]
        with tempfile.TemporaryDirectory(prefix="android-sdk-consumer-control-") as name:
            root = Path(name)
            tool = self.tool(root, "22.0")
            calls = []
            def execute(command, *arguments):
                subprocess.run([command, *arguments], check=True, capture_output=True, timeout=5)
                calls.append((command, arguments))
            scope = {"sdk_manager": owner.sdk_manager, "Path": Path, "os": os, "run": execute}
            with patch.dict(os.environ, {"PATH": str(tool.parent), "ANDROID_HOME": str(root)}):
                exec(compile(ast.Module(body=statements, type_ignores=[]), "actual-consumer-sdk", "exec"), scope)
                self.assertEqual(calls, [(str(tool), ("--install", "platforms;android-36", "build-tools;36.0.0"))])
                mutant = ast.parse('sdkmanager = Path(os.environ["ANDROID_HOME"]) / "cmdline-tools/latest/bin/sdkmanager"').body[0]
                with self.assertRaises(FileNotFoundError):
                    exec(compile(ast.Module(body=[mutant, statements[1]], type_ignores=[]), "old-consumer-sdk", "exec"), scope)

    def test_both_sdk_consumers_provision_before_sdk_use(self):
        source = (ROOT / ".github/workflows/native-conformance-worker.yml").read_text()
        for name in ("android-build", "android-consumer"):
            with self.subTest(job=name):
                require_sdk_setup(source, name)
                mutant = re.sub(r"(?ms)^      - name: prepare the declared Android SDK\n.*?(?=^      - |\Z)", "", source)
                with self.assertRaises(AssertionError):
                    require_sdk_setup(mutant, name)


if __name__ == "__main__":
    unittest.main()
