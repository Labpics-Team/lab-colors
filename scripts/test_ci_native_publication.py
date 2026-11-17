"""Native archives must retain their tested source identity at publication."""

import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock


spec = importlib.util.spec_from_file_location(
    "publish_native", Path(__file__).with_name("publish-native.py"))
publisher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(publisher)


class NativeSourceTagTest(unittest.TestCase):
    def setUp(self):
        environment = mock.patch.dict(os.environ)
        environment.start()
        self.addCleanup(environment.stop)
        for name in list(os.environ):
            if name.startswith("GIT_"):
                del os.environ[name]
        os.environ.update({
            "GIT_CONFIG_GLOBAL": os.devnull,
            "GIT_CONFIG_SYSTEM": os.devnull,
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_TERMINAL_PROMPT": "0",
        })

    def test_missing_matching_and_conflicting_tags(self):
        with tempfile.TemporaryDirectory() as directory:
            def git(*args):
                return subprocess.check_output(
                    ["git", "-C", directory, *args], text=True,
                    stderr=subprocess.PIPE).strip()

            git("init", "-q")
            git("config", "user.name", "Native publication test")
            git("config", "user.email", "native-test@example.invalid")
            git("-c", "commit.gpgsign=false", "commit", "--allow-empty", "-qm", "tested source")
            tested = git("rev-parse", "HEAD")
            git("-c", "commit.gpgsign=false", "commit", "--allow-empty", "-qm", "different source")
            different = git("rev-parse", "HEAD")
            publisher.verify_source_tag(directory, "absent", tested)
            for annotated in (False, True):
                for correct, commit in ((True, tested), (False, different)):
                    tag = f"native-{annotated}-{correct}"
                    git("-c", "tag.gpgsign=false", "tag", *(["-a", "-m", tag] if annotated else []), tag, commit)
                    with self.subTest(annotated=annotated, correct=correct):
                        if correct:
                            publisher.verify_source_tag(directory, tag, tested)
                        else:
                            with self.assertRaisesRegex(ValueError, "different source"):
                                publisher.verify_source_tag(directory, tag, tested)

    def test_unavailable_remote_is_not_an_absent_tag(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(subprocess.CalledProcessError):
                publisher.verify_source_tag(str(Path(directory) / "absent.git"), "native", "a" * 40)


if __name__ == "__main__":
    unittest.main()
