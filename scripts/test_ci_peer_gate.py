#!/usr/bin/env python3
"""Контракт нативного CI DAG и допуска точной попытки к публикации."""

from __future__ import annotations

import copy
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[1]
WORKFLOWS = REPO / ".github/workflows"
REPOSITORY = "Labpics-Team/lab-colors"
CI_PIN = "f65c3717ede52341c160d5dd56b6c3b65c8c8fed"
NATIVE_PIN = "1beda3770a990bb62d1b97e0188b1f2620e16c07"
SHA = "a" * 40
REQUIRED_JOBS = (
    "Node 22 consumer floor", "MSRV workspace check", "clippy + rustfmt",
    "cargo doc (intra-doc links)", "test", "cargo audit (rustsec)",
    "wasm build + headless test + size",
    "swift conformance (self-hosted Linux, pinned toolchain)", "CI",
)
GATE_SCRIPT = 'set -euo pipefail\n[[ "$RESULTS" == "success,success" ]]\n'
EXPECTED_CI_JOBS = f"""jobs:
  worker:
    name: CI
    permissions:
      contents: read
    uses: {REPOSITORY}/.github/workflows/ci-worker.yml@{CI_PIN}
  native:
    name: Native conformance (Swift)
    permissions:
      contents: read
    uses: {REPOSITORY}/.github/workflows/native-conformance-worker.yml@{NATIVE_PIN}
  ci:
    name: CI
    if: ${{{{ always() }}}}
    needs: [worker, native]
    runs-on: ubuntu-latest
    timeout-minutes: 5
    permissions: {{}}
    steps:
      - name: Require every native verification branch
        shell: bash
        env:
          RESULTS: ${{{{ join(needs.*.result, ',') }}}}
        run: |
          set -euo pipefail
          [[ "$RESULTS" == "success,success" ]]
"""


def source(name: str) -> str:
    return (WORKFLOWS / name).read_text(encoding="utf-8")


def without_comments(text: str) -> str:
    return "\n".join(line.rstrip() for line in text.splitlines()
                     if line.strip() and not line.lstrip().startswith("#"))


def section(text: str, key: str) -> str:
    clean = without_comments(text)
    anchors = list(re.finditer(rf"(?m)^{re.escape(key)}:\s*$", clean))
    if len(anchors) != 1:
        raise AssertionError(f"expected one canonical {key} mapping")
    tail = clean[anchors[0].end():]
    following = re.search(r"(?m)^[^ \n]", tail)
    end = anchors[0].end() + following.start() if following else len(clean)
    return clean[anchors[0].start():end].rstrip()


def assert_ci_graph(text: str) -> None:
    if section(text, "jobs") != without_comments(EXPECTED_CI_JOBS):
        raise AssertionError("CI must run both immutable workers and their strict native gate")
    if section(text, "on") != "on:\n  push:\n    branches: [main]\n  pull_request:\n  merge_group:":
        raise AssertionError("CI must run the whole graph on PR, merge group and main push")
    if section(text, "permissions") != "permissions:\n  contents: read":
        raise AssertionError("CI must not receive write permissions")


def gate_script(text: str) -> str:
    body = text.split("\n  ci:\n", 1)[1].split("        run: |\n", 1)[1]
    return "\n".join(line[10:] for line in body.splitlines() if line.strip()) + "\n"


def references() -> list[dict]:
    return [{"path": f"{REPOSITORY}/.github/workflows/{name}@{pin}", "sha": pin}
            for name, pin in (("ci-worker.yml", CI_PIN), ("native-conformance-worker.yml", NATIVE_PIN))]


def evidence() -> dict:
    return {
        "runs": [{"id": 101, "run_attempt": 2, "path": ".github/workflows/ci.yml",
                  "head_sha": SHA, "head_branch": "main", "event": "push",
                  "status": "completed", "conclusion": "success",
                  "referenced_workflows": references()}],
        "jobs": [{"name": f"outer / {name}" if name != "CI" else name,
                  "run_id": 101, "status": "completed", "conclusion": "success"}
                 for name in REQUIRED_JOBS],
    }


