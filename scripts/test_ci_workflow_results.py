#!/usr/bin/env python3
"""Native CI graph and result contracts; immutable source and browser bindings."""

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
import re
import unittest
from unittest import mock


REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "scripts/ci"))
import workflow_results as gate


def checks(expected):
    return {"results": ["success"] * expected[0],
            "children": [json.dumps(checks(child)) for child in expected[1]]}


def needs(expected):
    return {name: {"result": producer.result,
                   "outputs": {} if producer.checks is None or producer.result == "skipped" else
                       {"checks": json.dumps(checks(producer.checks))}}
            for name, producer in expected.items()}


class NativeResultTest(unittest.TestCase):
    def setUp(self):
        self.expected = gate.graph(REPO)
        self.good = needs(self.expected)

    def test_exact_native_successes_are_green(self):
        gate.verify_needs(self.good, self.expected)

    def test_missing_empty_unknown_and_unsuccessful_dependencies_are_red(self):
        values = [{}, None, [], self.good | {"unknown": {"result": "success"}}]
        for name in self.good:
            values.append({key: value for key, value in self.good.items() if key != name})
            for status in ("failure", "cancelled", "skipped", "neutral", "pending", "", "invented", None):
                values.append(self.good | {name: {"result": status}})
            values.append(self.good | {name: {"outputs": self.good[name]["outputs"]}})
        for value in values:
            with self.subTest(value=value), self.assertRaises(gate.WorkflowError):
                gate.verify_needs(value, self.expected)

    def test_missing_empty_unknown_or_unsuccessful_nested_results_are_red(self):
        expected = (2, ((3, ()),))
        good = checks(expected)
        gate.verify_checks(good, expected)
        values = [None, {}, [], {"results": [], "children": []},
                  good | {"extra": "unknown"}, good | {"results": ["success"]},
                  good | {"children": []}, good | {"children": [None]}]
        for status in ("failure", "cancelled", "skipped", "neutral", "", "invented", None):
            values.append(good | {"results": ["success", status]})
            values.append(good | {"children": [json.dumps({"results": ["success", status, "success"], "children": []})]})
        for value in values:
            with self.subTest(value=value), self.assertRaises((gate.WorkflowError, ValueError, TypeError)):
                gate.verify_checks(value, expected)

    def test_child_order_does_not_change_success(self):
        expected = (3, ((1, ()), (2, ())))
        value = checks(expected)
        value["children"].reverse()
        gate.verify_checks(value, expected)

    def test_artifact_outputs_do_not_satisfy_native_results(self):
        expected = (2, ())
        with self.assertRaises(gate.WorkflowError):
            gate.verify_checks({"results": ["success", "skipped"], "children": [], "outputs": {"shard": "success"}}, expected)

    def test_transport_contains_only_small_result_projections(self):
        expected = self.expected["ci"]
        self.assertLess(len(json.dumps(needs(self.expected)).encode()), 8192)
        source = (REPO / ".github/workflows/ci-worker.yml").read_text(encoding="utf-8")
        self.assertIn("github.event_name == 'push' && format(", source)
        self.assertNotIn("toJSON(jobs)", gate.CHECKS)

    def test_real_cli_binds_named_producers_to_the_native_event(self):
        for event in ("pull_request", "merge_group"):
            expected = gate.graph(REPO, event)
            good = needs(expected)
            self.assertEqual(good["formal"]["result"], "skipped" if event == "pull_request" else "success")
            self.assertEqual(good["ci"]["result"], "success")
            variants = [(event, good, True), ("workflow_dispatch", good, False),
                        ("merge_group" if event == "pull_request" else "pull_request", good, False)]
            swapped = copy.deepcopy(good)
            swapped["ci"]["result"], swapped["formal"]["result"] = "skipped", "success"
            variants.append((event, swapped, False))
            for name in good:
                for status in ("failure", "cancelled", "neutral", "", None):
                    bad = copy.deepcopy(good)
                    bad[name]["result"] = status
                    with self.subTest(event=event, name=name, status=status), self.assertRaises(gate.WorkflowError):
                        gate.verify_needs(bad, expected)
                    if name == "ci" and status in ("failure", "cancelled"):
                        variants.append((event, bad, False))
            for bad in ({name: value for name, value in good.items() if name != "formal"},
                        good | {"unknown": {"result": "success", "outputs": {}}}):
                variants.append((event, bad, False))
            for bound_event, value, success in variants:
                with self.subTest(event=event, bound_event=bound_event, value=value):
                    result = subprocess.run([sys.executable, str(REPO / "scripts/ci/workflow_results.py"), "complete"],
                        env={**os.environ, "GATE_EVENT": bound_event, "CI_NEEDS": json.dumps(value)},
                        capture_output=True, text=True, timeout=10)
                    self.assertEqual(result.returncode == 0, success, result.stderr)

    def test_skipped_producer_cannot_export_stale_results(self):
        expected = gate.graph(REPO, "pull_request")
        good = needs(expected)
        for output in ({"checks": json.dumps(checks(expected["formal"].checks))},
                       {"unknown": "success"}, None):
            bad = copy.deepcopy(good)
            bad["formal"]["outputs"] = output
            with self.subTest(output=output), self.assertRaises(gate.WorkflowError):
                gate.verify_needs(bad, expected)
        good["formal"]["outputs"] = {"checks": ""}
        gate.verify_needs(good, expected)


class NativeWorkflowGraphTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="native-ci-graph-")
        self.root = Path(self.temp.name)
        shutil.copytree(REPO / ".github/workflows", self.root / ".github/workflows")

    def tearDown(self):
        self.temp.cleanup()

    def test_all_premerge_workflows_are_in_one_native_graph(self):
        gate.graph(self.root)

    def test_independent_new_or_renamed_premerge_root_is_red(self):
        for event in ("pull_request", "pull_request_target", "merge_group"):
            p = self.root / ".github/workflows/new-root.yml"
            p.write_text(f"on:\n  {event}:\njobs:\n  new:\n    runs-on: ubuntu-latest\n    steps:\n      - run: true\n", encoding="utf-8")
            with self.subTest(event=event), self.assertRaises(gate.WorkflowError):
                gate.graph(self.root)
            p.unlink()

    def test_removed_dependency_skipped_gate_or_unbound_context_is_red(self):
        p = self.root / gate.ENTRY
        source = p.read_text(encoding="utf-8")
        for before, after in (("workflow-policy, ci, formal, native, admission", "workflow-policy, ci, native, admission"),
                              ("if: ${{ always() }}", "if: success()"),
                              ("toJSON(needs)", "toJSON(github)"),
                              ("github.workflow_sha", "github.sha"),
                              ("./.github/workflows/ci-worker.yml", "foreign/repo/.github/workflows/ci-worker.yml@main")):
            p.write_text(source.replace(before, after), encoding="utf-8")
            with self.subTest(before=before), self.assertRaises(gate.WorkflowError):
                gate.graph(self.root)
        p.write_text(source, encoding="utf-8")

    def test_new_jobs_are_discovered_and_missing_or_failed_results_are_red(self):
        p = self.root / ".github/workflows/native-conformance-worker.yml"
        source = p.read_text(encoding="utf-8")
        old = gate.graph(self.root)
        p.write_text(source + "\n  new-check:\n    runs-on: ubuntu-latest\n    steps:\n      - run: true\n", encoding="utf-8")
        current = gate.graph(self.root)
        with self.assertRaises(gate.WorkflowError):
            gate.verify_needs(needs(old), current)
        value = needs(current)
        gate.verify_needs(value, current)
        tree = json.loads(value["native"]["outputs"]["checks"])
        tree["results"][-1] = "failure"
        value["native"]["outputs"]["checks"] = json.dumps(tree)
        with self.assertRaises(gate.WorkflowError):
            gate.verify_needs(value, current)

    def test_conditional_new_check_or_missing_nested_export_is_red(self):
        p = self.root / ".github/workflows/package-worker.yml"
        source = p.read_text(encoding="utf-8")
        for before, after in (("  wasm:\n", "  wasm:\n    if: false\n"),
                              (gate.CHECKS, "${{ toJSON(jobs.wasm.outputs) }}")):
            p.write_text(source.replace(before, after), encoding="utf-8")
            with self.subTest(before=before), self.assertRaises(gate.WorkflowError):
                gate.graph(self.root)
        p.write_text(source, encoding="utf-8")

    def test_yaml_duplicate_keys_and_empty_jobs_are_red(self):
        p = self.root / ".github/workflows/package-worker.yml"
        for value in ("on: workflow_call\njobs: {}\n", "jobs:\n  duplicate: {}\n  duplicate: {}\n"):
            p.write_text(value, encoding="utf-8")
            with self.subTest(value=value), self.assertRaises(gate.WorkflowError):
                gate.graph(self.root)

    def test_ci_caller_selects_the_worker_from_its_own_snapshot(self):
        verify_ci_binding(REPO, os.environ)

    def test_event_guard_cannot_hide_a_partial_qualification_graph(self):
        p = self.root / gate.ENTRY
        source = p.read_text(encoding="utf-8")
        for after in ("github.event_name != 'pull_request'", "always() && github.event_name == 'merge_group'", "false"):
            p.write_text(source.replace("always() && github.event_name != 'pull_request'", after), encoding="utf-8")
            with self.subTest(after=after), self.assertRaises(gate.WorkflowError):
                gate.graph(self.root)
        p.write_text(source, encoding="utf-8")
        formal = self.root / ".github/workflows/formal-worker.yml"
        original = formal.read_text(encoding="utf-8")
        formal.write_text(original + "\n  new-check:\n    runs-on: ubuntu-latest\n    steps:\n      - run: true\n", encoding="utf-8")
        current = gate.graph(self.root)
        with self.assertRaises(gate.WorkflowError):
            gate.verify_needs(needs(gate.graph(REPO)), current)
        gate.verify_needs(needs(current), current)
        formal.write_text(original.replace("  formal-positive:\n", "  formal-positive:\n    if: false\n"), encoding="utf-8")
        with self.assertRaises(gate.WorkflowError):
            gate.graph(self.root, "pull_request")

    def test_deferred_premerge_qualification_remains_full_for_publication(self):
        p = self.root / ".github/workflows/ci.yml"
        source = p.read_text(encoding="utf-8")
        call = "  formal:\n    name: Formal qualification\n    uses: ./.github/workflows/formal-worker.yml\n\n"
        self.assertIn(call, source)
        for after in ("", call.replace("    uses:", "    if: false\n    uses:"),
                      call.replace("    uses:", "    continue-on-error: true\n    uses:")):
            p.write_text(source.replace(call, after), encoding="utf-8")
            with self.subTest(after=after), self.assertRaises(gate.WorkflowError):
                gate.graph(self.root)

def browser_script(workflow: str | None = None) -> str:
    if workflow is None:
        try:
            workflow = (REPO / ".github/workflows/package-worker.yml").read_text(encoding="utf-8")
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
