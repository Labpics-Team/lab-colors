"""Штатный hosted SDK разрешается из объявленного root независимо от PATH."""
import ast
import importlib.util
import os
from pathlib import Path
import re
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
    if "    runs-on: ubuntu-latest\n" not in block or "setup-android@" in block:
        raise AssertionError("Android consumer must use the declared hosted SDK")
    target = "bindings/kotlin/ci/build.py" if name == "android-build" else "bindings/kotlin/ci/test-consumer.py"
    if target not in block:
        raise AssertionError("Android SDK consumer is missing")


class AndroidSdkSetupTests(unittest.TestCase):
    def tool(self, root, version):
        tool = root / "cmdline-tools" / version / "bin/sdkmanager"
        tool.parent.mkdir(parents=True)
        tool.write_text("#!/bin/sh\nexit 0\n")
        tool.chmod(0o700)
        return tool

    def test_declared_latest_tools_do_not_require_path_discovery(self):
        for path in ("", "foreign-tools"):
            with self.subTest(path=path), tempfile.TemporaryDirectory(prefix="android-sdk-control-") as name:
                root = Path(name)
                tool = self.tool(root, "latest")
                with patch.dict(os.environ, {"PATH": path}):
                    self.assertEqual(owner.sdk_manager(root), tool.resolve())

    def test_missing_declared_tools_do_not_fall_back_to_path(self):
        with tempfile.TemporaryDirectory(prefix="android-sdk-control-") as name:
            root = Path(name)
            sdk = root / "sdk"
            self.tool(sdk, "22.0")
            foreign = self.tool(root / "foreign", "22.0")
            for path in (str(root / "absent"), str(foreign.parent)):
                with self.subTest(path=path), patch.dict(os.environ, {"PATH": path}):
                    with self.assertRaises(FileNotFoundError):
                        owner.sdk_manager(sdk)

    def test_sdk_directory_symlink_cannot_redirect_to_another_sdk(self):
        with tempfile.TemporaryDirectory(prefix="android-sdk-control-") as name:
            root = Path(name)
            foreign = self.tool(root / "foreign", "latest")
            sdk = root / "sdk"
            sdk.mkdir()
            (sdk / "cmdline-tools").symlink_to(root / "foreign/cmdline-tools", target_is_directory=True)
            with patch.dict(os.environ, {"PATH": str(foreign.parent)}):
                with self.assertRaises(RuntimeError):
                    owner.sdk_manager(sdk)

    def test_non_executable_declared_tool_is_rejected(self):
        with tempfile.TemporaryDirectory(prefix="android-sdk-control-") as name:
            sdk = Path(name)
            self.tool(sdk, "latest")
            with patch.object(owner.os, "access", return_value=False):
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
            tool = self.tool(root, "latest")
            calls = []
            def execute(command, *arguments):
                calls.append((command, arguments))
            scope = {"sdk_manager": owner.sdk_manager, "Path": Path, "os": os, "run": execute}
            with patch.dict(os.environ, {"PATH": str(tool.parent), "ANDROID_HOME": str(root)}):
                exec(compile(ast.Module(body=statements, type_ignores=[]), "actual-consumer-sdk", "exec"), scope)
                self.assertEqual(calls, [(str(tool), ("--install", "platforms;android-36", "build-tools;36.0.0"))])

    def test_both_sdk_consumers_use_the_hosted_sdk(self):
        source = (ROOT / ".github/workflows/native-conformance-worker.yml").read_text()
        for name in ("android-build", "android-consumer"):
            with self.subTest(job=name):
                require_sdk_setup(source, name)
                mutant = source.replace("runs-on: ubuntu-latest", "runs-on: [self-hosted, Linux, X64]")
                with self.assertRaises(AssertionError):
                    require_sdk_setup(mutant, name)


if __name__ == "__main__":
    unittest.main()