# Исполняется Node-блок workflow. VM заменяет все IO-порты;
# реальная сеть, секреты и запись файлов недоступны.
NODE_HARNESS = r"""
const fs = require('node:fs');
const vm = require('node:vm');
const {script, fixture} = JSON.parse(fs.readFileSync(0, 'utf8'));
const calls = [], diagnostics = [];
let output = '', listings = 0;
const context = {
  URLSearchParams,
  process: {env: {
    GITHUB_REPOSITORY: 'Labpics-Team/lab-colors', GITHUB_SHA: 'a'.repeat(40),
    GITHUB_API_URL: 'https://api.example.invalid', GITHUB_OUTPUT: '/fixture/output',
    GH_READ_TOKEN: 'fixture-token',
  }, exitCode: 0},
  require(name) {
    if (name !== 'node:fs') throw new Error(`unmocked module: ${name}`);
    return {appendFileSync(path, value) {
      if (path !== '/fixture/output') throw new Error(`unmocked write: ${path}`);
      output += value;
    }};
  },
  console: {log() {}, error(message) {diagnostics.push(String(message));}},
  async fetch(address) {
    const url = new URL(address);
    calls.push(url.pathname + url.search);
    if (fixture.apiFailure) return {ok:false, status:503, async text(){return 'fixture outage';}};
    if (url.origin !== 'https://api.example.invalid') throw new Error('unexpected API origin');
    const base = '/repos/Labpics-Team/lab-colors/actions';
    let payload;
    if (url.pathname === `${base}/workflows/ci.yml/runs`) {
      if (url.searchParams.get('branch') !== 'main' || url.searchParams.get('event') !== 'push' ||
          url.searchParams.get('head_sha') !== 'a'.repeat(40)) throw new Error('unbound run query');
      listings++;
      payload = {workflow_runs: listings > 1 && fixture.rereadRuns ? fixture.rereadRuns : fixture.runs};
    } else if (url.pathname === `${base}/runs/101/attempts/2/jobs`) {
      payload = {jobs: fixture.jobs};
    } else throw new Error(`unexpected API path: ${url.pathname}`);
    if (fixture.malformedResponse) payload = {};
    if (fixture.unboundedPage) payload = Object.fromEntries(Object.keys(payload).map(key=>[key, Array(100).fill(payload[key][0])]));
    return {ok:true, async json(){return payload;}};
  },
};
(async () => {
  try {await new vm.Script(script).runInNewContext(context, {timeout: 1000});}
  catch (error) {diagnostics.push(String(error)); context.process.exitCode = 1;}
  process.stdout.write(JSON.stringify({code: context.process.exitCode, output, calls, diagnostics}));
})().catch(error=>{console.error(error); process.exitCode=1;});
"""


def publish_script() -> str:
    step = source("publish-worker.yml").split(
        "      - name: guard — canonical exact-SHA workflow runs and their own jobs\n", 1)[1]
    body = step.split("          node <<'NODE'\n", 1)[1].split("          NODE\n", 1)[0]
    return "\n".join(line[10:] for line in body.splitlines()) + "\n"


