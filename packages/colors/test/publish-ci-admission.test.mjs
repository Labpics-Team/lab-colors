import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { runInNewContext } from "node:vm";

const workflow = (name) => readFileSync(new URL("../../../.github/workflows/" + name, import.meta.url), "utf8");
const publisher = workflow("publish-worker.yml");
const sha = "1234567890abcdef1234567890abcdef12345678";
const repository = "Labpics-Team/lab-colors";

function stepScript(source, id) {
  const lines = source.split("\n");
  const anchors = lines.flatMap((line, index) => line.trim() === "id: " + id ? [index] : []);
  assert.equal(anchors.length, 1, "publication must execute one " + id + " admission");
  const start = lines.findIndex((line, index) => index > anchors[0] && /^\s+node <<'NODE'\s*$/u.test(line));
  assert.ok(start > anchors[0], id + " admission script is absent");
  const indent = lines[start].match(/^\s*/u)[0];
  const end = lines.findIndex((line, index) => index > start && line === indent + "NODE");
  assert.ok(end > start, id + " admission script is not terminated");
  return lines.slice(start + 1, end).map((line) => {
    assert.ok(line === "" || line.startsWith(indent), "malformed workflow script indentation");
    return line.slice(indent.length);
  }).join("\n");
}

// Fixture job names come from the producer, never from the publisher's allowlist.
// Only literal job-level names participate; matrix leaves are covered by their
// existing aggregate jobs. Unexpected naming syntax cannot silently shrink it.
function producerNames(files) {
  const names = files.flatMap((file) => [...workflow(file).matchAll(/^    name: (.+)$/gmu)]
    .map((match) => match[1].trim())
    .filter((name) => !name.includes("${{"))
    .map((name) => name.startsWith('"') ? JSON.parse(name) : name.startsWith("'") ? name.slice(1, -1) : name));
  assert.ok(names.length > 0, "producer fixture must contain actual jobs");
  return [...new Set(names)];
}

function evidence() {
  const nativeReferences = [...workflow("native-conformance.yml").matchAll(/^    uses: ([^\s]+)@([0-9a-f]{40})$/gmu)];
  assert.equal(nativeReferences.length, 1, "native caller must bind one immutable worker");
  const ciNames = producerNames(["ci-worker.yml", "package-worker.yml"]);
  assert.ok(ciNames.includes("core tests and artifact contracts"), "healthy control lacks the Core aggregate");
  assert.ok(ciNames.includes("test"), "healthy control lacks the full verification aggregate");
  const nativeNames = producerNames(["native-conformance-worker.yml"]);
  const run = (id, file, references) => ({
    id, run_attempt: 1, path: ".github/workflows/" + file,
    head_sha: sha, head_branch: "main", event: "push",
    status: "completed", conclusion: "success", referenced_workflows: references,
  });
  const ci = run(101, "ci.yml", ["ci-worker.yml", "package-worker.yml"].map((file) => ({
    path: repository + "/.github/workflows/" + file + "@" + sha, sha,
  })));
  const native = run(202, "native-conformance.yml", [{
    path: nativeReferences[0][1] + "@" + nativeReferences[0][2], sha: nativeReferences[0][2],
  }]);
  const jobs = (id, names) => names.map((name, index) => ({
    id: id * 100 + index, run_id: id, name: "caller / nested caller / " + name,
    status: "completed", conclusion: "success",
  }));
  return {
    runs: { "ci.yml": [ci], "native-conformance.yml": [native] },
    jobs: { 101: jobs(101, ciNames), 202: jobs(202, nativeNames) },
  };
}

