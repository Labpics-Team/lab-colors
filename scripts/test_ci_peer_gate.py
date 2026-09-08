#!/usr/bin/env python3
"""Adversarial tests for the fail-closed CI peer gate."""

from __future__ import annotations

import copy
import importlib.util
import json
import os
from pathlib import Path
from ci_workflow_binding import TestCiWorkflowBinding, verify_ci_binding
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


REPO = Path(__file__).resolve().parents[1]
SCRIPT = Path(__file__).with_name("ci_peer_gate.py")
SPEC = importlib.util.spec_from_file_location("ci_peer_gate", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
gate = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = gate
SPEC.loader.exec_module(gate)


SHA = "a" * 40
CI = ".github/workflows/ci.yml"
NATIVE = ".github/workflows/native-conformance.yml"


def run(
    path: str,
    *,
    status: str = "completed",
    conclusion: str | None = "success",
    head_sha: str = SHA,
    event: str = "pull_request",
    run_id: int = 1,
    run_attempt: int = 1,
) -> dict[str, object]:
    return {
        "path": path,
        "status": status,
        "conclusion": conclusion,
        "head_sha": head_sha,
        "event": event,
        "id": run_id,
        "run_attempt": run_attempt,
    }


class PeerGateTest(unittest.TestCase):
    def evaluate(self, runs: list[dict], event: str = "pull_request"):
        return gate.evaluate_runs(runs, head_sha=SHA, event=event)

    def test_exact_required_successes_are_the_only_green_state(self) -> None:
        decision = self.evaluate([run(CI), run(NATIVE)])
        self.assertEqual(decision.state, "success")

    def test_empty_run_set_waits_and_can_never_turn_green(self) -> None:
        decision = self.evaluate([])
        self.assertEqual(decision.state, "wait")
        self.assertEqual(len(decision.reasons), 2)

    def test_wrong_head_and_wrong_event_do_not_satisfy_admission(self) -> None:
        for mutant in (
            [run(CI, head_sha="b" * 40), run(NATIVE)],
            [run(CI, event="merge_group"), run(NATIVE)],
        ):
            with self.subTest(mutant=mutant):
                self.assertEqual(self.evaluate(mutant).state, "wait")

    def test_skipped_neutral_cancelled_and_failure_are_red(self) -> None:
        for conclusion in ("skipped", "neutral", "cancelled", "failure", None):
            with self.subTest(conclusion=conclusion):
                decision = self.evaluate(
                    [run(CI, conclusion=conclusion), run(NATIVE)]
                )
                self.assertEqual(decision.state, "fail")

    def test_pending_required_run_waits(self) -> None:
        decision = self.evaluate(
            [run(CI, status="in_progress", conclusion=None), run(NATIVE)]
        )
        self.assertEqual(decision.state, "wait")

    def test_unknown_status_is_red_not_pending(self) -> None:
        decision = self.evaluate(
            [run(CI, status="invented", conclusion=None), run(NATIVE)]
        )
        self.assertEqual(decision.state, "fail")

    def test_latest_attempt_owns_the_result(self) -> None:
        decision = self.evaluate(
            [
                run(CI, conclusion="failure", run_id=10, run_attempt=1),
                run(CI, run_id=10, run_attempt=2),
                run(NATIVE),
            ]
        )
        self.assertEqual(decision.state, "success")

    def test_new_run_supersedes_an_older_rerun(self) -> None:
        decision = self.evaluate(
            [
                run(CI, run_id=10, run_attempt=2),
                run(CI, conclusion="failure", run_id=11, run_attempt=1),
                run(NATIVE),
            ]
        )
        self.assertEqual(decision.state, "fail")

    def test_unadmitted_peer_is_red(self) -> None:
        decision = self.evaluate(
            [run(CI), run(NATIVE), run(".github/workflows/new-peer.yml")]
        )
        self.assertEqual(decision.state, "fail")

    def test_gate_run_is_ignored_without_counting_as_evidence(self) -> None:
        decision = self.evaluate(
            [run(gate.GATE_WORKFLOW), run(CI), run(NATIVE)]
        )
        self.assertEqual(decision.state, "success")

    def test_merge_queue_requires_the_same_complete_evidence(self) -> None:
        decision = self.evaluate(
            [run(CI, event="merge_group"), run(NATIVE, event="merge_group")],
            event="merge_group",
        )
        self.assertEqual(decision.state, "success")

    def test_unsupported_event_fails_closed(self) -> None:
        self.assertEqual(self.evaluate([], event="workflow_dispatch").state, "fail")

    def test_ci_caller_selects_the_worker_from_its_own_snapshot(self) -> None:
        verify_ci_binding(REPO, os.environ)


def browser_script(workflow: str | None = None) -> str:
    if workflow is None:
        try:
            workflow = (REPO / ".github/workflows/ci-worker.yml").read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as error:
            raise AssertionError("cannot read browser workflow as UTF-8") from error
    anchor = "      - name: terminal Program in real browser\n"
    if workflow.count(anchor) != 1:
        raise AssertionError("browser step anchor must be unique")
    step = workflow.split(anchor, 1)[1].split("\n      - ", 1)[0]
    run = "        run: |\n"
    if step.count(run) != 1:
        raise AssertionError("browser step must have one literal run block")
    lines: list[str] = []
    for line in step.split(run, 1)[1].splitlines():
        if line.strip() and not line.startswith(" " * 10):
            break
        lines.append(line[10:])
    while lines and not lines[-1]:
        lines.pop()
    return "\n".join(lines) + "\n"


class BrowserBinaryAdmissionTest(unittest.TestCase):
    def test_script_read_errors_remain_assertions(self) -> None:
        for error in (OSError("unreadable workflow"),
                      UnicodeDecodeError("utf-8", b"\xff", 0, 1, "invalid")):
            with self.subTest(error=error), mock.patch.object(Path, "read_text", side_effect=error):
                with self.assertRaises(AssertionError) as failure:
                    browser_script()
                self.assertIs(failure.exception.__cause__, error)

    def test_script_extraction_preserves_body_and_stops_at_next_step(self) -> None:
        source = ("      - name: terminal Program in real browser\n"
                  "        run: |\n          echo first\n\n          echo second\n"
                  "      - name: next\n        run: |\n          echo foreign\n")
        self.assertEqual(browser_script(source), "echo first\n\necho second\n")
        for mutant in (source.replace("terminal Program in real browser", "renamed"),
                       source.replace("        run: |\n", "        run: >\n", 1),
                       source + source):
            with self.subTest(mutant=mutant), self.assertRaises(AssertionError):
                browser_script(mutant)

    def exercise(self, *, script: str | None = None, wasm_exit: int = 0,
                 missing_browser: bool = False, capabilities: dict | None = None) -> dict:
        node = shutil.which("node")
        bash = shutil.which("bash")
        if node is None or bash is None:
            self.fail("real Node and Bash are required")
        original = capabilities if capabilities is not None else json.loads(
            (REPO / "crates/labcolors-wasm/webdriver.json").read_text(encoding="utf-8"))
        with tempfile.TemporaryDirectory(prefix="browser admission ") as directory:
            root = Path(directory)
            temp = root / 'temp " quoted'
            bin_dir = root / "bin"
            temp.mkdir()
            bin_dir.mkdir()
            webdriver = root / "crates/labcolors-wasm/webdriver.json"
            webdriver.parent.mkdir(parents=True)
            webdriver.write_text(json.dumps(original) + "\n")
            before = webdriver.read_bytes()
            browser = temp / 'verified " chrome'
            driver = temp / "verified driver"
            for executable in (browser, driver):
                executable.write_text("#!/bin/sh\nexit 0\n")
                executable.chmod(0o755)
            if missing_browser:
                browser.unlink()
            inherited = temp / "foreign-webdriver.json"
            inherited.write_text("foreign configuration\n")
            captured = root / "captured.json"
            product_marker = root / "product-called"
            wasm_marker = root / "wasm-called"
            wasm = bin_dir / "wasm-pack"
            wasm.write_text(f"#!{sys.executable}\n" + r'''
import json, os, pathlib, sys
pathlib.Path(os.environ["WASM_MARKER"]).write_text("called\n")
path = os.environ.get("WASM_BINDGEN_TEST_WEBDRIVER_JSON")
if not path:
    sys.exit(69)
config = pathlib.Path(path)
try:
    value = json.loads(config.read_text())
except (ValueError, OSError):
    sys.exit(70)
pathlib.Path(os.environ["CAPTURED"]).write_text(json.dumps({
    "capabilities": value, "path": path, "mode": config.stat().st_mode & 0o777,
    "args": sys.argv[1:]}))
if value.get("goog:chromeOptions", {}).get("binary") != os.environ["CHROME_PATH"]:
    sys.exit(73)
sys.exit(int(os.environ["WASM_EXIT"]))
''')
            wasm.chmod(0o755)
            node_wrapper = bin_dir / "node"
            node_wrapper.write_text(f"#!{sys.executable}\n" + r'''
import os, pathlib, sys
if sys.argv[1:2] == ["scripts/test-program-runtime-browser.mjs"]:
    pathlib.Path(os.environ["PRODUCT_MARKER"]).write_text("called\n")
else:
    os.execv(os.environ["REAL_NODE"], [os.environ["REAL_NODE"], *sys.argv[1:]])
''')
            node_wrapper.chmod(0o755)
            result = subprocess.run([bash, "--noprofile", "--norc", "-c", script or browser_script()],
                cwd=root, env={**os.environ, "PATH": f"{bin_dir}:{os.environ['PATH']}",
                    "REAL_NODE": node, "RUNNER_TEMP": str(temp), "GITHUB_JOB": "wasm",
                    "CHROME_PATH": str(browser), "CHROMEDRIVER_PATH": str(driver),
                    "WASM_BINDGEN_TEST_WEBDRIVER_JSON": str(inherited), "WASM_EXIT": str(wasm_exit),
                    "CAPTURED": str(captured), "PRODUCT_MARKER": str(product_marker), "WASM_MARKER": str(wasm_marker),
                    "VERIFIED_TARBALL": "/fixture/archive.tgz", "VERIFIED_TARBALL_SHA256": "a" * 64},
                capture_output=True, text=True, timeout=15)
            capture = json.loads(captured.read_text()) if captured.exists() else None
            self.assertEqual(webdriver.read_bytes(), before, "source capabilities must not be rewritten")
            self.assertEqual(inherited.read_text(), "foreign configuration\n")
            if capture:
                self.assertEqual(Path(capture["path"]).parent, temp)
                self.assertFalse(Path(capture["path"]).exists(), "owned webdriver JSON must be removed on exit")
            self.assertEqual(sorted(path.name for path in temp.iterdir()),
                             sorted(["foreign-webdriver.json", "verified driver"] +
                                    ([] if missing_browser else ['verified " chrome'])))
            return {"code": result.returncode, "stderr": result.stderr, "capture": capture,
                    "product_called": product_marker.exists(), "wasm_called": wasm_marker.exists(),
                    "original": original, "browser": str(browser)}

    def test_real_bash_binds_verified_browser_and_preserves_every_option(self) -> None:
        result = self.exercise()
        self.assertEqual(result["code"], 0, result)
        expected = copy.deepcopy(result["original"])
        expected["goog:chromeOptions"]["binary"] = result["browser"]
        self.assertEqual(result["capture"]["capabilities"], expected)
        self.assertEqual(result["capture"]["mode"], 0o600)
        self.assertEqual(result["capture"]["args"], ["test", "--headless", "--chrome", "--chromedriver",
                        str(Path(result["browser"]).with_name("verified driver")), "crates/labcolors-wasm", "--locked"])
        self.assertTrue(result["product_called"])

    def test_failed_wasm_run_cleans_temp_and_prevents_product_probe(self) -> None:
        result = self.exercise(wasm_exit=42)
        self.assertEqual(result["code"], 42, result)
        self.assertFalse(result["product_called"])

    def test_missing_executable_and_invalid_options_fail_before_wasm_pack(self) -> None:
        cases = (self.exercise(missing_browser=True),
                 self.exercise(capabilities={}),
                 self.exercise(capabilities={"browserName": "chrome", "goog:chromeOptions": []}))
        for result in cases:
            self.assertNotEqual(result["code"], 0, result)
            self.assertIsNone(result["capture"])
            self.assertFalse(result["wasm_called"])
            self.assertFalse(result["product_called"])

    def test_removing_explicit_binary_reproduces_the_admission_failure(self) -> None:
        script = browser_script()
        binding = 'capabilities["goog:chromeOptions"].binary = process.env.CHROME_PATH;'
        self.assertEqual(script.count(binding), 1)
        result = self.exercise(script=script.replace(binding, ""))
        self.assertEqual(result["code"], 73, result)
        self.assertFalse(result["product_called"])

    def test_browser_setup_does_not_rely_on_driver_discovery(self) -> None:
        workflow = (REPO / ".github/workflows/ci-worker.yml").read_text(encoding="utf-8")
        self.assertNotIn("CHROME_BIN_DIR", workflow)
        self.assertNotIn('ln -sf "$CHROME_BIN"', workflow)
        self.assertIn('wasm-pack test --headless --chrome --chromedriver "$CHROMEDRIVER_PATH"', browser_script())


if __name__ == "__main__":
    unittest.main()