class NativePeerGateTest(unittest.TestCase):
    def test_complete_native_graph_is_the_only_automatic_admission_path(self) -> None:
        assert_ci_graph(source("ci.yml"))
        self.assertFalse((WORKFLOWS / "ci-gate.yml").exists())
        self.assertFalse((REPO / "scripts/ci_peer_gate.py").exists())
        self.assertEqual(section(source("native-conformance.yml"), "on"), "on:\n  workflow_dispatch:")
        for workflow in (*WORKFLOWS.glob("*.yml"), *WORKFLOWS.glob("*.yaml")):
            if workflow.name != "ci.yml":
                self.assertNotRegex(section(workflow.read_text(encoding="utf-8"), "on"),
                                    r"(?m)^  (pull_request|pull_request_target|merge_group):", workflow.name)

    def test_removed_dependencies_extra_jobs_and_skipping_cannot_weaken_the_graph(self) -> None:
        original = source("ci.yml")
        assert_ci_graph(original)
        mutations = (
            ("needs: [worker, native]", "needs: [worker]"),
            ("${{ always() }}", "${{ success() }}"),
            ("    timeout-minutes: 5", "    continue-on-error: true\n    timeout-minutes: 5"),
            ("    name: Native conformance (Swift)", "    if: false\n    name: Native conformance (Swift)"),
            ("    runs-on: ubuntu-latest", "    runs-on: self-hosted"),
            ('[[ "$RESULTS" == "success,success" ]]', "true"),
            ("  merge_group:\n", ""),
        )
        changed_graphs = [original.replace(old, new) for old, new in mutations]
        changed_graphs.append(original + "\n  unbound:\n    runs-on: ubuntu-latest\n    steps:\n      - run: true\n")
        for changed in changed_graphs:
            with self.subTest(changed=changed[-120:]):
                self.assertNotEqual(changed, original)
                with self.assertRaises(AssertionError):
                    assert_ci_graph(changed)

    def test_actual_shell_refuses_every_non_success_branch(self) -> None:
        script = gate_script(source("ci.yml"))
        self.assertEqual(script, GATE_SCRIPT)
        bash = shutil.which("bash")
        if os.name == "nt":
            bash = str(Path(os.environ.get("ProgramFiles", "C:/Program Files")) / "Git/bin/bash.exe")
        self.assertTrue(bash, "Bash is required to execute the admission contract")
        values = ("success", "failure", "cancelled", "skipped", "neutral", "", "unknown")
        cases = [(f"{left},{right}", left == right == "success") for left in values for right in values]
        cases += [(value, False) for value in ("", "success", "success,success,success", "success,success\n")]
        for value, expected in cases:
            result = subprocess.run([bash, "--noprofile", "--norc", "-c", script],
                                    env={**os.environ, "RESULTS": value}, capture_output=True, timeout=10)
            self.assertEqual(result.returncode == 0, expected, value)

    def test_both_workers_keep_their_reviewed_bytes_and_ancestor_pins(self) -> None:
        for name, pin in (("ci-worker.yml", CI_PIN), ("native-conformance-worker.yml", NATIVE_PIN)):
            self.assertIn(f"uses: {REPOSITORY}/.github/workflows/{name}@{pin}", source("ci.yml"))
            committed = subprocess.check_output(["git", "show", f"{pin}:.github/workflows/{name}"], cwd=REPO)
            self.assertEqual(committed, (WORKFLOWS / name).read_bytes())
            subprocess.run(["git", "merge-base", "--is-ancestor", pin, "HEAD"], cwd=REPO, check=True)
        self.assertIn(f"native-conformance-worker.yml@{NATIVE_PIN}", source("native-conformance.yml"))
        for name in ("ci-worker.yml", "native-conformance-worker.yml"):
            self.assertNotIn("continue-on-error:", source(name))
            self.assertNotRegex(source(name), r"(?m)^        if:")
        self.assertIn("    if: github.event_name == 'workflow_dispatch'\n    runs-on: macos-15",
                      source("native-conformance-worker.yml"))