async function admit(record, script = stepScript(publisher, "canonical-runs")) {
  const writes = [], errors = [], requests = [];
  const process = { env: {
    GITHUB_REPOSITORY: repository, GITHUB_SHA: sha,
    GITHUB_API_URL: "https://github-api.example.invalid", GITHUB_OUTPUT: "/fixture/output",
    GH_READ_TOKEN: "fixture-read-token",
  } };
  const fetch = async (input) => {
    const url = new URL(input);
    requests.push(url.href);
    assert.ok(requests.length <= 30, "admission must have bounded API reads");
    assert.equal(url.origin, process.env.GITHUB_API_URL);
    let payload;
    const runMatch = url.pathname.match(/\/actions\/workflows\/([^/]+)\/runs$/u);
    const jobMatch = url.pathname.match(/\/actions\/runs\/(\d+)\/jobs$/u);
    if (runMatch) {
      assert.equal(url.searchParams.get("head_sha"), sha);
      assert.equal(url.searchParams.get("branch"), "main");
      assert.equal(url.searchParams.get("event"), "push");
      payload = { workflow_runs: record.runs[decodeURIComponent(runMatch[1])] ?? [] };
    } else if (jobMatch) {
      payload = { jobs: record.jobs[jobMatch[1]] ?? [] };
    } else {
      throw new Error("unexpected API route: " + url.pathname);
    }
    return { ok: true, json: async () => structuredClone(payload) };
  };
  const require = (name) => {
    assert.equal(name, "node:fs");
    return { appendFileSync(path, value, encoding) {
      assert.equal(path, "/fixture/output");
      assert.equal(encoding, "utf8");
      writes.push(value);
    } };
  };
  await runInNewContext(script, {
    process, fetch, require, URLSearchParams,
    console: { log() {}, error(error) { errors.push(String(error)); } },
  }, { timeout: 1000, filename: "publish-canonical-runs.cjs" });
  return { writes, errors, exitCode: process.exitCode ?? 0, requests };
}

function assertRefused(result, reason) {
  assert.equal(result.exitCode, 1, result.errors.join("\n"));
  assert.deepEqual(result.writes, [], "a refused run must not export partial release authority");
  assert.match(result.errors.join("\n"), reason);
}

test("publication admits successful current producer jobs at arbitrary caller depth", async () => {
  const result = await admit(evidence());
  assert.equal(result.exitCode, 0, result.errors.join("\n"));
  assert.deepEqual(result.errors, []);
  assert.deepEqual(result.writes, ["ci_run_id=101\nci_run_attempt=1\nnative_run_id=202\n"]);
});

for (const condition of ["missing", "duplicate", "foreign-run", "failure", "skipped", "in-progress"]) {
  test("publication refuses " + condition + " verification aggregate without partial outputs", async () => {
    const record = evidence();
    const jobs = record.jobs[101];
    const index = jobs.findIndex((job) => job.name.endsWith(" / test"));
    assert.ok(index >= 0);
    if (condition === "missing") jobs.splice(index, 1);
    else if (condition === "duplicate") jobs.push({ ...jobs[index], id: 999 });
    else if (condition === "foreign-run") jobs[index].run_id = 999;
    else if (condition === "in-progress") Object.assign(jobs[index], { status: "in_progress", conclusion: null });
    else jobs[index].conclusion = condition;
    assertRefused(await admit(record), /test:/u);
  });
}

test("publication does not fall back to an old successful run after a failed rerun", async () => {
  const record = evidence();
  record.runs["ci.yml"].push({ ...record.runs["ci.yml"][0], id: 303, conclusion: "failure" });
  assertRefused(await admit(record), /303.*failure/u);
});

test("publication refuses workflow references from another source revision", async () => {
  const record = evidence();
  record.runs["ci.yml"][0].referenced_workflows[1].sha = "0".repeat(40);
  assertRefused(await admit(record), /escaped the admitted source revision/u);
});

test("publication rejects a missing native result after a successful CI run", async () => {
  const record = evidence();
  record.jobs[202] = [];
  assertRefused(await admit(record), /swift conformance/u);
});

test("publication inventory and exports match the source package contract", () => {
  const script = stepScript(publisher, "verified-artifact");
  const boundary = script.indexOf("(async () => {");
  assert.ok(boundary > 0, "artifact validator must expose its declaration before IO");
  const declared = runInNewContext(script.slice(0, boundary) +
    "\n({ files: EXPECTED_PACKAGE_FILES, exports: EXPECTED_PACKAGE_EXPORTS });", {
    require: createRequire(import.meta.url), Buffer,
    process: { env: { GITHUB_WORKSPACE: fileURLToPath(new URL("../../../", import.meta.url)) } },
  }, { timeout: 1000, filename: "publish-package-contract.cjs" });
  const actual = JSON.parse(JSON.stringify(declared));
  const source = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));
  assert.deepEqual(actual.files, source.files, "publication must retain the source package inventory");
  assert.deepEqual(actual.exports, source.exports, "publication must retain the source public API");
});