class PublishAdmissionTest(unittest.TestCase):
    def run_guard(self, fixture: dict, script: str | None = None) -> dict:
        node = shutil.which("node")
        self.assertTrue(node, "Node is required to execute the publisher admission contract")
        result = subprocess.run([node, "-e", NODE_HARNESS],
                                input=json.dumps({"script": script or publish_script(), "fixture": fixture}),
                                text=True, capture_output=True, timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def reject(self, fixture: dict) -> None:
        result = self.run_guard(fixture)
        self.assertEqual(result["code"], 1, result)
        self.assertEqual(result["output"], "", result)

    def test_one_exact_run_admits_both_workers_and_all_nine_jobs(self) -> None:
        result = self.run_guard(evidence())
        self.assertEqual(result["code"], 0, result)
        self.assertEqual(result["output"], "ci_run_id=101\nci_run_attempt=2\n")
        self.assertEqual(len(result["calls"]), 3)
        self.assertIn("/runs/101/attempts/2/jobs?", result["calls"][1])
        self.assertFalse(any("native-conformance.yml" in call for call in result["calls"]))
        reversed_refs = evidence()
        reversed_refs["runs"][0]["referenced_workflows"].reverse()
        self.assertEqual(self.run_guard(reversed_refs)["code"], 0)

    def test_missing_or_foreign_worker_reference_cannot_pass(self) -> None:
        for wrong in (None, [], references()[:1], references() * 2,
                      [references()[0], references()[0]], [references()[1], references()[1]],
                      references() + [{"path": "foreign", "sha": "b" * 40}],
                      [references()[0], {"path": references()[1]["path"], "sha": "b" * 40}],
                      [references()[0], {"path": "foreign", "sha": NATIVE_PIN}], [references()[0], {}]):
            with self.subTest(references=wrong):
                fixture = evidence()
                fixture["runs"][0]["referenced_workflows"] = wrong
                self.reject(fixture)

    def test_every_original_check_and_native_gate_is_required_exactly_once(self) -> None:
        for index, name in enumerate(REQUIRED_JOBS):
            with self.subTest(job=name):
                missing = evidence()
                missing["jobs"].pop(index)
                self.reject(missing)
                duplicate = evidence()
                duplicate["jobs"].append(copy.deepcopy(duplicate["jobs"][index]))
                self.reject(duplicate)
                for field, value in (("conclusion", "failure"), ("conclusion", "skipped"),
                                     ("conclusion", "cancelled"), ("conclusion", "neutral"),
                                     ("conclusion", None), ("run_id", 100), ("status", "in_progress")):
                    failed = evidence()
                    failed["jobs"][index][field] = value
                    self.reject(failed)

    def test_wrong_run_identity_and_incomplete_run_fail(self) -> None:
        for field, value in (("head_sha", "b" * 40), ("head_branch", "feature"), ("event", "pull_request"),
                             ("event", "workflow_dispatch"), ("path", ".github/workflows/native-conformance.yml"),
                             ("status", "in_progress"), ("conclusion", "failure"), ("run_attempt", 0),
                             ("run_attempt", "2"), ("run_attempt", None), ("id", "101"), ("id", 0)):
            with self.subTest(field=field, value=value):
                fixture = evidence()
                fixture["runs"][0][field] = value
                self.reject(fixture)
        fixture = evidence()
        fixture["runs"] = []
        self.reject(fixture)

    def test_newer_failed_run_cannot_be_replaced_by_an_older_green_run(self) -> None:
        fixture = evidence()
        newer = copy.deepcopy(fixture["runs"][0])
        newer.update(id=102, conclusion="failure")
        fixture["runs"].append(newer)
        self.reject(fixture)

    def test_run_or_attempt_changing_during_admission_is_rejected(self) -> None:
        for field, value in (("id", 102), ("run_attempt", 3), ("conclusion", "failure")):
            fixture = evidence()
            fixture["rereadRuns"] = copy.deepcopy(fixture["runs"])
            fixture["rereadRuns"][0][field] = value
            self.reject(fixture)

    def test_api_failure_malformed_payload_and_pagination_overflow_fail(self) -> None:
        for flag in ("apiFailure", "malformedResponse", "unboundedPage"):
            fixture = evidence()
            fixture[flag] = True
            self.reject(fixture)

    def test_negative_fixture_detects_removal_of_the_swift_requirement(self) -> None:
        script = publish_script()
        line = '"swift conformance (self-hosted Linux, pinned toolchain)",'
        self.assertEqual(script.count(line), 1)
        fixture = evidence()
        fixture["jobs"].pop(7)
        self.assertEqual(self.run_guard(fixture)["code"], 1)
        self.assertEqual(self.run_guard(fixture, script.replace(line, ""))["code"], 0)


def browser_script() -> str:
    step = source("ci-worker.yml").split("      - name: terminal Program in real browser\n", 1)[1]
    body = step.split("        run: |\n", 1)[1].split("\n#", 1)[0]
    return "\n".join(line[10:] for line in body.splitlines() if line.strip()) + "\n"


class BrowserBinaryAdmissionTest(unittest.TestCase):
    def exercise(self, *, script: str | None = None, wasm_exit: int = 0,
                 missing_browser: bool = False, capabilities: dict | None = None) -> dict:
        node = shutil.which("node")
        bash = shutil.which("bash")
        self.assertTrue(node and bash, "real Node and Bash are required")
        original = capabilities or json.loads((REPO / "crates/labcolors-wasm/webdriver.json").read_text())
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
        workflow = source("ci-worker.yml")
        self.assertNotIn("CHROME_BIN_DIR", workflow)
        self.assertNotIn('ln -sf "$CHROME_BIN"', workflow)
        self.assertIn('wasm-pack test --headless --chrome --chromedriver "$CHROMEDRIVER_PATH"', browser_script())


if __name__ == "__main__":
    unittest.main()
